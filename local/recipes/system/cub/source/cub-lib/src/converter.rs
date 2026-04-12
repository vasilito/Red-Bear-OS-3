use crate::deps::map_dependency;
use crate::error::CubError;
use crate::rbpkgbuild::{
    BuildSection, BuildTemplate, CompatSection, ConversionStatus, DependenciesSection,
    InstallSection, PackageSection, PatchesSection, PolicySection, RbPkgBuild, SourceEntry,
    SourceSection, SourceType,
};

pub struct ConversionResult {
    pub rbpkg: RbPkgBuild,
    pub report: ConversionReport,
}

pub struct ConversionReport {
    pub status: ConversionStatus,
    pub warnings: Vec<String>,
    pub actions_required: Vec<String>,
}

pub fn convert_pkgbuild(content: &str) -> Result<ConversionResult, CubError> {
    let pkgname = extract_scalar_assignment(content, "pkgname")
        .ok_or_else(|| CubError::Conversion("missing pkgname in PKGBUILD".to_string()))?;
    let pkgver = extract_scalar_assignment(content, "pkgver")
        .ok_or_else(|| CubError::Conversion("missing pkgver in PKGBUILD".to_string()))?;

    let pkgrel = extract_scalar_assignment(content, "pkgrel")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    let pkgdesc = extract_scalar_assignment(content, "pkgdesc").unwrap_or_default();
    let url = extract_scalar_assignment(content, "url").unwrap_or_default();
    let licenses = extract_array_assignment(content, "license").unwrap_or_default();
    let depends = extract_array_assignment(content, "depends").unwrap_or_default();
    let makedepends = extract_array_assignment(content, "makedepends").unwrap_or_default();
    let checkdepends = extract_array_assignment(content, "checkdepends").unwrap_or_default();
    let sources = extract_array_assignment(content, "source").unwrap_or_default();
    let sha256sums = extract_array_assignment(content, "sha256sums").unwrap_or_default();

    let template = detect_build_template(content);
    let mut warnings = detect_linuxisms(content);
    let mut actions_required = Vec::new();

    let mapped_runtime = map_dep_list(&depends, &mut warnings, &mut actions_required);
    let mapped_build = map_dep_list(&makedepends, &mut warnings, &mut actions_required);
    let mapped_check = map_dep_list(&checkdepends, &mut warnings, &mut actions_required);

    if sources.is_empty() {
        warnings.push("PKGBUILD does not define any source entries".to_string());
    }

    let status = if warnings.is_empty() && actions_required.is_empty() {
        ConversionStatus::Full
    } else {
        ConversionStatus::Partial
    };

    let rbpkg = RbPkgBuild {
        format: 1,
        package: PackageSection {
            name: sanitize_pkgname(&pkgname),
            version: pkgver,
            release: pkgrel,
            description: pkgdesc,
            homepage: url,
            license: licenses,
            architectures: vec!["x86_64-unknown-redox".to_string()],
            maintainers: Vec::new(),
        },
        source: SourceSection {
            sources: sources
                .into_iter()
                .enumerate()
                .map(|(index, source)| {
                    source_from_arch(source, sha256sums.get(index).map(String::as_str))
                })
                .collect(),
        },
        dependencies: DependenciesSection {
            build: mapped_build,
            runtime: mapped_runtime,
            check: mapped_check,
            optional: Vec::new(),
            provides: Vec::new(),
            conflicts: Vec::new(),
        },
        build: BuildSection {
            template,
            ..BuildSection::default()
        },
        install: InstallSection::default(),
        patches: PatchesSection::default(),
        compat: CompatSection {
            imported_from: "aur".to_string(),
            original_pkgbuild: content.to_string(),
            conversion_status: status.clone(),
            target: "x86_64-unknown-redox".to_string(),
        },
        policy: PolicySection::default(),
    };

    rbpkg.validate()?;
    let _ = rbpkg.to_srcinfo();

    Ok(ConversionResult {
        rbpkg,
        report: ConversionReport {
            status,
            warnings,
            actions_required,
        },
    })
}

