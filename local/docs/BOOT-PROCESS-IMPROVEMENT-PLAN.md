# Red Bear OS — Boot Process Improvement Plan

**Version:** 1.0 — 2026-04-27
**Status:** Active — supersedes ad-hoc boot fixes and replaces historical P0–P6 boot notes
**Canonical plans:** `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0), `local/docs/GREETER-LOGIN-IMPLEMENTATION-PLAN.md`
**Diagnosis:** `local/docs/BOOT-PROCESS-ASSESSMENT.md` (Phase 7 kernel RAM hang + ISO organization)

---

## 1. Target Contract

| Profile | Required boot outcome | Current state | Gap |
|---------|----------------------|---------------|-----|
| `redbear-full` | **Graphical Wayland greeter → KDE desktop session** | Text login only; KWin uses virtual backend | Three blockers |
| `redbear-mini` | **Text login** | ✅ Working | None |
| `redbear-grub` | **Text login** | ✅ Working | None |

---

## 2. Current Boot Reality (2026-04-27 Diagnosis)

### What works

- UEFI bootloader → kernel → init phase 1/2/3 → services → text login prompt
- D-Bus system bus, redbear-sessiond (login1), seatd, redbear-authd, redbear-polkit
- redbear-upower, redbear-udisks (read-only)
- Framebuffer via vesad (1280×720), fbcond handoff
- udev-shim, evdevd input stack
- All 37 rootfs units schedule and start

### What does NOT work

1. **No graphical login** — `redbear-greeter-compositor` falls back to `kwin_wayland_wrapper --virtual` because `KWIN_DRM_DEVICES` is empty. The Qt6/QML greeter UI never renders.
2. **Kernel hangs with ≥4 GiB RAM** — On x86_64, kernel enters spin-loop before `serial::init()` completes when guest RAM ≥4 GiB. `make qemu` default 2048 MiB is unaffected.
3. **Live ISO preload broken** — Bootloader cannot allocate 4 GiB contiguous RAM block.

---

## 3. Blocker Resolution Plan

### 3.1 Blocker A: Fix kernel 4 GiB RAM hang

**Priority:** P0 — blocks real hardware and any QEMU config with >2 GiB RAM.

**Symptom:** With `-m 4096` (4 GiB guest RAM), the kernel loads but produces zero serial output. CPU trace shows spin-loop (`pause` + `jmp`). With 2 GiB, boots normally.

**Root cause:** Memory map processing or SMP initialization bug in `startup::memory::init()` or `arch/x86_shared/start.rs` when physical memory exceeds ~2 GiB.

**Evidence:** Kernel binary identical between mini and full (MD5 confirmed). Mini boots at 4 GiB, full does not. Bootloader, kernel, and initfs are byte-identical across profiles.

**Files to modify:**

| File | Change | Why |
|------|--------|-----|
| `recipes/core/kernel/source/src/arch/x86_shared/start.rs` | Add raw COM1 `outb` before `serial::init()` as canary | Proves serial hardware works; isolates hang point |
| `recipes/core/kernel/source/src/startup/memory.rs` | Add debug logging around memory region processing | Identify overflow / bad mapping at large memory sizes |
| `recipes/core/kernel/source/src/arch/x86_shared/device/serial.rs` | Ensure COM1 init path is robust for all memory configs | If serial init itself hangs, diagnose why |

**Acceptance criteria:**
- [ ] `make qemu` with `QEMU_MEM=4096` produces `Redox OS starting...` on serial
- [ ] Full init sequence completes (phase 1 → phase 2 → phase 3 → login prompt)
- [ ] Kernel patch generated, wired into `local/patches/kernel/`, and `recipe.toml` updated per durability policy

**Estimated effort:** 2–4 days (requires kernel debugging with QEMU GDB)

---

### 3.2 Blocker B: Enable DRM/KMS for Wayland compositor

**Priority:** P0 — KWin needs a real DRM device to render the greeter.

**Symptom:** `redbear-greeter-compositor: using virtual KWin backend (set KWIN_DRM_DEVICES to enable DRM)`

**Root cause chain:**

1. `redox-drm` daemon is not being spawned by `pcid-spawner` for the active GPU
2. No `/scheme/drm/card0` device exists
3. `KWIN_DRM_DEVICES` environment variable is not set to the correct path
4. KWin's `--drm` path never activates

**Files to modify:**

| File | Change | Why |
|------|--------|-----|
| `config/redbear-full.toml` — `20_greeter.service` | Add `KWIN_DRM_DEVICES = "/scheme/drm/card0"` to greeter env | Tells greeter compositor where to find DRM device |
| `config/redbear-device-services.toml` | Verify `/lib/pcid.d/` rules are installed with correct paths and vendor/class match patterns | pcid-spawner needs matching rules to auto-spawn redox-drm |
| `local/recipes/gpu/redox-drm/source/src/main.rs` | Add startup logging (which PCI device matched, driver initialized, scheme registered) | Diagnostic visibility — confirms daemon runs |
| `local/recipes/system/redbear-greeter/source/redbear-greeter-compositor` | Add `KWIN_DRM_DEVICES` awareness and fallback logging | Already partially done — verify env propagation from init service |

**QEMU-specific fix:** The `virtio-vga` device (vendor `0x1AF4`, class `0x0300`) needs a pcid rule. Check if `config/redbear-full.toml`'s `virtio-gpud.toml` matches.

**Acceptance criteria:**
- [ ] `redox-drm` daemon appears in `ps` after boot (or logs "DRM daemon started" in boot log)
- [ ] `/scheme/drm/card0` is accessible from the guest
- [ ] `KWIN_DRM_DEVICES` is set and points to `/scheme/drm/card0`
- [ ] `redbear-greeter-compositor` logs "using DRM KWin backend" instead of "virtual"
- [ ] QEMU VNC framebuffer shows the Qt6/QML greeter UI (not bootloader menu)

**Estimated effort:** 3–5 days (pcid matching + DRM device node plumbing + env wiring)

---

### 3.3 Blocker C: Wire the Qt6/QML greeter UI

**Priority:** P1 — requires Blocker B resolved first.

**Symptom:** Text login prompt only. The greeter compositor starts but the Qt6/QML UI never renders.

**Root cause chain:**

1. KWin compositor needs a DRM backend to create a Wayland display (→ Blocker B)
2. `redbear-greeterd` starts the compositor, waits for Wayland socket, then launches `redbear-greeter-ui`
3. If compositor uses virtual backend, the greeter UI may still try to connect to a Wayland display that doesn't exist or lacks rendering
4. Qt6 plugin path and QML import path must be correct for the greeter UI to load

**Files to verify/modify:**

| File | Check/Change | Why |
|------|-------------|-----|
| `local/recipes/system/redbear-greeter/source/src/main.rs` | Verify greeterd waits for compositor Wayland socket before launching UI | Race condition if UI starts before compositor is ready |
| `local/recipes/system/redbear-greeter/source/redbear-greeter-compositor` | Verify `WAYLAND_DISPLAY` is exported and matches what the UI expects | UI connects to compositor via this socket |
| `local/recipes/system/redbear-greeter/source/ui/main.cpp` | Add diagnostic logging: "UI started, connecting to compositor..." | Visibility into UI launch |
| `local/recipes/system/redbear-greeter/source/ui/Main.qml` | Verify Qt6 QML imports resolve at runtime | Missing QtQuick/QtWayland imports cause silent failure |
| `local/recipes/system/redbear-greeter/recipe.toml` | Verify Qt plugin, QML, and asset paths in `package.files` | UI binaries need Qt runtime files staged in sysroot |

**Acceptance criteria:**
- [ ] `redbear-greeterd` logs "compositor ready, launching greeter UI"
- [ ] `redbear-greeter-ui` process appears in `ps`
- [ ] Qt6/QML greeter login screen visible on the display (QEMU VNC)
- [ ] Text input field accepts username, password field accepts password
- [ ] Login attempt reaches `redbear-authd` (visible in authd logs)

**Estimated effort:** 3–5 days (compositor-to-UI handoff + Qt runtime path validation)

---

### 3.4 Blocker D: Session handoff after successful login

**Priority:** P1 — requires Blocker C resolved first.

**Symptom:** Unknown — haven't reached this stage yet. Expected gap: after `redbear-authd` authenticates, `redbear-session-launch` starts the KDE session but KWin/Plasma may fail.

**Files to verify:**

| File | Check | Why |
|------|-------|-----|
| `local/recipes/system/redbear-authd/source/src/main.rs` | `start_session()` flow: does it call session-launch correctly? | Authd initiates the session launch after successful auth |
| `local/recipes/system/redbear-session-launch/source/src/main.rs` | Verify uid/gid drop, env setup, `dbus-run-session` invocation | Session needs correct user context and D-Bus session bus |
| `config/wayland.toml` | Verify canonical KWin launch env (`KWIN_DRM_DEVICES`, `XDG_RUNTIME_DIR`, `QT_*` paths) | KWin session needs same DRM/seat/Qt env as greeter |
| `local/recipes/kde/kwin/` | Verify `kwin_wayland_wrapper` binary is staged and executable | KWin wrapper must be in PATH for session launch |

**Acceptance criteria:**
- [ ] Successful login in greeter triggers session launch
- [ ] `redbear-session-launch` starts with correct UID/GID
- [ ] D-Bus session bus starts for the user session
- [ ] `kwin_wayland_wrapper --drm` starts as the user session compositor
- [ ] `plasmashell` starts (or at minimum, a KWin desktop surface appears)

**Critical gap:** `redbear-kde-session` — the script that `redbear-session-launch` invokes for the KDE session — was not found in the source tree. This script or binary must be created/staged at `/usr/bin/redbear-kde-session`. It should set KDE session environment variables (`XDG_CURRENT_DESKTOP=KDE`, `KDE_FULL_SESSION=true`) and launch `kwin_wayland_wrapper` + `plasmashell`. The upstream KWin Wayland service entry (`plasma-kwin_wayland.service.in`) provides a reference template.

**Estimated effort:** 4–7 days (session handoff + KDE session bring-up + missing script creation)

---

### 3.5 Non-blocker: Fix live ISO preload

**Priority:** P2 — live mode is a convenience, not required for graphical login.

**Symptom:** `live: disabled (unable to allocate 4078 MiB upfront)` — even with 6 GiB guest RAM.

**Fix:** Modify bootloader in `recipes/core/bootloader/source/src/main.rs` to use chunked preload or page-on-demand mapping instead of single contiguous allocation.

**Estimated effort:** 2–3 days

---

## 4. Execution Order

```
Phase 1 (P0): Fix kernel 4 GiB RAM hang
  └── Unblocks real hardware testing and 4 GiB QEMU configs

