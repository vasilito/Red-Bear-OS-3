use std::sync::{Arc, RwLock};

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
        }
    }
}

pub type SharedRuntime = Arc<RwLock<SessionRuntime>>;

pub fn shared_runtime() -> SharedRuntime {
    Arc::new(RwLock::new(SessionRuntime::default()))
}
