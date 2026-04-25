mod acpi_watcher;
mod control;
mod device_map;
mod manager;
mod runtime_state;
mod seat;
mod session;

use std::{
    env,
    error::Error,
    process,
    time::Duration,
};

use device_map::DeviceMap;
use manager::LoginManager;
use seat::LoginSeat;
use session::LoginSession;
use runtime_state::shared_runtime;
use tokio::runtime::Builder as RuntimeBuilder;
use zbus::{
    Address,
    connection::Builder as ConnectionBuilder,
    zvariant::OwnedObjectPath,
};

const BUS_NAME: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/c1";
const SEAT_PATH: &str = "/org/freedesktop/login1/seat/seat0";
const USER_PATH: &str = "/org/freedesktop/login1/user/current";

enum Command {
    Run,
    Help,
}

fn usage() -> &'static str {
    "Usage: redbear-sessiond [--help]"
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

fn parse_object_path(path: &str) -> Result<OwnedObjectPath, Box<dyn Error>> {
    Ok(OwnedObjectPath::try_from(path.to_owned())?)
}

async fn wait_for_dbus_socket() {
    let socket_path = env::var("DBUS_STARTER_ADDRESS")
        .ok()
        .and_then(|addr| addr.strip_prefix("unix:path=").map(String::from))
        .unwrap_or_else(|| "/run/dbus/system_bus_socket".to_string());

    for _ in 0..30 {
        if tokio::net::UnixStream::connect(&socket_path).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    eprintln!("redbear-sessiond: timed out waiting for D-Bus socket at {socket_path}");
}

fn system_connection_builder() -> Result<ConnectionBuilder<'static>, Box<dyn Error>> {
    if let Ok(address) = env::var("DBUS_STARTER_ADDRESS") {
        Ok(ConnectionBuilder::address(Address::try_from(address.as_str())?)?)
    } else {
        Ok(ConnectionBuilder::address(Address::try_from("unix:path=/run/dbus/system_bus_socket")?)?)
    }
}

#[cfg(all(unix, not(target_os = "redox")))]
async fn wait_for_shutdown(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Result<(), Box<dyn Error>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = terminate.recv() => Ok(()),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = shutdown_rx.changed() => Ok(()),
    }
}

#[cfg(target_os = "redox")]
async fn wait_for_shutdown(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Result<(), Box<dyn Error>> {
    tokio::select! {
        _ = std::future::pending::<()>() => Ok(()),
        _ = shutdown_rx.changed() => Ok(()),
    }
}

#[cfg(all(not(unix), not(target_os = "redox")))]
async fn wait_for_shutdown(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Result<(), Box<dyn Error>> {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = shutdown_rx.changed() => Ok(()),
    }
}

async fn run_daemon() -> Result<(), Box<dyn Error>> {
    wait_for_dbus_socket().await;

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut last_err = None;
    for attempt in 1..=5 {
        let session_path = parse_object_path(SESSION_PATH)?;
        let seat_path = parse_object_path(SEAT_PATH)?;
        let user_path = parse_object_path(USER_PATH)?;
        let runtime = shared_runtime();
        let device_map = DeviceMap::discover();

        let session = LoginSession::new(seat_path.clone(), user_path.clone(), device_map, runtime.clone());
        let seat = LoginSeat::new(session_path.clone(), runtime.clone());
        let manager = LoginManager::new(session_path, seat_path, user_path, runtime.clone());

        match system_connection_builder()?
            .name(BUS_NAME)?
            .serve_at(MANAGER_PATH, manager)?
            .serve_at(SESSION_PATH, session)?
            .serve_at(SEAT_PATH, seat)?
            .build()
            .await
        {
            Ok(connection) => {
                eprintln!("redbear-sessiond: registered {BUS_NAME} on the system bus");
                control::start_control_socket(runtime.clone(), shutdown_tx.clone());
                tokio::spawn(acpi_watcher::watch_and_emit(connection.clone(), runtime.clone()));
                wait_for_shutdown(shutdown_rx).await?;
                drop(connection);
                return Ok(());
            }
            Err(err) => {
                if attempt < 5 {
                    eprintln!("redbear-sessiond: attempt {attempt}/5 failed ({err}), retrying in 2s...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                last_err = Some(err.into());
            }
        }
    }
    Err(last_err.unwrap())
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
                    eprintln!("redbear-sessiond: failed to create tokio runtime: {err}");
                    process::exit(1);
                }
            };

            if let Err(err) = runtime.block_on(run_daemon()) {
                eprintln!("redbear-sessiond: fatal error: {err}");
                process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("redbear-sessiond: {err}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    }
}
