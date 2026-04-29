use std::{
    collections::HashSet,
    os::fd::OwnedFd as StdOwnedFd,
    process,
    sync::Mutex,
};

use zbus::{
    fdo,
    interface,
    object_server::SignalEmitter,
    zvariant::{Fd, OwnedFd, OwnedObjectPath},
};

use crate::device_map::DeviceMap;
use crate::runtime_state::SharedRuntime;

#[derive(Debug)]
pub struct LoginSession {
    seat_path: OwnedObjectPath,
    user_path: OwnedObjectPath,
    device_map: DeviceMap,
    runtime: SharedRuntime,
    controlled: Mutex<bool>,
    taken_devices: Mutex<HashSet<(u32, u32)>>,
}

impl LoginSession {
    pub fn new(
        seat_path: OwnedObjectPath,
        user_path: OwnedObjectPath,
        device_map: DeviceMap,
        runtime: SharedRuntime,
    ) -> Self {
        Self {
            seat_path,
            user_path,
            device_map,
            runtime,
            controlled: Mutex::new(false),
            taken_devices: Mutex::new(HashSet::new()),
        }
    }

    fn control_state(&self) -> fdo::Result<std::sync::MutexGuard<'_, bool>> {
        self.controlled
            .lock()
            .map_err(|_| fdo::Error::Failed(String::from("login1 control state is poisoned")))
    }

    fn taken_devices(&self) -> fdo::Result<std::sync::MutexGuard<'_, HashSet<(u32, u32)>>> {
        self.taken_devices
            .lock()
            .map_err(|_| fdo::Error::Failed(String::from("login1 device state is poisoned")))
    }

    fn runtime(&self) -> fdo::Result<crate::runtime_state::SessionRuntime> {
        self.runtime
            .read()
            .map(|runtime| runtime.clone())
            .map_err(|_| fdo::Error::Failed(String::from("login1 runtime state is poisoned")))
    }

    fn runtime_write(&self) -> fdo::Result<std::sync::RwLockWriteGuard<'_, crate::runtime_state::SessionRuntime>> {
        self.runtime
            .write()
            .map_err(|_| fdo::Error::Failed(String::from("login1 runtime state is poisoned")))
    }
}

#[interface(name = "org.freedesktop.login1.Session")]
impl LoginSession {
    fn activate(&self) -> fdo::Result<()> {
        eprintln!("redbear-sessiond: Activate requested for session {}", self.runtime()?.session_id);
        Ok(())
    }

    fn take_control(&self, force: bool) -> fdo::Result<()> {
        let mut controlled = self.control_state()?;
        let runtime = self.runtime()?;
        if *controlled && !force {
            return Err(fdo::Error::Failed(format!(
                "session {} is already under control",
                runtime.session_id
            )));
        }
        *controlled = true;
        eprintln!(
            "redbear-sessiond: TakeControl requested for session {} (force={force})",
            runtime.session_id
        );
        Ok(())
    }

    fn release_control(&self) -> fdo::Result<()> {
        let mut controlled = self.control_state()?;
        *controlled = false;
        self.taken_devices()?.clear();
        eprintln!("redbear-sessiond: ReleaseControl requested for session {}", self.runtime()?.session_id);
        Ok(())
    }

    fn take_device(&self, major: u32, minor: u32) -> fdo::Result<OwnedFd> {
        let runtime = self.runtime()?;
        if !*self.control_state()? {
            return Err(fdo::Error::AccessDenied(format!(
                "session {} must TakeControl before TakeDevice",
                runtime.session_id
            )));
        }

        let mut taken_devices = self.taken_devices()?;
        if taken_devices.contains(&(major, minor)) {
            return Err(fdo::Error::Failed(format!(
                "device ({major}, {minor}) is already taken for session {}",
                runtime.session_id
            )));
        }

        let (path, file) = self
            .device_map
            .open_device(major, minor)
            .map_err(|err| fdo::Error::Failed(format!("TakeDevice({major}, {minor}) failed: {err}")))?;

        taken_devices.insert((major, minor));

        let owned_fd: StdOwnedFd = file.into();
        eprintln!(
            "redbear-sessiond: TakeDevice granted for session {} -> ({major}, {minor}) at {}",
            runtime.session_id, path
        );

        Ok(OwnedFd::from(owned_fd))
    }

