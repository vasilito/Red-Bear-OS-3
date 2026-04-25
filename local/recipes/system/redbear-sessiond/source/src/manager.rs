use std::{
    os::fd::OwnedFd as StdOwnedFd,
    os::unix::net::UnixStream,
    sync::{Arc, Mutex},
};

use zbus::{
    fdo,
    interface,
    object_server::SignalEmitter,
    zvariant::{OwnedFd, OwnedObjectPath},
};

use crate::runtime_state::{InhibitorEntry, SharedRuntime};

#[derive(Clone, Debug)]
pub struct LoginManager {
    runtime: SharedRuntime,
    session_path: OwnedObjectPath,
    seat_path: OwnedObjectPath,
    user_path: OwnedObjectPath,
    inhibitor_fds: Arc<Mutex<Vec<StdOwnedFd>>>,
}

impl LoginManager {
    pub fn new(
        session_path: OwnedObjectPath,
        seat_path: OwnedObjectPath,
        user_path: OwnedObjectPath,
        runtime: SharedRuntime,
    ) -> Self {
        Self {
            runtime,
            session_path,
            seat_path,
            user_path,
            inhibitor_fds: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn runtime_read(&self) -> fdo::Result<std::sync::RwLockReadGuard<'_, crate::runtime_state::SessionRuntime>> {
        self.runtime
            .read()
            .map_err(|_| fdo::Error::Failed(String::from("login1 runtime state is poisoned")))
    }

}

#[interface(name = "org.freedesktop.login1.Manager")]
impl LoginManager {
    fn get_session(&self, id: &str) -> fdo::Result<OwnedObjectPath> {
        let runtime = self.runtime_read()?;
        if id == runtime.session_id || id == "auto" {
            return Ok(self.session_path.clone());
        }

        Err(fdo::Error::Failed(format!("unknown login1 session '{id}'")))
    }

    fn list_sessions(&self) -> fdo::Result<Vec<(String, u32, String, String, OwnedObjectPath)>> {
        let runtime = self.runtime_read()?;
        Ok(vec![(
            runtime.session_id.clone(),
            runtime.uid,
            runtime.username.clone(),
            runtime.seat_id.clone(),
            self.session_path.clone(),
        )])
    }

    fn get_seat(&self, id: &str) -> fdo::Result<OwnedObjectPath> {
        let runtime = self.runtime_read()?;
        if id == runtime.seat_id {
            return Ok(self.seat_path.clone());
        }

        Err(fdo::Error::Failed(format!("unknown login1 seat '{id}'")))
    }

    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> fdo::Result<OwnedFd> {
        if mode != "block" && mode != "delay" {
            return Err(fdo::Error::Failed(format!(
                "inhibit mode must be 'block' or 'delay', got '{mode}'"
            )));
        }

        let (end_caller, end_daemon) = UnixStream::pair()
            .map_err(|err| fdo::Error::Failed(format!("failed to create inhibit pipe: {err}")))?;

        let fd_caller: StdOwnedFd = end_caller.into();
        let fd_daemon: StdOwnedFd = end_daemon.into();

        let uid = self.runtime_read().map(|r| r.uid).unwrap_or(0);
        let pid = std::process::id();

        let entry = InhibitorEntry {
            what: what.to_owned(),
            who: who.to_owned(),
            why: why.to_owned(),
            mode: mode.to_owned(),
            pid,
            uid,
        };

        if let Ok(mut runtime) = self.runtime.write() {
            runtime.inhibitors.push(entry);
        }

        if let Ok(mut fds) = self.inhibitor_fds.lock() {
            fds.push(fd_daemon);
        }

        eprintln!(
            "redbear-sessiond: Inhibit(what={what}, who={who}, mode={mode}) granted"
        );

        Ok(OwnedFd::from(fd_caller))
    }

    fn can_power_off(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn can_reboot(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn can_suspend(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn can_hibernate(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn can_hybrid_sleep(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn can_suspend_then_hibernate(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn can_sleep(&self) -> fdo::Result<String> {
        Ok(String::from("na"))
    }

    fn power_off(&self, _interactive: bool) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: PowerOff requested");
        if let Ok(mut runtime) = self.runtime.write() {
            runtime.preparing_for_shutdown = true;
        }
        Ok(())
    }

    fn reboot(&self, _interactive: bool) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: Reboot requested");
        Ok(())
    }

    fn suspend(&self, _interactive: bool) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: Suspend requested");
        Ok(())
    }

    fn get_session_by_pid(&self, _pid: u32) -> fdo::Result<OwnedObjectPath> {
        Ok(self.session_path.clone())
    }

    fn list_users(&self) -> fdo::Result<Vec<(u32, String, OwnedObjectPath)>> {
        let runtime = self.runtime_read()?;
        Ok(vec![(
            runtime.uid,
            runtime.username.clone(),
            self.user_path.clone(),
        )])
    }

    fn list_seats(&self) -> fdo::Result<Vec<(String, OwnedObjectPath)>> {
        let runtime = self.runtime_read()?;
        Ok(vec![(runtime.seat_id.clone(), self.seat_path.clone())])
    }

    fn list_inhibitors(&self) -> fdo::Result<Vec<(String, String, String, String, u32, u32)>> {
        let runtime = self.runtime_read()?;
        Ok(runtime
            .inhibitors
            .iter()
            .map(|entry| {
                (
                    entry.what.clone(),
                    entry.who.clone(),
                    entry.why.clone(),
                    entry.mode.clone(),
                    entry.pid,
                    entry.uid,
                )
            })
            .collect())
    }

    fn activate_session(&self, session_id: &str) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: ActivateSession({session_id}) — no-op");
        Ok(())
    }

    fn lock_session(&self, session_id: &str) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: LockSession({session_id})");
        Ok(())
    }

    fn unlock_session(&self, session_id: &str) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: UnlockSession({session_id})");
        Ok(())
    }

