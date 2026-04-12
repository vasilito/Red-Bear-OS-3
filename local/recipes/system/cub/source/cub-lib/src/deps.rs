#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedDep {
    pub original: String,
    pub mapped: String,
    pub is_exact: bool,
}

pub fn map_dependency(arch_name: &str) -> MappedDep {
    let cleaned = arch_name.trim();
    let base = dependency_base_name(cleaned);

    let (mapped, is_exact) = match base.as_str() {
        "glibc" => ("relibc".to_string(), false),
        "gcc" | "make" => ("build-base".to_string(), false),
        "pkg-config" => ("pkg-config".to_string(), true),
        "openssl" => ("openssl3".to_string(), false),
        "zlib" => ("zlib".to_string(), true),
        "libffi" => ("libffi".to_string(), true),
        "pcre2" => ("pcre2".to_string(), true),
        "ncurses" => ("ncurses".to_string(), true),
        "readline" => ("readline".to_string(), true),
        "curl" => ("curl".to_string(), true),
        "git" => ("git".to_string(), true),
        "python" => ("python".to_string(), true),
        "rust" => ("rust".to_string(), true),
        "cargo" => ("cargo".to_string(), true),
        "cmake" => ("cmake".to_string(), true),
        "meson" => ("meson".to_string(), true),
        "autoconf" => ("autoconf".to_string(), true),
        "automake" => ("automake".to_string(), true),
        "libtool" => ("libtool".to_string(), true),
        "systemd" => (String::new(), false),
        "dbus" => ("dbus".to_string(), true),
        _ => (base.clone(), true),
    };

    MappedDep {
        original: cleaned.to_string(),
        mapped,
        is_exact,
    }
}

pub fn map_dependencies(arch_deps: &[String]) -> Vec<MappedDep> {
    arch_deps.iter().map(|dep| map_dependency(dep)).collect()
}

fn dependency_base_name(name: &str) -> String {
    let trimmed = name.trim();
    let no_prefix = trimmed.strip_prefix("host:").unwrap_or(trimmed);

    no_prefix
        .chars()
        .take_while(|ch| !matches!(ch, '<' | '>' | '=' | ':' | ' ' | '\t'))
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_dependency() {
        let mapped = map_dependency("glibc");

        assert_eq!(mapped.original, "glibc");
        assert_eq!(mapped.mapped, "relibc");
        assert!(!mapped.is_exact);
    }

    #[test]
    fn keeps_unknown_dependency_name() {
        let mapped = map_dependency("expat");

        assert_eq!(mapped.mapped, "expat");
        assert!(mapped.is_exact);
    }

    #[test]
    fn strips_version_constraints() {
        let mapped = map_dependency("openssl>=1.1");

        assert_eq!(mapped.original, "openssl>=1.1");
        assert_eq!(mapped.mapped, "openssl3");
    }

    #[test]
    fn marks_unavailable_dependency() {
        let mapped = map_dependency("systemd");

        assert!(mapped.mapped.is_empty());
        assert!(!mapped.is_exact);
    }

    #[test]
    fn maps_collections() {
        let deps = vec!["glibc".to_string(), "cmake".to_string()];
        let mapped = map_dependencies(&deps);

        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0].mapped, "relibc");
        assert_eq!(mapped[1].mapped, "cmake");
    }
}
