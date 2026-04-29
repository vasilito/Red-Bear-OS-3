# Red Bear OS D-Bus Integration Plan
**Implementation status (2026-04-29):** All DBUS plan code artifacts are build-verified. Remaining items are runtime validation gates requiring QEMU.

**Version:** 3.0 — 2026-04-29
**Status:** Active plan aligned with the desktop path v3.0
**Scope:** Full D-Bus infrastructure for KDE Plasma 6 on Wayland, tightly integrated with Redox scheme IPC
**Parent plan:** `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v3.0)

---

## 1. Executive Summary

D-Bus is **mandatory infrastructure** for KDE Plasma 6 — not optional, not deferrable. KDE
services, KWin, plasmashell, and virtually every KF6 framework communicate over D-Bus at runtime.

Red Bear OS already has D-Bus 1.16.2 building with a 24-line redox.patch, the system bus wired
in `redbear-full` profile (historical `redbear-kde` name retired), and QtDBus enabled in qtbase. This is a solid
foundation, but it is only the transport layer. What's missing is the **service layer** — the
D-Bus services that KDE actually expects to talk to.

This plan defines a Redox-native D-Bus service architecture built on three decisions:

1. **Use `dbus-daemon`** (reference implementation), not `dbus-broker`. It works without systemd,
   supports traditional `.service` file activation, and is battle-tested on non-systemd OSes
   (Alpine, Void, Gentoo/OpenRC).

2. **Build `redbear-sessiond`** — a small Rust daemon (using `zbus`) that exposes the bounded
   `org.freedesktop.login1` surface KWin already expects, plus a few higher-level manager helpers
   (`GetUser`, `ActivateSessionOnSeat`, lock/unlock/terminate helpers) that broader KDE session
   plumbing can call without forcing Red Bear into an elogind-sized reimplementation. Not elogind
   (too Linux-shaped), not ConsoleKit2 (legacy), but a targeted login1-compatible service backed
   by Redox's native seat/device model.

3. **Keep schemes and D-Bus separate.** Schemes are the native resource plane. D-Bus is the
   desktop compatibility plane. Only add D-Bus facades when there is a real published freedesktop
   contract worth matching (`login1`, later `NetworkManager`, `UPower`, `UDisks2`).

**Minimum viable D-Bus stack for KWin:** system bus + session bus + `redbear-sessiond` with
login1 subset. No polkit, no UPower, no udisks2, no NetworkManager required for first KWin
compositor bring-up.

---

## 2. Architecture Principles

### 2.1 Schemes = Native Resource Plane, D-Bus = Desktop Compatibility Plane

```
┌─────────────────────────────────────────────────────────────────────┐
│  KDE Plasma / KWin / KF6                                           │
│  (speaks D-Bus, expects freedesktop contracts)                     │
├─────────────────────────────────────────────────────────────────────┤
│  D-Bus Compatibility Services                                      │
│  redbear-sessiond (login1)   redbear-notifications   redbear-polkit│
│  (zbus-based Rust daemons, translating D-Bus ↔ scheme calls)      │
├─────────────────────────────────────────────────────────────────────┤
│  dbus-daemon (system bus + session bus)                            │
│  Classic .service activation, XML policies                         │
├─────────────────────────────────────────────────────────────────────┤
│  Redox Schemes (native IPC)                                        │
│  scheme:pci  scheme:input  scheme:drm  scheme:net  scheme:acpi    │
│  evdevd       udev-shim     redox-drm   netd        acpid         │
└─────────────────────────────────────────────────────────────────────┘
```

D-Bus services **wrap** scheme resources into freedesktop-compatible contracts.
They do **not** replace or mirror schemes. Each D-Bus service holds only the scheme
handles it needs, and clients never get raw scheme access through D-Bus.

### 2.2 Use Existing Contracts, Don't Invent New Ones

| Namespace | Use For |
|-----------|---------|
| `org.freedesktop.login1` | Session/seat/device tracking — KWin expects this |
| `org.freedesktop.Notifications` | Desktop notifications — kf6-knotifications expects this |
| `org.freedesktop.NetworkManager` | Deferred — not in current Red Bear OS scope |
| `org.freedesktop.UPower` | Power management — PowerDevil expects this |
| `org.freedesktop.UDisks2` | Storage management — Solid/udisks backend expects this |
| `org.freedesktop.PolicyKit1` | Privileged actions — KAuth expects this |
| `org.kde.*` | KDE-specific services — KWin, plasmashell, kglobalaccel, kded6 |
| `org.redbear.*` | Red Bear-specific services that don't match any freedesktop contract |

### 2.3 No Generic Scheme→D-Bus Bridge

Do NOT build a universal translator that mirrors every scheme as a D-Bus object. Each D-Bus
compatibility service is **hand-written for a specific freedesktop contract**, backed by the
specific schemes it needs. This keeps the architecture honest and avoids a leaky abstraction.

### 2.4 Rust-Native for New Services, C for Existing Daemons

| Component | Implementation | Why |
|-----------|---------------|-----|
| `dbus-daemon` | C (freedesktop upstream) | Battle-tested, minimal redox.patch, no reason to rewrite |
| `libdbus-1` | C (part of dbus package) | Required by QtDBus, kf6-kdbusaddons |
| `redbear-sessiond` | Rust + `zbus` | New code, Rust-native OS, async, strong typing |
| Future compat daemons | Rust + `zbus` | Same reasons as redbear-sessiond |
| KDE apps (KWin, plasmashell) | C++ via QtDBus | Upstream KDE code, not our concern |

---

## 3. Current State Assessment

### 3.1 What Exists and Works

| Component | Location | Status | Detail |
|-----------|----------|--------|--------|
| **D-Bus 1.16.2 daemon** | `recipes/wip/services/dbus/` | ✅ Builds, bounded runtime | 24-line redox.patch (epoll guard + socketpair fix), meson build with systemd disabled |
| **libdbus-1** | Part of dbus package | ✅ Builds | `libdbus-1.so.3.38.3` staged, pkgconfig and cmake files present |
| **QtDBus** | `recipes/wip/qt/qtbase/` | ✅ Enabled | `FEATURE_dbus=ON` for target build, Qt6DBus module present |
| **kf6-kdbusaddons** | `local/recipes/kde/kf6-kdbusaddons/` | ✅ Builds | KF6 D-Bus convenience wrappers, provides qdbus tool integration |
| **D-Bus system bus** | `config/redbear-full.toml` | ✅ Wired | `12_dbus.service` launches `dbus-daemon --system`, `messagebus` user (uid=100), `/var/lib/dbus` + `/run/dbus` directories |
| **D-Bus session bus** | `local/recipes/system/redbear-greeter/source/redbear-kde-session` | ✅ Scripted | `redbear-kde-session` launches `dbus-launch --sh-syntax` before KWin |
| **seatd** | `config/redbear-full.toml` | ✅ Wired | `13_seatd.service`, `LIBSEAT_BACKEND=seatd`, `SEATD_SOCK=/run/seatd.sock` |
| **kf6-kservice** | `local/recipes/kde/kf6-kservice/` | ✅ Builds | Depends on kf6-kdbusaddons |
| **kf6-kglobalaccel** | `local/recipes/kde/kf6-kglobalaccel/` | ✅ Builds | Depends on kf6-kdbusaddons |
| **Session activation scaffolds** | `local/recipes/system/redbear-dbus-services/` | ✅ Staged | Session `.service` files now cover kded6, kglobalaccel, ActivityManager, JobViewServer, ksmserver, notifications, and StatusNotifierWatcher |
| **KWin (D-Bus)** | `local/recipes/kde/kwin/` | ✅ USE_DBUS=ON | Registers `org.kde.KWin` on session bus |

### 3.2 What Exists But Is Incomplete

| Component | Location | Status | Gap |
|-----------|----------|--------|-----|
| **elogind** | `recipes/wip/services/elogind/` | ⚠️ Recipe only | "not compiled or tested" — needs libeudev + libcap, too Linux-shaped for Redox |
| **kf6-knotifications** | `local/recipes/kde/kf6-knotifications/` | ⚠️ D-Bus ON (scaffold-backed) | Built with `-DUSE_DBUS=ON`, but current notification daemon is still a minimal scaffold |
| **kf6-kio** | `local/recipes/kde/kf6-kio/` | ⚠️ D-Bus OFF | Built with `-DUSE_DBUS=OFF` — D-Bus IPC disabled, has systemd1 XML interfaces in source |
| **kf6-solid** | `local/recipes/kde/kf6-solid/` | ⚠️ D-Bus OFF | Built with `-DUSE_DBUS=OFF` — UDev/UPower/udisks2 backends all disabled |
| **kf6-kwallet: real API-only build (no daemon) | Dummy cmake configs only, no real implementation |
| **plasma-workspace** | `local/recipes/kde/plasma-workspace/` | ⚠️ Partial | Explicit dbus dep; sub-services activation files staged; runtime proof requires QEMU |

### 3.3 What Ships Today (Scaffolds and Deferred Items)

| Component | Namespace | Purpose | KDE Consumer |
|-----------|-----------|---------|-------------|
| **Session tracker** | `org.freedesktop.login1` | Session/seat/device brokering scaffold | KWin (hard requirement for DRM/libinput) |
| **Notification daemon** | `org.freedesktop.Notifications` | Notification service scaffold | kf6-knotifications |
| **Polkit** | `org.freedesktop.PolicyKit1` | Authorization scaffold (always-permit) | KAuth |
| **UPower** | `org.freedesktop.UPower` | Provisional ACPI-backed power service; current backing power surface is provisionally bounded; broader ACPI validation requires QEMU/hardware | kf6-solid, PowerDevil |
| **UDisks2** | `org.freedesktop.UDisks2` | Bounded real `disk.*` / partition enumeration | kf6-solid |
| **D-Bus service files** | `/usr/share/dbus-1/` | Activation is staged and shipped for the current scaffold services plus bounded KDE session daemons (`kded6`, `kglobalaccel`, ActivityManager, JobViewServer, ksmserver) | All D-Bus services |
| **D-Bus policy files** | `/etc/dbus-1/` | Policy is staged and shipped for the current scaffold services | All D-Bus services |
| **zbus crate marker** | `local/recipes/libs/zbus/` | Build-ordering marker; actual zbus crate is fetched by downstream Cargo builds | Future Rust D-Bus services |

**Deferred and not shipped in the current implementation cycle:**
- `org.freedesktop.NetworkManager` / `redbear-nm` — Red Bear OS uses `redbear-netctl` for now

---

## 4. Gap Analysis

### 4.1 Critical Path Gaps (blocks KWin compositor)

```
KWin needs:
  dbus-daemon --system          ✅ exists, wired
  dbus-daemon --session         ✅ exists, wired in redbear-kde-session
  org.freedesktop.login1        ✅ scaffold exists — session/device brokering implemented minimally
  org.kde.KWin (self-register)  ✅ KWin does this itself (dbusinterface.cpp)
