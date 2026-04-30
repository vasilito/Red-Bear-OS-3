use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct FirmwareRequest {
    pub name: String,
    pub callback: Box<dyn FnOnce(Result<Vec<u8>, String>) + Send>,
    pub timeout_ms: u64,
}

const POLL_INTERVAL_MS: u64 = 100;
const DEFAULT_FIRMWARE_DIR: &str = "/lib/firmware";
const DEFAULT_UEVENT_DIR: &str = "/run/firmware/uevents";

pub fn request_firmware_nowait(
    name: &str,
    timeout_ms: u64,
    callback: impl FnOnce(Result<Vec<u8>, String>) + Send + 'static,
) {
    let request = FirmwareRequest {
        name: name.to_string(),
        callback: Box::new(callback),
        timeout_ms,
    };

    thread::spawn(move || {
        execute_request(request);
    });
}

fn execute_request(request: FirmwareRequest) {
    let start = Instant::now();
    let timeout = Duration::from_millis(request.timeout_ms);
    let firmware_path = firmware_path(&request.name);
    let mut callback = Some(request.callback);
    let mut dispatched_uevent = false;

    loop {
        match fs::read(&firmware_path) {
            Ok(data) => {
                if let Some(callback) = callback.take() {
                    callback(Ok(data));
                }
                return;
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => {
                if let Some(callback) = callback.take() {
                    callback(Err(format!(
                        "failed to read firmware {} from {}: {}",
                        request.name,
                        firmware_path.display(),
                        err
                    )));
                }
                return;
            }
        }

        if !dispatched_uevent {
            if let Err(err) = dispatch_uevent(&request.name, request.timeout_ms) {
                log::warn!(
                    "firmware-loader: failed to dispatch uevent for {}: {}",
                    request.name,
                    err
                );
            }
            dispatched_uevent = true;
        }

        if start.elapsed() >= timeout {
            if let Some(callback) = callback.take() {
                callback(Err(format!(
                    "timeout while waiting for firmware {} after {}ms",
                    request.name, request.timeout_ms
                )));
            }
            return;
        }

        let remaining = timeout.saturating_sub(start.elapsed());
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS).min(remaining));
    }
}

