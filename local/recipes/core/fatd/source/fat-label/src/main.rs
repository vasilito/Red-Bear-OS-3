use std::env;
use std::io::{Read, Seek, SeekFrom, Write};
use std::process;

use fat_blockdev::FileDisk;
use fatfs::FsOptions;

fn usage() -> ! {
    eprintln!("Usage: fat-label [-s <label>|--set <label>] <device>");
    process::exit(1)
}

fn printable_label(label: &str) -> String {
    if label.is_empty() {
        "(no label)".to_string()
    } else {
        label.to_string()
    }
}

fn invalid_volume_label_char(byte: u8) -> bool {
    matches!(
        byte,
        0x00..=0x1F
            | 0x7F
            | b'"'
            | b'*'
            | b'+'
            | b','
            | b'.'
            | b'/'
            | b':'
            | b';'
            | b'<'
            | b'='
            | b'>'
            | b'?'
            | b'['
            | b'\\'
            | b']'
            | b'|'
    )
}

fn normalize_label(label: &str) -> [u8; 11] {
    if !label.is_ascii() {
        eprintln!("fat-label: label must contain only ASCII characters");
        process::exit(1);
    }

    if label.len() > 11 {
        eprintln!("fat-label: label too long (max 11 chars)");
        process::exit(1);
    }

    let label = label.to_ascii_uppercase();
    if let Some(invalid) = label.bytes().find(|byte| invalid_volume_label_char(*byte)) {
        eprintln!("fat-label: invalid character '{}' in label", invalid as char);
        process::exit(1);
    }

    let mut bytes = [b' '; 11];
    for (index, byte) in label.bytes().enumerate() {
        bytes[index] = byte;
    }
    bytes
}

fn label_string(bytes: &[u8; 11]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(' ')
        .to_string()
}

fn read_volume_label(device: &str) -> String {
    let disk = FileDisk::open(device).unwrap_or_else(|e| {
        eprintln!("fat-label: failed to open {device}: {e}");
        process::exit(1);
    });

    let buf_disk = fscommon::BufStream::new(disk);
    let fs = fatfs::FileSystem::new(buf_disk, FsOptions::new()).unwrap_or_else(|e| {
        eprintln!("fat-label: failed to mount {device}: {e}");
        process::exit(1);
    });

    fs.volume_label()
}

fn write_volume_label(device: &str, label: [u8; 11]) {
    let mut disk = FileDisk::open(device).unwrap_or_else(|e| {
        eprintln!("fat-label: failed to open {device}: {e}");
        process::exit(1);
    });

    let mut boot_sector = [0u8; 512];
    disk.seek(SeekFrom::Start(0))
        .and_then(|_| disk.read_exact(&mut boot_sector))
        .unwrap_or_else(|e| {
            eprintln!("fat-label: failed to read BPB from {device}: {e}");
            process::exit(1);
        });

    if boot_sector[510] != 0x55 || boot_sector[511] != 0xAA {
        eprintln!(
            "fat-label: invalid boot sector signature {:02X} {:02X}",
            boot_sector[510], boot_sector[511]
        );
        process::exit(1);
    }

    let root_entry_count = u16::from_le_bytes([boot_sector[17], boot_sector[18]]);
    let fat_size_32 = u32::from_le_bytes([
        boot_sector[36],
        boot_sector[37],
        boot_sector[38],
        boot_sector[39],
    ]);
    let label_offset = if root_entry_count == 0 && fat_size_32 != 0 {
        71
    } else {
        43
    };

    disk.seek(SeekFrom::Start(label_offset))
        .and_then(|_| disk.write_all(&label))
        .and_then(|_| disk.flush())
        .unwrap_or_else(|e| {
            eprintln!("fat-label: failed to write BPB volume label to {device}: {e}");
            process::exit(1);
        });

    drop(disk);

    update_root_dir_label(device, label, &boot_sector);
}

