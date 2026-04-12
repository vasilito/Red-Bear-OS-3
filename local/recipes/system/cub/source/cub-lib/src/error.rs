use thiserror::Error;

#[derive(Error, Debug)]
pub enum CubError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("Invalid RBPKGBUILD: {0}")]
    InvalidPkgbuild(String),
    #[error("Build failed: {0}")]
    BuildFailed(String),
    #[error("Package not found: {0}")]
    PackageNotFound(String),
    #[error("Conversion error: {0}")]
    Conversion(String),
    #[error("Dependency resolution failed: {0}")]
    Dependency(String),
    #[error("Sandbox error: {0}")]
    Sandbox(String),
}
