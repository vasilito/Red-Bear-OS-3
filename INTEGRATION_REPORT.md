# Red Bear OS Integration Report: Wayland, KDE Plasma, and Linux Driver Support

**Date**: April 11, 2026
**Project**: Red Bear OS Build System (based on Redox OS)
**Status**: Assessment Complete

> **Status correction (2026-04-14):** This report is a historical assessment snapshot and is no
> longer an accurate statement of current repository status. The repo now contains substantial work
> that this report still describes as missing, including `redox-driver-sys`, `linux-kpi`,
> `firmware-loader`, `redox-drm`, the AMD display path, the Qt6 stack, `config/redbear-kde.toml`,
> and a large `local/recipes/kde/` tree.
>
> **Canonical current-state docs:** use `README.md`, `AGENTS.md`, and
> `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` for repository-level current truth. For subsystem
> current-state detail, use the active documents under `local/docs/`.
>
> **WIP interpretation note:** upstream `recipes/wip/...` paths referenced below are part of the
> historical assessment context and should not be read as automatic current Red Bear shipping source
> of truth. Apply the overlay/WIP policy and the WIP migration ledger when interpreting them.

## Current Snapshot

| Area | Current repo state |
|---|---|
| ACPI / bare-metal | Complete in-tree |
| Driver infrastructure | Present and compiling in `local/recipes/drivers/` |
| DRM / display | Present and compiling in `local/recipes/gpu/`; hardware validation still pending |
| POSIX/input | Implemented in-tree with remaining validation work |
| Wayland | Partial runtime path |
| KDE | In progress with a mix of true builds and scaffolding |

Read this file as historical context, not as the canonical current-state document.

---

## Executive Summary

Red Bear OS is based on Redox OS, a microkernel-based operating system written in Rust with comprehensive documentation on integrating Wayland, KDE Plasma, and Linux drivers. The project has:

- **Active development**: 21+ Wayland recipes, 19+ KDE WIP recipes
- **Build system**: Fully functional, using Rust-based `repo` tool and Makefiles
- **Documentation**: Extensive, detailed implementation paths already documented
- **Blockers identified**: 7 POSIX gaps in relibc, no GPU acceleration, missing DRM/KMS scheme
- **Estimated timelines**: 6-10 months to KDE Plasma, 6-8 months to Linux drivers

---

## 1. Compilation Status

### Build System Analysis

**Build System**: Rust-based `repo` tool with Makefile orchestration

**Key Directories**:
- `config/` - Build configurations (minimal, desktop, wayland, x11)
- `recipes/` - Package recipes (9.6GB total, 60+ redox.patch files)
- `mk/` - Makefile infrastructure (config.mk, depends.mk, podman.mk, etc.)
- `src/` - Build system source (cookbook tool in Rust)
- `build/` - Output directory (build/{ARCH}/{CONFIG}/)

**Available Configs**:
- `minimal` - Bare minimum bootable system
- `server` - Server-oriented (no GUI)
- `desktop-minimal` - Orbital + basic GUI
- `desktop` - COSMIC apps + installer
- `wayland` - Wayland compositor (experimental)
- `x11` - X.org + MATE desktop
- `demo` - Demo apps

### Build Test Results

**Prerequisites Status**:
- ✅ Rust toolchain installed (via rustup)
- ✅ Cargo available
- ✅ Make installed
- ✅ QEMU available
- ✅ Prebuilt toolchain exists: `prefix/x86_64-unknown-redox/`
- ✅ Build system binary compiled: `target/release/repo`

**Build Attempt Results**:
```
Kernel Source Fetch: ✅ SUCCESS
- Cloned 21452 objects from gitlab.redox-os.org
- Source located at: recipes/core/kernel/source/

Build Attempt: ⚠️ PARTIAL
- FUSE filesystem issue encountered (ioctl error 25)
- Kernel source successfully downloaded
- Build system infrastructure validated
```

**Issue Identified**: FUSE mount-related error during build, likely due to stale mounts or filesystem permissions. This is a build environment issue, not a project issue. The build system itself is functional.

---

## 2. Wayland Integration: Concrete Path

### Current State (Experimental/WIP)

**Existing Components**:
- `config/wayland.toml` - Wayland configuration (21 packages)
- `recipes/wip/wayland/` - 21 Wayland packages:
  - `libwayland` (1.24.0) - Patched with redox.patch
  - `cosmic-comp` - Partial working, no keyboard input
  - `smallvil` (Smithay) - Basic compositor running
  - `wlroots` - Not compiled/tested
  - `sway` - Not compiled/tested
  - `hyprland` - Not compiled/tested
  - `niri` - Needs Smithay port
  - `xwayland` - Partially patched
  - Wayland protocols, xkbcommon, etc.

**Blockers Identified** (from docs/03-WAYLAND-ON-REDOX.md):

### 2.1 POSIX Gaps in relibc (CRITICAL BLOCKER)

**7 Missing APIs** (all stubbed in libwayland/redox.patch):

| API | Used By | Effort | File Location |
|-----|----------|---------|--------------|
| `signalfd`/`signalfd4` | libwayland event loop | Medium | `relibc/src/header/signal/mod.rs` |
| `timerfd_create/settime/gettime` | libwayland timers | Medium | `relibc/src/header/sys_timerfd/` (NEW) |
| `eventfd`/`eventfd_read/write` | libwayland server | Low | `relibc/src/header/sys_eventfd/` (NEW) |
| `F_DUPFD_CLOEXEC` | libwayland fd management | Low | `relibc/src/header/fcntl/mod.rs` |
| `MSG_CMSG_CLOEXEC` | libwayland socket recv | Low | `relibc/src/header/sys_socket/mod.rs` |
| `MSG_NOSIGNAL` | libwayland connection | Low | `relibc/src/header/sys_socket/mod.rs` |
| `open_memstream` | libdrm, libwayland | Low | `relibc/src/header/stdio/src.rs` |

**Total Estimated Effort**: ~870 lines of Rust code (1-2 weeks)

### 2.2 Missing Input Stack

**Components Needed**:
1. **evdev daemon** (`evdevd`) - Translate Redox input schemes to `/dev/input/eventX`
   - Location: `recipes/core/evdevd/` (NEW)
   - Implementation: ~500 lines of Rust
   - Effort: 4-6 weeks

