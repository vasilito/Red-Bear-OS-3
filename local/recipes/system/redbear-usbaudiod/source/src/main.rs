use log::{info, LevelFilter};
use std::fs;
use std::time::Duration;
struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool { m.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] redbear-usbaudiod: {}", r.level(), r.args()); }
    fn flush(&self) {}
}
fn scan() -> usize {
    let mut n = 0;
    let _ = fs::create_dir_all("/dev/audio");
    if let Ok(dir) = fs::read_dir("/scheme/usb") {
        for e in dir.flatten() {
            if let Ok(c) = fs::read_to_string(e.path().join("config")) {
                if c.contains("class=01") {
                    let tgt = e.path();
                    let lnk = format!("/dev/audio/usb{}", n);
                    let _ = std::os::unix::fs::symlink(&tgt, &lnk);
                    n += 1;
                }
            }
        }
    }
    n
}
fn main() {
    log::set_logger(&StderrLogger).ok();
    log::set_max_level(LevelFilter::Info);
    info!("redbear-usbaudiod: USB Audio Class daemon");
    loop { let c = scan(); if c > 0 { info!("redbear-usbaudiod: {} usb audio symlink(s)", c); } std::thread::sleep(Duration::from_secs(5)); }
}
