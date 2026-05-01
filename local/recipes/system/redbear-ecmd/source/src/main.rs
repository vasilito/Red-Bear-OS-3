use log::{info, LevelFilter};
use std::fs;
use std::time::Duration;
struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool { m.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] redbear-ecmd: {}", r.level(), r.args()); }
    fn flush(&self) {}
}
fn scan() -> usize {
    let mut n = 0;
    let _ = fs::create_dir_all("/dev/net");
    if let Ok(dir) = fs::read_dir("/scheme/usb") {
        for e in dir.flatten() {
            if let Ok(c) = fs::read_to_string(e.path().join("config")) {
                if c.contains("class=02") && (c.contains("subclass=06") || c.contains("subclass=0d")) {
                    let tgt = e.path();
                    let lnk = format!("/dev/net/usb{}", n);
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
    info!("redbear-ecmd: USB CDC ECM/NCM ethernet daemon");
    loop { let c = scan(); if c > 0 { info!("redbear-ecmd: {} usb net symlink(s)", c); } std::thread::sleep(Duration::from_secs(5)); }
}