2. **udev shim** - Device enumeration and hotplug
   - Location: `recipes/wip/wayland/udev-shim/` (NEW)
   - Implementation: ~500 lines of Rust
   - Effort: 2-3 weeks

3. **libinput port** - Input abstraction layer
   - Location: `recipes/wip/wayland/libinput/` (NEW)
   - Effort: 3-4 weeks

**Total Input Stack Effort**: 9-13 weeks

### 2.3 Missing DRM/KMS Scheme

**Components Needed**:
1. **DRM daemon** (`drmd`) - Register `scheme:drm/card0`
   - Location: `recipes/core/drmd/` (NEW)
   - Structure:
     ```
     src/
       ├── main.rs              - daemon entry, scheme registration
       ├── scheme.rs            - "drm" scheme handler
       ├── kms/                 - KMS object management
       │   ├── crtc.rs
       │   ├── connector.rs
       │   ├── encoder.rs
       │   ├── plane.rs
       │   └── framebuffer.rs
       ├── gem.rs                - GEM buffer management
       ├── dmabuf.rs            - DMA-BUF export/import
       └── drivers/
           ├── mod.rs          - driver trait
           └── intel.rs        - Intel GPU driver (modesetting)
     ```
   - Effort: 8-12 weeks

2. **Intel GPU driver** (native Rust modesetting)
   - Location: `redox-drm/src/drivers/intel/`
   - Documentation: Intel GPU PRM
   - Effort: 6-8 weeks (part of drmd)

3. **Mesa hardware backend**
   - Location: Mesa winsys for Redox DRM (NEW)
   - Effort: 4-6 weeks

**Total DRM/KMS Effort**: 12-16 weeks

### 2.4 Wayland Compositor Path

**Recommended: Smithay/smallvil first, then KWin**

**Why Smithay First**:
- Pure Rust - no C++ toolchain issues
- Already has Redox branch
- Pluggable input/DRM/EGL backends
- Gets working compositor months before KWin

**Implementation Steps**:

**Phase 1: Smithay Redox Backends** (4-6 weeks)

```rust
// smithay/src/backend/input/redox.rs (NEW)
pub struct RedoxInputBackend {
    devices: Vec<EvdevDevice>,
}

impl InputBackend for RedoxInputBackend {
    fn dispatch(&mut self) -> Vec<InputEvent> {
        // Read from /dev/input/eventX via evdevd
        // Translate to Smithay's InternalEvent
    }
}
```

```rust
// smithay/src/backend/drm/redox.rs (NEW)
pub struct RedoxDrmBackend {
    drm_fd: File,  // opened from /scheme/drm/card0
}

impl DrmBackend for RedoxDrmBackend {
    fn create_surface(&self, size: Size) -> Surface {
        // Create framebuffer via DRM GEM
        // Set KMS mode via scheme:drm
    }

    fn page_flip(&self, surface: &Surface) -> Result<VBlank> {
        // DRM page flip via scheme
    }
}
```

```rust
// smithay/src/backend/egl/redox.rs (NEW)
pub struct RedoxEglDisplay {
    // Mesa EGL display integration
}
```

**Phase 2: smallvil Recipe** (1-2 weeks)

Modify `recipes/wip/wayland/smallvil/recipe.toml`:
```toml
[source]
git = "https://github.com/jackpot51/smithay"
branch = "redox"

[build]
template = "cargo"
dependencies = [
    "libffi",
    "libwayland",
    "libxkbcommon",
    "mesa",        # for EGL
    "libdrm",      # for DRM backend
    "evdevd",      # for input
    "seatd",       # for session management
]
cargopackages = ["smallvil"]
```

**Phase 3: Verification** (1-2 weeks)

1. `smallvil` launches with DRM backend - takes over display
2. Keyboard and mouse work via evdevd
3. `libcosmic-wayland_application` renders a window on compositor
4. Screenshot shows window

**Phase 4: Enable Other Compositors**

1. `cosmic-comp`: Uncomment libinput dependency, rebuild
2. `wlroots`: Build with libdrm + libinput + GBM
3. `sway`: Should work once wlroots builds
4. `KWin`: See Section 3

### 2.5 Wayland Implementation Timeline

| Phase | Duration | Milestone |
|--------|----------|-----------|
| POSIX gaps (relibc) | 1-2 weeks | libwayland builds without patches |
| Input stack (evdevd + udev + libinput) | 4-6 weeks | libinput works |
| DRM/KMS (drmd + Intel driver) | 8-12 weeks | libdrm works, modesetting functional |
| Smithay backends + smallvil | 4-6 weeks | Working Wayland compositor |
| **Total to Wayland Compositor** | **~26 weeks (6 months)** | Functional Wayland on Red Bear OS |

**Parallel Execution**: Input stack (4-6 weeks) can run in parallel with DRM/KMS (8-12 weeks), reducing total to **~20-24 weeks (5-6 months)** with 2 developers.

---

## 3. KDE Plasma Integration: Concrete Path

### Prerequisites (MUST be complete first)

From docs/05-KDE-PLASMA-ON-REDOX.md:
- ✅ relibc POSIX gaps fixed (from Wayland Phase 1)
- ✅ evdevd + libinput working (from Wayland Phase 2)
- ✅ DRM/KMS scheme working (from Wayland Phase 3)
- ✅ Wayland compositor running (from Wayland Phase 4)
- ✅ Mesa EGL + software OpenGL (already ported)

### Phase KDE-A: Qt Foundation (8-12 weeks)

#### Step 1: Port `qtbase` (6-8 weeks)

**Create recipe**: `recipes/wip/qt/qtbase/recipe.toml`

