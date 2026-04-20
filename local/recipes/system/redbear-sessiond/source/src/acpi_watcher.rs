use zbus::Connection;

use crate::runtime_state::SharedRuntime;

#[cfg(target_os = "redox")]
const KSTOP_PATH: &str = "/scheme/kernel.acpi/kstop";

#[cfg(target_os = "redox")]
fn wait_for_shutdown_edge() -> std::io::Result<()> {
    use std::io::Read;

    let mut file = std::fs::File::open(KSTOP_PATH)?;
    let mut byte = [0_u8; 1];
    let _ = file.read(&mut byte)?;
    Ok(())
}

pub async fn watch_and_emit(connection: Connection, runtime: SharedRuntime) {
    #[cfg(target_os = "redox")]
    match tokio::task::spawn_blocking(wait_for_shutdown_edge).await {
        Ok(Ok(())) => {
            if let Ok(mut state) = runtime.write() {
                state.preparing_for_shutdown = true;
            }
            let _ = connection
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "PrepareForShutdown",
                    &true,
                )
                .await;
        }
        Ok(Err(err)) => {
            eprintln!("redbear-sessiond: ACPI shutdown watcher failed: {err}");
        }
        Err(err) => {
            eprintln!("redbear-sessiond: ACPI shutdown watcher task failed: {err}");
        }
    }

    #[cfg(not(target_os = "redox"))]
    {
        let _ = connection;
        let _ = runtime;
    }
}
