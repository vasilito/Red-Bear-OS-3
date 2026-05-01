mod registers;

use std::env;
use std::process;
use std::fs;
use log::{info, error, warn, LevelFilter};
use registers::*;

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, md: &log::Metadata) -> bool { md.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] ohcid: {}", r.level(), r.args()); }
    fn flush(&self) {}
}

fn main() {
    log::set_logger(&StderrLogger).ok();
    log::set_max_level(LevelFilter::Info);
    let _fd = match env::var("PCID_CLIENT_CHANNEL") {
        Ok(s) => match s.parse::<usize>() { Ok(fd) => fd, Err(_) => { error!("invalid PCID_CLIENT_CHANNEL"); process::exit(1); } },
        Err(_) => { error!("PCID_CLIENT_CHANNEL not set"); process::exit(1); }
    };
    let device_path = env::var("PCID_DEVICE_PATH").unwrap_or_default();
    info!("OHCI USB 1.1 at {}", device_path);
    let config_path = format!("{}/config", device_path);
    match fs::read(&config_path) {
        Ok(data) if data.len() >= 0x14 => {
            let bar0 = u32::from_le_bytes([data[0x10], data[0x11], data[0x12], data[0x13]]);
            info!("OHCI MMIO base: 0x{:08X} (BAR0)", bar0 & 0xFFFFFFF0);
            info!("ohcid: MMIO detected, ready for port enumeration");
        }
        _ => warn!("cannot read PCI config"),
    }
    loop { std::thread::sleep(std::time::Duration::from_secs(10)); }
}