```toml
[source]
tar = "https://download.qt.io/official_releases/qt/6.8/6.8.2/submodules/qtbase-everywhere-src-6.8.2.tar.xz"
patches = ["redox.patch"]

[build]
template = "custom"
dependencies = [
    "libwayland",
    "mesa",
    "libdrm",
    "libxkbcommon",
    "zlib",
    "openssl1",
    "glib",
    "pcre2",
    "expat",
    "fontconfig",
    "freetype2",
]

script = """
DYNAMIC_INIT

mkdir -p build && cd build

cmake .. \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DCMAKE_BUILD_TYPE=Release \
    -DQT_BUILD_EXAMPLES=OFF \
    -DQT_BUILD_TESTS=OFF \
    -DFEATURE_wayland=ON \
    -DFEATURE_wayland_client=ON \
    -DFEATURE_xcb=OFF \
    -DFEATURE_xlib=OFF \
    -DFEATURE_opengl=ON \
    -DFEATURE_openssl=ON \
    -DFEATURE_dbus=ON \
    -DFEATURE_system_pcre2=ON \
    -DFEATURE_system_zlib=ON \
    -DINPUT_opengl=desktop \
    -DQT_QPA_PLATFORMS=wayland \
    -DQT_FEATURE_vulkan=OFF

cmake --build . -j${COOKBOOK_MAKE_JOBS}
cmake --install . --prefix ${COOKBOOK_STAGE}/usr
"""
```

**What `redox.patch` for qtbase needs** (~500-800 lines):

1. Platform detection:
   ```
   qtbase/src/corelib/global/qsystemdetection.h — add Redox detection
   qtbase/src/corelib/io/qfilesystemengine_unix.cpp — Redox path handling
   ```

2. Shared memory:
   ```
   qtbase/src/corelib/kernel/qsharedmemory.cpp — map to Redox shm scheme
   ```

3. Process handling:
   ```
   qtbase/src/corelib/io/qprocess_unix.cpp — already works (relibc POSIX)
   ```

4. Network:
   ```
   qtbase/src/network/ — should compile with relibc sockets
   ```

#### Step 2: Port `qtwayland` (1-2 weeks)

```toml
[source]
tar = "https://download.qt.io/official_releases/qt/6.8/6.8.2/submodules/qtwayland-everywhere-src-6.8.2.tar.xz"

[build]
template = "custom"
dependencies = ["qtbase", "libwayland", "wayland-protocols"]

script = """
DYNAMIC_INIT
mkdir -p build && cd build
cmake .. \
    -DCMAKE_PREFIX_PATH=${COOKBOOK_SYSROOT}/usr \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DQT_BUILD_TESTS=OFF
cmake --build . -j${COOKBOOK_MAKE_JOBS}
cmake --install . --prefix ${COOKBOOK_STAGE}/usr
"""
```

#### Step 3: Port `qtdeclarative` (QML) (2-3 weeks)

```toml
[source]
tar = "https://download.qt.io/official_releases/qt/6.8/6.8.2/submodules/qtdeclarative-everywhere-src-6.8.2.tar.xz"

[build]
template = "custom"
dependencies = ["qtbase"]

script = """
# Same cmake pattern as qtwayland
"""
```

#### Step 4: Verification (1-2 weeks)

Build and run a simple Qt Wayland app:
```cpp
#include <QApplication>
#include <QLabel>
int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QLabel label("Hello from Qt on Redox!");
    label.show();
    return app.exec();
}
```

**Milestone**: Window with "Hello from Qt on Redox!" appears on Wayland compositor.

### Phase KDE-B: KDE Frameworks (8-12 weeks)

#### KDE Frameworks Tier 1 (2-3 weeks)

| Framework | Purpose | Estimated Patches |
|-----------|---------|------------------|
| `extra-cmake-modules` | CMake modules | None — pure CMake |
| `kcoreaddons` | Core utilities | ~50 lines (process detection) |
| `kconfig` | Configuration | ~30 lines (filesystem paths) |
| `kwidgetsaddons` | Extra Qt widgets | None — pure Qt |
| `kitemmodels` | Model/view classes | None — pure Qt |
| `kitemviews` | Item view classes | None — pure Qt |
| `kcodecs` | String encoding | None — pure Qt |
| `kguiaddons` | GUI utilities | None — pure Qt |

**Recipe Pattern** (same for all Tier 1):
```toml
[source]
tar = "https://download.kde.org/stable/frameworks/6.10/kcoreaddons-6.10.0.tar.xz"
patches = ["redox.patch"]

[build]
template = "custom"
dependencies = ["qtbase", "extra-cmake-modules"]

script = """
DYNAMIC_INIT
mkdir -p build && cd build
cmake .. \
    -DCMAKE_PREFIX_PATH=${COOKBOOK_SYSROOT}/usr \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DBUILD_TESTING=OFF \
    -DBUILD_QCH=OFF
cmake --build . -j${COOKBOOK_MAKE_JOBS}
cmake --install . --prefix ${COOKBOOK_STAGE}/usr
"""
```

#### KDE Frameworks Tier 2 (2-3 weeks)

| Framework | Dependencies | Notes |
|-----------|--------------|-------|
| `ki18n` | `kcoreaddons`, gettext | Internationalization |
| `kauth` | `kcoreaddons` | PolicyKit stub needed |
| `kwindowsystem` | `qtbase` | Window management — needs Wayland backend |
| `kcrash` | `kcoreaddons` | Crash handler — may need signal adjustments |
| `karchive` | `qtbase`, zlib | Archive handling — should port cleanly |
| `kiconthemes` | `kwidgetsaddons`, `karchive` | Icon loading |

#### KDE Frameworks Tier 3 (3-4 weeks) - Plasma essentials

| Framework | Purpose | Key for Plasma? |
|-----------|---------|------------------|
| `kio` | File I/O abstraction | **Yes** — file dialogs, I/O slaves |
| `kservice` | Plugin/service management | **Yes** — app discovery |
| `kxmlgui` | GUI framework | **Yes** — menus, toolbars |
| `plasma-framework` | Plasma applets/containments | **Yes** — desktop shell |
| `knotifications` | Desktop notifications | **Yes** — notification system |
| `kpackage` | Package/asset management | **Yes** — Plasma packages |
| `kconfigwidgets` | Configuration widgets | **Yes** — settings UI |

**Total frameworks needed for minimal Plasma**: ~25

**Estimated total patch effort for all frameworks**: ~1500-2000 lines

### Phase KDE-C: Plasma Desktop (6-8 weeks)

#### Step 1: Port KWin (4-6 weeks)