    fn release_device(&self, major: u32, minor: u32) -> fdo::Result<()> {
        let runtime = self.runtime()?;
        let mut taken_devices = self.taken_devices()?;
        if !taken_devices.remove(&(major, minor)) {
            return Err(fdo::Error::Failed(format!(
                "device ({major}, {minor}) was not taken for session {}",
                runtime.session_id
            )));
        }
        eprintln!(
            "redbear-sessiond: ReleaseDevice requested for session {} -> ({major}, {minor})",
            runtime.session_id
        );
        Ok(())
    }

    fn pause_device_complete(&self, major: u32, minor: u32) -> fdo::Result<()> {
        eprintln!(
            "redbear-sessiond: PauseDeviceComplete received for session {} -> ({major}, {minor})",
            self.runtime()?.session_id
        );
        Ok(())
    }

    fn set_idle_hint(&self, idle: bool) -> fdo::Result<()> {
        let runtime = self.runtime()?;
        let session_id = runtime.session_id.clone();
        drop(runtime);

        if let Ok(mut guard) = self.runtime.write() {
            guard.idle_hint = idle;
        }
        eprintln!("redbear-sessiond: SetIdleHint({idle}) for session {session_id}");
        Ok(())
    }

    fn set_locked_hint(&self, locked: bool) -> fdo::Result<()> {
        let runtime = self.runtime()?;
        let session_id = runtime.session_id.clone();
        drop(runtime);

        if let Ok(mut guard) = self.runtime.write() {
            guard.locked_hint = locked;
        }
        eprintln!("redbear-sessiond: SetLockedHint({locked}) for session {session_id}");
        Ok(())
    }

    fn set_type(&self, session_type: &str) -> fdo::Result<()> {
        let runtime = self.runtime()?;
        let session_id = runtime.session_id.clone();
        drop(runtime);

        if let Ok(mut guard) = self.runtime.write() {
            guard.session_type = session_type.to_owned();
        }
        eprintln!("redbear-sessiond: SetType({session_type}) for session {session_id}");
        Ok(())
    }

    fn terminate(&self) -> fdo::Result<()> {
        let runtime = self.runtime()?;
        let session_id = runtime.session_id.clone();
        drop(runtime);

        self.runtime_write()?.state = String::from("closing");
        eprintln!("redbear-sessiond: Terminate requested for session {session_id}");
        Ok(())
    }

