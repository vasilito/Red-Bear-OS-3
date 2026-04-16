mod backend;
mod scheme;

use std::env;
#[cfg(target_os = "redox")]
use std::os::fd::RawFd;
use std::path::Path;
use std::process;

use backend::{Backend, IntelBackend, NoDeviceBackend, StubBackend};
use log::LevelFilter;
#[cfg(target_os = "redox")]
use log::{error, info};
#[cfg(target_os = "redox")]
use redox_scheme::{scheme::SchemeSync, SignalBehavior, Socket};
#[cfg(target_os = "redox")]
use scheme::WifiCtlScheme;

fn init_logging(level: LevelFilter) {
    log::set_max_level(level);
}

#[cfg(target_os = "redox")]
unsafe fn get_init_notify_fd() -> RawFd {
    let fd: RawFd = env::var("INIT_NOTIFY")
        .expect("redbear-wifictl: INIT_NOTIFY not set")
        .parse()
        .expect("redbear-wifictl: INIT_NOTIFY is not a valid fd");
    unsafe {
        libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    fd
}

#[cfg(target_os = "redox")]
fn notify_scheme_ready(notify_fd: RawFd, socket: &Socket, scheme: &mut WifiCtlScheme) {
    let cap_id = scheme
        .scheme_root()
        .expect("redbear-wifictl: scheme_root failed");
    let cap_fd = socket
        .create_this_scheme_fd(0, cap_id, 0, 0)
        .expect("redbear-wifictl: create_this_scheme_fd failed");

    syscall::call_wo(
        notify_fd as usize,
        &libredox::Fd::new(cap_fd).into_raw().to_ne_bytes(),
        syscall::CallFlags::FD,
        &[],
    )
    .expect("redbear-wifictl: failed to notify init that scheme is ready");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendMode {
    Intel,
    NoDevice,
    Stub,
}

fn iwlwifi_command_path() -> std::path::PathBuf {
    env::var_os("REDBEAR_IWLWIFI_CMD")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/usr/lib/drivers/redbear-iwlwifi"))
}

fn select_backend_mode(
    explicit: Option<&str>,
    intel_driver_present: bool,
    intel_interfaces_present: bool,
    redox_runtime: bool,
) -> BackendMode {
    match explicit {
        Some("intel") => BackendMode::Intel,
        Some("stub") => BackendMode::Stub,
        _ if redox_runtime && intel_driver_present && intel_interfaces_present => {
            BackendMode::Intel
        }
        _ if redox_runtime && intel_driver_present => BackendMode::NoDevice,
        _ => BackendMode::Stub,
    }
}

fn build_backend() -> Box<dyn Backend> {
    let explicit = env::var("REDBEAR_WIFICTL_BACKEND").ok();
    let intel_driver_present = Path::new(&iwlwifi_command_path()).exists();
    let intel_interfaces_present = if cfg!(target_os = "redox") && intel_driver_present {
        !IntelBackend::from_env().interfaces().is_empty()
    } else {
        false
    };
    let mode = select_backend_mode(
        explicit.as_deref(),
        intel_driver_present,
        intel_interfaces_present,
        cfg!(target_os = "redox"),
    );

    match mode {
        BackendMode::Intel => Box::new(IntelBackend::from_env()),
        BackendMode::NoDevice => Box::new(NoDeviceBackend::new()),
        BackendMode::Stub => Box::new(StubBackend::from_env()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_backend_selection_wins() {
        assert_eq!(
            select_backend_mode(Some("intel"), false, false, false),
            BackendMode::Intel
        );
        assert_eq!(
            select_backend_mode(Some("stub"), true, true, true),
            BackendMode::Stub
        );
    }

    #[test]
    fn redox_runtime_prefers_intel_when_driver_present() {
        assert_eq!(
            select_backend_mode(None, true, true, true),
            BackendMode::Intel
        );
        assert_eq!(
            select_backend_mode(None, false, false, true),
            BackendMode::Stub
        );
    }

    #[test]
    fn redox_runtime_uses_no_device_backend_without_detected_intel_interfaces() {
        assert_eq!(
            select_backend_mode(None, true, false, true),
            BackendMode::NoDevice
        );
    }

    #[test]
    fn host_runtime_stays_stub_without_explicit_override() {
        assert_eq!(
            select_backend_mode(None, true, true, false),
            BackendMode::Stub
        );
        assert_eq!(
            select_backend_mode(None, false, false, false),
            BackendMode::Stub
        );
    }
}

fn main() {
    let log_level = match env::var("REDBEAR_WIFICTL_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        Ok("warn") => LevelFilter::Warn,
        Ok("error") => LevelFilter::Error,
        _ => LevelFilter::Info,
    };
    init_logging(log_level);

    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--probe") => {
            let backend = build_backend();
            println!("interfaces={}", backend.interfaces().join(","));
            println!("capabilities={}", backend.capabilities().join(","));
            return;
        }
        Some("--prepare") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            match backend.prepare(&iface) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("status={}", status.as_str());
                    println!("firmware_status={}", backend.firmware_status(&iface));
                    println!("transport_status={}", backend.transport_status(&iface));
                    println!("transport_init_status=transport_init=not-run");
                    return;
                }
                Err(err) => {
                    eprintln!("redbear-wifictl: prepare failed for {}: {}", iface, err);
                    process::exit(1);
                }
            }
        }
        Some("--status") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let backend = build_backend();
            println!("interface={}", iface);
            println!("status={}", backend.initial_status(&iface).as_str());
            println!("link_state={}", backend.initial_link_state(&iface));
            println!("firmware_status={}", backend.firmware_status(&iface));
            println!("transport_status={}", backend.transport_status(&iface));
            println!("transport_init_status=transport_init=unknown");
            println!("connect_result={}", backend.connect_result(&iface));
            return;
        }
        Some("--scan") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            match backend.scan(&iface) {
                Ok(results) => {
                    println!("interface={}", iface);
                    println!("status=scanning");
                    println!("firmware_status={}", backend.firmware_status(&iface));
                    println!("transport_status={}", backend.transport_status(&iface));
                    println!("scan_results={}", results.join(","));
                    return;
                }
                Err(err) => {
                    eprintln!("redbear-wifictl: scan failed for {}: {}", iface, err);
                    process::exit(1);
                }
            }
        }
        Some("--transport-probe") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            match backend.transport_probe(&iface) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("transport_status={}", status);
                    return;
                }
                Err(err) => {
                    eprintln!(
                        "redbear-wifictl: transport probe failed for {}: {}",
                        iface, err
                    );
                    process::exit(1);
                }
            }
        }
        Some("--init-transport") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            match backend.init_transport(&iface) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("transport_init_status={}", status);
                    println!("transport_status={}", backend.transport_status(&iface));
                    return;
                }
                Err(err) => {
                    eprintln!(
                        "redbear-wifictl: transport init failed for {}: {}",
                        iface, err
                    );
                    process::exit(1);
                }
            }
        }
        Some("--activate-nic") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            match backend.activate(&iface) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("activation_status={}", status);
                    println!("transport_status={}", backend.transport_status(&iface));
                    return;
                }
                Err(err) => {
                    eprintln!(
                        "redbear-wifictl: activate-nic failed for {}: {}",
                        iface, err
                    );
                    process::exit(1);
                }
            }
        }
        Some("--retry") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            match backend.retry(&iface) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("status={}", status.as_str());
                    println!("link_state=link=retrying");
                    return;
                }
                Err(err) => {
                    eprintln!("redbear-wifictl: retry failed for {}: {}", iface, err);
                    process::exit(1);
                }
            }
        }
        Some("--connect") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let ssid = args.next().unwrap_or_default();
            let security = args.next().unwrap_or_else(|| "open".to_string());
            let key = args.next().unwrap_or_default();
            let mut backend = build_backend();
            if let Err(err) = backend.prepare(&iface) {
                eprintln!("redbear-wifictl: prepare failed for {}: {}", iface, err);
                process::exit(1);
            }
            if let Err(err) = backend.init_transport(&iface) {
                eprintln!(
                    "redbear-wifictl: transport init failed for {}: {}",
                    iface, err
                );
                process::exit(1);
            }
            if let Err(err) = backend.activate(&iface) {
                eprintln!(
                    "redbear-wifictl: activate-nic failed for {}: {}",
                    iface, err
                );
                process::exit(1);
            }
            let state = backend::InterfaceState {
                ssid,
                security,
                key,
                ..Default::default()
            };
            match backend.connect(&iface, &state) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("status={}", status.as_str());
                    println!("firmware_status={}", backend.firmware_status(&iface));
                    println!("transport_status={}", backend.transport_status(&iface));
                    println!("connect_result={}", backend.connect_result(&iface));
                    return;
                }
                Err(err) => {
                    eprintln!("redbear-wifictl: connect failed for {}: {}", iface, err);
                    process::exit(1);
                }
            }
        }
        Some("--disconnect") => {
            let iface = args.next().unwrap_or_else(|| "wlan0".to_string());
            let mut backend = build_backend();
            if let Err(err) = backend.prepare(&iface) {
                eprintln!("redbear-wifictl: prepare failed for {}: {}", iface, err);
                process::exit(1);
            }
            if let Err(err) = backend.init_transport(&iface) {
                eprintln!(
                    "redbear-wifictl: transport init failed for {}: {}",
                    iface, err
                );
                process::exit(1);
            }
            if let Err(err) = backend.activate(&iface) {
                eprintln!(
                    "redbear-wifictl: activate-nic failed for {}: {}",
                    iface, err
                );
                process::exit(1);
            }
            match backend.disconnect(&iface) {
                Ok(status) => {
                    println!("interface={}", iface);
                    println!("status={}", status.as_str());
                    println!("firmware_status={}", backend.firmware_status(&iface));
                    println!("transport_status={}", backend.transport_status(&iface));
                    println!("disconnect_result={}", backend.disconnect_result(&iface));
                    return;
                }
                Err(err) => {
                    eprintln!("redbear-wifictl: disconnect failed for {}: {}", iface, err);
                    process::exit(1);
                }
            }
        }
        _ => {}
    }

    #[cfg(not(target_os = "redox"))]
    {
        eprintln!("redbear-wifictl: daemon mode is only supported on Redox; use --probe on host");
        process::exit(1);
    }

    #[cfg(target_os = "redox")]
    {
        let notify_fd = unsafe { get_init_notify_fd() };
        let socket = match Socket::create() {
            Ok(s) => s,
            Err(err) => {
                error!("redbear-wifictl: failed to create scheme socket: {err}");
                process::exit(1);
            }
        };
        let mut scheme = WifiCtlScheme::new(build_backend());
        let mut state = redox_scheme::scheme::SchemeState::new();

        notify_scheme_ready(notify_fd, &socket, &mut scheme);
        match libredox::call::setrens(0, 0) {
            Ok(_) => info!("redbear-wifictl: registered scheme:wifictl"),
            Err(err) => {
                error!("redbear-wifictl: failed to enter null namespace: {err}");
                process::exit(1);
            }
        }

        let mut exit_code = 0;
        loop {
            let request = match socket.next_request(SignalBehavior::Restart) {
                Ok(Some(req)) => req,
                Ok(None) => {
                    info!("redbear-wifictl: scheme socket closed, shutting down");
                    break;
                }
                Err(err) => {
                    error!("redbear-wifictl: failed to read scheme request: {err}");
                    exit_code = 1;
                    break;
                }
            };
            match request.kind() {
                redox_scheme::RequestKind::Call(request) => {
                    let response = request.handle_sync(&mut scheme, &mut state);
                    if let Err(err) = socket.write_response(response, SignalBehavior::Restart) {
                        error!("redbear-wifictl: failed to write response: {err}");
                        exit_code = 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        process::exit(exit_code);
    }
}