Phase 2 (P0): Enable DRM/KMS for Wayland
  └── redox-drm auto-spawn + KWIN_DRM_DEVICES wiring
  └── Unblocks KWin --drm mode

Phase 3 (P1): Wire Qt6/QML greeter UI
  └── Requires Phase 2 (DRM backend for compositor)
  └── Deliverable: visible greeter login screen on framebuffer

Phase 4 (P1): Session handoff
  └── Requires Phase 3 (greeter auth working)
  └── Deliverable: post-login KDE session starts

Phase 5 (P2): Fix live ISO preload
  └── Independent of phases 1–4
  └── Deliverable: ISO boots with live mode enabled
```

### Parallel work opportunities

- **Phase 5** (live ISO) can proceed in parallel with Phases 1–4
- Within Phase 2: pcid rule creation and KWIN_DRM_DEVICES env wiring are independent
- Within Phase 3: greeterd protocol fixes and Qt6 path validation are independent

---

## 5. Files Inventory (All Locations Touched)

### Kernel (Phase 1)

```
recipes/core/kernel/source/src/arch/x86_shared/start.rs
recipes/core/kernel/source/src/startup/memory.rs
recipes/core/kernel/source/src/arch/x86_shared/device/serial.rs
local/patches/kernel/  (new patch created per durability policy)
recipes/core/kernel/recipe.toml  (patch wired in)
```

### DRM/KMS (Phase 2)

```
config/redbear-full.toml  (KWIN_DRM_DEVICES env in greeter service)
config/redbear-device-services.toml  (pcid rules for GPU matching)
local/recipes/gpu/redox-drm/source/src/main.rs  (startup logging)
local/config/pcid.d/  (GPU match rules)
```

### Greeter UI (Phase 3)

```
local/recipes/system/redbear-greeter/source/src/main.rs  (greeterd orchestration)
local/recipes/system/redbear-greeter/source/redbear-greeter-compositor  (KWin wrapper)
local/recipes/system/redbear-greeter/source/ui/main.cpp  (UI entry point)
local/recipes/system/redbear-greeter/source/ui/Main.qml  (login screen)
local/recipes/system/redbear-greeter/recipe.toml  (staging paths)
```

### Session Handoff (Phase 4)

```
local/recipes/system/redbear-authd/source/src/main.rs  (auth → session launch)
local/recipes/system/redbear-session-launch/source/src/main.rs  (user session bootstrap)
config/wayland.toml  (canonical KWin DRM launch env)
local/recipes/kde/kwin/  (KWin wrapper binary)
```

### Bootloader (Phase 5)

```
recipes/core/bootloader/source/src/main.rs  (live preload allocator)
```

---

## 6. Verification Protocol

After each phase, verify with:

```bash
# Build the full image
make all CONFIG_NAME=redbear-full

