#![no_std]
//! A super simple initfs, only meant to be loaded into RAM by the bootloader, and then directly be
//! read.

use core::convert::{TryFrom, TryInto};

pub mod types;

use self::types::*;

#[derive(Clone, Copy)]
pub struct InitFs<'initfs> {
    base: &'initfs [u8],
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Inode(u16);

#[derive(Clone, Copy, Debug)]
pub struct Error;

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid or corrupt initfs")
    }
}

impl core::error::Error for Error {}

type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Copy)]
pub struct InodeStruct<'initfs> {
    initfs: InitFs<'initfs>,
    inode_id: Inode,
    inode: &'initfs InodeHeader,
}

#[derive(Clone, Copy)]
pub struct InodeFile<'initfs> {
    inner: InodeStruct<'initfs>,
}
impl<'initfs> InodeFile<'initfs> {
    pub fn inode(self) -> InodeStruct<'initfs> {
        self.inner
    }
    pub fn data(&self) -> Result<&'initfs [u8]> {
        self.inner.data()
    }
}

#[derive(Clone, Copy)]
pub struct InodeDir<'initfs> {
    inner: InodeStruct<'initfs>,
}
impl<'initfs> InodeDir<'initfs> {
    pub fn inode(self) -> InodeStruct<'initfs> {
        self.inner
    }
    pub fn entry_count(&self) -> Result<u32> {
        let len = self.entries()?.len();

        // NOTE: Len is originally stored as a u32 in the struct, so it can never exceed u32
        // despite first being converted to usize.
        let len = len as u32;

        Ok(len)
    }
    pub fn get_entry(&self, idx: u32) -> Result<Option<Entry<'initfs>>> {
        let idx = usize::try_from(idx).map_err(|_| Error)?;

        self.entries().map(|entries| {
            let entry = entries.get(idx)?;

            Some(Entry {
                entry,
                initfs: self.inner.initfs,
            })
        })
    }
    fn entries(&self) -> Result<&'initfs [DirEntry]> {
        let bytes = self.inner.data()?;
        let entries = plain::slice_from_bytes::<DirEntry>(bytes)
            .expect("expected dir entry to have alignment 1");

        Ok(entries)
    }
}

#[derive(Clone, Copy)]
pub struct InodeLink<'initfs> {
    inner: InodeStruct<'initfs>,
}

impl<'initfs> InodeLink<'initfs> {
    pub fn inode(self) -> InodeStruct<'initfs> {
        self.inner
    }
    pub fn data(&self) -> Result<&'initfs [u8]> {
        self.inner.data()
    }
}

#[derive(Clone, Copy)]
pub struct Entry<'initfs> {
    initfs: InitFs<'initfs>,
    entry: &'initfs DirEntry,
}

impl<'initfs> Entry<'initfs> {
    pub fn inode(&self) -> Inode {
        Inode(self.entry.inode.get())
    }
    pub fn name(&self) -> Result<&'initfs [u8]> {
        let name_offset: usize = self
            .entry
            .name_offset
            .0
            .get()
            .try_into()
            .map_err(|_| Error)?;
        let name_length: usize = self.entry.name_len.get().into();

        let name_end = name_offset.checked_add(name_length).ok_or(Error)?;

        self.initfs.base.get(name_offset..name_end).ok_or(Error)
    }
}

#[derive(Clone, Copy)]
pub enum InodeKind<'initfs> {
    File(InodeFile<'initfs>),
    Dir(InodeDir<'initfs>),
    Link(InodeLink<'initfs>),
    Unknown,
}

impl<'initfs> InodeStruct<'initfs> {
    pub fn id(&self) -> u64 {
        self.inode_id.0.into()
    }
    fn data(&self) -> Result<&'initfs [u8]> {
        let start: usize = self.inode.offset.0.get().try_into().map_err(|_| Error)?;
        let length: usize = self.inode.length.0.get().try_into().map_err(|_| Error)?;

        let end = start.checked_add(length).ok_or(Error)?;

        self.initfs.base.get(start..end).ok_or(Error)
    }
    pub fn mode(&self) -> u16 {
        match self.ty() {
            Some(InodeType::RegularFile) => 0o444,
            Some(InodeType::ExecutableFile) => 0o555,
            Some(InodeType::Dir) => 0o555,
            Some(InodeType::Link) => 0o777,
            None => 0o000,
        }
    }
    fn ty(&self) -> Option<InodeType> {
        InodeType::try_from(self.inode.type_.get()).ok()
    }
    pub fn kind(&self) -> InodeKind<'initfs> {
        let inner = *self;
        match self.ty() {
            Some(InodeType::Dir) => InodeKind::Dir(InodeDir { inner }),
            Some(InodeType::RegularFile | InodeType::ExecutableFile) => {
                InodeKind::File(InodeFile { inner })
            }
            Some(InodeType::Link) => InodeKind::Link(InodeLink { inner }),
            None => InodeKind::Unknown,
        }
    }
}

