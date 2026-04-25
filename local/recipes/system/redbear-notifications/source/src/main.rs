use std::{
    collections::HashMap,
    env,
    error::Error,
    process,
    sync::atomic::{AtomicU32, Ordering},
};

use tokio::runtime::Builder as RuntimeBuilder;
use zbus::{
    connection::Builder as ConnectionBuilder,
    interface,
    object_server::SignalEmitter,
    zvariant::Value,
};

const BUS_NAME: &str = "org.freedesktop.Notifications";
const OBJECT_PATH: &str = "/org/freedesktop/Notifications";

#[derive(Debug)]
struct Notifications {
    next_id: AtomicU32,
}

impl Notifications {
    fn new() -> Self {
        Self {
            next_id: AtomicU32::new(1),
        }
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[zbus(name = "Notify")]
    fn notify(
        &self,
        app_name: &str,
        _replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        _hints: HashMap<String, Value<'_>>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        eprintln!("notification: [{app_name}] {summary}: {body}");

        for chunk in actions.chunks_exact(2) {
            eprintln!("notification {id}: action key '{}'", chunk[0]);
        }

        id
    }

    #[zbus(name = "CloseNotification")]
    async fn close_notification(
        &self,
        #[zbus(signal_emitter)] signal_emitter: SignalEmitter<'_>,
        id: u32,
    ) {
        eprintln!("notification: closed {id}");

        let _ = Self::notification_closed(&signal_emitter, id, 3).await;
    }

    #[zbus(name = "GetCapabilities")]
    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "body".to_owned(),
            "body-markup".to_owned(),
            "actions".to_owned(),
        ]
    }

    #[zbus(name = "GetServerInformation")]
    fn get_server_information(&self) -> (String, String, String, String) {
        (
            String::from("redbear-notifications"),
            String::from("Red Bear OS"),
            String::from("0.1.0"),
            String::from("1.2"),
        )
    }

    #[zbus(property(emits_changed_signal = "const"), name = "Idle")]
    fn idle(&self) -> bool {
        false
    }

    #[zbus(signal, name = "NotificationClosed")]
    async fn notification_closed(
        signal_emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal, name = "ActionInvoked")]
    async fn action_invoked(
        signal_emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

enum Command {
    Run,
    Help,
}

fn usage() -> &'static str {
    "Usage: redbear-notifications [--help]"
}

fn parse_args() -> Result<Command, String> {
    let mut args = env::args().skip(1);

    match args.next() {
        None => Ok(Command::Run),
        Some(arg) if arg == "--help" || arg == "-h" => {
            if args.next().is_some() {
                return Err(String::from("unexpected extra arguments after --help"));
            }

            Ok(Command::Help)
        }
        Some(arg) => Err(format!("unrecognized argument '{arg}'")),
    }
}

fn spawn_signal_handler(shutdown_tx: tokio::sync::watch::Sender<bool>) {
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
                tokio::select! {
                    _ = sigterm.recv() => {},
                    _ = tokio::signal::ctrl_c() => {},
                }
            } else {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        let _ = shutdown_tx.send(true);
    });
}

async fn run_daemon() -> Result<(), Box<dyn Error>> {
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    spawn_signal_handler(shutdown_tx);

    let _connection = ConnectionBuilder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, Notifications::new())?
        .build()
        .await?;

    eprintln!("redbear-notifications: registered {BUS_NAME} on the session bus");

    let _ = shutdown_rx.changed().await;
    eprintln!("redbear-notifications: shutdown signal received, exiting cleanly");

    Ok(())
}

fn main() {
    match parse_args() {
        Ok(Command::Help) => {
            println!("{}", usage());
        }
        Ok(Command::Run) => {
            let runtime = match RuntimeBuilder::new_multi_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(err) => {
                    eprintln!("redbear-notifications: failed to create tokio runtime: {err}");
                    process::exit(1);
                }
            };

            if let Err(err) = runtime.block_on(run_daemon()) {
                eprintln!("redbear-notifications: fatal error: {err}");
                process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("redbear-notifications: {err}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    }
}