fn update_root_dir_label(device: &str, label: [u8; 11], boot_sector: &[u8; 512]) {
    let root_entry_count = u16::from_le_bytes([boot_sector[17], boot_sector[18]]) as u32;
    let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]) as u32;
    let sectors_per_cluster = boot_sector[13] as u32;
    let reserved_sectors = u16::from_le_bytes([boot_sector[14], boot_sector[15]]) as u32;
    let num_fats = boot_sector[16] as u32;
    let fat_size_16 = u16::from_le_bytes([boot_sector[22], boot_sector[23]]) as u32;
    let fat_size_32 = u32::from_le_bytes([boot_sector[36], boot_sector[37], boot_sector[38], boot_sector[39]]);
    let fat_size = if fat_size_32 != 0 && root_entry_count == 0 { fat_size_32 } else { fat_size_16 };
    let root_dir_sectors = (root_entry_count * 32).div_ceil(bytes_per_sector);
    let first_root_dir_sector = reserved_sectors + num_fats * fat_size;
    let root_cluster = u32::from_le_bytes([boot_sector[44], boot_sector[45], boot_sector[46], boot_sector[47]]);

    let mut disk = FileDisk::open(device).unwrap_or_else(|e| {
        eprintln!("fat-label: failed to reopen {device} for root dir update: {e}");
        process::exit(1);
    });

    let is_fat32 = root_entry_count == 0 && fat_size_32 != 0;

    let mut new_entry = [0u8; 32];
    new_entry[0..11].copy_from_slice(&label);
    new_entry[11] = 0x08;

    if is_fat32 {
        let first_data_sector = first_root_dir_sector + root_dir_sectors;
        let cluster_offset = |cluster: u32| -> u64 {
            let first_sector = first_data_sector + (cluster - 2) * sectors_per_cluster;
            u64::from(first_sector) * u64::from(bytes_per_sector)
        };

        let mut found = false;
        let mut first_free_offset: Option<u64> = None;
        let mut cluster = root_cluster;
        let cluster_size = (bytes_per_sector * sectors_per_cluster) as usize;
        loop {
            let offset = cluster_offset(cluster);
            let mut buf = vec![0u8; cluster_size];
            disk.seek(SeekFrom::Start(offset))
                .and_then(|_| disk.read_exact(&mut buf))
                .unwrap_or_else(|e| {
                    eprintln!("fat-label: failed to read root dir cluster: {e}");
                    process::exit(1);
                });

            for (i, chunk) in buf.chunks_exact(32).enumerate() {
                if chunk[0] == 0x00 {
                    if first_free_offset.is_none() {
                        first_free_offset = Some(offset + (i as u64 * 32));
                    }
                    break;
                }
                if chunk[0] == 0xE5 {
                    if first_free_offset.is_none() {
                        first_free_offset = Some(offset + (i as u64 * 32));
                    }
                    continue;
                }
                if chunk[11] == 0x08 {
                    let entry_offset = offset + (i as u64 * 32);
                    disk.seek(SeekFrom::Start(entry_offset))
                        .and_then(|_| disk.write_all(&label))
                        .and_then(|_| disk.flush())
                        .unwrap_or_else(|e| {
                            eprintln!("fat-label: failed to write root dir label entry: {e}");
                            process::exit(1);
                        });
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }

            let fat_offset = cluster as usize * 4;
            let fat_byte_offset = reserved_sectors as u64 * bytes_per_sector as u64;
            let mut fat_entry = [0u8; 4];
            disk.seek(SeekFrom::Start(fat_byte_offset + fat_offset as u64))
                .and_then(|_| disk.read_exact(&mut fat_entry))
                .unwrap_or_else(|e| {
                    eprintln!("fat-label: failed to read FAT entry: {e}");
                    process::exit(1);
                });
            let next = u32::from_le_bytes(fat_entry) & 0x0FFF_FFFF;
            if next >= 0x0FFF_FFF8 {
                break;
            }
            cluster = next;
        }

        if !found {
            if let Some(free_offset) = first_free_offset {
                disk.seek(SeekFrom::Start(free_offset))
                    .and_then(|_| disk.write_all(&new_entry))
                    .and_then(|_| disk.flush())
                    .unwrap_or_else(|e| {
                        eprintln!("fat-label: failed to create root dir label entry: {e}");
                        process::exit(1);
                    });
            } else {
                eprintln!("fat-label: warning: root directory full, BPB label updated but no root-dir entry created");
            }
        }
    } else {
        let root_dir_offset = u64::from(first_root_dir_sector) * u64::from(bytes_per_sector);
        let root_dir_size = (root_entry_count * 32) as usize;
        let mut buf = vec![0u8; root_dir_size];
        disk.seek(SeekFrom::Start(root_dir_offset))
            .and_then(|_| disk.read_exact(&mut buf))
            .unwrap_or_else(|e| {
                eprintln!("fat-label: failed to read root dir: {e}");
                process::exit(1);
            });

        let mut found = false;
        let mut first_free_offset: Option<u64> = None;

        for (i, chunk) in buf.chunks_exact(32).enumerate() {
            if chunk[0] == 0x00 {
                if first_free_offset.is_none() {
                    first_free_offset = Some(root_dir_offset + (i as u64 * 32));
                }
                break;
            }
            if chunk[0] == 0xE5 {
                if first_free_offset.is_none() {
                    first_free_offset = Some(root_dir_offset + (i as u64 * 32));
                }
                continue;
            }
            if chunk[11] == 0x08 {
                let entry_offset = root_dir_offset + (i as u64 * 32);
                disk.seek(SeekFrom::Start(entry_offset))
                    .and_then(|_| disk.write_all(&label))
                    .and_then(|_| disk.flush())
                    .unwrap_or_else(|e| {
                        eprintln!("fat-label: failed to write root dir label entry: {e}");
                        process::exit(1);
                    });
                found = true;
                break;
            }
        }

        if !found {
            if let Some(free_offset) = first_free_offset {
                disk.seek(SeekFrom::Start(free_offset))
                    .and_then(|_| disk.write_all(&new_entry))
                    .and_then(|_| disk.flush())
                    .unwrap_or_else(|e| {
                        eprintln!("fat-label: failed to create root dir label entry: {e}");
                        process::exit(1);
                    });
            } else {
                eprintln!("fat-label: warning: root directory full, BPB label updated but no root-dir entry created");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test invalid_volume_label_char
    #[test]
    fn test_invalid_volume_label_char_valid_range() {
        // A-Z, 0-9 are valid
        assert!(!invalid_volume_label_char(b'A'));
        assert!(!invalid_volume_label_char(b'Z'));
        assert!(!invalid_volume_label_char(b'0'));
        assert!(!invalid_volume_label_char(b'9'));
    }

    #[test]
    fn test_invalid_volume_label_char_invalid_chars() {
        // Control chars 0x00-0x1F
        assert!(invalid_volume_label_char(0x00));
        assert!(invalid_volume_label_char(0x1F));
        assert!(invalid_volume_label_char(0x7F));
        // Invalid symbols
        assert!(invalid_volume_label_char(b'"'));
        assert!(invalid_volume_label_char(b'*'));
        assert!(invalid_volume_label_char(b'+'));
        assert!(invalid_volume_label_char(b','));
        assert!(invalid_volume_label_char(b'.'));
        assert!(invalid_volume_label_char(b'/'));
        assert!(invalid_volume_label_char(b':'));
        assert!(invalid_volume_label_char(b';'));
        assert!(invalid_volume_label_char(b'<'));
        assert!(invalid_volume_label_char(b'='));
        assert!(invalid_volume_label_char(b'>'));
        assert!(invalid_volume_label_char(b'?'));
        assert!(invalid_volume_label_char(b'['));
        assert!(invalid_volume_label_char(b'\\'));
        assert!(invalid_volume_label_char(b']'));
        assert!(invalid_volume_label_char(b'|'));
    }

    // Test label_string
    #[test]
    fn test_label_string_trims_trailing_spaces() {
        let bytes = *b"TEST       ";
        let result = label_string(&bytes);
        assert_eq!(result, "TEST");
    }

    #[test]
    fn test_label_string_no_trailing_spaces() {
        let bytes = *b"NOTRIMMED  ";
        let result = label_string(&bytes);
        assert_eq!(result, "NOTRIMMED");
    }

    #[test]
    fn test_label_string_all_spaces() {
        let bytes = *b"           ";
        let result = label_string(&bytes);
        assert_eq!(result, "");
    }

    // Test printable_label
    #[test]
    fn test_printable_label_empty() {
        let result = printable_label("");
        assert_eq!(result, "(no label)");
    }

    #[test]
    fn test_printable_label_non_empty() {
        let result = printable_label("MYDISK");
        assert_eq!(result, "MYDISK");
    }
}

fn main() {
    let mut args = env::args().skip(1);
    let mut new_label = None;
    let mut device = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-s" | "--set" => {
                if new_label.is_some() {
                    eprintln!("fat-label: volume label already specified");
                    usage();
                }
                new_label = Some(args.next().unwrap_or_else(|| {
                    eprintln!("fat-label: {} requires a label argument", arg);
                    process::exit(1);
                }));
            }
            _ if arg.starts_with('-') => {
                eprintln!("fat-label: unknown option '{arg}'");
                usage();
            }
            _ => {
                if device.is_some() {
                    eprintln!("fat-label: unexpected extra argument '{arg}'");
                    usage();
                }
                device = Some(arg);
            }
        }
    }

    let device = device.unwrap_or_else(|| usage());

    match new_label {
        Some(label) => {
            let old_label = read_volume_label(&device);
            let label_bytes = normalize_label(&label);
            write_volume_label(&device, label_bytes);

            let new_label = read_volume_label(&device);
            let expected_label = label_string(&label_bytes);
            if new_label != expected_label {
                eprintln!(
                    "fat-label: verification failed: expected '{}', got '{}'",
                    printable_label(&expected_label),
                    printable_label(&new_label)
                );
                process::exit(1);
            }

            println!("old label: {}", printable_label(&old_label));
            println!("new label: {}", printable_label(&new_label));
        }
        None => {
            let label = read_volume_label(&device);
            if label.is_empty() {
                println!("(no label)");
            } else {
                println!("{label}");
            }
        }
    }
}
