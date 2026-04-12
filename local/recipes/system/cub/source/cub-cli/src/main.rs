use std::cell::RefCell;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use cub::converter::{self, ConversionReport, ConversionResult};
use cub::error::CubError;
use cub::rbpkgbuild::RbPkgBuild;
use cub::rbsrcinfo::RbSrcInfo;
use cub::sandbox::SandboxConfig;
use pkg::callback::IndicatifCallback;
use pkg::{Library, PackageName};

const DEFAULT_TARGET: &str = "x86_64-unknown-redox";
const HOST_INSTALL_PATH: &str = "/tmp/pkg_install";
const REDOX_INSTALL_PATH: &str = "/";
const PKG_DOWNLOAD_DIR: &str = "/tmp/pkg_download/";
const CUB_CACHE_DIR: &str = "/tmp/cub_cache/";
const DEFAULT_BUR_REPO_URL: &str = "https://gitlab.redox-os.org/redox-os/bur.git";
const DEFAULT_AUR_BASE_URL: &str = "https://aur.archlinux.org";
const PUBLIC_KEY_FILE: &str = "id_ed25519.pub.toml";
const DEFAULT_SECRET_KEY_FILE: &str = "id_ed25519.toml";

struct CookbookAdapter;

impl CookbookAdapter {
    fn write_recipe_dir(rbpkg: &RbPkgBuild, recipe_dir: &Path) -> Result<(), CubError> {
        fs::create_dir_all(recipe_dir)?;
        let recipe = cub::cookbook::generate_recipe(rbpkg)?;
        fs::write(recipe_dir.join("recipe.toml"), recipe)?;
        Ok(())
    }
}

struct PkgbuildConverter;

impl PkgbuildConverter {
    fn convert(content: &str) -> Result<ConversionResult, CubError> {
        converter::convert_pkgbuild(content)
    }
}

struct PackageCreator;

impl PackageCreator {
    fn create_pkgar(
        stage_dir: &Path,
        output_path: &Path,
        secret_key_path: &Path,
    ) -> Result<(), CubError> {
        cub::package::PackageCreator::create_from_stage(stage_dir, output_path, secret_key_path)
    }

    fn generate_package_toml(rbpkg: &RbPkgBuild) -> String {
        cub::package::PackageCreator::generate_package_toml(rbpkg)
    }
}

#[derive(Debug, Parser)]
#[command(name = "cub")]
#[command(version)]
#[command(about = "Red Bear OS Package Builder")]
#[command(arg_required_else_help = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Install a package from the official repo or BUR
    Install { package: String },
    /// Search packages in the official repo and cached BUR
    Search { query: String },
    /// Build and install a local RBPKGBUILD directory
    Build { dir: String },
    /// Fetch a BUR recipe into the current directory
    Get { package: String },
    /// Inspect an installed package or local RBPKGBUILD
    Inspect { target: String },
    /// Convert an AUR PKGBUILD into an RBPKGBUILD tree
    ImportAur { target: String },
    /// Update all installed packages
    UpdateAll,
    /// Remove cub and pkg download caches
    CleanCache,
}

struct AppContext {
    install_path: PathBuf,
    target: String,
}

impl AppContext {
    fn new() -> Self {
        let install_path = if cfg!(target_os = "redox") {
            PathBuf::from(REDOX_INSTALL_PATH)
        } else {
            PathBuf::from(HOST_INSTALL_PATH)
        };

        let target = if cfg!(target_os = "redox") {
            env::var("TARGET").unwrap_or_else(|_| DEFAULT_TARGET.to_string())
        } else {
            DEFAULT_TARGET.to_string()
        };

        Self {
            install_path,
            target,
        }
    }

    fn open_library(&self) -> Result<Library, pkg::backend::Error> {
        let callback = new_pkg_callback();
        Library::new(&self.install_path, &self.target, callback)
    }

