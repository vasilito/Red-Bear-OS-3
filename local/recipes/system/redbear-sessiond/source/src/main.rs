mod acpi_watcher;
mod device_map;
mod manager;
mod seat;
mod session;

use std::{
    env,
    error::Error,
    process,
};

use device_map::DeviceMap;
use manager::LoginManager;
use seat::LoginSeat;
use session::LoginSession;
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
const USER_PATH: &str = "/org/freedesktop/login1/user/0";

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

fn system_connection_builder() -> Result<ConnectionBuilder<'static>, Box<dyn Error>> {
    if let Ok(address) = env::var("DBUS_STARTER_ADDRESS") {
        Ok(ConnectionBuilder::address(Address::try_from(address.as_str())?)?)
    } else {
        Ok(ConnectionBuilder::address(Address::try_from("unix:path=/run/dbus/system_bus_socket")?)?)
    }
}

#[cfg(all(unix, not(target_os = "redox")))]
async fn wait_for_shutdown() -> Result<(), Box<dyn Error>> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = terminate.recv() => Ok(()),
        _ = tokio::signal::ctrl_c() => Ok(()),
    }
}

#[cfg(target_os = "redox")]
async fn wait_for_shutdown() -> Result<(), Box<dyn Error>> {
    std::future::pending::<()>().await;
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(all(not(unix), not(target_os = "redox")))]
async fn wait_for_shutdown() -> Result<(), Box<dyn Error>> {
    tokio::signal::ctrl_c().await?;
    Ok(())
}

async fn run_daemon() -> Result<(), Box<dyn Error>> {
    eprintln!("redbear-sessiond: startup begin");
    let session_path = parse_object_path(SESSION_PATH)?;
    let seat_path = parse_object_path(SEAT_PATH)?;
    let user_path = parse_object_path(USER_PATH)?;
    eprintln!("redbear-sessiond: object paths parsed");

    let session = LoginSession::new(seat_path.clone(), user_path, DeviceMap::new());
    let seat = LoginSeat::new(session_path.clone());
    let manager = LoginManager::new(session_path, seat_path);

    eprintln!("redbear-sessiond: starter address={:?}", env::var("DBUS_STARTER_ADDRESS").ok());
    eprintln!("redbear-sessiond: building D-Bus connection");
    let mut builder = system_connection_builder()?;
    eprintln!("redbear-sessiond: builder created");
    builder = builder.name(BUS_NAME)?;
    eprintln!("redbear-sessiond: bus name reserved");
    builder = builder.serve_at(MANAGER_PATH, manager)?;
    eprintln!("redbear-sessiond: served manager path {MANAGER_PATH}");
    builder = builder.serve_at(SESSION_PATH, session)?;
    eprintln!("redbear-sessiond: served session path {SESSION_PATH}");
    builder = builder.serve_at(SEAT_PATH, seat)?;
    eprintln!("redbear-sessiond: served seat path {SEAT_PATH}");
    eprintln!("redbear-sessiond: finalizing connection build");
    let connection = builder.build().await?;

    eprintln!("redbear-sessiond: registered {BUS_NAME} on the system bus");

    tokio::spawn(acpi_watcher::watch_and_emit(connection.clone()));

    wait_for_shutdown().await?;
    eprintln!("redbear-sessiond: received shutdown signal, exiting cleanly");

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