    fn terminate_session(&self, session_id: &str) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: TerminateSession({session_id})");
        Ok(())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "IdleHint")]
    fn idle_hint(&self) -> bool {
        self.runtime_read().map(|r| r.idle_hint).unwrap_or(false)
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
        self.runtime_read()
            .map(|r| {
                r.inhibitors
                    .iter()
                    .filter(|i| i.mode == "block")
                    .map(|i| i.what.as_str())
                    .collect::<Vec<&str>>()
                    .join(":")
            })
            .unwrap_or_default()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "DelayInhibited")]
    fn delay_inhibited(&self) -> String {
        self.runtime_read()
            .map(|r| {
                r.inhibitors
                    .iter()
                    .filter(|i| i.mode == "delay")
                    .map(|i| i.what.as_str())
                    .collect::<Vec<&str>>()
                    .join(":")
            })
            .unwrap_or_default()
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
        self.runtime_read()
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

    fn test_manager() -> LoginManager {
        LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1"))
                .expect("session path should parse"),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0"))
                .expect("seat path should parse"),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current"))
                .expect("user path should parse"),
            shared_runtime(),
        )
    }

    #[test]
    fn get_session_accepts_runtime_session_id() {
        let manager = test_manager();

        let path = manager
            .get_session("c1")
            .expect("runtime session id should resolve");
        assert_eq!(path.as_str(), "/org/freedesktop/login1/session/c1");
    }

    #[test]
    fn get_session_accepts_auto_alias() {
        let manager = test_manager();

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
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current"))
                .expect("user path should parse"),
            runtime,
        );

        assert!(manager.preparing_for_shutdown());
    }

    #[test]
    fn can_methods_return_na() {
        let manager = test_manager();
        assert_eq!(manager.can_power_off().unwrap(), "na");
        assert_eq!(manager.can_reboot().unwrap(), "na");
        assert_eq!(manager.can_suspend().unwrap(), "na");
        assert_eq!(manager.can_hibernate().unwrap(), "na");
        assert_eq!(manager.can_hybrid_sleep().unwrap(), "na");
        assert_eq!(manager.can_suspend_then_hibernate().unwrap(), "na");
        assert_eq!(manager.can_sleep().unwrap(), "na");
    }

    #[test]
    fn list_users_returns_runtime_user() {
        let runtime = shared_runtime();
        runtime.write().expect("lock").username = String::from("testuser");
        runtime.write().expect("lock").uid = 1000;

        let manager = LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            runtime,
        );

        let users = manager.list_users().expect("list_users should succeed");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].0, 1000);
        assert_eq!(users[0].1, "testuser");
    }

    #[test]
    fn list_seats_returns_runtime_seat() {
        let manager = test_manager();
        let seats = manager.list_seats().expect("list_seats should succeed");
        assert_eq!(seats.len(), 1);
        assert_eq!(seats[0].0, "seat0");
    }

    #[test]
    fn get_session_by_pid_returns_session_path() {
        let manager = test_manager();
        let path = manager.get_session_by_pid(1234).expect("should succeed");
        assert_eq!(path.as_str(), "/org/freedesktop/login1/session/c1");
    }

    #[test]
    fn list_inhibitors_empty_by_default() {
        let manager = test_manager();
        let inhibitors = manager.list_inhibitors().expect("should succeed");
        assert!(inhibitors.is_empty());
    }

    #[test]
    fn inhibit_rejects_invalid_mode() {
        let manager = test_manager();
        let err = manager.inhibit("sleep", "test", "reason", "invalid").unwrap_err();
        match err {
            fdo::Error::Failed(msg) => assert!(msg.contains("block") || msg.contains("delay")),
            other => panic!("expected Failed error, got {other:?}"),
        }
    }

    #[test]
    fn inhibit_tracks_entry_in_runtime() {
        let runtime = shared_runtime();
        let manager = LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            runtime.clone(),
        );

        let _fd = manager
            .inhibit("sleep", "testapp", "testing", "block")
            .expect("inhibit should succeed");

        let runtime_guard = runtime.read().expect("lock");
        assert_eq!(runtime_guard.inhibitors.len(), 1);
        assert_eq!(runtime_guard.inhibitors[0].what, "sleep");
        assert_eq!(runtime_guard.inhibitors[0].who, "testapp");
        assert_eq!(runtime_guard.inhibitors[0].mode, "block");
    }

    #[test]
    fn block_inhibited_joins_what_fields() {
        let runtime = shared_runtime();
        runtime.write().expect("lock").inhibitors.push(InhibitorEntry {
            what: String::from("sleep"),
            who: String::from("app1"),
            why: String::from("r"),
            mode: String::from("block"),
            pid: 1,
            uid: 0,
        });
        runtime.write().expect("lock").inhibitors.push(InhibitorEntry {
            what: String::from("shutdown"),
            who: String::from("app2"),
            why: String::from("r"),
            mode: String::from("block"),
            pid: 2,
            uid: 0,
        });

        let manager = LoginManager::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/session/c1")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            runtime,
        );

        let blocked = manager.block_inhibited();
        assert!(blocked.contains("sleep"));
        assert!(blocked.contains("shutdown"));
    }
}
