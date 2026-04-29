use std::fs;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use pcid_interface::config::Config;
use pcid_interface::PciFunctionHandle;

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let initfs = args.contains("--initfs");

    common::setup_logging(
        "bus",
        "pci",
        "pci-spawner.log",
        common::output_level(),
        common::file_level(),
    );

    let mut config_data = String::new();
    for path in if initfs {
        config::config_for_initfs("pcid")?
    } else {
        config::config("pcid")?
    } {
        if let Ok(tmp) = fs::read_to_string(path) {
            config_data.push_str(&tmp);
        }
    }

    let config: Config = toml::from_str(&config_data)?;

    for entry in fs::read_dir("/scheme/pci")? {
        let entry = entry.context("failed to get entry")?;
        let device_path = entry.path();
        log::trace!("ENTRY: {}", device_path.to_string_lossy());

        let mut handle = match PciFunctionHandle::connect_by_path(&device_path) {
            Ok(handle) => handle,
            Err(err) if err.raw_os_error() == Some(syscall::ENOLCK) => {
                log::debug!(
                    "pcid-spawner: {} already in use: {err}",
                    device_path.display(),
                );
                continue;
            }
            Err(err) => {
                log::error!(
                    "pcid-spawner: failed to open channel for {}: {err}",
                    device_path.display(),
                );
                continue;
            }
        };

        let full_device_id = handle.config().func.full_device_id;

        log::debug!(
            "pcid-spawner enumerated: PCI {} {}",
            handle.config().func.addr,
            full_device_id.display()
        );

        let Some(driver) = config
            .drivers
            .iter()
            .find(|driver| driver.match_function(&full_device_id))
        else {
            log::debug!("no driver for {}, continuing", handle.config().func.addr);
            continue;
        };

        let mut args = driver.command.iter();

        let program = args
            .next()
            .ok_or_else(|| anyhow!("driver configuration entry did not have any command!"))?;
        let program = if program.starts_with('/') {
            program.to_owned()
        } else {
            "/usr/lib/drivers/".to_owned() + program
        };

        let mut command = Command::new(program);
        command.args(args);

        log::info!("pcid-spawner: spawn {:?}", command);

        handle.enable_device();

        let channel_fd = handle.into_inner_fd();
        command.env("PCID_CLIENT_CHANNEL", channel_fd.to_string());

        #[allow(deprecated, reason = "we can't yet move this to init")]
        daemon::Daemon::spawn(command);
        syscall::close(channel_fd as usize).unwrap();
    }

    Ok(())
}
