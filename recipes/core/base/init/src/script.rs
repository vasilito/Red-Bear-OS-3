use std::collections::BTreeMap;
use std::{env, io, iter};

use crate::InitConfig;
use crate::unit::UnitId;

pub fn subst_env<'a>(arg: &str) -> String {
    if arg.starts_with('$') {
        env::var(&arg[1..]).unwrap_or(String::new())
    } else {
        arg.to_owned()
    }
}

pub struct Script(pub Vec<Command>, pub Vec<UnitId>);

impl Script {
    pub fn from_str(config: &str, errors: &mut Vec<String>) -> io::Result<Script> {
        let mut cmds = vec![];
        let mut requires_weak = vec![];

        for line_raw in config.lines() {
            let line = line_raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let args = line.split(' ').map(subst_env);

            match Command::parse(args, &mut requires_weak) {
                Ok(None) => {}
                Ok(Some(cmd)) => cmds.push(cmd),
                Err(err) => errors.push(err),
            }
        }

        Ok(Script(cmds, requires_weak))
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    process: Process,
    kind: CommandKind,
}

#[derive(Clone, Debug)]
enum CommandKind {
    Oneshot,
    OneshotAsync,
}

impl Command {
    fn parse(
        mut args: impl Iterator<Item = String>,
        requires_weak: &mut Vec<UnitId>,
    ) -> Result<Option<Command>, String> {
        let Some(cmd) = args.next() else {
            return Ok(None);
        };

        match cmd.as_str() {
            "requires_weak" => {
                requires_weak.extend(args.map(UnitId));
                Ok(None)
            }
            "nowait" => {
                let process = Process::parse(args)?;
                Ok(Some(Command {
                    process,
                    kind: CommandKind::OneshotAsync,
                }))
            }
            _ => {
                let process = Process::parse(iter::once(cmd).chain(args))?;
                Ok(Some(Command {
                    process,
                    kind: CommandKind::Oneshot,
                }))
            }
        }
    }

    pub fn run(&self, config: &mut InitConfig) {
        let Command { process, kind } = self;

        if config.skip_cmd.contains(&process.cmd) {
            eprintln!(
                "init: skipping '{} {}'",
                process.cmd,
                process.args.join(" ")
            );
            return;
        }

        let mut command = std::process::Command::new(&process.cmd);
        command.args(process.args.iter().map(|arg| subst_env(arg)));
        command.env_clear();
        command.envs(&config.envs).envs(&process.envs);

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                eprintln!("init: failed to execute {:?}: {}", command, err);
                return;
            }
        };

        match kind {
            CommandKind::Oneshot => match child.wait() {
                Ok(exit_status) => {
                    if !exit_status.success() {
                        eprintln!("init: {command:?} failed with {exit_status}");
                    }
                }
                Err(err) => {
                    eprintln!("init: failed to wait for {:?}: {}", command, err)
                }
            },
            CommandKind::OneshotAsync => {}
        }
    }
}

#[derive(Clone, Debug)]
struct Process {
    cmd: String,
    args: Vec<String>,
    envs: BTreeMap<String, String>,
}

impl Process {
    fn parse(parts: impl Iterator<Item = String>) -> Result<Process, String> {
        let mut cmd = None;
        let mut args = vec![];
        let mut envs = BTreeMap::new();

        for arg in parts {
            if cmd.is_none() {
                if let Some((env, value)) = arg.split_once('=') {
                    let value = if value == "$" {
                        env::var(env).unwrap_or_default()
                    } else {
                        subst_env(value)
                    };
                    if !value.is_empty() {
                        envs.insert(env.to_owned(), value);
                    }
                } else {
                    cmd = Some(arg);
                }
            } else {
                args.push(arg);
            }
        }

        if let Some(cmd) = cmd {
            Ok(Process { cmd, args, envs })
        } else {
            Err("no command given".to_owned())
        }
    }
}
