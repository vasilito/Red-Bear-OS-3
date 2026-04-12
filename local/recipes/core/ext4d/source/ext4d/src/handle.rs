use rsext4::{api::OpenFile, bmalloc::InodeNumber, disknode::Ext4Inode};
use syscall::flag::{O_ACCMODE, O_RDONLY, O_RDWR, O_WRONLY};

pub enum Handle {
    File(FileHandle),
    Directory(DirectoryHandle),
    SchemeRoot,
}

pub struct FileHandle {
    path: String,
    pub file: OpenFile,
    flags: usize,
}

pub struct DirectoryHandle {
    path: String,
    inode_num: InodeNumber,
    inode: Ext4Inode,
    flags: usize,
}

impl FileHandle {
    pub fn new(path: String, file: OpenFile, flags: usize) -> Self {
        Self { path, file, flags }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn inode_num(&self) -> InodeNumber {
        self.file.inode_num
    }

    pub fn flags(&self) -> usize {
        self.flags
    }

    pub fn can_read(&self) -> bool {
        matches!(self.flags & O_ACCMODE, O_RDONLY | O_RDWR)
    }

    pub fn can_write(&self) -> bool {
        matches!(self.flags & O_ACCMODE, O_WRONLY | O_RDWR)
    }

    pub fn set_path(&mut self, path: String) {
        self.path = path;
    }
}

impl DirectoryHandle {
    pub fn new(path: String, inode_num: InodeNumber, inode: Ext4Inode, flags: usize) -> Self {
        Self {
            path,
            inode_num,
            inode,
            flags,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn inode_num(&self) -> InodeNumber {
        self.inode_num
    }

    pub fn inode(&self) -> &Ext4Inode {
        &self.inode
    }

    pub fn flags(&self) -> usize {
        self.flags
    }
}

impl Handle {
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::File(handle) => Some(handle.path()),
            Self::Directory(handle) => Some(handle.path()),
            Self::SchemeRoot => Some(""),
        }
    }

    pub fn flags(&self) -> Option<usize> {
        match self {
            Self::File(handle) => Some(handle.flags()),
            Self::Directory(handle) => Some(handle.flags()),
            Self::SchemeRoot => None,
        }
    }
}
