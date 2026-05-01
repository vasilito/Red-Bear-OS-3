use std::collections::VecDeque;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::Duration;
use std::{env, io};

use crate::InitConfig;
use crate::service::ServiceType;
use crate::unit::{RestartPolicy, UnitId, UnitKind, UnitStore};

const MAX_DEPENDENCY_WAIT_RETRIES: u32 = 1000;

pub struct Scheduler {
    pending: VecDeque<Job>,
}

struct Job {
    unit: UnitId,
    kind: JobKind,
    dep_retries: u32,
}

enum JobKind {
    Start,
    Restart { backoff: Duration },
}

impl Scheduler {
    pub fn new() -> Scheduler {
        Scheduler {
            pending: VecDeque::new(),
        }
    }

    pub fn schedule_start_and_report_errors(
        &mut self,
        unit_store: &mut UnitStore,
        unit_id: UnitId,
    ) {
        let mut errors = vec![];
        self.schedule_start(unit_store, unit_id, &mut errors);
        for error in errors {
            eprintln!("init: {error}");
        }
    }

    pub fn schedule_start(
        &mut self,
        unit_store: &mut UnitStore,
        unit_id: UnitId,
        errors: &mut Vec<String>,
    ) {
        let loaded_units = unit_store.load_units(unit_id.clone(), errors);
        for unit_id in loaded_units {
            if !unit_store.unit(&unit_id).conditions_met() {
                continue;
            }

            self.pending.push_back(Job {
                unit: unit_id,
                kind: JobKind::Start,
                dep_retries: 0,
            });
        }
    }

