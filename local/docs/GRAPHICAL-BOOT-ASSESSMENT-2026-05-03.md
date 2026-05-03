# Red Bear OS — Graphical Boot Assessment

**Date**: 2026-05-03
**Tested**: redbear-full live ISO + harddrive.img in QEMU

## Result: Graphical boot FAILED

### Root Cause

`make live CONFIG_NAME=redbear-full` fails during KDE package build:

```
cook kf6-kitemviews - failed
failed to build: failed to install 'recipes/wip/wayland/libwayland/.../stage.pkgar'
in 'recipes/kde/kf6-kitemviews/.../sysroot.tmp': No such file or directory
```

The KDE package dependency chain (`libwayland` → `kf6-kitemviews`) has a staging/packaging race condition — `libwayland` builds successfully but its `stage.pkgar` is not found when `kf6-kitemviews` tries to install it as a dependency.

### What Works

| Component | Status |
|-----------|--------|
| redbear-mini (text-only) | ✅ Boots to login prompt |
| redbear-full base packages | ✅ Build and boot |
| Kernel, drivers, initfs | ✅ Works |
| Colored init output | ✅ Visible in QEMU |
| evdevd, inputd, ps2d | ✅ Registered |
| D-Bus daemon | 🔲 Not tested (build failed before image creation) |
| redbear-sessiond (login1) | 🔲 Not tested |
| KWin / Wayland | 🔲 Not tested |
| Greeter | 🔲 Not tested |

### What redbear-mini Shows

Booted in QEMU with UEFI/OVMF:
```
[ OK ] switchroot to /scheme/initfs
[ OK ] Started Logger
[ OK ] Started /dev/random
[ OK ] Started /dev/zero
[ OK ] Started Set time from realtime clock
[ OK ] Started VT input and graphics multiplexer
[ OK ] Started Graphical bootlog
[ OK ] Started Framebuffer text console
[ OK ] Started PS/2 driver
[ OK ] Started Hardware manager
[ OK ] Started PCI driver spawner
[ OK ] Started Rootfs
[ OK ] switchroot to /usr /etc
[ OK ] Started PTY daemon
[ OK ] Started IPC daemon
[ OK ] Started Network stack
[ OK ] Started DHCP client daemon
[ OK ] Started Input event device daemon (evdevd)
RB_SERIAL_PROBE_OK
```

Console login is available on the framebuffer (not visible with `-nographic`).

### Blockers for Graphical Boot

| Blocker | Detail |
|---------|--------|
| KDE package build chain | `libwayland` pkgar missing when `kf6-kitemviews` builds |
| QML gate | `kirigami` → `plasma-framework` → `plasma-workspace` requires QtQuick/QML |
| KWin compilation | KWin builds but runtime is untested due to above blockers |
| Wayland compositor | `redbear-compositor` not yet in boot path |

### Recommendation

The KDE package dependency issue is a **build system artifact staging problem** — the cookbook tool's pkgar push mechanism doesn't properly order dependency installations when packages are rebuilt. This requires a fix to `src/cook/` (the Rust cookbook tool) to ensure dependency packages have their pkgar files available before dependents attempt to install them.
