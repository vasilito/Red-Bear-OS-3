use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use zbus::Connection;

static SLEEP_ACTIVE: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_FIRED: AtomicBool = AtomicBool::new(false);

const ACPI_SLEEP_PATH: &str = "/scheme/acpi/sleep";
const ACPI_SHUTDOWN_PATH: &str = "/scheme/acpi/shutdown";
const POLL_INTERVAL: Duration = Duration::from_secs(5);

fn read_acpi_flag(path: &str) -> bool {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let trimmed = content.trim().to_lowercase();
            !trimmed.is_empty() && trimmed != "0"
        }
        Err(_) => false,
    }
}

pub async fn watch_and_emit(connection: Connection) {
    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let sleep_now = tokio::task::spawn_blocking(|| read_acpi_flag(ACPI_SLEEP_PATH))
            .await
            .unwrap_or(false);

        let was_sleeping = SLEEP_ACTIVE.load(Ordering::Relaxed);

        if sleep_now && !was_sleeping {
            SLEEP_ACTIVE.store(true, Ordering::Relaxed);
            let _ = connection.emit_signal(
                None::<&str>,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                &true,
            ).await;
        } else if !sleep_now && was_sleeping {
            SLEEP_ACTIVE.store(false, Ordering::Relaxed);
            let _ = connection.emit_signal(
                None::<&str>,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                &false,
            ).await;
        }

        let shutdown_now = tokio::task::spawn_blocking(|| read_acpi_flag(ACPI_SHUTDOWN_PATH))
            .await
            .unwrap_or(false);

        if shutdown_now && !SHUTDOWN_FIRED.load(Ordering::Relaxed) {
            SHUTDOWN_FIRED.store(true, Ordering::Relaxed);
            let _ = connection.emit_signal(
                None::<&str>,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForShutdown",
                &true,
            ).await;
        }
    }
}
