use std::env;
use std::fs;
use std::path::Path;

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    let headers_src = Path::new(&manifest_dir).join("src/c_headers");
    let headers_dst = Path::new(&out_dir).join("include");

    if headers_src.exists() {
        copy_dir_recursive(&headers_src, &headers_dst)
            .expect("failed to copy C headers to OUT_DIR");

        println!("cargo:include={}", headers_dst.display());
    }

    let sysroot = env::var("COOKBOOK_SYSROOT").ok();
    if let Some(ref sysroot_path) = sysroot {
        let sysroot_include = Path::new(sysroot_path).join("include/linux-kpi");
        if headers_src.exists() {
            copy_dir_recursive(&headers_src, &sysroot_include)
                .expect("failed to copy C headers to COOKBOOK_SYSROOT");
        }
    }

    let stage = env::var("COOKBOOK_STAGE").ok();
    if let Some(ref stage_path) = stage {
        let stage_include = Path::new(stage_path).join("usr/include/linux-kpi");
        if headers_src.exists() {
            copy_dir_recursive(&headers_src, &stage_include)
                .expect("failed to copy C headers to COOKBOOK_STAGE");
        }
    }

    println!("cargo:rerun-if-changed=src/c_headers");
}
