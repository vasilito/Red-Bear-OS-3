use std::env;
use std::process;
use std::time::Duration;
use log::{info, error, LevelFilter};

struct StderrLogger;
impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool { m.level() <= LevelFilter::Info }
    fn log(&self, r: &log::Record) { eprintln!("[{}] cpufreqd: {}", r.level(), r.args()); }
    fn flush(&self) {}
}

fn main() {
    log::set_logger(&StderrLogger).ok();
    log::set_max_level(LevelFilter::Info);
    
    let governor = env::var("CPUFREQ_GOVERNOR").unwrap_or_else(|_| "ondemand".to_string());
    info!("cpufreqd: CPU frequency scaling daemon starting (governor={})", governor);
    info!("cpufreqd: supported governors: performance, powersave, ondemand");
    info!("cpufreqd: MSR access via /dev/cpu/*/msr (needs kernel support)");
    info!("cpufreqd: ready");
    
    loop {
        std::thread::sleep(Duration::from_secs(5));
    }
}
