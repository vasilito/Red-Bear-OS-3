# 05 — KDE Plasma on Redox: Concrete Implementation Path

## Goal

Run KDE Plasma 6 desktop environment on Redox OS, starting with a minimal viable
desktop and expanding to full Plasma.

## Prerequisites (from docs 03 and 04)

Before KDE work begins, these MUST be complete:
- [x] relibc POSIX gaps fixed (signalfd, timerfd, eventfd, etc.)
- [x] evdevd + libinput working
- [x] DRM/KMS scheme working (at least Intel modesetting)
- [x] Wayland compositor running (Smithay/smallvil)
- [x] Mesa EGL + software OpenGL (already ported)

## Three-Phase KDE Implementation

### Phase KDE-A: Qt Foundation (2-3 months)

**Goal**: A Qt application displays a window on the Redox Wayland compositor.

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
    "mesa",         # EGL + OpenGL
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

# Qt 6 uses CMake
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

**What `redox.patch` for qtbase needs to fix**:

1. **Platform detection**: Add `__redox__` as a POSIX-like platform
   ```
   qtbase/src/corelib/global/qsystemdetection.h  — add Redox detection
   qtbase/src/corelib/io/qfilesystemengine_unix.cpp — Redox path handling
   ```

2. **Shared memory**: Qt uses `shm_open()` for Wayland buffers
   ```
   qtbase/src/corelib/kernel/qsharedmemory.cpp — map to Redox shm scheme
   ```

3. **Process handling**: `fork`/`exec` differences
   ```
   qtbase/src/corelib/io/qprocess_unix.cpp — already works (relibc POSIX)
   ```

4. **Network**: Qt uses BSD sockets — already work via relibc
   ```
   qtbase/src/network/ — should compile with relibc sockets
   ```

**Estimated patch size**: ~500-800 lines for qtbase.

#### Step 2: Port `qtwayland` (1-2 weeks)

```toml
# recipes/wip/qt/qtwayland/recipe.toml
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
# recipes/wip/qt/qtdeclarative/recipe.toml
[source]
tar = "https://download.qt.io/official_releases/qt/6.8/6.8.2/submodules/qtdeclarative-everywhere-src-6.8.2.tar.xz"

[build]
template = "custom"
dependencies = ["qtbase"]

script = """
# Same cmake pattern as qtwayland
"""
```

#### Step 4: Verify

```bash
# Build and run a simple Qt Wayland app:
cat > test.cpp << 'EOF'
#include <QApplication>
#include <QLabel>
int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    QLabel label("Hello from Qt on Redox!");
    label.show();
    return app.exec();
}
EOF

x86_64-unknown-redox-g++ test.cpp -o test-qt -I/usr/include/qt6 -lQt6Widgets -lQt6Gui -lQt6Core
# Run on compositor: WAYLAND_DISPLAY=wayland-0 ./test-qt
```

**Milestone**: Window with "Hello from Qt on Redox!" appears on Wayland compositor.

---

### Phase KDE-B: KDE Frameworks (2-3 months)

**Goal**: KDE applications can be built and run.

#### KDE Frameworks Tier 1 (2-3 weeks)

These have minimal dependencies — just Qt and CMake.

| Framework | Purpose | Estimated Patches |
|---|---|---|
| `extra-cmake-modules` | CMake modules for KDE | None — pure CMake |
| `kcoreaddons` | Core utilities | ~50 lines (process detection) |
| `kconfig` | Configuration system | ~30 lines (filesystem paths) |
| `kwidgetsaddons` | Extra Qt widgets | None — pure Qt |
| `kitemmodels` | Model/view classes | None — pure Qt |
| `kitemviews` | Item view classes | None — pure Qt |
| `kcodecs` | String encoding | None — pure Qt |
| `kguiaddons` | GUI utilities | None — pure Qt |

**Recipe pattern** (same for all Tier 1):
```toml
# recipes/wip/kde/kcoreaddons/recipe.toml
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
|---|---|---|
| `ki18n` | `kcoreaddons`, gettext | Internationalization |
| `kauth` | `kcoreaddons` | PolicyKit stub needed |
| `kwindowsystem` | `qtbase` | Window management — needs Wayland backend |
| `kcrash` | `kcoreaddons` | Crash handler — may need signal adjustments |
| `karchive` | `qtbase`, zlib | Archive handling — should port cleanly |
| `kiconthemes` | `kwidgetsaddons`, `karchive` | Icon loading |

#### KDE Frameworks Tier 3 (3-4 weeks) — Plasma essentials only

| Framework | Purpose | Key for Plasma? |
|---|---|---|
| `kio` | File I/O abstraction | **Yes** — file dialogs, I/O slaves |
| `kservice` | Plugin/service management | **Yes** — app discovery |
| `kxmlgui` | GUI framework | **Yes** — menus, toolbars |
| `plasma-framework` | Plasma applets/containments | **Yes** — the desktop shell |
| `knotifications` | Desktop notifications | **Yes** — notification system |
| `kpackage` | Package/asset management | **Yes** — Plasma packages |
| `kconfigwidgets` | Configuration widgets | **Yes** — settings UI |
| `ktextwidgets` | Text editing widgets | Nice-to-have |
| `kbookmarks` | Bookmark management | Nice-to-have |

**Total frameworks needed for minimal Plasma: ~25**

**Estimated total patch effort for all frameworks: ~1500-2000 lines**

---

### Phase KDE-C: Plasma Desktop (2-3 months)

**Goal**: Full KDE Plasma desktop session.

#### Step 1: Port KWin (4-6 weeks)

KWin is the hardest component. It needs:
- DRM/KMS (for display control) → via our DRM scheme
- libinput (for input) → via our evdevd
- OpenGL ES 2.0+ (for effects) → via Mesa
- Wayland (for compositor protocol) → via libwayland

```toml
# recipes/wip/kde/kwin/recipe.toml
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