    fn open_local_library(
        &self,
        source_dir: &Path,
        pubkey_dir: &Path,
    ) -> Result<Library, pkg::backend::Error> {
        let callback = new_pkg_callback();
        Library::new_local(
            source_dir,
            pubkey_dir,
            &self.install_path,
            &self.target,
            callback,
        )
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = rewrite_shortcut_args(env::args_os())?;
    let cli = Cli::parse_from(args);
    let context = AppContext::new();

    match cli.command {
        Commands::Install { package } => install_package(&context, &package)?,
        Commands::Search { query } => search_packages(&context, &query)?,
        Commands::Build { dir } => build_local_dir(&context, Path::new(&dir))?,
        Commands::Get { package } => fetch_bur_recipe(&package)?,
        Commands::Inspect { target } => inspect_target(&context, &target)?,
        Commands::ImportAur { target } => import_aur_target(&target)?,
        Commands::UpdateAll => update_all(&context)?,
        Commands::CleanCache => clean_cache()?,
    }

    Ok(())
}

fn rewrite_shortcut_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Vec<OsString>, Box<dyn std::error::Error>> {
    let collected: Vec<OsString> = args.into_iter().collect();
    if collected.len() <= 1 {
        return Ok(collected);
    }

    let binary = collected[0].clone();
    let rest = &collected[1..];
    let Some(flag) = rest.first().and_then(|value| value.to_str()) else {
        return Ok(collected);
    };

    match flag {
        "-S" => rewrite_value_command(binary, rest, "install", "package"),
        "-Ss" => rewrite_value_command(binary, rest, "search", "query"),
        "-B" => rewrite_value_command(binary, rest, "build", "dir"),
        "-G" => rewrite_value_command(binary, rest, "get", "package"),
        "-Pi" => rewrite_value_command(binary, rest, "inspect", "target"),
        "--import-aur" => rewrite_value_command(binary, rest, "import-aur", "target"),
        "-Sua" => rewrite_flag_command(binary, rest, "update-all"),
        "-Sc" => rewrite_flag_command(binary, rest, "clean-cache"),
        _ => Ok(collected),
    }
}

fn rewrite_value_command(
    binary: OsString,
    rest: &[OsString],
    subcommand: &str,
    value_name: &str,
) -> Result<Vec<OsString>, Box<dyn std::error::Error>> {
    let Some(value) = rest.get(1) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("missing {value_name} for {}", rest[0].to_string_lossy()),
        )
        .into());
    };

    let mut rewritten = vec![binary, OsString::from(subcommand), value.clone()];
    rewritten.extend(rest.iter().skip(2).cloned());
    Ok(rewritten)
}

fn rewrite_flag_command(
    binary: OsString,
    rest: &[OsString],
    subcommand: &str,
) -> Result<Vec<OsString>, Box<dyn std::error::Error>> {
    let mut rewritten = vec![binary, OsString::from(subcommand)];
    rewritten.extend(rest.iter().skip(1).cloned());
    Ok(rewritten)
}

fn new_pkg_callback() -> Rc<RefCell<IndicatifCallback>> {
    let mut callback = IndicatifCallback::new();
    callback.set_interactive(true);
    Rc::new(RefCell::new(callback))
}

fn install_package(context: &AppContext, package: &str) -> Result<(), Box<dyn std::error::Error>> {
    let package_name = PackageName::new(package.to_string())?;
    let mut library = context.open_library()?;

    match library.install(vec![package_name.clone()], false) {
        Ok(()) => {
            let applied = apply_library_changes(&mut library)?;
            println!(
                "Installed {} from the official repository ({} change(s)).",
                package, applied
            );
            Ok(())
        }
        Err(pkg::backend::Error::PackageNotFound(_)) => {
            println!(
                "{} was not found in the official repository. Trying BUR...",
                package
            );
            let bur_dir = ensure_bur_package_dir(package)?;
            build_local_dir(context, &bur_dir)
        }
        Err(error) => Err(Box::new(error)),
    }
}

