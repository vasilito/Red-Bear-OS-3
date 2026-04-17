use std::env;
use std::fs::OpenOptions;
use std::process;

use fat_blockdev::FileDisk;
use fatfs::{FatType, FormatVolumeOptions};

fn usage() -> ! {
    eprintln!("Usage: fat-mkfs [options] <device>");
    eprintln!("Options:");
    eprintln!("  -F <12|16|32>  FAT type (default: auto)");
    eprintln!("  -n <label>     Volume label (max 11 chars)");
    eprintln!("  -s <size>      File size in bytes (for file-backed images)");
    eprintln!("  -c <sectors>   Sectors per cluster (must be power of 2)");
    process::exit(1)
}

fn parse_fat_type(s: &str) -> Option<FatType> {
    match s {
        "12" => Some(FatType::Fat12),
        "16" => Some(FatType::Fat16),
        "32" => Some(FatType::Fat32),
        _ => None,
    }
}

fn main() {
    let mut args = env::args().skip(1).peekable();
    let mut fat_type: Option<FatType> = None;
    let mut label: Option<String> = None;
    let mut file_size: Option<u64> = None;
    let mut cluster_sectors: Option<u8> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-F" => {
                let val = args.next().unwrap_or_else(|| {
                    eprintln!("fat-mkfs: -F requires an argument (12, 16, or 32)");
                    process::exit(1);
                });
                fat_type = Some(parse_fat_type(&val).unwrap_or_else(|| {
                    eprintln!("fat-mkfs: invalid FAT type '{val}', use 12, 16, or 32");
                    process::exit(1);
                }));
            }
            "-n" => {
                let val = args.next().unwrap_or_else(|| {
                    eprintln!("fat-mkfs: -n requires a label argument");
                    process::exit(1);
                });
                if val.len() > 11 {
                    eprintln!("fat-mkfs: volume label too long (max 11 chars)");
                    process::exit(1);
                }
                label = Some(val.to_uppercase());
            }
            "-s" => {
                let val = args.next().unwrap_or_else(|| {
                    eprintln!("fat-mkfs: -s requires a size argument");
                    process::exit(1);
                });
                file_size = Some(val.parse::<u64>().unwrap_or_else(|_| {
                    eprintln!("fat-mkfs: invalid size '{val}'");
                    process::exit(1);
                }));
            }
            "-c" => {
                let val = args.next().unwrap_or_else(|| {
                    eprintln!("fat-mkfs: -c requires a sectors argument");
                    process::exit(1);
                });
                let sc = val.parse::<u8>().unwrap_or_else(|_| {
                    eprintln!("fat-mkfs: invalid sectors '{val}'");
                    process::exit(1);
                });
                if sc == 0 || (sc & (sc - 1)) != 0 {
                    eprintln!("fat-mkfs: sectors per cluster must be a power of 2 and greater than 0");
                    process::exit(1);
                }
                cluster_sectors = Some(sc);
            }
            other if other.starts_with('-') => {
                eprintln!("fat-mkfs: unknown option '{other}'");
                usage();
            }
            _ => {
                let path = arg;
                if let Some(size) = file_size {
                    let file = std::fs::File::create(&path)
                        .unwrap_or_else(|e| {
                            eprintln!("fat-mkfs: failed to create {path}: {e}");
                            process::exit(1);
                        });
                    // Pre-zero to avoid sparse file issues on some hosts
                    use std::io::{BufWriter, Write};
                    let mut writer = BufWriter::new(file);
                    let zeros = [0u8; 65536];
                    let mut remaining = size as usize;
                    while remaining > 0 {
                        let chunk_size = remaining.min(zeros.len());
                        writer.write_all(&zeros[..chunk_size]).unwrap_or_else(|e| {
                            eprintln!("fat-mkfs: failed to zero-fill {path}: {e}");
                            process::exit(1);
                        });
                        remaining -= chunk_size;
                    }
                    drop(writer);
                }

                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&path)
                    .unwrap_or_else(|e| {
                        eprintln!("fat-mkfs: failed to open {path}: {e}");
                        process::exit(1);
                    });

                let mut options = FormatVolumeOptions::new();
                if let Some(ft) = fat_type {
                    options = options.fat_type(ft);
                }
                if let Some(ref lbl) = label {
                    let mut label_bytes = [b' '; 11];
                    for (i, b) in lbl.bytes().take(11).enumerate() {
                        label_bytes[i] = b;
                    }
                    options = options.volume_label(label_bytes);
                }
                if let Some(sc) = cluster_sectors {
                    options = options.bytes_per_cluster(u32::from(sc) * 512);
                }

                let mut disk = FileDisk::new(file);
                fatfs::format_volume(&mut disk, options)
                    .unwrap_or_else(|e| {
                        eprintln!("fat-mkfs: format failed: {e}");
                        process::exit(1);
                    });

                let type_str = match fat_type {
                    Some(FatType::Fat12) => "FAT12",
                    Some(FatType::Fat16) => "FAT16",
                    Some(FatType::Fat32) => "FAT32",
                    None => "auto-detected",
                };
                let cluster_str = if let Some(sc) = cluster_sectors {
                    format!(", cluster {} sectors", sc)
                } else {
                    String::new()
                };
                eprintln!("fat-mkfs: formatted {path} as {type_str}{cluster_str}");
                return;
            }
        }
    }
    usage();
}
