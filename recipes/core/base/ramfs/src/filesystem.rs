use std::collections::BTreeMap;
use std::convert::TryInto;
use std::{fs, iter, time};

use std::os::unix::io::AsRawFd;

use indexmap::IndexMap;
use syscall::error::{EACCES, EIO, ENFILE, ENOENT};
use syscall::{Error, Result, TimeSpec, MODE_DIR};

use super::scheme::current_perm;

#[derive(Debug)]
pub struct File {
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub nlink: usize,
    pub parent: Inode,

    pub open_handles: usize,

    pub atime: TimeSpec,
    pub ctime: TimeSpec,
    pub mtime: TimeSpec,

    pub data: FileData,
}

#[derive(Clone, Debug)]
pub struct Inode(pub usize);

#[derive(Debug)]
pub enum FileData {
    File(Vec<u8>),
    Directory(IndexMap<String, Inode>),
}
impl FileData {
    pub fn size(&self) -> usize {
        match self {
            &Self::File(ref data) => data.len(),
            &Self::Directory(_) => 0,
        }
    }
}

pub struct Filesystem {
    pub files: BTreeMap<usize, File>,
    pub memory_file: fs::File,
    pub last_inode_number: usize,
}
impl Filesystem {
    pub const DEFAULT_BLOCK_SIZE: u32 = 4096;
    pub const ROOT_INODE: usize = 1;

    pub fn new() -> Result<Self> {
        Ok(Self {
            files: iter::once((Self::ROOT_INODE, Self::create_root_inode())).collect(),
            memory_file: fs::File::open("/scheme/memory").or(Err(Error::new(EIO)))?,
            last_inode_number: Self::ROOT_INODE,
        })
    }
    fn create_root_inode() -> File {
        let cur_time = current_time();
        File {
            atime: cur_time,
            ctime: cur_time,
            mtime: cur_time,

            mode: MODE_DIR | 0o755,
            nlink: 1,
            open_handles: 0,

            uid: 0,
            gid: 0,

            data: FileData::Directory(IndexMap::new()),
            parent: Inode(Self::ROOT_INODE),
        }
    }
    pub fn get_block_size(&self) -> Result<u32> {
        Ok(libredox::call::fstatvfs(self.memory_file.as_raw_fd() as usize)?.f_bsize as u32)
    }
    pub fn block_size(&self) -> u32 {
        self.get_block_size().unwrap_or(Self::DEFAULT_BLOCK_SIZE)
    }
    pub fn next_inode_number(&mut self) -> Result<usize> {
        let next = self
            .last_inode_number
            .checked_add(1)
            .ok_or(Error::new(ENFILE))?;
        self.last_inode_number = next;
        Ok(next)
    }
    fn resolve_generic(&self, mut parts: Vec<&str>, uid: u32, gid: u32) -> Result<usize> {
        let mut current_file = self
            .files
            .get(&Self::ROOT_INODE)
            .ok_or(Error::new(ENOENT))?;
        let mut current_inode = Self::ROOT_INODE;

        let mut i = 0;

        loop {
            let Some(&part) = parts.get(i) else {
                break;
            };
            let dentries = match current_file.data {
                FileData::Directory(ref dentries) => dentries,
                FileData::File(_) => return Err(Error::new(ENOENT)),
            };
            let perm = current_perm(&current_file, uid, gid);
            if perm & 0o1 == 0 {
                return Err(Error::new(EACCES));
            }

            if part == "." || part == ".." {
                parts.remove(i);
            }

            let part = *parts.get(i).unwrap();
            if part == ".." && i > 0 {
                i -= 1;
                parts.remove(i);
            }
            let part = *parts.get(i).unwrap();

            current_inode = dentries.get(part).ok_or(Error::new(ENOENT))?.0;
            current_file = self.files.get(&current_inode).ok_or(Error::new(EIO))?;

            i += 1;
        }
        Ok(current_inode)
    }
    pub fn resolve_except_last<'a>(
        &self,
        path_bytes: &'a str,
        uid: u32,
        gid: u32,
    ) -> Result<(usize, Option<&'a str>)> {
        let mut parts =
            path_components_iter(path_bytes.trim_start_matches('/')).collect::<Vec<_>>();

        let last = if parts.len() >= 1 {
            Some(parts.pop().unwrap())
        } else {
            None
        };

        Ok((self.resolve_generic(parts, uid, gid)?, last))
    }
    pub fn resolve(&self, path: &str, uid: u32, gid: u32) -> Result<usize> {
        let parts = path_components_iter(path.trim_start_matches('/')).collect::<Vec<_>>();

        self.resolve_generic(parts, uid, gid)
    }
}
pub fn path_components_iter(bytes: &str) -> impl Iterator<Item = &str> + '_ {
    let components_iter = bytes.split(|c| c == '/');
    components_iter.filter(|item| !item.is_empty())
}
pub fn current_time() -> TimeSpec {
    let sys_time = time::SystemTime::now();

    let duration = match sys_time.duration_since(time::SystemTime::UNIX_EPOCH) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("The time is apparently now before the Unix epoch...");

            let negative_duration = e.duration();

            return TimeSpec {
                tv_sec: negative_duration
                    .as_secs()
                    .try_into()
                    .unwrap_or(i64::min_value()),
                tv_nsec: negative_duration
                    .subsec_nanos()
                    .try_into()
                    .unwrap_or(i32::min_value()),
            };
        }
    };

    TimeSpec {
        tv_sec: duration.as_secs().try_into().unwrap_or(i64::max_value()),
        tv_nsec: duration
            .subsec_nanos()
            .try_into()
            .unwrap_or(i32::max_value()),
    }
}
