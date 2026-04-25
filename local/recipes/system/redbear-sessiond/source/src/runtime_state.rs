use std::sync::{Arc, RwLock};

#[derive(Clone, Debug)]
pub struct InhibitorEntry {
    pub what: String,
    pub who: String,
    pub why: String,
    pub mode: String,
    pub pid: u32,
    pub uid: u32,
}

#[derive(Clone, Debug)]
pub struct SessionRuntime {
    pub session_id: String,
    pub seat_id: String,
    pub username: String,
    pub uid: u32,
    pub vt: u32,
    pub leader: u32,
    pub state: String,
    pub active: bool,
    pub preparing_for_shutdown: bool,
    pub idle_hint: bool,
    pub locked_hint: bool,
    pub session_type: String,
    pub inhibitors: Vec<InhibitorEntry>,
}

impl Default for SessionRuntime {
    fn default() -> Self {
        Self {
            session_id: String::from("c1"),
            seat_id: String::from("seat0"),
            username: String::from("root"),
            uid: 0,
            vt: 3,
            leader: std::process::id(),
            state: String::from("online"),
            active: true,
            preparing_for_shutdown: false,
            idle_hint: false,
            locked_hint: false,
            session_type: String::from("wayland"),
            inhibitors: Vec::new(),
        }
    }
}

pub type SharedRuntime = Arc<RwLock<SessionRuntime>>;

pub fn shared_runtime() -> SharedRuntime {
    Arc::new(RwLock::new(SessionRuntime::default()))
}
