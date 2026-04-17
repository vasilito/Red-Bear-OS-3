use std::collections::HashSet;
use std::env;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::process;

use fat_blockdev::FileDisk;
use fatfs::FsOptions;

struct CheckResult {
    errors: Vec<String>,
    warnings: Vec<String>,
    info: Vec<String>,
}

impl CheckResult {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
        }
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.info.push(msg.into());
    }

    fn print(&self) {
        for msg in &self.info {
            println!("INFO: {msg}");
        }
        for msg in &self.warnings {
            println!("WARNING: {msg}");
        }
        for msg in &self.errors {
            println!("ERROR: {msg}");
        }
        if self.errors.is_empty() && self.warnings.is_empty() {
            println!("Filesystem is clean.");
        } else {
            println!(
                "{} error(s), {} warning(s), {} info message(s)",
                self.errors.len(),
                self.warnings.len(),
                self.info.len()
            );
        }
    }
}

struct RepairStats {
    dirty_flag_cleared: bool,
    fsinfo_corrected: bool,
    lost_clusters_reclaimed: usize,
    orphaned_lfn_entries_removed: usize,
}

impl RepairStats {
    fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.lost_clusters_reclaimed != 0 {
            parts.push(format!("{} lost cluster(s)", self.lost_clusters_reclaimed));
        }
        if self.orphaned_lfn_entries_removed != 0 {
            parts.push(format!(
                "{} orphaned LFN entr(ies)",
                self.orphaned_lfn_entries_removed
            ));
        }
        if self.dirty_flag_cleared {
            parts.push("dirty flag cleared".to_string());
        }
        if self.fsinfo_corrected {
            parts.push("FSInfo updated".to_string());
        }