fn search_packages(context: &AppContext, query: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut library = context.open_library()?;
    let official_matches = library.search(query)?;
    let bur_matches = search_cached_bur(query)?;

    if official_matches.is_empty() {
        println!("Official repo: no matches for {query:?}");
    } else {
        println!("Official repo:");
        for (name, score) in official_matches {
            println!("  {} ({score:.2})", name);
        }
    }

    if bur_matches.is_empty() {
        println!("Cached BUR: no matches for {query:?}");
    } else {
        println!("Cached BUR:");
        for entry in bur_matches {
            if let Some(description) = &entry.description {
                println!("  {} - {}", entry.name, description);
            } else {
                println!("  {}", entry.name);
            }
        }
    }

    Ok(())
}

fn build_local_dir(context: &AppContext, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let rbpkg_path = dir.join("RBPKGBUILD");
    let rbpkg = RbPkgBuild::from_file(&rbpkg_path)?;
    rbpkg.validate()?;

    let work_dir = create_temp_dir("cub-build")?;
    let recipe_dir = work_dir.join(&rbpkg.package.name);
    CookbookAdapter::write_recipe_dir(&rbpkg, &recipe_dir)?;

    let sandbox = SandboxConfig::new(&work_dir);
    sandbox.setup()?;

    let mut command = Command::new("repo");
    command.arg("cook");
    command.arg(&recipe_dir);
    command.envs(sandbox.env_vars());

    let status = command.status()?;
    if !status.success() {
        return Err(Box::new(CubError::BuildFailed(format!(
            "repo cook {} failed with status {status}",
            recipe_dir.display()
        ))));
    }

    let stage_dir = find_stage_dir(&sandbox, &work_dir)?;
    let secret_key_path = resolve_secret_key_path()?;
    let public_key_dir = resolve_public_key_dir(&secret_key_path)?;

    let local_repo_dir = work_dir.join("local-repo");
    let target_repo_dir = local_repo_dir.join(&context.target);
    fs::create_dir_all(&target_repo_dir)?;

    let pkgar_path = target_repo_dir.join(format!("{}.pkgar", rbpkg.package.name));
    PackageCreator::create_pkgar(&stage_dir, &pkgar_path, &secret_key_path)?;

    let package_toml_path = target_repo_dir.join(format!("{}.toml", rbpkg.package.name));
    fs::write(
        package_toml_path,
        PackageCreator::generate_package_toml(&rbpkg),
    )?;

    let package_name = PackageName::new(rbpkg.package.name.clone())?;
    let mut library = context.open_local_library(&local_repo_dir, &public_key_dir)?;
    library.install(vec![package_name], false)?;
    let applied = apply_library_changes(&mut library)?;

    println!(
        "Built and installed {} successfully ({} change(s)).",
        rbpkg.package.name, applied
    );

    Ok(())
}

fn fetch_bur_recipe(package: &str) -> Result<(), Box<dyn std::error::Error>> {
    let source_dir = ensure_bur_package_dir(package)?;
    let destination = env::current_dir()?.join(package);
    if destination.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("destination already exists: {}", destination.display()),
        )
        .into());
    }

    copy_dir_recursive(&source_dir, &destination)?;
    println!(
        "Fetched BUR recipe {} to {}.",
        package,
        destination.display()
    );
    Ok(())
}

fn inspect_target(context: &AppContext, target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(target);
    if path.exists() {
        inspect_rbpkgbuild_path(path)?;
        return Ok(());
    }

    let mut library = context.open_library()?;
    let info = library.info(PackageName::new(target.to_string())?)?;
    println!("{info:#?}");
    Ok(())
}

