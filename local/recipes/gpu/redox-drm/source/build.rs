use std::env;
use std::path::{Path, PathBuf};

const LIB_NAME: &str = "libamdgpu_dc_redox.so";
const ENV_HINTS: &[&str] = &[
    "AMDGPU_DC_LIB_DIR",
    "COOKBOOK_STAGE",
    "COOKBOOK_SYSROOT",
    "REDOX_SYSROOT",
    "SYSROOT",
    "TARGET_SYSROOT",
];

fn push_candidate_dirs(candidates: &mut Vec<PathBuf>, base: &Path) {
    candidates.push(base.to_path_buf());
    candidates.push(base.join("usr/lib/redox/drivers"));
    candidates.push(base.join("lib"));
    candidates.push(base.join("usr/lib"));
}

fn register_candidate_watch(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

fn find_amdgpu_dc_library(manifest_dir: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    for key in ENV_HINTS {
        println!("cargo:rerun-if-env-changed={key}");
        if let Some(value) = env::var_os(key) {
            push_candidate_dirs(&mut candidates, Path::new(&value));
        }
    }

    push_candidate_dirs(&mut candidates, &manifest_dir.join("../../amdgpu"));
    push_candidate_dirs(&mut candidates, &manifest_dir.join("../../amdgpu/stage"));
    push_candidate_dirs(&mut candidates, &manifest_dir.join("../amdgpu"));
    push_candidate_dirs(&mut candidates, &manifest_dir.join("../amdgpu/stage"));

    for dir in candidates {
        register_candidate_watch(&dir.join(LIB_NAME));
        if dir.join(LIB_NAME).exists() {
            return Some(dir);
        }
    }

    None
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(no_amdgpu_c)");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));

    if let Some(dir) = find_amdgpu_dc_library(&manifest_dir) {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=amdgpu_dc_redox");
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=m");
    } else {
        println!("cargo:rustc-cfg=no_amdgpu_c");
    }
}
