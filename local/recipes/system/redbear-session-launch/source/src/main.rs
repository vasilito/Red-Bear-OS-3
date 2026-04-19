use std::{
    collections::{BTreeMap, HashMap},
    env,
    ffi::CString,
    fs,
    io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{self, Command},
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Account {
    username: String,
    uid: u32,
    gid: u32,
    home: String,
    shell: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GroupEntry {
    gid: u32,
    members: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LaunchMode {
    Session,
    Command { program: String, args: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Args {
    username: String,
    vt: u32,
    session: String,
    runtime_dir: Option<PathBuf>,
    wayland_display: String,
    mode: LaunchMode,
}

fn usage() -> &'static str {
    "Usage: redbear-session-launch --username USER [--mode session|command] [--session kde-wayland] [--vt N] [--runtime-dir PATH] [--wayland-display NAME] [--command PROGRAM [ARGS...]]"
}

fn parse_args_from<I>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut username = None;
    let mut vt = 3_u32;
    let mut session = String::from("kde-wayland");
    let mut runtime_dir = None;
    let mut wayland_display = String::from("wayland-0");
    let mut mode = String::from("session");
    let mut command = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Err(String::new()),
            "--username" => username = Some(args.next().ok_or_else(|| String::from("missing value after --username"))?),
            "--vt" => {
                let value = args.next().ok_or_else(|| String::from("missing value after --vt"))?;
                vt = value.parse().map_err(|_| format!("invalid VT '{value}'"))?;
            }
            "--session" => session = args.next().ok_or_else(|| String::from("missing value after --session"))?,
            "--runtime-dir" => {
                runtime_dir = Some(PathBuf::from(
                    args.next().ok_or_else(|| String::from("missing value after --runtime-dir"))?,
                ));
            }
            "--wayland-display" => {
                wayland_display = args
                    .next()
                    .ok_or_else(|| String::from("missing value after --wayland-display"))?;
            }
            "--mode" => mode = args.next().ok_or_else(|| String::from("missing value after --mode"))?,
            "--command" => {
                let program = args.next().ok_or_else(|| String::from("missing program after --command"))?;
                let rest = args.collect::<Vec<_>>();
                command = Some((program, rest));
                break;
            }
            other => return Err(format!("unrecognized argument '{other}'")),
        }
    }

    let username = username.ok_or_else(|| String::from("--username is required"))?;
    let mode = match mode.as_str() {
        "session" => LaunchMode::Session,
        "command" => {
            let (program, args) = command.ok_or_else(|| String::from("--command is required when --mode=command"))?;
            LaunchMode::Command { program, args }
        }
        other => return Err(format!("unsupported launch mode '{other}'")),
    };

    Ok(Args {
        username,
        vt,
        session,
        runtime_dir,
        wayland_display,
        mode,
    })
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(env::args().skip(1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountFormat {
    Redox,
    Unix,
}

fn split_account_fields(line: &str) -> (AccountFormat, Vec<&str>) {
    let format = if line.contains(';') {
        AccountFormat::Redox
    } else {
        AccountFormat::Unix
    };
    let delimiter = match format {
        AccountFormat::Redox => ';',
        AccountFormat::Unix => ':',
    };
    (format, line.split(delimiter).collect::<Vec<_>>())
}

fn parse_passwd(contents: &str) -> Result<HashMap<String, Account>, String> {
    let mut accounts = HashMap::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (format, parts) = split_account_fields(line);
        let (uid_index, gid_index, home_index, shell_index) = match format {
            AccountFormat::Redox if parts.len() >= 6 => (1, 2, 4, 5),
            AccountFormat::Unix if parts.len() >= 7 => (2, 3, 5, 6),
            AccountFormat::Redox => return Err(format!("invalid Redox passwd entry on line {}", index + 1)),
            AccountFormat::Unix => return Err(format!("invalid passwd entry on line {}", index + 1)),
        };

        let uid = parts[uid_index]
            .parse::<u32>()
            .map_err(|_| format!("invalid uid on line {}", index + 1))?;
        let gid = parts[gid_index]
            .parse::<u32>()
            .map_err(|_| format!("invalid gid on line {}", index + 1))?;

        accounts.insert(
            parts[0].to_string(),
            Account {
                username: parts[0].to_string(),
                uid,
                gid,
                home: parts[home_index].to_string(),
                shell: parts[shell_index].to_string(),
            },
        );
    }

    Ok(accounts)
}

fn parse_groups(contents: &str) -> Result<Vec<GroupEntry>, String> {
    let mut groups = Vec::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (_format, parts) = split_account_fields(line);
        if parts.len() < 4 {
            return Err(format!("invalid group entry on line {}", index + 1));
        }

        let gid = parts[2]
            .parse::<u32>()
            .map_err(|_| format!("invalid group gid on line {}", index + 1))?;
        let members = if parts[3].is_empty() {
            Vec::new()
        } else {
            parts[3].split(',').map(str::to_string).collect::<Vec<_>>()
        };

        groups.push(GroupEntry { gid, members });
    }

    Ok(groups)
}

fn load_account(username: &str) -> Result<Account, String> {
    let passwd = fs::read_to_string("/etc/passwd").map_err(|err| format!("failed to read /etc/passwd: {err}"))?;
    let accounts = parse_passwd(&passwd)?;
    accounts
        .get(username)
        .cloned()
        .ok_or_else(|| format!("unknown user '{username}'"))
}

fn load_supplementary_groups(username: &str, primary_gid: u32) -> Result<Vec<u32>, String> {
    let Ok(group_contents) = fs::read_to_string("/etc/group") else {
        return Ok(vec![primary_gid]);
    };

    let mut groups = parse_groups(&group_contents)?
        .into_iter()
        .filter(|entry| entry.gid == primary_gid || entry.members.iter().any(|member| member == username))
        .map(|entry| entry.gid)
        .collect::<Vec<_>>();
    groups.sort_unstable();
    groups.dedup();
    if groups.is_empty() {
        groups.push(primary_gid);
    }
    Ok(groups)
}

fn default_runtime_dir(uid: u32) -> PathBuf {
    if Path::new("/run/user").exists() {
        PathBuf::from(format!("/run/user/{uid}"))
    } else {
        PathBuf::from(format!("/tmp/run/user/{uid}"))
    }
}

fn ensure_runtime_dir(path: &Path, uid: u32, gid: u32) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|err| format!("failed to create runtime dir {}: {err}", path.display()))?;
    let c_path = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| format!("runtime dir {} contains interior NUL", path.display()))?;
    let result = unsafe { libc::chown(c_path.as_ptr(), uid, gid) };
    if result != 0 {
        return Err(format!("failed to chown runtime dir {}: {}", path.display(), io::Error::last_os_error()));
    }
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .map_err(|err| format!("failed to set runtime dir permissions on {}: {err}", path.display()))
}

