# Qt6 Port — Red Bear OS

**Last updated:** 2026-04-16
**Qt version:** 6.11.0
**Target:** x86_64-unknown-redox (cross-compiled from Linux x86_64 host)

> **Phase numbering note:** The phases below (Phase 1–6) are this document's internal Qt porting
> phases, not the canonical desktop plan phases. For the project-wide desktop execution plan
> (Phase 1: Runtime Substrate → Phase 5: Hardware GPU), see
> `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0).

**Qt Phase 1 status:** ✅ COMPLETE — Qt6 core stack + OpenGL/EGL + D-Bus + Wayland
**Qt Phase 2 status:** ✅ COMPLETE — All 32 KF6 frameworks built
**Qt Phase 3 status:** 🔄 IN PROGRESS — KWin + KDE Plasma build

> **Execution note (2026-04-17):** The repo now has a large Redox-desktop-relevant Qt 6.11 subset
> cook-verified across Waves 1–2, but this is not yet the same claim as full Redox-applicable Qt
> 6.11 coverage. Additional graphics / input / desktop-adjacent modules remain to be ported.

## Current Status Summary

| Component | Status | Details |
|-----------|--------|---------|
| **qtbase** | ✅ | 13 libs incl. OpenGL, EGL, DBus, WaylandClient |
| **qtdeclarative** | ✅ | 11 libs, QML JIT disabled |
| **qtsvg** | ✅ | 2 libs |
| **qtwayland** | ◐ Partial | Wayland client path verified; compositor slice still intentionally reduced in recipe |
| **qtimageformats** | ✅ | Real recipe + cook verified |
| **qt5compat** | ✅ | Real recipe + cook verified |
| **qttools** | ◐ Partial | Redox-scoped tooling slice cook verified; designer/assistant/qdoc/qtattributionsscanner intentionally omitted |
| **qttranslations** | ✅ | Translation catalogs cook verified |
| **qtshadertools** | ✅ | Real recipe + cook verified |
| **qtscxml** | ✅ | Real recipe + cook verified |
| **qtserialport** | ✅ | Real recipe + cook verified; unsupported modem-control ioctls mapped to runtime UnsupportedOperationError on Redox |
| **qtwebchannel** | ✅ | Real recipe + cook verified |
| **qtcharts** | ✅ | Real recipe + cook verified |
| **qtquicktimeline** | ✅ | Real recipe + cook verified after Qt Quick substrate export fixes |
| **Mesa EGL+GBM** | ✅ | libEGL, libgbm, libGLESv2, swrast DRI |
| **libdrm** | ✅ | libdrm + libdrm_amdgpu |
| **libinput** | ✅ | 1.30.2 with comprehensive redox.patch |
| **D-Bus** | ✅ | 1.16.2, libdbus-1.so |
| **KF6 Frameworks** | ✅ 32/32 | All frameworks built |
| **KWin** | 🔄 | Recipe ready, now using real `libxcvt`, but still blocked by remaining shimmed/stubbed deps and incomplete runtime path |
| **Hardware acceleration** | ❌ | PRIME/DMA-BUF scheme ioctls implemented; blocked on GPU command submission (CS ioctl) |

---

## Wave 1 — Redox-applicable Qt 6.11 module expansion

The first post-core Qt 6.11 coverage wave is now real-cook verified:

| Module | Status | Verification |
|--------|--------|--------------|
| `qtimageformats` | ✅ | `CI=1 ./target/release/repo cook qtimageformats` |
| `qt5compat` | ✅ | `CI=1 ./target/release/repo cook qt5compat` |
| `qttools` | ✅ | `CI=1 ./target/release/repo cook qttools` |
| `qttranslations` | ✅ | `CI=1 ./target/release/repo cook qttranslations` |
| `qtshadertools` | ✅ | `CI=1 ./target/release/repo cook qtshadertools` |

This means the repo now has real Qt 6.11 recipes for the first high-yield Redox-applicable
expansion set, all verified by actual `repo cook` runs.

## Wave 2 — Qt Quick / integration expansion

The second Redox-applicable Qt 6.11 wave is now also real-cook verified:

| Module | Status | Verification |
|--------|--------|--------------|
| `qtscxml` | ✅ | `CI=1 ./target/release/repo cook qtscxml` |
| `qtserialport` | ✅ | `CI=1 ./target/release/repo cook qtserialport` |
| `qtwebchannel` | ✅ | `CI=1 ./target/release/repo cook qtwebchannel` |
| `qtcharts` | ✅ | `CI=1 ./target/release/repo cook qtcharts` |
| `qtquicktimeline` | ✅ | `CI=1 ./target/release/repo cook qtquicktimeline` |

Wave 2 also required a real repair of the Qt Quick substrate rather than recipe-only leaf work:

- `qtbase` host build now uses a clean separate host build dir with `build/qt-host-build` as the install prefix.
- `qtshadertools` now installs a real host `qsb` and `Qt6ShaderToolsTools` package into that prefix.
- `qtdeclarative` now exports usable `Qt6Quick` / `Qt6Qml` CMake metadata to downstream Redox sysroots.

## Scope Definition

**Phase 1 scope**: qtbase, qtdeclarative, qtsvg — the foundational Qt6 stack.
Qt6 consists of many modules — each is a separate source package. Phase 2 (qtwayland + KF6 Tier 1)
follows in the next step.

**User-agreed scope constraints:**
- OpenGL: now enabled (GLES 2.0 software path via Mesa/LLVMpipe); hardware acceleration still future work
- Still disabled features: process testlib, sql, printsupport remain out of scope for current iteration
- Iterative approach: enable modules incrementally, re-enable disabled features later

## Build Status

### qtbase — Enabled Modules (7 libraries built)

| Module | Library | Size | Description |
|--------|---------|------|-------------|
| QtCore | libQt6Core.so.6.11.0 | 13 MB | Core non-GUI: event loop, IO, threads, plugins |
| QtConcurrent | libQt6Concurrent.so.6.11.0 | 26 KB | High-level multi-threading without locks |
| QtXml | libQt6Xml.so.6.11.0 | 212 KB | XML stream reader/writer (SAX/DOM) |
| QtGui | libQt6Gui.so.6.11.0 | 12 MB | GUI infra: images, painting, text, input, windowing |
| QtWidgets | libQt6Widgets.so.6.11.0 | 9.4 MB | Widget toolkit: buttons, layouts, dialogs |
| QtWaylandClient | libQt6WaylandClient.so.6.11.0 | — | Wayland client integration |
| QtWlShellIntegration | libQt6WlShellIntegration.so.6.11.0 | — | Wayland Shell integration |

### qtbase — Plugins (12 plugin libraries)

| Plugin | File | Type |
|--------|------|------|
| redox | libqredox.so | QPA platform |
| offscreen | libqoffscreen.so | QPA platform |
| minimal | libqminimal.so | QPA platform |
| wayland-bsoft-integration | libqwayland-bsoft-integration.so | Wayland integration |
| gif | libqgif.so | Image format |
| ico | libqico.so | Image format |
| jpeg | libqjpeg.so | Image format |
| png | libqpng.so | Image format |
| svg | libqsvg.so | Image format |
| iconengines | libqsvgicon.so | Icon engine |
| text | libqtext.so | Text platform |
| xkb | libqxkb.so | XKB support |

### qtdeclarative — Built Successfully (build 15)

| Library | Description |
|---------|-------------|
| libQt6Qml.so.6.11.0 | QML core |
| libQt6QmlModels.so.6.11.0 | Models (ListModel, etc.) |
| libQt6Quick.so.6.11.0 | QtQuick UI framework |
| libQt6QmlCore.so.6.11.0 | QML internals |
| libQt6QmlCompiler.so.6.11.0 | QML JIT compiler |
| libQt6QmlWorkerScript.so.6.11.0 | Worker script runtime |
| libQt6QmlMeta.so.6.11.0 | QML meta-object |
| libQt6QmlXmlListModel.so.6.11.0 | XML ListModel |
| libQt6LabsFolderListModel.so.6.11.0 | Folder list model |
| libQt6LabsQmlModels.so.6.11.0 | Lab models |
| libQt6LabsSettings.so.6.11.0 | Settings |
| libQt6LabsSynchronizer.so.6.11.0 | Synchronizer |

Plus: QML debug plugins, QtQuick/QML modules staged.

**Note**: QML JIT (`QT_FEATURE_qml_jit`) does not compile for Redox — disabled.

### qtsvg — Built Successfully

| Component | File |
|-----------|------|
| libQt6Svg.so.6.11.0 | SVG rendering |
| libQt6SvgWidgets.so.6.11.0 | SVG widget integration |
| qsvg icon engine | libqsvgicon.so |
| qsvg image format | libqsvg.so |

### Disabled Modules — Full Blocker Analysis

| Module | Status | Blocker | Re-enable Path |
|--------|--------|---------|----------------|
| QtNetwork | ❌ Disabled | relibc networking runtime semantics still incomplete (DNS resolver, IPv6 multicast) | Validate QtNetwork against the updated relibc networking surface |
| QtSql | ❌ Disabled | User-agreed scope exclusion | Add sqlite/odbc recipe → enable QtSql |
| QtPrintSupport | ❌ Disabled | User-agreed scope exclusion, no printing subsystem on Redox | Port cups/filters → enable QtPrintSupport |

> **Previously disabled, now enabled:** QtOpenGL (✅ Phase 4b), QtOpenGLWidgets (✅ Phase 4b), and
> QtDBus (✅ Phase 2a) were disabled in earlier builds but have since been enabled and built
> successfully. See Phase 4b and Phase 2a sections below for details.

### Disabled Features — Full Blocker Analysis

| Feature | CMake Flag | Status | Notes |
|---------|-----------|--------|-------|
| XCB/Xlib | `-DFEATURE_xcb=OFF -DFEATURE_xlib=OFF` | ❌ Disabled | Not applicable — Redox uses Wayland, not X11 |
| Vulkan | `-DFEATURE_vulkan=OFF` | ❌ Disabled | No Vulkan runtime on Redox |
| OpenSSL | `-DFEATURE_openssl=OFF` | ❌ Disabled | OpenSSL3 port in WIP but not validated |
| qmake | `-DFEATURE_qmake=OFF` | ❌ Disabled | Build tool, not needed with CMake |
| SQL | `-DFEATURE_sql=OFF` | ❌ Disabled | User-agreed scope exclusion |
| Print Support | `-DFEATURE_printsupport=OFF` | ❌ Disabled | User-agreed scope exclusion |
| QML JIT | `-DFEATURE_qml_jit=OFF` | ❌ Disabled | Does not compile for Redox |

> **Previously disabled, now enabled:** OpenGL (`-DFEATURE_opengl=ON`), EGL (`-DFEATURE_egl=ON`),
> and D-Bus (`-DFEATURE_dbus=ON`) were disabled in earlier builds but have since been enabled and
> built successfully. Process, shared memory, and system semaphore were also enabled after relibc
> improvements. See respective Phase sections for details.

---

## New Discoveries (Builds 8–17)

| # | Discovery | Fix |
|---|-----------|-----|
| 8 | qtwaylandscanner is a host tool, needs `FEATURE_qtwaylandscanner=ON` in both host and target builds | Enable feature in both cmake configs |
| 9 | wayland-scanner must be host binary — use `-DWaylandScanner_EXECUTABLE=/usr/bin/wayland-scanner` | Pass explicit path to host wayland-scanner |
| 10 | OpenGL guards needed in Wayland code (`#if QT_CONFIG(opengl)`) | Add guard in qtbase patch |
| 11 | `cmake --install` produces relocatable cmake files — replaced manual cmake copy | Use cmake install instead of manual sed |
| 12 | `QT_MKSPECS_DIR` must point to staged mkspecs — conditional in toolchain file | Add conditional logic in redox-toolchain.cmake |
| 13 | QtNetwork features leak into downstream — pass `QT_FEATURE_ssl=OFF` etc. | Explicitly disable in downstream cmake |
| 14 | SBOM generation fails — use `-DQT_GENERATE_SBOM=OFF` | Disable SBOM generation |
| 15 | Sysroot path mismatch — cookbook only symlinks bin/include/lib/share, need manual symlinks for plugins/mkspecs/metatypes/modules | Add manual symlinks in recipe |
| 16 | masm `CheckedArithmetic.h` missing `ArithmeticOperations<unsigned,long>` for LP64 | Add missing arithmetic operation to masm |
| 17 | QML JIT (`QT_FEATURE_qml_jit`) doesn't compile for Redox — disabled | Disable feature, works without JIT |
| 56 | **plasma-wayland-protocols** is a required separate package — kf6-kwayland needs PLASMA_WAYLAND_PROTOCOLS_DIR pointing to protocol XMLs | Created recipe that installs XML files + symlink for naming mismatch (org-kde-plasma-virtual-desktop.xml → plasma-virtual-desktop.xml) |
| 57 | **kf6-kcmutils** requires Qt6Quick unconditionally upstream | Strip Quick/QML/kcmshell from CMakeLists via Python-based source patching — produces libKF6KCMUtils.so + libKF6KCMUtilsCore.so (widget-only build) |
| 58 | **kf6-kwayland** fails with `get_filename_component called with incorrect number of arguments` when PLASMA_WAYLAND_PROTOCOLS_DIR is unset | Fix: create plasma-wayland-protocols package + point the cmake variable to the installed XMLs |
| 59 | **seatd** now builds as a standalone runtime package for Redox and is wired into the KDE runtime config; keep it out of KWin compile deps until DRM-lease/runtime validation exists | Runtime dependency only |

