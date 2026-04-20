use zbus::{
    fdo,
    interface,
    object_server::SignalEmitter,
    zvariant::OwnedObjectPath,
};

use crate::runtime_state::SharedRuntime;

#[derive(Clone, Debug)]
pub struct LoginManager {
    runtime: SharedRuntime,
    session_path: OwnedObjectPath,
    seat_path: OwnedObjectPath,
}

impl LoginManager {
    pub fn new(session_path: OwnedObjectPath, seat_path: OwnedObjectPath, runtime: SharedRuntime) -> Self {
        Self {
            runtime,
            session_path,
            seat_path,
        }
    }
}

#[interface(name = "org.freedesktop.login1.Manager")]
impl LoginManager {
    fn get_session(&self, id: &str) -> fdo::Result<OwnedObjectPath> {
        let runtime = self
            .runtime
            .read()
            .map_err(|_| fdo::Error::Failed(String::from("login1 runtime state is poisoned")))?;
        if id == runtime.session_id || id == "auto" {
            return Ok(self.session_path.clone());
        }

        Err(fdo::Error::Failed(format!("unknown login1 session '{id}'")))
    }

    fn list_sessions(&self) -> fdo::Result<Vec<(String, u32, String, String, OwnedObjectPath)>> {
        let runtime = self
            .runtime
            .read()
            .map_err(|_| fdo::Error::Failed(String::from("login1 runtime state is poisoned")))?;
        Ok(vec![(
            runtime.session_id.clone(),
            runtime.uid,
            runtime.username.clone(),
            runtime.seat_id.clone(),
            self.session_path.clone(),
        )])
    }

    fn get_seat(&self, id: &str) -> fdo::Result<OwnedObjectPath> {
        let runtime = self
            .runtime
            .read()
            .map_err(|_| fdo::Error::Failed(String::from("login1 runtime state is poisoned")))?;
        if id == runtime.seat_id {
            return Ok(self.seat_path.clone());
        }

        Err(fdo::Error::Failed(format!("unknown login1 seat '{id}'")))
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IdleHint")]
    fn idle_hint(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IdleSinceHint")]
    fn idle_since_hint(&self) -> u64 {
        0
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IdleSinceHintMonotonic")]
    fn idle_since_hint_monotonic(&self) -> u64 {
        0
    }

    #[zbus(property(emits_changed_signal = "const"), name = "BlockInhibited")]
    fn block_inhibited(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "DelayInhibited")]
    fn delay_inhibited(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "InhibitDelayMaxUSec")]
    fn inhibit_delay_max_usec(&self) -> u64 {
        0
    }

    #[zbus(property(emits_changed_signal = "const"), name = "HandleLidSwitch")]
    fn handle_lid_switch(&self) -> String {
        String::from("ignore")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "HandlePowerKey")]
    fn handle_power_key(&self) -> String {
        String::from("poweroff")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "PreparingForSleep")]
    fn preparing_for_sleep(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"), name = "PreparingForShutdown")]
    fn preparing_for_shutdown(&self) -> bool {
        self.runtime
            .read()
            .map(|runtime| runtime.preparing_for_shutdown)
            .unwrap_or(false)
    }

    #[zbus(signal, name = "PrepareForSleep")]
    async fn prepare_for_sleep(signal_emitter: &SignalEmitter<'_>, before: bool) -> zbus::Result<()>;

    #[zbus(signal, name = "PrepareForShutdown")]
    async fn prepare_for_shutdown(
        signal_emitter: &SignalEmitter<'_>,
        before: bool,
    ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_state::shared_runtime;

    #[test]
    fn get_session_accepts_runtime_session_id() {
        let manager = LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1"))
                .expect("session path should parse"),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0"))
                .expect("seat path should parse"),
            shared_runtime(),
        );

        let path = manager
            .get_session("c1")
            .expect("runtime session id should resolve");
        assert_eq!(path.as_str(), "/org/freedesktop/login1/session/c1");
    }

    #[test]
    fn get_session_accepts_auto_alias() {
        let manager = LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1"))
                .expect("session path should parse"),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0"))
                .expect("seat path should parse"),
            shared_runtime(),
        );

        let path = manager
            .get_session("auto")
            .expect("auto alias should resolve to current session");
        assert_eq!(path.as_str(), "/org/freedesktop/login1/session/c1");
    }

    #[test]
    fn preparing_for_shutdown_reflects_runtime_state() {
        let runtime = shared_runtime();
        runtime
            .write()
            .expect("runtime lock should be writable")
            .preparing_for_shutdown = true;

        let manager = LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1"))
                .expect("session path should parse"),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0"))
                .expect("seat path should parse"),
            runtime,
        );

        assert!(manager.preparing_for_shutdown());
    }
}