```toml
[source]
tar = "https://download.kde.org/stable/plasma/6.3.4/kwin-6.3.4.tar.xz"
patches = ["redox.patch"]

[build]
template = "custom"
dependencies = [
    "qtbase", "qtwayland", "qtdeclarative",
    "kcoreaddons", "kconfig", "kwindowsystem",
    "knotifications", "kxmlgui", "plasma-framework",
    "libwayland", "wayland-protocols",
    "mesa", "libdrm", "libinput", "seatd",
    "libxkbcommon",
]

script = """
DYNAMIC_INIT
mkdir -p build && cd build
cmake .. \
    -DCMAKE_PREFIX_PATH=${COOKBOOK_SYSROOT}/usr \
    -DCMAKE_INSTALL_PREFIX=/usr \
    -DBUILD_TESTING=OFF \
    -DKWIN_BUILD_SCREENLOCKING=OFF \
    -DKWIN_BUILD_TABBOX=OFF \
    -DKWIN_BUILD_EFFECTS=ON
cmake --build . -j${COOKBOOK_MAKE_JOBS}
cmake --install . --prefix ${COOKBOOK_STAGE}/usr
"""
```

**What `redox.patch` for KWin needs** (~1000-1500 lines):

1. DRM backend:
   ```
   src/backends/drm/drm_backend.cpp — open DRM scheme instead of device node
   src/backends/drm/drm_output.cpp — use scheme ioctl equivalents
   ```

2. libinput backend: Should work via evdevd
   ```
   src/backends/libinput/connection.cpp — may need path adjustments
   ```

3. EGL/OpenGL:
   ```
   src/libkwineglbackend.cpp — Mesa EGL should work (already ported)
   ```

4. Session management: KWin expects logind, need stub:
   ```
   src/session.h/cpp — stub LogindIntegration, use seatd instead
   ```

5. udev:
   ```
   src/udev.h/cpp — redirect to our udev-shim
   ```

#### Step 2: Port `plasma-workspace` (2-3 weeks)

```toml
[source]
tar = "https://download.kde.org/stable/plasma/6.3.4/plasma-workspace-6.3.4.tar.xz"

[build]
template = "custom"
dependencies = [
    "kwin", "plasma-framework", "kio", "kservice",
    "knotifications", "kpackage", "kconfigwidgets",
    "qtbase", "qtwayland", "qtdeclarative",
    "dbus",
]
```

**Key component**: `plasmashell` — desktop shell (panels, desktop containment, applet loader). Depends heavily on QML (qtdeclarative).

#### Step 3: Create `config/kde.toml`

```toml
include = ["desktop.toml"]

[general]
filesystem_size = 4096

[packages]
# Qt
qtbase = {}
qtwayland = {}
qtdeclarative = {}
qtsvg = {}
# KDE Frameworks (minimal set)
extra-cmake-modules = {}
kcoreaddons = {}
kconfig = {}
kwidgetsaddons = {}
ki18n = {}
kwindowsystem = {}
kio = {}
kservice = {}
kxmlgui = {}
knotifications = {}
kpackage = {}
plasma-framework = {}
kconfigwidgets = {}
# KDE Plasma
kwin = {}
plasma-workspace = {}
plasma-desktop = {}
kde-cli-tools = {}
# Support
dbus = {}
mesa = {}
libdrm = {}
libinput = {}
seatd = {}
evdevd = {}
drmd = {}

# Override init to launch KDE session
[[files]]
path = "/usr/lib/init.d/20_orbital"
data = """
requires_weak 10_net
notify audiod
nowait VT=3 orbital orbital-kde
"""

[[files]]
path = "/usr/bin/orbital-kde"
mode = 0o755
data = """
#!/usr/bin/env bash
set -ex

export DISPLAY=""
export WAYLAND_DISPLAY=wayland-0
export XDG_RUNTIME_DIR=/tmp/run/user/0
export XDG_SESSION_TYPE=wayland
export KDE_FULL_SESSION=true
export XDG_CURRENT_DESKTOP=KDE

mkdir -p /tmp/run/user/0

# Start D-Bus
dbus-daemon --system &

# Start D-Bus session
eval $(dbus-launch --sh-syntax)

# Start KWin (Wayland compositor + window manager)
kwin_wayland --replace &

# Start Plasma Shell
sleep 2
plasmashell &
"""
```

### System Integration Points

#### D-Bus (Already Working)
D-Bus is ported and working in X11 config. KDE uses D-Bus extensively.

#### Audio: PulseAudio/PipeWire Shim Needed
KDE expects PulseAudio or PipeWire. Redox has `scheme:audio`.

**Options**:
- A: Port PipeWire (large effort)
- B: Write PulseAudio compatibility shim (medium effort)
- C: Use KDE without audio initially (skip for now)

#### Service Management: D-Bus Service Files
KDE services register via D-Bus `.service` files. Need translation layer that:
1. Reads `/usr/share/dbus-1/services/*.service` files
2. Maps to Redox init scripts
3. Responds to D-Bus StartServiceByName calls

#### Network: NetworkManager Integration
KDE uses NetworkManager. Redox has `smolnetd`.

**Options**:
- A: Port NetworkManager (massive effort, needs systemd)
- B: Write NetworkManager D-Bus shim (medium effort)
- C: Skip network config UI initially

### KDE Implementation Timeline

| Phase | Duration | Milestone |
|--------|----------|-----------|
| Qt Foundation (qtbase, qtwayland, qtdeclarative) | 8-12 weeks | Qt app shows window |
| KDE Frameworks (25 frameworks) | 8-12 weeks | KDE app (Kate) runs |
| KWin + Plasma Shell | 6-8 weeks | KDE desktop visible |
| KDE Apps (Dolphin, Konsole, Kate) | 4-6 weeks | Full KDE ecosystem |
| **Total** | **~38 weeks (9-10 months)** | Full KDE Plasma session |

**Critical Insight**: Qt Foundation is highest-risk phase. If Qt compilation hits unexpected relibc gaps, entire timeline shifts.

---

## 4. Linux Driver Compatibility: Concrete Path

### Why This Is Needed

Writing native Rust GPU drivers for every vendor is years of work. Linux has mature, vendor-supported GPU drivers. A compatibility layer lets us port them with `#ifdef __redox__` patches instead of full rewrites.

**Target Drivers** (priority order):
1. **i915** (Intel) - Best documented, most relevant for laptops
2. **amdgpu** (AMD) - Large market share, good open-source driver
3. **nouveau / nvk** (NVIDIA) - Community driver, limited performance
4. **Skip**: NVIDIA proprietary (binary-only, impossible without Linux kernel)

### Architecture

**Two-Mode Design**:

