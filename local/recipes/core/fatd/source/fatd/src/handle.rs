use syscall::flag::{O_ACCMODE, O_RDONLY, O_RDWR, O_WRONLY};

pub enum Handle {
    File(FileHandle),
    Directory(DirectoryHandle),
    SchemeRoot,
}

pub struct FileHandle {
    path: String,
    offset: u64,
    flags: usize,
}

pub struct DirectoryHandle {
    path: String,
    entries: Vec<(u64, String, u8)>,
    cursor: usize,
    flags: usize,
}

impl FileHandle {
    pub fn new(path: String, flags: usize) -> Self {
        Self {
            path,
            offset: 0,
            flags,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn set_offset(&mut self, offset: u64) {
        self.offset = offset;
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

    pub fn update_path(&mut self, new_path: String) {
        self.path = new_path;
    }
}

impl DirectoryHandle {
    pub fn new(path: String, entries: Vec<(u64, String, u8)>, flags: usize) -> Self {
        Self {
            path,
            entries,
            cursor: 0,
            flags,
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn entries(&self) -> &[(u64, String, u8)] {
        &self.entries
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = cursor;
    }

    pub fn flags(&self) -> usize {
        self.flags
    }

    pub fn update_path(&mut self, new_path: String) {
        self.path = new_path;
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