    fn kill(&self, who: &str, signal_number: i32) -> fdo::Result<()> {
        eprintln!(
            "redbear-sessiond: Kill requested for session {} (who={who}, signal={signal_number}) — no-op",
            self.runtime()?.session_id
        );
        Ok(())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Active")]
    fn active(&self) -> bool {
        self.runtime().map(|runtime| runtime.active).unwrap_or(true)
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Remote")]
    fn remote(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "false"), name = "Type")]
    fn kind(&self) -> String {
        self.runtime()
            .map(|r| r.session_type.clone())
            .unwrap_or_else(|_| String::from("wayland"))
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Class")]
    fn class(&self) -> String {
        String::from("user")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Service")]
    fn service(&self) -> String {
        String::from("redbear")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Desktop")]
    fn desktop(&self) -> String {
        String::from("KDE")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Display")]
    fn display(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Id")]
    fn id(&self) -> String {
        self.runtime().map(|runtime| runtime.session_id).unwrap_or_else(|_| String::from("c1"))
    }

    #[zbus(property(emits_changed_signal = "false"), name = "State")]
    fn state(&self) -> String {
        self.runtime().map(|runtime| runtime.state).unwrap_or_else(|_| String::from("online"))
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Seat")]
    fn seat(&self) -> (String, OwnedObjectPath) {
        (
            self.runtime()
                .map(|runtime| runtime.seat_id)
                .unwrap_or_else(|_| String::from("seat0")),
            self.seat_path.clone(),
        )
    }

    #[zbus(property(emits_changed_signal = "const"), name = "User")]
    fn user(&self) -> (u32, OwnedObjectPath) {
        (
            self.runtime().map(|runtime| runtime.uid).unwrap_or(0),
            self.user_path.clone(),
        )
    }

    #[zbus(property(emits_changed_signal = "const"), name = "VTNr")]
    fn vt_nr(&self) -> u32 {
        self.runtime().map(|runtime| runtime.vt).unwrap_or(3)
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Leader")]
    fn leader(&self) -> u32 {
        self.runtime().map(|runtime| runtime.leader).unwrap_or(process::id())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Audit")]
    fn audit(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "TTY")]
    fn tty(&self) -> String {
        format!("tty{}", self.runtime().map(|runtime| runtime.vt).unwrap_or(3))
    }

    #[zbus(property(emits_changed_signal = "const"), name = "RemoteUser")]
    fn remote_user(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "RemoteHost")]
    fn remote_host(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "false"), name = "IdleHint")]
    fn idle_hint(&self) -> bool {
        self.runtime().map(|r| r.idle_hint).unwrap_or(false)
    }

    #[zbus(property(emits_changed_signal = "false"), name = "LockedHint")]
    fn locked_hint(&self) -> bool {
        self.runtime().map(|r| r.locked_hint).unwrap_or(false)
    }

    #[zbus(signal, name = "PauseDevice")]
    async fn pause_device(
        signal_emitter: &SignalEmitter<'_>,
        major: u32,
        minor: u32,
        kind: String,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ResumeDevice")]
    async fn resume_device(
        signal_emitter: &SignalEmitter<'_>,
        major: u32,
        minor: u32,
        fd: Fd<'_>,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "Lock")]
    async fn lock(signal_emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal, name = "Unlock")]
    async fn unlock(signal_emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_map::DeviceMap;
    use crate::runtime_state::shared_runtime;

    fn test_session() -> LoginSession {
        LoginSession::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            DeviceMap::new(),
            shared_runtime(),
        )
    }

    #[test]
    fn set_idle_hint_updates_runtime() {
        let runtime = shared_runtime();
        let session = LoginSession::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            DeviceMap::new(),
            runtime.clone(),
        );

        assert!(!session.idle_hint());
        session.set_idle_hint(true).unwrap();
        assert!(session.idle_hint());

        let guard = runtime.read().expect("lock");
        assert!(guard.idle_hint);
    }

    #[test]
    fn set_locked_hint_updates_runtime() {
        let session = test_session();
        assert!(!session.locked_hint());
        session.set_locked_hint(true).unwrap();
        assert!(session.locked_hint());
    }

    #[test]
    fn set_type_updates_runtime() {
        let runtime = shared_runtime();
        let session = LoginSession::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            DeviceMap::new(),
            runtime.clone(),
        );

        assert_eq!(session.kind(), "wayland");
        session.set_type("x11").unwrap();
        assert_eq!(session.kind(), "x11");

        let guard = runtime.read().expect("lock");
        assert_eq!(guard.session_type, "x11");
    }

    #[test]
    fn terminate_sets_state_to_closing() {
        let runtime = shared_runtime();
        let session = LoginSession::new(
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/seat/seat0")).unwrap(),
            OwnedObjectPath::try_from(String::from("/org/freedesktop/login1/user/current")).unwrap(),
            DeviceMap::new(),
            runtime.clone(),
        );

        assert_eq!(session.state(), "online");
        session.terminate().unwrap();
        assert_eq!(session.state(), "closing");
    }

    #[test]
    fn take_control_then_take_device_rejects_duplicate() {
        let session = test_session();
        session.take_control(false).unwrap();
        session.taken_devices().unwrap().insert((226, 0));
        let err = session.take_device(226, 0).unwrap_err();
        match err {
            fdo::Error::Failed(msg) => assert!(msg.contains("already taken")),
            other => panic!("expected Failed error, got {other:?}"),
        }
    }

    #[test]
    fn release_device_rejects_unknown() {
        let session = test_session();
        session.take_control(false).unwrap();
        let err = session.release_device(226, 99).unwrap_err();
        match err {
            fdo::Error::Failed(msg) => assert!(msg.contains("was not taken")),
            other => panic!("expected Failed error, got {other:?}"),
        }
    }
}