---

## Build Iteration History

| # | Issue | Fix |
|---|-------|-----|
| 1-7 | Patch format, byteswap.h, forwarding headers | Patch structure |
| 8 | qtwaylandscanner is host tool | `FEATURE_qtwaylandscanner=ON` in host+target |
| 9 | wayland-scanner must be host binary | `-DWaylandScanner_EXECUTABLE=/usr/bin/wayland-scanner` |
| 10 | OpenGL guards in Wayland code | `#if QT_CONFIG(opengl)` guard |
| 11 | cmake --install relocatable | Use cmake install over manual copy |
| 12 | QT_MKSPECS_DIR mismatch | Conditional in toolchain file |
| 13 | QtNetwork feature leak | Pass explicit QT_FEATURE_* flags |
| 14 | SBOM generation fails | `-DQT_GENERATE_SBOM=OFF` |
| 15 | Sysroot path mismatch (plugins/mkspecs/metatypes/modules) | Manual symlinks |
| 16 | masm CheckedArithmetic.h missing LP64 operation | Add ArithmeticOperations |
| 17 | QML JIT doesn't compile for Redox | Disable `QT_FEATURE_qml_jit` |
| **Phase 1** | **qtbase + qtdeclarative + qtsvg complete** | **✅ Core stack built** |

