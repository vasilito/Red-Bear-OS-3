use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::Path,
};

use serde::Deserialize;

use crate::runtime_state::SharedRuntime;

pub const CONTROL_SOCKET_PATH: &str = "/run/redbear-sessiond-control.sock";

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ControlMessage {
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

fn apply_message(runtime: &SharedRuntime, message: ControlMessage) {
    let Ok(mut runtime) = runtime.write() else {
        eprintln!("redbear-sessiond: runtime state is poisoned");
        return;
    };

    match message {
        ControlMessage::SetSession {
            username,
            uid,
            vt,
            leader,
            state,
        } => {
            runtime.username = username;
            runtime.uid = uid;
            runtime.vt = vt;
            runtime.leader = leader;
            runtime.state = state;
            runtime.active = true;
        }
        ControlMessage::ResetSession { vt } => {
            runtime.username = String::from("root");
            runtime.uid = 0;
            runtime.vt = vt;
            runtime.leader = std::process::id();
            runtime.state = String::from("closing");
            runtime.active = true;
        }
    }
}

pub fn start_control_socket(runtime: SharedRuntime) {
    std::thread::spawn(move || {
        if Path::new(CONTROL_SOCKET_PATH).exists() {
            if let Err(err) = fs::remove_file(CONTROL_SOCKET_PATH) {
                eprintln!("redbear-sessiond: failed to remove stale control socket: {err}");
                return;
            }
        }

        let listener = match UnixListener::bind(CONTROL_SOCKET_PATH) {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("redbear-sessiond: failed to bind control socket: {err}");
                return;
            }
        };

        if let Err(err) = fs::set_permissions(CONTROL_SOCKET_PATH, fs::Permissions::from_mode(0o600)) {
            eprintln!("redbear-sessiond: failed to chmod control socket: {err}");
        }

        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                continue;
            };
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            match serde_json::from_str::<ControlMessage>(line.trim()) {
                Ok(message) => apply_message(&runtime, message),
                Err(err) => eprintln!("redbear-sessiond: invalid control message: {err}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_state::shared_runtime;

    #[test]
    fn set_session_message_updates_runtime_state() {
        let runtime = shared_runtime();

        apply_message(
            &runtime,
            ControlMessage::SetSession {
                username: String::from("user"),
                uid: 1000,
                vt: 7,
                leader: 4242,
                state: String::from("active"),
            },
        );

        let runtime = runtime.read().expect("runtime lock should remain healthy");
        assert_eq!(runtime.username, "user");
        assert_eq!(runtime.uid, 1000);
        assert_eq!(runtime.vt, 7);
        assert_eq!(runtime.leader, 4242);
        assert_eq!(runtime.state, "active");
        assert!(runtime.active);
    }

    #[test]
    fn reset_session_message_restores_root_scaffold() {
        let runtime = shared_runtime();

        apply_message(
            &runtime,
            ControlMessage::SetSession {
                username: String::from("user"),
                uid: 1000,
                vt: 7,
                leader: 4242,
                state: String::from("active"),
            },
        );
        apply_message(&runtime, ControlMessage::ResetSession { vt: 3 });

        let runtime = runtime.read().expect("runtime lock should remain healthy");
        assert_eq!(runtime.username, "root");
        assert_eq!(runtime.uid, 0);
        assert_eq!(runtime.vt, 3);
        assert_eq!(runtime.state, "closing");
        assert!(runtime.active);
    }

    #[test]
    fn control_message_json_matches_expected_shape() {
        let message = serde_json::from_str::<ControlMessage>(
            r#"{"type":"set_session","username":"user","uid":1000,"vt":3,"leader":99,"state":"online"}"#,
        )
        .expect("control message json should parse");

        match message {
            ControlMessage::SetSession {
                username,
                uid,
                vt,
                leader,
                state,
            } => {
                assert_eq!(username, "user");
                assert_eq!(uid, 1000);
                assert_eq!(vt, 3);
                assert_eq!(leader, 99);
                assert_eq!(state, "online");
            }
            ControlMessage::ResetSession { .. } => panic!("expected set_session message"),
        }
    }
}