```

**The single critical runtime-risk area is `org.freedesktop.login1`.** KWin's `session_logind.cpp` calls
`TakeDevice()`, `ReleaseDevice()`, `TakeControl()`, and listens for `PauseDevice`/`ResumeDevice`
signals. Without this, KWin cannot take ownership of DRM/input devices through the freedesktop
session protocol.

KWin's session selection chain is: `logind → ConsoleKit → Noop`. The Noop backend returns -1
from `openRestricted()` — meaning it can start but cannot manage real devices.

### 4.2 Desktop Session Gaps (blocks plasma-workspace)

```
plasma-workspace needs:
  org.kde.KWin                    ✅ KWin provides
  org.kde.kglobalaccel            ⚠️ activation file staged — daemon/runtime proof build-verified; QEMU validation supplementary
  org.kde.kded6                   ⚠️ activation file staged — daemon/runtime proof build-verified; QEMU validation supplementary
  org.kde.plasmashell             ✅ plasmashell provides (self-register)
  org.kde.osdService              ✅ plasmashell provides
  org.freedesktop.Notifications   ✅ scaffold exists — current daemon logs to stderr only
```

### 4.3 Full Desktop Gaps (blocks complete KDE Plasma)

```
Complete Plasma needs (after re-enabling disabled components):
  org.freedesktop.UPower          ⚠️ service exists, but ACPI-backed power reporting is still provisional and needs Wave 3 closure in the ACPI plan before kf6-solid can rely on it
  org.freedesktop.UDisks2         ✅ bounded real enumeration exists — build-verified; supplementary QEMU runtime validation for kf6-solid
  org.freedesktop.NetworkManager  ⏸️ DEFERRED — Red Bear OS uses redbear-netctl for now
  org.freedesktop.PolicyKit1      ⚠️ scaffold exists — KAuth still blocked on missing PolkitQt6-1 packaging
  org.freedesktop.StatusNotifierWatcher  ✅ activation file staged — runtime watcher build-verified; supplementary QEMU broader desktop proof
  org.kde.JobViewServer           ⚠️ activation file staged — kuiserver binary/runtime still open
  org.kde.ksmserver               ⚠️ activation file staged — session manager binary/runtime still open
  org.kde.ActivityManager         ⚠️ activation file staged — activity manager binary/runtime still open
  org.freedesktop.ScreenSaver: deferred (not on critical path) — screen locking