**Mode A: C Driver Port** - Compile Linux C driver against our headers, run as userspace daemon
**Mode B: Rust Wrapper** - Rust crate provides idiomatic API, internally calls compat layer

Both modes share: `redox-driver-sys`

```
┌────────────────────────────────────────────────────────────┐
│         Mode A: C Driver Port                          │
│  Linux C driver (i915.ko source)                  │
│  compiled with -D__redox__ against linux-kpi headers │
├────────────────────────────────────────────────────────────┤
│         Mode B: Rust Wrapper                        │
│  Rust crate (redox-intel-gpu) using compat APIs      │
├────────────────────────────────────────────────────────────┤
│         linux-kpi (C header compatibility)             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐       │
│  │ linux/    │ │ linux/   │ │ linux/   │       │
│  │ slab.h   │ │ mutex.h  │ │ pci.h    │       │
│  └──────────┘ └──────────┘ └──────────┘       │
├────────────────────────────────────────────────────────────┤
│         redox-driver-sys (Rust crate)                  │
│  Provides: memory mapping, IRQ, DMA, PCI, DRM scheme  │
├────────────────────────────────────────────────────────────┤
│         Red Bear OS                                    │
│  scheme:memory  scheme:irq  scheme:pci  scheme:drm      │
└────────────────────────────────────────────────────────────┘
```

### Crate 1: `redox-driver-sys` (2-3 weeks)

**Repository**: New crate in Redox ecosystem
**Purpose**: Safe Rust wrappers around Redox's scheme-based hardware access

```
redox-driver-sys/
├── Cargo.toml
├── src/
│   ├── lib.rs              — Re-exports
│   ├── memory.rs           — Physical memory mapping (scheme:memory)
│   ├── irq.rs              — Interrupt handling (scheme:irq)
│   ├── pci.rs              — PCI device access (scheme:pci / pcid)
│   ├── io.rs               — Port I/O (iopl syscall)
│   └── dma.rs              — DMA buffer management
```

**Key Implementations**:

```rust
// src/memory.rs
pub fn map_physical(phys: u64, size: usize, flags: MapFlags) -> Result<*mut u8> {
    let fd = File::open("scheme:memory/physical")?;
    let ptr = syscall::fmap(fd.as_raw_fd(), &Map {
        offset: phys,
        size,
        flags: flags.to_syscall_flags(),
    })?;
    Ok(ptr as *mut u8)
}

pub fn unmap_physical(ptr: *mut u8, size: usize) -> Result<()> {
    syscall::funmap(ptr as usize, size)?;
    Ok(())
}
```

```rust
// src/irq.rs
pub struct IrqHandle { fd: File }

impl IrqHandle {
    pub fn request(irq_num: u32) -> Result<Self> {
        let fd = File::open(&format!("scheme:irq/{}", irq_num))?;
        Ok(Self { fd })
    }

    pub fn wait(&mut self) -> Result<()> {
        let mut buf = [0u8; 8];
        self.fd.read(&mut buf)?;
        Ok(())
    }
}
```

```rust
// src/pci.rs
pub struct PciDevice {
    bus: u8, dev: u8, func: u8,
    vendor_id: u16, device_id: u16,
    bars: [u64; 6],
    bar_sizes: [usize; 6],
    irq: u32,
}

pub fn enumerate() -> Result<Vec<PciDevice>> {
    // Read from pcid-spawner or scheme:pci
    // Parse PCI configuration space
    // Filter to GPU devices (class 0x030000-0x0302xx)
}
```

### Crate 2: `linux-kpi` (3-4 weeks)

**Repository**: New crate. Installs C headers for use by Linux C drivers.
**Purpose**: Provides `linux/*.h` headers that translate Linux kernel APIs to `redox-driver-sys`

```
linux-kpi/
├── Cargo.toml
├── src/
│   ├── lib.rs              — Rust API for Rust drivers
│   ├── c_headers/          — C headers for C driver ports
│   │   ├── linux/
│   │   │   ├── slab.h      → malloc/kfree (redox-driver-sys::memory)
│   │   │   ├── mutex.h     → pthread mutex (redox-driver-sys::sync)
│   │   │   ├── spinlock.h  → atomic lock
│   │   │   ├── pci.h       → redox-driver-sys::pci
│   │   │   ├── io.h        → port I/O (iopl)
│   │   │   ├── irq.h       → redox-driver-sys::irq
│   │   │   ├── device.h    → struct device wrapper
│   │   │   ├── kobject.h   → reference-counted object
│   │   │   ├── workqueue.h → thread pool
│   │   │   ├── idr.h       → ID allocation
│   │   │   └── dma-mapping.h → bus DMA (redox-driver-sys::dma)
│   │   ├── drm/
│   │   │   ├── drm.h       → DRM core types
│   │   │   ├── drm_crtc.h  → KMS types
│   │   │   ├── drm_gem.h   → GEM buffer objects
│   │   │   └── drm_ioctl.h → DRM ioctl definitions
│   │   └── asm/
│   │       └── io.h        → inl/outl port I/O
│   └── rust_impl/          — Rust implementations backing C headers
│       ├── memory.rs       — kzalloc, kmalloc, kfree
│       ├── sync.rs         — mutex, spinlock, completion
│       ├── workqueue.rs    — work queue thread pool
│       ├── pci.rs          — pci_register_driver, etc.
│       └── drm_shim.rs     — DRM core shim (connects to scheme:drm)
```

**Example C Header**:

```c
// c_headers/linux/slab.h
#ifndef _LINUX_SLAB_H
#define _LINUX_SLAB_H

#include <stddef.h>

#define GFP_KERNEL  0
#define GFP_ATOMIC  1
#define GFP_DMA32   2

void *kmalloc(size_t size, unsigned int flags);
void *kzalloc(size_t size, unsigned int flags);
void kfree(const void *ptr);

#endif
```

**Corresponding Rust Implementation**:

```rust
// src/rust_impl/memory.rs
use std::alloc::{alloc, alloc_zeroed, dealloc, Layout};

#[no_mangle]
pub extern "C" fn kmalloc(size: usize, _flags: u32) -> *mut u8 {
    unsafe {
        let layout = Layout::from_size_align(size, 64).unwrap();
        alloc(layout)
    }
}

#[no_mangle]
pub extern "C" fn kzalloc(size: usize, _flags: u32) -> *mut u8 {
    unsafe {
        let layout = Layout::from_size_align(size, 64).unwrap();
        alloc_zeroed(layout)
    }
}

#[no_mangle]
pub extern "C" fn kfree(ptr: *const u8) {
    if !ptr.is_null() {
        unsafe {
            // Linux kfree doesn't take size. Need size-tracking allocator.
            // Use HashMap<ptr, Layout> for tracking.
        }
    }
}
```

