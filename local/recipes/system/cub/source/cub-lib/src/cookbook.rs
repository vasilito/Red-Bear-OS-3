use serde_derive::Serialize;

use crate::error::CubError;
use crate::rbpkgbuild::{BuildTemplate, RbPkgBuild, SourceType};

#[derive(Debug, Serialize)]
struct CookbookRecipe {
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<CookbookSource>,
    build: CookbookBuild,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<CookbookPackage>,
}

#[derive(Debug, Default, Serialize)]
struct CookbookSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    git: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blake3: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    patches: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CookbookBuild {
    template: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
    #[serde(rename = "dev-dependencies", skip_serializing_if = "Vec::is_empty")]
    dev_dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cargoflags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    configureflags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    cmakeflags: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mesonflags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    script: Option<String>,
}

#[derive(Debug, Serialize)]
struct CookbookPackage {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

pub fn generate_recipe(rbpkg: &RbPkgBuild) -> Result<String, CubError> {
    rbpkg.validate()?;

    if rbpkg.source.sources.len() > 1 {
        return Err(CubError::Conversion(
            "Cookbook recipe generation currently supports a single primary source".to_string(),
        ));
    }

    let source = rbpkg
        .source
        .sources
        .first()
        .map(convert_source)
        .transpose()?
        .map(|mut source| {
            source.patches = rbpkg.patches.files.clone();
            source
        });
    let build = convert_build(rbpkg)?;
    let package = build_package_section(rbpkg);

    toml::to_string_pretty(&CookbookRecipe {
        source,
        build,
        package,
    })
    .map_err(CubError::from)
}

fn convert_source(source: &crate::rbpkgbuild::SourceEntry) -> Result<CookbookSource, CubError> {
    let mut cookbook = CookbookSource::default();

    match source.source_type {
        SourceType::Git => {
            cookbook.git = Some(source.url.clone());
            cookbook.branch = non_empty(&source.branch);
            cookbook.rev = non_empty(&source.rev);
        }
        SourceType::Tar => {
            cookbook.tar = Some(source.url.clone());
            cookbook.blake3 = non_empty(&source.sha256);
        }
    }

    Ok(cookbook)
}

fn convert_build(rbpkg: &RbPkgBuild) -> Result<CookbookBuild, CubError> {
    let mut build = CookbookBuild {
        template: template_name(&rbpkg.build.template).to_string(),
        dependencies: rbpkg.dependencies.build.clone(),
        dev_dependencies: rbpkg.dependencies.check.clone(),
        cargoflags: Vec::new(),
        configureflags: Vec::new(),
        cmakeflags: Vec::new(),
        mesonflags: Vec::new(),
        script: None,
    };

    match rbpkg.build.template {
        BuildTemplate::Cargo => {
            if rbpkg.build.release {
                build.cargoflags.push("--release".to_string());
            }
            if !rbpkg.build.features.is_empty() {
                build.cargoflags.push("--features".to_string());
                build.cargoflags.push(rbpkg.build.features.join(","));
            }
            build.cargoflags.extend(rbpkg.build.args.clone());
        }
        BuildTemplate::Configure => build.configureflags = rbpkg.build.args.clone(),
        BuildTemplate::Cmake => build.cmakeflags = rbpkg.build.args.clone(),
        BuildTemplate::Meson => build.mesonflags = rbpkg.build.args.clone(),
        BuildTemplate::Custom => {
            let script = custom_script(rbpkg)?;
            build.script = Some(script);
        }
    }

    Ok(build)
}

fn build_package_section(rbpkg: &RbPkgBuild) -> Option<CookbookPackage> {
    let description = non_empty(&rbpkg.package.description);
    let version = Some(if rbpkg.package.release > 0 {
        format!("{}-{}", rbpkg.package.version, rbpkg.package.release)
    } else {
        rbpkg.package.version.clone()
    });

    if rbpkg.dependencies.runtime.is_empty() && description.is_none() && version.is_none() {
        None
    } else {
        Some(CookbookPackage {
            dependencies: rbpkg.dependencies.runtime.clone(),
            version,
            description,
        })
    }
}

fn custom_script(rbpkg: &RbPkgBuild) -> Result<String, CubError> {
    let mut parts = Vec::new();

    parts.extend(rbpkg.build.prepare.iter().cloned());
    parts.extend(rbpkg.build.build_script.iter().cloned());
    if rbpkg.policy.allow_tests {
        parts.extend(rbpkg.build.check.iter().cloned());
    }
    parts.extend(rbpkg.build.install_script.iter().cloned());

    if parts.is_empty() {
        return Err(CubError::InvalidPkgbuild(
            "custom template requires at least one prepare/build/check/install command".to_string(),
        ));
    }

    Ok(parts.join("\n"))
}

fn template_name(template: &BuildTemplate) -> &'static str {
    match template {
        BuildTemplate::Custom => "custom",
        BuildTemplate::Cargo => "cargo",
        BuildTemplate::Configure => "configure",
        BuildTemplate::Cmake => "cmake",
        BuildTemplate::Meson => "meson",
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rbpkgbuild::{
        BuildSection, BuildTemplate, CompatSection, ConversionStatus, DependenciesSection,
        InstallSection, PackageSection, PatchesSection, PolicySection, RbPkgBuild, SourceEntry,
        SourceSection, SourceType,
    };

    fn base_pkg(template: BuildTemplate) -> RbPkgBuild {
        RbPkgBuild {
            format: 1,
            package: PackageSection {
                name: "demo".to_string(),
                version: "1.0.0".to_string(),
                release: 1,
                description: "demo package".to_string(),
                homepage: String::new(),
                license: vec!["MIT".to_string()],
                architectures: vec!["x86_64-unknown-redox".to_string()],
                maintainers: Vec::new(),
            },
            source: SourceSection {
                sources: vec![SourceEntry {
                    source_type: SourceType::Git,
                    url: "https://example.com/repo.git".to_string(),
                    sha256: String::new(),
                    rev: "abc123".to_string(),
                    branch: "main".to_string(),
                }],
            },
            dependencies: DependenciesSection {
                build: vec!["cargo".to_string()],
                runtime: vec!["openssl3".to_string()],
                check: vec!["python".to_string()],
                optional: Vec::new(),
                provides: Vec::new(),
                conflicts: Vec::new(),
            },
            build: BuildSection {
                template,
                release: true,
                features: vec!["cli".to_string(), "full".to_string()],
                args: vec!["--locked".to_string()],
                build_dir: String::new(),
                prepare: vec!["./autogen.sh".to_string()],
                build_script: vec!["make".to_string()],
                check: vec!["make test".to_string()],
                install_script: vec!["make install DESTDIR=\"${COOKBOOK_STAGE}\"".to_string()],
            },
            install: InstallSection::default(),
            patches: PatchesSection {
                files: vec!["redox.patch".to_string()],
            },
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
    fn generates_cargo_recipe() {
        let recipe = generate_recipe(&base_pkg(BuildTemplate::Cargo)).expect("generate recipe");
        let value: toml::Value = toml::from_str(&recipe).expect("parse generated recipe");

        assert_eq!(
            value["source"]["git"].as_str(),
            Some("https://example.com/repo.git")
        );
        assert_eq!(value["build"]["template"].as_str(), Some("cargo"));
        assert_eq!(value["build"]["dependencies"][0].as_str(), Some("cargo"));
        assert_eq!(value["source"]["patches"][0].as_str(), Some("redox.patch"));
        assert_eq!(
            value["package"]["dependencies"][0].as_str(),
            Some("openssl3")
        );
    }

    #[test]
    fn generates_tar_recipe_with_checksum() {
        let mut pkg = base_pkg(BuildTemplate::Cargo);
        pkg.source.sources[0] = SourceEntry {
            source_type: SourceType::Tar,
            url: "https://example.com/demo.tar.gz".to_string(),
            sha256: "abc123deadbeef".to_string(),
            rev: String::new(),
            branch: String::new(),
        };

        let recipe = generate_recipe(&pkg).expect("generate recipe");
        let value: toml::Value = toml::from_str(&recipe).expect("parse generated recipe");

        assert_eq!(
            value["source"]["tar"].as_str(),
            Some("https://example.com/demo.tar.gz")
        );
        assert_eq!(value["source"]["blake3"].as_str(), Some("abc123deadbeef"));
    }

    #[test]
    fn generates_custom_script() {
        let recipe = generate_recipe(&base_pkg(BuildTemplate::Custom)).expect("generate recipe");
        let value: toml::Value = toml::from_str(&recipe).expect("parse generated recipe");
        let script = value["build"]["script"].as_str().expect("custom script");

        assert!(script.contains("./autogen.sh"));
        assert!(
            script.contains("make\n") || script.ends_with("make") || script.contains("make test")
        );
        assert!(script.contains("make install"));
    }

    #[test]
    fn omits_test_commands_when_policy_disallows_them() {
        let mut pkg = base_pkg(BuildTemplate::Custom);
        pkg.policy.allow_tests = false;

        let recipe = generate_recipe(&pkg).expect("generate recipe");
        assert!(!recipe.contains("make test"));
    }
}
