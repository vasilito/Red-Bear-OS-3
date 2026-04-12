use std::path::Path;

use serde_derive::Serialize;

use crate::error::CubError;
use crate::rbpkgbuild::RbPkgBuild;

pub struct PackageCreator {
    pub name: String,
    pub version: String,
    pub target: String,
}

impl PackageCreator {
    pub fn create_from_stage(
        stage_dir: &Path,
        output_path: &Path,
        secret_key_path: &Path,
    ) -> Result<(), CubError> {
        if !stage_dir.is_dir() {
            return Err(CubError::PackageNotFound(format!(
                "stage directory does not exist: {}",
                stage_dir.display()
            )));
        }

        pkgar_keys::get_skey(secret_key_path).map_err(|err| {
            CubError::BuildFailed(format!(
                "failed to load pkgar secret key {}: {err}",
                secret_key_path.display()
            ))
        })?;

        pkgar::folder_entries(stage_dir).map_err(|err| {
            CubError::BuildFailed(format!(
                "failed to scan stage directory {}: {err}",
                stage_dir.display()
            ))
        })?;

        let flags = pkgar_core::HeaderFlags::latest(
            pkgar_core::Architecture::Independent,
            pkgar_core::Packaging::Uncompressed,
        );
        pkgar::create_with_flags(secret_key_path, output_path, stage_dir, flags).map_err(|err| {
            CubError::BuildFailed(format!(
                "failed to create pkgar archive {}: {err}",
                output_path.display()
            ))
        })
    }

    pub fn generate_package_toml(rbpkg: &RbPkgBuild) -> String {
        #[derive(Serialize)]
        struct PackageMetadata {
            name: String,
            version: String,
            target: String,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            depends: Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            optdepends: Vec<String>,
        }

        let metadata = PackageMetadata {
            name: rbpkg.package.name.clone(),
            version: if rbpkg.package.release > 0 {
                format!("{}-{}", rbpkg.package.version, rbpkg.package.release)
            } else {
                rbpkg.package.version.clone()
            },
            target: rbpkg
                .package
                .architectures
                .first()
                .cloned()
                .unwrap_or_else(|| "x86_64-unknown-redox".to_string()),
            depends: rbpkg.dependencies.runtime.clone(),
            optdepends: rbpkg.dependencies.optional.clone(),
        };

        match toml::to_string_pretty(&metadata) {
            Ok(rendered) => rendered,
            Err(_) => format!(
                "name = \"{}\"\nversion = \"{}\"\ntarget = \"{}\"\n",
                metadata.name, metadata.version, metadata.target
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbpkgbuild::{
        BuildSection, CompatSection, ConversionStatus, DependenciesSection, InstallSection,
        PackageSection, PatchesSection, PolicySection, RbPkgBuild, SourceSection,
    };
    use tempfile::tempdir;

    fn sample_rbpkgbuild() -> RbPkgBuild {
        RbPkgBuild {
            format: 1,
            package: PackageSection {
                name: "demo".to_string(),
                version: "1.0.0".to_string(),
                release: 1,
                description: "demo package".to_string(),
                homepage: String::new(),
                license: Vec::new(),
                architectures: vec!["x86_64-unknown-redox".to_string()],
                maintainers: Vec::new(),
            },
            source: SourceSection::default(),
            dependencies: DependenciesSection {
                build: Vec::new(),
                runtime: vec!["openssl3".to_string()],
                check: Vec::new(),
                optional: Vec::new(),
                provides: vec!["demo-virtual".to_string()],
                conflicts: vec!["demo-old".to_string()],
            },
            build: BuildSection {
                build_script: vec!["make".to_string()],
                install_script: vec!["make install".to_string()],
                ..BuildSection::default()
            },
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
    fn generates_package_toml() {
        let mut rbpkg = sample_rbpkgbuild();
        rbpkg.dependencies.optional = vec!["git".to_string()];

        let rendered = PackageCreator::generate_package_toml(&rbpkg);

        assert!(rendered.contains("name = \"demo\""));
        assert!(rendered.contains("version = \"1.0.0-1\""));
        assert!(rendered.contains("target = \"x86_64-unknown-redox\""));
        assert!(rendered.contains("depends = [\"openssl3\"]"));
        assert!(rendered.contains("optdepends = [\"git\"]"));
        assert!(!rendered.contains("dependencies ="));
    }

    #[test]
    fn errors_when_stage_dir_is_missing() {
        let temp = tempdir().expect("tempdir");
        let err = PackageCreator::create_from_stage(
            &temp.path().join("missing-stage"),
            &temp.path().join("out.pkgar"),
            &temp.path().join("secret.toml"),
        )
        .expect_err("missing stage should fail");

        assert!(matches!(err, CubError::PackageNotFound(_)));
    }
}