        if parts.is_empty() {
            "Repaired: nothing needed".to_string()
        } else {
            format!("Repaired: {}", parts.join(", "))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FatKind {
    Fat12,
    Fat16,
    Fat32,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct VolumeInfo {
    fat_kind: FatKind,
    bytes_per_sector: u32,
    sectors_per_cluster: u32,
    reserved_sectors: u32,
    num_fats: u32,
    root_entry_count: u32,
    fat_size_sectors: u32,
    total_sectors: u64,
    root_dir_sectors: u32,
    first_fat_sector: u32,
    first_root_dir_sector: u32,
    first_data_sector: u32,
    total_clusters: u32,
    root_cluster: u32,
    fsinfo_sector: Option<u32>,
}

impl VolumeInfo {
    fn cluster_size(self) -> usize {
        (self.bytes_per_sector * self.sectors_per_cluster) as usize
    }

    fn max_cluster(self) -> u32 {
        self.total_clusters + 1
    }

    fn is_valid_cluster(self, cluster: u32) -> bool {
        (2..=self.max_cluster()).contains(&cluster)
    }

    fn sector_offset(self, sector: u32) -> u64 {
        u64::from(sector) * u64::from(self.bytes_per_sector)
    }

    fn cluster_offset(self, cluster: u32) -> u64 {
        let first_sector = self.first_data_sector + (cluster - 2) * self.sectors_per_cluster;
        self.sector_offset(first_sector)
    }
}

#[derive(Clone, Copy)]
struct FsInfoState {
    sector: u32,
    signatures_ok: bool,
    recorded_free_clusters: u32,
    recorded_next_free: u32,
}

#[derive(Clone, Copy)]
struct RawDirEntry {
    offset: u64,
    data: [u8; 32],
}

#[derive(Clone, Copy)]
struct PendingLfnEntry {
    offset: u64,
    order: u8,
    checksum: u8,
}

struct ScanState {
    dirty_flag_set: bool,
    fsinfo: Option<FsInfoState>,
    actual_free_clusters: u32,
    first_free_cluster: Option<u32>,
    reachable_clusters: Vec<bool>,
    lost_clusters: Vec<u32>,
    orphaned_lfn_offsets: Vec<u64>,
}

enum DirectoryLocation {
    RootFixed,
    ClusterChain(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClusterState {
    Free,
    Next(u32),
    Eof,
    Bad,
    Reserved,
    Invalid,
}

fn usage() -> ! {
    eprintln!("Usage: fat-check [--repair|-r] <device>");
    process::exit(1)
}

fn exit_with_result(result: &CheckResult, repairs_made: bool) -> ! {
    result.print();
    if repairs_made {
        process::exit(2);
    }
    if !result.errors.is_empty() {
        process::exit(1);
    }
    process::exit(0);
}

fn read_exact_at(disk: &mut FileDisk, offset: u64, buf: &mut [u8]) -> io::Result<()> {
    disk.seek(SeekFrom::Start(offset))?;
    disk.read_exact(buf)
}

fn write_all_at(disk: &mut FileDisk, offset: u64, buf: &[u8]) -> io::Result<()> {
    disk.seek(SeekFrom::Start(offset))?;
    disk.write_all(buf)
}

fn read_boot_sector(disk: &mut FileDisk) -> io::Result<[u8; 512]> {
    let mut boot_sector = [0u8; 512];
    read_exact_at(disk, 0, &mut boot_sector)?;
    Ok(boot_sector)
}

fn check_bpb(result: &mut CheckResult, boot_sector: &[u8; 512]) -> Option<VolumeInfo> {
    let signature = &boot_sector[510..512];
    if signature != [0x55, 0xAA] {
        result.error(format!(
            "invalid boot sector signature: {:02X} {:02X} (expected 55 AA)",
            signature[0], signature[1]
        ));
    }

    let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]) as u32;
    if ![512, 1024, 2048, 4096].contains(&bytes_per_sector) {
        result.error(format!("invalid bytes per sector: {bytes_per_sector}"));
        return None;
    }
    result.info(format!("bytes per sector: {bytes_per_sector}"));

    let sectors_per_cluster = boot_sector[13] as u32;
    if sectors_per_cluster == 0 || (sectors_per_cluster & (sectors_per_cluster - 1)) != 0 {
        result.error(format!(
            "invalid sectors per cluster: {sectors_per_cluster} (must be power of 2)"
        ));
        return None;
    }
    result.info(format!("sectors per cluster: {sectors_per_cluster}"));

    let reserved_sectors = u16::from_le_bytes([boot_sector[14], boot_sector[15]]) as u32;
    if reserved_sectors == 0 {
        result.error("reserved sector count is 0");
        return None;
    }

    let num_fats = boot_sector[16] as u32;
    if num_fats == 0 {
        result.error("number of FATs is 0");
        return None;
    }
    result.info(format!("number of FATs: {num_fats}"));

    let root_entry_count = u16::from_le_bytes([boot_sector[17], boot_sector[18]]) as u32;
    let total_sectors_16 = u16::from_le_bytes([boot_sector[19], boot_sector[20]]) as u64;
    let fat_size_16 = u16::from_le_bytes([boot_sector[22], boot_sector[23]]) as u32;
    let total_sectors_32 = u32::from_le_bytes([
        boot_sector[32],
        boot_sector[33],
        boot_sector[34],
        boot_sector[35],
    ]) as u64;
    let fat_size_32 = u32::from_le_bytes([
        boot_sector[36],
        boot_sector[37],
        boot_sector[38],
        boot_sector[39],
    ]);

    let total_sectors = if total_sectors_32 != 0 {
        total_sectors_32
    } else {
        total_sectors_16
    };
    if total_sectors == 0 {
        result.error("total sector count is 0");
        return None;
    }
    result.info(format!("total sectors: {total_sectors}"));

    let root_dir_sectors = (root_entry_count * 32).div_ceil(bytes_per_sector);
    let fat_size_sectors = if fat_size_32 != 0 && root_entry_count == 0 {
        fat_size_32
    } else {
        fat_size_16
    };
    if fat_size_sectors == 0 {
        result.error("FAT size is 0 sectors");
        return None;
    }

    let layout_sectors = reserved_sectors + num_fats * fat_size_sectors + root_dir_sectors;
    let total_sectors_u32 = total_sectors as u32;
    if total_sectors_u32 <= layout_sectors {
        result.error("filesystem layout leaves no data area");
        return None;
    }

    let data_sectors = total_sectors_u32 - layout_sectors;
    let total_clusters = data_sectors / sectors_per_cluster;
    let fat_kind = if fat_size_32 != 0 && root_entry_count == 0 {
        FatKind::Fat32
    } else if total_clusters < 4085 {
        FatKind::Fat12
    } else {
        FatKind::Fat16
    };

    match fat_kind {
        FatKind::Fat32 => {
            result.info("filesystem type: FAT32");
            result.info(format!("FAT size: {fat_size_sectors} sectors"));
        }
        FatKind::Fat16 => {
            result.info("filesystem type: FAT16");
            result.info(format!("FAT size: {fat_size_sectors} sectors"));
        }
        FatKind::Fat12 => {
            result.info("filesystem type: FAT12");
        }
    }

    let first_fat_sector = reserved_sectors;
    let first_root_dir_sector = reserved_sectors + num_fats * fat_size_sectors;
    let first_data_sector = first_root_dir_sector + root_dir_sectors;
    let root_cluster = if fat_kind == FatKind::Fat32 {
        u32::from_le_bytes([
            boot_sector[44],
            boot_sector[45],
            boot_sector[46],
            boot_sector[47],
        ])
    } else {
        0
    };
    let fsinfo_sector = if fat_kind == FatKind::Fat32 {
        Some(u16::from_le_bytes([boot_sector[48], boot_sector[49]]) as u32)
    } else {
        None
    };

    Some(VolumeInfo {
        fat_kind,
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        root_entry_count,
        fat_size_sectors,
        total_sectors,
        root_dir_sectors,
        first_fat_sector,
        first_root_dir_sector,
        first_data_sector,
        total_clusters,
        root_cluster,
        fsinfo_sector,
    })
}

fn fat_entry(fat: &[u8], fat_kind: FatKind, cluster: u32) -> u32 {
    match fat_kind {
        FatKind::Fat12 => {
            let offset = (cluster as usize * 3) / 2;
            if offset + 1 >= fat.len() {
                return 0;
            }
            let word = u16::from_le_bytes([fat[offset], fat[offset + 1]]);
            if cluster & 1 == 0 {
                u32::from(word & 0x0FFF)
            } else {
                u32::from(word >> 4)
            }
        }
        FatKind::Fat16 => {
            let offset = cluster as usize * 2;
            if offset + 1 >= fat.len() {
                return 0;
            }
            u32::from(u16::from_le_bytes([fat[offset], fat[offset + 1]]))
        }
        FatKind::Fat32 => {
            let offset = cluster as usize * 4;
            if offset + 3 >= fat.len() {
                return 0;
            }
            u32::from_le_bytes([fat[offset], fat[offset + 1], fat[offset + 2], fat[offset + 3]])
                & 0x0FFF_FFFF
        }
    }
}

fn set_fat_entry(fat: &mut [u8], fat_kind: FatKind, cluster: u32, value: u32) {
    match fat_kind {
        FatKind::Fat12 => {
            let offset = (cluster as usize * 3) / 2;
            if offset + 1 >= fat.len() {
                return;
            }
            let current = u16::from_le_bytes([fat[offset], fat[offset + 1]]);
            let masked = (value & 0x0FFF) as u16;
            let updated = if cluster & 1 == 0 {
                (current & 0xF000) | masked
            } else {
                (current & 0x000F) | (masked << 4)
            };
            let bytes = updated.to_le_bytes();
            fat[offset] = bytes[0];
            fat[offset + 1] = bytes[1];
        }
        FatKind::Fat16 => {
            let offset = cluster as usize * 2;
            if offset + 1 >= fat.len() {
                return;
            }
            let bytes = (value as u16).to_le_bytes();
            fat[offset] = bytes[0];
            fat[offset + 1] = bytes[1];
        }
        FatKind::Fat32 => {
            let offset = cluster as usize * 4;
            if offset + 3 >= fat.len() {
                return;
            }
            let current = u32::from_le_bytes([fat[offset], fat[offset + 1], fat[offset + 2], fat[offset + 3]]);
            let updated = (current & 0xF000_0000) | (value & 0x0FFF_FFFF);
            let bytes = updated.to_le_bytes();
            fat[offset] = bytes[0];
            fat[offset + 1] = bytes[1];
            fat[offset + 2] = bytes[2];
            fat[offset + 3] = bytes[3];
        }
    }
}

fn classify_cluster(fat_kind: FatKind, value: u32, max_cluster: u32) -> ClusterState {
    match fat_kind {
        FatKind::Fat12 => match value & 0x0FFF {
            0x000 => ClusterState::Free,
            0x002..=0x0FEF if value <= max_cluster => ClusterState::Next(value),
            0x0FF7 => ClusterState::Bad,
            0x0FF8..=0x0FFF => ClusterState::Eof,
            _ => ClusterState::Reserved,
        },
        FatKind::Fat16 => match value {
            0x0000 => ClusterState::Free,
            0x0002..=0xFFEF if value <= max_cluster => ClusterState::Next(value),
            0xFFF7 => ClusterState::Bad,
            0xFFF8..=0xFFFF => ClusterState::Eof,
            0xFFF0..=0xFFF6 | 0x0001 => ClusterState::Reserved,
            _ => ClusterState::Invalid,
        },
        FatKind::Fat32 => match value & 0x0FFF_FFFF {
            0x0000_0000 => ClusterState::Free,
            0x0000_0002..=0x0FFF_FFEF if value <= max_cluster => ClusterState::Next(value),
            0x0FFF_FFF7 => ClusterState::Bad,
            0x0FFF_FFF8..=0x0FFF_FFFF => ClusterState::Eof,
            0x0FFF_FFF0..=0x0FFF_FFF6 | 0x0000_0001 => ClusterState::Reserved,
            _ => ClusterState::Invalid,
        },
    }
}

fn read_fat(disk: &mut FileDisk, info: VolumeInfo) -> io::Result<Vec<u8>> {
    let mut fat = vec![0u8; (info.fat_size_sectors * info.bytes_per_sector) as usize];
    read_exact_at(disk, info.sector_offset(info.first_fat_sector), &mut fat)?;
    Ok(fat)
}

fn write_fat_copies(disk: &mut FileDisk, info: VolumeInfo, fat: &[u8]) -> io::Result<()> {
    for fat_index in 0..info.num_fats {
        let sector = info.first_fat_sector + fat_index * info.fat_size_sectors;
        write_all_at(disk, info.sector_offset(sector), fat)?;
    }
    disk.flush()
}

fn dirty_flag_set(info: VolumeInfo, fat: &[u8]) -> bool {
    match info.fat_kind {
        FatKind::Fat12 => {
            let entry = fat_entry(fat, info.fat_kind, 1);
            entry & 0x0800 == 0 || entry & 0x0400 == 0
        }
        FatKind::Fat16 => {
            let entry = fat_entry(fat, info.fat_kind, 1);
            entry & 0x8000 == 0 || entry & 0x4000 == 0
        }
        FatKind::Fat32 => {
            let entry = fat_entry(fat, info.fat_kind, 1);
            entry & 0x0800_0000 == 0 || entry & 0x0400_0000 == 0
        }
    }
}

fn read_fsinfo(disk: &mut FileDisk, info: VolumeInfo) -> io::Result<Option<FsInfoState>> {
    let Some(sector) = info.fsinfo_sector else {
        return Ok(None);
    };
    let mut fsinfo = vec![0u8; info.bytes_per_sector as usize];
    read_exact_at(disk, info.sector_offset(sector), &mut fsinfo)?;
    if fsinfo.len() < 496 {
        return Ok(Some(FsInfoState {
            sector,
            signatures_ok: false,
            recorded_free_clusters: 0,
            recorded_next_free: 0,
        }));
    }

    let sig1 = fsinfo[0..4] == [0x52, 0x52, 0x61, 0x41];
    let sig2 = fsinfo[484..488] == [0x72, 0x72, 0x41, 0x61];

    Ok(Some(FsInfoState {
        sector,
        signatures_ok: sig1 && sig2,
        recorded_free_clusters: u32::from_le_bytes([fsinfo[488], fsinfo[489], fsinfo[490], fsinfo[491]]),
        recorded_next_free: u32::from_le_bytes([fsinfo[492], fsinfo[493], fsinfo[494], fsinfo[495]]),
    }))
}

fn collect_chain_clusters(info: VolumeInfo, fat: &[u8], start_cluster: u32) -> Vec<u32> {
    let mut clusters = Vec::new();
    if !info.is_valid_cluster(start_cluster) {
        return clusters;
    }

    let mut seen = HashSet::new();
    let mut cluster = start_cluster;
    loop {
        if !info.is_valid_cluster(cluster) || !seen.insert(cluster) {
            break;
        }
        clusters.push(cluster);
        match classify_cluster(info.fat_kind, fat_entry(fat, info.fat_kind, cluster), info.max_cluster()) {
            ClusterState::Next(next) => cluster = next,
            ClusterState::Eof => break,
            ClusterState::Free | ClusterState::Bad | ClusterState::Reserved | ClusterState::Invalid => break,
        }
    }

    clusters
}

fn mark_chain_reachable(scan: &mut ScanState, info: VolumeInfo, fat: &[u8], start_cluster: u32) {
    if !info.is_valid_cluster(start_cluster) {
        return;
    }

    let mut seen = HashSet::new();
    let mut cluster = start_cluster;
    loop {
        if !info.is_valid_cluster(cluster) || !seen.insert(cluster) {
            break;
        }
        if scan.reachable_clusters[cluster as usize] {
            break;
        }
        scan.reachable_clusters[cluster as usize] = true;
        match classify_cluster(info.fat_kind, fat_entry(fat, info.fat_kind, cluster), info.max_cluster()) {
            ClusterState::Next(next) => cluster = next,
            ClusterState::Eof => break,
            ClusterState::Free | ClusterState::Bad | ClusterState::Reserved | ClusterState::Invalid => break,
        }
    }
}

fn push_directory_entries(entries: &mut Vec<RawDirEntry>, base_offset: u64, data: &[u8]) {
    for (index, chunk) in data.chunks_exact(32).enumerate() {
        let mut entry = [0u8; 32];
        entry.copy_from_slice(chunk);
        entries.push(RawDirEntry {
            offset: base_offset + (index as u64 * 32),
            data: entry,
        });
    }
}

fn read_directory_entries(
    disk: &mut FileDisk,
    info: VolumeInfo,
    fat: &[u8],
    location: DirectoryLocation,
) -> io::Result<Vec<RawDirEntry>> {
    let mut entries = Vec::new();
    match location {
        DirectoryLocation::RootFixed => {
            let size = (info.root_dir_sectors * info.bytes_per_sector) as usize;
            let mut buffer = vec![0u8; size];
            read_exact_at(disk, info.sector_offset(info.first_root_dir_sector), &mut buffer)?;
            push_directory_entries(&mut entries, info.sector_offset(info.first_root_dir_sector), &buffer);
        }
        DirectoryLocation::ClusterChain(start_cluster) => {
            for cluster in collect_chain_clusters(info, fat, start_cluster) {
                let mut buffer = vec![0u8; info.cluster_size()];
                read_exact_at(disk, info.cluster_offset(cluster), &mut buffer)?;
                push_directory_entries(&mut entries, info.cluster_offset(cluster), &buffer);
            }
        }
    }
    Ok(entries)
}

fn lfn_checksum(short_name: &[u8; 11]) -> u8 {
    let mut checksum = 0u8;
    for byte in short_name {
        checksum = ((checksum & 1) << 7).wrapping_add(checksum >> 1).wrapping_add(*byte);
    }
    checksum
}

fn lfn_matches_short(pending: &[PendingLfnEntry], short_entry: &[u8; 32]) -> bool {
    if pending.is_empty() {
        return true;
    }

    let mut short_name = [0u8; 11];
    short_name.copy_from_slice(&short_entry[0..11]);
    let checksum = lfn_checksum(&short_name);
    let total = pending.len() as u8;

    for (index, entry) in pending.iter().enumerate() {
        if entry.checksum != checksum {
            return false;
        }

        let order = entry.order & 0x1F;
        let expected = total.saturating_sub(index as u8);
        if order == 0 || order != expected {
            return false;
        }

        let last = entry.order & 0x40 != 0;
        if index == 0 {
            if !last || order != total {
                return false;
            }
        } else if last {
            return false;
        }
    }

    true
}

fn mark_pending_lfns_orphaned(scan: &mut ScanState, pending: &[PendingLfnEntry]) {
    for entry in pending {
        scan.orphaned_lfn_offsets.push(entry.offset);
    }
}

fn entry_start_cluster(info: VolumeInfo, entry: &[u8; 32]) -> u32 {
    let low = u16::from_le_bytes([entry[26], entry[27]]) as u32;
    if info.fat_kind == FatKind::Fat32 {
        let high = u16::from_le_bytes([entry[20], entry[21]]) as u32;
        ((high << 16) | low) & 0x0FFF_FFFF
    } else {
        low
    }
}

fn scan_directory_entries(
    info: VolumeInfo,
    fat: &[u8],
    scan: &mut ScanState,
    entries: &[RawDirEntry],
    subdirs: &mut Vec<u32>,
) {
    let mut pending = Vec::new();

    for entry in entries {
        let first = entry.data[0];
        if first == 0x00 {
            if !pending.is_empty() {
                mark_pending_lfns_orphaned(scan, &pending);
            }
            return;
        }
        if first == 0xE5 {
            if !pending.is_empty() {
                mark_pending_lfns_orphaned(scan, &pending);
                pending.clear();
            }
            continue;
        }

        let attr = entry.data[11];
        if attr == 0x0F {
            pending.push(PendingLfnEntry {
                offset: entry.offset,
                order: entry.data[0],
                checksum: entry.data[13],
            });
            continue;
        }

        if !pending.is_empty() {
            if !lfn_matches_short(&pending, &entry.data) {
                mark_pending_lfns_orphaned(scan, &pending);
            }
            pending.clear();
        }

        if first == b'.' || attr & 0x08 != 0 {
            continue;
        }

        let start_cluster = entry_start_cluster(info, &entry.data);
        if start_cluster >= 2 {
            mark_chain_reachable(scan, info, fat, start_cluster);
        }

        if attr & 0x10 != 0 && start_cluster >= 2 {
            subdirs.push(start_cluster);
        }
    }

    if !pending.is_empty() {
        mark_pending_lfns_orphaned(scan, &pending);
    }
}

fn scan_directory(
    disk: &mut FileDisk,
    info: VolumeInfo,
    fat: &[u8],
    location: DirectoryLocation,
    scan: &mut ScanState,
    visited_dirs: &mut HashSet<u32>,
) -> io::Result<()> {
    let entries = read_directory_entries(disk, info, fat, location)?;
    let mut subdirs = Vec::new();
    scan_directory_entries(info, fat, scan, &entries, &mut subdirs);

    for cluster in subdirs {
        if visited_dirs.insert(cluster) {
            scan_directory(
                disk,
                info,
                fat,
                DirectoryLocation::ClusterChain(cluster),
                scan,
                visited_dirs,
            )?;
        }
    }

    Ok(())
}

fn scan_filesystem(disk: &mut FileDisk, info: VolumeInfo, fat: &[u8]) -> io::Result<ScanState> {
    let mut scan = ScanState {
        dirty_flag_set: dirty_flag_set(info, fat),
        fsinfo: read_fsinfo(disk, info)?,
        actual_free_clusters: 0,
        first_free_cluster: None,
        reachable_clusters: vec![false; (info.max_cluster() + 1) as usize],
        lost_clusters: Vec::new(),
        orphaned_lfn_offsets: Vec::new(),
    };

    for cluster in 2..=info.max_cluster() {
        if matches!(
            classify_cluster(info.fat_kind, fat_entry(fat, info.fat_kind, cluster), info.max_cluster()),
            ClusterState::Free
        ) {
            scan.actual_free_clusters += 1;
            if scan.first_free_cluster.is_none() {
                scan.first_free_cluster = Some(cluster);
            }
        }
    }

    let mut visited_dirs = HashSet::new();
    match info.fat_kind {
        FatKind::Fat32 => {
            if info.root_cluster >= 2 {
                mark_chain_reachable(&mut scan, info, fat, info.root_cluster);
                visited_dirs.insert(info.root_cluster);
                scan_directory(
                    disk,
                    info,
                    fat,
                    DirectoryLocation::ClusterChain(info.root_cluster),
                    &mut scan,
                    &mut visited_dirs,
                )?;
            }
        }
        FatKind::Fat12 | FatKind::Fat16 => {
            scan_directory(
                disk,
                info,
                fat,
                DirectoryLocation::RootFixed,
                &mut scan,
                &mut visited_dirs,
            )?;
        }
    }

    for cluster in 2..=info.max_cluster() {
        match classify_cluster(info.fat_kind, fat_entry(fat, info.fat_kind, cluster), info.max_cluster()) {
            ClusterState::Next(_) | ClusterState::Eof if !scan.reachable_clusters[cluster as usize] => {
                scan.lost_clusters.push(cluster);
            }
            ClusterState::Free
            | ClusterState::Next(_)
            | ClusterState::Eof
            | ClusterState::Bad
            | ClusterState::Reserved
            | ClusterState::Invalid => {}
        }
    }

    Ok(scan)
}

fn check_dirty_flags(result: &mut CheckResult, scan: &ScanState) {
    if scan.dirty_flag_set {
        result.error("filesystem has unclean shutdown flags set");
    }
}

fn check_fsinfo(result: &mut CheckResult, info: VolumeInfo, scan: &ScanState) {
    if info.fat_kind != FatKind::Fat32 {
        return;
    }

    let Some(fsinfo) = scan.fsinfo else {
        result.error("FAT32 FSInfo sector is missing");
        return;
    };

    if !fsinfo.signatures_ok {
        result.error(format!("invalid FAT32 FSInfo signatures at sector {}", fsinfo.sector));
        return;
    }

    if fsinfo.recorded_free_clusters == 0xFFFF_FFFF {
        result.info("FSInfo free cluster count is unknown (newly formatted)");
    } else if fsinfo.recorded_free_clusters != scan.actual_free_clusters {
        result.error(format!(
            "FSInfo free cluster count mismatch: recorded {}, actual {}",
            fsinfo.recorded_free_clusters, scan.actual_free_clusters
        ));
    }
}

fn check_lost_clusters(result: &mut CheckResult, scan: &ScanState) {
    if !scan.lost_clusters.is_empty() {
        result.error(format!("lost clusters found: {}", scan.lost_clusters.len()));
    }
}

fn check_orphaned_lfns(result: &mut CheckResult, scan: &ScanState) {
    if !scan.orphaned_lfn_offsets.is_empty() {
        result.error(format!(
            "orphaned LFN entries found: {}",
            scan.orphaned_lfn_offsets.len()
        ));
    }
}

fn report_repair_findings(result: &mut CheckResult, info: VolumeInfo, scan: &ScanState) {
    if scan.dirty_flag_set {
        result.info("found unclean shutdown flags");
    }
    if info.fat_kind == FatKind::Fat32 {
        if let Some(fsinfo) = scan.fsinfo {
            if !fsinfo.signatures_ok {
                result.warn(format!(
                    "cannot repair FSInfo sector {}: invalid signatures",
                    fsinfo.sector
                ));
            } else if fsinfo.recorded_free_clusters != scan.actual_free_clusters {
                result.info(format!(
                    "found FSInfo free cluster count mismatch: recorded {}, actual {}",
                    fsinfo.recorded_free_clusters, scan.actual_free_clusters
                ));
            }
        }
    }
    if !scan.lost_clusters.is_empty() {
        result.info(format!("found {} lost cluster(s)", scan.lost_clusters.len()));
    }
    if !scan.orphaned_lfn_offsets.is_empty() {
        result.info(format!(
            "found {} orphaned LFN entr(ies)",
            scan.orphaned_lfn_offsets.len()
        ));
    }
}

fn repair_dirty_flags(stats: &mut RepairStats, info: VolumeInfo, fat: &mut [u8], scan: &ScanState) -> bool {
    if !scan.dirty_flag_set {
        return false;
    }

    match info.fat_kind {
        FatKind::Fat12 => {
            let entry = fat_entry(fat, info.fat_kind, 1);
            set_fat_entry(fat, info.fat_kind, 1, entry | 0x0C00);
            stats.dirty_flag_cleared = true;
            true
        }
        FatKind::Fat16 => {
            let entry = fat_entry(fat, info.fat_kind, 1);
            set_fat_entry(fat, info.fat_kind, 1, entry | 0xC000);
            stats.dirty_flag_cleared = true;
            true
        }
        FatKind::Fat32 => {
            let entry = fat_entry(fat, info.fat_kind, 1);
            set_fat_entry(fat, info.fat_kind, 1, entry | 0x0C00_0000);
            stats.dirty_flag_cleared = true;
            true
        }
    }
}

fn repair_lost_clusters(stats: &mut RepairStats, info: VolumeInfo, fat: &mut [u8], scan: &ScanState) -> bool {
    if scan.lost_clusters.is_empty() {
        return false;
    }

    for cluster in &scan.lost_clusters {
        set_fat_entry(fat, info.fat_kind, *cluster, 0);
    }
    stats.lost_clusters_reclaimed = scan.lost_clusters.len();
    true
}

fn repair_orphaned_lfns(
    stats: &mut RepairStats,
    disk: &mut FileDisk,
    scan: &ScanState,
) -> io::Result<()> {
    if scan.orphaned_lfn_offsets.is_empty() {
        return Ok(());
    }

    let mut deleted = [0u8; 32];
    deleted[0] = 0xE5;
    for offset in &scan.orphaned_lfn_offsets {
        write_all_at(disk, *offset, &deleted)?;
    }
    disk.flush()?;
    stats.orphaned_lfn_entries_removed = scan.orphaned_lfn_offsets.len();
    Ok(())
}

fn repair_fsinfo(
    stats: &mut RepairStats,
    disk: &mut FileDisk,
    info: VolumeInfo,
    scan: &ScanState,
) -> io::Result<()> {
    if info.fat_kind != FatKind::Fat32 {
        return Ok(());
    }

    let Some(fsinfo_state) = scan.fsinfo else {
        return Ok(());
    };
    if !fsinfo_state.signatures_ok {
        return Ok(());
    }

    let desired_free = scan.actual_free_clusters;
    let desired_next = scan.first_free_cluster.unwrap_or(0xFFFF_FFFF);
    if fsinfo_state.recorded_free_clusters == desired_free
        && fsinfo_state.recorded_next_free == desired_next
    {
        return Ok(());
    }

    let mut fsinfo = vec![0u8; info.bytes_per_sector as usize];
    read_exact_at(disk, info.sector_offset(fsinfo_state.sector), &mut fsinfo)?;
    if fsinfo.len() < 496 {
        return Ok(());
    }

    let free_bytes = desired_free.to_le_bytes();
    fsinfo[488] = free_bytes[0];
    fsinfo[489] = free_bytes[1];
    fsinfo[490] = free_bytes[2];
    fsinfo[491] = free_bytes[3];

    let next_bytes = desired_next.to_le_bytes();
    fsinfo[492] = next_bytes[0];
    fsinfo[493] = next_bytes[1];
    fsinfo[494] = next_bytes[2];
    fsinfo[495] = next_bytes[3];

    write_all_at(disk, info.sector_offset(fsinfo_state.sector), &fsinfo)?;
    disk.flush()?;
    stats.fsinfo_corrected = true;
    Ok(())
}

fn check_directory_tree(result: &mut CheckResult, fs: &fatfs::FileSystem<fscommon::BufStream<FileDisk>>) {
    let root_dir = fs.root_dir();

    fn walk_dir(
        dir: &fatfs::Dir<fscommon::BufStream<FileDisk>>,
        path: &str,
        result: &mut CheckResult,
        stats: &mut (u64, u64, u64),
    ) {
        for entry in dir.iter().filter_map(|e| e.ok()) {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let full_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };

            let len = entry.len();
            if entry.is_dir() {
                stats.1 += 1;
                let sub_dir = entry.to_dir();
                walk_dir(&sub_dir, &full_path, result, stats);
            } else {
                stats.0 += 1;
                stats.2 += len;
            }
        }
    }

    let mut stats = (0u64, 0u64, 0u64);
    walk_dir(&root_dir, "", result, &mut stats);
    result.info(format!("files: {}, directories: {}, total data: {} bytes", stats.0, stats.1, stats.2));
}

fn parse_args() -> (bool, String) {
    let mut repair = false;
    let mut device = None;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--repair" | "-r" => repair = true,
            other if other.starts_with('-') => {
                eprintln!("fat-check: unknown option '{other}'");
                usage();
            }
            _ if device.is_none() => device = Some(arg),
            _ => usage(),
        }
    }