---

## relibc Gaps — Complete Inventory

### Resolved (workarounds in recipe/patch)

| Gap | Workaround | Location |
|-----|-----------|----------|
| `sys/statfs.h` missing | Wrapper → `sys/statvfs.h` (typedef, #define) | recipe.toml heredoc |
| `ELFMAG` string missing from `elf.h` | `#define ELFMAG "\177ELF"` prepended to source | recipe.toml printf |
| `resolv.h` availability | Minimal relibc header now exists in-tree | verify downstream consumers against the generated header |
| `unlinkat()`/`linkat()` missing | Inline stubs with `AT_FDCWD` | redox.patch |
| `byteswap.h` missing | Skip include on Redox | redox.patch (brg_endian.h) |
| Float16 soft-fp (`__truncsfhf2` etc.) | Custom IEEE 754 C implementation | redox.patch (qt_float16_shims.c) |
| Half-float comparison (`__eqhf2` etc.) | Custom IEEE 754 C implementation | redox.patch (same file) |
| `openat()` not available | `#ifdef Q_OS_REDOX` guard | redox.patch (qcore_unix_p.h) |

### Networking Surface — Now Present, Still Needs Runtime Validation

| Gap | Impact | relibc File to Modify |
|-----|--------|----------------------|
| `resolv.h` | Present in relibc as a minimal source-visible header | `recipes/core/relibc/source/src/header/resolv/` |
| `in6_pktinfo` / `ipv6_mreq` | Present in relibc | `recipes/core/relibc/source/src/header/netinet_in/mod.rs` |
| `SIOCGIF*` ioctls | Present for the current Redox `eth0` model | `recipes/core/relibc/source/src/header/sys_ioctl/redox/mod.rs` |
| `::ioctl` path | Present in relibc Redox ioctl implementation | `recipes/core/relibc/source/src/header/sys_ioctl/` |
| `ifreq` / `ifconf` / `ifaddrs` | Present for the current Redox `eth0` model | `recipes/core/relibc/source/src/header/net_if/mod.rs`, `recipes/core/relibc/source/src/header/ifaddrs/mod.rs` |

### Unresolved — Blocks Other Qt Modules/Features

| Gap | Impact | Module Blocked |
|-----|--------|---------------|
| broader networking runtime validation | QtNetwork end-to-end behavior | QtNetwork |
| GPU hardware display validation | Hardware-accelerated rendering | QtOpenGL hardware path |
| broader shared-memory validation beyond the existing `shm_open()` path | Shared memory | QSharedMemory |
| broader semaphore/system-IPC validation beyond the new `sem_open()` path | POSIX semaphores | QSystemSemaphore |
| process/runtime validation beyond the new bounded `waitid()` path | QProcess internals | QProcess |

Recent relibc implementation progress in this repo now also includes:

- source-visible `signalfd`, `timerfd`, `eventfd`, `open_memstream`, `F_DUPFD_CLOEXEC`, and `MSG_NOSIGNAL`
- a bounded `waitid()` path in relibc, replacing the old Qt-side waitid stub workaround
- a bounded `eth0`-backed `net_if` / `ifaddrs` path in relibc
- a minimal source-visible `resolv.h` surface in relibc
- bounded `sys/ipc.h` / `sys/shm.h` surfaces for the `IPC_PRIVATE` shared-memory workflow

Current downstream build proof in this repo now includes:

- `libwayland` cooking successfully against the updated relibc surfaces
- qtbase configuring, building, and staging with `process`, `sharedmemory`, and `systemsemaphore` enabled
| Fontconfig | Advanced font selection | QtGui (bundled FreeType works for basic) |

---

## Next Steps

### Phase 2a — qtbase D-Bus Enablement (✅ COMPLETE)

- qtbase target build: `-DFEATURE_dbus=ON` (Qt6DBus module built and staged)
- qtbase host build: `-DFEATURE_dbus=OFF` (qdbus tools provisioned via /usr/bin symlinks)
- libQt6DBus.so + Qt6DBusConfig.cmake + Qt6DBus.pc staged to sysroot
- D-Bus 1.16.2 already built (24-line redox.patch for epoll + socketpair)
- Unblocks: kf6-kdbusaddons, kf6-kservice, kf6-kpackage, kf6-kglobalaccel
- D-Bus plan: `local/docs/DBUS-INTEGRATION-PLAN.md` — redbear-sessiond login1 broker + D-Bus service infrastructure for KDE Plasma

**redbear-sessiond:** Implemented. Rust daemon at `local/recipes/system/redbear-sessiond/` using zbus 5, serving `org.freedesktop.login1` Manager/Session/Seat interfaces on the system bus. Maps `TakeDevice(major, minor)` to Redox scheme paths (`/scheme/drm/card0`, `/dev/input/eventN`). Config wired in `config/redbear-kde.toml` with init service at slot 13.

**qdbuscpp2xml/qdbusxml2cpp provisioning:** Qt host build has `FEATURE_dbus=OFF` with these tools disabled. KDE recipes provision them via symlinks: kf6-kdbusaddons falls back to `/usr/bin/qdbuscpp2xml` and `/usr/bin/qdbusxml2cpp` from the host system. This works for cross-compilation but is not a long-term solution. Future improvement: enable FEATURE_dbus=ON in host build once D-Bus session bus validation passes.

**KF6 D-Bus re-enablement roadmap:** 15 KF6 components currently build with `-DUSE_DBUS=OFF`. Re-enablement is gated on D-Bus service availability: kf6-knotifications needs `org.freedesktop.Notifications` (DB-2, now enabled against a stub notification daemon), kf6-solid needs runtime-validated `org.freedesktop.UPower` + `org.freedesktop.UDisks2` enumeration (DB-3, both daemons now expose bounded real enumeration). The runtime proof harness is now in place, but `kf6-solid` still keeps `-DUSE_DBUS=OFF`, `-DBUILD_DEVICE_BACKEND_upower=OFF`, and `-DBUILD_DEVICE_BACKEND_udisks2=OFF` until `solid-hardware6`/Phase 6 validation can confirm the consumer path. kf6-kio and 10 others need full desktop services (DB-5). See `local/docs/DBUS-INTEGRATION-PLAN.md` Section 14 for the complete matrix.

**Key insight:** QtDBus is NOT the gap — Qt6DBus builds and kf6-kdbusaddons provides the KDE convenience layer. The gap is the freedesktop service contracts (login1, Notifications, UPower, UDisks2, PolicyKit) that need Redox-native implementations. NetworkManager is deferred; Red Bear OS uses `redbear-netctl` for now.

### Phase 2b — qtwayland Module (🔄 Building)

- Recipe at `recipes/wip/qt/qtwayland/recipe.toml`
- Uses redox-toolchain.cmake + host Qt build pattern
- Wayland compositor disabled, client-only build
- OpenGL guards applied for software rendering

### Phase 2c — Input Stack (✅ COMPLETE)

- linux-input-headers: ✅ Built — provides linux/input.h + linux/types.h + _CNT macros
- libevdev 1.13.2: ✅ Built — uinput stubs + input.h __redox__ guard
- libinput 1.30.2: ✅ Built — comprehensive redox.patch:
  - SYS_pidfd_open meson guard (cc.has_header check)
  - Non-udev shim (libudev stub for HAVE_UDEV=0)
  - Vendored Linux input.h selection for __redox__
  - strtod_l() fallback
  - timerfd fallback (tracks expiry without timerfd fd)
  - Linux-only tool binaries skipped on Redox

### Phase 3 — KF6 Frameworks (✅ ALL 32 BUILT)

All KF6 frameworks built and staged:

ecm, kcoreaddons, kwidgetsaddons, kconfig, ki18n, kcodecs, kguiaddons, kcolorscheme,
kauth, kwindowsystem, knotifications, kjobwidgets, kconfigwidgets, karchive, sonnet,
kcompletion, kitemviews, kitemmodels, solid, kdbusaddons, kservice, kpackage,
kcrash, ktextwidgets, kiconthemes, kglobalaccel, kdeclarative, kxmlgui, kbookmarks,
kidletime, kio, kcmutils

Additional KDE packages:
- kdecoration ✅ BUILT (KDecoration3 window decoration library)
- kirigami ✅ STUB ONLY (dependency-resolution package, not a real runtime-ready Kirigami build)
- kf6-kwayland ✅ BUILT
- kf6-kcmutils ✅ BUILT (widget-only, Quick/QML/kcmshell stripped)
- plasma-wayland-protocols ✅ BUILT (protocol XMLs for kf6-kwayland)

Graphics stack (PRIMARY DELIVERABLE):
- Mesa EGL+GBM ✅ BUILT (libEGL.so, libgbm.so, libGLESv2.so, swrast_dri.so)
- libdrm amdgpu ✅ BUILT (libdrm_amdgpu.so, /scheme/drm/ paths)
- Qt6 OpenGL ✅ BUILT (libQt6OpenGL.so, libQt6EglFSDeviceIntegration.so, GLES 2.0)
- D-Bus ✅ BUILT (libdbus-1.so.3.38.3, dbus-daemon)
- libinput ✅ BUILT (libinput.so.10.13.0, comprehensive redox.patch)
- libevdev ✅ BUILT (libevdev.so.2.3.0, uinput stubs)

KWin recipe updated with 40 dependencies (all KF6 + Mesa + libdrm + libinput + qtwayland).
plasma-workspace, plasma-desktop recipes created.

### Phase 4 — Graphics Stack (✅ build-side complete, 🚧 runtime incomplete)

Mesa EGL+GBM+GLES2 built:
- libEGL.so (225KB) — platforms: redox, surfaceless, drm
- libgbm.so (68KB) — Generic Buffer Manager for compositor buffer allocation
- libGLESv2.so — OpenGL ES 2.0 (software via LLVMpipe)
- libGLESv1_CM.so — OpenGL ES 1.1
- swrast_dri.so + kms_swrast_dri.so — LLVMpipe software DRI drivers
- pkgconfig: egl.pc, gbm.pc, osmesa.pc, glesv2.pc, dri.pc

libdrm amdgpu enabled:
- libdrm_amdgpu.so (48KB) — AMD GPU DRM API
- Device paths: /scheme/drm/cardN, /scheme/drm/renderD

Qt6 OpenGL enabled:
- libQt6OpenGL.so (716KB) — Qt OpenGL module (GLES 2.0 path)
- libQt6OpenGLWidgets.so — Qt OpenGL widgets
- libQt6EglFSDeviceIntegration.so — EGLFS platform integration
- EGLFS KMS plugin for direct DRM/KMS rendering

Current truth for Phase 4:

- the graphics stack now builds end to end: Mesa EGL+GBM+GLES2, libdrm amdgpu, Qt6 OpenGL/EGL,
  and qtwayland all stage successfully
- the current `redbear-wayland` validation profile is still a bounded smallvil-first runtime path,
  not proof of a hardware-accelerated desktop session
- the current QEMU validation harness is still software-rendered (`llvmpipe`) and should be treated
  as a bounded regression/test path, not as the final acceleration proof target
- the in-repo Phase 4 runtime check currently still fails in `qt6-bootstrap-check` during early Qt
  startup, so even the bounded software-path runtime proof remains incomplete
- true hardware-accelerated desktop readiness still requires GPU command submission (CS ioctl) plus real
  AMD/Intel hardware validation through the DRM → GBM/EGL → compositor → Qt client path
  (PRIME/DMA-BUF cross-process buffer sharing is implemented at scheme level)

### Phase 4b — Qt6 OpenGL Enablement (✅ build-side complete, 🚧 runtime incomplete)

qtbase rebuilt with `-DFEATURE_opengl=ON -DINPUT_opengl=es2 -DFEATURE_egl=ON`
Qt cmake summary: EGL=yes, OpenGL=yes, "OpenGL ES 2.0=yes, EGLFS GBM=yes"

### Phase 5 — KDE Plasma / desktop-session layer (🔄 IN PROGRESS)

KDE Plasma packages built:
- kf6-kwayland ✅ BUILT
- kf6-kcmutils ✅ BUILT (widget-only, Quick/QML/kcmshell stripped)
- kirigami ✅ STUB ONLY (dependency-resolution package, not a real runtime-ready Kirigami build)
- plasma-wayland-protocols ✅ BUILT (protocol XMLs for kf6-kwayland)
- kdecoration ✅ BUILT (KDecoration3 window decoration library)

plasma-workspace stub dependencies partially resolved:
- kf6-knewstuff ✅ STUB ONLY (KF6NewStuff cmake INTERFACE IMPORTED targets for plasma-workspace dep resolution)
- kf6-kwallet ✅ STUB ONLY (KF6Wallet cmake INTERFACE IMPORTED targets for plasma-workspace dep resolution)
- kf6-prison ✅ REAL RECIPE (real cmake build against libqrencode; dmtx/ZXing disabled; not yet compiled)

qt6-wayland-smoke improved to create a visible QWindow:
- Creates a 320x240 colored window (red background, "Red Bear OS - Qt6 Wayland Smoke Test" text)
- Uses QBackingStore for software rendering
- Runs for 3 seconds (previously 1 second, no window)
- This turns the smoke test from a bootstrap check into a real Wayland surface proof target

KWin recipe updated — features re-enabled where deps are satisfied:
- KWIN_BUILD_DECORATIONS=ON (kdecoration builds ✅)
- KWIN_BUILD_GLOBALSHORTCUTS=ON (kglobalaccel builds ✅)
- KWIN_BUILD_RUNNERS=ON (kf6-kio builds ✅)
- KWIN_BUILD_NOTIFICATIONS=ON (knotifications builds ✅)
- USE_DBUS=ON (D-Bus 1.16.2 builds ✅)
- Still disabled (9): KCMS, screen locking, tabbox, effects, X11, QML, running-in-kde,
  signing docs, screenlocker
- Stub deps remaining: libepoxy-stub, libudev-stub, lcms2-stub, libdisplay-info-stub

New dependency library:
- libqrencode 4.1.1 ✅ BUILT (QR code encoder, dependency of kf6-prison)
- kf6-kwayland ✅
- seatd builds separately (runtime dependency, not needed for compilation)

### Phase 6 — KWin (🔄 BUILDING)

## Dependency Graph

```
Phase 1 ✅ (qtbase + qtdeclarative + qtsvg)
    └── Phase 2a ✅ (D-Bus daemon + qtbase D-Bus enablement)
    └── Phase 2b ✅ (qtwayland built)
    └── Phase 2c ✅ (libevdev + libinput built)
    └── Phase 3 ✅ (KF6 — ALL 32 frameworks built)
    └── Phase 4 ✅ build-side / 🚧 runtime (Mesa EGL+GBM+GLES2, Qt6 OpenGL+EGL, libdrm amdgpu)
    └── Phase 5 🔄 (kdecoration ✅, kf6-kwayland ✅, kirigami stub-only, KWin still blocked on shimmed/scaffolded deps)
```

---

## Known Issues

1. **QML JIT disabled** — `QT_FEATURE_qml_jit` does not compile for Redox. QML still works
   via the interpreter path, just without JIT acceleration. Non-blocking for basic QML apps.

2. **QtNetwork disabled** — relibc now exposes bounded resolver compatibility (`resolv.h`,
   `arpa/nameser.h`, `res_query`, `res_search`), but DNS/runtime semantics and IPv6 multicast
   coverage are still incomplete. HTTP/WebSocket remain unavailable until relibc networking is
   validated more broadly. QML network access is also affected.

3. **No GPU hardware acceleration** — Qt6 OpenGL/EGL and Mesa EGL+GBM now build, but they are still validated only on the software/LLVMpipe path.
   True hardware acceleration (radeonsi or equivalent) still requires GPU command submission and real hardware validation.
   PRIME/DMA-BUF cross-process buffer sharing is implemented at the scheme level.

4. **relibc / graphics surface still incomplete for runtime** — the build-side `open_memstream` and Wayland-facing header export path now work,
   but DMA-BUF ioctls, sync objects, and broader graphics runtime validation are still unavailable.

5. **KDE Plasma does NOT compile or run end-to-end** — KWin, plasma-workspace, plasma-desktop recipes exist,
   but are still blocked on shimmed/stubbed dependencies, runtime integration, and compositor validation.

## Honest Status Assessment

The Qt6/KF6 build stack is substantially further along than the earlier "~50%" estimate implied:
- Qt6, QtWayland, Mesa EGL+GBM, Qt6 OpenGL, libdrm amdgpu, and all 32 KF6 frameworks now build
- the remaining blockers are concentrated in KWin/Plasma runtime integration and in the still-shimmed or stub-only packages such as Kirigami, libepoxy, libudev, lcms2, and libdisplay-info
- hardware acceleration still requires GPU command submission and real hardware validation (PRIME/DMA-BUF buffer sharing is implemented)
- a successful build stack is not yet the same thing as a working KDE Plasma session

For the canonical execution plan from this state to a working KDE Plasma desktop, see
`local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0). The Qt work described here maps to
pre-Phase work (builds complete) and Phase 3 (KWin desktop session) in the canonical plan.

(Updated 2026-04-16 — aligned with CONSOLE-TO-KDE-DESKTOP-PLAN.md v2.0)
