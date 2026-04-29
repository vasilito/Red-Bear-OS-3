use core::mem::offset_of;

pub const MAGIC_LEN: usize = 8;
pub const MAGIC: [u8; 8] = *b"RedoxFtw";

macro_rules! primitive(
    ($wrapper:ident, $bits:expr, $primitive:ident) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, Default)]
        pub struct $wrapper([u8; $bits / 8]);

        impl $wrapper {
            #[inline]
            pub const fn get(self) -> $primitive {
                <$primitive>::from_le_bytes(self.0)
            }
            #[inline]
            pub const fn new(primitive: $primitive) -> Self {
                Self(<$primitive>::to_le_bytes(primitive))
            }
        }
        impl From<$primitive> for $wrapper {
            fn from(primitive: $primitive) -> Self {
                Self::new(primitive)
            }
        }
        impl From<$wrapper> for $primitive {
            fn from(wrapper: $wrapper) -> Self {
                wrapper.get()
            }
        }
        impl core::fmt::Debug for $wrapper {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{:#0width$x}", self.get(), width = 2 * core::mem::size_of::<$primitive>())
            }
        }
    }
);

primitive!(U16, 16, u16);
primitive!(U32, 32, u32);
primitive!(U64, 64, u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Magic(pub [u8; MAGIC_LEN]);

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Offset(pub U32);

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Length(pub U32);

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub magic: Magic,
    pub inode_table_offset: Offset,
    pub initfs_size: U64,
    pub page_size: U16,
    pub root_inode: U16,
    pub inode_count: U16,
    pub bootstrap_entry: U64,
}

const _: () = {
    // Ensure the offsets of field used by the bootloader stay stable.
    assert!(offset_of!(Header, bootstrap_entry) == 0x1a);
};

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct InodeHeader {
    pub type_: U32,
    pub length: Length,
    pub offset: Offset,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum InodeType {
    RegularFile = 0x0,
    ExecutableFile = 0x1,
    Dir = 0x2,
    Link = 0x3,
    // All other bit patterns are reserved... for now.
}

impl TryFrom<u32> for InodeType {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, ()> {
        Ok(if value == InodeType::RegularFile as u32 {
            InodeType::RegularFile
        } else if value == InodeType::ExecutableFile as u32 {
            InodeType::ExecutableFile
        } else if value == InodeType::Dir as u32 {
            InodeType::Dir
        } else if value == InodeType::Link as u32 {
            InodeType::Link
        } else {
            return Err(());
        })
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct DirEntry {
    pub inode: U16,
    pub name_len: U16,
    pub name_offset: Offset,
}

unsafe impl plain::Plain for Header {}
unsafe impl plain::Plain for InodeHeader {}
unsafe impl plain::Plain for DirEntry {}