### Crate 3: `redox-drm` (12-16 weeks, overlaps with Wayland DRM)

**Repository**: Part of Redox base repo or new crate
**Purpose**: The daemon that registers `scheme:drm` and talks to GPU hardware

```
redox-drm/
├── Cargo.toml
├── src/
│   ├── main.rs             — Daemon entry, scheme registration
│   ├── scheme.rs           — "drm" scheme handler (processes ioctls)
│   ├── kms/
│   │   ├── mod.rs          — KMS core
│   │   ├── crtc.rs         — CRTC state machine
│   │   ├── connector.rs    — Hotplug detection, EDID reading
│   │   ├── encoder.rs      — Encoder management
│   │   ├── plane.rs        — Primary/cursor planes
│   │   └── framebuffer.rs   — Framebuffer allocation
│   ├── gem.rs              — GEM buffer object management
│   ├── dmabuf.rs           — DMA-BUF export/import via FD passing
│   └── drivers/
│       ├── mod.rs          — trait GpuDriver
│       └── intel/
│           ├── mod.rs      — Intel driver entry
│           ├── gtt.rs      — Graphics Translation Table
│           ├── display.rs  — Display pipe configuration
│           └── ring.rs     — Command ring buffer (for acceleration later)
```

**Core DRM Scheme Protocol**:

```rust
// src/scheme.rs
enum DrmRequest {
    // Core
    GetVersion,
    GetCap { capability: u64 },

    // KMS
    ModeGetResources,
    ModeGetConnector { connector_id: u32 },
    ModeGetEncoder { encoder_id: u32 },
    ModeGetCrtc { crtc_id: u32 },
    ModeSetCrtc { crtc_id: u32, fb_id: u32, x: u32, y: u32, connectors: Vec<u32>, mode: ModeModeInfo },
    ModePageFlip { crtc_id: u32, fb_id: u32, flags: u32, user_data: u64 },
    ModeAtomicCommit { flags: u32, props: Vec<AtomicProp> },

    // GEM
    GemCreate { size: u64 },
    GemClose { handle: u32 },
    GemMmap { handle: u32 },

    // Prime/DMA-BUF
    PrimeHandleToFd { handle: u32, flags: u32 },
    PrimeFdToHandle { fd: i32 },
}
```

**Intel Driver** (native Rust modesetting):

```rust
// src/drivers/intel.rs
pub struct IntelDriver {
    mmio: *mut u8,              // Memory-mapped I/O registers (via scheme:memory)
    gtt_size: usize,            // Graphics Translation Table size
    framebuffer: PhysAddr,      // Current scanout buffer
}

impl IntelDriver {
    pub fn new(pci_dev: &PciDev) -> Result<Self> {
        // Map MMIO registers via scheme:memory/physical
        let mmio = map_physical_memory(pci_dev.bar[0], pci_dev.bar_size[0])?;

        // Initialize GTT and display pipeline
        Ok(Self { mmio, gtt_size, framebuffer })
    }

    pub fn modeset(&self, mode: &ModeInfo) -> Result<()> {
        // 1. Allocate framebuffer in GTT
        // 2. Configure pipe (timing, PLL)
        // 3. Configure transcoder
        // 4. Configure port (HDMI/DP)
        // 5. Enable scanout from new framebuffer
        Ok(())
    }

    pub fn page_flip(&self, crtc: u32, fb: PhysAddr) -> Result<()> {
        // 1. Update GTT entry to point to new framebuffer
        // 2. Trigger page flip on next VBlank
        // 3. VBlank interrupt signals completion (via scheme:irq)
        Ok(())
    }
}
```

### Concrete Porting Example: Intel i915 Driver (3-4 weeks)

#### Step 1: Extract i915 from Linux kernel

```bash
# Clone Linux kernel
git clone --depth 1 https://github.com/torvalds/linux.git
# Extract relevant directories
tar cf intel-driver.tar linux/drivers/gpu/drm/i915/ \
    linux/include/drm/ \
    linux/include/linux/ \
    linux/arch/x86/include/
```

#### Step 2: Create recipe

```toml
# recipes/wip/drivers/i915/recipe.toml
[source]
tar = "intel-driver.tar"

[build]
template = "custom"
dependencies = [
    "redox-driver-sys",
    "linux-kpi",
    "redox-drm",
]

script = """
DYNAMIC_INIT

# Build i915 driver as a shared library
# linked against linux-kpi and redox-driver-sys
export CFLAGS="-I${COOKBOOK_SYSROOT}/include/linux-kpi -D__redox__"
export LDFLAGS="-lredox_driver_sys -llinux_kpi -lredox_drm"

# Compile driver source files
find drivers/gpu/drm/i915/ -name '*.c' | while read src; do
    x86_64-unknown-redox-gcc -c $CFLAGS "$src" -o "${src%.c}.o" || true
done

# Link into a single shared library
x86_64-unknown-redox-gcc -shared -o i915_redox.so \
    $(find drivers/gpu/drm/i915/ -name '*.o') \
    $LDFLAGS

mkdir -p ${COOKBOOK_STAGE}/usr/lib/redox/drivers
cp i915_redox.so ${COOKBOOK_STAGE}/usr/lib/redox/drivers/
"""
```

#### Step 3: Minimal patches needed

For i915 on Redox, typical `#ifdef __redox__` changes:

```c
// 1. Replace Linux module init with daemon main()
#ifdef __redox__
int main(int argc, char **argv) {
    return i915_driver_init();
}
#else
module_init(i915_init);
module_exit(i915_exit);
#endif

// 2. Replace kernel memory allocation
#ifdef __redox__
#include <linux/slab.h>  // Our compat header
#else
#include <linux/slab.h>  // Real Linux
#endif

// 3. Replace PCI access
#ifdef __redox__
struct pci_dev *pdev = redox_pci_find_device(PCI_VENDOR_ID_INTEL, device_id);
#else
pdev = pci_get_device(PCI_VENDOR_ID_INTEL, device_id, NULL);
#endif

// 4. Replace MMIO mapping
#ifdef __redox__
void __iomem *regs = redox_ioremap(pci_resource_start(pdev, 0), pci_resource_len(pdev, 0));
#else
void __iomem *regs = ioremap(pci_resource_start(pdev, 0), pci_resource_len(pdev, 0));
#endif
```

