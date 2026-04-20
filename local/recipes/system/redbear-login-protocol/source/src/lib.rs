use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GreeterRequest {
    Hello { version: u32 },
    SubmitLogin { username: String, password: String },
    RequestShutdown,
    RequestReboot,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GreeterResponse {
    HelloOk {
        background: String,
        icon: String,
        session_name: String,
        state: String,
        message: String,
    },
    LoginResult {
        ok: bool,
        state: String,
        message: String,
    },
    ActionResult {
        ok: bool,
        message: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthRequest {
    Authenticate {
        request_id: u64,
        username: String,
        password: String,
        vt: u32,
    },
    StartSession {
        request_id: u64,
        username: String,
        session: String,
        vt: u32,
    },
    PowerAction {
        request_id: u64,
        action: String,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthResponse {
    AuthenticateResult {
        request_id: u64,
        ok: bool,
        message: String,
    },
    SessionResult {
        request_id: u64,
        ok: bool,
        exit_code: Option<i32>,
        message: String,
    },
    PowerResult {
        request_id: u64,
        ok: bool,
        message: String,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeter_request_round_trips() {
        let request = GreeterRequest::SubmitLogin {
            username: String::from("root"),
            password: String::from("password"),
        };
        let json = serde_json::to_string(&request).expect("request should serialize");
        let parsed: GreeterRequest = serde_json::from_str(&json).expect("request should deserialize");
        assert_eq!(parsed, request);
    }

    #[test]
    fn auth_response_round_trips() {
        let response = AuthResponse::SessionResult {
            request_id: 7,
            ok: true,
            exit_code: Some(0),
            message: String::from("ok"),
        };
        let json = serde_json::to_string(&response).expect("response should serialize");
        let parsed: AuthResponse = serde_json::from_str(&json).expect("response should deserialize");
        assert_eq!(parsed, response);
    }
}