    pub fn step(&mut self, unit_store: &mut UnitStore, init_config: &mut InitConfig) {
        'a: loop {
            let Some(mut job) = self.pending.pop_front() else {
                return;
            };

            match job.kind {
                JobKind::Start => {
                    let unit = unit_store.unit(&job.unit);

                    let timeout_secs = unit.info.dependency_timeout_secs;
                    let mut deps_pending = false;
                    for dep in &unit.info.requires_weak {
                        for pending_job in &self.pending {
                            if &pending_job.unit == dep {
                                deps_pending = true;
                                break;
                            }
                        }
                        if deps_pending {
                            break;
                        }
                    }

                    if deps_pending {
                        if timeout_secs > 0 {
                            job.dep_retries += 1;
                            let max_retries = timeout_secs * 100; // ~10ms per retry
                            if job.dep_retries > max_retries as u32 {
                                eprintln!(
                                    "init: {}: dependency timeout after {}s, failing",
                                    job.unit.0, timeout_secs
                                );
                                continue;
                            }
                        } else if job.dep_retries >= MAX_DEPENDENCY_WAIT_RETRIES {
                            eprintln!(
                                "init: {}: dependency wait exceeded {} retries, failing",
                                job.unit.0, MAX_DEPENDENCY_WAIT_RETRIES
                            );
                            continue;
                        }
                        job.dep_retries += 1;
                        self.pending.push_back(job);
                        continue 'a;
                    }

                    if let Err(restart) = run(unit_store, &job.unit, init_config) {
                        if let Some(backoff) = restart {
                            self.pending.push_back(Job {
                                unit: job.unit.clone(),
                                kind: JobKind::Restart { backoff },
                                dep_retries: 0,
                            });
                        }
                    }
                }
                JobKind::Restart { backoff } => {
                    std::thread::sleep(backoff);
                    let next_backoff = (backoff * 2).min(Duration::from_secs(60));
                    if let Err(restart) = run(unit_store, &job.unit, init_config) {
                        if let Some(_next) = restart {
                            self.pending.push_back(Job {
                                unit: job.unit,
                                kind: JobKind::Restart {
                                    backoff: next_backoff,
                                },
                                dep_retries: 0,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn run(
    unit_store: &UnitStore,
    unit_id: &UnitId,
    config: &mut InitConfig,
) -> Result<(), Option<Duration>> {
    let unit = unit_store.unit(unit_id);

    let restart_policy = unit.info.restart;

    match &unit.kind {
        UnitKind::LegacyScript { script } => {
            for cmd in script.clone() {
                if config.log_debug {
                    eprintln!("init: running: {cmd:?}");
                }
                cmd.run(config);
            }
            Ok(())
        }
        UnitKind::Service { service } => {
            if config.skip_cmd.contains(&service.cmd) {
                eprintln!("Skipping '{} {}'", service.cmd, service.args.join(" "));
                return Ok(());
            }
            if config.log_debug {
                eprintln!(
                    "Starting {} ({})",
                    unit.info.description.as_ref().unwrap_or(&unit.id.0),
                    service.cmd,
                );
            }

            let mut command = Command::new(&service.cmd);
            command.args(&service.args);
            command.env_clear();
            for env in &service.inherit_envs {
                if let Some(value) = env::var_os(env) {
                    command.env(env, value);
                }
            }
            command.envs(config.envs.iter().map(|(k, v)| (k.as_str(), v.as_os_str())));

            let (read_pipe, write_pipe) = match io::pipe() {
                Ok(p) => p,
                Err(err) => {
                    eprintln!("init: pipe failed for {}: {}", service.cmd, err);
                    return Err(restart_signal(restart_policy));
                }
            };

            let write_fd: std::os::fd::OwnedFd = write_pipe.into();
            unsafe {
                command.env("INIT_NOTIFY", format!("{}", write_fd.as_raw_fd()));
                command.pre_exec(move || {
                    if unsafe { libc::fcntl(write_fd.as_raw_fd(), libc::F_SETFD, 0) } == -1 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }

            let status = service_spawn_status(read_pipe, command, &service.type_, &service.cmd);

            match status {
                SpawnStatus::Success => Ok(()),
                SpawnStatus::Failed => Err(restart_signal(restart_policy)),
                SpawnStatus::Async => Ok(()),
            }
        }
        UnitKind::Target {} => {
            if config.log_debug {
                eprintln!(
                    "Reached target {}",
                    unit.info.description.as_ref().unwrap_or(&unit.id.0),
                );
            }
            Ok(())
        }
    }
}

enum SpawnStatus {
    Success,
    Failed,
    Async,
}

fn restart_signal(policy: RestartPolicy) -> Option<Duration> {
    match policy {
        RestartPolicy::No => None,
        RestartPolicy::OnFailure | RestartPolicy::Always => Some(Duration::from_secs(1)),
    }
}

fn service_spawn_status(
    mut read_pipe: impl Read + AsRawFd,
    mut command: Command,
    service_type: &ServiceType,
    cmd: &str,
) -> SpawnStatus {
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            eprintln!("init: failed to execute {}: {}", cmd, err);
            return SpawnStatus::Failed;
        }
    };

    match service_type {
        ServiceType::Notify => match read_pipe.read_exact(&mut [0]) {
            Ok(()) => SpawnStatus::Success,
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                eprintln!("init: {cmd} exited without notifying readiness");
                SpawnStatus::Failed
            }
            Err(err) => {
                eprintln!("init: failed to wait for {cmd}: {err}");
                SpawnStatus::Failed
            }
        },
        ServiceType::Scheme(scheme) => {
            let scheme = scheme.clone();
            let mut new_fd = usize::MAX;
            let res = loop {
                match syscall::call_ro(
                    read_pipe.as_raw_fd() as usize,
                    unsafe { plain::as_mut_bytes(&mut new_fd) },
                    syscall::CallFlags::FD | syscall::CallFlags::FD_UPPER,
                    &[],
                ) {
                    Err(syscall::Error {
                        errno: syscall::EINTR,
                    }) => continue,
                    Ok(0) => break SpawnStatus::Failed,
                    Ok(1) => break SpawnStatus::Success,
                    Ok(n) => {
                        eprintln!("init: incorrect amount of fds {n} returned from {cmd}");
                        break SpawnStatus::Failed;
                    }
                    Err(err) => {
                        eprintln!("init: failed to wait for {cmd}: {err}");
                        break SpawnStatus::Failed;
                    }
                }
            };

            if matches!(res, SpawnStatus::Success) {
                match libredox::call::getns() {
                    Ok(current_namespace_fd) => {
                        if let Err(err) = libredox::call::register_scheme_to_ns(
                            current_namespace_fd,
                            &scheme,
                            new_fd,
                        ) {
                            eprintln!("init: scheme registration failed for {cmd}: {err}");
                            return SpawnStatus::Failed;
                        }
                    }
                    Err(err) => {
                        eprintln!("init: getns failed for {cmd}: {err}");
                        return SpawnStatus::Failed;
                    }
                }
            }
            res
        }
        ServiceType::Oneshot => {
            drop(read_pipe);
            match child.wait() {
                Ok(exit_status) => {
                    if !exit_status.success() {
                        eprintln!("init: {cmd} failed with {exit_status}");
                        SpawnStatus::Failed
                    } else {
                        SpawnStatus::Success
                    }
                }
                Err(err) => {
                    eprintln!("init: failed to wait for {cmd}: {err}");
                    SpawnStatus::Failed
                }
            }
        }
        ServiceType::OneshotAsync => SpawnStatus::Async,
    }
}
