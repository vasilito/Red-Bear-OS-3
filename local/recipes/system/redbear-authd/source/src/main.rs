use std::{
    collections::HashMap,
    env,
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::{UnixListener, UnixStream}},
    path::Path,
    process::{self, Command},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use argon2::{self, verify_encoded};
use redbear_login_protocol::{AuthRequest, AuthResponse};
use serde::Serialize;
use sha_crypt::{PasswordVerifier, ShaCrypt};

#[derive(Debug, PartialEq, Eq)]
enum VerifyError {
    UnsupportedHashFormat,
}

const AUTH_SOCKET_PATH: &str = "/run/redbear-authd.sock";
const SESSIOND_SOCKET_PATH: &str = "/run/redbear-sessiond-control.sock";
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
const LOCKOUT_DURATION: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
struct Account {
    username: String,
    password: String,
    uid: u32,
    shell: String,
}

#[derive(Clone, Debug)]
struct Approval {
    expires_at: Instant,
    vt: u32,
}

#[derive(Clone, Debug, Default)]
struct FailureState {
    attempts: Vec<Instant>,
    locked_until: Option<Instant>,
}

#[derive(Clone, Debug, Default)]
struct RuntimeState {
    approvals: Arc<Mutex<HashMap<String, Approval>>>,
    failures: Arc<Mutex<HashMap<String, FailureState>>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SessiondUpdate {
    SetSession {
        username: String,
        uid: u32,
        vt: u32,
        leader: u32,
        state: String,
    },
    ResetSession {
        vt: u32,
    },
}

fn usage() -> &'static str {
    "Usage: redbear-authd [--help]"
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

fn split_account_fields(line: &str) -> (AccountFormat, Vec<String>) {
    let format = if line.contains(';') {
        AccountFormat::Redox
    } else {
        AccountFormat::Unix
    };
    let delimiter = match format {
        AccountFormat::Redox => ';',
        AccountFormat::Unix => ':',
    };
    (format, line.split(delimiter).map(str::to_string).collect::<Vec<_>>())
}

fn load_shadow_passwords() -> Result<HashMap<String, String>, String> {
    if !Path::new("/etc/shadow").exists() {
        return Ok(HashMap::new());
    }

    let mut passwords = HashMap::new();
    let contents = fs::read_to_string("/etc/shadow")
        .map_err(|err| format!("failed to read /etc/shadow: {err}"))?;
    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (_format, parts) = split_account_fields(line);
        if parts.len() < 2 {
            return Err(format!("invalid shadow entry on line {}", index + 1));
        }
        passwords.insert(parts[0].clone(), parts[1].clone());
    }
    Ok(passwords)
}

fn load_account(username: &str) -> Result<Account, String> {
    let shadow_passwords = load_shadow_passwords()?;
    let contents = fs::read_to_string("/etc/passwd")
        .map_err(|err| format!("failed to read /etc/passwd: {err}"))?;
    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (format, parts) = split_account_fields(line);
        if parts[0] != username {
            continue;
        }

        let (uid_index, gid_index, shell_index, passwd_index) = match format {
            AccountFormat::Redox if parts.len() >= 6 => (1, 2, 5, None),
            AccountFormat::Unix if parts.len() >= 7 => (2, 3, 6, Some(1)),
            AccountFormat::Redox => {
                return Err(format!("invalid Redox passwd entry for user '{username}' on line {}", index + 1))
            }
            AccountFormat::Unix => {
                return Err(format!("invalid passwd entry for user '{username}' on line {}", index + 1))
            }
        };

        let uid = parts[uid_index]
            .parse::<u32>()
            .map_err(|_| format!("invalid uid for user '{username}'"))?;
        let _gid = parts[gid_index]
            .parse::<u32>()
            .map_err(|_| format!("invalid gid for user '{username}'"))?;
        let password = shadow_passwords
            .get(username)
            .cloned()
            .unwrap_or_else(|| passwd_index.map(|index| parts[index].clone()).unwrap_or_default());

        return Ok(Account {
            username: parts[0].clone(),
            password,
            uid,
            shell: parts[shell_index].clone(),
        });
    }

    Err(format!("unknown user '{username}'"))
}

