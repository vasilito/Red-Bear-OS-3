use crate::Error;
use crate::Result;
use crate::bail_other_err;
use crate::config::translate_mirror;
use crate::cook::cook_build;
use crate::cook::fetch_repo;
use crate::cook::fetch_repo::PlainPtyCallback;
use crate::cook::fs::*;
use crate::cook::package::get_package_name;
use crate::cook::package::package_source_paths;
use crate::cook::pty::PtyOut;
use crate::cook::script::*;
use crate::is_redox;
use crate::log_to_pty;
use crate::recipe::BuildKind;
use crate::recipe::CookRecipe;
use crate::recipe::SourceRecipe;
use crate::wrap_io_err;
use crate::wrap_other_err;
use pkg::SourceIdentifier;
use pkg::net_backend::DownloadBackendWriter;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

pub struct FetchResult {
    pub source_dir: PathBuf,
    pub source_ident: String,
    pub cached: bool,
}

pub(crate) fn cleanup_workspace_pollution(recipe_dir: &Path, logger: &PtyOut) {
    let recipes_root = recipe_dir.join("../..");
    for file in &["Cargo.toml", "Cargo.lock"] {
        let path = recipes_root.join(file);
        if path.is_file() && !path.is_symlink() {
            if let Err(e) = fs::remove_file(&path) {
                log_to_pty!(logger, "[WARN] failed to remove workspace pollution {}: {e}", path.display());
            } else {
                log_to_pty!(logger, "[CLEAN] removed workspace pollution {}", path.display());
            }
        }
    }
}

fn redbear_protected_recipe(name: &str) -> bool {
    matches!(
        name,
        // Core patched recipes (upstream + Red Bear patches)
        "relibc"
            | "bootloader"
            | "kernel"
            | "base"
            | "base-initfs"
            | "installer"
            | "redoxfs"
            | "grub"
        // Red Bear custom core recipes
            | "ext4d"
            | "fatd"
        // Red Bear driver infrastructure
            | "redox-driver-sys"
            | "linux-kpi"
            | "firmware-loader"
            | "redbear-btusb"
            | "redbear-iwlwifi"
        // Red Bear GPU stack
            | "redox-drm"
            | "amdgpu"
        // Red Bear system tools
            | "cub"
            | "evdevd"
            | "udev-shim"
            | "iommu"
            | "redbear-firmware"
            | "redbear-hwutils"
            | "redbear-info"
            | "rbos-info"
            | "redbear-meta"
            | "redbear-netctl"
            | "redbear-netctl-console"
            | "redbear-netstat"
            | "redbear-btctl"
            | "redbear-wifictl"
            | "redbear-traceroute"
            | "redbear-mtr"
            | "redbear-nmap"
            | "redbear-sessiond"
            | "redbear-authd"
            | "redbear-session-launch"
            | "redbear-greeter"
            | "redbear-dbus-services"
            | "redbear-notifications"
            | "redbear-upower"
            | "redbear-udisks"
            | "redbear-polkit"
            | "redbear-quirks"
        // Red Bear branding
            | "redbear-release"
        // Red Bear library stubs and custom libs
            | "libepoxy-stub"
            | "libdisplay-info-stub"
            | "lcms2-stub"
            | "libxcvt-stub"
            | "libudev-stub"
            | "zbus"
            | "libqrencode"
        // Red Bear Wayland
            | "qt6-wayland-smoke"
            | "smallvil"
            | "seatd-redox"
        // Red Bear KDE (47 recipes)
            | "kf6-extra-cmake-modules"
            | "kf6-kcoreaddons"
            | "kf6-kwidgetsaddons"
            | "kf6-kconfig"
            | "kf6-ki18n"
            | "kf6-kcodecs"
            | "kf6-kguiaddons"
            | "kf6-kcolorscheme"
            | "kf6-kauth"
            | "kf6-kitemmodels"
            | "kf6-kitemviews"
            | "kf6-karchive"
            | "kf6-kwindowsystem"
            | "kf6-knotifications"
            | "kf6-kjobwidgets"
            | "kf6-kconfigwidgets"
            | "kf6-kcrash"
            | "kf6-kdbusaddons"
            | "kf6-kglobalaccel"
            | "kf6-kservice"
            | "kf6-kpackage"
            | "kf6-kiconthemes"
            | "kf6-kxmlgui"
            | "kf6-ktextwidgets"
            | "kf6-solid"
            | "kf6-sonnet"
            | "kf6-kio"
            | "kf6-kbookmarks"
            | "kf6-kcompletion"
            | "kf6-kdeclarative"
            | "kf6-kcmutils"
            | "kf6-kidletime"
            | "kf6-kwayland"
            | "kf6-knewstuff"
            | "kf6-kwallet"
            | "kf6-prison"
            | "kf6-kirigami"
            | "kdecoration"
            | "kwin"
            | "plasma-desktop"
            | "plasma-workspace"
            | "plasma-framework"
            | "plasma-wayland-protocols"
            | "kirigami"
        // Orbutils (has local patch)
            | "orbutils"
    )
}

