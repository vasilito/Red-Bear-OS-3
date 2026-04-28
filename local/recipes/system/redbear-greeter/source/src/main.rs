use std::{
    env,
    fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}},
    path::{Path, PathBuf},
    process::{self, Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use redbear_login_protocol::{AuthRequest, AuthResponse, GreeterRequest, GreeterResponse};

const GREETER_SOCKET_PATH: &str = "/run/redbear-greeterd.sock";
const AUTH_SOCKET_PATH: &str = "/run/redbear-authd.sock";
const BACKGROUND_PATH: &str = "/usr/share/redbear/greeter/background.png";
const ICON_PATH: &str = "/usr/share/redbear/greeter/icon.png";
const COMPOSITOR_BIN_PATH: &str = "/usr/bin/redbear-greeter-compositor";
const COMPOSITOR_SHARE_PATH: &str = "/usr/share/redbear/greeter/redbear-greeter-compositor";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GreeterState {
    Starting,
    GreeterReady,
    Authenticating,
    LaunchingSession,
    SessionRunning,
    ReturningToGreeter,
    PowerAction,
    FatalError,
}

impl GreeterState {
    fn as_str(self) -> &'static str {
        match self {
            GreeterState::Starting => "starting",
            GreeterState::GreeterReady => "greeter_ready",
            GreeterState::Authenticating => "authenticating",
            GreeterState::LaunchingSession => "launching_session",
            GreeterState::SessionRunning => "session_running",
            GreeterState::ReturningToGreeter => "returning_to_greeter",
            GreeterState::PowerAction => "power_action",
            GreeterState::FatalError => "fatal_error",
        }
    }
}

#[derive(Debug)]
struct GreeterDaemon {
    listener: UnixListener,
    vt: u32,
    greeter_user: String,
    runtime_dir: PathBuf,
    wayland_display: String,
    state: GreeterState,
    message: String,
    compositor: Option<Child>,
    ui: Option<Child>,
    restart_attempts: Vec<Instant>,
}

fn usage() -> &'static str {
    "Usage: redbear-greeterd [--help]"
}