fn env_value(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| env::var(key).ok())
}

fn build_environment(account: &Account, args: &Args, runtime_dir: &Path) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    values.insert(String::from("HOME"), account.home.clone());
    values.insert(String::from("USER"), account.username.clone());
    values.insert(String::from("LOGNAME"), account.username.clone());
    values.insert(String::from("SHELL"), account.shell.clone());
    values.insert(String::from("PATH"), String::from("/usr/bin:/bin"));
    values.insert(String::from("XDG_RUNTIME_DIR"), runtime_dir.display().to_string());
    values.insert(String::from("WAYLAND_DISPLAY"), args.wayland_display.clone());
    values.insert(String::from("XDG_SEAT"), String::from("seat0"));
    values.insert(String::from("XDG_VTNR"), args.vt.to_string());
    values.insert(String::from("LIBSEAT_BACKEND"), String::from("seatd"));
    values.insert(String::from("SEATD_SOCK"), String::from("/run/seatd.sock"));
    values.insert(String::from("DISPLAY"), String::new());
    values.insert(String::from("XDG_SESSION_TYPE"), String::from("wayland"));

    if let Some(theme) = env_value(&["XCURSOR_THEME"]) {
        values.insert(String::from("XCURSOR_THEME"), theme);
    }
    if let Some(root) = env_value(&["XKB_CONFIG_ROOT"]) {
        values.insert(String::from("XKB_CONFIG_ROOT"), root);
    }
    if let Some(path) = env_value(&["QT_PLUGIN_PATH"]) {
        values.insert(String::from("QT_PLUGIN_PATH"), path);
    }
    if let Some(path) = env_value(&["QT_QPA_PLATFORM_PLUGIN_PATH"]) {
        values.insert(String::from("QT_QPA_PLATFORM_PLUGIN_PATH"), path);
    }
    if let Some(path) = env_value(&["QML2_IMPORT_PATH"]) {
        values.insert(String::from("QML2_IMPORT_PATH"), path);
    }

    match args.mode {
        LaunchMode::Session => {
            values.insert(String::from("XDG_CURRENT_DESKTOP"), String::from("KDE"));
            values.insert(String::from("KDE_FULL_SESSION"), String::from("true"));
        }
        LaunchMode::Command { .. } => {}
    }

    values
}

