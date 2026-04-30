use log::{info, warn, LevelFilter};
use std::fs;
use std::time::Duration;

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool { m.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] redbear-usbaudiod: {}", r.level(), r.args()); }
    fn flush(&self) {}
}

fn scan_and_create() -> usize {
    let mut n = 0;
    let _ = fs::create_dir_all("/dev/audio");
    if let Ok(dir) = fs::read_dir("/scheme/usb") {
        for entry in dir.flatten() {
            if let Ok(config) = fs::read_to_string(entry.path().join("config")) {
                if config.contains("class=01") {
                    let dev = format!("/dev/audio/usb{}", n);
                    let _ = fs::write(&dev, &[]);
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
    loop {
        let n = scan_and_create();
        if n > 0 { info!("redbear-usbaudiod: {} usb audio device(s)", n); }
        std::thread::sleep(Duration::from_secs(5));
    }
}
