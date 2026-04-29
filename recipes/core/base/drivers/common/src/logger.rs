use std::str::FromStr;

use libredox::{flag, Fd};
use redox_log::{OutputBuilder, RedoxLogger};

/// Get the log verbosity for the output level.
pub fn output_level() -> log::LevelFilter {
    log::LevelFilter::Info
}

/// Get the log verbosity for the file level.
pub fn file_level() -> log::LevelFilter {
    log::LevelFilter::Info
}

/// Configures logging for a single driver.
#[cfg_attr(not(target_os = "redox"), allow(unused_variables, unused_mut))]
pub fn setup_logging(
    category: &str,
    subcategory: &str,
    logfile_base: &str,
    mut output_level: log::LevelFilter,
    file_level: log::LevelFilter,
) {
    RedoxLogger::init_timezone();
    if let Some(log_level) = read_bootloader_log_level_env(category, subcategory) {
        output_level = log_level;
    }

    let mut logger = RedoxLogger::new().with_output(
        OutputBuilder::stderr()
            .with_filter(output_level) // limit global output to important info
            .with_ansi_escape_codes()
            .flush_on_newline(true)
            .build(),
    );

    #[cfg(target_os = "redox")]
    match OutputBuilder::in_redox_logging_scheme(
        category,
        subcategory,
        format!("{logfile_base}.log"),
    ) {
        Ok(b) => {
            logger = logger.with_output(b.with_filter(file_level).flush_on_newline(true).build())
        }
        Err(error) => eprintln!("Failed to create {logfile_base}.log: {}", error),
    }

    #[cfg(target_os = "redox")]
    match OutputBuilder::in_redox_logging_scheme(
        category,
        subcategory,
        format!("{logfile_base}.ansi.log"),
    ) {
        Ok(b) => {
            logger = logger.with_output(
                b.with_filter(file_level)
                    .with_ansi_escape_codes()
                    .flush_on_newline(true)
                    .build(),
            )
        }
        Err(error) => eprintln!("Failed to create {logfile_base}.ansi.log: {}", error),
    }

    logger.enable().expect("failed to set default logger");
}

fn read_bootloader_log_level_env(category: &str, subcategory: &str) -> Option<log::LevelFilter> {
    let mut env_bytes = [0_u8; 4096];

    // TODO: Have the kernel env can specify prefixed env key instead of having to read all of them
    let envs = {
        let Ok(fd) = Fd::open("/scheme/sys/env", flag::O_RDONLY | flag::O_CLOEXEC, 0) else {
            return None;
        };
        let Ok(bytes_read) = fd.read(&mut env_bytes) else {
            return None;
        };
        if bytes_read >= env_bytes.len() {
            return None;
        }
        let env_bytes = &mut env_bytes[..bytes_read];

        env_bytes
            .split(|&c| c == b'\n')
            .filter(|var| var.starts_with(b"DRIVER_"))
            .collect::<Vec<_>>()
    };

    let log_env_keys = [
        format!("DRIVER_{}_LOG_LEVEL=", subcategory.to_ascii_uppercase()),
        format!("DRIVER_{}_LOG_LEVEL=", category.to_ascii_uppercase()),
        "DRIVER_LOG_LEVEL=".to_string(),
    ];

    for log_env_key in log_env_keys {
        let log_env_key = log_env_key.as_bytes();
        if let Some(log_env) = envs.iter().find_map(|var| var.strip_prefix(log_env_key)) {
            if let Ok(Ok(log_level)) = str::from_utf8(&log_env).map(log::LevelFilter::from_str) {
                return Some(log_level);
            }
        }
    }

    None
}
