use common::io::{Io, Mmio, ReadOnly};
use std::mem;
use syscall::error::{Error, Result, EIO};

use super::MmioRegion;

const MBOX_VBT: u32 = 1 << 3;

fn read_bios_string(array: &[ReadOnly<Mmio<u8>>]) -> String {
    let mut string = String::new();
    for reg in array.iter() {
        let b = reg.read();
        if b == 0 {
            break;
        }
        string.push(b as char);
    }
    string
}

#[repr(C, packed)]
pub struct BiosHeader {
    signature: [ReadOnly<Mmio<u8>>; 16],
    size: ReadOnly<Mmio<u32>>,
    struct_version: ReadOnly<Mmio<u32>>,
    system_bios_version: [ReadOnly<Mmio<u8>>; 32],
    video_bios_version: [ReadOnly<Mmio<u8>>; 16],
    //TODO: should we write graphics driver version?
    graphics_driver_version: [ReadOnly<Mmio<u8>>; 16],
    mailboxes: ReadOnly<Mmio<u32>>,
    driver_model: Mmio<u32>,
    platform_config: ReadOnly<Mmio<u32>>,
    gop_version: [ReadOnly<Mmio<u8>>; 32],
}

impl BiosHeader {
    pub fn dump(&self) {
        eprint!("  op region header");
        eprint!(" signature {:?}", read_bios_string(&self.signature));
        eprint!(" size {:08X}", self.size.read());
        eprint!(" struct_version {:08X}", self.struct_version.read());
        eprint!(
            " system_bios_version {:?}",
            read_bios_string(&self.system_bios_version)
        );
        eprint!(
            " video_bios_version {:?}",
            read_bios_string(&self.video_bios_version)
        );
        eprint!(
            " graphics_driver_version {:?}",
            read_bios_string(&self.graphics_driver_version)
        );
        eprint!(" mailboxes {:08X}", self.mailboxes.read());
        eprint!(" driver_model {:08X}", self.driver_model.read());
        eprint!(" platform_config {:08X}", self.platform_config.read());
        eprint!(" gop_version {:?}", read_bios_string(&self.gop_version));
        eprintln!();
    }
}

#[repr(C, packed)]
pub struct VbtHeader {
    signature: [ReadOnly<Mmio<u8>>; 20],
    version: Mmio<u16>,
    header_size: Mmio<u16>,
    vbt_size: Mmio<u16>,
    vbt_checksum: Mmio<u8>,
    _rsvd: Mmio<u8>,
    bdb_offset: Mmio<u32>,
    aim_offsets: [Mmio<u32>; 4],
}

impl VbtHeader {
    pub fn dump(&self) {
        eprint!("  VBT header");
        eprint!(" signature {:?}", read_bios_string(&self.signature));
        eprint!(" version {:04X}", self.version.read());
        eprint!(" header_size {:04X}", self.header_size.read());
        eprint!(" vbt_size {:04X}", self.vbt_size.read());
        eprint!(" vbt_checksum {:02X}", self.vbt_checksum.read());
        eprint!(" bdb_offset {:08X}", self.bdb_offset.read());
        for (i, aim_offset) in self.aim_offsets.iter().enumerate() {
            eprint!(" aim_offset{} {:08X}", i, aim_offset.read());
        }
        eprintln!();
    }
}

#[repr(C, packed)]
pub struct BdbHeader {
    signature: [ReadOnly<Mmio<u8>>; 16],
    version: Mmio<u16>,
    header_size: Mmio<u16>,
    bdb_size: Mmio<u16>,
}

impl BdbHeader {
    pub fn dump(&self) {
        eprint!("    BDB header");
        eprint!(" signature {:?}", read_bios_string(&self.signature));
        eprint!(" version {:04X}", self.version.read());
        eprint!(" header_size {:04X}", self.header_size.read());
        eprint!(" bdb_size {:04X}", self.bdb_size.read());
        eprintln!();
    }
}

#[repr(C, packed)]
pub struct BdbBlock {
    id: Mmio<u8>,
    size: Mmio<u16>,
}