    let device = device.unwrap_or_else(|| usage());
    (repair, device)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test fat_entry encoding

    #[test]
    fn test_fat_entry_fat12_even_cluster() {
        // FAT12: cluster 0 at offset 0, word [0..1], masked with 0x0FFF
        // Entry at cluster 0: bytes [0xFF, 0xF0] -> word 0xF0FF -> & 0x0FFF = 0x0FF
        let fat = [0xFF, 0xF0, 0xFF, 0xFF, 0xFF, 0xFF];
        let entry = fat_entry(&fat, FatKind::Fat12, 0);
        assert_eq!(entry, 0x0FF);
    }

    #[test]
    fn test_fat_entry_fat12_odd_cluster() {
        // FAT12: cluster 1 at offset (1*3)/2 = 1 (byte offset), word [1..2]
        // word = 0xFFFF, odd cluster -> entry = word >> 4 = 0x0FFF
        let fat = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let entry = fat_entry(&fat, FatKind::Fat12, 1);
        assert_eq!(entry, 0x0FFF);
    }

    #[test]
    fn test_fat_entry_fat12_cluster2() {
        // FAT12: cluster 2 at offset (2*3)/2 = 3, word [3..4]
        // word = 0xFFFF, even cluster -> entry = word & 0x0FFF = 0x0FFF
        let fat = [0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00];
        let entry = fat_entry(&fat, FatKind::Fat12, 2);
        assert_eq!(entry, 0x0FFF);
    }

