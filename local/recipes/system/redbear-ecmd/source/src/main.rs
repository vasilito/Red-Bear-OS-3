use log::{info, warn, LevelFilter};
use std::fs;
use std::time::Duration;

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool { m.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] redbear-ecmd: {}", r.level(), r.args()); }
    fn flush(&self) {}
}

fn scan_and_create() -> usize {
    let mut n = 0;
    let _ = fs::create_dir_all("/dev/net");
    if let Ok(dir) = fs::read_dir("/scheme/usb") {
        for entry in dir.flatten() {
            if let Ok(config) = fs::read_to_string(entry.path().join("config")) {
                if config.contains("class=02") && (config.contains("subclass=06") || config.contains("subclass=0d")) {
                    let dev = format!("/dev/net/usb{}", n);
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
    info!("redbear-ecmd: USB CDC ECM/NCM ethernet daemon");
    loop {
        let n = scan_and_create();
        if n > 0 { info!("redbear-ecmd: {} usb net device(s)", n); }
        std::thread::sleep(Duration::from_secs(5));
    }
}
