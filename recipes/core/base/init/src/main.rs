use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::{env, fs, io};

use libredox::flag::{O_RDONLY, O_WRONLY};

use crate::scheduler::Scheduler;
use crate::unit::{UnitId, UnitStore};

mod scheduler;
mod script;
mod service;
mod unit;

fn switch_stdio(stdio: &str) -> io::Result<()> {
    let stdin = libredox::Fd::open(stdio, O_RDONLY, 0)?;
    let stdout = libredox::Fd::open(stdio, O_WRONLY, 0)?;
    let stderr = libredox::Fd::open(stdio, O_WRONLY, 0)?;

    stdin.dup2(0, &[])?;
    stdout.dup2(1, &[])?;
    stderr.dup2(2, &[])?;

    Ok(())
}

struct InitConfig {
    log_debug: bool,
    skip_cmd: Vec<String>,
    envs: BTreeMap<String, OsString>,
}

impl InitConfig {
    fn new() -> Self {
        let log_level = env::var("INIT_LOG_LEVEL").unwrap_or("INFO".into());
        let log_debug = matches!(log_level.as_str(), "DEBUG" | "TRACE");
        let skip_cmd: Vec<String> = match env::var("INIT_SKIP") {
            Ok(v) if v.len() > 0 => v.split(',').map(|s| s.to_string()).collect(),
            _ => Vec::new(),
        };

        Self {
            log_debug,
            skip_cmd,
            envs: BTreeMap::from([("RUST_BACKTRACE".to_owned(), "1".into())]),
        }
    }
}

fn switch_root(unit_store: &mut UnitStore, config: &mut InitConfig, prefix: &Path, etcdir: &Path) {
    eprintln!(
        "init: switchroot to {} {}",
        prefix.display(),
        etcdir.display()
    );

    config
        .envs
        .insert("PATH".to_owned(), prefix.join("bin").into_os_string());
    config.envs.insert(
        "LD_LIBRARY_PATH".to_owned(),
        prefix.join("lib").into_os_string(),
    );

    unit_store.config_dirs = vec![prefix.join("lib").join("init.d"), etcdir.join("init.d")];

    let env_dirs = &[
        prefix.join("lib").join("environment.d"),
        etcdir.join("environment.d"),
    ];
    match config::config_for_dirs(env_dirs) {
        Ok(files) => {
            for file in files {
                match fs::read_to_string(&file) {
                    Ok(envs) => {
                        for env in envs.lines() {
                            if env.is_empty() || env.starts_with("#") {
                                continue;
                            }
                            let Some((key, value)) = env.split_once('=') else {
                                eprintln!(
                                    "init: failed to parse env line from {}: {env:?}",
                                    file.display(),
                                );
                                continue;
                            };
                            config
                                .envs
                                .insert(key.to_owned().into(), value.to_owned().into());
                        }
                    }
                    Err(err) => {
                        eprintln!(
                            "init: failed to read environment from {}: {err}",
                            file.display(),
                        );
                    }
                }
            }
        }
        Err(err) => {
            eprintln!(
                "init: failed to read environments from {}: {err}",
                env_dirs
                    .iter()
                    .map(|dir| dir.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

fn main() {
    let mut init_config = InitConfig::new();
    let mut unit_store = UnitStore::new();
    let mut scheduler = Scheduler::new();

    switch_root(
        &mut unit_store,
        &mut init_config,
        Path::new("/scheme/initfs"),
        Path::new("/scheme/initfs/etc"),
    );

    // Start logd first such that we can pass /scheme/log as stdio to all other services
    scheduler
        .schedule_start_and_report_errors(&mut unit_store, UnitId("00_logd.service".to_owned()));
    scheduler.step(&mut unit_store, &mut init_config);
    if let Err(err) = switch_stdio("/scheme/log") {
        eprintln!("init: failed to switch stdio to '/scheme/log': {err}");
    }

    let runtime_target = UnitId("00_runtime.target".to_owned());
    scheduler.schedule_start_and_report_errors(&mut unit_store, runtime_target.clone());
    unit_store.set_runtime_target(runtime_target);

    scheduler
        .schedule_start_and_report_errors(&mut unit_store, UnitId("90_initfs.target".to_owned()));
    scheduler.step(&mut unit_store, &mut init_config);

    switch_root(
        &mut unit_store,
        &mut init_config,
        Path::new("/usr"),
        Path::new("/etc"),
    );
    {
        // FIXME introduce multi-user.target unit and replace the config dir iteration
        // scheduler.schedule_start_and_report_errors(&mut unit_store, UnitId("multi-user.target".to_owned()));

        let entries = match config::config_for_dirs(&unit_store.config_dirs) {
            Ok(entries) => entries,
            Err(err) => {
                eprintln!(
                    "init: failed to read configs from {}: {err}",
                    unit_store
                        .config_dirs
                        .iter()
                        .map(|dir| dir.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                return;
            }
        };
        for entry in entries {
            scheduler.schedule_start_and_report_errors(
                &mut unit_store,
                UnitId(entry.file_name().unwrap().to_str().unwrap().to_owned()),
            );
        }
    };

    scheduler.step(&mut unit_store, &mut init_config);

    libredox::call::setrens(0, 0).expect("init: failed to enter null namespace");

    loop {
        let mut status = 0;
        libredox::call::waitpid(0, &mut status, 0).unwrap();
    }
}
