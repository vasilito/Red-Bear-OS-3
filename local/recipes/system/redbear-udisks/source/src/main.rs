mod interfaces;
mod inventory;

use std::{
    env,
    error::Error,
    path::Path,
    process,
    sync::Arc,
    thread,
    time::Duration,
};

use interfaces::{BlockDeviceInterface, DriveInterface, ObjectManagerRoot, UDisksManager};
use inventory::{Inventory, MANAGER_PATH, ROOT_PATH};
use tokio::runtime::Builder as RuntimeBuilder;
use zbus::{
    Address,
    connection::Builder as ConnectionBuilder,
    zvariant::OwnedObjectPath,
};

const BUS_NAME: &str = "org.freedesktop.UDisks2";

enum Command {
    Run,
    Help,
}

fn usage() -> &'static str {
    "Usage: redbear-udisks [--help]"
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
    eprintln!("redbear-udisks: timed out waiting for D-Bus socket at {socket_path}");
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
    wait_for_dbus_socket().await;

    let mut last_err = None;
    for attempt in 1..=5 {
        let _root_path = parse_object_path(ROOT_PATH)?;
        let _manager_path = parse_object_path(MANAGER_PATH)?;
        let inventory = Arc::new(Inventory::scan());

        let mut builder = system_connection_builder()?
            .name(BUS_NAME)?
            .serve_at(ROOT_PATH, ObjectManagerRoot::new(inventory.clone()))?
            .serve_at(MANAGER_PATH, UDisksManager::new(inventory.clone()))?;

        for drive in inventory.drives() {
            builder = builder.serve_at(drive.object_path.as_str(), DriveInterface::new(drive.clone()))?;
        }

        for block in inventory.blocks() {
            builder = builder.serve_at(block.object_path.as_str(), BlockDeviceInterface::new(block.clone()))?;
        }

        match builder.build().await {
            Ok(connection) => {
                eprintln!(
                    "redbear-udisks: registered {BUS_NAME} on the system bus ({} drives, {} blocks)",
                    inventory.drives().len(),
                    inventory.blocks().len(),
                );
                wait_for_shutdown().await?;
                drop(connection);
                return Ok(());
            }
            Err(err) => {
                if attempt < 5 {
                    eprintln!("redbear-udisks: attempt {attempt}/5 failed ({err}), retrying in 2s...");
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
                    eprintln!("redbear-udisks: failed to create tokio runtime: {err}");
                    process::exit(1);
                }
            };

            if let Err(err) = runtime.block_on(run_daemon()) {
                eprintln!("redbear-udisks: fatal error: {err}");
                process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("redbear-udisks: {err}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    }
}
