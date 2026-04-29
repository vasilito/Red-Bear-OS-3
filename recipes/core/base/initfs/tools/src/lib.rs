use std::convert::{TryFrom, TryInto};
use std::fs::{DirEntry, File, OpenOptions};
use std::io::{prelude::*, SeekFrom};
use std::path::{Path, PathBuf};

use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, FileTypeExt, PermissionsExt};

use anyhow::{anyhow, bail, Context, Result};

use redox_initfs::types as initfs;

const KIBIBYTE: u64 = 1024;
const MEBIBYTE: u64 = KIBIBYTE * 1024;

#[cfg(debug_assertions)]
pub const DEFAULT_MAX_SIZE: u64 = 256 * MEBIBYTE;

#[cfg(not(debug_assertions))]
pub const DEFAULT_MAX_SIZE: u64 = 64 * MEBIBYTE;

// FIXME make this configurable to handle systems with 16k and 64k pages.
const PAGE_SIZE: u16 = 4096;

pub enum EntryKind {
    File { file: File, executable: bool },
    Dir(Vec<Entry>),
    Link(PathBuf),
}

pub struct Entry {
    pub name: Vec<u8>,
    pub kind: EntryKind,
}

struct State<'path> {
    file: OutputImageGuard<'path>,
    offset: u64,
    max_size: u64,
    buffer: Box<[u8]>,
    inode_table: InodeTable,
}

fn write_all_at(file: &File, buf: &[u8], offset: u64, r#where: &str) -> Result<()> {
    file.write_all_at(buf, offset)?;
    log::trace!(
        "Wrote {}..{} within {}",
        offset,
        offset + buf.len() as u64,
        r#where
    );
    Ok(())
}

