use std::env;
use std::process;

use ext4_blockdev::FileDisk;
use rsext4::{mkfs, Jbd2Dev};

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: ext4-mkfs <image> [size_in_mb]");
        process::exit(1);
    }

    let path = &args[1];
    let size_mb: u64 = if args.len() > 2 {
        args[2].parse().unwrap_or(100)
    } else {
        100
    };
    let block_size = 4096u32;
    let size = size_mb * 1024 * 1024;

    let disk = FileDisk::create(path, size, block_size).unwrap_or_else(|e| {
        eprintln!("ext4-mkfs: failed to create {}: {}", path, e);
        process::exit(1);
    });

    let mut jbd = Jbd2Dev::initial_jbd2dev(0, disk, false);

    mkfs(&mut jbd).unwrap_or_else(|e| {
        eprintln!("ext4-mkfs: failed to format: {}", e);
        process::exit(1);
    });

    eprintln!(
        "ext4-mkfs: created ext4 filesystem on {} ({}MB)",
        path, size_mb
    );
}