fn import_aur_target(target: &str) -> Result<(), Box<dyn std::error::Error>> {
    let repo_url = aur_repo_url(target);
    let clone_dir = create_temp_dir("cub-aur")?;

    let status = Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(&repo_url)
        .arg(&clone_dir)
        .status()?;
    if !status.success() {
        return Err(Box::new(CubError::BuildFailed(format!(
            "failed to clone AUR source from {repo_url}"
        ))));
    }

    let pkgbuild_path = clone_dir.join("PKGBUILD");
    let pkgbuild = fs::read_to_string(&pkgbuild_path)?;
    let conversion = PkgbuildConverter::convert(&pkgbuild)?;
    let output_dir = env::current_dir()?.join(&conversion.rbpkg.package.name);

    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(output_dir.join("patches"))?;
    fs::create_dir_all(output_dir.join("import"))?;

    fs::write(output_dir.join("RBPKGBUILD"), conversion.rbpkg.to_toml()?)?;
    fs::write(
        output_dir.join(".RBSRCINFO"),
        RbSrcInfo::from_rbpkgbuild(&conversion.rbpkg).to_string(),
    )?;
    fs::write(output_dir.join("import").join("PKGBUILD"), pkgbuild)?;

    let report = render_conversion_report(&conversion.report);
    fs::write(output_dir.join("import").join("report.txt"), &report)?;

    println!("Imported AUR package into {}", output_dir.display());
    println!("{report}");
    Ok(())
}

fn update_all(context: &AppContext) -> Result<(), Box<dyn std::error::Error>> {
    let mut library = context.open_library()?;
    library.update(Vec::new())?;
    let applied = apply_library_changes(&mut library)?;
    println!("Updated installed packages ({} change(s)).", applied);
    Ok(())
}

fn clean_cache() -> Result<(), Box<dyn std::error::Error>> {
    remove_dir_if_exists(Path::new(PKG_DOWNLOAD_DIR))?;
    remove_dir_if_exists(Path::new(CUB_CACHE_DIR))?;
    println!("Removed package caches from {PKG_DOWNLOAD_DIR} and {CUB_CACHE_DIR}.");
    Ok(())
}

fn apply_library_changes(library: &mut Library) -> Result<usize, Box<dyn std::error::Error>> {
    match library.apply() {
        Ok(changes) => Ok(changes),
        Err(error) => {
            if let Err(abort_error) = library.abort() {
                eprintln!("Failed to abort package transaction: {abort_error}");
            }
            Err(Box::new(error))
        }
    }
}

fn inspect_rbpkgbuild_path(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let rbpkg_path = if path.is_dir() {
        path.join("RBPKGBUILD")
    } else {
        path.to_path_buf()
    };
    let rbpkg = RbPkgBuild::from_file(&rbpkg_path)?;

    println!("Package:");
    println!("  name = {}", rbpkg.package.name);
    println!("  version = {}", rbpkg.package.version);
    println!("  release = {}", rbpkg.package.release);
    println!("  description = {}", rbpkg.package.description);
    println!("  homepage = {}", rbpkg.package.homepage);
    println!("  license = {:?}", rbpkg.package.license);
    println!("  architectures = {:?}", rbpkg.package.architectures);
    println!("  maintainers = {:?}", rbpkg.package.maintainers);

    println!("Source:");
    for source in &rbpkg.source.sources {
        println!(
            "  {:?}: url={}, rev={}, branch={}, sha256={}",
            source.source_type, source.url, source.rev, source.branch, source.sha256
        );
    }

    println!("Dependencies:");
    println!("  build = {:?}", rbpkg.dependencies.build);
    println!("  runtime = {:?}", rbpkg.dependencies.runtime);
    println!("  check = {:?}", rbpkg.dependencies.check);
    println!("  optional = {:?}", rbpkg.dependencies.optional);
    println!("  provides = {:?}", rbpkg.dependencies.provides);
    println!("  conflicts = {:?}", rbpkg.dependencies.conflicts);

    println!("Build:");
    println!("  template = {:?}", rbpkg.build.template);
    println!("  release = {}", rbpkg.build.release);
    println!("  features = {:?}", rbpkg.build.features);
    println!("  args = {:?}", rbpkg.build.args);
    println!("  build_dir = {}", rbpkg.build.build_dir);
    println!("  prepare = {:?}", rbpkg.build.prepare);
    println!("  build_script = {:?}", rbpkg.build.build_script);
    println!("  check = {:?}", rbpkg.build.check);
    println!("  install_script = {:?}", rbpkg.build.install_script);

    println!("Install:");
    println!("  bins = {:?}", rbpkg.install.bins);
    println!("  libs = {:?}", rbpkg.install.libs);
    println!("  headers = {:?}", rbpkg.install.headers);
    println!("  docs = {:?}", rbpkg.install.docs);
    println!("  man = {:?}", rbpkg.install.man);

    println!("Patches:");
    println!("  files = {:?}", rbpkg.patches.files);

    println!("Compat:");
    println!("  imported_from = {}", rbpkg.compat.imported_from);
    println!("  conversion_status = {:?}", rbpkg.compat.conversion_status);
    println!("  target = {}", rbpkg.compat.target);

    println!("Policy:");
    println!("  allow_network = {}", rbpkg.policy.allow_network);
    println!("  allow_tests = {}", rbpkg.policy.allow_tests);
    println!("  review_required = {}", rbpkg.policy.review_required);

    println!("Generated .RBSRCINFO:");
    println!("{}", rbpkg.to_srcinfo().to_string());

    Ok(())
}

