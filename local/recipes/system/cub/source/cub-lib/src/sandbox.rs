use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::CubError;

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    pub target: String,
    pub gnu_target: String,
    pub destdir: PathBuf,
    pub prefix: String,
    pub cores: u32,
    pub allow_network: bool,
    pub source_dir: PathBuf,
    pub build_dir: PathBuf,
    pub stage_dir: PathBuf,
    pub sysroot_dir: PathBuf,
}

impl SandboxConfig {
    pub fn new(source_dir: &Path) -> Self {
        let root = source_dir.join(".cub-sandbox");
        let build_dir = root.join("build");
        let stage_dir = root.join("stage");
        let sysroot_dir = root.join("sysroot");

        Self {
            target: "x86_64-unknown-redox".to_string(),
            gnu_target: "x86_64-redox".to_string(),
            destdir: stage_dir.clone(),
            prefix: "/usr".to_string(),
            cores: std::thread::available_parallelism()
                .map(|count| count.get() as u32)
                .unwrap_or(1),
            allow_network: false,
            source_dir: source_dir.to_path_buf(),
            build_dir,
            stage_dir,
            sysroot_dir,
        }
    }

    pub fn env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        let current_path = std::env::var("PATH").unwrap_or_default();
        let tool_path = self.sysroot_dir.join("bin");

        env.insert(
            "COOKBOOK_SOURCE".to_string(),
            self.source_dir.display().to_string(),
        );
        env.insert(
            "COOKBOOK_STAGE".to_string(),
            self.stage_dir.display().to_string(),
        );
        env.insert(
            "COOKBOOK_SYSROOT".to_string(),
            self.sysroot_dir.display().to_string(),
        );
        env.insert("COOKBOOK_TARGET".to_string(), self.target.clone());
        env.insert(
            "COOKBOOK_HOST_TARGET".to_string(),
            "x86_64-unknown-linux-gnu".to_string(),
        );
        env.insert("COOKBOOK_MAKE_JOBS".to_string(), self.cores.to_string());
        env.insert("DESTDIR".to_string(), self.stage_dir.display().to_string());
        env.insert("TARGET".to_string(), self.target.clone());
        env.insert("GNU_TARGET".to_string(), self.gnu_target.clone());
        env.insert(
            "PATH".to_string(),
            if current_path.is_empty() {
                tool_path.display().to_string()
            } else {
                format!("{}:{}", tool_path.display(), current_path)
            },
        );

        env
    }

    pub fn setup(&self) -> Result<(), CubError> {
        for dir in [
            &self.build_dir,
            &self.stage_dir,
            &self.sysroot_dir,
            &self.destdir,
        ] {
            fs::create_dir_all(dir).map_err(|err| {
                CubError::Sandbox(format!("failed to create {}: {err}", dir.display()))
            })?;
        }

        Ok(())
    }

    pub fn cleanup(&self) -> Result<(), CubError> {
        let mut dirs = BTreeSet::new();
        dirs.insert(self.destdir.clone());
        dirs.insert(self.stage_dir.clone());
        dirs.insert(self.build_dir.clone());
        dirs.insert(self.sysroot_dir.clone());

        for dir in dirs.into_iter().rev() {
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|err| {
                    CubError::Sandbox(format!("failed to remove {}: {err}", dir.display()))
                })?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn builds_expected_defaults() {
        let temp = tempdir().expect("tempdir");
        let sandbox = SandboxConfig::new(temp.path());

        assert_eq!(sandbox.target, "x86_64-unknown-redox");
        assert_eq!(sandbox.gnu_target, "x86_64-redox");
        assert_eq!(sandbox.prefix, "/usr");
        assert!(sandbox.cores >= 1);
    }

    #[test]
    fn exposes_cookbook_environment() {
        let temp = tempdir().expect("tempdir");
        let sandbox = SandboxConfig::new(temp.path());
        let env = sandbox.env_vars();

        assert_eq!(
            env.get("COOKBOOK_TARGET"),
            Some(&"x86_64-unknown-redox".to_string())
        );
        assert_eq!(env.get("GNU_TARGET"), Some(&"x86_64-redox".to_string()));
        assert!(env
            .get("PATH")
            .expect("PATH set")
            .starts_with(&sandbox.sysroot_dir.join("bin").display().to_string()));
    }

    #[test]
    fn sets_up_and_cleans_directories() {
        let temp = tempdir().expect("tempdir");
        let sandbox = SandboxConfig::new(temp.path());

        sandbox.setup().expect("setup sandbox");
        assert!(sandbox.build_dir.exists());
        assert!(sandbox.stage_dir.exists());
        assert!(sandbox.sysroot_dir.exists());

        sandbox.cleanup().expect("cleanup sandbox");
        assert!(!sandbox.build_dir.exists());
        assert!(!sandbox.stage_dir.exists());
        assert!(!sandbox.sysroot_dir.exists());
    }
}
