use std::fs;
use std::path::Path;

use serde_derive::{Deserialize, Serialize};

use crate::error::CubError;
use crate::rbsrcinfo::RbSrcInfo;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RbPkgBuild {
    pub format: u32,
    pub package: PackageSection,
    #[serde(default)]
    pub source: SourceSection,
    #[serde(default)]
    pub dependencies: DependenciesSection,
    #[serde(default)]
    pub build: BuildSection,
    #[serde(default)]
    pub install: InstallSection,
    #[serde(default)]
    pub patches: PatchesSection,
    #[serde(default)]
    pub compat: CompatSection,
    #[serde(default)]
    pub policy: PolicySection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageSection {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub release: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub license: Vec<String>,
    #[serde(default)]
    pub architectures: Vec<String>,
    #[serde(default)]
    pub maintainers: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceSection {
    #[serde(default)]
    pub sources: Vec<SourceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceEntry {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub rev: String,
    #[serde(default)]
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Tar,
    Git,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DependenciesSection {
    #[serde(default)]
    pub build: Vec<String>,
    #[serde(default)]
    pub runtime: Vec<String>,
    #[serde(default)]
    pub check: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    #[serde(default)]
    pub provides: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildSection {
    #[serde(default)]
    pub template: BuildTemplate,
    #[serde(default)]
    pub release: bool,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub build_dir: String,
    #[serde(default)]
    pub prepare: Vec<String>,
    #[serde(default)]
    pub build_script: Vec<String>,
    #[serde(default)]
    pub check: Vec<String>,
    #[serde(default)]
    pub install_script: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BuildTemplate {
    #[default]
    Custom,
    Cargo,
    Configure,
    Cmake,
    Meson,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallSection {
    #[serde(default)]
    pub bins: Vec<InstallEntry>,
    #[serde(default)]
    pub libs: Vec<InstallEntry>,
    #[serde(default)]
    pub headers: Vec<InstallEntry>,
    #[serde(default)]
    pub docs: Vec<InstallEntry>,
    #[serde(default)]
    pub man: Vec<InstallEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallEntry {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchesSection {
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompatSection {
    #[serde(default)]
    pub imported_from: String,
    #[serde(default)]
    pub original_pkgbuild: String,
    #[serde(default)]
    pub conversion_status: ConversionStatus,
    #[serde(default)]
    pub target: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConversionStatus {
    #[default]
    Full,
    Partial,
    Manual,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySection {
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default = "default_true")]
    pub allow_tests: bool,
    #[serde(default = "default_true")]
    pub review_required: bool,
}

fn default_true() -> bool {
    true
}

impl RbPkgBuild {
    pub fn from_file(path: impl AsRef<Path>) -> Result<RbPkgBuild, CubError> {
        let contents = fs::read_to_string(path)?;
        Self::from_str(&contents)
    }

    pub fn from_str(s: &str) -> Result<RbPkgBuild, CubError> {
        let parsed: RbPkgBuild = toml::from_str(s)?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn to_toml(&self) -> Result<String, CubError> {
        self.validate()?;
        toml::to_string_pretty(self).map_err(CubError::from)
    }

    pub fn validate(&self) -> Result<(), CubError> {
        if self.format != 1 {
            return Err(CubError::InvalidPkgbuild(format!(
                "unsupported format {}, expected 1",
                self.format
            )));
        }

        if self.package.name.is_empty() {
            return Err(CubError::InvalidPkgbuild(
                "package.name must not be empty".to_string(),
            ));
        }

        if !valid_package_name(&self.package.name) {
            return Err(CubError::InvalidPkgbuild(format!(
                "package.name must match [a-z0-9-_]+: {}",
                self.package.name
            )));
        }

        if self.package.version.trim().is_empty() {
            return Err(CubError::InvalidPkgbuild(
                "package.version must not be empty".to_string(),
            ));
        }

        if !self
            .package
            .architectures
            .iter()
            .any(|arch| arch == "x86_64-unknown-redox")
        {
            return Err(CubError::InvalidPkgbuild(
                "package.architectures must include x86_64-unknown-redox".to_string(),
            ));
        }

        for source in &self.source.sources {
            if source.url.trim().is_empty() {
                return Err(CubError::InvalidPkgbuild(
                    "source entry url must not be empty".to_string(),
                ));
            }

            if matches!(source.source_type, SourceType::Git) && source.url.contains(' ') {
                return Err(CubError::InvalidPkgbuild(format!(
                    "git source url must not contain spaces: {}",
                    source.url
                )));
            }
        }

        for (i, source) in self.source.sources.iter().enumerate() {
            match source.source_type {
                SourceType::Tar => {
                    if source.sha256.is_empty() {
                        return Err(CubError::InvalidPkgbuild(format!(
                            "source[{}]: tar source requires sha256 checksum",
                            i
                        )));
                    }
                }
                SourceType::Git => {
                    if source.rev.is_empty() && source.branch.is_empty() {
                        // Warning only for MVP: some git sources intentionally track default branch.
                    }
                }
            }
        }

        if matches!(self.build.template, BuildTemplate::Custom)
            && self.build.prepare.is_empty()
            && self.build.build_script.is_empty()
            && self.build.install_script.is_empty()
            && self.install.bins.is_empty()
            && self.install.libs.is_empty()
            && self.install.headers.is_empty()
            && self.install.docs.is_empty()
            && self.install.man.is_empty()
        {
            return Err(CubError::InvalidPkgbuild(
                "custom builds require prepare/build/install instructions".to_string(),
            ));
        }

        Ok(())
    }

    pub fn to_srcinfo(&self) -> RbSrcInfo {
        RbSrcInfo::from_rbpkgbuild(self)
    }
}

fn valid_package_name(name: &str) -> bool {
    name.chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    const SAMPLE_TOML: &str = r#"
format = 1

[package]
name = "demo-pkg"
version = "1.0.0"
release = 1
description = "demo package"
homepage = "https://example.com"
license = ["MIT"]
architectures = ["x86_64-unknown-redox", "aarch64-unknown-redox"]
maintainers = ["Red Bear OS"]

[source]
sources = [
    { type = "git", url = "https://example.com/repo.git", rev = "abc123", branch = "main" }
]

[dependencies]
build = ["cargo"]
runtime = ["openssl3"]

[build]
template = "cargo"
release = true
features = ["std"]

[policy]
allow_network = false
"#;

    #[test]
    fn parses_valid_rbpkgbuild() {
        let pkg = RbPkgBuild::from_str(SAMPLE_TOML).expect("parse RBPKGBUILD");

        assert_eq!(pkg.format, 1);
        assert_eq!(pkg.package.name, "demo-pkg");
        assert_eq!(pkg.build.template, BuildTemplate::Cargo);
        assert!(pkg.build.release);
    }

    #[test]
    fn rejects_invalid_name() {
        let invalid = SAMPLE_TOML.replace("demo-pkg", "DemoPkg");
        let err = RbPkgBuild::from_str(&invalid).expect_err("invalid name should fail");

        assert!(matches!(err, CubError::InvalidPkgbuild(_)));
    }

    #[test]
    fn rejects_missing_redox_architecture() {
        let invalid = SAMPLE_TOML.replace(
            "[\"x86_64-unknown-redox\", \"aarch64-unknown-redox\"]",
            "[\"x86_64-unknown-linux-gnu\"]",
        );
        let err = RbPkgBuild::from_str(&invalid).expect_err("missing redox arch should fail");

        assert!(matches!(err, CubError::InvalidPkgbuild(_)));
    }

    #[test]
    fn rejects_tar_source_without_sha256() {
        let invalid = SAMPLE_TOML.replace(
            r#"{ type = "git", url = "https://example.com/repo.git", rev = "abc123", branch = "main" }"#,
            r#"{ type = "tar", url = "https://example.com/demo.tar.gz" }"#,
        );
        let err =
            RbPkgBuild::from_str(&invalid).expect_err("tar source without sha256 should fail");

        assert!(matches!(err, CubError::InvalidPkgbuild(_)));
    }

    #[test]
    fn round_trips_to_toml() {
        let pkg = RbPkgBuild::from_str(SAMPLE_TOML).expect("parse RBPKGBUILD");
        let toml = pkg.to_toml().expect("serialize RBPKGBUILD");
        let reparsed = RbPkgBuild::from_str(&toml).expect("reparse RBPKGBUILD");

        assert_eq!(reparsed.package.name, "demo-pkg");
        assert_eq!(reparsed.build.features, vec!["std"]);
    }

    #[test]
    fn parses_from_file() {
        let file = NamedTempFile::new().expect("temp file");
        fs::write(file.path(), SAMPLE_TOML).expect("write RBPKGBUILD");

        let pkg = RbPkgBuild::from_file(file.path()).expect("read RBPKGBUILD");
        assert_eq!(pkg.package.version, "1.0.0");
    }

    #[test]
    fn converts_to_srcinfo() {
        let pkg = RbPkgBuild::from_str(SAMPLE_TOML).expect("parse RBPKGBUILD");
        let srcinfo = pkg.to_srcinfo();

        assert_eq!(srcinfo.pkgname, "demo-pkg");
        assert_eq!(srcinfo.pkgver, "1.0.0");
        assert_eq!(srcinfo.makedepends, vec!["cargo"]);
        assert_eq!(srcinfo.depends, vec!["openssl3"]);
    }
}