fn parse_args() -> Result<(), String> {
    let mut args = env::args().skip(1);
    match args.next() {
        None => Ok(()),
        Some(arg) if arg == "--help" || arg == "-h" => Err(String::new()),
        Some(arg) => Err(format!("unrecognized argument '{arg}'")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountFormat {
    Redox,
    Unix,
}

fn split_account_fields(line: &str) -> (AccountFormat, Vec<&str>) {
    let format = if line.contains(';') {
        AccountFormat::Redox
    } else {
        AccountFormat::Unix
    };
    let delimiter = match format {
        AccountFormat::Redox => ';',
        AccountFormat::Unix => ':',
    };
    (format, line.split(delimiter).collect())
}

fn parse_uid_gid(parts: &[&str], format: AccountFormat) -> Option<(u32, u32)> {
    let (uid_index, gid_index) = match format {
        AccountFormat::Redox if parts.len() >= 3 => (1, 2),
        AccountFormat::Unix if parts.len() >= 4 => (2, 3),
        _ => return None,
    };

    let uid = parts[uid_index].parse::<u32>().ok()?;
    let gid = parts[gid_index].parse::<u32>().ok()?;
    Some((uid, gid))
}

fn load_uid_gid(username: &str) -> Result<(u32, u32), String> {
    let passwd = fs::read_to_string("/etc/passwd").map_err(|err| format!("failed to read /etc/passwd: {err}"))?;
    for line in passwd.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (format, parts) = split_account_fields(line);
        if parts.len() < 3 || parts[0] != username {
            continue;
        }
        if let Some((uid, gid)) = parse_uid_gid(&parts, format) {
            return Ok((uid, gid));
        }
        return Err(format!("invalid uid/gid for user '{username}'"));
    }
    Err(format!("unknown greeter user '{username}'"))
}

fn change_socket_ownership(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("socket path {} contains interior NUL", path.display()))?;
    let result = unsafe { libc::chown(c_path.as_ptr(), uid, gid) };
    if result == 0 {
        Ok(())
    } else {
        Err(format!("failed to chown {}: {}", path.display(), io::Error::last_os_error()))
    }
}

fn send_auth_request(request: &AuthRequest) -> Result<AuthResponse, String> {
    let mut stream = UnixStream::connect(AUTH_SOCKET_PATH)
        .map_err(|err| format!("failed to connect to {AUTH_SOCKET_PATH}: {err}"))?;
    let payload = serde_json::to_string(request).map_err(|err| format!("failed to serialize auth request: {err}"))?;
    stream
        .write_all(payload.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to write auth request: {err}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|err| format!("failed to read auth response: {err}"))?;
    serde_json::from_str(line.trim()).map_err(|err| format!("failed to parse auth response: {err}"))
}

impl GreeterDaemon {
    fn hello_response(&self) -> GreeterResponse {
        GreeterResponse::HelloOk {
            background: String::from(BACKGROUND_PATH),
            icon: String::from(ICON_PATH),
            session_name: String::from("KDE on Wayland"),
            state: String::from(self.state.as_str()),
            message: self.message.clone(),
        }
    }

    fn new() -> Result<Self, String> {
        let vt = env::var("VT")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(3);
        let greeter_user = env::var("REDBEAR_GREETER_USER").unwrap_or_else(|_| String::from("greeter"));

        if Path::new(GREETER_SOCKET_PATH).exists() {
            fs::remove_file(GREETER_SOCKET_PATH)
                .map_err(|err| format!("failed to remove stale greeter socket: {err}"))?;
        }
        let listener = UnixListener::bind(GREETER_SOCKET_PATH)
            .map_err(|err| format!("failed to bind {GREETER_SOCKET_PATH}: {err}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("failed to set nonblocking socket mode: {err}"))?;
        let (uid, gid) = load_uid_gid(&greeter_user)?;
        fs::set_permissions(GREETER_SOCKET_PATH, fs::Permissions::from_mode(0o660))
            .map_err(|err| format!("failed to chmod {GREETER_SOCKET_PATH}: {err}"))?;
        change_socket_ownership(Path::new(GREETER_SOCKET_PATH), uid, gid)?;

        Ok(Self {
            listener,
            vt,
            greeter_user,
            runtime_dir: PathBuf::from("/tmp/run/redbear-greeter"),
            wayland_display: String::from("wayland-0"),
            state: GreeterState::Starting,
            message: String::from("Starting greeter"),
            compositor: None,
            ui: None,
            restart_attempts: Vec::new(),
        })
    }

    fn set_state(&mut self, state: GreeterState, message: impl Into<String>) {
        self.state = state;
        self.message = message.into();
    }

    fn configure_command(&self, command: &mut Command) {
        command.env("QT_PLUGIN_PATH", "/usr/plugins");
        command.env("QT_QPA_PLATFORM_PLUGIN_PATH", "/usr/plugins/platforms");
        command.env("QML2_IMPORT_PATH", "/usr/qml");
        command.env("XCURSOR_THEME", "Pop");
        command.env("XKB_CONFIG_ROOT", "/usr/share/X11/xkb");
        command.env("WAYLAND_DISPLAY", &self.wayland_display);
    }

    fn activate_vt(&self, vt: u32) -> Result<(), String> {
        let status = Command::new("/usr/bin/inputd")
            .arg("-A")
            .arg(vt.to_string())
            .status()
            .map_err(|err| format!("failed to invoke inputd for VT {vt}: {err}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("inputd failed to activate VT {vt}: {status}"))
        }
    }

    fn spawn_as_greeter(&self, program: &str) -> Result<Child, String> {
        let mut command = Command::new("/usr/bin/redbear-session-launch");
        command
            .arg("--username")
            .arg(&self.greeter_user)
            .arg("--mode")
            .arg("command")
            .arg("--vt")
            .arg(self.vt.to_string())
            .arg("--runtime-dir")
            .arg(&self.runtime_dir)
            .arg("--wayland-display")
            .arg(&self.wayland_display)
            .arg("--command")
            .arg(program);
        self.configure_command(&mut command);
        command
            .spawn()
            .map_err(|err| format!("failed to spawn {program} as {}: {err}", self.greeter_user))
    }

    fn wait_for_wayland_socket(&self) -> Result<(), String> {
        let socket_path = self.runtime_dir.join(&self.wayland_display);
        for _ in 0..60 {
            if socket_path.exists() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
        Err(format!("timed out waiting for compositor socket {}", socket_path.display()))
    }

    fn start_surface(&mut self) -> Result<(), String> {
        self.set_state(GreeterState::Starting, "Starting greeter surface");
        println!("redbear-greeterd: starting compositor ({})...", COMPOSITOR_BIN_PATH);
        let compositor_path = if Path::new(COMPOSITOR_BIN_PATH).is_file() {
            COMPOSITOR_BIN_PATH
        } else {
            COMPOSITOR_SHARE_PATH
        };
        self.compositor = Some(self.spawn_as_greeter(compositor_path)?);
        println!("redbear-greeterd: waiting for Wayland socket...");
        self.wait_for_wayland_socket()?;
        println!("redbear-greeterd: compositor ready, launching greeter UI...");
        self.ui = Some(self.spawn_as_greeter("/usr/bin/redbear-greeter-ui")?);
        println!("redbear-greeterd: greeter UI launched, activating VT {}", self.vt);
        self.activate_vt(self.vt)?;
        self.set_state(GreeterState::GreeterReady, "Ready");
        println!("redbear-greeterd: greeter ready on VT {}", self.vt);
        Ok(())
    }

    fn kill_child(child: &mut Option<Child>) {
        if let Some(process) = child.as_mut() {
            let _ = process.kill();
            let _ = process.wait();
        }
        *child = None;
    }

    fn note_restart(&mut self) -> Result<(), String> {
        let now = Instant::now();
        self.restart_attempts
            .retain(|attempt| now.saturating_duration_since(*attempt) <= Duration::from_secs(60));
        self.restart_attempts.push(now);
        if self.restart_attempts.len() > 3 {
            self.set_state(GreeterState::FatalError, "Greeter restart limit reached");
            return Err(String::from("greeter restart limit reached; leaving fallback consoles available"));
        }
        Ok(())
    }

    fn handle_surface_exit(&mut self, status: ExitStatus) -> Result<(), String> {
        self.ui = None;
        if status.success() {
            self.message = String::from("Greeter UI exited");
        } else {
            let code = status.code().unwrap_or(-1);
            let hint = if code == 1 {
                " — QML loading failed (check QML2_IMPORT_PATH and QT_PLUGIN_PATH)"
            } else {
                ""
            };
            self.message = format!("Greeter UI exited unexpectedly: {status}{hint}");
        }
        self.note_restart()?;
        Self::kill_child(&mut self.compositor);
        self.start_surface()
    }

    fn launch_session(&mut self, username: &str) -> Result<(), String> {
        self.set_state(GreeterState::LaunchingSession, "Starting session");
        Self::kill_child(&mut self.ui);
        Self::kill_child(&mut self.compositor);
        self.set_state(GreeterState::SessionRunning, "Session running");

        let response = send_auth_request(&AuthRequest::StartSession {
            request_id: 2,
            username: username.to_string(),
            session: String::from("kde-wayland"),
            vt: self.vt,
        })?;

        self.set_state(GreeterState::ReturningToGreeter, "Returning to greeter");
        match response {
            AuthResponse::SessionResult { ok, message, .. } => {
                if !ok {
                    self.set_state(GreeterState::GreeterReady, message.clone());
                }
                self.message = message;
            }
            AuthResponse::Error { message } => self.message = message,
            _ => self.message = String::from("Unexpected auth response while starting session"),
        }
        self.start_surface()
    }

    fn handle_connection(&mut self, stream: UnixStream) -> Result<(), String> {
        stream
            .set_nonblocking(false)
            .map_err(|err| format!("failed to set blocking greeter stream mode: {err}"))?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|err| format!("failed to read greeter request: {err}"))?;

        let request = serde_json::from_str::<GreeterRequest>(line.trim())
            .map_err(|err| format!("invalid greeter request: {err}"))?;
        let mut launch_username = None;
        let response = match request {
            GreeterRequest::Hello { version } => {
                if version != 1 {
                    GreeterResponse::Error {
                        message: format!("unsupported greeter protocol version {version}"),
                    }
                } else {
                    self.hello_response()
                }
            }
            GreeterRequest::SubmitLogin { username, password } => {
                self.set_state(GreeterState::Authenticating, "Authenticating");
                match send_auth_request(&AuthRequest::Authenticate {
                    request_id: 1,
                    username: username.clone(),
                    password: password.clone(),
                    vt: self.vt,
                })? {
                    AuthResponse::AuthenticateResult { ok, message, .. } => {
                        if ok {
                            self.set_state(GreeterState::LaunchingSession, "Starting session");
                            launch_username = Some(username);
                        } else {
                            self.set_state(GreeterState::GreeterReady, message.clone());
                        }
                        GreeterResponse::LoginResult {
                            ok,
                            state: String::from(self.state.as_str()),
                            message,
                        }
                    }
                    AuthResponse::Error { message } => {
                        self.set_state(GreeterState::GreeterReady, message.clone());
                        GreeterResponse::Error { message }
                    }
                    _ => GreeterResponse::Error {
                        message: String::from("unexpected auth response"),
                    },
                }
            }
            GreeterRequest::RequestShutdown => {
                self.set_state(GreeterState::PowerAction, "Requesting shutdown");
                match send_auth_request(&AuthRequest::PowerAction {
                    request_id: 3,
                    action: String::from("shutdown"),
                })? {
                    AuthResponse::PowerResult { ok, message, .. } => GreeterResponse::ActionResult { ok, message },
                    AuthResponse::Error { message } => GreeterResponse::Error { message },
                    _ => GreeterResponse::Error {
                        message: String::from("unexpected power-action response"),
                    },
                }
            }
            GreeterRequest::RequestReboot => {
                self.set_state(GreeterState::PowerAction, "Requesting reboot");
                match send_auth_request(&AuthRequest::PowerAction {
                    request_id: 4,
                    action: String::from("reboot"),
                })? {
                    AuthResponse::PowerResult { ok, message, .. } => GreeterResponse::ActionResult { ok, message },
                    AuthResponse::Error { message } => GreeterResponse::Error { message },
                    _ => GreeterResponse::Error {
                        message: String::from("unexpected power-action response"),
                    },
                }
            }
        };

        let payload = serde_json::to_string(&response)
            .map_err(|err| format!("failed to serialize greeter response: {err}"))?;
        let mut stream = reader.into_inner();
        stream
            .write_all(payload.as_bytes())
            .and_then(|_| stream.write_all(b"\n"))
            .map_err(|err| format!("failed to write greeter response: {err}"))?;

        if let Some(username) = launch_username {
            self.launch_session(&username)?;
        }
        Ok(())
    }

    fn check_children(&mut self) -> Result<(), String> {
        if let Some(process) = self.compositor.as_mut() {
            if let Some(status) = process.try_wait().map_err(|err| format!("failed to poll compositor: {err}"))? {
                self.compositor = None;
                self.note_restart()?;
                self.message = format!("Greeter compositor exited unexpectedly: {status}");
                Self::kill_child(&mut self.ui);
                self.start_surface()?;
                return Ok(());
            }
        }

        if let Some(process) = self.ui.as_mut() {
            if let Some(status) = process.try_wait().map_err(|err| format!("failed to poll greeter UI: {err}"))? {
                return self.handle_surface_exit(status);
            }
        }

        Ok(())
    }

    fn run(&mut self) -> Result<(), String> {
        self.start_surface()?;
        loop {
            self.check_children()?;
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Err(err) = self.handle_connection(stream) {
                        eprintln!("redbear-greeterd: {err}");
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(err) => return Err(format!("failed to accept greeter connection: {err}")),
            }
        }
    }
}

fn run() -> Result<(), String> {
    match parse_args() {
        Ok(()) => {}
        Err(err) if err.is_empty() => {
            println!("{}", usage());
            return Ok(());
        }
        Err(err) => return Err(err),
    }

    let mut daemon = GreeterDaemon::new()?;
    daemon.run()
}

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-greeterd: {err}");
        eprintln!("{}", usage());
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SOCKET_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_daemon() -> GreeterDaemon {
        let unique = TEST_SOCKET_COUNTER.fetch_add(1, Ordering::Relaxed);
        let socket_path = std::env::temp_dir().join(format!(
            "redbear-greeterd-test-{}-{}.sock",
            process::id(),
            unique
        ));
        let _ = fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).expect("test listener should bind");
        listener
            .set_nonblocking(true)
            .expect("test listener should become nonblocking");

        GreeterDaemon {
            listener,
            vt: 3,
            greeter_user: String::from("greeter"),
            runtime_dir: PathBuf::from("/tmp/run/redbear-greeter-test"),
            wayland_display: String::from("wayland-0"),
            state: GreeterState::Starting,
            message: String::from("Starting greeter"),
            compositor: None,
            ui: None,
            restart_attempts: Vec::new(),
        }
    }

    fn send_daemon_request(daemon: &mut GreeterDaemon, request: &str) -> GreeterResponse {
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        client
            .write_all(request.as_bytes())
            .and_then(|_| client.write_all(b"\n"))
            .expect("request should write");
        daemon.handle_connection(server).expect("handler should succeed");
        let mut line = String::new();
        BufReader::new(client)
            .read_line(&mut line)
            .expect("response should read");
        serde_json::from_str(line.trim()).expect("response should parse")
    }

    #[test]
    fn greeter_state_strings_match_protocol_contract() {
        assert_eq!(GreeterState::Starting.as_str(), "starting");
        assert_eq!(GreeterState::GreeterReady.as_str(), "greeter_ready");
        assert_eq!(GreeterState::Authenticating.as_str(), "authenticating");
        assert_eq!(GreeterState::LaunchingSession.as_str(), "launching_session");
        assert_eq!(GreeterState::SessionRunning.as_str(), "session_running");
        assert_eq!(GreeterState::ReturningToGreeter.as_str(), "returning_to_greeter");
        assert_eq!(GreeterState::PowerAction.as_str(), "power_action");
        assert_eq!(GreeterState::FatalError.as_str(), "fatal_error");
    }

    #[test]
    fn hello_response_uses_installed_asset_paths() {
        let mut daemon = test_daemon();
        daemon.set_state(GreeterState::GreeterReady, "Ready");

        match daemon.hello_response() {
            GreeterResponse::HelloOk {
                background,
                icon,
                session_name,
                state,
                message,
            } => {
                assert_eq!(background, BACKGROUND_PATH);
                assert_eq!(icon, ICON_PATH);
                assert_eq!(session_name, "KDE on Wayland");
                assert_eq!(state, "greeter_ready");
                assert_eq!(message, "Ready");
            }
            _ => panic!("expected hello_ok response"),
        }
    }

    #[test]
    fn note_restart_bounds_repeated_failures() {
        let mut daemon = test_daemon();

        for _ in 0..3 {
            daemon.note_restart().expect("restart should remain bounded");
            assert_ne!(daemon.state, GreeterState::FatalError);
        }

        let error = daemon.note_restart().expect_err("fourth restart should fail");
        assert!(error.contains("restart limit"));
        assert_eq!(daemon.state, GreeterState::FatalError);
        assert_eq!(daemon.message, "Greeter restart limit reached");
    }

    #[test]
    fn handle_connection_rejects_unsupported_protocol_version() {
        let mut daemon = test_daemon();

        match send_daemon_request(&mut daemon, r#"{"type":"hello","version":99}"#) {
            GreeterResponse::Error { message } => {
                assert_eq!(message, "unsupported greeter protocol version 99");
            }
            _ => panic!("expected error response"),
        }
    }

    #[test]
    fn handle_connection_rejects_invalid_json_request() {
        let mut daemon = test_daemon();
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        client
            .write_all(b"not-json\n")
            .expect("request should write");
        let error = daemon
            .handle_connection(server)
            .expect_err("invalid request should fail");
        assert!(error.contains("invalid greeter request"));
    }

    #[test]
    fn parse_uid_gid_accepts_redox_style_layout() {
        assert_eq!(
            parse_uid_gid(
                &["greeter", "101", "101", "Greeter", "/nonexistent", "/usr/bin/ion"],
                AccountFormat::Redox,
            ),
            Some((101, 101))
        );
    }

    #[test]
    fn parse_uid_gid_accepts_unix_style_layout() {
        assert_eq!(
            parse_uid_gid(
                &["root", "x", "0", "0", "root", "/root", "/usr/bin/ion"],
                AccountFormat::Unix,
            ),
            Some((0, 0))
        );
    }

    #[test]
    fn split_account_fields_detects_redox_layout() {
        let (format, parts) = split_account_fields("greeter;101;101;Greeter;/nonexistent;/usr/bin/ion");
        assert_eq!(format, AccountFormat::Redox);
        assert_eq!(parts[0], "greeter");
        assert_eq!(parts[2], "101");
    }
}