struct BurMatch {
    name: String,
    description: Option<String>,
}

fn search_cached_bur(query: &str) -> Result<Vec<BurMatch>, Box<dyn std::error::Error>> {
    let repo_dir = bur_repo_dir();
    if !repo_dir.exists() {
        return Ok(Vec::new());
    }

    let mut matches = Vec::new();
    let lowered_query = query.to_ascii_lowercase();
    for entry in fs::read_dir(repo_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name == ".git" {
            continue;
        }

        let rbpkg_path = path.join("RBPKGBUILD");
        let mut description = None;
        let mut matched = name.to_ascii_lowercase().contains(&lowered_query);
        if rbpkg_path.is_file() {
            if let Ok(pkg) = RbPkgBuild::from_file(&rbpkg_path) {
                if pkg
                    .package
                    .name
                    .to_ascii_lowercase()
                    .contains(&lowered_query)
                    || pkg
                        .package
                        .description
                        .to_ascii_lowercase()
                        .contains(&lowered_query)
                {
                    matched = true;
                }
                if !pkg.package.description.trim().is_empty() {
                    description = Some(pkg.package.description);
                }
            }
        }

        if matched {
            matches.push(BurMatch {
                name: name.to_string(),
                description,
            });
        }
    }

    matches.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(matches)
}

fn ensure_bur_package_dir(package: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo_dir = sync_bur_repo()?;
    let package_dir = repo_dir.join(package);
    if package_dir.is_dir() {
        Ok(package_dir)
    } else {
        Err(Box::new(CubError::PackageNotFound(format!(
            "{package} not found in BUR cache {}",
            repo_dir.display()
        ))))
    }
}

fn sync_bur_repo() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo_dir = bur_repo_dir();
    let parent = repo_dir
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid BUR cache path"))?;
    fs::create_dir_all(parent)?;

    if repo_dir.join(".git").is_dir() {
        let status = Command::new("git")
            .arg("pull")
            .arg("--ff-only")
            .current_dir(&repo_dir)
            .status()?;
        if !status.success() {
            return Err(Box::new(CubError::BuildFailed(format!(
                "failed to update BUR cache at {}",
                repo_dir.display()
            ))));
        }
    } else {
        let status = Command::new("git")
            .arg("clone")
            .arg(default_bur_repo_url())
            .arg(&repo_dir)
            .status()?;
        if !status.success() {
            return Err(Box::new(CubError::BuildFailed(format!(
                "failed to clone BUR repository into {}",
                repo_dir.display()
            ))));
        }
    }

    Ok(repo_dir)
}

fn default_bur_repo_url() -> String {
    env::var("CUB_BUR_REPO_URL").unwrap_or_else(|_| DEFAULT_BUR_REPO_URL.to_string())
}

