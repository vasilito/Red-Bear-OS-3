use std::{process::Command, sync::Mutex};

use zbus::{fdo, interface, zvariant::OwnedObjectPath};

use crate::runtime_state::SharedRuntime;

#[derive(Debug)]
pub struct LoginSeat {
    id: String,
    session_path: OwnedObjectPath,
    runtime: SharedRuntime,
    last_requested_vt: Mutex<u32>,
}

impl LoginSeat {
    pub fn new(session_path: OwnedObjectPath, runtime: SharedRuntime) -> Self {
        Self {
            id: String::from("seat0"),
            session_path,
            runtime,
            last_requested_vt: Mutex::new(1),
        }
    }

    fn last_requested_vt(&self) -> fdo::Result<std::sync::MutexGuard<'_, u32>> {
        self.last_requested_vt
            .lock()
            .map_err(|_| fdo::Error::Failed(String::from("seat VT state is poisoned")))
    }

    fn request_vt_switch(program: &str, vt: u32) -> fdo::Result<()> {
        let output = Command::new(program)
            .args(["-A", &vt.to_string()])
            .output()
            .map_err(|err| fdo::Error::Failed(format!("failed to run {program} -A {vt}: {err}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else if !stdout.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                format!("exit status {}", output.status)
            };
            return Err(fdo::Error::Failed(format!(
                "{program} -A {vt} failed: {detail}"
            )));
        }

        Ok(())
    }

    fn current_session_id(&self) -> String {
        self.runtime
            .read()
            .map(|runtime| runtime.session_id.clone())
            .unwrap_or_else(|_| String::from("c1"))
    }
}

#[interface(name = "org.freedesktop.login1.Seat")]
impl LoginSeat {
    fn switch_to(&mut self, vt: u32) -> fdo::Result<()> {
        Self::request_vt_switch("inputd", vt)?;

        let mut last_requested_vt = self.last_requested_vt()?;
        *last_requested_vt = vt;

        let mut runtime = self
            .runtime
            .write()
            .map_err(|_| fdo::Error::Failed(String::from("seat runtime state is poisoned")))?;
        runtime.vt = vt;
        runtime.active = true;

        eprintln!("redbear-sessiond: SwitchTo requested for seat {} -> vt {vt}", self.id);
        Ok(())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Id")]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "ActiveSession")]
    fn active_session(&self) -> (String, OwnedObjectPath) {
        (self.current_session_id(), self.session_path.clone())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Sessions")]
    fn sessions(&self) -> Vec<(String, OwnedObjectPath)> {
        vec![(self.current_session_id(), self.session_path.clone())]
    }

    #[zbus(property(emits_changed_signal = "const"), name = "CanGraphical")]
    fn can_graphical(&self) -> bool {
        true
    }

    #[zbus(property(emits_changed_signal = "const"), name = "CanTTY")]
    fn can_tty(&self) -> bool {
        true
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IdleHint")]
    fn idle_hint(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_state::shared_runtime;

    #[test]
    fn request_vt_switch_accepts_successful_command() {
        LoginSeat::request_vt_switch("/bin/true", 3).expect("true should succeed");
    }

    #[test]
    fn request_vt_switch_rejects_failed_command() {
        let err = LoginSeat::request_vt_switch("/bin/false", 3).expect_err("false should fail");
        match err {
            fdo::Error::Failed(message) => {
                assert!(message.contains("/bin/false -A 3 failed"));
            }
            other => panic!("expected failed error, got {other:?}"),
        }
    }

    #[test]
    fn active_session_reflects_runtime_vt_after_update() {
        let session_path = OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1"))
            .expect("session path should parse");
        let runtime = shared_runtime();
        {
            let mut guard = runtime.write().expect("runtime lock should remain healthy");
            guard.vt = 7;
        }
        let seat = LoginSeat::new(session_path, runtime);
        assert_eq!(seat.active_session().0, "c1");
        assert_eq!(seat.id(), "seat0");
    }
}