    #[test]
    fn test_fat_entry_fat16_basic() {
        // FAT16: 8 bytes, entry at cluster * 2
        // Cluster 0: bytes [0,1] = 0x0000
        let fat = [0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];
        let entry = fat_entry(&fat, FatKind::Fat16, 0);
        assert_eq!(entry, 0x0000);

        // Cluster 1: bytes [2,3] = 0xFFFF
        let entry1 = fat_entry(&fat, FatKind::Fat16, 1);
        assert_eq!(entry1, 0xFFFF);

        // Cluster 2: bytes [4,5] = 0x0000
        let entry2 = fat_entry(&fat, FatKind::Fat16, 2);
        assert_eq!(entry2, 0x0000);
    }

    #[test]
    fn test_fat_entry_fat32_basic() {
        // FAT32: 16 bytes, entry at cluster * 4
        // Cluster 0: bytes [0..3] = 0x00000000
        let fat = [0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
                  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let entry = fat_entry(&fat, FatKind::Fat32, 0);
        assert_eq!(entry, 0x0000_0000);

        // Cluster 1: bytes [4..7] = 0xFFFFFFFF
        let entry1 = fat_entry(&fat, FatKind::Fat32, 1);
        assert_eq!(entry1, 0x0FFF_FFFF);
    }

    // Test set_fat_entry round-trip

    #[test]
    fn test_set_fat_entry_roundtrip_fat12() {
        let mut fat = [0u8; 6];
        let test_values = [0x001u32, 0x002u32, 0x0FFFu32, 0x0FEFu32];

        for (i, &val) in test_values.iter().enumerate() {
            let cluster = i as u32;
            set_fat_entry(&mut fat, FatKind::Fat12, cluster, val);
            let read = fat_entry(&fat, FatKind::Fat12, cluster);
            assert_eq!(read, val, "FAT12 cluster {} round-trip failed", cluster);
        }
    }

    #[test]
    fn test_set_fat_entry_roundtrip_fat16() {
        let mut fat = [0u8; 8];
        set_fat_entry(&mut fat, FatKind::Fat16, 0, 0x0000);
        assert_eq!(fat_entry(&fat, FatKind::Fat16, 0), 0x0000);

        set_fat_entry(&mut fat, FatKind::Fat16, 1, 0x0001);
        assert_eq!(fat_entry(&fat, FatKind::Fat16, 1), 0x0001);

        set_fat_entry(&mut fat, FatKind::Fat16, 2, 0xFFEF);
        assert_eq!(fat_entry(&fat, FatKind::Fat16, 2), 0xFFEF);

        set_fat_entry(&mut fat, FatKind::Fat16, 3, 0xFFFF);
        assert_eq!(fat_entry(&fat, FatKind::Fat16, 3), 0xFFFF);
    }

    #[test]
    fn test_set_fat_entry_roundtrip_fat32() {
        let mut fat = [0u8; 16];
        set_fat_entry(&mut fat, FatKind::Fat32, 0, 0x0000_0000);
        assert_eq!(fat_entry(&fat, FatKind::Fat32, 0), 0x0000_0000);

        set_fat_entry(&mut fat, FatKind::Fat32, 1, 0x0000_0001);
        assert_eq!(fat_entry(&fat, FatKind::Fat32, 1), 0x0000_0001);

        set_fat_entry(&mut fat, FatKind::Fat32, 2, 0x0FFF_FFEF);
        assert_eq!(fat_entry(&fat, FatKind::Fat32, 2), 0x0FFF_FFEF);

        set_fat_entry(&mut fat, FatKind::Fat32, 3, 0x0FFF_FFFF);
        assert_eq!(fat_entry(&fat, FatKind::Fat32, 3), 0x0FFF_FFFF);
    }

    // Test classify_cluster

    #[test]
    fn test_classify_cluster_fat12_free() {
        assert_eq!(classify_cluster(FatKind::Fat12, 0x000, 0x0FEF), ClusterState::Free);
    }

    #[test]
    fn test_classify_cluster_fat12_next() {
        assert_eq!(classify_cluster(FatKind::Fat12, 0x002, 0x0FEF), ClusterState::Next(0x002));
        assert_eq!(classify_cluster(FatKind::Fat12, 0x0FEF, 0x0FEF), ClusterState::Next(0x0FEF));
    }

    #[test]
    fn test_classify_cluster_fat12_bad() {
        assert_eq!(classify_cluster(FatKind::Fat12, 0x0FF7, 0x0FEF), ClusterState::Bad);
    }

    #[test]
    fn test_classify_cluster_fat12_eof() {
        assert_eq!(classify_cluster(FatKind::Fat12, 0x0FF8, 0x0FEF), ClusterState::Eof);
        assert_eq!(classify_cluster(FatKind::Fat12, 0x0FFF, 0x0FEF), ClusterState::Eof);
    }

    #[test]
    fn test_classify_cluster_fat12_reserved() {
        assert_eq!(classify_cluster(FatKind::Fat12, 0x001, 0x0FEF), ClusterState::Reserved);
    }

    #[test]
    fn test_classify_cluster_fat16_free() {
        assert_eq!(classify_cluster(FatKind::Fat16, 0x0000, 0xFFEF), ClusterState::Free);
    }

    #[test]
    fn test_classify_cluster_fat16_next() {
        assert_eq!(classify_cluster(FatKind::Fat16, 0x0002, 0xFFEF), ClusterState::Next(0x0002));
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFEF, 0xFFEF), ClusterState::Next(0xFFEF));
    }

    #[test]
    fn test_classify_cluster_fat16_bad() {
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFF7, 0xFFEF), ClusterState::Bad);
    }

    #[test]
    fn test_classify_cluster_fat16_eof() {
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFF8, 0xFFEF), ClusterState::Eof);
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFFF, 0xFFEF), ClusterState::Eof);
    }

    #[test]
    fn test_classify_cluster_fat16_reserved() {
        assert_eq!(classify_cluster(FatKind::Fat16, 0x0001, 0xFFEF), ClusterState::Reserved);
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFF0, 0xFFEF), ClusterState::Reserved);
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFF6, 0xFFEF), ClusterState::Reserved);
    }

    #[test]
    fn test_classify_cluster_fat16_invalid() {
        // Value > max_cluster
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFF8, 0xFFEF), ClusterState::Eof);
        assert_eq!(classify_cluster(FatKind::Fat16, 0xFFFF, 0xFFEF), ClusterState::Eof);
    }

    #[test]
    fn test_classify_cluster_fat32_free() {
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0000_0000, 0x0FFF_FFEF), ClusterState::Free);
    }

    #[test]
    fn test_classify_cluster_fat32_next() {
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0000_0002, 0x0FFF_FFEF), ClusterState::Next(0x0000_0002));
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0FFF_FFEF, 0x0FFF_FFEF), ClusterState::Next(0x0FFF_FFEF));
    }

    #[test]
    fn test_classify_cluster_fat32_bad() {
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0FFF_FFF7, 0x0FFF_FFEF), ClusterState::Bad);
    }

    #[test]
    fn test_classify_cluster_fat32_eof() {
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0FFF_FFF8, 0x0FFF_FFEF), ClusterState::Eof);
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0FFF_FFFF, 0x0FFF_FFEF), ClusterState::Eof);
    }

    #[test]
    fn test_classify_cluster_fat32_reserved() {
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0000_0001, 0x0FFF_FFEF), ClusterState::Reserved);
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0FFF_FFF0, 0x0FFF_FFEF), ClusterState::Reserved);
        assert_eq!(classify_cluster(FatKind::Fat32, 0x0FFF_FFF6, 0x0FFF_FFEF), ClusterState::Reserved);
    }

    #[test]
    fn test_lfn_checksum_known_vector() {
        let checksum = lfn_checksum(b"TEST    TXT");
        assert_eq!(checksum, 0x8F);
    }

    #[test]
    fn test_lfn_checksum_deterministic() {
        let a = lfn_checksum(b"TEST    TXT");
        let b = lfn_checksum(b"TEST    TXT");
        assert_eq!(a, b);
    }

    #[test]
    fn test_lfn_checksum_different_names() {
        let a = lfn_checksum(b"TEST    TXT");
        let b = lfn_checksum(b"README  TXT");
        assert_ne!(a, b);
    }

    #[test]
    fn test_lfn_checksum_high_bytes_no_panic() {
        lfn_checksum(&[0xFF; 11]);
    }

}

