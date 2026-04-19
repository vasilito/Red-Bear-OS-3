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
        if id == runtime.session_id {
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
        false
    }

    #[zbus(signal, name = "PrepareForSleep")]
    async fn prepare_for_sleep(signal_emitter: &SignalEmitter<'_>, before: bool) -> zbus::Result<()>;

    #[zbus(signal, name = "PrepareForShutdown")]
    async fn prepare_for_shutdown(
        signal_emitter: &SignalEmitter<'_>,
        before: bool,
    ) -> zbus::Result<()>;
}
