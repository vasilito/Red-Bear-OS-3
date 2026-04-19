use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    process,
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

const PROGRAM: &str = "redbear-greeter-check";
const USAGE: &str = "Usage: redbear-greeter-check [--invalid USER PASSWORD | --valid USER PASSWORD]\n\nQuery the installed Red Bear greeter surface inside the guest.";
const GREETER_SOCKET: &str = "/run/redbear-greeterd.sock";
const GREETERD_BIN: &str = "/usr/bin/redbear-greeterd";
const GREETER_UI_BIN: &str = "/usr/bin/redbear-greeter-ui";
const AUTHD_BIN: &str = "/usr/bin/redbear-authd";
const SESSION_LAUNCH_BIN: &str = "/usr/bin/redbear-session-launch";
const GREETER_BACKGROUND: &str = "/usr/share/redbear/greeter/background.png";
const GREETER_ICON: &str = "/usr/share/redbear/greeter/icon.png";
const AUTHD_SERVICE: &str = "/usr/lib/init.d/19_redbear-authd.service";
const DISPLAY_SHIM_SERVICE: &str = "/usr/lib/init.d/20_display.service";
const GREETER_SERVICE: &str = "/usr/lib/init.d/20_greeter.service";
const ACTIVATE_CONSOLE_SERVICE: &str = "/usr/lib/init.d/29_activate_console.service";
const CONSOLE_SERVICE: &str = "/usr/lib/init.d/30_console.service";
const DEBUG_CONSOLE_SERVICE: &str = "/usr/lib/init.d/31_debug_console.service";
const VALIDATION_REQUEST: &str = "/run/redbear-kde-session.validation-request";
const VALIDATION_SUCCESS: &str = "/run/redbear-kde-session.validation-success";

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Request<'a> {
    Hello { version: u32 },
    SubmitLogin { username: &'a str, password: &'a str },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    HelloOk {
        background: String,
        icon: String,
        session_name: String,
        state: String,
        message: String,
    },
    LoginResult {
        ok: bool,
        state: String,
        message: String,
    },
    Error {
        message: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Status,
    Invalid { username: String, password: String },
    Valid { username: String, password: String },
}

fn parse_mode_from_args<I>(args: I) -> Result<Mode, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    match args.next() {
        None => Ok(Mode::Status),
        Some(flag) if flag == "--help" || flag == "-h" => Err(String::new()),
        Some(flag) if flag == "--invalid" => {
            let username = args.next().ok_or_else(|| String::from("missing username after --invalid"))?;
            let password = args.next().ok_or_else(|| String::from("missing password after --invalid"))?;
            if args.next().is_some() {
                return Err(String::from("unexpected extra arguments after --invalid USER PASSWORD"));
            }
            Ok(Mode::Invalid { username, password })
        }
        Some(flag) if flag == "--valid" => {
            let username = args.next().ok_or_else(|| String::from("missing username after --valid"))?;
            let password = args.next().ok_or_else(|| String::from("missing password after --valid"))?;
            if args.next().is_some() {
                return Err(String::from("unexpected extra arguments after --valid USER PASSWORD"));
            }
            Ok(Mode::Valid { username, password })
        }
        Some(other) => Err(format!("unsupported argument '{other}'")),
    }
}

fn parse_mode() -> Result<Mode, String> {
    parse_mode_from_args(std::env::args().skip(1))
}

fn send_request(request: &Request<'_>) -> Result<Response, String> {
    let mut stream = UnixStream::connect(GREETER_SOCKET)
        .map_err(|err| format!("failed to connect to {GREETER_SOCKET}: {err}"))?;
    let payload = serde_json::to_string(request)
        .map_err(|err| format!("failed to serialize greeter request: {err}"))?;
    stream
        .write_all(payload.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|err| format!("failed to write greeter request: {err}"))?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|err| format!("failed to read greeter response: {err}"))?;
    serde_json::from_str(line.trim()).map_err(|err| format!("failed to parse greeter response: {err}"))
}

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("{path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn wait_for_validation_marker(path: &str, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() <= timeout {
        if Path::new(path).exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }

    Err(format!("timed out waiting for {path}"))
}

fn wait_for_greeter_ready(timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() <= timeout {
        match send_request(&Request::Hello { version: 1 }) {
            Ok(Response::HelloOk { state, message, .. }) if state == "greeter_ready" => {
                println!("GREETER_VALID_READY_MESSAGE={message}");
                return Ok(());
            }
            Ok(_) => {}
            Err(_) => {}
        }
        thread::sleep(Duration::from_millis(250));
    }

    Err(String::from("timed out waiting for greeter to return to greeter_ready"))
}

fn run_status() -> Result<(), String> {
    println!("=== Red Bear Greeter Runtime Check ===");
    require_path(GREETERD_BIN)?;
    require_path(GREETER_UI_BIN)?;
    require_path(AUTHD_BIN)?;
    require_path(SESSION_LAUNCH_BIN)?;
    require_path(GREETER_BACKGROUND)?;
    require_path(GREETER_ICON)?;
    require_path(AUTHD_SERVICE)?;
    require_path(DISPLAY_SHIM_SERVICE)?;
    require_path(GREETER_SERVICE)?;
    require_path(ACTIVATE_CONSOLE_SERVICE)?;
    require_path(CONSOLE_SERVICE)?;
    require_path(DEBUG_CONSOLE_SERVICE)?;
    require_path(GREETER_SOCKET)?;

    match send_request(&Request::Hello { version: 1 })? {
        Response::HelloOk {
            background,
            icon,
            session_name,
            state,
            message,
        } => {
            println!("GREETER_BACKGROUND={background}");
            println!("GREETER_ICON={icon}");
            println!("GREETER_SESSION={session_name}");
            println!("GREETER_STATE={state}");
            println!("GREETER_MESSAGE={message}");
            println!("GREETER_HELLO=ok");
            Ok(())
        }
        Response::Error { message } => Err(format!("greeter hello failed: {message}")),
        Response::Other => Err(String::from("unexpected greeter hello response")),
        Response::LoginResult { .. } => Err(String::from("unexpected login result when greeting greeter")),
    }
}