impl<'initfs> InitFs<'initfs> {
    pub fn new(base: &'initfs [u8], required_page_size: Option<u16>) -> Result<Self> {
        let this = Self { base };

        if base.len() < core::mem::size_of::<Header>() {
            return Err(Error);
        }
        if u32::try_from(base.len()).is_err() {
            return Err(Error);
        }

        let header = this.get_header_assume_valid();

        if header.magic != Magic(MAGIC) {
            return Err(Error);
        }

        let inode_table_offset = header.inode_table_offset.0.get();

        if inode_table_offset < u32::from(Self::header_len_8()) {
            return Err(Error);
        }

        let inode_table_size = this.inode_table_size();

        let inode_table_end = inode_table_offset
            .checked_add(inode_table_size)
            .ok_or(Error)?;

        if inode_table_end > this.base_len_32() {
            return Err(Error);
        }

        if let Some(required_page_size) = required_page_size
            && header.page_size.get() != required_page_size
        {
            return Err(Error);
        }

        // From now on, we can be completely sure that the header and inode tables offsets are
        // valid, and thus continue based on that assumption.

        Ok(this)
    }
    fn get_header_assume_valid(&self) -> &Header {
        plain::from_bytes::<Header>(&self.base[..core::mem::size_of::<Header>()])
            .expect("expected header type to require no alignment, and size to be sufficient")
    }
    pub fn header(&self) -> &Header {
        self.get_header_assume_valid()
    }
    fn header_len_8() -> u8 {
        core::mem::size_of::<Header>()
            .try_into()
            .expect("expected header size to fit within u8")
    }
    fn inode_struct_len_8() -> u8 {
        core::mem::size_of::<InodeHeader>()
            .try_into()
            .expect("expected inode struct size to fit within u8")
    }
    fn inode_table_offset_usize(&self) -> usize {
        // NOTE: We have already validated that the inode table fits within the initfs slice, and
        // that this length, which must fit within usize, also fits within u32.
        self.get_header_assume_valid().inode_table_offset.0.get() as usize
    }
    fn inode_table_end_usize(&self) -> usize {
        // NOTE: This follows the same reasoning as in inode_table_offset_usize(). The end offset
        // has been checked against u32 and the initfs slice length.
        self.inode_table_offset_usize()
            .wrapping_add(self.inode_table_size() as usize)
    }
    fn inode_table_range(&self) -> core::ops::Range<usize> {
        self.inode_table_offset_usize()..self.inode_table_end_usize()
    }
    fn base_len_32(&self) -> u32 {
        // NOTE: We have already validated that the length is sufficient.
        self.base.len() as u32
    }
    fn inode_table_size(&self) -> u32 {
        let count = self.get_header_assume_valid().inode_count.get();
        let struct_size = Self::inode_struct_len_8();

        // NOTE: We know for a fact, that even the largest u8 (255) and the largest u16 (65536),
        // can only be approximately 2^24 at max.
        u32::wrapping_mul(u32::from(count), u32::from(struct_size))
    }
    fn inode_table(&self) -> &'initfs [InodeHeader] {
        let inode_table_bytes = &self.base[self.inode_table_range()];

        plain::slice_from_bytes::<InodeHeader>(inode_table_bytes)
            .expect("expected inode struct alignment to be 1")
    }
    pub fn root_inode(&self) -> Inode {
        Inode(self.get_header_assume_valid().root_inode.get())
    }
    pub fn all_inodes(&self) -> impl Iterator<Item = Inode> {
        (0..self.inode_count()).map(Inode)
    }

    pub fn inode_count(&self) -> u16 {
        self.get_header_assume_valid().inode_count.get()
    }
    pub fn get_inode(&self, inode_id: Inode) -> Option<InodeStruct<'initfs>> {
        Some(InodeStruct {
            initfs: *self,
            inode_id,
            inode: self.inode_table().get(usize::from(inode_id.0))?,
        })
    }
}
