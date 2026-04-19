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

    #[zbus(property(emits_changed_signal = "const"), name = "Active")]
    fn active(&self) -> bool {
        self.runtime().map(|runtime| runtime.active).unwrap_or(true)
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Remote")]
    fn remote(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Type")]
    fn kind(&self) -> String {
        String::from("wayland")
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

    #[zbus(property(emits_changed_signal = "const"), name = "State")]
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

    #[zbus(property(emits_changed_signal = "const"), name = "IdleHint")]
    fn idle_hint(&self) -> bool {
        false
    }

    #[zbus(property(emits_changed_signal = "const"), name = "LockedHint")]
    fn locked_hint(&self) -> bool {
        false
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
}