fn trim_failures(entries: &mut Vec<Instant>, now: Instant) {
    entries.retain(|entry| now.saturating_duration_since(*entry) <= FAILURE_WINDOW);
}

fn login_allowed(account: &Account) -> bool {
    if account.uid != 0 && account.uid < 1000 {
        return false;
    }
    !account.shell.is_empty()
}

fn verify_shadow_password(password: &str, shadow_hash: &str) -> Result<bool, VerifyError> {
    if shadow_hash.starts_with("$6$") || shadow_hash.starts_with("$5$") {
        return Ok(ShaCrypt::default()
            .verify_password(password.as_bytes(), shadow_hash)
            .is_ok());
    }
    if shadow_hash.starts_with("$argon2") {
        return Ok(verify_encoded(shadow_hash, password.as_bytes()).unwrap_or(false));
    }
    Err(VerifyError::UnsupportedHashFormat)
}

fn verify_password(account: &Account, password: &str) -> bool {
    if account.password.is_empty() || account.password.starts_with('!') || account.password.starts_with('*') {
        return false;
    }

    if account.password.starts_with('$') {
        match verify_shadow_password(password, &account.password) {
            Ok(ok) => return ok,
            Err(VerifyError::UnsupportedHashFormat) => {
                eprintln!(
                    "redbear-authd: password hash for user {} uses an unsupported shadow format",
                    account.username
                );
                return false;
            }
        }
    }

    account.password == password
}

fn remember_success(state: &RuntimeState, username: &str, vt: u32) -> Result<(), String> {
    let mut approvals = state
        .approvals
        .lock()
        .map_err(|_| String::from("approval state is poisoned"))?;
    approvals.insert(
        username.to_string(),
        Approval {
            expires_at: Instant::now() + Duration::from_secs(15),
            vt,
        },
    );

    let mut failures = state
        .failures
        .lock()
        .map_err(|_| String::from("failure state is poisoned"))?;
    failures.remove(username);
    Ok(())
}

fn remember_failure(state: &RuntimeState, username: &str) -> Result<String, String> {
    let mut failures = state
        .failures
        .lock()
        .map_err(|_| String::from("failure state is poisoned"))?;
    let now = Instant::now();
    let entry = failures.entry(username.to_string()).or_default();
    trim_failures(&mut entry.attempts, now);
    entry.attempts.push(now);
    if entry.attempts.len() >= 5 {
        entry.locked_until = Some(now + LOCKOUT_DURATION);
        Ok(String::from("Too many failed attempts. Try again shortly."))
    } else {
        Ok(String::from("Invalid username or password."))
    }
}

fn check_lockout(state: &RuntimeState, username: &str) -> Result<Option<String>, String> {
    let mut failures = state
        .failures
        .lock()
        .map_err(|_| String::from("failure state is poisoned"))?;
    let now = Instant::now();
    if let Some(entry) = failures.get_mut(username) {
        trim_failures(&mut entry.attempts, now);
        if let Some(locked_until) = entry.locked_until {
            if locked_until > now {
                return Ok(Some(String::from("Too many failed attempts. Try again shortly.")));
            }
            entry.locked_until = None;
        }
    }
    Ok(None)
}

fn take_approval(state: &RuntimeState, username: &str, vt: u32) -> Result<(), String> {
    let mut approvals = state
        .approvals
        .lock()
        .map_err(|_| String::from("approval state is poisoned"))?;
    let Some(approval) = approvals.remove(username) else {
        return Err(String::from("No recent authentication approval exists for this user."));
    };
    if approval.expires_at < Instant::now() {
        return Err(String::from("Authentication approval expired. Please log in again."));
    }
    if approval.vt != vt {
        return Err(String::from("Authentication approval does not match the requested VT."));
    }
    Ok(())
}

