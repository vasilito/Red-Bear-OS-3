use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::{
    fdo::{self, ObjectManager},
    interface,
    object_server::SignalEmitter,
    connection::Builder as ConnectionBuilder,
    zvariant::ObjectPath,
};

const BUS_NAME: &str = "org.freedesktop.StatusNotifierWatcher";
const OBJECT_PATH: &str = "/StatusNotifierWatcher";

/// org.freedesktop.StatusNotifierWatcher D-Bus interface
/// Tracks registered system tray items and hosts for KDE Plasma.
struct StatusNotifierWatcher {
    items: Arc<Mutex<HashSet<String>>>,
    hosts: Arc<Mutex<HashSet<String>>>,
}

#[interface(name = "org.freedesktop.StatusNotifierWatcher")]
impl StatusNotifierWatcher {
    // --- Methods ---

    /// Register a status notifier item.
    /// The item parameter is either a full object path (e.g., "/org/example/Item")
    /// sent by the item itself, or a bus name (e.g., ":1.42" or "org.example.App")
    /// sent via the KDE protocol extension.
    async fn register_status_notifier_item(
        &self,
        #[zbus(signal_emitter)] signal_emitter: SignalEmitter<'_>,
        item: &str,
    ) -> fdo::Result<()> {
        let is_new = {
            let mut items = self.items.lock().map_err(|e| {
                fdo::Error::Failed(format!("items lock poisoned: {e}"))
            })?;
            items.insert(item.to_owned())
        };
        if is_new {
            eprintln!("statusnotifierwatcher: item registered: {item}");
            let _ = Self::status_notifier_item_registered(&signal_emitter, item).await;
        }
        Ok(())
    }

    /// Register a status notifier host (typically the system tray panel).
    async fn register_status_notifier_host(
        &self,
        #[zbus(signal_emitter)] signal_emitter: SignalEmitter<'_>,
        host: &str,
    ) -> fdo::Result<()> {
        let is_new = {
            let mut hosts = self.hosts.lock().map_err(|e| {
                fdo::Error::Failed(format!("hosts lock poisoned: {e}"))
            })?;
            hosts.insert(host.to_owned())
        };
        if is_new {
            eprintln!("statusnotifierwatcher: host registered: {host}");
        }
        Ok(())
    }

    // --- Properties ---

    /// List of registered status notifier item bus names / paths.
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> fdo::Result<Vec<String>> {
        let items = self.items.lock().map_err(|e| {
            fdo::Error::Failed(format!("items lock poisoned: {e}"))
        })?;
        Ok(items.iter().cloned().collect())
    }

    /// Whether at least one status notifier host is registered.
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> fdo::Result<bool> {
        let hosts = self.hosts.lock().map_err(|e| {
            fdo::Error::Failed(format!("hosts lock poisoned: {e}"))
        })?;
        Ok(!hosts.is_empty())
    }

    /// Protocol version (always 0 per spec).
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    // --- Signals ---

    /// Emitted when a new status notifier item is registered.
    #[zbus(signal, name = "StatusNotifierItemRegistered")]
    async fn status_notifier_item_registered(
        signal_emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    /// Emitted when a status notifier item is unregistered.
    #[zbus(signal, name = "StatusNotifierItemUnregistered")]
    async fn status_notifier_item_unregistered(
        signal_emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;
}

async fn wait_for_session_bus() {
    for _ in 0..30 {
        if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wait_for_session_bus().await;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    // Signal handler task for clean shutdown
    let signal_tx = shutdown_tx.clone();
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
        let _ = signal_tx.send(true);
    });
    // Keep original sender alive so receiver doesn't see all-senders-dropped
    let _shutdown_guard = shutdown_tx;

    let watcher = StatusNotifierWatcher {
        items: Arc::new(Mutex::new(HashSet::new())),
        hosts: Arc::new(Mutex::new(HashSet::new())),
    };

    let path: ObjectPath<'_> = OBJECT_PATH.try_into()?;
    let connection = ConnectionBuilder::session()?
        .name(BUS_NAME)?
        .serve_at(path, watcher)?
        .build()
        .await?;

    eprintln!("statusnotifierwatcher: {BUS_NAME} registered on session bus");

    // Wait for shutdown signal
    let _ = shutdown_rx.changed().await;
    eprintln!("statusnotifierwatcher: shutdown signal received, exiting cleanly");
    drop(connection);
    Ok(())
}
