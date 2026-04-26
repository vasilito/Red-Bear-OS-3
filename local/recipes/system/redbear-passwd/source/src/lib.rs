use std::{
    collections::HashMap,
    fs,
    path::Path,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountFormat {
    Redox,
    Unix,
}

/// Detect whether a passwd/shadow/group line uses Redox (`;`) or Unix (`:`) delimiters.
pub fn detect_format(line: &str) -> AccountFormat {
    if line.contains(';') {
        AccountFormat::Redox
    } else {
        AccountFormat::Unix
    }
}

/// Split a line into fields according to its detected format.
pub fn split_fields(line: &str) -> (AccountFormat, Vec<&str>) {
    let format = detect_format(line);
    let delimiter = match format {
        AccountFormat::Redox => ';',
        AccountFormat::Unix => ':',
    };
    (format, line.split(delimiter).collect())
}

/// Parse uid and gid from passwd-format fields.
pub fn parse_uid_gid(parts: &[&str], format: AccountFormat) -> Option<(u32, u32)> {
    let (uid_index, gid_index) = match format {
        AccountFormat::Redox if parts.len() >= 3 => (1, 2),
        AccountFormat::Unix if parts.len() >= 4 => (2, 3),
        _ => return None,
    };

    let uid = parts[uid_index].parse::<u32>().ok()?;
    let gid = parts[gid_index].parse::<u32>().ok()?;
    Some((uid, gid))
}

/// Load the uid/gid pair for a given username from `/etc/passwd`.
pub fn load_uid_gid(username: &str) -> Result<(u32, u32), String> {
    let passwd = fs::read_to_string("/etc/passwd")
        .map_err(|err| format!("failed to read /etc/passwd: {err}"))?;
    for line in passwd.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (format, parts) = split_fields(trimmed);
        if parts.len() < 3 || parts[0] != username {
            continue;
        }
        if let Some((uid, gid)) = parse_uid_gid(&parts, format) {
            return Ok((uid, gid));
        }
        return Err(format!("invalid uid/gid for user '{username}'"));
    }
    Err(format!("unknown user '{username}'"))
}

/// A full account entry as found in `/etc/passwd`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub username: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
}

/// Parse the contents of `/etc/passwd` into a map keyed by username.
pub fn parse_passwd(contents: &str) -> Result<HashMap<String, Account>, String> {
    let mut accounts = HashMap::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (format, parts) = split_fields(line);
        let (uid_index, gid_index, home_index, shell_index) = match format {
            AccountFormat::Redox if parts.len() >= 6 => (1, 2, 4, 5),
            AccountFormat::Unix if parts.len() >= 7 => (2, 3, 5, 6),
            AccountFormat::Redox => {
                return Err(format!("invalid Redox passwd entry on line {}", index + 1))
            }
            AccountFormat::Unix => {
                return Err(format!("invalid passwd entry on line {}", index + 1))
            }
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

/// Load a single account by username from `/etc/passwd`.
pub fn load_account(username: &str) -> Result<Account, String> {
    let passwd = fs::read_to_string("/etc/passwd")
        .map_err(|err| format!("failed to read /etc/passwd: {err}"))?;
    let accounts = parse_passwd(&passwd)?;
    accounts
        .get(username)
        .cloned()
        .ok_or_else(|| format!("unknown user '{username}'"))
}

/// Load shadow password hashes from `/etc/shadow`.
pub fn load_shadow_passwords() -> Result<HashMap<String, String>, String> {
    if !Path::new("/etc/shadow").exists() {
        return Ok(HashMap::new());
    }

    let mut passwords = HashMap::new();
    let contents = fs::read_to_string("/etc/shadow")
        .map_err(|err| format!("failed to read /etc/shadow: {err}"))?;
    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (_format, parts) = split_fields(line);
        if parts.len() < 2 {
            return Err(format!("invalid shadow entry on line {}", index + 1));
        }
        passwords.insert(parts[0].to_string(), parts[1].to_string());
    }
    Ok(passwords)
}

/// A group entry as found in `/etc/group`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupEntry {
    pub gid: u32,
    pub members: Vec<String>,
}

/// Parse the contents of `/etc/group` into a vector of group entries.
pub fn parse_groups(contents: &str) -> Result<Vec<GroupEntry>, String> {
    let mut groups = Vec::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (_format, parts) = split_fields(line);
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

/// Load supplementary group IDs for a user, including the primary gid.
pub fn load_supplementary_groups(username: &str, primary_gid: u32) -> Result<Vec<u32>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_format_redox() {
        assert_eq!(detect_format("a;b;c"), AccountFormat::Redox);
    }

    #[test]
    fn detect_format_unix() {
        assert_eq!(detect_format("a:b:c"), AccountFormat::Unix);
    }

    #[test]
    fn split_fields_redox() {
        let (format, parts) = split_fields("greeter;101;101;Greeter;/nonexistent;/usr/bin/ion");
        assert_eq!(format, AccountFormat::Redox);
        assert_eq!(parts[0], "greeter");
        assert_eq!(parts[2], "101");
    }

    #[test]
    fn split_fields_unix() {
        let (format, parts) = split_fields("root:x:0:0:root:/root:/usr/bin/ion");
        assert_eq!(format, AccountFormat::Unix);
        assert_eq!(parts[2], "0");
    }

    #[test]
    fn parse_uid_gid_redox() {
        assert_eq!(
            parse_uid_gid(&["greeter", "101", "101", "Greeter", "/nonexistent", "/usr/bin/ion"], AccountFormat::Redox),
            Some((101, 101))
        );
    }

    #[test]
    fn parse_uid_gid_unix() {
        assert_eq!(
            parse_uid_gid(&["root", "x", "0", "0", "root", "/root", "/usr/bin/ion"], AccountFormat::Unix),
            Some((0, 0))
        );
    }

    #[test]
    fn parse_passwd_basic() {
        let accounts = parse_passwd("root:x:0:0:root:/root:/usr/bin/ion\nuser:x:1000:1000:User:/home/user:/usr/bin/ion\n")
            .expect("passwd should parse");
        assert_eq!(accounts["root"].uid, 0);
        assert_eq!(accounts["user"].home, "/home/user");
    }

    #[test]
    fn parse_passwd_redox_style() {
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
    fn parse_groups_redox_style() {
        let groups = parse_groups("greeter;x;101;greeter\n").expect("redox group should parse");
        assert_eq!(groups[0].gid, 101);
        assert_eq!(groups[0].members, vec![String::from("greeter")]);
    }

    #[test]
    fn load_supplementary_groups_includes_primary() {
        let groups = parse_groups("users:x:1000:user\n").expect("group should parse");
        let gids: Vec<u32> = groups.into_iter()
            .filter(|g| g.members.iter().any(|m| m == "user"))
            .map(|g| g.gid)
            .collect();
        assert!(gids.contains(&1000));
    }
}