fn send_sessiond_update(message: &SessiondUpdate) {
    let Ok(mut stream) = UnixStream::connect(SESSIOND_SOCKET_PATH) else {
        return;
    };
    let Ok(json) = serde_json::to_string(message) else {
        return;
    };
    let _ = stream.write_all(json.as_bytes());
    let _ = stream.write_all(b"\n");
}

fn launch_session(account: &Account, session: &str, vt: u32) -> Result<Option<i32>, String> {
    if session != "kde-wayland" {
        return Err(format!("unsupported session '{session}'"));
    }

    let mut child = Command::new("/usr/bin/redbear-session-launch")
        .arg("--username")
        .arg(&account.username)
        .arg("--mode")
        .arg("session")
        .arg("--session")
        .arg(session)
        .arg("--vt")
        .arg(vt.to_string())
        .spawn()
        .map_err(|err| format!("failed to launch session for {}: {err}", account.username))?;

    send_sessiond_update(&SessiondUpdate::SetSession {
        username: account.username.clone(),
        uid: account.uid,
        vt,
        leader: child.id(),
        state: String::from("online"),
    });

    let status = child
        .wait()
        .map_err(|err| format!("failed while waiting for session process: {err}"))?;

    send_sessiond_update(&SessiondUpdate::ResetSession { vt });
    Ok(status.code())
}

fn run_power_action(action: &str) -> Result<String, String> {
    let candidates: &[&[&str]] = match action {
        "shutdown" => &[&["/usr/bin/shutdown"], &["shutdown"], &["poweroff"]],
        "reboot" => &[&["/usr/bin/reboot"], &["reboot"]],
        other => return Err(format!("unsupported power action '{other}'")),
    };

    for candidate in candidates {
        let program = candidate[0];
        let args = &candidate[1..];
        let Ok(status) = Command::new(program).args(args).status() else {
            continue;
        };
        if status.success() {
            return Ok(format!("{action} requested"));
        }
    }

    Err(format!("failed to execute {action} command"))
}

fn handle_request(request: AuthRequest, state: &RuntimeState) -> AuthResponse {
    match request {
        AuthRequest::Authenticate {
            request_id,
            username,
            password,
            vt,
        } => {
            match check_lockout(state, &username) {
                Ok(Some(message)) => {
                    return AuthResponse::AuthenticateResult {
                        request_id,
                        ok: false,
                        message,
                    };
                }
                Ok(None) => {}
                Err(message) => return AuthResponse::Error { message },
            }

            match load_account(&username) {
                Ok(account) if login_allowed(&account) && verify_password(&account, &password) => {
                    if let Err(message) = remember_success(state, &username, vt) {
                        return AuthResponse::Error { message };
                    }
                    AuthResponse::AuthenticateResult {
                        request_id,
                        ok: true,
                        message: String::from("Authentication successful."),
                    }
                }
                Ok(_) | Err(_) => {
                    let message = remember_failure(state, &username)
                        .unwrap_or_else(|_| String::from("Invalid username or password."));
                    AuthResponse::AuthenticateResult {
                        request_id,
                        ok: false,
                        message,
                    }
                }
            }
        }
        AuthRequest::StartSession {
            request_id,
            username,
            session,
            vt,
        } => {
            if let Err(message) = take_approval(state, &username, vt) {
                return AuthResponse::SessionResult {
                    request_id,
                    ok: false,
                    exit_code: None,
                    message,
                };
            }

            match load_account(&username).and_then(|account| {
                let exit_code = launch_session(&account, &session, vt)?;
                Ok((account, exit_code))
            }) {
                Ok((_account, exit_code)) => AuthResponse::SessionResult {
                    request_id,
                    ok: true,
                    exit_code,
                    message: String::from("Session completed."),
                },
                Err(message) => AuthResponse::SessionResult {
                    request_id,
                    ok: false,
                    exit_code: None,
                    message,
                },
            }
        }
        AuthRequest::PowerAction { request_id, action } => match run_power_action(&action) {
            Ok(message) => AuthResponse::PowerResult {
                request_id,
                ok: true,
                message,
            },
            Err(message) => AuthResponse::PowerResult {
                request_id,
                ok: false,
                message,
            },
        },
    }
}

