/// numad — Red Bear OS NUMA topology daemon
///
/// Reads ACPI SRAT/SLIT from physical memory via /scheme/memory/physical
/// and feeds NUMA topology hints to the kernel for scheduler placement.
use std::fs;
use std::io::{Read, Write};
use std::mem;

const RSDP_SIGNATURE: &[u8; 8] = b"RSD PTR ";
const SRAT_SIGNATURE: &[u8; 4] = b"SRAT";
const SLIT_SIGNATURE: &[u8; 4] = b"SLIT";
const MAX_NUMA_NODES: usize = 8;

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct Rsdp {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_addr: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct SratEntry {
    entry_type: u8,
    length: u8,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct SratProcessorApic {
    entry: SratEntry,
    proximity_domain_lo: u8,
    apic_id: u8,
    flags: u32,
    local_sapic_eid: u8,
    proximity_domain_hi: [u8; 3],
    clock_domain: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
struct SratMemory {
    entry: SratEntry,
    proximity_domain: u32,
    reserved: u16,
    base_address: u64,
    length: u64,
    reserved2: [u8; 8],
    flags: u32,
    reserved3: [u8; 8],
}

struct NumaNode {
    id: u8,
    apic_ids: Vec<u8>,
}

fn main() {
    eprintln!("numad: starting NUMA topology discovery");

    // Read RSDP from known physical locations (EBDA or BIOS area)
    let rsdp = match find_rsdp() {
        Some(r) => r,
        None => {
            eprintln!("numad: no RSDP found, assuming UMA (single-node)");
            return;
        }
    };

    // Read RSDT to find SRAT and SLIT
    let sdt_addr = rsdp.rsdt_addr as usize;
    let sdt_header = read_phys::<SdtHeader>(sdt_addr);
    if &sdt_header.signature != b"RSDT" {
        eprintln!("numad: no RSDT found");
        return;
    }

    let num_entries = (sdt_header.length as usize - mem::size_of::<SdtHeader>()) / 4;
    let entries_base = sdt_addr + mem::size_of::<SdtHeader>();

    let mut srat_data: Option<Vec<u8>> = None;
    let mut slit_data: Option<Vec<u8>> = None;

    for i in 0..num_entries {
        let entry_addr = entries_base + i * 4;
        let table_ptr: u32 = read_phys(entry_addr);
        let table_addr = table_ptr as usize;
        if table_addr == 0 {
            continue;
        }
        let header = read_phys::<SdtHeader>(table_addr);
        match &header.signature {
            SRAT_SIGNATURE => {
                srat_data = Some(read_phys_bytes(table_addr, header.length as usize));
            }
            SLIT_SIGNATURE => {
                slit_data = Some(read_phys_bytes(table_addr, header.length as usize));
            }
            _ => {}
        }
    }

    let Some(srat) = srat_data else {
        eprintln!("numad: no SRAT found, assuming UMA");
        return;
    };

    let mut nodes: Vec<NumaNode> = Vec::new();
    let sdt_offset = mem::size_of::<SdtHeader>();
    let mut offset = sdt_offset;

    while offset + mem::size_of::<SratEntry>() <= srat.len() {
        let entry: &SratEntry = unsafe { &*(srat.as_ptr().add(offset) as *const SratEntry) };
        if entry.length < mem::size_of::<SratEntry>() as u8 || offset + entry.length as usize > srat.len() {
            break;
        }

        match entry.entry_type {
            0 => {
                // Processor Local APIC
                if entry.length as usize >= mem::size_of::<SratProcessorApic>() {
                    let proc: &SratProcessorApic = unsafe {
                        &*(srat.as_ptr().add(offset) as *const SratProcessorApic)
                    };
                    if proc.flags & 1 != 0 {
                        let proximity = proc.proximity_domain_lo;
                        while nodes.len() <= proximity as usize {
                            nodes.push(NumaNode { id: nodes.len() as u8, apic_ids: Vec::new() });
                        }
                        nodes[proximity as usize].apic_ids.push(proc.apic_id);
                    }
                }
            }
            _ => {}
        }
        offset += entry.length as usize;
    }

    if nodes.is_empty() {
        eprintln!("numad: no CPU entries in SRAT, assuming UMA");
        return;
    }

    eprintln!("numad: found {} NUMA nodes", nodes.len());
    for node in &nodes {
        eprintln!("  node {}: {} CPUs", node.id, node.apic_ids.len());
    }

    // Write topology hints to kernel via proc: scheme
    // Format: "node_id,apic_id\n" per CPU
    if let Ok(mut fd) = fs::OpenOptions::new().write(true).open("/scheme/proc/numa") {
        for node in &nodes {
            let mut line = format!("{},", node.id);
            for apic_id in &node.apic_ids {
                line.push_str(&format!("{},", apic_id));
            }
            line.push('\n');
            let _ = fd.write_all(line.as_bytes());
        }
        eprintln!("numad: topology hints written to kernel");
    } else {
        eprintln!("numad: kernel NUMA interface not available (scheme:proc/numa)");
    }

    eprintln!("numad: NUMA topology discovery complete");
}

fn find_rsdp() -> Option<Rsdp> {
    // Search EBDA and BIOS areas for RSDP signature
    let search_areas: &[(usize, usize)] = &[
        (0x000E_0000, 0x000F_FFFF), // BIOS ROM area
        (0x0008_0000, 0x0009_FFFF), // EBDA/upper conventional
    ];

    for &(start, end) in search_areas {
        for addr in (start..end).step_by(16) {
            if addr + mem::size_of::<Rsdp>() > end {
                break;
            }
            let sig = read_phys_bytes(addr, 8);
            if &sig == RSDP_SIGNATURE {
                let rsdp: Rsdp = read_phys(addr);
                if validate_checksum(&rsdp) {
                    return Some(rsdp);
                }
            }
        }
    }
    None
}

fn validate_checksum(rsdp: &Rsdp) -> bool {
    let bytes = unsafe {
        std::slice::from_raw_parts(rsdp as *const _ as *const u8, mem::size_of::<Rsdp>())
    };
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b)) == 0
}

fn read_phys<T: Copy>(addr: usize) -> T {
    let path = format!("/scheme/memory/physical@{}", addr);
    if let Ok(mut fd) = fs::File::open(&path) {
        let mut buf = vec![0u8; mem::size_of::<T>()];
        if fd.read_exact(&mut buf).is_ok() {
            return unsafe { std::ptr::read(buf.as_ptr() as *const T) };
        }
    }
    unsafe { std::mem::zeroed() }
}

fn read_phys_bytes(addr: usize, len: usize) -> Vec<u8> {
    let path = format!("/scheme/memory/physical@{}", addr);
    if let Ok(mut fd) = fs::File::open(&path) {
        let mut buf = vec![0u8; len];
        if fd.read_exact(&mut buf).is_ok() {
            return buf;
        }
    }
    vec![0u8; len]
}
