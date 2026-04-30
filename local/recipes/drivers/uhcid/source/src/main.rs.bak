mod registers;

use std::env;
use std::process;
use log::{info, error, LevelFilter};

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, md: &log::Metadata) -> bool { md.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] uhcid: {}", r.level(), r.args()); }
    fn flush(&self) {}
}

fn main() {
    log::set_logger(&StderrLogger).ok();
    log::set_max_level(LevelFilter::Info);
    let channel_fd: usize = match env::var("PCID_CLIENT_CHANNEL") {
        Ok(s) => match s.parse() { Ok(fd) => fd, Err(_) => { error!("invalid PCID_CLIENT_CHANNEL"); process::exit(1); } },
        Err(_) => { error!("PCID_CLIENT_CHANNEL not set"); process::exit(1); }
    };
    info!("UHCI USB 1.1 controller (PCI fd: {})", channel_fd);
    info!("uhcid: ready");
}