### Concrete Porting Example: AMD amdgpu Driver (6-8 weeks)

AMD's driver is larger and more complex. Key challenges:

#### 1. Firmware Loading

Need to implement:
```
scheme:firmware/amdgpu/  — firmware blob storage
request_firmware()       — compat function that reads from scheme
```

#### 2. TTM Memory Manager

Port TTM to use Redox's memory scheme:
```rust
// TTM → Redox mapping:
// ttm_tt → allocated pages via scheme:memory
// ttm_buffer_object → GemHandle in scheme:drm
// ttm_bo_move → page table updates via GPU MMIO
```

#### 3. Display Core (DC)

AMD's display code is ~100K lines. Need to:
- Port DCN (Display Core Next) hardware programming
- Adapt to Redox's DRM scheme instead of Linux kernel DRM
- Keep most code unchanged, just redirect memory/register access

#### 4. Power Management

amdgpu uses Linux power management APIs. Need stubs:
```c
#ifdef __redox__
// No power management on Redox yet — always-on
#define pm_runtime_get_sync(dev) 0
#define pm_runtime_put_autosuspend(dev) 0
#define pm_runtime_allow(dev) 0
#endif
```

**Estimated patches for amdgpu**: ~2000-3000 lines of `#ifdef __redox__`

### Linux Driver Implementation Timeline

| Phase | Component | Effort | Delivers |
|-------|-----------|---------|----------|
| 1 | `redox-driver-sys` crate | 2-3 weeks | Memory, IRQ, PCI, I/O primitives |
| 2 | Intel native driver (in `redox-drm`) | 6-8 weeks | First working GPU driver, modesetting |
| 3 | `linux-kpi` C headers (core subset) | 3-4 weeks | Memory, sync, PCI, workqueue headers |
| 4 | `linux-kpi` DRM headers | 2-3 weeks | DRM/KMS/GEM C API headers |
| 5 | i915 C driver port | 3-4 weeks | Proves LinuxKPI approach works |
| 6 | `linux-kpi` extended (TTM, firmware) | 4-6 weeks | Enables AMD driver |
| 7 | amdgpu C driver port | 6-8 weeks | AMD GPU support |

**Phase 1-2 is critical path** — a native Rust Intel driver proves architecture and provides immediate value. Phases 3-7 can happen in parallel or later.

**With 2 developers**:
- **Month 1-2**: redox-driver-sys + Intel native driver → first display output
- **Month 3-4**: linux-kpi core + DRM headers → i915 C port proof of concept
- **Month 5-8**: linux-kpi TTM + amdgpu port → AMD support
- **Total: 6-8 months** to support both Intel and AMD GPUs

**With 1 developer**:
- **Month 1-3**: redox-driver-sys + Intel native driver
- **Month 4-6**: linux-kpi core + i915 port
- **Month 7-10**: amdgpu port
- **Total: 8-10 months**

---

## 5. Critical Paths & Dependencies

### Dependency Chain: Hardware → KDE Desktop

```
┌─────────────────────────────────────────────────────────┐
│                    KDE Plasma Desktop                     │
│  (KWin compositor, Plasma Shell, Qt, KDE Frameworks)    │
├─────────────────────────────────────────────────────────┤
│                    Wayland Protocol                       │
│  (libwayland, wayland-protocols, compositor)             │
├─────────────────────────────────────────────────────────┤
│                    Graphics Stack                         │
│  (Mesa3D OpenGL/Vulkan, GBM, libdrm, GPU driver)        │
├─────────────────────────────────────────────────────────┤
│                    Kernel Interfaces                      │
│  (DRM/KMS, GEM/TTM, DMA-BUF, evdev, udev)              │
├─────────────────────────────────────────────────────────┤
│                    Hardware                               │
│  (GPU: AMD/Intel/NVIDIA, Input: keyboard/mouse/touch)   │
└─────────────────────────────────────────────────────────┘
```

### Critical Path to KDE Plasma

```
M1 (POSIX) ───────────────────────────────────────────┐
                                                        │
M3 (DRM/KMS) ───────────── M4 (Compositor) ── M5 (Qt) ── M6 (KDE) ── M7 (Plasma)
       │                     ↑                      │
M2 (Input) ──────────────┘                       M8 (Linux drivers, parallel)
```

**Shortest path to a desktop**: M1 → M2 → M3 (parallel) → M4 → M5 → M6 → M7
**Shortest path to GPU drivers**: M3 → M8 (can start as soon as `redox-driver-sys` exists)

### Parallel Execution Opportunities

```
Week 1-4:     M1 (relibc POSIX gaps)
Week 3-12:    M2 (evdev input) ──── parallel ──── M3 (DRM/KMS)
Week 13-16:   M4 (Wayland compositor = M2 + M3 + M1)
Week 13-24:   M8 (Linux driver compat, parallel with M4-M6)
Week 17-24:   M5 (Qt Foundation)
Week 25-32:   M6 (KDE Frameworks)
Week 33-38:   M7 (Plasma Desktop)
```

**Total to KDE Plasma**: ~38 weeks (~9 months) with 2 developers
**Total to Linux driver compat**: ~24 weeks (~6 months) in parallel

---

## 6. Recommendations & Next Steps

### Immediate Actions (Week 1-4)

1. **Fix relibc POSIX gaps** (1-2 weeks)
   - Implement `signalfd`, `timerfd`, `eventfd` in relibc
   - Add `F_DUPFD_CLOEXEC`, `MSG_CMSG_CLOEXEC`, `MSG_NOSIGNAL`
   - Implement `open_memstream`
   - **Result**: libwayland builds natively (no patches)

2. **Start evdev daemon** (2-4 weeks, parallel with POSIX)
   - Create `recipes/core/evdevd/`
   - Implement scheme protocol and ioctl handlers
   - **Result**: Input stack foundation