fn read_directory(path: &Path, root_path: &Path) -> Result<Vec<Entry>> {
    let read_dir = path
        .read_dir()
        .with_context(|| anyhow!("failed to read directory `{}`", path.to_string_lossy(),))?;

    let entries = read_dir
        .map(|result| {
            let entry = result.with_context(|| {
                anyhow!(
                    "failed to get a directory entry from `{}`",
                    path.to_string_lossy(),
                )
            })?;

            let metadata = entry.metadata().with_context(|| {
                anyhow!(
                    "failed to get metadata for `{}`",
                    entry.path().to_string_lossy(),
                )
            })?;
            let file_type = metadata.file_type();

            let unsupported_type = |ty: &str, entry: &DirEntry| {
                Err(anyhow!(
                    "failed to include {} at `{}`: not supported by redox-initfs",
                    ty,
                    entry.path().to_string_lossy()
                ))
            };
            let name = entry
                .path()
                .file_name()
                .context("expected path to have a valid filename")?
                .as_bytes()
                .to_owned();

            let entry_kind = if file_type.is_socket() {
                return unsupported_type("socket", &entry);
            } else if file_type.is_fifo() {
                return unsupported_type("FIFO", &entry);
            } else if file_type.is_block_device() {
                return unsupported_type("block device", &entry);
            } else if file_type.is_char_device() {
                return unsupported_type("character device", &entry);
            } else if file_type.is_file() {
                let executable = metadata.permissions().mode() & 0o100 != 0;
                EntryKind::File {
                    file: File::open(entry.path()).with_context(|| {
                        anyhow!("failed to open file `{}`", entry.path().to_string_lossy(),)
                    })?,
                    executable,
                }
            } else if file_type.is_dir() {
                EntryKind::Dir(read_directory(&entry.path(), root_path)?)
            } else if file_type.is_symlink() {
                let link_file_path = entry.path();

                let link_path = std::fs::read_link(&link_file_path)?;
                let cannonical = if link_path.is_absolute() {
                    link_path.clone()
                } else {
                    let Some(link_parent) = link_file_path.parent() else {
                        bail!("Link at `{}` has no parent", link_file_path.display())
                    };
                    link_parent.canonicalize()?.join(link_path.clone())
                };

                let dir_path = path
                    .canonicalize()
                    .context("Failed to cannonicalize path")?;
                let path = pathdiff::diff_paths(cannonical, &dir_path).ok_or_else(|| {
                    anyhow!(
                        "Failed to diff symlink path [{}] to path [{}]",
                        link_path.display(),
                        dir_path.display()
                    )
                })?;
                EntryKind::Link(path)
            } else {
                return Err(anyhow!(
                    "unknown file type at `{}`",
                    entry.path().to_string_lossy()
                ));
            };

            Ok(Entry {
                kind: entry_kind,
                name,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(entries)
}

fn bump_alloc(state: &mut State, size: u64, why: &str) -> Result<u64> {
    let end = (state.offset + size).next_multiple_of(PAGE_SIZE.into());
    if end <= state.max_size {
        let offset = state.offset;
        state.offset = end;
        log::debug!("Allocating range {}..{} in {}", offset, state.offset, why);
        Ok(offset)
    } else {
        Err(anyhow!("bump allocation failed: max limit reached"))
    }
}
struct WriteResult {
    size: u32,
    offset: u32,
}

fn allocate_and_write_file(state: &mut State, mut file: &File) -> Result<WriteResult> {
    let size = file
        .seek(SeekFrom::End(0))
        .context("failed to seek to end")?;

    let size: u32 = size.try_into().context("file too large")?;

    let offset: u32 = bump_alloc(state, size.into(), "allocate space for file")
        .context("failed to allocate space for file")?
        .try_into()
        .context("file offset too high")?;

    let buffer_size: u32 = state.buffer.len().try_into().context("buffer too large")?;

    file.seek(SeekFrom::Start(0))
        .context("failed to seek to start")?;

    let mut relative_offset = 0;

    // TODO: If this would ever turn out to be a bottleneck, then perhaps we could use
    // copy_file_range in `nix`.

    while relative_offset < size {
        let allowed_length = std::cmp::min(buffer_size, size - relative_offset);
        let allowed_length =
            usize::try_from(allowed_length).expect("expected buffer size not to be outside usize");

        file.read(&mut state.buffer[..allowed_length])
            .context("failed to read from source file")?;

        write_all_at(
            &state.file,
            &state.buffer[..allowed_length],
            u64::from(offset + relative_offset),
            "allocate_and_write_file buffer chunk",
        )
        .context("failed to write source file into destination image")?;

        relative_offset += buffer_size;
    }

    Ok(WriteResult { size, offset })
}

fn allocate_and_write_link(state: &mut State, link: &Path) -> Result<WriteResult> {
    let data = link.as_os_str().as_bytes();
    let size: u32 = data.len().try_into().unwrap();

    let offset: u32 = bump_alloc(state, size.into(), "allocate space for file")
        .context("failed to allocate space for file")?
        .try_into()
        .context("file offset too high")?;

    write_all_at(
        &state.file,
        data,
        u64::from(offset),
        "allocate_and_write_link target path",
    )
    .context("failed to write source file into destination image")?;

    Ok(WriteResult { size, offset })
}

fn allocate_and_write_dir(state: &mut State, dir: &[Entry]) -> Result<WriteResult> {
    let entry_size =
        u16::try_from(std::mem::size_of::<initfs::DirEntry>()).context("entry size too large")?;
    let entry_count = u16::try_from(dir.len()).context("too many subdirectories")?;

    let entry_table_length = u32::from(entry_count)
        .checked_mul(u32::from(entry_size))
        .ok_or_else(|| anyhow!("entry table length too large when multiplying by size"))?;

    let entry_table_offset: u32 =
        bump_alloc(state, entry_table_length.into(), "allocate entry table")
            .context("failed to allocate entry table")?
            .try_into()
            .context("directory entries offset too high")?;

    for (index, entry) in dir.iter().enumerate() {
        let (write_result, ty) = match entry.kind {
            EntryKind::Dir(ref subdir) => {
                let write_result = allocate_and_write_dir(state, subdir).with_context(|| {
                    anyhow!(
                        "failed to copy directory entries from `{}` into image",
                        String::from_utf8_lossy(&entry.name)
                    )
                })?;

                (write_result, initfs::InodeType::Dir)
            }

            EntryKind::File {
                ref file,
                executable,
            } => {
                let write_result = allocate_and_write_file(state, file)
                    .context("failed to copy file into image")?;

                let type_ = if executable {
                    initfs::InodeType::ExecutableFile
                } else {
                    initfs::InodeType::RegularFile
                };

                (write_result, type_)
            }

            EntryKind::Link(ref path) => {
                let write_result = allocate_and_write_link(state, path)
                    .context("failed to copy symbolic link into image")?;
                (write_result, initfs::InodeType::Link)
            }
        };

        let index: u16 = index
            .try_into()
            .expect("expected dir entry count not to exceed u32");

        let inode = state.inode_table.allocate(ty, write_result);

        let (name_offset, name_len) = {
            let name_len: u16 = entry.name.len().try_into().context("file name too long")?;

            let offset: u32 = bump_alloc(state, u64::from(name_len), "allocate file name")
                .context("failed to allocate space for file name")?
                .try_into()
                .context("file name offset too high up")?;

            write_all_at(&state.file, &entry.name, offset.into(), "writing file name")
                .context("failed to write file name")?;

            (offset, name_len)
        };
        {
            let mut direntry_buf = [0_u8; std::mem::size_of::<initfs::DirEntry>()];

            let direntry = plain::from_mut_bytes::<initfs::DirEntry>(&mut direntry_buf)
                .expect("expected dir entry struct to have alignment 1, and buffer size to match");

            log::debug!(
                "Linking inode {} into dir entry index {}, file name `{}`",
                inode,
                index,
                String::from_utf8_lossy(&entry.name)
            );

            *direntry = initfs::DirEntry {
                inode: inode.into(),
                name_len: name_len.into(),
                name_offset: initfs::Offset(name_offset.into()),
            };

            write_all_at(
                &state.file,
                &direntry_buf,
                u64::from(entry_table_offset + u32::from(index) * u32::from(entry_size)),
                "allocate_and_write_dir entry",
            )
            .context("failed to write dir entry struct to image")?;
        }
    }

    Ok(WriteResult {
        size: entry_table_length,
        offset: entry_table_offset,
    })
}
fn allocate_contents(state: &mut State, dir: &[Entry]) -> Result<initfs::U16> {
    let write_result = allocate_and_write_dir(state, dir)
        .context("failed to allocate and write all directories and files")?;

    let root_inode = state
        .inode_table
        .allocate(initfs::InodeType::Dir, write_result);

    Ok(root_inode.into())
}

struct InodeTable {
    entries: Vec<initfs::InodeHeader>,
}

impl InodeTable {
    fn new() -> Self {
        Self { entries: vec![] }
    }

    fn count(&self) -> u16 {
        self.entries
            .len()
            .try_into()
            .expect("inode count too large")
    }

    fn allocate(&mut self, ty: initfs::InodeType, write_result: WriteResult) -> u16 {
        let inode = self.entries.len();
        self.entries.push(initfs::InodeHeader {
            type_: (ty as u32).into(),
            length: initfs::Length(write_result.size.into()),
            offset: initfs::Offset(write_result.offset.into()),
        });
        inode.try_into().expect("inode count too large")
    }
}

fn write_inode_table(state: &mut State) -> Result<initfs::Offset> {
    log::debug!("there are {} inodes", state.inode_table.count());

    let inode_size: u32 = std::mem::size_of::<initfs::InodeHeader>()
        .try_into()
        .expect("inode header length cannot fit within u32");

    let inode_table_length = {
        u64::from(inode_size)
            .checked_mul(u64::from(state.inode_table.count()))
            .ok_or_else(|| anyhow!("inode table too large"))?
    };

    let inode_table_offset = bump_alloc(state, inode_table_length, "allocate inode table")?;
    let inode_table_offset =
        u32::try_from(inode_table_offset).with_context(|| "inode table located too far away")?;

    for (i, inode) in state.inode_table.entries.iter().enumerate() {
        // TODO: Use main buffer and write in bulk.
        let mut inode_buf = [0_u8; std::mem::size_of::<initfs::InodeHeader>()];

        let inode_hdr = plain::from_mut_bytes::<initfs::InodeHeader>(&mut inode_buf)
            .expect("expected inode struct to have alignment 1, and buffer size to match");

        *inode_hdr = *inode;

        log::debug!(
            "Writing inode index {} from offset {}",
            i,
            inode_table_offset
        );
        write_all_at(
            &state.file,
            &inode_buf,
            u64::from(inode_table_offset + u32::try_from(i).unwrap() * inode_size),
            "write_inode",
        )
        .context("failed to write inode struct to disk image")?;
    }

    let inode_table_offset = initfs::Offset(inode_table_offset.into());

    Ok(inode_table_offset)
}

struct OutputImageGuard<'a> {
    file: File,
    path: &'a Path,
    ok: bool,
}

impl std::ops::Deref for OutputImageGuard<'_> {
    type Target = File;

    fn deref(&self) -> &Self::Target {
        &self.file
    }
}
impl std::ops::DerefMut for OutputImageGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }
}

