use ext4_blockdev::FileDisk;
use rsext4::{
    api, dir, entries::DirEntryIterator, loopfile, mkdir, mkfile, mkfs, mount as ext4_mount,
    umount, Jbd2Dev,
};

#[test]
fn roundtrip_mkfs_mount_read_write_remount() {
    let _ = env_logger::builder().is_test(true).try_init();

    let path = "/tmp/test-ext4-roundtrip.img";
    let size: u64 = 100 * 1024 * 1024; // 100MB
    let block_size = 4096u32;

    // Step 1: Create and format
    println!("=== Step 1: Create ext4 image ===");
    let disk = FileDisk::create(path, size, block_size).expect("create disk");
    let mut jbd = Jbd2Dev::initial_jbd2dev(0, disk, false);
    mkfs(&mut jbd).expect("mkfs");
    println!("Formatted {} ({}MB)", path, size / (1024 * 1024));

    // Step 2: Mount
    println!("\n=== Step 2: Mount ===");
    let disk = FileDisk::open(path, block_size).expect("open for mount");
    let mut jbd = Jbd2Dev::initial_jbd2dev(0, disk, true);
    let mut fs = ext4_mount(&mut jbd).expect("mount");
    println!(
        "Mounted: {} blocks, {} free",
        fs.superblock.blocks_count(),
        fs.statfs().free_blocks
    );

    // Step 3: Create directory
    println!("\n=== Step 3: Create directory /testdir ===");
    mkdir(&mut jbd, &mut fs, "/testdir").expect("mkdir");
    println!("Created /testdir");

    // Step 4: Create file
    println!("\n=== Step 4: Create file /testdir/hello.txt ===");
    mkfile(&mut jbd, &mut fs, "/testdir/hello.txt", None, None).expect("mkfile");
    println!("Created /testdir/hello.txt");

    // Step 5: Open and write
    println!("\n=== Step 5: Write data ===");
    let mut file = api::open(&mut jbd, &mut fs, "/testdir/hello.txt", false).expect("open file");
    let data = b"Hello from Red Bear OS ext4!\n";
    api::write_at(&mut jbd, &mut fs, &mut file, data).expect("write");
    println!("Wrote {} bytes to /testdir/hello.txt", data.len());

    // Step 6: Read back
    println!("\n=== Step 6: Read back ===");
    api::lseek(&mut file, 0).expect("seek to 0");
    let read_data = api::read_at(&mut jbd, &mut fs, &mut file, data.len()).expect("read");
    let read_str = std::str::from_utf8(&read_data).expect("utf8");
    println!("Read back: {:?}", read_str.trim());
    assert_eq!(
        data,
        &read_data[..data.len()],
        "read data matches written data"
    );

    // Step 7: List root directory
    println!("\n=== Step 7: List root directory ===");
    let (_, root_inode) = dir::get_inode_with_num(&mut fs, &mut jbd, "/")
        .expect("get root inode")
        .expect("root inode found");

    let mut root_copy = root_inode;
    let blocks = loopfile::resolve_inode_block_allextend(&mut fs, &mut jbd, &mut root_copy)
        .expect("resolve root blocks");
    let block_size_usize = fs.superblock.block_size() as usize;
    for (&_logical, &phys) in blocks.iter() {
        let cached = fs
            .datablock_cache
            .get_or_load(&mut jbd, phys)
            .expect("cache load");
        for (entry, _) in DirEntryIterator::new(&cached.data[..block_size_usize]) {
            if let Some(name) = entry.name_str() {
                if !name.is_empty() && name != "." && name != ".." {
                    println!("  /{} (inode={})", name, entry.inode);
                }
            }
        }
    }

    // Step 8: List /testdir
    println!("\n=== Step 8: List /testdir ===");
    let (_, dir_inode) = dir::get_inode_with_num(&mut fs, &mut jbd, "/testdir")
        .expect("get testdir inode")
        .expect("testdir found");

    let mut dir_copy = dir_inode;
    let dir_blocks = loopfile::resolve_inode_block_allextend(&mut fs, &mut jbd, &mut dir_copy)
        .expect("resolve testdir blocks");
    for (&_logical, &phys) in dir_blocks.iter() {
        let cached = fs
            .datablock_cache
            .get_or_load(&mut jbd, phys)
            .expect("cache load dir");
        for (entry, _) in DirEntryIterator::new(&cached.data[..block_size_usize]) {
            if let Some(name) = entry.name_str() {
                if !name.is_empty() && name != "." && name != ".." {
                    println!("  /testdir/{} (inode={})", name, entry.inode);
                }
            }
        }
    }

    // Step 9: Stat filesystem
    println!("\n=== Step 9: Filesystem stats ===");
    let stats = fs.statfs();
    println!("  block_size: {}", stats.block_size);
    println!("  total_blocks: {}", stats.total_blocks);
    println!("  free_blocks: {}", stats.free_blocks);
    println!("  total_inodes: {}", stats.total_inodes);
    println!("  free_inodes: {}", stats.free_inodes);

    // Step 10: Sync and unmount
    println!("\n=== Step 10: Sync + Unmount ===");
    fs.sync_filesystem(&mut jbd).expect("sync");
    umount(fs, &mut jbd).expect("umount");
    println!("Synced and unmounted cleanly");

    // Step 11: Re-mount and verify data persists
    println!("\n=== Step 11: Re-mount and verify persistence ===");
    let disk2 = FileDisk::open(path, block_size).expect("reopen");
    let mut jbd2 = Jbd2Dev::initial_jbd2dev(0, disk2, true);
    let mut fs2 = ext4_mount(&mut jbd2).expect("remount");

    let mut file2 =
        api::open(&mut jbd2, &mut fs2, "/testdir/hello.txt", false).expect("reopen file");
    let read_data2 = api::read_at(&mut jbd2, &mut fs2, &mut file2, data.len()).expect("reread");
    assert_eq!(
        data,
        &read_data2[..data.len()],
        "data persists after remount"
    );
    let read_str2 = std::str::from_utf8(&read_data2).expect("utf8");
    println!("After remount, read: {:?}", read_str2.trim());

    fs2.sync_filesystem(&mut jbd2).expect("sync2");
    umount(fs2, &mut jbd2).expect("umount2");
}