# Run in QEMU with DRM-capable GPU
qemu-system-x86_64 \
  -machine q35 -cpu host -enable-kvm \
  -smp 4 -m 2048 \
  -vga none -device virtio-gpu \
  -drive if=pflash,format=raw,unit=0,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd,readonly=on \
  -drive if=pflash,format=raw,unit=1,file=build/x86_64/redbear-full/fw_vars.bin \
  -drive file=build/x86_64/redbear-full/harddrive.img,format=raw,if=none,id=drv0 \
  -device nvme,drive=drv0,serial=NVME_SERIAL \
  -device e1000,netdev=net0 -netdev user,id=net0 \
  -display gtk,gl=on \
  -serial stdio -monitor none -no-reboot

# Phase-specific checks:
# Phase 1: grep "Redox OS starting" in serial output
# Phase 2: grep "DRM backend" in serial; check /scheme/drm/card0 exists
# Phase 3: visual greeter screen; grep "greeter UI" in serial
# Phase 4: visual KDE desktop; grep "session started" in serial
```

### Phase 1 additional verification (4 GiB):

```bash
# After fix, verify 4 GiB no longer hangs:
qemu-system-x86_64 -nographic -m 4096 [rest of flags] | grep "Redox OS starting"
# Must produce the kernel startup line
```

---

## 7. Related Documentation

| Document | Role |
|----------|------|
| `local/docs/BOOT-PROCESS-ASSESSMENT.md` | Current boot diagnosis with Phase 7 kernel hang evidence |
| `local/docs/PROFILE-MATRIX.md` | ISO organization, RAM requirements, known QEMU issues |
| `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Canonical desktop path (Phase 1–5 model) |
| `local/docs/GREETER-LOGIN-IMPLEMENTATION-PLAN.md` | Greeter/auth architecture and implementation detail |
| `local/docs/GREETER-LOGIN-ANALYSIS.md` | Greeter component topology and protocol analysis |
| `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` | Current build/runtime truth matrix |
| `local/docs/DRM-MODERNIZATION-EXECUTION-PLAN.md` | DRM execution detail beneath desktop path |
| `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` | Wayland subsystem plan |
| `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` | Public implementation plan |

---

## 8. Deleted Stale Documentation (2026-04-27 Cleanup)

Removed four files that were explicitly historical, superseded, or empty:

| Deleted file | Reason | Replaced by |
|-------------|--------|-------------|
| `local/docs/BAREMETAL-LOG.md` | Empty template, no data | `local/docs/BOOT-PROCESS-ASSESSMENT.md` |
| `local/docs/ACPI-FIXES.md` | Self-declared "historical P0 bring-up ledger" | `local/docs/ACPI-IMPROVEMENT-PLAN.md` |
| `docs/02-GAP-ANALYSIS.md` | Self-declared "historical roadmap" | `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` |
| `docs/_CUB_RBPKGBUILD_IMPL_PLAN.md` | Old internal build plan (April 12) | Standard `make` build flow |

All cross-references in `docs/README.md`, `docs/AGENTS.md`, `README.md`, and `local/docs/*` updated.