fn main() {
    let (repair_mode, device) = parse_args();
    let mut result = CheckResult::new();

    let mut disk = FileDisk::open(&device).unwrap_or_else(|e| {
        eprintln!("fat-check: failed to open {device}: {e}");
        process::exit(1);
    });

    let boot_sector = read_boot_sector(&mut disk).unwrap_or_else(|e| {
        eprintln!("fat-check: cannot read boot sector from {device}: {e}");
        process::exit(1);
    });

    let Some(info) = check_bpb(&mut result, &boot_sector) else {
        exit_with_result(&result, false);
    };

    let fat = read_fat(&mut disk, info).unwrap_or_else(|e| {
        eprintln!("fat-check: cannot read FAT from {device}: {e}");
        exit_with_result(&result, false);
    });
    let initial_scan = scan_filesystem(&mut disk, info, &fat).unwrap_or_else(|e| {
        eprintln!("fat-check: cannot scan {device}: {e}");
        exit_with_result(&result, false);
    });

    let mut repairs_made = false;

    if repair_mode {
        report_repair_findings(&mut result, info, &initial_scan);

        let mut repair_stats = RepairStats {
            dirty_flag_cleared: false,
            fsinfo_corrected: false,
            lost_clusters_reclaimed: 0,
            orphaned_lfn_entries_removed: 0,
        };
        let mut fat_copy = fat.clone();
        let mut fat_changed = false;

        fat_changed |= repair_dirty_flags(&mut repair_stats, info, &mut fat_copy, &initial_scan);
        fat_changed |= repair_lost_clusters(&mut repair_stats, info, &mut fat_copy, &initial_scan);

        if fat_changed {
            write_fat_copies(&mut disk, info, &fat_copy).unwrap_or_else(|e| {
                eprintln!("fat-check: cannot write FAT repairs to {device}: {e}");
                exit_with_result(&result, false);
            });
        }

        repair_orphaned_lfns(&mut repair_stats, &mut disk, &initial_scan).unwrap_or_else(|e| {
            eprintln!("fat-check: cannot repair orphaned LFN entries on {device}: {e}");
            exit_with_result(&result, false);
        });

        let repaired_fat = read_fat(&mut disk, info).unwrap_or_else(|e| {
            eprintln!("fat-check: cannot reread FAT from {device}: {e}");
            exit_with_result(&result, false);
        });
        let repaired_scan = scan_filesystem(&mut disk, info, &repaired_fat).unwrap_or_else(|e| {
            eprintln!("fat-check: cannot rescan {device} after repair: {e}");
            exit_with_result(&result, false);
        });

        repair_fsinfo(&mut repair_stats, &mut disk, info, &repaired_scan).unwrap_or_else(|e| {
            eprintln!("fat-check: cannot update FSInfo on {device}: {e}");
            exit_with_result(&result, false);
        });
        result.info(repair_stats.summary());

        repairs_made = repair_stats.dirty_flag_cleared
            || repair_stats.fsinfo_corrected
            || repair_stats.lost_clusters_reclaimed > 0
            || repair_stats.orphaned_lfn_entries_removed > 0;

        let final_fat = read_fat(&mut disk, info).unwrap_or_else(|e| {
            eprintln!("fat-check: cannot reread final FAT from {device}: {e}");
            exit_with_result(&result, repairs_made);
        });
        let final_scan = scan_filesystem(&mut disk, info, &final_fat).unwrap_or_else(|e| {
            eprintln!("fat-check: cannot run final scan for {device}: {e}");
            exit_with_result(&result, repairs_made);
        });

        check_dirty_flags(&mut result, &final_scan);
        check_fsinfo(&mut result, info, &final_scan);
        check_lost_clusters(&mut result, &final_scan);
        check_orphaned_lfns(&mut result, &final_scan);
    } else {
        check_dirty_flags(&mut result, &initial_scan);
        check_fsinfo(&mut result, info, &initial_scan);
        check_lost_clusters(&mut result, &initial_scan);
        check_orphaned_lfns(&mut result, &initial_scan);
    }

    drop(disk);
    let disk = FileDisk::open(&device).unwrap_or_else(|e| {
        eprintln!("fat-check: failed to reopen {device}: {e}");
        exit_with_result(&result, repairs_made);
    });
    let buf_disk = fscommon::BufStream::new(disk);

    let fs = fatfs::FileSystem::new(buf_disk, FsOptions::new()).unwrap_or_else(|e| {
        eprintln!("fat-check: failed to mount {device}: {e}");
        exit_with_result(&result, repairs_made);
    });

    result.info(format!("mounted successfully: {device}"));

    let stats = match fs.stats() {
        Ok(s) => s,
        Err(e) => {
            result.warn(format!("cannot read filesystem stats: {e}"));
            exit_with_result(&result, repairs_made);
        }
    };
    result.info(format!(
        "clusters: {} total, {} free, {} used, cluster size: {} bytes",
        stats.total_clusters(),
        stats.free_clusters(),
        stats.total_clusters() - stats.free_clusters(),
        stats.cluster_size(),
    ));

    check_directory_tree(&mut result, &fs);
    exit_with_result(&result, repairs_made);
}