fn bur_repo_dir() -> PathBuf {
    PathBuf::from(CUB_CACHE_DIR).join("bur")
}

fn aur_repo_url(target: &str) -> String {
    if target.contains("://") || target.ends_with(".git") {
        target.to_string()
    } else {
        format!("{DEFAULT_AUR_BASE_URL}/{}.git", target)
    }
}

fn resolve_secret_key_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = env::var_os("CUB_PKGAR_SECRET_KEY") {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let home = env::var_os("HOME").map(PathBuf::from);
    let candidates = [
        home.as_ref()
            .map(|path| path.join(".pkg").join(DEFAULT_SECRET_KEY_FILE)),
        Some(PathBuf::from("/etc/pkg").join(DEFAULT_SECRET_KEY_FILE)),
        Some(PathBuf::from("/pkg").join(DEFAULT_SECRET_KEY_FILE)),
        Some(env::current_dir()?.join(DEFAULT_SECRET_KEY_FILE)),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(Box::new(CubError::BuildFailed(
        "could not locate a pkgar secret key; set CUB_PKGAR_SECRET_KEY".to_string(),
    )))
}

fn resolve_public_key_dir(secret_key_path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(path) = env::var_os("CUB_PKGAR_PUBKEY_DIR") {
        let candidate = PathBuf::from(path);
        if candidate.join(PUBLIC_KEY_FILE).is_file() {
            return Ok(candidate);
        }
    }

    let Some(parent) = secret_key_path.parent() else {
        return Err(Box::new(CubError::BuildFailed(format!(
            "could not determine public key directory for {}",
            secret_key_path.display()
        ))));
    };

    if parent.join(PUBLIC_KEY_FILE).is_file() {
        Ok(parent.to_path_buf())
    } else {
        Err(Box::new(CubError::BuildFailed(format!(
            "missing {} in {}",
            PUBLIC_KEY_FILE,
            parent.display()
        ))))
    }
}

fn find_stage_dir(
    sandbox: &SandboxConfig,
    search_root: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let direct_candidates = [
        sandbox.stage_dir.clone(),
        sandbox.destdir.clone(),
        search_root.join("stage"),
        search_root.join("destdir"),
    ];

    for candidate in direct_candidates {
        if directory_has_entries(&candidate)? {
            return Ok(candidate);
        }
    }

    let mut stack = vec![search_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };

            if matches!(name, "stage" | "destdir") && directory_has_entries(&path)? {
                return Ok(path);
            }

            stack.push(path);
        }
    }

    Err(Box::new(CubError::BuildFailed(format!(
        "unable to locate a populated stage directory under {}",
        search_root.display()
    ))))
}

fn directory_has_entries(path: &Path) -> Result<bool, io::Error> {
    if !path.is_dir() {
        return Ok(false);
    }

    Ok(fs::read_dir(path)?.next().transpose()?.is_some())
}

fn render_conversion_report(report: &ConversionReport) -> String {
    let mut output = String::new();
    output.push_str(&format!("Conversion: {:?}\n", report.status));

    if !report.warnings.is_empty() {
        output.push_str("\nWarnings:\n");
        for warning in &report.warnings {
            output.push_str(&format!("- {warning}\n"));
        }
    }

    if !report.actions_required.is_empty() {
        output.push_str("\nActions required:\n");
        for action in &report.actions_required {
            output.push_str(&format!("- {action}\n"));
        }
    }

    output
}

fn create_temp_dir(prefix: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let base = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    for attempt in 0..128 {
        let candidate = base.join(format!("{prefix}-{}-{nanos}-{attempt}", std::process::id()));
        if !candidate.exists() {
            fs::create_dir_all(&candidate)?;
            return Ok(candidate);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("failed to allocate temporary directory for {prefix}"),
    )
    .into())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let destination_path = dst.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &destination_path)?;
        } else {
            fs::copy(&entry_path, &destination_path)?;
        }
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> Result<(), io::Error> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}
