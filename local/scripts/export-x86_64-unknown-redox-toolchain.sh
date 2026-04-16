#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
root_dir="$(CDPATH= cd -- "${script_dir}/../.." && pwd)"

target="${TARGET:-x86_64-unknown-redox}"
source_sysroot="${SOURCE_SYSROOT:-${root_dir}/prefix/${target}/sysroot}"
dest_root="${1:-${root_dir}/build/toolchain-export/${target}}"
dest_sysroot="${dest_root}/sysroot"
dest_bin="${dest_root}/bin"
partial_root="${dest_root}.partial"
partial_sysroot="${partial_root}/sysroot"
partial_bin="${partial_root}/bin"

if [ ! -d "${source_sysroot}" ]; then
    echo "error: missing source sysroot: ${source_sysroot}" >&2
    echo "hint: build the prefix first with 'make prefix TARGET=${target}' or 'make prefix'" >&2
    exit 1
fi

rm -rf "${partial_root}"
mkdir -p "${partial_root}"

cp -a "${source_sysroot}" "${partial_sysroot}"
mkdir -p "${partial_bin}"

for tool in gcc c++ ar ranlib ld strip objcopy objdump; do
    tool_name="${target}-${tool}"
    if [ -x "${partial_sysroot}/bin/${tool_name}" ]; then
        ln -s "../sysroot/bin/${tool_name}" "${partial_bin}/${tool_name}"
    fi
done

cat > "${partial_bin}/${target}-pkg-config" <<EOF
#!/usr/bin/env bash
set -euo pipefail

script_dir="\$(CDPATH= cd -- "\$(dirname -- "\$0")" && pwd)"
toolchain_root="\$(CDPATH= cd -- "\${script_dir}/.." && pwd)"
sysroot="\${REDBEAR_REDOX_SYSROOT:-\${toolchain_root}/sysroot}"

export PKG_CONFIG_SYSROOT_DIR="\${PKG_CONFIG_SYSROOT_DIR:-\${sysroot}}"
export PKG_CONFIG_LIBDIR="\${PKG_CONFIG_LIBDIR:-\${sysroot}/lib/pkgconfig}"
export PKG_CONFIG_PATH="\${PKG_CONFIG_PATH:-\${sysroot}/share/pkgconfig}"

if [ -n "\${COOKBOOK_DYNAMIC:-}" ]; then
    exec pkg-config "\$@"
else
    exec pkg-config --static "\$@"
fi
EOF

cat > "${partial_bin}/${target}-llvm-config" <<EOF
#!/usr/bin/env python3
import os
import subprocess
import sys

LLVM_CONFIG = "/bin/llvm-config"
TARGET = "${target}"

ARCH_MAP = {
    "x86_64": ("X86", "x86", "X86"),
    "i586": ("X86", "x86", "X86"),
    "aarch64": ("AArch64", "aarch64", "AArch64"),
    "riscv64gc": ("RISCV", "riscv", "RISCV"),
}

ALL_ARCH_COMPS = ["x86", "aarch64", "riscv"]
ALL_ARCH_LIBS = ["X86", "AArch64", "RISCV"]


def is_unwanted_arch(item, allowed_prefix, all_prefixes, is_lib=False):
    matched_arch = None
    for arch in all_prefixes:
        if is_lib and f"LLVM{arch}" in item:
            matched_arch = arch
            break
        if not is_lib and item.startswith(arch):
            matched_arch = arch
            break

    return matched_arch is not None and matched_arch != allowed_prefix


def main():
    script_dir = os.path.dirname(os.path.realpath(__file__))
    toolchain_root = os.path.dirname(script_dir)
    toolchain_path = os.environ.get("COOKBOOK_HOST_SYSROOT", os.path.join(toolchain_root, "sysroot"))
    sysroot_path = os.environ.get("COOKBOOK_SYSROOT", toolchain_path)
    target_triple = os.environ.get("TARGET", TARGET)

    target_arch = target_triple.split("-")[0]
    mapped_archs = ARCH_MAP.get(target_arch)
    if mapped_archs is None:
        print(f"Error: unsupported target architecture in {target_triple}", file=sys.stderr)
        sys.exit(1)

    target_built_name, comp_prefix, lib_prefix = mapped_archs
    cmd = [toolchain_path + LLVM_CONFIG] + sys.argv[1:]

    try:
        result = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=sys.stderr,
            check=False,
            text=True,
        )
    except FileNotFoundError:
        print(f"Error: Could not find executable '{LLVM_CONFIG}' under {toolchain_path}", file=sys.stderr)
        sys.exit(1)

    if result.returncode != 0:
        sys.exit(result.returncode)

    output = result.stdout.strip()
    args_set = set(sys.argv[1:])

    if "--bindir" in args_set:
        output = toolchain_path + "/usr/bin"
    elif "--targets-built" in args_set:
        output = target_built_name
    elif "--components" in args_set:
        components = output.split()
        output = " ".join(
            c for c in components if not is_unwanted_arch(c, comp_prefix, ALL_ARCH_COMPS, is_lib=False)
        )
    elif "--libs" in args_set:
        libs = output.split()
        output = " ".join(
            l for l in libs if not is_unwanted_arch(l, lib_prefix, ALL_ARCH_LIBS, is_lib=True)
        )
        output = output.replace(toolchain_path.rstrip(os.sep), sysroot_path.rstrip(os.sep))
    else:
        output = output.replace(toolchain_path.rstrip(os.sep), sysroot_path.rstrip(os.sep))

    print(output, end="\n")


if __name__ == "__main__":
    main()
EOF

cat > "${partial_root}/activate.sh" <<EOF
#!/usr/bin/env bash
set -euo pipefail

toolchain_root="\$(CDPATH= cd -- "\$(dirname -- "\${BASH_SOURCE[0]}")" && pwd)"

export TARGET="${target}"
export REDBEAR_REDOX_SYSROOT="\${REDBEAR_REDOX_SYSROOT:-\${toolchain_root}/sysroot}"
export COOKBOOK_HOST_SYSROOT="\${COOKBOOK_HOST_SYSROOT:-\${REDBEAR_REDOX_SYSROOT}}"
export COOKBOOK_SYSROOT="\${COOKBOOK_SYSROOT:-\${REDBEAR_REDOX_SYSROOT}}"
export PATH="\${toolchain_root}/bin:\${REDBEAR_REDOX_SYSROOT}/bin:\${PATH}"

echo "Activated ${target} toolchain from \${toolchain_root}"
EOF

chmod 0755 \
    "${partial_root}/activate.sh" \
    "${partial_bin}/${target}-pkg-config" \
    "${partial_bin}/${target}-llvm-config"

rm -rf "${dest_root}"
mv "${partial_root}" "${dest_root}"

echo "exported ${target} toolchain to ${dest_root}"
echo "source ${dest_root}/activate.sh"