impl BdbBlock {
    pub fn dump(&self) {
        eprint!("    BDB block");
        eprint!(" id {}", self.id.read());
        eprint!(" size {}", self.size.read());
        eprintln!();
    }
}

#[repr(C, packed)]
pub struct BdbGeneralDefinitions {
    crt_ddc_gmbus_pin: Mmio<u8>,
    dpms: Mmio<u8>,
    boot_displays: [Mmio<u8>; 2],
    child_dev_size: Mmio<u8>,
}

impl BdbGeneralDefinitions {
    pub fn dump(&self) {
        eprint!("      BDB general definitions");
        eprint!(" crt_ddc_gmbus_pin {:02X}", self.crt_ddc_gmbus_pin.read());
        eprint!(" dpms {:02X}", self.dpms.read());
        for (i, boot_display) in self.boot_displays.iter().enumerate() {
            eprint!(" boot_display{} {:02X}", i, boot_display.read());
        }
        eprint!(" child_dev_size {:02X}", self.child_dev_size.read());
        eprintln!();
    }
}

pub struct Bios {
    region: MmioRegion,
    header: &'static mut BiosHeader,
}

impl Bios {
    pub fn new(region: MmioRegion) -> Result<Self> {
        let header = unsafe { &mut *(region.virt as *mut BiosHeader) };
        header.dump();

        {
            let sig = read_bios_string(&header.signature);
            if sig != "IntelGraphicsMem" {
                log::warn!("invalid op region signature {:?}", sig);
                return Err(Error::new(EIO));
            }
        }

        let size = (header.size.read() as usize) * 1024;
        if size != region.size {
            log::warn!("invalid op region size {}", size);
            return Err(Error::new(EIO));
        }

        //TODO: other mailboxes?

        if header.mailboxes.read() & MBOX_VBT == 0 {
            log::warn!("op region does not support VBT mailbox");
            return Err(Error::new(EIO));
        }

        //TODO: read VBT from mailbox 3 RVDA (0x3BA) and RVDS (0x3C2) if missing in mailbox 4
        let vbt_addr = region.virt + 1024;
        let vbt_header = unsafe { &*(vbt_addr as *const VbtHeader) };
        vbt_header.dump();

        //TODO: check vbt checksum
        {
            let sig = read_bios_string(&vbt_header.signature);
            if !sig.starts_with("$VBT") {
                log::warn!("invalid VBT signature {:?}", sig);
                return Err(Error::new(EIO));
            }
        }

        let bdb_addr = vbt_addr + (vbt_header.bdb_offset.read() as usize);
        let bdb_header = unsafe { &*(bdb_addr as *const BdbHeader) };
        bdb_header.dump();
        {
            let sig = read_bios_string(&bdb_header.signature);
            if sig != "BIOS_DATA_BLOCK " {
                log::warn!("invalid BDB signature {:?}", sig);
                bdb_header.dump();
                return Err(Error::new(EIO));
            }
        }

        let mut block_addr = bdb_addr + bdb_header.header_size.read() as usize;
        let block_end = bdb_addr + bdb_header.bdb_size.read() as usize;
        while block_addr + mem::size_of::<BdbBlock>() <= block_end {
            let block = unsafe { &*(block_addr as *const BdbBlock) };
            //TODO: mipi sequence v3 has different size field
            let id = block.id.read();
            let size = block.size.read() as usize;
            block_addr += mem::size_of::<BdbBlock>();
            if block_addr + size <= block_end {
                match id {
                    2 => {
                        if size >= mem::size_of::<BdbGeneralDefinitions>() {
                            let gen_defs =
                                unsafe { &*(block_addr as *const BdbGeneralDefinitions) };
                            gen_defs.dump();
                        } else {
                            log::warn!("BDB general definitions too small");
                            block.dump();
                        }
                    }
                    _ => block.dump(),
                }
                block_addr += block.size.read() as usize;
            } else {
                log::warn!("truncated block id {} size {}", id, size);
                break;
            }
        }

        Ok(Self { region, header })
    }
}