fn map_dep_list(
    deps: &[String],
    warnings: &mut Vec<String>,
    actions_required: &mut Vec<String>,
) -> Vec<String> {
    let mut mapped = Vec::new();

    for dep in deps {
        let mapping = map_dependency(dep);
        if mapping.mapped.is_empty() {
            warnings.push(format!(
                "dependency '{}' has no Redox mapping and was omitted",
                mapping.original
            ));
            actions_required.push(format!(
                "port or replace dependency '{}' manually",
                mapping.original
            ));
            continue;
        }

        if !mapping.is_exact {
            warnings.push(format!(
                "dependency '{}' mapped to '{}'",
                mapping.original, mapping.mapped
            ));
        }

        if !mapped.contains(&mapping.mapped) {
            mapped.push(mapping.mapped);
        }
    }

    mapped
}

fn detect_build_template(content: &str) -> BuildTemplate {
    let lowered = content.to_ascii_lowercase();

    if lowered.contains("cargo build") || lowered.contains("cargo install") {
        BuildTemplate::Cargo
    } else if lowered.contains("meson setup") || lowered.contains(" meson ") {
        BuildTemplate::Meson
    } else if lowered.contains("cmake") {
        BuildTemplate::Cmake
    } else if lowered.contains("./configure") || lowered.contains(" configure ") {
        BuildTemplate::Configure
    } else {
        BuildTemplate::Custom
    }
}

fn detect_linuxisms(content: &str) -> Vec<String> {
    let lowered = content.to_ascii_lowercase();
    let checks = [
        (
            "systemctl",
            "uses systemctl, which is not available on Redox",
        ),
        (
            "/usr/lib/systemd",
            "references /usr/lib/systemd, which is Linux-specific",
        ),
        (
            "systemd",
            "references systemd, which is unavailable on Redox",
        ),
        (
            "/proc",
            "references /proc, which may require Redox-specific adaptation",
        ),
    ];

    let mut warnings = Vec::new();
    for (needle, warning) in checks {
        if lowered.contains(needle) {
            warnings.push(warning.to_string());
        }
    }
    warnings
}

fn sanitize_pkgname(name: &str) -> String {
    name.trim_matches('"')
        .to_ascii_lowercase()
        .replace('_', "-")
}

fn source_from_arch(entry: String, sha256: Option<&str>) -> SourceEntry {
    let normalized = normalize_source_entry(&entry);
    let source_type = if normalized.starts_with("git+")
        || normalized.starts_with("git://")
        || normalized.ends_with(".git")
    {
        SourceType::Git
    } else {
        SourceType::Tar
    };

    SourceEntry {
        sha256: if matches!(source_type, SourceType::Tar) {
            sha256.unwrap_or_default().to_string()
        } else {
            String::new()
        },
        url: normalized,
        source_type,
        rev: String::new(),
        branch: String::new(),
    }
}

fn normalize_source_entry(entry: &str) -> String {
    let stripped = entry
        .split_once("::")
        .map(|(_, value)| value)
        .unwrap_or(entry)
        .trim();

    stripped
        .strip_prefix("git+")
        .unwrap_or(stripped)
        .to_string()
}

fn extract_scalar_assignment(content: &str, name: &str) -> Option<String> {
    extract_assignment(content, name).map(|raw| parse_scalar(&raw))
}

fn extract_array_assignment(content: &str, name: &str) -> Option<Vec<String>> {
    extract_assignment(content, name).map(|raw| parse_array(&raw))
}

fn extract_assignment(content: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    let mut lines = content.lines();

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(&prefix) {
            continue;
        }

        let mut value = trimmed[prefix.len()..].trim().to_string();
        if value.starts_with('(') {
            let mut depth = paren_balance(&value);
            while depth > 0 {
                let Some(next) = lines.next() else {
                    break;
                };
                value.push('\n');
                value.push_str(next.trim());
                depth += paren_balance(next);
            }
        } else {
            while value.ends_with('\\') {
                value.pop();
                let Some(next) = lines.next() else {
                    break;
                };
                value.push(' ');
                value.push_str(next.trim());
            }
        }

        return Some(value);
    }

    None
}

