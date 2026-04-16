# External Redox Toolchain Export

This repo already builds the Redox cross toolchain into:

```text
prefix/x86_64-unknown-redox/sysroot
```

That works for in-tree builds, but it is awkward for external consumers because:

- the checkout path leaks into ad hoc scripts and CMake files,
- `pkg-config` and `llvm-config` need host-side wrappers,
- consumers usually want a single directory they can add to `PATH`.

## Proposed Export Shape

Export a standalone toolchain directory:

```text
<dest>/
├── activate.sh
├── bin/
│   ├── x86_64-unknown-redox-gcc
│   ├── x86_64-unknown-redox-c++
│   ├── x86_64-unknown-redox-ar
│   ├── x86_64-unknown-redox-ranlib
│   ├── x86_64-unknown-redox-ld
│   ├── x86_64-unknown-redox-strip
│   ├── x86_64-unknown-redox-objcopy
│   ├── x86_64-unknown-redox-objdump
│   ├── x86_64-unknown-redox-pkg-config
│   └── x86_64-unknown-redox-llvm-config
└── sysroot/
```

`bin/` contains symlinks to the real cross binaries inside `sysroot/bin`, plus host-side
wrappers for `pkg-config` and `llvm-config`.

## Export Script

Use:

```bash
./local/scripts/export-x86_64-unknown-redox-toolchain.sh /opt/redbear/toolchains/x86_64-unknown-redox
```

Defaults:

- source sysroot: `prefix/x86_64-unknown-redox/sysroot`
- export destination: `build/toolchain-export/x86_64-unknown-redox`

Optional overrides:

```bash
TARGET=x86_64-unknown-redox \
SOURCE_SYSROOT=/custom/sysroot \
./local/scripts/export-x86_64-unknown-redox-toolchain.sh /tmp/redox-toolchain
```

## Use From External Builds

```bash
source /opt/redbear/toolchains/x86_64-unknown-redox/activate.sh
x86_64-unknown-redox-gcc --version
```

`activate.sh` exports:

- `PATH=<toolchain>/bin:<toolchain>/sysroot/bin:$PATH`
- `TARGET=x86_64-unknown-redox`
- `REDBEAR_REDOX_SYSROOT=<toolchain>/sysroot`
- `COOKBOOK_HOST_SYSROOT=<toolchain>/sysroot`
- `COOKBOOK_SYSROOT=<toolchain>/sysroot`

That keeps external CMake, Cargo, Meson, and ad hoc builds aligned with the in-tree cookbook
environment.

## Why This Shape

- It is relocatable after export.
- It does not require the original repo checkout at runtime.
- It reuses the already-built canonical sysroot from `mk/prefix.mk`.
- It avoids teaching every external project Red Bear-specific path conventions.
