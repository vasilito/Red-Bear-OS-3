mod naming;
mod device_db;
mod scheme;

use std::env;
use std::os::fd::RawFd;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use log::{error, info, warn, LevelFilter, Metadata, Record};
use redox_scheme::{
    scheme::{SchemeState, SchemeSync},
    SignalBehavior, Socket,
};

use naming::write_default_rules_file;
use scheme::UdevScheme;

struct StderrLogger {
    level: LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

fn init_logging(level: LevelFilter) {
    if log::set_boxed_logger(Box::new(StderrLogger { level })).is_err() {
        return;
    }
    log::set_max_level(level);
}

unsafe fn get_init_notify_fd() -> RawFd {
    let fd: RawFd = env::var("INIT_NOTIFY")
        .expect("udev-shim: INIT_NOTIFY not set")
        .parse()
        .expect("udev-shim: INIT_NOTIFY is not a valid fd");
    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    fd
}

fn notify_scheme_ready(notify_fd: RawFd, socket: &Socket, scheme: &mut UdevScheme) {
    let cap_id = scheme.scheme_root().expect("udev-shim: scheme_root failed");
    let cap_fd = socket
        .create_this_scheme_fd(0, cap_id, 0, 0)
        .expect("udev-shim: create_this_scheme_fd failed");

    syscall::call_wo(
        notify_fd as usize,
        &libredox::Fd::new(cap_fd).into_raw().to_ne_bytes(),
        syscall::CallFlags::FD,
        &[],
    )
    .expect("udev-shim: failed to notify init that scheme is ready");
}

fn main() {
    let log_level = match env::var("UDEV_SHIM_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };

    init_logging(log_level);

    let mut scheme = UdevScheme::new();

    match scheme.scan_pci_devices() {
        Ok(n) => info!("udev-shim: enumerated {} PCI device(s)", n),
        Err(e) => error!("udev-shim: PCI scan failed: {}", e),
    }

    match write_default_rules_file() {
        Ok(path) => info!("udev-shim: wrote default rules to {path}"),
        Err(err) => warn!("udev-shim: failed to write default rules: {err}"),
    }

    let notify_fd = unsafe { get_init_notify_fd() };
    let socket = Socket::create().expect("udev-shim: failed to create udev scheme");
    let mut state = SchemeState::new();

    notify_scheme_ready(notify_fd, &socket, &mut scheme);

    libredox::call::setrens(0, 0).expect("udev-shim: failed to enter null namespace");

    info!("udev-shim: registered scheme:udev");

    // Hotplug polling: periodically check driver-manager for device changes
    let scheme = Arc::new(Mutex::new(scheme));
    let scheme_clone = Arc::clone(&scheme);
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(2));
            if let Ok(mut s) = scheme_clone.lock() {
                match s.scan_pci_devices() {
                    Ok(n) if n > 0 => info!("udev-shim: hotplug detected {} device(s)", n),
                    Err(e) => error!("udev-shim: hotplug scan failed: {}", e),
                    _ => {}
                }
            }
        }
    });

    while let Some(request) = socket
        .next_request(SignalBehavior::Restart)
        .expect("udev-shim: failed to read scheme request")
    {
        match request.kind() {
            redox_scheme::RequestKind::Call(request) => {
                let response = {
                    let mut guard = match scheme.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => {
                            error!("udev-shim: recovering from poisoned scheme lock");
                            poisoned.into_inner()
                        }
                    };

                    request.handle_sync(&mut *guard, &mut state)
                };
                socket
                    .write_response(response, SignalBehavior::Restart)
                    .expect("udev-shim: failed to write response");
            }
            _ => (),
        }
    }

    std::process::exit(0);
}