fn handle_connection(stream: UnixStream, state: RuntimeState) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }

    let response = match serde_json::from_str::<AuthRequest>(line.trim()) {
        Ok(request) => handle_request(request, &state),
        Err(err) => AuthResponse::Error {
            message: format!("invalid request: {err}"),
        },
    };

    let Ok(payload) = serde_json::to_string(&response) else {
        return;
    };
    let mut stream = reader.into_inner();
    let _ = stream.write_all(payload.as_bytes());
    let _ = stream.write_all(b"\n");
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

    if Path::new(AUTH_SOCKET_PATH).exists() {
        fs::remove_file(AUTH_SOCKET_PATH)
            .map_err(|err| format!("failed to remove stale auth socket {AUTH_SOCKET_PATH}: {err}"))?;
    }

    let listener = UnixListener::bind(AUTH_SOCKET_PATH)
        .map_err(|err| format!("failed to bind auth socket {AUTH_SOCKET_PATH}: {err}"))?;
    fs::set_permissions(AUTH_SOCKET_PATH, fs::Permissions::from_mode(0o600))
        .map_err(|err| format!("failed to set permissions on {AUTH_SOCKET_PATH}: {err}"))?;
    let state = RuntimeState::default();

    eprintln!("redbear-authd: listening on {AUTH_SOCKET_PATH}");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_connection(stream, state.clone()),
            Err(err) => eprintln!("redbear-authd: failed to accept connection: {err}"),
        }
    }

    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-authd: {err}");
        eprintln!("{}", usage());
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    fn send_handle_connection_request(request: &str) -> AuthResponse {
        let state = RuntimeState::default();
        let (mut client, server) = UnixStream::pair().expect("socket pair should open");
        client
            .write_all(request.as_bytes())
            .and_then(|_| client.write_all(b"\n"))
            .expect("request should write");
        handle_connection(server, state);
        let mut line = String::new();
        BufReader::new(client)
            .read_line(&mut line)
            .expect("response should read");
        serde_json::from_str(line.trim()).expect("response should parse")
    }

    #[test]
    fn verify_password_accepts_plain_passwords() {
        let account = Account {
            username: String::from("root"),
            password: String::from("password"),
            uid: 0,
            shell: String::from("/usr/bin/ion"),
        };
        assert!(verify_password(&account, "password"));
        assert!(!verify_password(&account, "wrong"));
    }

    #[test]
    fn verify_shadow_password_accepts_sha512_crypt() {
        let hash = "$6$saltstring$adDbXsJjcDlq2662QPgd.tkSOVmnG9Tt3oXl4HR60SusC3AGjirnDenVZp3DGwLwqy6iYKCzannhaX9DR72nN1";
        assert_eq!(verify_shadow_password("password", hash), Ok(true));
        assert_eq!(verify_shadow_password("wrong", hash), Ok(false));
    }

    #[test]
    fn verify_shadow_password_accepts_sha256_crypt() {
        let hash = "$5$saltstring$OH4IDuTlsuTYPdED1gsuiRMyTAwNlRWyA6Xr3I4/dQ5";
        assert_eq!(verify_shadow_password("password", hash), Ok(true));
        assert_eq!(verify_shadow_password("wrong", hash), Ok(false));
    }

    #[test]
    fn verify_shadow_password_accepts_argon2_hashes() {
        let config = argon2::Config::default();
        let hash = argon2::hash_encoded(b"password", b"testsalt", &config)
            .expect("argon2 hash should encode");
        assert_eq!(verify_shadow_password("password", &hash), Ok(true));
        assert_eq!(verify_shadow_password("wrong", &hash), Ok(false));
    }

    #[test]
    fn verify_shadow_password_rejects_unknown_hash_prefix() {
        assert_eq!(verify_shadow_password("password", "$1$legacy$hash"), Err(VerifyError::UnsupportedHashFormat));
    }

    #[test]
    fn verify_password_rejects_locked_accounts() {
        let account = Account {
            username: String::from("greeter"),
            password: String::from("!"),
            uid: 101,
            shell: String::from("/usr/bin/ion"),
        };
        assert!(!verify_password(&account, "anything"));
    }

    #[test]
    fn login_allowed_rejects_low_uid_non_root_accounts() {
        let account = Account {
            username: String::from("greeter"),
            password: String::from("password"),
            uid: 101,
            shell: String::from("/usr/bin/ion"),
        };
        assert!(!login_allowed(&account));
    }

    #[test]
    fn remember_failure_locks_after_five_attempts() {
        let state = RuntimeState::default();
        for _ in 0..4 {
            let message = remember_failure(&state, "user").expect("failure tracking should succeed");
            assert_eq!(message, "Invalid username or password.");
        }

        let message = remember_failure(&state, "user").expect("lockout tracking should succeed");
        assert_eq!(message, "Too many failed attempts. Try again shortly.");
        assert_eq!(
            check_lockout(&state, "user").expect("lockout lookup should succeed"),
            Some(String::from("Too many failed attempts. Try again shortly."))
        );
    }

    #[test]
    fn take_approval_rejects_vt_mismatch() {
        let state = RuntimeState::default();
        remember_success(&state, "user", 3).expect("approval should be recorded");
        assert_eq!(
            take_approval(&state, "user", 4),
            Err(String::from("Authentication approval does not match the requested VT."))
        );
    }

    #[test]
    fn start_session_request_rejects_missing_approval() {
        let state = RuntimeState::default();
        let response = handle_request(
            AuthRequest::StartSession {
                request_id: 7,
                username: String::from("user"),
                session: String::from("kde-wayland"),
                vt: 3,
            },
            &state,
        );

        match response {
            AuthResponse::SessionResult {
                request_id,
                ok,
                exit_code,
                message,
            } => {
                assert_eq!(request_id, 7);
                assert!(!ok);
                assert_eq!(exit_code, None);
                assert_eq!(message, "No recent authentication approval exists for this user.");
            }
            _ => panic!("expected session_result response"),
        }
    }

    #[test]
    fn authenticate_request_rejects_locked_account_marker() {
        let account = Account {
            username: String::from("greeter"),
            password: String::from("!"),
            uid: 101,
            shell: String::from("/usr/bin/ion"),
        };

        assert!(!login_allowed(&account) || !verify_password(&account, "anything"));
    }

    #[test]
    fn power_action_request_rejects_unsupported_action() {
        let state = RuntimeState::default();
        let response = handle_request(
            AuthRequest::PowerAction {
                request_id: 11,
                action: String::from("hibernate"),
            },
            &state,
        );

        match response {
            AuthResponse::PowerResult {
                request_id,
                ok,
                message,
            } => {
                assert_eq!(request_id, 11);
                assert!(!ok);
                assert_eq!(message, "unsupported power action 'hibernate'");
            }
            _ => panic!("expected power_result response"),
        }
    }

    #[test]
    fn handle_connection_returns_error_for_invalid_json() {
        match send_handle_connection_request("not-json") {
            AuthResponse::Error { message } => {
                assert!(message.contains("invalid request:"));
            }
            _ => panic!("expected error response"),
        }
    }

    #[test]
    fn split_account_fields_detects_redox_layout() {
        let (format, parts) = split_account_fields("greeter;101;101;Greeter;/nonexistent;/usr/bin/ion");
        assert_eq!(format, AccountFormat::Redox);
        assert_eq!(parts[0], "greeter");
        assert_eq!(parts[1], "101");
    }

    #[test]
    fn split_account_fields_detects_unix_layout() {
        let (format, parts) = split_account_fields("root:x:0:0:root:/root:/usr/bin/ion");
        assert_eq!(format, AccountFormat::Unix);
        assert_eq!(parts[2], "0");
    }

    #[test]
    fn split_account_fields_keeps_empty_redox_shadow_hash() {
        let (_format, parts) = split_account_fields("greeter;");
        assert_eq!(parts, vec![String::from("greeter"), String::new()]);
    }
}
