use std::fs;
use std::path::Path;

use crate::error::CubError;
use crate::rbpkgbuild::{RbPkgBuild, SourceType};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RbSrcInfo {
    pub pkgname: String,
    pub pkgver: String,
    pub pkgrel: u32,
    pub pkgdesc: String,
    pub arch: String,
    pub depends: Vec<String>,
    pub makedepends: Vec<String>,
    pub source: Vec<String>,
    pub sha256sums: Vec<String>,
    pub provides: Vec<String>,
    pub conflicts: Vec<String>,
}

impl RbSrcInfo {
    pub fn from_file(path: impl AsRef<Path>) -> Result<RbSrcInfo, CubError> {
        let contents = fs::read_to_string(path)?;
        Self::from_str(&contents)
    }

    pub fn to_string(&self) -> String {
        let mut lines = Vec::new();

        push_scalar(&mut lines, "pkgname", &self.pkgname);
        push_scalar(&mut lines, "pkgver", &self.pkgver);
        lines.push(format!("pkgrel = {}", self.pkgrel));
        push_scalar(&mut lines, "pkgdesc", &self.pkgdesc);
        push_scalar(&mut lines, "arch", &self.arch);

        push_list(&mut lines, "depends", &self.depends);
        push_list(&mut lines, "makedepends", &self.makedepends);
        push_list(&mut lines, "source", &self.source);
        push_list(&mut lines, "sha256sums", &self.sha256sums);
        push_list(&mut lines, "provides", &self.provides);
        push_list(&mut lines, "conflicts", &self.conflicts);

        let mut output = lines.join("\n");
        output.push('\n');
        output
    }

    pub fn from_rbpkgbuild(rb: &RbPkgBuild) -> Self {
        let mut sha256sums = Vec::new();
        let source = rb
            .source
            .sources
            .iter()
            .map(|entry| {
                if matches!(entry.source_type, SourceType::Tar) && !entry.sha256.is_empty() {
                    sha256sums.push(entry.sha256.clone());
                }
                entry.url.clone()
            })
            .collect();

        Self {
            pkgname: rb.package.name.clone(),
            pkgver: rb.package.version.clone(),
            pkgrel: rb.package.release,
            pkgdesc: rb.package.description.clone(),
            arch: rb
                .package
                .architectures
                .first()
                .cloned()
                .unwrap_or_else(|| "x86_64-unknown-redox".to_string()),
            depends: rb.dependencies.runtime.clone(),
            makedepends: rb.dependencies.build.clone(),
            source,
            sha256sums,
            provides: rb.dependencies.provides.clone(),
            conflicts: rb.dependencies.conflicts.clone(),
        }
    }

    fn from_str(contents: &str) -> Result<RbSrcInfo, CubError> {
        let mut info = RbSrcInfo::default();

        for raw_line in contents.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "pkgname" => info.pkgname = value.to_string(),
                "pkgver" => info.pkgver = value.to_string(),
                "pkgrel" => {
                    info.pkgrel = value.parse().map_err(|_| {
                        CubError::InvalidPkgbuild(format!("invalid pkgrel in .RBSRCINFO: {value}"))
                    })?
                }
                "pkgdesc" => info.pkgdesc = value.to_string(),
                "arch" => info.arch = value.to_string(),
                "depends" => info.depends.push(value.to_string()),
                "makedepends" => info.makedepends.push(value.to_string()),
                "source" => info.source.push(value.to_string()),
                "sha256sums" => info.sha256sums.push(value.to_string()),
                "provides" => info.provides.push(value.to_string()),
                "conflicts" => info.conflicts.push(value.to_string()),
                _ => {}
            }
        }

        Ok(info)
    }
}

fn push_scalar(lines: &mut Vec<String>, key: &str, value: &str) {
    if !value.is_empty() {
        lines.push(format!("{key} = {value}"));
    }
}

fn push_list(lines: &mut Vec<String>, key: &str, values: &[String]) {
    for value in values {
        if !value.is_empty() {
            lines.push(format!("{key} = {value}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbpkgbuild::{
        BuildSection, CompatSection, ConversionStatus, DependenciesSection, InstallSection,
        PackageSection, PatchesSection, PolicySection, RbPkgBuild, SourceEntry, SourceSection,
        SourceType,
    };
    use tempfile::NamedTempFile;

    fn sample_rbpkgbuild() -> RbPkgBuild {
        RbPkgBuild {
            format: 1,
            package: PackageSection {
                name: "demo".to_string(),
                version: "1.2.3".to_string(),
                release: 4,
                description: "Demo package".to_string(),
                homepage: String::new(),
                license: vec!["MIT".to_string()],
                architectures: vec!["x86_64-unknown-redox".to_string()],
                maintainers: Vec::new(),
            },
            source: SourceSection {
                sources: vec![SourceEntry {
                    source_type: SourceType::Tar,
                    url: "https://example.com/demo.tar.xz".to_string(),
                    sha256: "abc123".to_string(),
                    rev: String::new(),
                    branch: String::new(),
                }],
            },
            dependencies: DependenciesSection {
                build: vec!["cmake".to_string()],
                runtime: vec!["zlib".to_string()],
                check: Vec::new(),
                optional: Vec::new(),
                provides: vec!["demo-virtual".to_string()],
                conflicts: vec!["demo-old".to_string()],
            },
            build: BuildSection::default(),
            install: InstallSection::default(),
            patches: PatchesSection::default(),
            compat: CompatSection {
                imported_from: String::new(),
                original_pkgbuild: String::new(),
                conversion_status: ConversionStatus::Full,
                target: String::new(),
            },
            policy: PolicySection::default(),
        }
    }

    #[test]
    fn converts_from_rbpkgbuild() {
        let info = RbSrcInfo::from_rbpkgbuild(&sample_rbpkgbuild());

        assert_eq!(info.pkgname, "demo");
        assert_eq!(info.pkgver, "1.2.3");
        assert_eq!(info.pkgrel, 4);
        assert_eq!(info.depends, vec!["zlib"]);
        assert_eq!(info.makedepends, vec!["cmake"]);
        assert_eq!(info.sha256sums, vec!["abc123"]);
    }

    #[test]
    fn serializes_and_parses_round_trip() {
        let info = RbSrcInfo::from_rbpkgbuild(&sample_rbpkgbuild());
        let rendered = info.to_string();
        let reparsed = RbSrcInfo::from_str(&rendered).expect("parse .RBSRCINFO");

        assert_eq!(reparsed, info);
    }

    #[test]
    fn parses_from_file() {
        let file = NamedTempFile::new().expect("temp file");
        fs::write(
            file.path(),
            "pkgname = demo\npkgver = 1.0.0\npkgrel = 1\narch = x86_64-unknown-redox\n",
        )
        .expect("write .RBSRCINFO");

        let info = RbSrcInfo::from_file(file.path()).expect("read .RBSRCINFO");
        assert_eq!(info.pkgname, "demo");
        assert_eq!(info.pkgver, "1.0.0");
        assert_eq!(info.pkgrel, 1);
        assert_eq!(info.arch, "x86_64-unknown-redox");
    }
}