fn firmware_path(name: &str) -> PathBuf {
    env::var_os("FIRMWARE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIRMWARE_DIR))
        .join(name)
}

fn dispatch_uevent(name: &str, timeout_ms: u64) -> Result<(), String> {
    let content = uevent_content(name, timeout_ms);

    if let Some(helper) = env::var_os("FIRMWARE_UEVENT_HELPER") {
        dispatch_helper(PathBuf::from(helper), name.to_string(), timeout_ms, content.clone())?;
    }

    let spool_dir = env::var_os("FIRMWARE_UEVENT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_UEVENT_DIR));
    write_uevent_file(&spool_dir, name, &content)
}

fn dispatch_helper(
    helper: PathBuf,
    name: String,
    timeout_ms: u64,
    content: String,
) -> Result<(), String> {
    thread::spawn(move || {
        let result = Command::new(&helper)
            .env("ACTION", "add")
            .env("SUBSYSTEM", "firmware")
            .env("FIRMWARE", &name)
            .env("TIMEOUT_MS", timeout_ms.to_string())
            .env("DEVPATH", format!("/devices/virtual/firmware/{}", sanitize_name(&name)))
            .env("UEVENT_CONTENT", &content)
            .status();

        match result {
            Ok(status) if !status.success() => log::warn!(
                "firmware-loader: uevent helper {} exited with status {} for {}",
                helper.display(),
                status,
                name
            ),
            Ok(_) => {}
            Err(err) => log::warn!(
                "firmware-loader: failed to execute uevent helper {} for {}: {}",
                helper.display(),
                name,
                err
            ),
        }
    });

    Ok(())
}

fn write_uevent_file(spool_dir: &Path, name: &str, content: &str) -> Result<(), String> {
    fs::create_dir_all(spool_dir).map_err(|err| {
        format!(
            "failed to create uevent spool directory {}: {}",
            spool_dir.display(),
            err
        )
    })?;

    let timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos(),
        Err(_) => 0,
    };
    let file_name = format!("{}-{timestamp}.uevent", sanitize_name(name));
    let path = spool_dir.join(file_name);

    fs::write(&path, content).map_err(|err| {
        format!(
            "failed to write uevent file {} for firmware {}: {}",
            path.display(),
            name,
            err
        )
    })
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn uevent_content(name: &str, timeout_ms: u64) -> String {
    format!(
        "ACTION=add\nSUBSYSTEM=firmware\nDEVPATH=/devices/virtual/firmware/{}\nFIRMWARE={}\nTIMEOUT_MS={}\n",
        sanitize_name(name),
        name,
        timeout_ms
    )
}

#[cfg(test)]
mod tests {
    use super::request_firmware_nowait;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::{LazyLock, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn temp_root(prefix: &str) -> PathBuf {
        let stamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(err) => panic!("system clock error while creating temp path: {err}"),
        };
        let path = std::env::temp_dir().join(format!("{prefix}-{stamp}"));
        if let Err(err) = fs::create_dir_all(&path) {
            panic!("failed to create temp directory {}: {err}", path.display());
        }
        path
    }

    #[test]
    fn request_firmware_nowait_returns_existing_blob() {
        let _guard = match TEST_ENV_LOCK.lock() {
            Ok(guard) => guard,
            Err(err) => panic!("failed to acquire test env lock: {err}"),
        };
        let root = temp_root("rbos-fw-async-ok");
        let uevent_dir = temp_root("rbos-fw-async-uevents");

        if let Err(err) = fs::write(root.join("iwlwifi-test.ucode"), [9u8, 8, 7]) {
            panic!("failed to write async firmware blob: {err}");
        }

        unsafe {
            std::env::set_var("FIRMWARE_DIR", &root);
            std::env::set_var("FIRMWARE_UEVENT_DIR", &uevent_dir);
        }

        let (tx, rx) = mpsc::channel();
        request_firmware_nowait("iwlwifi-test.ucode", 500, move |result| {
            let _ = tx.send(result);
        });

        let result = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(result) => result,
            Err(err) => panic!("async callback was not received in time: {err}"),
        };
        match result {
            Ok(bytes) => assert_eq!(bytes, vec![9u8, 8, 7]),
            Err(err) => panic!("unexpected async firmware error: {err}"),
        }

        unsafe {
            std::env::remove_var("FIRMWARE_DIR");
            std::env::remove_var("FIRMWARE_UEVENT_DIR");
        }
        if let Err(err) = fs::remove_dir_all(&root) {
            panic!("failed to remove temp directory {}: {err}", root.display());
        }
        if let Err(err) = fs::remove_dir_all(&uevent_dir) {
            panic!("failed to remove temp directory {}: {err}", uevent_dir.display());
        }
    }

    #[test]
    fn request_firmware_nowait_dispatches_uevent_and_retries() {
        let _guard = match TEST_ENV_LOCK.lock() {
            Ok(guard) => guard,
            Err(err) => panic!("failed to acquire test env lock: {err}"),
        };
        let root = temp_root("rbos-fw-async-retry");
        let uevent_dir = temp_root("rbos-fw-async-spool");
        let firmware_name = "intel/ibt-test.sfi";
        let firmware_path = root.join(firmware_name);
        let parent = match firmware_path.parent() {
            Some(parent) => parent.to_path_buf(),
            None => panic!("firmware test path unexpectedly had no parent"),
        };

        unsafe {
            std::env::set_var("FIRMWARE_DIR", &root);
            std::env::set_var("FIRMWARE_UEVENT_DIR", &uevent_dir);
        }

        let writer_path = firmware_path.clone();
        let writer_dir = uevent_dir.clone();
        let writer = std::thread::spawn(move || {
            for _ in 0..50 {
                let has_uevent = match fs::read_dir(&writer_dir) {
                    Ok(entries) => entries
                        .filter_map(Result::ok)
                        .any(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("uevent")),
                    Err(_) => false,
                };
                if has_uevent {
                    if let Err(err) = fs::create_dir_all(&parent) {
                        panic!("failed to create parent firmware directory: {err}");
                    }
                    if let Err(err) = fs::write(&writer_path, [1u8, 2, 3, 4]) {
                        panic!("failed to write firmware after uevent dispatch: {err}");
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("uevent dispatch file was not observed in time");
        });

        let (tx, rx) = mpsc::channel();
        request_firmware_nowait(firmware_name, 1000, move |result| {
            let _ = tx.send(result);
        });

        let result = match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => result,
            Err(err) => panic!("async retry callback was not received in time: {err}"),
        };
        match result {
            Ok(bytes) => assert_eq!(bytes, vec![1u8, 2, 3, 4]),
            Err(err) => panic!("unexpected async retry error: {err}"),
        }

        match writer.join() {
            Ok(()) => {}
            Err(_) => panic!("uevent writer thread panicked"),
        }

        unsafe {
            std::env::remove_var("FIRMWARE_DIR");
            std::env::remove_var("FIRMWARE_UEVENT_DIR");
        }
        if let Err(err) = fs::remove_dir_all(&root) {
            panic!("failed to remove temp directory {}: {err}", root.display());
        }
        if let Err(err) = fs::remove_dir_all(&uevent_dir) {
            panic!("failed to remove temp directory {}: {err}", uevent_dir.display());
        }
    }
}
