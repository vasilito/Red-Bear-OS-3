use std::{collections::HashMap, env, error::Error, process};

use tokio::runtime::Builder as RuntimeBuilder;
use zbus::{
    Address,
    connection::Builder as ConnectionBuilder,
    interface,
    zvariant::{ObjectPath, OwnedObjectPath},
};

const BUS_NAME: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";

type AuthorizationDetails = HashMap<String, String>;
type EnumeratedAction = (
    String,
    String,
    String,
    String,
    String,
    u32,
    AuthorizationDetails,
);

#[derive(Debug, Default)]
struct PolicyKitAuthority;

enum Command {
    Run,
    Help,
}

fn usage() -> &'static str {
    "Usage: redbear-polkit [--help]"
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
        Ok(ConnectionBuilder::system()?)
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

#[interface(name = "org.freedesktop.PolicyKit1.Authority")]
impl PolicyKitAuthority {
    #[zbus(name = "CheckAuthorization")]
    fn check_authorization(
        &self,
        _action_id: &str,
        _details: AuthorizationDetails,
        _flags: u32,
        _cancellation_id: &str,
    ) -> (bool, bool, AuthorizationDetails) {
        (true, false, AuthorizationDetails::new())
    }

    #[zbus(name = "RegisterAuthenticationAgent")]
    fn register_authentication_agent(
        &self,
        _session: (&str, ObjectPath<'_>),
        _locale: &str,
        _object_path: &str,
    ) {
    }

    #[zbus(name = "UnregisterAuthenticationAgent")]
    fn unregister_authentication_agent(
        &self,
        _session: (&str, ObjectPath<'_>),
        _object_path: &str,
    ) {
    }

    #[zbus(name = "EnumerateActions")]
    fn enumerate_actions(&self, _locale: &str) -> Vec<EnumeratedAction> {
        Vec::new()
    }

    #[zbus(property(emits_changed_signal = "const"), name = "BackendName")]
    fn backend_name(&self) -> String {
        String::from("redbear-permit-all")
    }

    #[zbus(property(emits_changed_signal = "const"), name = "BackendVersion")]
    fn backend_version(&self) -> String {
        String::from("0.1.0")
    }
}

async fn run_daemon() -> Result<(), Box<dyn Error>> {
    eprintln!("redbear-polkit: startup begin");
    let _authority_path = parse_object_path(AUTHORITY_PATH)?;
    eprintln!("redbear-polkit: object paths parsed");

    eprintln!("redbear-polkit: starter address={:?}", env::var("DBUS_STARTER_ADDRESS").ok());
    eprintln!("redbear-polkit: building D-Bus connection");
    let connection = system_connection_builder()?
        .name(BUS_NAME)?
        .serve_at(AUTHORITY_PATH, PolicyKitAuthority)?
        .build()
        .await?;

    eprintln!("redbear-polkit: registered {BUS_NAME} on the system bus");

    wait_for_shutdown().await?;
    drop(connection);
    eprintln!("redbear-polkit: received shutdown signal, exiting cleanly");

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
                    eprintln!("redbear-polkit: failed to create tokio runtime: {err}");
                    process::exit(1);
                }
            };

            if let Err(err) = runtime.block_on(run_daemon()) {
                eprintln!("redbear-polkit: fatal error: {err}");
                process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("redbear-polkit: {err}");
            eprintln!("{}", usage());
            process::exit(1);
        }
    }
}