fn redbear_allow_protected_fetch() -> bool {
    matches!(
        env::var("REDBEAR_ALLOW_PROTECTED_FETCH").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn redbear_release() -> Option<String> {
    env::var("REDBEAR_RELEASE")
        .ok()
        .map(|value| value.trim().trim_start_matches('=').to_string())
        .filter(|value| !value.is_empty())
}

fn redbear_project_root(recipe_dir: &Path) -> Option<PathBuf> {
    let absolute_recipe_dir = if recipe_dir.is_absolute() {
        recipe_dir.to_path_buf()
    } else {
        env::current_dir().ok()?.join(recipe_dir)
    };
    for ancestor in absolute_recipe_dir.ancestors() {
        if ancestor.file_name().is_some_and(|name| name == "recipes") {
            return ancestor.parent().map(Path::to_path_buf);
        }
    }
    None
}

fn redbear_recipe_restore_path(recipe_dir: &Path) -> Option<String> {
    let mut saw_recipes = false;
    let mut parts = Vec::new();
    for component in recipe_dir.components() {
        let value = component.as_os_str().to_string_lossy();
        if saw_recipes {
            parts.push(value.to_string());
        } else if value == "recipes" {
            saw_recipes = true;
        }
    }
    if saw_recipes && !parts.is_empty() {
        Some(parts.join("/"))
    } else {
        None
    }
}

fn redbear_try_restore_source(recipe_dir: &Path, logger: &PtyOut, force: bool) -> Result<()> {
    let Some(release) = redbear_release() else {
        return Ok(());
    };
    let Some(project_root) = redbear_project_root(recipe_dir) else {
        return Ok(());
    };
    let Some(recipe_path) = redbear_recipe_restore_path(recipe_dir) else {
        return Ok(());
    };
    let restore_script = project_root.join("local/scripts/restore-sources.sh");
    if !restore_script.is_file() {
        return Ok(());
    }
    let mut command = Command::new("python3");
    command.current_dir(&project_root);
    command.arg(&restore_script);
    command.arg(format!("--release={release}"));
    if force {
        command.arg("--force");
    }
    command.arg(recipe_path);
    run_command(command, logger)
}

fn redbear_source_dir_is_effectively_empty(source_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(source_dir) else {
        return true;
    };
    let visible_entries = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name() != ".gitkeep")
        .count();
    visible_entries == 0
}

fn redbear_ensure_offline_source(recipe_dir: &Path, source_dir: &PathBuf, logger: &PtyOut) -> Result<()> {
    if !source_dir.exists() || redbear_source_dir_is_effectively_empty(source_dir) {
        redbear_try_restore_source(recipe_dir, logger, true)?;
    }
    offline_check_exists(source_dir)
}

fn redbear_ensure_offline_git_source(
    recipe_dir: &Path,
    source_dir: &PathBuf,
    logger: &PtyOut,
) -> Result<()> {
    let git_head = source_dir.join(".git/HEAD");
    if !source_dir.exists() || !git_head.is_file() {
        redbear_try_restore_source(recipe_dir, logger, true)?;
    }
    offline_check_exists(source_dir)
}

/// Check if a recipe directory is a local Red Bear overlay (symlink into local/).
fn is_local_overlay(recipe_dir: &Path) -> bool {
    if let Ok(resolved) = recipe_dir.canonicalize() {
        let resolved_str = resolved.to_string_lossy();
        return resolved_str.contains("/local/recipes/");
    }
    false
}

impl FetchResult {
    pub fn new(source_dir: PathBuf, ident: String, cached: bool) -> Self {
        Self {
            source_dir,
            source_ident: ident,
            cached,
        }
    }

    pub fn cached(source_dir: PathBuf, ident: String) -> Self {
        Self {
            source_dir,
            source_ident: ident,
            cached: true,
        }
    }
}

pub(crate) fn get_blake3(path: &PathBuf) -> Result<String> {
    let mut f = fs::File::open(&path).map_err(wrap_io_err!(path, "Opening file for blake3"))?;
    let hash = blake3::Hasher::new()
        .update_reader(&mut f)
        .map_err(wrap_io_err!(path, "Reading file for blake3"))?
        .finalize();
    Ok(hash.to_hex().to_string())
}

pub fn fetch_offline(recipe: &CookRecipe, logger: &PtyOut) -> Result<FetchResult> {
    let recipe_dir = &recipe.dir;
    let source_dir = recipe_dir.join("source");

    // Clean up workspace pollution that may have been left by previous
    // builds (e.g. Cargo workspace files leaked into the recipes/ root).
    cleanup_workspace_pollution(recipe_dir, logger);

    match recipe.recipe.build.kind {
        BuildKind::None => {
            // the build function doesn't need source dir exists
            let ident = fetch_apply_source_info(recipe, "".to_string())?;
            return Ok(FetchResult::cached(source_dir, ident));
        }
        BuildKind::Remote => {
            return fetch_remote(recipe_dir, recipe, true, source_dir, logger);
        }
        _ => {}
    }

    let result = match &recipe.recipe.source {
        Some(SourceRecipe::Path { path: _ }) => {
            redbear_ensure_offline_source(recipe_dir, &source_dir, logger)?;
            let ident = fetch_apply_source_info(recipe, "".to_string())?;
            FetchResult::cached(source_dir, ident)
        }
        None => {
            let ident = fetch_apply_source_info(recipe, "".to_string())?;
            FetchResult::cached(source_dir, ident)
        }
        Some(SourceRecipe::SameAs { same_as }) => {
            let recipe = fetch_resolve_canon(recipe_dir, &same_as, recipe.name.is_host())?;
            // recursively fetch
            let r = fetch_offline(&recipe, logger)?;
            fetch_make_symlink(&source_dir, &same_as)?;
            r
        }
        Some(SourceRecipe::Git {
            git: _,
            upstream: _,
            branch: _,
            rev,
            patches,
            script,
            shallow_clone: _,
        }) => {
            redbear_ensure_offline_git_source(recipe_dir, &source_dir, logger)?;
            let git_head = source_dir.join(".git/HEAD");
            if !git_head.is_file() {
                let source_ident = rev.clone().unwrap_or_else(|| {
                    format!("release-archive:{}", recipe.name.name())
                });
                FetchResult::cached(source_dir, source_ident)
            } else {
            let (head_rev, _) = get_git_head_rev(&source_dir)?;
            if let Some(expected_rev) = rev {
                let head_short = &head_rev[..head_rev.len().min(7)];
                let expected_short = &expected_rev[..expected_rev.len().min(7)];
                if !head_rev.starts_with(expected_rev.as_str())
                    && head_short != expected_short
                {
                    bail_other_err!(
                        "source at {} has revision {} but recipe expects {}. \
                         Source archives may be corrupted. Restore from release archives.",
                        source_dir.display(), head_short, expected_rev
                    );
                }
            }
            // Validate all patch symlinks resolve before touching source.
            fetch_validate_patch_symlinks(recipe_dir, patches)?;

            if (!patches.is_empty() || script.is_some())
                && fetch_patches_state_stale(recipe_dir, patches, script, &source_dir)
            {
                log_to_pty!(logger, "[INFO] patches state stale or missing — re-applying");
                // Reset source to clean state, including submodules.
                let mut clean_cmd = Command::new("git");
                clean_cmd.arg("-C").arg(&source_dir);
                clean_cmd.arg("clean").arg("-ffdx");
                let _ = run_command(clean_cmd, logger);
                let mut reset_cmd = Command::new("git");
                reset_cmd.arg("-C").arg(&source_dir);
                reset_cmd.arg("reset").arg("--hard");
                run_command(reset_cmd, logger)?;
                // Recursively reset submodules if any exist.
                if source_dir.join(".gitmodules").exists() {
                    let mut sub_cmd = Command::new("git");
                    sub_cmd.arg("-C").arg(&source_dir);
                    sub_cmd.arg("submodule").arg("foreach");
                    sub_cmd.arg("--recursive");
                    sub_cmd.arg("git reset --hard && git clean -ffdx");
                    run_command(sub_cmd, logger)?;
                }
                fetch_apply_patches(recipe_dir, patches, script, &source_dir, logger)?;
            }
            FetchResult::cached(source_dir, head_rev)
            }
        }
        Some(SourceRecipe::Tar {
            tar: _,
            blake3,
            patches,
            script,
        }) => {
            let ident = blake3.clone().unwrap_or("no_tar_blake3_hash_info".into());
            let cached = source_dir.is_dir();
            if !cached {
                let source_tar = recipe_dir.join("source.tar");
                let source_tar_blake3 = get_blake3(&source_tar)?;
                if source_tar.exists() {
                    if let Some(blake3) = blake3 {
                        if source_tar_blake3 != *blake3 {
                            bail_other_err!(
                                "The downloaded tar blake3 {source_tar_blake3:?} is not equal to blake3 in recipe.toml"
                            );
                        }
                        create_dir(&source_dir)?;
                        fetch_extract_tar(source_tar, &source_dir, logger)?;
                        fetch_apply_patches(recipe_dir, patches, script, &source_dir, logger)?;
                    } else {
                        // need to trust this tar file
                        bail_other_err!(
                            "Please add blake3 = {source_tar_blake3:?} to {recipe:?}",
                            recipe = recipe_dir.join("recipe.toml").display(),
                        );
                    }
                }
            }
            redbear_ensure_offline_source(recipe_dir, &source_dir, logger)?;
            FetchResult::new(source_dir, ident, cached)
        }
    };

    fetch_apply_source_info(recipe, result.source_ident.clone())?;

    Ok(result)
}

pub fn fetch(recipe: &CookRecipe, check_source: bool, logger: &PtyOut) -> Result<FetchResult> {
    if redbear_protected_recipe(recipe.name.name()) && !redbear_allow_protected_fetch() {
        log_to_pty!(
            logger,
            "[INFO]: protected recipe {} uses local source (fetch disabled; use --allow-protected flag or set REDBEAR_ALLOW_PROTECTED_FETCH=1 to override)",
            recipe.name.name()
        );
        return fetch_offline(recipe, logger);
    }

    let recipe_dir = &recipe.dir;
    let source_dir = recipe_dir.join("source");
    match recipe.recipe.build.kind {
        BuildKind::None => {
            // the build function doesn't need source dir exists
            let ident = fetch_apply_source_info(recipe, "".to_string())?;
            return Ok(FetchResult::cached(source_dir, ident));
        }
        BuildKind::Remote => {
            return fetch_remote(recipe_dir, recipe, false, source_dir, logger);
        }
        _ => {}
    }

    let result = match &recipe.recipe.source {
        Some(SourceRecipe::SameAs { same_as }) => {
            let recipe = fetch_resolve_canon(recipe_dir, &same_as, recipe.name.is_host())?;
            // recursively fetch
            let r = fetch(&recipe, check_source, logger)?;
            fetch_make_symlink(&source_dir, &same_as)?;
            r
        }
        Some(SourceRecipe::Path { path }) => {
            let path = recipe_dir.join(path);
            let cached = source_dir.is_dir() && modified_dir(&path)? <= modified_dir(&source_dir)?;
            if !cached {
                log_to_pty!(
                    logger,
                    "[DEBUG]: {:?} is newer than {:?}",
                    path.display(),
                    source_dir.display()
                );
                copy_dir_all(&path, &source_dir).map_err(wrap_io_err!(
                    &path,
                    source_dir,
                    "Copying source"
                ))?;
            }
            FetchResult::new(source_dir, "local_source".to_string(), cached)
        }
        Some(SourceRecipe::Git {
            git,
            upstream,
            branch,
            rev,
            patches,
            script,
            shallow_clone,
        }) => {
            //TODO: use libgit?
            let shallow_clone = *shallow_clone == Some(true);
            let cached = if !source_dir.is_dir() {
                // Create source.tmp
                let source_dir_tmp = recipe_dir.join("source.tmp");
                create_dir_clean(&source_dir_tmp)?;

                // Clone the repository to source.tmp
                let mut command = Command::new("git");
                command
                    .arg("clone")
                    .arg("--recursive")
                    .arg(translate_mirror(&git));
                if let Some(branch) = branch {
                    command.arg("--branch").arg(branch);
                }
                if shallow_clone {
                    command
                        .arg("--filter=tree:0")
                        .arg("--also-filter-submodules");
                }
                command.arg(&source_dir_tmp);
                if let Err(e) = run_command(command, logger) {
                    if !is_redox() {
                        return Err(e);
                    }
                    // TODO: RedoxFS has a race condition problem with `--recursive` and running in multi CPU.
                    //       It is appear that running the submodule update separately fixes it. Remove this when
                    //       `git clone https://gitlab.redox-os.org/redox-os/relibc --recursive` proven to work in Redox OS.
                    let mut cmds = vec!["update", "--init"];
                    if shallow_clone {
                        cmds.push("--filter=tree:0");
                    }
                    manual_git_recursive_submodule(logger, &source_dir_tmp, cmds)?;
                }

                // Move source.tmp to source atomically
                rename(&source_dir_tmp, &source_dir)?;

                false
            } else if !check_source {
                true
            } else {
                if !source_dir.join(".git").is_dir() {
                    bail_other_err!(
                        "{:?} is not a git repository, but recipe indicated git source",
                        source_dir.display()
                    );
                }

                // Reset origin
                let mut command = Command::new("git");
                command.arg("-C").arg(&source_dir);
                command.arg("remote").arg("set-url").arg("origin").arg(git);
                run_command(command, logger)?;

                // Fetch origin
                let mut command = Command::new("git");
                command.arg("-C").arg(&source_dir);
                command.arg("fetch").arg("origin");
                run_command(command, logger)?;

                let (head_rev, detached_rev) = get_git_head_rev(&source_dir)?;
                match (rev, detached_rev) {
                    (Some(rev), true) => {
                        if let Ok(exp_rev) = get_git_tag_rev(&source_dir, &rev) {
                            exp_rev == head_rev
                        } else {
                            let mut command = Command::new("git");
                            command.arg("-C").arg(&source_dir);
                            command.arg("gc");
                            run_command(command, logger)?;
                            if let Ok(exp_rev) = get_git_tag_rev(&source_dir, &rev) {
                                exp_rev == head_rev
                            } else {
                                false
                            }
                        }
                    }
                    (None, false) => {
                        let (_, remote_branch, remote_name, remote_url) =
                            get_git_remote_tracking(&source_dir)?;
                        // TODO: how to get default branch and compare it here?
                        if let Some(branch) = branch
                            && branch != &remote_branch
                        {
                            false
                        } else if remote_name != "origin" || &remote_url != chop_dot_git(git) {
                            false
                        } else {
                            match get_git_fetch_rev(&source_dir, &remote_url, &remote_branch) {
                                Ok(fetch_rev) => fetch_rev == head_rev,
                                Err(e) => {
                                    log_to_pty!(logger, "{}", e);
                                    false
                                }
                            }
                        }
                    }
                    _ => false,
                }
            };

            if !cached {
                if let Some(_upstream) = upstream {
                    //TODO: set upstream URL (is this needed?)
                    // git remote set-url upstream "$GIT_UPSTREAM" &> /dev/null ||
                    // git remote add upstream "$GIT_UPSTREAM"
                    // git fetch upstream
                }

                if !patches.is_empty() || script.is_some() {
                    if is_local_overlay(recipe_dir) && !redbear_allow_protected_fetch() {
                        log_to_pty!(
                            logger,
                            "[WARN] skipping git reset --hard for local overlay recipe at {} \
                             (set REDBEAR_ALLOW_PROTECTED_FETCH=1 to override)",
                            recipe_dir.display()
                        );
                    } else {
                        let mut clean_cmd = Command::new("git");
                        clean_cmd.arg("-C").arg(&source_dir);
                        clean_cmd.arg("clean").arg("-fd");
                        let _ = run_command(clean_cmd, logger);

                        // Hard reset
                        let mut command = Command::new("git");
                        command.arg("-C").arg(&source_dir);
                        command.arg("reset").arg("--hard");
                        run_command(command, logger)?;
                    }
                }

                if let Some(rev) = rev {
                    // Check out specified revision
                    let mut command = Command::new("git");
                    command.arg("-C").arg(&source_dir);
                    command.arg("checkout").arg(rev);
                    run_command(command, logger)?;
                } else if !is_redox() {
                    //TODO: complicated stuff to check and reset branch to origin
                    //TODO: redox can't undestand this (got exit status 1)
                    let mut command = Command::new("bash");
                    command.arg("-c").arg(GIT_RESET_BRANCH);
                    if let Some(branch) = branch {
                        command.env("BRANCH", branch);
                    }
                    command.current_dir(&source_dir);
                    run_command(command, logger)?;
                }

                // Sync submodules URL
                let mut command = Command::new("git");
                command.arg("-C").arg(&source_dir);
                command.arg("submodule").arg("sync").arg("--recursive");

                if let Err(e) = run_command(command, logger) {
                    if !is_redox() {
                        return Err(e);
                    }
                    manual_git_recursive_submodule(logger, &source_dir, vec!["sync"])?;
                }

                // Update submodules
                let mut command = Command::new("git");
                command.arg("-C").arg(&source_dir);
                command
                    .arg("submodule")
                    .arg("update")
                    .arg("--init")
                    .arg("--recursive");
                if shallow_clone {
                    command.arg("--filter=tree:0");
                }
                if let Err(e) = run_command(command, logger) {
                    if !is_redox() {
                        return Err(e);
                    }
                    let mut cmds = vec!["update", "--init"];
                    if shallow_clone {
                        cmds.push("--filter=tree:0");
                    }
                    manual_git_recursive_submodule(logger, &source_dir, cmds)?;
                }

                fetch_validate_patch_symlinks(recipe_dir, patches)?;
                fetch_apply_patches(recipe_dir, patches, script, &source_dir, logger)?;
            }

            let (head_rev, _) = get_git_head_rev(&source_dir)?;
            FetchResult::new(source_dir, head_rev, cached)
        }
        Some(SourceRecipe::Tar {
            tar,
            blake3,
            patches,
            script,
        }) => {
            let source_tar = recipe_dir.join("source.tar");
            let ident = blake3.clone().unwrap_or("no_tar_blake3_hash_info".into());
            let mut tar_updated = false;
            loop {
                if !source_tar.is_file() {
                    tar_updated = true;
                    download_wget(&tar, &source_tar, logger)?;
                }
                if !check_source {
                    break;
                }
                let source_tar_blake3 = get_blake3(&source_tar)?;
                if let Some(blake3) = blake3 {
                    if source_tar_blake3 == *blake3 {
                        break;
                    }
                    if tar_updated {
                        bail_other_err!(
                            "The downloaded tar blake3 {source_tar_blake3:?} is not equal to blake3 in recipe.toml"
                        )
                    } else {
                        log_to_pty!(
                            logger,
                            "DEBUG: source tar blake3 is different and need redownload"
                        );
                        remove_all(&source_tar)?;
                    }
                } else {
                    //TODO: set blake3 hash on the recipe with something like "cook fix"
                    log_to_pty!(
                        logger,
                        "WARNING: set blake3 for '{}' to '{}'",
                        source_tar.display(),
                        source_tar_blake3
                    );
                    break;
                }
            }
            let mut cached = true;
            if source_dir.is_dir() {
                if tar_updated || fetch_is_patches_newer(recipe_dir, patches, &source_dir)? {
                    if is_local_overlay(recipe_dir) && !redbear_allow_protected_fetch() {
                        log_to_pty!(
                            logger,
                            "[WARN] refusing to wipe source for local overlay recipe at {} \
                             (set REDBEAR_ALLOW_PROTECTED_FETCH=1 to override)",
                            recipe_dir.display()
                        );
                    } else {
                        log_to_pty!(
                            logger,
                            "DEBUG: source tar or patches is newer than the source directory"
                        );
                        remove_all(&source_dir)?
                    }
                }
            }
            if !source_dir.is_dir() {
                // Create source.tmp
                let source_dir_tmp = recipe_dir.join("source.tmp");
                create_dir_clean(&source_dir_tmp)?;
                fetch_extract_tar(source_tar, &source_dir_tmp, logger)?;
                fetch_apply_patches(recipe_dir, patches, script, &source_dir_tmp, logger)?;

                // Move source.tmp to source atomically
                rename(&source_dir_tmp, &source_dir)?;
                cached = false;
            }
            FetchResult::new(source_dir, ident, cached)
        }
        // Local Sources
        None => {
            if !source_dir.is_dir() {
                log_to_pty!(
                    logger,
                    "WARNING: Recipe without source section expected source dir at '{}'",
                    source_dir.display(),
                );
                create_dir(&source_dir)?;
            }
            FetchResult::cached(source_dir, "local_source".into())
        }
    };

    if let BuildKind::Cargo {
        cargopath,
        cargoflags: _,
        cargopackages: _,
        cargoexamples: _,
    } = &recipe.recipe.build.kind
    {
        if fetch_will_build(recipe) {
            fetch_cargo(&result.source_dir, cargopath.as_ref(), logger)?;
        }
    }

    fetch_apply_source_info(recipe, result.source_ident.to_string())?;

    Ok(result)
}

fn manual_git_recursive_submodule(
    logger: &PtyOut,
    source_dir: &PathBuf,
    cmd: Vec<&str>,
) -> Result<()> {
    log_to_pty!(
        logger,
        "Git submodule {} failed, might be caused by race condition in RedoxFS, retrying without --recursive.",
        cmd[0]
    );

    let mut repo_registry: BTreeMap<PathBuf, bool> = BTreeMap::new();

    loop {
        let mut dirty_git = false;

        let output = Command::new("find")
            .args(&[".", "-name", ".git"])
            .current_dir(&source_dir)
            .output()
            .map_err(wrap_io_err!("Failed to execute find"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            let git_path = PathBuf::from(line);
            if let Some(repo_root) = git_path.parent() {
                let repo_root_buf = repo_root.to_path_buf();

                if !repo_registry.contains_key(&repo_root_buf) {
                    repo_registry.insert(repo_root_buf.clone(), false);
                    dirty_git = true;
                }
            }
        }

        if !dirty_git {
            // completed
            return Ok(());
        }

        let pending_repos: Vec<PathBuf> = repo_registry
            .iter()
            .filter(|&(_, &synced)| !synced)
            .map(|(path, _)| path.clone())
            .collect();

        if pending_repos.is_empty() {
            bail_other_err!("No pending repos but dirty");
        }

        for repo in pending_repos {
            println!("==> Processing: {:?}", repo);

            let mut command = Command::new("git");
            command.arg("-C").arg(&repo).current_dir(&source_dir);
            command.arg("submodule");

            for cmd in &cmd {
                command.arg(cmd);
            }
            run_command(command, logger)?;

            repo_registry.insert(repo, true);
        }
    }
}

/// This does the same check as in cook_build
fn fetch_will_build(recipe: &CookRecipe) -> bool {
    let check_source = !recipe.is_deps;
    if !check_source {
        // there could be more check here, but it's heavy so just assume it will build
        return true;
    }

    let stage_dirs =
        cook_build::get_stage_dirs(&recipe.recipe.optional_packages, &recipe.target_dir());
    let stage_pkgars: Vec<PathBuf> = stage_dirs
        .iter()
        .map(|p| p.with_added_extension("pkgar"))
        .collect();
    let stage_present = stage_pkgars.iter().all(|file| file.is_file());
    !stage_present
}

pub(crate) fn fetch_make_symlink(source_dir: &PathBuf, same_as: &String) -> Result<()> {
    let target_dir = Path::new(same_as).join("source");
    if !source_dir.is_symlink() {
        if source_dir.is_dir() {
            bail_other_err!(
                "'{dir:?}' is a directory, but recipe indicated a symlink. \n\
                        try removing '{dir:?}' if you haven't made any changes that would be lost",
                dir = source_dir.display(),
            )
        }
        std::os::unix::fs::symlink(&target_dir, source_dir).map_err(|err| {
            format!(
                "failed to symlink '{}' to '{}': {}\n{:?}",
                target_dir.display(),
                source_dir.display(),
                err,
                err
            )
        })?;
    }
    Ok(())
}

pub(crate) fn fetch_resolve_canon(
    recipe_dir: &Path,
    same_as: &String,
    is_host: bool,
) -> Result<CookRecipe> {
    let canon_dir = Path::new(recipe_dir).join(same_as);
    if canon_dir
        .to_str()
        .unwrap()
        .chars()
        .filter(|c| *c == '/')
        .count()
        > 50
    {
        bail_other_err!("Infinite loop detected");
    }
    if !canon_dir.exists() {
        bail_other_err!("{dir:?} is not exists", dir = canon_dir.display());
    }
    CookRecipe::from_path(canon_dir.as_path(), true, is_host).map_err(Error::from)
}

pub(crate) fn fetch_extract_tar(
    source_tar: PathBuf,
    source_dir_tmp: &PathBuf,
    logger: &PtyOut,
) -> Result<()> {
    let mut command = Command::new("tar");
    let verbose = crate::config::get_config().cook.verbose;
    if is_redox() {
        command.arg(if verbose { "xvf" } else { "xf" });
    } else {
        command.arg("--extract");
        command.arg("--no-same-owner");
        if verbose {
            command.arg("--verbose");
        }
        command.arg("--file");
    }
    command.arg(&source_tar);
    command.arg("--directory").arg(source_dir_tmp);
    command.arg("--strip-components").arg("1");
    run_command(command, logger)?;
    Ok(())
}

pub(crate) fn fetch_cargo(
    source_dir: &PathBuf,
    cargopath: Option<&String>,
    logger: &PtyOut,
) -> Result<()> {
    let mut source_dir = source_dir.clone();
    if let Some(cargopath) = cargopath {
        source_dir = source_dir.join(cargopath);
    }

    // Canonicalize source_dir so that relative path dependencies in Cargo.toml
    // resolve correctly when the recipe directory is a symlink (e.g. recipes/system/foo -> local/recipes/system/foo).
    // Without canonicalization, cargo resolves relative paths from the symlink location,
    // which may have a different depth than the real path, causing path resolution failures.
    if let Ok(canonical) = source_dir.canonicalize() {
        source_dir = canonical;
    }

    let local_redoxer = Path::new("target/release/cookbook_redbear_redoxer");
    let mut command = if is_redox() && !local_redoxer.is_file() {
        Command::new("cookbook_redbear_redoxer")
    } else {
        let cookbook_redoxer = local_redoxer
            .canonicalize()
            .unwrap_or(PathBuf::from("cargo"));
        Command::new(&cookbook_redoxer)
    };
    command.arg("fetch");
    command.arg("--manifest-path");
    command.arg(source_dir.join("Cargo.toml").into_os_string());
    run_command(command, logger)?;
    Ok(())
}

pub fn fetch_remote(
    recipe_dir: &Path,
    recipe: &CookRecipe,
    offline_mode: bool,
    source_dir: PathBuf,
    logger: &PtyOut,
) -> Result<FetchResult> {
    let (mut manager, repository) = fetch_repo::get_binary_repo();
    let target_dir = create_target_dir(recipe_dir, recipe.target)?;
    if logger.is_some() {
        let writer = logger.as_ref().unwrap().1.try_clone().unwrap();
        manager.set_callback(Rc::new(RefCell::new(PlainPtyCallback::new(writer))));
    }
    let packages = recipe.recipe.get_packages_list();

    let name = recipe_dir
        .file_name()
        .ok_or("Unable to get recipe name")?
        .to_str()
        .unwrap();

    let mut result = None;
    let mut cached = true;

    for package in packages {
        let (_, source_pkgar, source_toml) = package_source_paths(package, &target_dir);
        let source_name = get_package_name(name, package);
        let Some(repo_blake3) = repository.packages.get(&source_name) else {
            bail_other_err!("Package {source_name} does not exist in server repository")
        };

        if !offline_mode {
            if source_toml.is_file() {
                let pkg_toml = read_source_toml(&source_toml)?;
                if &pkg_toml.blake3 != repo_blake3 {
                    log_to_pty!(logger, "DEBUG: Updating source binaries");
                    remove_all(&source_toml)?;
                    if source_pkgar.is_file() {
                        remove_all(&source_pkgar)?;
                    }
                }
            }

            if !source_toml.is_file() {
                {
                    let toml_file = File::create(&source_toml)
                        .map_err(|e| format!("Unable to create source.toml: {e:?}"))?;
                    let mut writer = DownloadBackendWriter::ToFile(toml_file);
                    manager
                        .download(&format!("{}.toml", &source_name), None, &mut writer)
                        .map_err(|e| format!("Unable to download source.toml: {e:?}"))?;
                }
                let pkg_toml = read_source_toml(&source_toml)?;
                let pkgar_file = File::create(&source_pkgar)
                    .map_err(|e| format!("Unable to create source.pkgar: {e:?}"))?;
                let mut writer = DownloadBackendWriter::ToFile(pkgar_file);
                manager
                    .download(
                        &format!("{}.pkgar", &source_name),
                        Some(pkg_toml.network_size),
                        &mut writer,
                    )
                    .map_err(|e| format!("Unable to download source.pkgar: {e:?}"))?;

                cached = false;
            }

            // manager.download(file, 0, dest)
        } else {
            offline_check_exists(&source_pkgar)?;
            offline_check_exists(&source_toml)?;
        }

        // guaranteed to exist once and last in iteration
        if package.is_none() {
            let pkg_toml = read_source_toml(&source_toml)?;

            fetch_apply_source_info_from_remote(
                recipe,
                &SourceIdentifier {
                    commit_identifier: pkg_toml.commit_identifier.clone(),
                    source_identifier: pkg_toml.source_identifier.clone(),
                    time_identifier: pkg_toml.time_identifier.clone(),
                    ..Default::default()
                },
            )?;

            result = Some(FetchResult::new(
                source_dir.clone(),
                pkg_toml.source_identifier,
                cached,
            ));
        }
    }

    result.ok_or_else(wrap_other_err!("There's no mandatory package in remote"))
}

fn read_source_toml(source_toml: &Path) -> Result<pkg::Package> {
    let mut file =
        File::open(source_toml).map_err(|e| format!("Unable to open source.toml: {e:?}"))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|e| format!("Unable to read source.toml: {e:?}"))?;
    let pkg_toml = pkg::Package::from_toml(&contents)
        .map_err(|e| format!("Unable to parse source.toml: {e:?}"))?;
    Ok(pkg_toml)
}

pub(crate) fn fetch_is_patches_newer(
    recipe_dir: &Path,
    patches: &Vec<String>,
    source_dir: &PathBuf,
) -> Result<bool> {
    // don't check source files inside as it can be mixed with user patches
    let source_time = modified(&source_dir)?;
    for patch_name in patches {
        let patch_file = recipe_dir.join(patch_name);
        if !patch_file.is_file() {
            bail_other_err!("Failed to find patch file {:?}", patch_file.display());
        }

        let patch_time = modified(&patch_file)?;
        if patch_time > source_time {
            return Ok(true);
        }
    }
    return Ok(false);
}

pub(crate) fn fetch_apply_patches(
    recipe_dir: &Path,
    patches: &Vec<String>,
    script: &Option<String>,
    source_dir_tmp: &PathBuf,
    logger: &PtyOut,
) -> Result<()> {
    if patches.is_empty() && script.is_none() {
        return Ok(());
    }

    // Read and normalize all patch files.
    let mut patch_contents: Vec<(String, Vec<u8>)> = Vec::new();
    for patch_name in patches {
        let patch_file = recipe_dir.join(patch_name);
        if !patch_file.is_file() {
            bail_other_err!("Failed to find patch file {:?}", patch_file.display());
        }
        let raw = fs::read(&patch_file).map_err(|err| {
            format!(
                "failed to read patch file '{}': {err}",
                patch_file.display()
            )
        })?;
        let normalized = normalize_patch(&raw);
        patch_contents.push((patch_name.clone(), normalized));
    }

    // Apply all patches atomically to a staging directory.
    // If any patch fails, the staging directory is discarded and the
    // original source tree is left untouched.
    // Uses cp -al (hard links) for zero-copy staging.
    let staging_dir = source_dir_tmp.with_extension("staging");
    let _ = fs::remove_dir_all(&staging_dir);
    Command::new("cp")
        .arg("-al")
        .arg(source_dir_tmp)
        .arg(&staging_dir)
        .status()
        .map_err(|e| format!("failed to create staging copy via cp -al: {e}"))?;

    let result = (|| -> Result<Vec<String>> {
        let mut applied = Vec::new();
        for (patch_name, patch_data) in &patch_contents {
            let mut command = Command::new("patch");
            command.arg("--directory").arg(&staging_dir);
            command.arg("--strip=1");
            command.arg("--batch");
            command.arg("--fuzz=0");
            run_command_stdin(command, patch_data.as_slice(), logger)
                .map_err(|e| format!("patch {patch_name} FAILED: {e}"))?;

            for ext in &["rej", "orig"] {
                let rej_check = Command::new("find")
                    .arg(&staging_dir)
                    .arg("-name")
                    .arg(format!("*.{ext}"))
                    .arg("-print")
                    .arg("-quit")
                    .output();
                if let Ok(out) = rej_check {
                    if !out.stdout.is_empty() {
                        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        bail_other_err!(
                            "patch {patch_name} left .{ext} file (hunks failed to apply): {path}"
                        );
                    }
                }
            }
            applied.push(patch_name.clone());
        }
        Ok(applied)
    })();

    match result {
        Ok(applied) => {
            let backup_dir = source_dir_tmp.with_extension("backup");
            let _ = fs::remove_dir_all(&backup_dir);
            fs::rename(source_dir_tmp, &backup_dir)
                .map_err(|e| format!("failed to rename source to backup: {e}"))?;
            fs::rename(&staging_dir, source_dir_tmp)
                .map_err(|e| format!("failed to promote staging to source: {e}"))?;
            let _ = fs::remove_dir_all(&backup_dir);

            fetch_write_patches_state(recipe_dir, &applied, source_dir_tmp, script, logger)?;

            if let Some(script) = script {
                let mut command = Command::new("bash");
                command.arg("-ex");
                command.current_dir(source_dir_tmp);
                run_command_stdin(
                    command,
                    format!("{SHARED_PRESCRIPT}\n{script}").as_bytes(),
                    logger,
                )?;
            }
            log_to_pty!(logger, "[ATOMIC] {n}/{n} patches applied", n = applied.len());
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&staging_dir);
            log_to_pty!(logger, "[ATOMIC] patch application rolled back — source tree unchanged");
            Err(e)
        }
    }
}