#[cfg(not(target_os = "redox"))]
fn apply_groups(groups: &[u32]) -> io::Result<()> {
    let raw_groups = groups.iter().map(|gid| *gid as libc::gid_t).collect::<Vec<_>>();
    let result = unsafe { libc::setgroups(raw_groups.len(), raw_groups.as_ptr()) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "redox")]
fn apply_groups(_groups: &[u32]) -> io::Result<()> {
    Ok(())
}

fn command_for(args: &Args) -> Result<(String, Vec<String>), String> {
    match &args.mode {
        LaunchMode::Session => {
            if args.session != "kde-wayland" {
                return Err(format!("unsupported session '{}'", args.session));
            }

            if Path::new("/usr/bin/dbus-run-session").exists() {
                Ok((
                    String::from("/usr/bin/dbus-run-session"),
                    vec![String::from("--"), String::from("/usr/bin/redbear-kde-session")],
                ))
            } else {
                Ok((String::from("/usr/bin/redbear-kde-session"), Vec::new()))
            }
        }
        LaunchMode::Command { program, args } => Ok((program.clone(), args.clone())),
    }
}

fn run() -> Result<(), String> {
    let args = match parse_args() {
        Ok(parsed) => parsed,
        Err(err) if err.is_empty() => {
            println!("{}", usage());
            return Ok(());
        }
        Err(err) => return Err(err),
    };

    let account = load_account(&args.username)?;
    let groups = load_supplementary_groups(&account.username, account.gid)?;
    let runtime_dir = args
        .runtime_dir
        .clone()
        .unwrap_or_else(|| default_runtime_dir(account.uid));
    ensure_runtime_dir(&runtime_dir, account.uid, account.gid)?;
    let envs = build_environment(&account, &args, &runtime_dir);
    let (program, program_args) = command_for(&args)?;

    let group_clone = groups.clone();
    let mut command = Command::new(&program);
    command.args(&program_args);
    command.env_clear();
    command.envs(&envs);
    command.uid(account.uid);
    command.gid(account.gid);
    unsafe {
        command.pre_exec(move || apply_groups(&group_clone));
    }

    let error = command.exec();
    Err(format!("failed to exec {program}: {error}"))
}

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-session-launch: {err}");
        eprintln!("{}", usage());
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_command_mode() {
        let parsed = parse_args_from(vec![
            String::from("--username"),
            String::from("greeter"),
            String::from("--mode"),
            String::from("command"),
            String::from("--vt"),
            String::from("7"),
            String::from("--runtime-dir"),
            String::from("/tmp/greeter"),
            String::from("--wayland-display"),
            String::from("wayland-7"),
            String::from("--command"),
            String::from("/usr/bin/redbear-greeter-ui"),
            String::from("--fullscreen"),
        ])
        .expect("command mode should parse");

        assert_eq!(parsed.username, "greeter");
        assert_eq!(parsed.vt, 7);
        assert_eq!(parsed.runtime_dir, Some(PathBuf::from("/tmp/greeter")));
        assert_eq!(parsed.wayland_display, "wayland-7");
        assert_eq!(
            parsed.mode,
            LaunchMode::Command {
                program: String::from("/usr/bin/redbear-greeter-ui"),
                args: vec![String::from("--fullscreen")],
            }
        );
    }

    #[test]
    fn parse_args_requires_command_when_mode_is_command() {
        assert_eq!(
            parse_args_from(vec![
                String::from("--username"),
                String::from("greeter"),
                String::from("--mode"),
                String::from("command"),
            ]),
            Err(String::from("--command is required when --mode=command"))
        );
    }

    #[test]
    fn parse_args_rejects_unknown_mode() {
        assert_eq!(
            parse_args_from(vec![
                String::from("--username"),
                String::from("user"),
                String::from("--mode"),
                String::from("bogus"),
            ]),
            Err(String::from("unsupported launch mode 'bogus'"))
        );
    }

    #[test]
    fn parse_passwd_accepts_basic_entries() {
        let accounts = parse_passwd("root:x:0:0:root:/root:/usr/bin/ion\nuser:x:1000:1000:User:/home/user:/usr/bin/ion\n")
            .expect("passwd should parse");
        assert_eq!(accounts["root"].uid, 0);
        assert_eq!(accounts["user"].home, "/home/user");
    }

    #[test]
    fn parse_passwd_accepts_redox_style_layout() {
        let accounts = parse_passwd("greeter;101;101;Greeter;/nonexistent;/usr/bin/ion\n")
            .expect("redox passwd layout should parse");
        let greeter = accounts.get("greeter").expect("greeter entry should exist");
        assert_eq!(greeter.uid, 101);
        assert_eq!(greeter.gid, 101);
        assert_eq!(greeter.home, "/nonexistent");
        assert_eq!(greeter.shell, "/usr/bin/ion");
    }

    #[test]
    fn parse_groups_collects_members() {
        let groups = parse_groups("sudo:x:1:user,root\nusers:x:1000:user\n").expect("group should parse");
        assert_eq!(groups[0].gid, 1);
        assert_eq!(groups[0].members, vec![String::from("user"), String::from("root")]);
    }

    #[test]
    fn parse_groups_accepts_redox_style_layout() {
        let groups = parse_groups("greeter;x;101;greeter\n").expect("redox group should parse");
        assert_eq!(groups[0].gid, 101);
        assert_eq!(groups[0].members, vec![String::from("greeter")]);
    }

    #[test]
    fn build_environment_sets_kde_session_values() {
        let account = Account {
            username: String::from("user"),
            uid: 1000,
            gid: 1000,
            home: String::from("/home/user"),
            shell: String::from("/usr/bin/ion"),
        };
        let args = Args {
            username: String::from("user"),
            vt: 3,
            session: String::from("kde-wayland"),
            runtime_dir: None,
            wayland_display: String::from("wayland-0"),
            mode: LaunchMode::Session,
        };

        let envs = build_environment(&account, &args, Path::new("/run/user/1000"));
        assert_eq!(envs["XDG_CURRENT_DESKTOP"], "KDE");
        assert_eq!(envs["KDE_FULL_SESSION"], "true");
        assert_eq!(envs["XDG_VTNR"], "3");
    }

    #[test]
    fn build_environment_omits_kde_session_values_for_command_mode() {
        let account = Account {
            username: String::from("greeter"),
            uid: 101,
            gid: 101,
            home: String::from("/nonexistent"),
            shell: String::from("/usr/bin/ion"),
        };
        let args = Args {
            username: String::from("greeter"),
            vt: 3,
            session: String::from("kde-wayland"),
            runtime_dir: None,
            wayland_display: String::from("wayland-0"),
            mode: LaunchMode::Command {
                program: String::from("/usr/bin/redbear-greeter-ui"),
                args: Vec::new(),
            },
        };

        let envs = build_environment(&account, &args, Path::new("/tmp/run/greeter"));
        assert!(!envs.contains_key("XDG_CURRENT_DESKTOP"));
        assert!(!envs.contains_key("KDE_FULL_SESSION"));
        assert_eq!(envs["XDG_SESSION_TYPE"], "wayland");
    }

    #[test]
    fn command_for_rejects_unknown_session_name() {
        let args = Args {
            username: String::from("user"),
            vt: 3,
            session: String::from("plasma-x11"),
            runtime_dir: None,
            wayland_display: String::from("wayland-0"),
            mode: LaunchMode::Session,
        };

        assert_eq!(
            command_for(&args),
            Err(String::from("unsupported session 'plasma-x11'"))
        );
    }
}