fn paren_balance(input: &str) -> i32 {
    let opens = input.chars().filter(|ch| *ch == '(').count() as i32;
    let closes = input.chars().filter(|ch| *ch == ')').count() as i32;
    opens - closes
}

fn parse_scalar(raw: &str) -> String {
    let binding = strip_unquoted_comment(raw);
    let stripped = binding.trim();
    if let Some(unquoted) = unquote(stripped) {
        unquoted
    } else {
        stripped.to_string()
    }
}

fn parse_array(raw: &str) -> Vec<String> {
    let binding = strip_unquoted_comment(raw);
    let trimmed = binding.trim();
    let inner = trimmed
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or(trimmed);

    shell_split(inner)
}

fn strip_unquoted_comment(input: &str) -> String {
    let mut single = false;
    let mut double = false;
    let mut result = String::new();

    for ch in input.chars() {
        match ch {
            '\'' if !double => {
                single = !single;
                result.push(ch);
            }
            '"' if !single => {
                double = !double;
                result.push(ch);
            }
            '#' if !single && !double => break,
            _ => result.push(ch),
        }
    }

    result
}

fn unquote(value: &str) -> Option<String> {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[value.len() - 1] as char;
        if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
            return Some(value[1..value.len() - 1].to_string());
        }
    }
    None
}

fn shell_split(input: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' => escape = true,
            '\'' | '"' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
            '#' if quote.is_none() => break,
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    items.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        items.push(current);
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    const PKGBUILD: &str = r#"
pkgname=demo_pkg
pkgver=1.2.3
pkgrel=4
pkgdesc="Demo application"
url="https://example.com/demo"
license=('MIT')
depends=('glibc' 'openssl>=1.1' 'systemd')
makedepends=('cargo' 'pkg-config')
checkdepends=('python')
source=('https://example.com/demo-1.2.3.tar.xz')
sha256sums=('abc123deadbeef')

build() {
    cargo build --release
}

package() {
    install -Dm755 target/release/demo "$pkgdir/usr/bin/demo"
    systemctl --version >/dev/null
}
"#;

    #[test]
    fn converts_pkgbuild_to_rbpkgbuild() {
        let result = convert_pkgbuild(PKGBUILD).expect("convert PKGBUILD");

        assert_eq!(result.rbpkg.package.name, "demo-pkg");
        assert_eq!(result.rbpkg.package.version, "1.2.3");
        assert_eq!(result.rbpkg.package.release, 4);
        assert_eq!(result.rbpkg.build.template, BuildTemplate::Cargo);
        assert_eq!(
            result.rbpkg.dependencies.runtime,
            vec!["relibc", "openssl3"]
        );
        assert_eq!(result.rbpkg.dependencies.build, vec!["cargo", "pkg-config"]);
        assert_eq!(result.rbpkg.dependencies.check, vec!["python"]);
        assert_eq!(result.rbpkg.source.sources.len(), 1);
        assert_eq!(result.rbpkg.source.sources[0].sha256, "abc123deadbeef");
    }

    #[test]
    fn reports_linuxisms_and_unmapped_deps() {
        let result = convert_pkgbuild(PKGBUILD).expect("convert PKGBUILD");

        assert!(matches!(result.report.status, ConversionStatus::Partial));
        assert!(result
            .report
            .warnings
            .iter()
            .any(|w| w.contains("systemctl")));
        assert!(result
            .report
            .actions_required
            .iter()
            .any(|w| w.contains("systemd")));
    }

    #[test]
    fn parses_multiline_arrays() {
        let input = "depends=(\n  'glibc'\n  'zlib'\n)\n";
        let parsed = extract_array_assignment(input, "depends").expect("depends array");

        assert_eq!(parsed, vec!["glibc", "zlib"]);
    }

    #[test]
    fn detects_meson_template() {
        let input = "pkgname=demo\npkgver=1\nmeson setup build\n";
        assert_eq!(detect_build_template(input), BuildTemplate::Meson);
    }
}