impl Drop for OutputImageGuard<'_> {
    fn drop(&mut self) {
        if !self.ok {
            let _ = std::fs::remove_file(self.path);
        }
    }
}

pub struct Args<'a> {
    pub destination_path: &'a Path,
    pub max_size: u64,
    pub source: &'a Path,
    pub bootstrap_code: &'a Path,
}
pub fn archive(
    &Args {
        destination_path,
        max_size,
        source,
        bootstrap_code,
    }: &Args,
) -> Result<()> {
    let root_path = source;
    let root = read_directory(root_path, root_path).context("failed to read root")?;

    build_initfs(destination_path, max_size, bootstrap_code, root)
}

pub fn build_initfs(
    destination_path: &Path,
    max_size: u64,
    bootstrap_code: &Path,
    root: Vec<Entry>,
) -> std::result::Result<(), anyhow::Error> {
    let previous_extension = destination_path.extension().map_or("", |ext| {
        ext.to_str()
            .expect("expected destination path to be valid UTF-8")
    });

    if !destination_path
        .metadata()
        .map_or(true, |metadata| metadata.is_file())
    {
        return Err(anyhow!("Destination file must be a file"));
    }

    let destination_temp_path =
        destination_path.with_extension(format!("{}.partial", previous_extension));

    let destination_temp_file = OpenOptions::new()
        .read(false)
        .write(true)
        .create(true)
        .truncate(true)
        .create_new(false)
        .open(&destination_temp_path)
        .context("failed to open destination file")?;

    let guard = OutputImageGuard {
        file: destination_temp_file,
        path: &destination_temp_path,
        ok: false,
    };

    const BUFFER_SIZE: usize = 8192;

    let mut state = State {
        file: guard,
        offset: 0,
        max_size,
        buffer: vec![0_u8; BUFFER_SIZE].into_boxed_slice(),
        inode_table: InodeTable::new(),
    };

    // NOTE: The header is always stored at offset zero.
    let header_offset = bump_alloc(&mut state, 4096, "allocate header")?;
    assert_eq!(header_offset, 0);

    allocate_and_write_file(
        &mut state,
        &File::open(bootstrap_code).with_context(|| {
            anyhow!(
                "failed to open bootstrap code file `{}`",
                bootstrap_code.to_string_lossy(),
            )
        })?,
    )?;
    let bootstrap_data = std::fs::read(bootstrap_code).with_context(|| {
        anyhow!(
            "failed to read bootstrap code file `{}`",
            bootstrap_code.to_string_lossy(),
        )
    })?;
    let bootstrap_entry = elf_entry(&bootstrap_data);

    let root_inode = allocate_contents(&mut state, &root)?;

    let inode_table_offset = write_inode_table(&mut state)?;

    {
        let mut header_bytes = [0_u8; std::mem::size_of::<initfs::Header>()];
        let header = plain::from_mut_bytes(&mut header_bytes)
            .expect("expected header size to be sufficient and alignment to be 1");

        *header = initfs::Header {
            magic: initfs::Magic(initfs::MAGIC),
            inode_count: state.inode_table.count().into(),
            inode_table_offset,
            bootstrap_entry: bootstrap_entry.into(),
            initfs_size: state
                .file
                .metadata()
                .context("failed to get initfs size")?
                .len()
                .into(),
            page_size: PAGE_SIZE.into(),
            root_inode,
        };
        write_all_at(&state.file, &header_bytes, header_offset, "writing header")
            .context("failed to write header")?;
    }

    std::fs::rename(&destination_temp_path, destination_path)
        .context("failed to rename output image")?;

    state.file.ok = true;

    Ok(())
}

fn elf_entry(data: &[u8]) -> u64 {
    assert!(&data[..4] == b"\x7FELF");
    match (data[4], data[5]) {
        // 32-bit, little endian
        (1, 1) => u32::from_le_bytes(
            <[u8; 4]>::try_from(&data[0x18..0x18 + 4]).expect("conversion cannot fail"),
        ) as u64,
        // 32-bit, big endian
        (1, 2) => u32::from_be_bytes(
            <[u8; 4]>::try_from(&data[0x18..0x18 + 4]).expect("conversion cannot fail"),
        ) as u64,
        // 64-bit, little endian
        (2, 1) => u64::from_le_bytes(
            <[u8; 8]>::try_from(&data[0x18..0x18 + 8]).expect("conversion cannot fail"),
        ),
        // 64-bit, big endian
        (2, 2) => u64::from_be_bytes(
            <[u8; 8]>::try_from(&data[0x18..0x18 + 8]).expect("conversion cannot fail"),
        ),
        (ei_class, ei_data) => {
            panic!("Unsupported ELF EI_CLASS {} EI_DATA {}", ei_class, ei_data);
        }
    }
}