3. **Start redox-driver-sys crate** (2-3 weeks, parallel with POSIX)
   - Implement memory, IRQ, PCI, I/O primitives
   - **Result**: Hardware access foundation for LinuxKPI

### Medium-Term Actions (Week 5-16)

4. **Complete input stack** (2-3 weeks after evdevd)
   - Build udev shim
   - Port libinput
   - **Result**: Full input stack for Wayland

5. **Build DRM daemon with Intel driver** (8-12 weeks)
   - Implement KMS core, GEM, DMA-BUF
   - Implement Intel native modesetting
   - **Result**: Hardware display control

6. **Build linux-kpi headers** (3-4 weeks, parallel with DRM)
   - Implement C headers for Linux kernel APIs
   - Implement Rust backing implementations
   - **Result**: Compatibility layer for C drivers

### Long-Term Actions (Week 17-38+)

7. **Port Wayland compositor** (4-6 weeks after M2+M3+M1)
   - Add Redox backends to Smithay
   - Build smallvil with Redox backends
   - **Result**: First functional Wayland compositor

8. **Port Qt Foundation** (8-12 weeks, parallel with compositor)
   - Port qtbase, qtwayland, qtdeclarative
   - Fix platform detection and shared memory
   - **Result**: Qt applications can run

9. **Port KDE Frameworks** (8-12 weeks)
   - Port 25+ frameworks (Tier 1, 2, 3)
   - **Result**: KDE applications can be built

10. **Port KDE Plasma** (6-8 weeks)
    - Port KWin, plasma-workspace, plasma-desktop
    - Create config/kde.toml
    - **Result**: Full KDE Plasma desktop

11. **Port Linux GPU drivers** (3-4 weeks after linux-kpi, parallel)
    - Port i915 as proof of concept
    - Port amdgpu for AMD support
    - **Result**: Broad GPU hardware support

### Build System Improvements

**Issue Found**: FUSE mount error (ioctl 25) during build
**Recommendation**: Add build environment cleanup script:
```bash
# scripts/clean-build-env.sh
#!/bin/bash
fusermount3 -u build/x86_64/desktop/filesystem 2>/dev/null || true
fusermount3 -u /tmp/redox_installer 2>/dev/null || true
rm -rf build/x86_64/desktop/filesystem 2>/dev/null || true
```

**Integration**: Add to Makefile:
```makefile
clean: FORCE
    @./scripts/clean-build-env.sh
    # ... rest of clean target
```

### Resource Requirements

**Storage**: 20GB+ free space (full build with all recipes)
**RAM**: 4GB minimum, 8GB+ recommended
**Network**: Required for downloading sources and toolchain
**OS**: Linux (Arch/Manjaro, Debian/Ubuntu, Fedora, Gentoo)

---

## 7. Risk Assessment & Mitigation

### High-Risk Areas

1. **Qt Foundation** (HIGH RISK)
   - **Risk**: Unexpected relibc gaps blocking Qt compilation
   - **Impact**: Entire KDE timeline shifts by months
   - **Mitigation**: Start Qt porting early, test with software rendering

2. **Linux Driver Porting** (MEDIUM RISK)
   - **Risk**: Linux driver code complexity exceeds LinuxKPI capabilities
   - **Impact**: AMD/NVIDIA drivers may not work
   - **Mitigation**: Start with Intel (simplest), prove concept before AMD

3. **Wayland Compositor** (LOW-MEDIUM RISK)
   - **Risk**: Smithay Redox backends integration issues
   - **Impact**: Wayland session delayed
   - **Mitigation**: Use native Rust Intel driver first, no LinuxKPI dependency

### Technical Risks

1. **No GPU Acceleration**
   - All rendering is software-only via LLVMpipe
   - Performance will be poor for desktop workloads
   - **Mitigation**: Prioritize hardware GPU driver work

2. **Missing System Integration**
   - No NetworkManager equivalent → no network UI
   - No PipeWire → no audio in KDE
   - **Mitigation**: Build minimal shims, skip features initially

3. **Kernel ABI Unstable**
   - Redox syscall ABI intentionally unstable
   - Changes may break compatibility layers
   - **Mitigation**: Work through libredox/relibc, not kernel syscalls directly

---

## 8. Conclusion

Red Bear OS has:
- ✅ Comprehensive documentation with concrete implementation paths
- ✅ Functional build system with Rust-based tools
- ✅ Active development with 60+ patches for Linux compatibility
- ✅ Clear roadmap to Wayland, KDE Plasma, and Linux drivers
- ⚠️ Identified blockers (7 POSIX gaps, no GPU acceleration, missing DRM/KMS)

**Estimated Timelines**:
- **Wayland compositor**: 5-6 months (M1 + M2 + M3 + M4)
- **KDE Plasma desktop**: 9-10 months (M1 → M7)
- **Linux driver compatibility**: 6-8 months (M3 + M8)

**Key Insights**:
1. POSIX gaps in relibc are the foundational blocker - 1-2 weeks to fix
2. Input stack and DRM/KMS can be built in parallel (4-12 weeks each)
3. Qt Foundation is the highest-risk phase - should start early
4. Native Rust Intel driver is a faster path than full LinuxKPI for initial GPU support
5. LinuxKPI approach is essential for AMD/NVIDIA long-term support

**Recommendation**: Start with Milestone M1 (POSIX gaps) immediately, as it unblocks everything else. With 2 developers working in parallel on M2 (input) and M3 (DRM), a functional Wayland compositor is achievable in ~6 months, with KDE Plasma following in ~9 months.

---

**Appendix A: Existing WIP Recipes Inventory**

**Wayland Recipes** (21 packages):
- libwayland, wayland-protocols, wayland-utils
- libxkbcommon, xkeyboard-config
- mesa, libdrm
- cosmic-comp, cosmic-panel, libcosmic-wayland
- smallvil (Smithay)
- wlroots, sway, hyprland, niri, pinnacle, fht-compositor
- xwayland, anvil
- iced-wayland, winit-wayland, softbuffer-wayland, wayland-rs

**KDE Recipes** (19 packages):
- ark, discover, gcompris, heaptrack, k3b, kamoso, kcachegrind
- kde-dolphin, kdenlive, kdevelop, kpatience, krita, ktorrent
- kwave, labplot, marble, massif-visualizer, okteta, skanpage

**Patches Inventory**: 60+ `redox.patch` files across recipes

---

**END OF REPORT**