/// Normalizes a patch for compatibility with the `patch` command by stripping
/// git-specific headers (`diff --git`, `index`, `new file mode`, etc.) that
/// `patch` does not recognize.
fn normalize_patch(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());
    let mut prev_empty = true;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("diff --git ")
            || trimmed.starts_with("index ")
            || trimmed.starts_with("new file mode ")
            || trimmed.starts_with("deleted file mode ")
            || trimmed.starts_with("rename from ")
            || trimmed.starts_with("rename to ")
            || trimmed.starts_with("similarity index ")
            || trimmed.starts_with("dissimilarity index ")
        {
            continue;
        }
        if !prev_empty || !line.is_empty() {
            out.push_str(line);
            out.push('\n');
            prev_empty = line.is_empty();
        }
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.into_bytes()
}

/// Computes a BLAKE3 hash over all patch file contents (in order).
fn fetch_compute_patches_hash(
    recipe_dir: &Path,
    patches: &[String],
) -> Result<String> {
    // BLAKE3 is already a project dependency (used for source verification).
    let mut hasher = blake3::Hasher::new();
    for patch_name in patches {
        let patch_file = recipe_dir.join(patch_name);
        let content = fs::read(&patch_file).map_err(|err| {
            format!("failed to read patch for hashing '{}': {err}", patch_file.display())
        })?;
        hasher.update(&content);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Writes a .patches-state file into the recipe's *target* directory
/// (NOT the source checkout — git clean would delete it otherwise).
/// Contains: upstream commit, ordered patch list, composite hash, script hash,
/// and state schema version for forward-compatibility.
/// Computes a BLAKE3 hash over all tracked files in the source directory,
/// so that manual source edits (outside the patch system) are detected
/// and trigger re-patching on the next build.
fn fetch_compute_source_hash(source_dir: &Path) -> String {
    let output = Command::new("git")
        .arg("-C").arg(source_dir)
        .args(["ls-files", "-z"])
        .output();
    match output {
        Ok(out) if !out.stdout.is_empty() => {
            let mut hasher = blake3::Hasher::new();
            // Hash file paths in sorted order for stability.
            let mut files: Vec<&str> = out.stdout
                .split(|&b| b == 0)
                .filter_map(|s| std::str::from_utf8(s).ok())
                .collect();
            files.sort();
            for path in &files {
                hasher.update(path.as_bytes());
                hasher.update(b"\0");
                // Hash file contents for integrity.
                if let Ok(content) = fs::read(source_dir.join(path)) {
                    hasher.update(&content);
                }
                hasher.update(b"\0");
            }
            hasher.finalize().to_hex().to_string()
        }
        _ => "no-git".to_string(),
    }
}

fn fetch_write_patches_state(
    recipe_dir: &Path,
    applied: &[String],
    source_dir: &Path,
    script: &Option<String>,
    logger: &PtyOut,
) -> Result<()> {
    let head_rev = get_git_head_rev(&source_dir.to_path_buf())
        .map(|(r, _)| r)
        .unwrap_or_else(|_| "unknown".to_string());
    let hash = fetch_compute_patches_hash(recipe_dir, applied)
        .unwrap_or_else(|_| "hash-error".to_string());
    let script_hash = script.as_ref().map(|s| {
        blake3::hash(s.as_bytes()).to_hex().to_string()
    }).unwrap_or_else(|| "none".to_string());

    // State goes in target/ so git clean/reset won't delete it.
    let state_dir = recipe_dir.join("target");
    let _ = fs::create_dir_all(&state_dir);
    let state_file = state_dir.join(".patches-state");

    let source_hash = fetch_compute_source_hash(source_dir);

    let mut content = String::new();
    content.push_str("schema: 1\n");
    content.push_str(&format!("upstream-rev: {head_rev}\n"));
    content.push_str(&format!("patches-hash: {hash}\n"));
    content.push_str(&format!("script-hash: {script_hash}\n"));
    content.push_str(&format!("source-hash: {source_hash}\n"));
    for (i, name) in applied.iter().enumerate() {
        content.push_str(&format!("patch[{}]: {name}\n", i + 1));
    }
    fs::write(&state_file, &content).map_err(|err| {
        format!("failed to write .patches-state: {err}")
    })?;
    log_to_pty!(logger, "[OK] wrote .patches-state ({}/{} patches)", applied.len(), applied.len());
    Ok(())
}

/// Validates that every patch file path resolves to a real file before we
/// touch the source tree.  Fails early with a clear message if any symlink
/// is broken or file is missing.
fn fetch_validate_patch_symlinks(
    recipe_dir: &Path,
    patches: &[String],
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for patch_name in patches {
        let patch_file = recipe_dir.join(patch_name);
        if !patch_file.is_file() {
            bail_other_err!(
                "patch file not found: {:?}  (broken symlink or missing file in {})",
                patch_file.display(),
                recipe_dir.display()
            );
        }
        // Canonicalize to catch symlink chains
        let canonical = patch_file.canonicalize().map_err(|e| {
            format!(
                "cannot resolve patch path {:?}: {e}  (broken symlink?)",
                patch_file.display()
            )
        })?;
        if !seen.insert(canonical) {
            bail_other_err!(
                "duplicate patch after canonicalization: {:?}  (listed twice in recipe?)",
                patch_name
            );
        }
    }
    Ok(())
}

/// Checks whether the source directory's .patches-state matches the
/// recipe's current patch list.  Returns true if patches should be
/// (re-)applied.
fn fetch_patches_state_stale(
    recipe_dir: &Path,
    patches: &[String],
    script: &Option<String>,
    source_dir: &Path,
) -> bool {
    let state_file = recipe_dir.join("target/.patches-state");
    let state_content = match fs::read_to_string(&state_file) {
        Ok(c) => c,
        Err(_) => return true,
    };

    let expected_hash = match fetch_compute_patches_hash(recipe_dir, patches) {
        Ok(h) => h,
        Err(_) => return true,
    };
    let expected_script_hash = script.as_ref().map(|s| {
        blake3::hash(s.as_bytes()).to_hex().to_string()
    }).unwrap_or_else(|| "none".to_string());
    let current_source_hash = fetch_compute_source_hash(source_dir);

    let mut found_hash = false;
    let mut found_script = false;
    let mut found_source = false;
    for line in state_content.lines() {
        if let Some(stored) = line.strip_prefix("patches-hash: ") {
            if stored.trim() != expected_hash { return true; }
            found_hash = true;
        }
        if let Some(stored) = line.strip_prefix("script-hash: ") {
            if stored.trim() != expected_script_hash { return true; }
            found_script = true;
        }
        if let Some(stored) = line.strip_prefix("source-hash: ") {
            if stored.trim() != current_source_hash { return true; }
            found_source = true;
        }
    }

    !found_hash || !found_script || !found_source
}

pub(crate) fn fetch_apply_source_info(
    recipe: &CookRecipe,
    source_identifier: String,
) -> Result<String> {
    let ident = crate::cook::ident::get_ident();
    let info = SourceIdentifier {
        commit_identifier: ident.commit.to_string(),
        time_identifier: ident.time.to_string(),
        source_identifier: source_identifier,
    };

    fetch_apply_source_info_from_remote(&recipe, &info)?;

    Ok(info.source_identifier)
}

pub(crate) fn fetch_apply_source_info_from_remote(
    recipe: &CookRecipe,
    info: &SourceIdentifier,
) -> Result<()> {
    let target_dir = create_target_dir(&recipe.dir, recipe.target)?;
    let source_toml_path = target_dir.join("source_info.toml");
    serialize_and_write(&source_toml_path, &info)?;
    Ok(())
}

pub fn fetch_get_source_info(recipe: &CookRecipe) -> Result<SourceIdentifier> {
    let target_dir = recipe.target_dir();
    let source_toml_path = target_dir.join("source_info.toml");
    let toml_content = fs::read_to_string(source_toml_path)
        .map_err(|e| format!("Unable to read source_info.toml: {:?}", e))?;
    let parsed = toml::from_str(&toml_content)
        .map_err(|e| format!("Unable to parse source_info.toml: {:?}", e))?;
    Ok(parsed)
}
