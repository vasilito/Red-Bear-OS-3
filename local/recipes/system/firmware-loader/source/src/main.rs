mod blob;
mod scheme;

use std::env;
#[cfg(target_os = "redox")]
use std::os::fd::RawFd;
use std::path::PathBuf;
use std::process;

use log::{error, info, LevelFilter, Metadata, Record};
#[cfg(target_os = "redox")]
use redox_scheme::{scheme::SchemeSync, SignalBehavior, Socket};

use blob::FirmwareRegistry;
#[cfg(target_os = "redox")]
use scheme::FirmwareScheme;

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

fn default_firmware_dir() -> PathBuf {
    PathBuf::from("/lib/firmware/")
}

#[cfg(target_os = "redox")]
unsafe fn get_init_notify_fd() -> RawFd {
    let fd: RawFd = env::var("INIT_NOTIFY")
        .expect("firmware-loader: INIT_NOTIFY not set")
        .parse()
        .expect("firmware-loader: INIT_NOTIFY is not a valid fd");
    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    fd
}

#[cfg(target_os = "redox")]
fn notify_scheme_ready(notify_fd: RawFd, socket: &Socket, scheme: &mut FirmwareScheme) {
    let cap_id = scheme
        .scheme_root()
        .expect("firmware-loader: scheme_root failed");
    let cap_fd = socket
        .create_this_scheme_fd(0, cap_id, 0, 0)
        .expect("firmware-loader: create_this_scheme_fd failed");

    syscall::call_wo(
        notify_fd as usize,
        &libredox::Fd::new(cap_fd).into_raw().to_ne_bytes(),
        syscall::CallFlags::FD,
        &[],
    )
    .expect("firmware-loader: failed to notify init that scheme is ready");
}

#[cfg(target_os = "redox")]
fn run_daemon(notify_fd: RawFd, registry: FirmwareRegistry) -> ! {
    let socket = Socket::create().expect("firmware-loader: failed to create scheme socket");
    let mut scheme = FirmwareScheme::new(registry);

    notify_scheme_ready(notify_fd, &socket, &mut scheme);

    info!("firmware-loader: registered scheme:firmware");

    libredox::call::setrens(0, 0).expect("firmware-loader: failed to enter null namespace");

    while let Some(request) = socket
        .next_request(SignalBehavior::Restart)
        .expect("firmware-loader: failed to read scheme request")
    {
        match request.kind() {
            redox_scheme::RequestKind::Call(request) => {
                let mut state = redox_scheme::scheme::SchemeState::new();
                let response = request.handle_sync(&mut scheme, &mut state);
                socket
                    .write_response(response, SignalBehavior::Restart)
                    .expect("firmware-loader: failed to write response");
            }
            _ => (),
        }
    }

    process::exit(0);
}

fn main() {
    let log_level = match env::var("FIRMWARE_LOADER_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };

    init_logging(log_level);

    let firmware_dir = env::var("FIRMWARE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_firmware_dir());

    info!(
        "firmware-loader: starting with directory {}",
        firmware_dir.display()
    );

    let registry = match FirmwareRegistry::new(&firmware_dir) {
        Ok(registry) => registry,
        Err(blob::BlobError::DirNotFound(_)) => {
            error!(
                "firmware-loader: firmware directory not found, starting with an empty registry: {}",
                firmware_dir.display()
            );
            FirmwareRegistry::empty(&firmware_dir)
        }
        Err(e) => {
            error!("firmware-loader: fatal error: failed to initialize firmware registry: {e}");
            process::exit(1);
        }
    };

    info!(
        "firmware-loader: indexed {} firmware blob(s) from {}",
        registry.len(),
        firmware_dir.display()
    );

    if env::args().nth(1).as_deref() == Some("--probe") {
        println!("count={}", registry.len());
        let mut keys = registry.list_keys();
        keys.sort_unstable();
        for key in keys.into_iter().take(16) {
            println!("firmware={key}");
        }
        return;
    }

    #[cfg(not(target_os = "redox"))]
    {
        eprintln!("firmware-loader: daemon mode is only supported on Redox; use --probe on host");
        process::exit(1);
    }

    #[cfg(target_os = "redox")]
    {
        let notify_fd = unsafe { get_init_notify_fd() };
        run_daemon(notify_fd, registry);
    }
}