**What `redox.patch` for KWin needs to fix**:

1. **DRM backend**: Replace `/dev/dri/card0` with `scheme:drm/card0`
   ```
   src/backends/drm/drm_backend.cpp — open DRM scheme instead of device node
   src/backends/drm/drm_output.cpp — use scheme ioctl equivalents
   ```

2. **libinput backend**: Should work via evdevd if `/dev/input/eventX` exists
   ```
   src/backends/libinput/connection.cpp — may need path adjustments
   ```

3. **EGL/OpenGL**: KWin uses EGL + OpenGL ES
   ```
   src/libkwineglbackend.cpp — Mesa EGL should work (already ported)
   ```

4. **Session management**: KWin expects logind. Need to stub or implement:
   ```
   src/session.h/cpp — stub LogindIntegration, use seatd instead
   ```

5. **udev**: KWin uses udev for device enumeration
   ```
   src/udev.h/cpp — redirect to our udev-shim
   ```

**Estimated KWin patches**: ~1000-1500 lines.

#### Step 2: Port `plasma-workspace` (2-3 weeks)

```toml
# recipes/wip/kde/plasma-workspace/recipe.toml
[source]
tar = "https://download.kde.org/stable/plasma/6.3.4/plasma-workspace-6.3.4.tar.xz"

[build]
template = "custom"
dependencies = [
    # All KDE Frameworks above + kwin
    "kwin", "plasma-framework", "kio", "kservice", "knotifications",
    "kpackage", "kconfigwidgets",
    "qtbase", "qtwayland", "qtdeclarative",
    # System services
    "dbus",
]
```

**Key component**: `plasmashell` — the desktop shell. Creates panels, desktop containment,
applet loader. Depends heavily on QML (qtdeclarative).

#### Step 3: Port `plasma-desktop` (1-2 weeks)

System settings, desktop containment configuration. Mostly Qt/QML.

#### Step 4: Create session config

```toml
# config/kde.toml (new file)
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

---

## KDE Applications (Build on 19 WIP Recipes)

These are already partially ported in `recipes/wip/kde/`:

| App | Status | Notes |
|-----|--------|-------|
| kde-dolphin | WIP recipe exists | File manager — needs kio |
| kdenlive | WIP recipe exists | Video editor — needs MLT framework |
| krita | WIP recipe exists | Painting — needs Qt + OpenGL |
| kdevelop | WIP recipe exists | IDE — needs Qt + kio |
| okteta | WIP recipe exists | Hex editor |
| ktorrent | WIP recipe exists | BitTorrent client |
| ark | WIP recipe exists | Archive manager |
| kamoso | WIP recipe exists | Camera — needs PipeWire |
| kpatience | WIP recipe exists | Card game |

Once Qt + KDE Frameworks are ported, these apps should compile with minimal patches.

---

## System Integration Points

### D-Bus (Already Working)
D-Bus is ported and working in the X11 config. KDE uses D-Bus extensively.
Already configured in `config/x11.toml`.

### Audio: PulseAudio PipeWire Shim Needed
KDE expects PulseAudio or PipeWire for audio. Redox has its own `scheme:audio`.

**Option A**: Port PipeWire to Redox (large effort)
**Option B**: Write a PulseAudio compatibility shim that translates to Redox audio scheme
**Option C**: Use KDE without audio initially (just disable audio notifications)

### Service Management: D-Bus Service Files
KDE services register via D-Bus `.service` files. Redox init starts services.
Need a translation layer that:
1. Reads `/usr/share/dbus-1/services/*.service` files
2. Maps to Redox init scripts
3. Responds to D-Bus StartServiceByName calls

### Network: KDE NetworkManager integration
KDE uses NetworkManager for network configuration. Redox has `smolnetd`.

**Option A**: Port NetworkManager (massive effort, needs systemd)
**Option B**: Write a NetworkManager D-Bus shim that talks to smolnetd
**Option C**: Skip network configuration UI initially

---

## Timeline

| Phase | Duration | Milestone |
|-------|----------|-----------|
| Qt Foundation | 8-12 weeks | Qt app shows a window |
| KDE Frameworks | 8-12 weeks | KDE app (kate) runs |
| KWin + Plasma Shell | 6-8 weeks | KDE desktop visible |
| KDE Apps | 4-6 weeks | Dolphin, Konsole, Kate working |
| **Total** | **10-15 months** | Full KDE Plasma session |

**Critical insight**: The Qt Foundation phase is the highest-risk phase.
If Qt compilation hits unexpected relibc gaps, the entire KDE timeline shifts.
Mitigation: start Qt porting early, even before DRM/input is complete,
using software rendering and Orbital backend as a test environment.
