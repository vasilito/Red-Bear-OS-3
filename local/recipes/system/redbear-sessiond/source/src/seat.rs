use std::sync::Mutex;

use zbus::{fdo, interface, zvariant::OwnedObjectPath};

#[derive(Debug)]
pub struct LoginSeat {
    id: String,
    session_id: String,
    session_path: OwnedObjectPath,
    last_requested_vt: Mutex<u32>,
}

impl LoginSeat {
    pub fn new(session_path: OwnedObjectPath) -> Self {
        Self {
            id: String::from("seat0"),
            session_id: String::from("c1"),
            session_path,
            last_requested_vt: Mutex::new(1),
        }
    }

    fn last_requested_vt(&self) -> fdo::Result<std::sync::MutexGuard<'_, u32>> {
        self.last_requested_vt
            .lock()
            .map_err(|_| fdo::Error::Failed(String::from("seat VT state is poisoned")))
    }
}

#[interface(name = "org.freedesktop.login1.Seat")]
impl LoginSeat {
    fn switch_to(&mut self, vt: u32) -> fdo::Result<()> {
        let mut last_requested_vt = self.last_requested_vt()?;
        *last_requested_vt = vt;
        eprintln!(
            "redbear-sessiond: SwitchTo requested for seat {} -> vt {vt} (delegated to inputd -A externally)",
            self.id
        );
        Ok(())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Id")]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "ActiveSession")]
    fn active_session(&self) -> (String, OwnedObjectPath) {
        (self.session_id.clone(), self.session_path.clone())
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Sessions")]
    fn sessions(&self) -> Vec<(String, OwnedObjectPath)> {
        vec![(self.session_id.clone(), self.session_path.clone())]
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