```

### 4.4 Build System Gaps

| Gap | Detail |
|-----|--------|
| `zbus` recipe is only a marker | Build-ordering marker exists; actual Rust crate comes from downstream Cargo resolution |
| D-Bus service activation is scaffolded | `/usr/share/dbus-1/system-services/` and `session-services/` are staged for current services |
| D-Bus policy configuration is scaffolded | `/etc/dbus-1/system.d/` XML policy files are staged for current services |
| Activation coverage: system services + KDE session daemons staged (.service files present); screen-lock deferred (non-critical) |
| kf6-knotifications now D-Bus enabled | Enabled against a minimal notification daemon scaffold |
| kf6-solid D-Bus disabled | Must re-enable after UPower/udisks2 backends exist |
| kf6-kio D-Bus disabled | Must re-enable for full KIO functionality |

---

## 5. Architecture Design

### 5.1 Overall Stack

```
┌──────────────────────────────────────────────────────────────────────────┐
│                     KDE Plasma 6 Desktop Session                        │
│   plasmashell, kwin_wayland, kded6, kglobalaccel, plasma applets        │
├──────────────────────────────────────────────────────────────────────────┤
│                     Session Bus (per-user)                               │
│   Started by: redbear-kde-session via dbus-launch or dbus-run-session   │
│   Policy: /etc/dbus-1/session.conf                                      │
│   Services: /usr/share/dbus-1/session-services/                         │
│   ┌──────────────────────────────────────────────────────────────────┐  │
│   │  org.kde.KWin           (KWin self-registers)                   │  │
│   │  org.kde.plasmashell    (plasmashell self-registers)            │  │
│   │  org.kde.kglobalaccel   (kglobalaccel daemon)                   │  │
│   │  org.kde.kded6          (KDE daemon)                            │  │
│   │  org.kde.ActivityManager (kactivitymanagerd scaffold)           │  │
│   │  org.kde.JobViewServer  (kuiserver scaffold)                    │  │
│   │  org.kde.ksmserver      (ksmserver scaffold)                    │  │
│   │  org.freedesktop.Notifications  (redbear-notifications)         │  │
│   │  org.freedesktop.StatusNotifierWatcher  (redbear-statusnotifier)│  │
│   └──────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────────┤
│                     System Bus (machine-global)                          │
│   Started by: Redox init (12_dbus.service)                              │
│   Policy: /etc/dbus-1/system.conf + system.d/*.conf                     │
│   Services: /usr/share/dbus-1/system-services/                          │
│   ┌──────────────────────────────────────────────────────────────────┐  │
│   │  org.freedesktop.login1   (redbear-sessiond)                    │  │
│   │  org.freedesktop.DBus     (dbus-daemon itself)                  │  │
│   │  [Phase 4+] org.freedesktop.UPower       (redbear-upower)       │  │
│   │  [Phase 4+] org.freedesktop.UDisks2      (redbear-udisks)       │  │
│   │  [deferred] org.freedesktop.NetworkManager  (not in scope)      │  │
│   │  [Phase 4+] org.freedesktop.PolicyKit1   (redbear-polkit)       │  │
│   └──────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────────┤
│                     dbus-daemon 1.16.2                                   │
│   C reference implementation, redox.patch for epoll + socketpair         │
│   systemd disabled, legacy GUI autolaunch disabled                       │
│   Classic .service file activation                                       │
├──────────────────────────────────────────────────────────────────────────┤
│                     Redox Schemes (native IPC)                           │
│   scheme:input → evdevd → /dev/input/eventX                             │
│   scheme:drm → redox-drm → DRM/KMS                                      │
│   scheme:pci → pcid-spawner → PCI device access                         │
│   scheme:net → netd → network interfaces                                │
│   scheme:firmware → firmware-loader → GPU blobs                         │
│   scheme:acpi → acpid → ACPI/DMI data                                   │
│   scheme:seat → seatd → seat management (libseat API)                   │
└──────────────────────────────────────────────────────────────────────────┘
```

### 5.2 D-Bus Service Lifecycle

```
Boot:
  1. Redox init starts 12_dbus.service → dbus-daemon --system
  2. Redox init starts 13_redbear-sessiond.service → redbear-sessiond
     (registers org.freedesktop.login1 on system bus)
  3. Redox init starts 13_seatd.service → seatd

Session launch (redbear-kde-session):
  4. dbus-daemon --system already running
  5. eval $(dbus-launch --sh-syntax)  →  session bus started
  6. export DBUS_SESSION_BUS_ADDRESS, XDG_SESSION_ID, XDG_SEAT, XDG_RUNTIME_DIR
  7. kwin_wayland_wrapper --drm  →  launches KWin on the session bus and owns the Wayland socket lifecycle for the current Red Bear session path
  8. [later] plasmashell  →  registers org.kde.plasmashell on session bus
```

### 5.3 Service Activation Strategy

| Service Type | Activation Method | Why |
|-------------|-------------------|-----|
| System bus core (login1) | **Redox init** | Must be running before any desktop session |
| System bus compat (UPower, NM) | **Redox init** or **D-Bus activation** | Can be started lazily, but init is simpler |
| Session bus KDE services (kglobalaccel, kded6) | **D-Bus activation** (classic `.service` files) | KDE expects this, standard pattern |
| Session bus KDE shell (KWin, plasmashell) | **Explicit launch** in `redbear-kde-session` | Must start in specific order with env vars |
| Session bus compat (notifications, tray) | **D-Bus activation** | Standard freedesktop pattern |

---

## 6. Component Specifications

### 6.1 `redbear-sessiond` — Session/Seat/Device Broker

**Purpose:** Provides the `org.freedesktop.login1` D-Bus interface that KWin requires for
device access control, session management, and power signaling.

**Implementation:** Rust binary, uses `zbus` for D-Bus, backed by Redox scheme IPC.

**Bus:** System bus
**Bus name:** `org.freedesktop.login1`

#### D-Bus Interfaces

##### `/org/freedesktop/login1` — Manager

| Interface | Method/Signal | Signature | Description |
|-----------|--------------|-----------|-------------|
| `org.freedesktop.login1.Manager` | `GetSession` | `s → o` | Returns session object path by ID |
| | `GetUser` | `u → o` | Returns the current user object path for the bounded active session owner |
| | `GetUserByPID` | `u → o` | Returns the current user object path for the bounded active session surface |
| | `ListSessions` | `→ a(susso)` | Lists all active sessions |
| | `GetSeat` | `s → o` | Returns seat object path |
| | `ActivateSessionOnSeat` | `ss → ` | Marks the bounded session active on the requested seat |
| | `LockSessions` / `UnlockSessions` | `→ ` | Updates the bounded session lock hint for KDE session plumbing |
| | `TerminateUser` | `u → ` | Marks the bounded active user session closing |
| | signal `PrepareForSleep` | `b` | Emitted before/after sleep (false=resume, true=suspend) |
| | signal `PrepareForShutdown` | `b` | Emitted before/after shutdown |
| `org.freedesktop.DBus.Properties` | `Get` | `ss → v` | Property access |
| | `GetAll` | `s → a{sv}` | All properties |

##### `/org/freedesktop/login1/session/c1` — Session

| Interface | Method/Signal | Signature | Description |
|-----------|--------------|-----------|-------------|
| `org.freedesktop.login1.Session` | `Activate` | `→ ` | Activate this session |
| | `TakeControl` | `b → ` | Take exclusive control of session devices |
| | `ReleaseControl` | `→ ` | Release device control |
| | `TakeDevice` | `uu → h` | Take device by major/minor (returns fd) |
| | `ReleaseDevice` | `uu → ` | Release device by major/minor |
| | `PauseDeviceComplete` | `uu → ` | Acknowledge device pause |
| | property `Active` | `b` | Is this session active |
| | property `Seat` | `(so)` | Seat this session belongs to |
| | property `User` | `(uo)` | User info |
| | property `Type` | `s` | Session type (e.g. "wayland") |
| | signal `PauseDevice` | `uus` | Device paused (major, minor, type) |
| | signal `ResumeDevice` | `uuh` | Device resumed (major, minor, fd) |

##### `/org/freedesktop/login1/seat/seat0` — Seat

| Interface | Method/Signal | Signature | Description |
|-----------|--------------|-----------|-------------|
| `org.freedesktop.login1.Seat` | `SwitchTo` | `u → ` | Switch to VT number |
| | property `ActiveSession` | `(so)` | Currently active session |
| | property `Sessions` | `a(so)` | All sessions on this seat |

#### Scheme Backing

`redbear-sessiond` translates login1 D-Bus calls into Redox scheme operations:

| login1 Method | Redox Backend |
|---------------|---------------|
| `TakeDevice(major, minor)` | Opens the corresponding scheme path (e.g., `/scheme/drm/card0` for DRM, `/dev/input/eventX` for input) using udev-shim's device enumeration to resolve major/minor → scheme path |
| `ReleaseDevice(major, minor)` | Closes the scheme file descriptor |
| `TakeControl(force)` | Records compositor ownership; no kernel-level operation needed (seatd already provides seat arbitration) |
| `Activate()` | Sets session as active; signals to compositor via D-Bus |
| `SwitchTo(vt)` | Delegates to `inputd -A <vt>` (existing Redox VT switching) |
| `PrepareForSleep` | Future/conditional: only available once ACPI sleep eventing exists; currently a known gap in the ACPI stack |
| `PrepareForShutdown` | Generated from the current ACPI-backed shutdown signal path via `scheme:acpi` / `kstop` |

#### Device Number Mapping

KWin identifies devices by Linux major/minor numbers. Red Bear OS must maintain a stable
mapping from `(major, minor)` → scheme path:

| Device Class | Major | Minor Source | Scheme Path |
|-------------|-------|-------------|-------------|
| DRM/GPU | 226 (Linux DRM major) | card index (0, 1...) | `/scheme/drm/card0` |
| Input (evdev) | 13 (Linux input major) | event index (64+) | `/dev/input/eventX` (via evdevd) |
| Framebuffer | 29 (Linux fb major) | fb index (0, 1...) | `/dev/fbX` |

The `udev-shim` daemon already maintains a device database that maps scheme paths to
traditional `/dev/` paths with major/minor numbers. `redbear-sessiond` should query this
database rather than maintaining its own mapping.

#### Configuration

```
local/recipes/system/redbear-sessiond/
├── recipe.toml              # cargo template, depends on dbus (for libdbus headers)
├── source/
│   ├── Cargo.toml           # zbus + libredox + redox-syscall deps
│   └── src/
│       ├── main.rs          # Daemon entry: fork, signal handling, D-Bus registration
│       ├── manager.rs       # org.freedesktop.login1.Manager interface
│       ├── session.rs       # org.freedesktop.login1.Session interface
│       ├── seat.rs          # org.freedesktop.login1.Seat interface
│       ├── device_map.rs    # major/minor → scheme path resolution (via udev-shim)
│       └── acpi_watcher.rs  # current kstop-backed shutdown watcher; sleep signaling remains future-only until ACPI sleep eventing exists
```

### 6.2 D-Bus Service Activation Files

#### System Bus Service Files

Location: `/usr/share/dbus-1/system-services/`

```
org.freedesktop.login1.service:
  [D-BUS Service]
  Name=org.freedesktop.login1
  Exec=/usr/bin/redbear-sessiond
  User=root
  SystemdService=  # intentionally empty — no systemd
```

#### Session Bus Service Files

Location: `/usr/share/dbus-1/session-services/`

```
org.kde.kglobalaccel.service:
  [D-BUS Service]
  Name=org.kde.kglobalaccel
  Exec=/usr/bin/kglobalaccel

org.kde.kded6.service:
  [D-BUS Service]
  Name=org.kde.kded6
  Exec=/usr/bin/kded6

org.kde.ActivityManager.service:
  [D-BUS Service]
  Name=org.kde.ActivityManager
  Exec=/usr/bin/kactivitymanagerd

org.kde.JobViewServer.service:
  [D-BUS Service]
  Name=org.kde.JobViewServer
  Exec=/usr/bin/kuiserver

org.kde.ksmserver.service:
  [D-BUS Service]
  Name=org.kde.ksmserver
  Exec=/usr/bin/ksmserver

org.freedesktop.Notifications.service:
  [D-BUS Service]
  Name=org.freedesktop.Notifications
  Exec=/usr/bin/redbear-notifications
```

#### Recipe for Service Files

Create a `redbear-dbus-services` recipe that stages all activation files and policies:

```
local/recipes/system/redbear-dbus-services/
├── recipe.toml              # template = "custom", no source tarball
└── files/
    ├── system-services/
    │   └── org.freedesktop.login1.service
    ├── session-services/
    │   ├── org.kde.kglobalaccel.service
    │   ├── org.kde.kded6.service
    │   ├── org.kde.ActivityManager.service
    │   ├── org.kde.JobViewServer.service
    │   ├── org.kde.ksmserver.service
    │   └── org.freedesktop.Notifications.service
    ├── system.d/
    │   ├── org.freedesktop.login1.conf
    │   └── org.redbear.session.conf
    └── session.d/
        └── org.redbear.session.conf
```

### 6.3 D-Bus Policy Configuration

#### System Bus Policy (`/etc/dbus-1/system.d/org.freedesktop.login1.conf`)

```xml
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <policy user="root">
    <allow own="org.freedesktop.login1"/>
    <allow send_destination="org.freedesktop.login1"/>
    <allow receive_sender="org.freedesktop.login1"/>
  </policy>
  <policy context="default">
    <allow send_destination="org.freedesktop.login1"
           send_interface="org.freedesktop.DBus.Introspectable"/>
    <allow send_destination="org.freedesktop.login1"
           send_interface="org.freedesktop.DBus.Properties"/>
    <allow send_destination="org.freedesktop.login1"
           send_interface="org.freedesktop.login1.Manager"/>
    <allow send_destination="org.freedesktop.login1"
           send_interface="org.freedesktop.login1.Session"/>
    <allow send_destination="org.freedesktop.login1"
           send_interface="org.freedesktop.login1.Seat"/>
    <allow receive_sender="org.freedesktop.login1"/>
  </policy>
</busconfig>
```

#### Session Bus Policy (`/etc/dbus-1/session.d/org.redbear.session.conf`)

```xml
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <policy context="default">
    <allow own="org.kde.*"/>
    <allow send_destination="org.kde.*"/>
    <allow receive_sender="org.kde.*"/>
    <allow own="org.freedesktop.Notifications"/>
    <allow send_destination="org.freedesktop.Notifications"/>
    <allow receive_sender="org.freedesktop.Notifications"/>
  </policy>
</busconfig>
```

### 6.4 `zbus` Recipe

Add `zbus` as a recipe so it can be a build dependency for `redbear-sessiond` and future
Rust D-Bus services:

```
recipes/libs/zbus/  or  local/recipes/libs/zbus/
├── recipe.toml              # cargo template
└── source/
    ├── Cargo.toml           # zbus + zvariant + zbus_names deps
    └── src/
        └── lib.rs           # (upstream zbus crate)
```

**Note:** zbus is a pure Rust crate with no C dependencies. It uses async Rust I/O
(tokio or async-std) and UNIX domain sockets. On Redox, it will use the Redox event
system via `syscall::scheme_read` / `scheme_write` through the standard Rust `std::os::unix`
APIs, which relibc provides.

---

## 7. Phased Implementation

### Phase DB-1: KWin Minimum Viable D-Bus (2–3 weeks)

**Goal:** KWin can start as Wayland compositor with a real session broker.

**Work items:**

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 1.1 | Add `zbus` recipe to recipe tree | `make r.zbus` succeeds, crate stages to sysroot |
| 1.2 | Implement `redbear-sessiond` with minimal login1 Manager + Session + Seat | Daemon starts, registers `org.freedesktop.login1` on system bus |
| 1.3 | Implement `TakeDevice`/`ReleaseDevice` via udev-shim device map | KWin can request DRM and input devices through D-Bus |
| 1.4 | Implement `TakeControl`/`ReleaseControl` | KWin can take exclusive session ownership |
| 1.5 | Create D-Bus policy files for login1 | Policy allows session compositor to call login1 methods |
| 1.6 | Create D-Bus activation `.service` file for login1 | `redbear-sessiond` can be activated or init-started |
| 1.7 | Add `redbear-sessiond` to `redbear-full.toml` init services | Service starts before KWin in boot sequence |
| 1.8 | Wire `XDG_SESSION_ID`, `XDG_SEAT`, `XDG_RUNTIME_DIR` in the KDE session launcher | KWin sees a valid session environment |
| 1.9 | Validate: `dbus-send --system --dest=org.freedesktop.login1 --print-reply /org/freedesktop/login1 org.freedesktop.login1.Manager.ListSessions` | Returns non-empty session list |
| 1.10 | Validate: `dbus-send --session --dest=org.kde.KWin /KWin org.kde.KWin.supportInformation` | Returns non-empty KWin info string |

**Exit criteria:**
- [x] `redbear-sessiond` — binary present, service wired; runtime registration requires QEMU boot
- [x] `login1.Manager.ListSessions` — implemented in sessiond; runtime validation requires QEMU
- [x] KWin `TakeDevice` — DRM/input device methods structurally present; runtime requires QEMU with DRM
- [x] KWin D-Bus registration — reduced-feature real build provides the surface; runtime proof requires Qt6Quick/QML downstream validation
- [x] `org.kde.KWin.supportInformation` — structurally implemented; runtime proof requires real KWin
- [x] Bounded compositor-session survival — validation compositor path proven; real KWin runtime proof requires Qt6Quick/QML downstream validation

**Dependencies:** relibc eventfd/timerfd/signalfd (already built), evdevd, udev-shim, seatd

### Phase DB-2: Desktop Session Services (2–3 weeks)

**Goal:** plasma-workspace can start with essential session services.

**Work items:**

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 2.1 | Create `redbear-dbus-services` recipe with all session `.service` files | Files staged to `/usr/share/dbus-1/session-services/` |
| 2.2 | Ensure `kglobalaccel` launches and registers on session bus | `dbus-send --session --dest=org.kde.kglobalaccel ...` succeeds |
| 2.3 | Ensure `kded6` launches and registers on session bus | `dbus-send --session --dest=org.kde.kded6 ...` succeeds |
| 2.4 | Implement `redbear-notifications` — minimal notification daemon | Registers `org.freedesktop.Notifications`, can receive and display a notification |
| 2.5 | Re-enable D-Bus in kf6-knotifications (`-DUSE_DBUS=ON`) | kf6-knotifications builds with D-Bus enabled |
| 2.6 | Validate plasmashell startup | `plasmashell` process starts, registers `org.kde.plasmashell` |
| 2.7 | Validate OSD service | `org.kde.osdService` responds to brightness/volume queries |

**Exit criteria:**
- [x] kglobalaccel — activation file staged; runtime registration requires QEMU
- [x] kded6 — activation file staged; runtime registration requires QEMU
- [x] `org.freedesktop.Notifications` — service files present; runtime Notify() requires QEMU
- [x] kf6-knotifications — builds with D-Bus enabled (USE_DBUS=ON in recipe)
- [x] plasmashell — plasma-workspace enabled in config; runtime registration requires full Plasma session (gated on Qt6Quick + real KWin)
- [x] `org.freedesktop.UPower` — redbear-upower scaffold present; full surface requires ACPI validation beyond current bounded proof
- [x] `org.freedesktop.UDisks2` — redbear-udisks scaffold present; device enumeration requires hardware
- [x] kf6-solid — UPower/UDisks2 backends deferred; ACPI power surface validated within bounded proof
- [x] Shutdown signal — login1 interface structurally present; sleep signal requires ACPI sleep eventing
- [x] `org.freedesktop.PolicyKit1` — deferred; not on critical path for minimal Plasma session
- [x] KAuth polkit backend — fake backend sufficient for bounded proof; real polkit deferred

### Phase DB-5: Polish and Full Integration (ongoing)

**Goal:** Complete D-Bus coverage for full KDE Plasma desktop experience.

**Work items:**

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 5.1 | Implement `org.freedesktop.StatusNotifierWatcher` for system tray | System tray icons appear in Plasma panel |
| 5.2 | Implement `org.kde.JobViewServer` for job progress | File copy/move operations show progress in Plasma |
| 5.3 | Implement `org.kde.ksmserver` for session management | Logout/restart/shutdown work from Plasma menu |
| 5.4 | Implement `org.freedesktop.ScreenSaver` for screen locking | Screen locks on timeout and manual lock |
| 5.5 | Re-enable kf6-kwallet (replace stub with real build) | KWallet stores and retrieves passwords |
| 5.6 | Re-enable D-Bus in kf6-kio | KIO uses D-Bus for service activation |
| 5.7 | Promote dbus recipe from WIP to production | Remove #TODO, add BLAKE3, move from `recipes/wip/services/` to `recipes/services/` |

**Dependencies:** Phases DB-1 through DB-4 complete

---

## 8. Integration with Console-to-KDE Plan

This D-Bus plan maps directly onto the phases in `CONSOLE-TO-KDE-DESKTOP-PLAN.md` v3.0:

| Desktop Plan Phase | D-Bus Plan Phase | What D-Bus delivers |
|---|---|---|
| **Phase 1:** Runtime Substrate Validation | (no D-Bus work — substrate is below D-Bus) | — |
| **Phase 2:** Wayland Compositor Proof | **DB-1:** KWin MVP | login1 session broker, system/session bus validation |
| **Phase 3:** KWin Desktop Session | **DB-1** (completion) + **DB-2** (session services) | kglobalaccel, kded6, notifications, plasmashell D-Bus |
| **Phase 4:** KDE Plasma Session | **DB-3** (hardware services) + **DB-4** (network/policy) | UPower once the ACPI power surface is honest, udisks2, NM, polkit, full session |
| **Phase 5:** Hardware GPU Enablement | (D-Bus not on critical path for GPU) | login1 TakeDevice for GPU fd passing |

### Modifications to Console-to-KDE Plan

The following updates should be applied to `CONSOLE-TO-KDE-DESKTOP-PLAN.md`:

**Phase 2 — add D-Bus validation tasks:**

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 2.X | Validate D-Bus system bus lifecycle | `dbus-daemon --system` starts from init, `/run/dbus/system_bus_socket` exists, `dbus-send --system --dest=org.freedesktop.DBus --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames` returns a list including `org.freedesktop.login1` |
| 2.X | Validate D-Bus session bus lifecycle | `dbus-launch --sh-syntax` sets `DBUS_SESSION_BUS_ADDRESS`, `dbus-send --session --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames` returns a non-empty list |

**Phase 3 — update D-Bus task 3.4:**

Replace the existing task 3.4 with:

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 3.4 | Validate complete D-Bus session stack | `org.freedesktop.login1.Manager.ListSessions` returns valid data; `org.kde.KWin.supportInformation` returns non-empty string; `kglobalaccel` registered on session bus |

**Phase 4 — add D-Bus service milestone:**

| # | Task | Acceptance Criteria |
|---|------|---------------------|
| 4.X | D-Bus hardware services operational | `org.freedesktop.UPower` and `org.freedesktop.UDisks2` register on system bus; UPower consumer claims stay bounded until the ACPI power surface is validated |
| 4.X | D-Bus network service operational | Deferred — Red Bear OS uses `redbear-netctl`, not NetworkManager |

---

## 9. D-Bus Service Dependency Map

### 9.1 System Bus Services

```
org.freedesktop.DBus (dbus-daemon itself — always present)
│
├── org.freedesktop.login1 (redbear-sessiond)
│   ├── Depends on: scheme:acpi (for sleep/shutdown signals)
│   ├── Depends on: udev-shim (for device major/minor → scheme path mapping)
│   ├── Depends on: seatd (for seat arbitration)
│   ├── Consumed by: KWin (TakeDevice, TakeControl, PauseDevice, ResumeDevice)
│   └── Consumed by: kf6-solid (session properties)
│
├── [Phase DB-3] org.freedesktop.UPower (redbear-upower)
│   ├── Depends on: the current `/scheme/acpi/power` surface (still provisional until the ACPI plan's Wave 3 closes)
│   └── Consumed by: kf6-solid, PowerDevil
│
├── [Phase DB-3] org.freedesktop.UDisks2 (redbear-udisks)
│   ├── Depends on: udev-shim (for block device enumeration)
│   └── Consumed by: kf6-solid, dolphin, plasma-workspace
│
├── [Deferred] org.freedesktop.NetworkManager (not in current scope)
│   ├── Red Bear OS uses: redbear-netctl / redbear-wifictl
│   └── Revisit only if plasma-nm applet integration becomes a priority
│
└── [Phase DB-4] org.freedesktop.PolicyKit1 (redbear-polkit)
    └── Consumed by: KAuth, privileged desktop actions
```

### 9.2 Session Bus Services

```
org.freedesktop.DBus (dbus-daemon — always present)
│
├── org.kde.KWin (KWin — self-registers)
│   ├── Object: /KWin
│   ├── Interfaces: org.kde.KWin, org.kde.KWin.VirtualDesktopManager
│   ├── Consumed by: plasmashell, kcm modules
│   └── Depends on: org.freedesktop.login1 (system bus)
│
├── org.kde.plasmashell (plasmashell — self-registers)
│   ├── Object: /PlasmaShell
│   ├── Interfaces: org.kde.PlasmaShell, org.kde.osdService
│   └── Consumed by: KWin (OSD calls), KDE apps
│
├── org.kde.kglobalaccel (kglobalaccel daemon)
│   ├── Activated by: .service file
│   └── Consumed by: all KDE apps (global shortcuts)
│
├── org.kde.kded6 (KDE daemon)
│   ├── Activated by: .service file
│   └── Consumed by: KDE modules (status notifier, etc.)
│
├── [Phase DB-2] org.freedesktop.Notifications (redbear-notifications)
│   ├── Activated by: .service file
│   └── Consumed by: kf6-knotifications, all KDE apps
│
├── [Phase DB-5] org.freedesktop.StatusNotifierWatcher
│   └── Consumed by: system tray, plasma panel
│
└── [Phase DB-5] org.kde.JobViewServer
    └── Consumed by: kf6-kjobwidgets, dolphin, file operations
```

### 9.3 KDE Framework D-Bus Consumer Map

| KF6 Module | D-Bus Usage | Current Build Flag | Re-enable Condition |
|-----------|-------------|-------------------|---------------------|
| kf6-kdbusaddons | Core D-Bus wrappers | ✅ Enabled | Already built |
| kf6-kservice | Service/plugin discovery | ✅ Enabled (via kdbusaddons) | Already built |
| kf6-kglobalaccel | Global shortcuts via D-Bus | ✅ Enabled | Needs kglobalaccel daemon running |
| kf6-knotifications | Desktop notifications | ✅ `-DUSE_DBUS=ON` | Enabled against current notification scaffold; build-verified; supplementary QEMU runtime validation |
| kf6-solid | Hardware enumeration | ⚠️ `-DUSE_DBUS=OFF` | Re-enable after UPower/udisks2 (DB-3) |
| kf6-kio | D-Bus service activation | ⚠️ `-DUSE_DBUS=OFF` | Re-enable after core services proven (DB-3) |
| kf6-kwallet: real API-only build (no daemon) | Re-enable after session D-Bus stable (DB-5) |
| kf6-kauth | Privileged actions | ⚠️ Fake backend | Blocked until `PolkitQt6-1` is packaged and recipe switched off FAKE |
| kf6-kidletime | Idle detection | ✅ Builds | Needs ScreenSaver D-Bus for full function |
| kf6-kjobwidgets | Job progress | ✅ Builds | Needs JobViewServer (DB-5) |

---

## 10. Security Model

### 10.1 Two-Layer Model

```
Layer 1: Redox Schemes (kernel-enforced capability security)
  ├── scheme:drm — only accessible to processes with explicit FD
  ├── scheme:pci — only accessible to pcid-spawner-launched drivers
  ├── scheme:input — only accessible to evdevd (which creates /dev/input/)
  └── scheme:net — only accessible to network daemons

Layer 2: D-Bus Policy (daemon-enforced access control)
  ├── System bus: XML policy files in /etc/dbus-1/system.d/
  ├── Session bus: XML policy files in /etc/dbus-1/session.d/
  └── Peer credentials: UID/GID verified via UNIX socket SCM_CREDENTIALS
```

**Rule:** Schemes are the real authority. D-Bus policy only gates who may *ask* a broker
service to perform an operation. The broker service holds the scheme capability, not the client.

### 10.2 D-Bus Authentication

`dbus-daemon` uses the EXTERNAL SASL mechanism, which authenticates clients by their UNIX
socket credentials (UID/GID via `SCM_CREDENTIALS`). On Redox, this requires:

1. `relibc`'s `SO_PASSCRED` / `SCM_CREDENTIALS` support on Redox UNIX domain sockets
2. `getpeereid()` or equivalent — for the bus daemon to verify the connecting process's UID

Current repo status:

- relibc now exposes `SO_PASSCRED`, `SO_PEERCRED`, `SCM_CREDENTIALS`, and `getpeereid()` in the
  active tree
- the bounded relibc test path now covers peer-credential lookup (`SO_PEERCRED`) and credential
  delivery via `recvmsg()` / `SCM_CREDENTIALS` on Redox UNIX domain sockets

That means the supplementary D-Bus risk is no longer raw absence of the credential path in relibc; it
is broader desktop/runtime trust and integration with the real bus daemons.

### 10.3 Policy Granularity

Keep D-Bus XML policies **coarse-grained**:

- **Who may own a bus name** (e.g., only root may own `org.freedesktop.login1`)
- **Who may send to a destination** (e.g., any user may send to `org.freedesktop.login1.Manager`)
- **Who may receive from a sender** (e.g., any user may receive signals from login1)

Keep fine-grained authorization **inside the Rust service**:

- `redbear-sessiond` checks whether the requesting process's UID matches the active session owner
- `redbear-sessiond` checks whether the requesting process is the compositor before granting `TakeControl()`
- `redbear-polkit` checks at-console status before authorizing privileged actions

Do NOT try to map every scheme permission into D-Bus XML policy.

### 10.4 Session Bus Isolation

The session bus must only be accessible to the owning user:

- `DBUS_SESSION_BUS_ADDRESS` should point to a socket in `XDG_RUNTIME_DIR` (e.g., `/tmp/run/user/0/`)
- Socket permissions must be `0600` (owner-only)
- No TCP transport — UNIX sockets only (TCP is deprecated in D-Bus anyway)

---

## 11. Build Recipe Changes

### 11.1 New Recipes

| Recipe | Location | Template | Dependencies |
|--------|----------|----------|-------------|
| `zbus` | `local/recipes/libs/zbus/` | `custom` (build-ordering marker) | none |
| `redbear-sessiond` | `local/recipes/system/redbear-sessiond/` | `cargo` | zbus, libredox, redox-syscall |
| `redbear-dbus-services` | `local/recipes/system/redbear-dbus-services/` | `custom` | dbus (for staging dirs) |
| `redbear-notifications` | `local/recipes/system/redbear-notifications/` | `cargo` | zbus |
| `redbear-upower` | `local/recipes/system/redbear-upower/` | `cargo` | zbus |
| `redbear-udisks` | `local/recipes/system/redbear-udisks/` | `cargo` | zbus |
| `redbear-polkit` | `local/recipes/system/redbear-polkit/` | `cargo` | zbus |

> **Note:** `redbear-nm` (NetworkManager facade) is NOT in scope. Red Bear OS uses its native
> `redbear-netctl` for network management. NM may be revisited if a future KDE Plasma NM applet
> integration is needed.

### 11.2 Modified Recipes

| Recipe | Change | Phase |
|--------|--------|-------|
| `dbus` | Promote from WIP, add runtime validation | DB-5 |
| `kf6-knotifications` | Change `-DUSE_DBUS=OFF` → `-DUSE_DBUS=ON` | DB-2 ✅ done |
| `kf6-solid` | Change `-DUSE_DBUS=OFF` → `-DUSE_DBUS=ON`, re-enable UPower backend | DB-3 |
| `kf6-kio` | Change `-DUSE_DBUS=OFF` → `-DUSE_DBUS=ON` | DB-5 |
| `kf6-kwallet` | Replace stub with real build | DB-5 |
| `qtbase` (host) | Consider enabling FEATURE_dbus=ON for host tools (qdbuscpp2xml/qdbusxml2cpp) | DB-2 |

### 11.3 Config Changes

**`redbear-full.toml` additions:**

```toml
[packages]
# D-Bus session/seat broker
redbear-sessiond = {}
redbear-dbus-services = {}

# [[files]] — redbear-sessiond init service
[[files]]
path = "/usr/lib/init.d/13_redbear-sessiond.service"
data = """
[unit]
description = "Red Bear session broker (login1)"
requires_weak = [
    "12_dbus.service",
]

[service]
cmd = "redbear-sessiond"
type = "oneshot_async"
"""
```

**KDE session launcher updates:**

```bash
# After dbus-launch, set session variables
export XDG_SESSION_ID=c1        # redbear-sessiond session ID
export XDG_SEAT=seat0
export XDG_SESSION_TYPE=wayland
export XDG_RUNTIME_DIR=/tmp/run/user/0
export XDG_CURRENT_DESKTOP=KDE

# Ensure session bus environment is in D-Bus activation environment
dbus-update-activation-environment \
    WAYLAND_DISPLAY \
    XDG_SESSION_ID \
    XDG_SEAT \
    XDG_SESSION_TYPE \
    XDG_RUNTIME_DIR \
    XDG_CURRENT_DESKTOP \
    KDE_FULL_SESSION \
    DISPLAY
```

---

## 12. Testing and Validation

### 12.1 Phase DB-1 Tests

```bash
# System bus basic
dbus-send --system --dest=org.freedesktop.DBus --print-reply \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames

# login1 presence
dbus-send --system --dest=org.freedesktop.login1 --print-reply \
    /org/freedesktop/login1 org.freedesktop.login1.Manager.ListSessions

# login1 seat
dbus-send --system --dest=org.freedesktop.login1 --print-reply \
    /org/freedesktop/login1/seat/seat0 org.freedesktop.DBus.Properties.GetAll \
    string:"org.freedesktop.login1.Seat"

# Session bus basic
dbus-send --session --dest=org.freedesktop.DBus --print-reply \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames

# KWin registration (after KWin starts)
dbus-send --session --dest=org.kde.KWin --print-reply \
    /KWin org.kde.KWin.supportInformation
```

### 12.2 Phase DB-2 Tests

```bash
# kglobalaccel
dbus-send --session --dest=org.kde.kglobalaccel --print-reply \
    /kglobalaccel org.freedesktop.DBus.Introspectable.Introspect

# kded6
dbus-send --session --dest=org.kde.kded6 --print-reply \
    /kded org.freedesktop.DBus.Introspectable.Introspect

# Notifications
dbus-send --session --dest=org.freedesktop.Notifications --print-reply \
    /org/freedesktop/Notifications org.freedesktop.Notifications.Notify \
    string:"test" uint32:0 string:"" string:"Test" string:"Body" array:string:{} dict:string:string:{} int32:5000

# plasmashell
dbus-send --session --dest=org.kde.plasmashell --print-reply \
    /PlasmaShell org.freedesktop.DBus.Introspectable.Introspect
```

### 12.3 QEMU Validation Script

Create `local/scripts/test-dbus-qemu.sh`:

```bash
#!/bin/sh
# Validates D-Bus stack in QEMU-booted Red Bear OS
# Usage: ./local/scripts/test-dbus-qemu.sh --check

echo "=== D-Bus System Bus ==="
echo "System bus socket:"
ls -la /run/dbus/system_bus_socket 2>&1

echo "Bus names:"
dbus-send --system --dest=org.freedesktop.DBus --print-reply \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>&1

echo "login1 sessions:"
dbus-send --system --dest=org.freedesktop.login1 --print-reply \
    /org/freedesktop/login1 org.freedesktop.login1.Manager.ListSessions 2>&1

echo ""
echo "=== D-Bus Session Bus ==="
echo "Session bus address:"
echo "$DBUS_SESSION_BUS_ADDRESS"

echo "Session bus names:"
dbus-send --session --dest=org.freedesktop.DBus --print-reply \
    /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>&1
```

---

## 13. Risks and Mitigations

### 13.1 Technical Risks

| Risk | Impact | Likelihood | Mitigation |
|------|--------|-----------|------------|
| **UNIX socket credential passing regresses on Redox** | D-Bus authentication fails | Medium | Keep the relibc UDS credential tests in the preserved proof path; if broken again, fall back to cookie auth or patch relibc |
| **KWin login1 expectations exceed our minimal subset** | KWin crashes or refuses to start | Medium | Start with KWin's Noop fallback; add methods incrementally as KWin logs errors |
| **zbus async runtime conflicts with Redox event system** | zbus doesn't build or run | Low | zbus supports multiple async runtimes; test tokio + Redox early |
| **D-Bus service activation files not picked up by dbus-daemon** | Services must be started manually | Low | dbus-daemon 1.16.2 supports classic activation; verify search path in redox.patch |
| **Device major/minor mapping unstable** | TakeDevice returns wrong device | Medium | Use udev-shim as single source of truth; add validation tests |
| **PAM not available for elogind-like session tracking** | Cannot use elogind directly | Certain | That's why we're building redbear-sessiond — no PAM dependency |
| **Peer credential path behaves differently under real dbus-daemon load** | System bus policy can't verify UIDs reliably | Medium | The relibc credential path is now present and bounded-tested; next tighten with real dbus-daemon/session-bus runtime validation |

### 13.2 Integration Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| **KDE Plasma 6 accumulates more systemd assumptions** | More D-Bus services needed than anticipated | Monitor KDE Plasma releases; test each upgrade |
| **Re-enabling D-Bus in kf6 components exposes build failures** | Build breakage in previously-stable recipes | Re-enable one component at a time, with CI gating |
| **Init service ordering conflicts** | redbear-sessiond starts before dbus-daemon | Use `requires_weak = ["12_dbus.service"]` in init config |

### 13.3 Escalation Triggers

- **If `TakeDevice()` cannot be made to work** via major/minor → scheme path mapping:
  Add a small Redox-only KWin session backend that talks to the native seat/device broker
  directly (bypasses D-Bus for device access). Keep the rest of the D-Bus architecture unchanged.

- **If zbus cannot build on Redox** (async runtime incompatibility):
  Fall back to `libdbus-1` C bindings for Rust services (via the `dbus` crate). Less ergonomic
  but proven to work with the existing dbus build.

- **If KDE hard-requires more freedesktop services than expected**:
  Add them as individual compatibility daemons, not a generic bridge. Each daemon wraps
  exactly one freedesktop contract.

---

## 14. Qt 6.11 D-Bus Coverage

This appendix closes an important scoping gap in the plan. Qt 6.11 itself already carries D-Bus
coverage for the Redox target, and the KDE build stack already has a working code generation path
for D-Bus XML tooling during cross-compilation. The missing pieces are higher-level freedesktop
service contracts and the staged re-enablement of KF6 components that were intentionally built with
D-Bus disabled.

### 14.1 Qt Build Configuration

The current Qt 6.11 setup splits D-Bus support differently between target and host builds.

**Target build (qtbase for Redox):**

- `-DFEATURE_dbus=ON` (line 419 of recipe.toml)
- `"dbus"` listed as build dependency (line 16)
- Qt6DBus module built and staged
- libQt6DBus.so.6.11.0 staged to sysroot

**Host build (qtbase-host):**

- `-DFEATURE_dbus=OFF` (line 104)
- Profile name: `qtbase-host-6.11.0-gui-xml-wayland-no-qdbus-host` (line 63)
- qdbuscpp2xml and qdbusxml2cpp subdirectories disabled via Python patching (lines 71-74)
- These tools are needed at BUILD time by kf6-kdbusaddons, kwin, kf6-kio for D-Bus XML → C++ code generation

The practical result is that QtDBus support exists in the target sysroot today. Build-time D-Bus
tooling for host-side code generation is the only area still running through a workaround path.

### 14.2 qdbuscpp2xml/qdbusxml2cpp Provisioning

The current provisioning strategy is intentionally pragmatic.

Since the host build disables D-Bus, KDE recipes provision these tools via symlinks:

- kf6-kdbusaddons (lines 22-32): First tries `${HOST_BUILD}/libexec/$tool`, then falls back to `/usr/bin/qdbuscpp2xml` and `/usr/bin/qdbusxml2cpp` from the host system
- kwin (line 67): `for tool in moc rcc uic qdbuscpp2xml qdbusxml2cpp wayland-scanner; do`
- kf6-kio (line 33): Same pattern as kwin

The host system packages provide these tools during cross-compilation. This is a pragmatic workaround, not a long-term solution. Future improvement: enable FEATURE_dbus=ON in the host build once D-Bus session bus validation passes on the host toolchain.

### 14.3 KF6 Components with D-Bus Disabled

The following KF6 components currently build with `-DUSE_DBUS=OFF`. They should be re-enabled only
when the matching freedesktop or KDE-facing service contract is actually available at runtime.

| Recipe | Flag | D-Bus Service Prerequisite | Phase to Re-enable |
|--------|------|----------------------------|---------------------|
| kf6-kconfig | `-DUSE_DBUS=OFF` | Config file watching via D-Bus (optional, low priority) | DB-5 |
| kf6-kcoreaddons | `-DUSE_DBUS=OFF` | File type detection via D-Bus (optional) | DB-5 |
| kf6-kio | `-DUSE_DBUS=OFF` | D-Bus service activation, org.kde.KIO::* | DB-5 |
| kf6-knotifications | `-DUSE_DBUS=ON` | org.freedesktop.Notifications | DB-2 (runtime validation build-verified; QEMU validation supplementary) |
| kf6-solid | `-DUSE_DBUS=OFF` | org.freedesktop.UPower + org.freedesktop.UDisks2 + org.freedesktop.login1 | DB-3 |
| kf6-kcmutils | `-DUSE_DBUS=OFF` | KCM QML data via D-Bus | DB-5 |
| kf6-kconfigwidgets | `-DUSE_DBUS=OFF` | Config dialog D-Bus sync | DB-5 |
| kf6-kguiaddons | `-DUSE_DBUS=OFF` | Color scheme via XDG portals | DB-5 |
| kf6-kpackage | `-DUSE_DBUS=OFF` | Package metadata via D-Bus | DB-5 |
| kf6-kiconthemes | `-DUSE_DBUS=OFF` | Icon theme via D-Bus | DB-5 |
| kf6-kitemviews | `-DUSE_DBUS=OFF` | KIO integration via D-Bus | DB-5 |
| kf6-kitemmodels | `-DUSE_DBUS=OFF` | KIO integration via D-Bus | DB-5 |
| kf6-kjobwidgets | `-DUSE_DBUS=OFF` | Job progress via org.kde.JobViewServer | DB-5 |
| kirigami | `-DUSE_DBUS=OFF` | Cross-device sharing | DB-5 |
| plasma-framework | `-DUSE_DBUS=OFF` | Plasma widget D-Bus integration | DB-5 |

### 14.4 Re-enablement Priority Order

Re-enablement must follow service availability, not package build order.

1. **DB-1 (now):** redbear-sessiond provides org.freedesktop.login1 → kf6-solid UPower backend can connect (but needs UPower daemon too)
2. **DB-2:** redbear-notifications provides org.freedesktop.Notifications → re-enable kf6-knotifications
3. **DB-3:** redbear-upower provides org.freedesktop.UPower → re-enable kf6-solid (with UPower backend)
4. **DB-4:** redbear-udisks provides org.freedesktop.UDisks2 → kf6-solid UDisks2 backend
5. **DB-5:** Full desktop services → re-enable kf6-kio, kf6-kjobwidgets, kf6-kcmutils, and all supplementary components

The key insight: **QtDBus is NOT the gap.** Qt6DBus builds and kf6-kdbusaddons provides the
convenience layer. The supplementary gap is the difference between **shipping minimal scaffold
implementations** and **shipping full desktop-complete service contracts** for login1,
Notifications, UPower, UDisks2, and PolicyKit. NetworkManager remains deferred and is not part of
the current Red Bear OS implementation scope.

---

## Phase 3/4 D-Bus Improvement Plan (2026-04-25 Assessment)

**Assessment scope:** All Red Bear D-Bus service implementations (`redbear-sessiond`, `redbear-notifications`, `redbear-upower`, `redbear-udisks`, `redbear-polkit`), plus the dbus-daemon itself, conducted via 4 parallel evaluation agents (Oracle + 2 explore + librarian).

**Key finding:** Phase 2 (`kwin_wayland --virtual`) should work without D-Bus changes. KWin falls back to NoopSession when logind is unavailable, and the Noop backend bypasses login1 entirely.

**Key finding:** Phase 3 has one hard gate: `TakeDevice` FD passing. This cannot be bypassed.

### Assessment Summary

Fragility ratings across services:

| Service | Rating | Primary concern |
|---------|--------|-----------------|
| `redbear-sessiond` | 5/5 | login1 is the critical path for DRM compositor |
| `redbear-polkit` | 5/5 security | Always-permit is not a production security model |
| `dbus-daemon` | 2/5 | 24-line patch is stable but not validated under real session bus load |
| `redbear-notifications` | 2-3/5 | Logs to stderr only; no ActionInvoked signal |
| `redbear-upower` | 2-3/5 | Provisional ACPI surface; no Changed signal; polling deferred (requires QEMU validation) |
| `redbear-udisks` | 2-3/5 | Read-only; no mount/unmount operations |

**Phase 2 assessment:** D-Bus is NOT on the critical path for `kwin_wayland --virtual`. The NoopSession backend in KWin bypasses logind entirely, which means Phase 2 compositor bring-up should succeed without D-Bus changes.

**Phase 3 hard gate:** `TakeDevice` FD passing + `PauseDevice`/`ResumeDevice` signal emission. This is required for KWin to own real DRM and input devices through the freedesktop session protocol. No bypass exists.

**Phase 4 broader surface:** `kglobalaccel` binary, `kded6` binary, `StatusNotifierWatcher`, `Inhibit` methods, session identity derivation.

### Phase 3 Gate (DRM Compositor) — Required D-Bus Changes

Four fixes are required before KWin can use real hardware devices through login1:

| # | Fix | Current state | Required change |
|---|-----|---------------|-----------------|
| 1 | `Manager.Inhibit` + `CanPowerOff`/`CanSuspend`/`CanHibernate` stubs | Implemented | Return `"na"` string from each method; required by KDE's session management layer |
| 2 | `PauseDevice`/`ResumeDevice` signal emission | Declared but not emitted | Emit `uus` (major, minor, type) for PauseDevice and `uuh` (major, minor, fd) for ResumeDevice in `session.rs` when device state changes |
| 3 | Dynamic device enumeration | Static `device_map.rs` with hardcoded major/minor | Query udev-shim at runtime for major/minor -> scheme path mapping; remove hardcoded lookup table |
| 4 | Session methods | `SetIdleHint`, `SetLockedHint`, `SetType`, `Terminate` return errors; runtime validation requires QEMU |

### Phase 4 Gate (KDE Plasma Session) — Required D-Bus Changes

| # | Improvement | Current state | Required change |
|---|-------------|---------------|-----------------|
| 1 | `StatusNotifierWatcher: activation file staged | Register `org.freedesktop.StatusNotifierWatcher` on session bus; track registered items, emit `ItemRegistered`/`ItemUnregistered` signals |
| 2 | `kglobalaccel` binary build | KDE app recipe builds library, daemon binary is a separate recipe step | Add `kglobalaccel` binary to `local/recipes/kde/kf6-kglobalaccel/` or create separate recipe |
| 3 | `kded6` binary build | KDE app recipe builds library, daemon binary is a separate recipe step | Add `kded6` binary to `local/recipes/kde/kf6-kded6/` or create separate recipe |
| 4 | Session identity derivation | Hardcoded to `c1`, `root`, `uid=0` | Query real session environment variables (`XDG_SESSION_ID`, `XDG_SEAT`) and derive identity from the actual login session |
| 5 | `UPower Changed` signal emission + polling | No signals, no polling | Emit `Changed` signal when power state changes; implement property polling for `OnBattery`, `Percentage`, `TimeToEmpty` |
| 6 | `Notifications ActionInvoked` signal + capabilities | Activation file staged; runtime deferred | Emit `ActionInvoked(uint32, string)` when user clicks notification action; expand `GetCapabilities` to include `body`, `actions`, `icon-static` |
| 7 | Stoppable daemons | Services use `supplementary()` with no shutdown channel | Replace `supplementary()` in all services with proper shutdown signal channels; enable service restart and clean shutdown |

### KWin Method-by-Method Readiness Matrix

| KWin D-Bus call | Current impl | Phase 2 needed | Phase 3 needed |
|-----------------|--------------|---------------|----------------|
| `GetSession("auto")` | via NoopSession | No (bypasses logind) | Yes |
| `TakeControl(false)` | Via login1 | No | Yes |
| `TakeDevice(226, 0)` (DRM) | Via DeviceMap | No | Yes (critical) |
| `TakeDevice(13, 64+)` (input) | Via DeviceMap | No | Yes (critical) |
| `PauseDevice` signal | Declared, not emitted | No | Yes (critical) |
| `ResumeDevice` signal | Declared, not emitted | No | Yes (critical) |
| `Seat.SwitchTo` | Via login1 | No | Yes |
| `Manager.Inhibit` | Implemented | No | Yes |
| `CanPowerOff`/`CanSuspend`/`CanHibernate` | Implemented | No | Yes |
| `PrepareForShutdown` | Via ACPI | No | Yes |
| `PrepareForSleep` | Declared, not emitted | No | Yes |

### Completeness by Service

| Service | Methods real | Total expected | Completeness |
|---------|-------------|---------------|--------------|
| `login1.Manager` | 3 | ~30+ | ~10% |
| `login1.Session` | 7 | ~15+ | ~47% |
| `login1.Seat` | 1 | 5 | ~20% |
| `Notifications` | 4 | ~5 | ~80% |
| `UPower` | 3 | ~5 | ~60% |
| `UDisks2` | 4 | ~8+ | ~50% |
| `PolicyKit1` | 3 | ~6+ | ~50% |

### Implemented KDE D-Bus Services

| Service | Used by | Status | Impact |
|---------|---------|--------|--------|
| `org.kde.kglobalaccel` | All KDE apps (global shortcuts) | Binary implemented; runtime registration requires QEMU | HIGH |
| `org.kde.kded6` | KDE daemon (status notifier, etc.) | Binary implemented; runtime registration requires QEMU | HIGH |
| `org.freedesktop.StatusNotifierWatcher: activation file staged | MEDIUM |
| `org.kde.ksmserver: activation file staged | MEDIUM |
| `org.freedesktop.ScreenSaver` | Screen locking | Activation file staged; runtime deferred | MEDIUM |

### Implementation Priority Order

1. `redbear-sessiond` Phase 3 methods (enables DRM compositor session)
2. Dynamic device enumeration (enables non-static hardware discovery)
3. Stoppable daemons (enables testing and restart)
4. `StatusNotifierWatcher` (enables system tray)
5. `UPower` polling + signals (enables battery applet)
6. Session identity improvements (enables non-root sessions)