fn run_invalid(username: &str, password: &str) -> Result<(), String> {
    match send_request(&Request::SubmitLogin { username, password })? {
        Response::LoginResult { ok, state, message } => {
            println!("GREETER_INVALID_STATE={state}");
            println!("GREETER_INVALID_MESSAGE={message}");
            if ok {
                Err(String::from("invalid login unexpectedly succeeded"))
            } else {
                println!("GREETER_INVALID=ok");
                Ok(())
            }
        }
        Response::Error { message } => Err(format!("invalid-login request failed: {message}")),
        Response::Other => Err(String::from("unexpected greeter response for invalid login")),
        Response::HelloOk { .. } => Err(String::from("unexpected hello response for invalid login")),
    }
}

fn run_valid(username: &str, password: &str) -> Result<(), String> {
    let _ = fs::remove_file(VALIDATION_REQUEST);
    let _ = fs::remove_file(VALIDATION_SUCCESS);
    fs::write(VALIDATION_REQUEST, b"bounded-session\n")
        .map_err(|err| format!("failed to create validation request: {err}"))?;

    match send_request(&Request::SubmitLogin { username, password })? {
        Response::LoginResult { ok, state, message } => {
            println!("GREETER_VALID_STATE={state}");
            println!("GREETER_VALID_MESSAGE={message}");
            if !ok {
                let _ = fs::remove_file(VALIDATION_REQUEST);
                return Err(String::from("valid login unexpectedly failed"));
            }
        }
        Response::Error { message } => {
            let _ = fs::remove_file(VALIDATION_REQUEST);
            return Err(format!("valid-login request failed: {message}"));
        }
        Response::Other => {
            let _ = fs::remove_file(VALIDATION_REQUEST);
            return Err(String::from("unexpected greeter response for valid login"));
        }
        Response::HelloOk { .. } => {
            let _ = fs::remove_file(VALIDATION_REQUEST);
            return Err(String::from("unexpected hello response for valid login"));
        }
    }

    wait_for_validation_marker(VALIDATION_SUCCESS, Duration::from_secs(30))?;
    println!("GREETER_VALID_SESSION=started");
    wait_for_greeter_ready(Duration::from_secs(30))?;

    let _ = fs::remove_file(VALIDATION_REQUEST);
    let _ = fs::remove_file(VALIDATION_SUCCESS);
    println!("GREETER_VALID=ok");
    Ok(())
}

fn main() {
    let mode = match parse_mode() {
        Ok(mode) => mode,
        Err(err) if err.is_empty() => {
            println!("{USAGE}");
            process::exit(0);
        }
        Err(err) => {
            eprintln!("{PROGRAM}: {err}");
            eprintln!("{USAGE}");
            process::exit(1);
        }
    };

    let result = match mode {
        Mode::Status => run_status(),
        Mode::Invalid { username, password } => run_invalid(&username, &password),
        Mode::Valid { username, password } => run_valid(&username, &password),
    };

    if let Err(err) = result {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_defaults_to_status() {
        assert_eq!(parse_mode_from_args(Vec::<String>::new()).expect("status mode should parse"), Mode::Status);
    }

    #[test]
    fn parse_mode_accepts_invalid_login_arguments() {
        assert_eq!(
            parse_mode_from_args(vec![
                String::from("--invalid"),
                String::from("alice"),
                String::from("wrong"),
            ])
            .expect("invalid-login mode should parse"),
            Mode::Invalid {
                username: String::from("alice"),
                password: String::from("wrong"),
            }
        );
    }

    #[test]
    fn parse_mode_accepts_valid_login_arguments() {
        assert_eq!(
            parse_mode_from_args(vec![
                String::from("--valid"),
                String::from("alice"),
                String::from("password"),
            ])
            .expect("valid-login mode should parse"),
            Mode::Valid {
                username: String::from("alice"),
                password: String::from("password"),
            }
        );
    }

    #[test]
    fn parse_mode_rejects_extra_valid_arguments() {
        assert_eq!(
            parse_mode_from_args(vec![
                String::from("--valid"),
                String::from("alice"),
                String::from("password"),
                String::from("extra"),
            ]),
            Err(String::from("unexpected extra arguments after --valid USER PASSWORD"))
        );
    }

    #[test]
    fn parse_mode_rejects_extra_invalid_arguments() {
        assert_eq!(
            parse_mode_from_args(vec![
                String::from("--invalid"),
                String::from("alice"),
                String::from("wrong"),
                String::from("extra"),
            ]),
            Err(String::from("unexpected extra arguments after --invalid USER PASSWORD"))
        );
    }

    #[test]
    fn parse_mode_rejects_unknown_flags() {
        assert_eq!(
            parse_mode_from_args(vec![String::from("--bogus")]),
            Err(String::from("unsupported argument '--bogus'"))
        );
    }
}
