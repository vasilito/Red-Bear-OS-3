use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::{env, io};

use serde::Deserialize;

use crate::script::subst_env;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub envs: BTreeMap<String, String>,
    #[serde(default)]
    pub inherit_envs: BTreeSet<String>,
    #[serde(rename = "type")]
    pub type_: ServiceType,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceType {
    #[default]
    Notify,
    Scheme(String),
    Oneshot,
    OneshotAsync,
}

impl Service {
    pub fn spawn(&self, base_envs: &BTreeMap<String, OsString>) {
        let mut command = Command::new(&self.cmd);
        command.args(self.args.iter().map(|arg| subst_env(arg)));
        command.env_clear();
        for env in &self.inherit_envs {
            if let Some(value) = env::var_os(env) {
                command.env(env, value);
            }
        }
        command.envs(base_envs).envs(&self.envs);

        let (mut read_pipe, write_pipe) = io::pipe().unwrap();
        unsafe { pass_fd(&mut command, "INIT_NOTIFY", write_pipe.into()) };

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                eprintln!("init: failed to execute {:?}: {}", command, err);
                return;
            }
        };

        match &self.type_ {
            ServiceType::Notify => match read_pipe.read_exact(&mut [0]) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                    eprintln!("init: {command:?} exited without notifying readiness");
                }
                Err(err) => {
                    eprintln!("init: failed to wait for {command:?}: {err}");
                }
            },
            ServiceType::Scheme(scheme) => {
                let mut new_fd = usize::MAX;
                loop {
                    match syscall::call_ro(
                        read_pipe.as_raw_fd() as usize,
                        unsafe { plain::as_mut_bytes(&mut new_fd) },
                        syscall::CallFlags::FD | syscall::CallFlags::FD_UPPER,
                        &[],
                    ) {
                        Err(syscall::Error {
                            errno: syscall::EINTR,
                        }) => continue,
                        Ok(0) => {
                            eprintln!("init: {command:?} exited without notifying readiness");
                            return;
                        }
                        Ok(1) => break,
                        Ok(n) => {
                            eprintln!("init: incorrect amount of fds {n} returned");
                            return;
                        }
                        Err(err) => {
                            eprintln!("init: failed to wait for {command:?}: {err}");
                            return;
                        }
                    }
                }

                let current_namespace_fd = libredox::call::getns().expect("TODO");
                libredox::call::register_scheme_to_ns(current_namespace_fd, scheme, new_fd)
                    .expect("TODO");
            }
            ServiceType::Oneshot => {
                drop(read_pipe);
                match child.wait() {
                    Ok(exit_status) => {
                        if !exit_status.success() {
                            eprintln!("init: {command:?} failed with {exit_status}");
                        }
                    }
                    Err(err) => {
                        eprintln!("init: failed to wait for {:?}: {}", command, err)
                    }
                }
            }
            ServiceType::OneshotAsync => {}
        }
    }
}

unsafe fn pass_fd(cmd: &mut Command, env: &str, fd: OwnedFd) {
    cmd.env(env, format!("{}", fd.as_raw_fd()));
    unsafe {
        cmd.pre_exec(move || {
            // Pass notify pipe to child
            if libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, 0) == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}
