# Red Bear OS Greeter / Login Implementation Plan

**Version:** 1.0 — 2026-04-19
**Status:** Active plan with bounded greeter/login proof now passing on `redbear-full`; broader desktop-runtime trust still remains experimental
**Scope:** Red Bear-native graphical greeter, authentication boundary, and session handoff for the KDE-on-Wayland desktop path
**Parent plans:** `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` (v2.0), `local/docs/DBUS-INTEGRATION-PLAN.md`

---

## 1. Executive Summary

Red Bear OS currently has enough session substrate to start **one fixed KDE Wayland session** and now
has a bounded Red Bear-native graphical greeter/login proof on `redbear-full`, but it does not yet
have a runtime-trusted generally stable desktop login surface.

What exists today:

- `dbus-daemon` on the system bus
- `redbear-sessiond` exposing a minimal `org.freedesktop.login1` subset for KWin
- `seatd` as the seat/libseat backend
- a direct session launcher (`redbear-kde-session`) that now starts `kwin_wayland_wrapper --drm`
- fallback text `getty` surfaces on VT2 and `/scheme/debug/no-preserve`

What does **not** exist today:

- no display manager
- no runtime-trusted generally stable compositor-backed desktop login surface
- no PAM-backed or systemd-logind-shaped login stack

This plan defines the forward path for the missing layer:

1. **Do not adopt SDDM first.** Upstream KDE convention points to SDDM, but the current Red Bear
   session/auth substrate is not yet shaped like a conventional Linux desktop-login environment.
2. **Build a Red Bear-native minimal greeter/login path first.** The system should present one
   graphical login surface for one session only: **KDE on Wayland**.
3. **Keep the architecture narrow.** Separate:
   - `redbear-sessiond` → login1/session compatibility for KWin
   - `redbear-greeter` → login UX and session orchestration
   - `redbear-authd` → credential verification and privilege boundary
   - `redbear-session-launch` → user-session bootstrap only

This plan intentionally avoids generic display-manager scope. Red Bear wants **one desktop direction**,
not a multi-session desktop-manager framework.

---

## 2. Scope and Non-Goals

### 2.1 In Scope

- One graphical login surface for the Red Bear KDE-on-Wayland desktop path
- A Red Bear-native greeter daemon, greeter UI, authentication daemon, and session launcher
- Integration with existing `dbus-daemon`, `redbear-sessiond`, `seatd`, `inputd`, and `redbear-kde-session`
- Explicit VT ownership and handoff on the desktop VT
- A narrow local-user authentication model backed by `/etc/passwd`, `/etc/shadow`, and `/etc/group`
- Branding integration using Red Bear assets from `local/Assets/`
- Packaging and config wiring under `local/recipes/system/` and tracked `config/redbear-*.toml`

This plan applies only to the **graphical desktop path**. It does **not** replace console-first or
minimal non-desktop configurations. Existing text and debug console surfaces remain part of the
recovery model.

### 2.2 Out of Scope

- X11 login surfaces
- multiple desktop environments
- session chooser UI
- remote authentication
- PAM/NSS plugin ecosystems
- LDAP/SSO/smartcard/fingerprint login
- graphical lock screen / unlock manager
- full Plasma session-manager semantics (`ksmserver`, multi-user desktop switching)

### 2.3 Policy Assumption

This plan assumes the Red Bear desktop direction converges on **one KDE-on-Wayland path**.

Current implementation answer: the first tracked owner is `redbear-full` (and therefore
`redbear-live-full` for live media). Older names such as `redbear-kde` may still appear in
historical or staging material, but they are not the supported compile-target surface for this plan.

---

## 3. Evidence Model

This plan uses the same evidence language as the canonical desktop plan.

| Class | Meaning | Safe to say | Not safe to say |
|---|---|---|---|
| **builds** | Package compiles and stages | "builds" | "works" |
| **boots** | Image reaches prompt or known runtime surface | "boots" | "desktop works" |
| **enumerates** | Service/register surface appears and answers basic queries | "enumerates" | "usable end to end" |
| **usable** | Bounded runtime path performs its intended task | "usable for this path" | "broadly stable" |
| **validated** | Repeated proof on the intended target class | "validated" | "complete everywhere" |
| **experimental** | Partial, scaffolded, or unproven | "experimental" | "done" |

Rules:

- A greeter binary that compiles is only **builds**.
- A VM image that reaches a graphical login surface is only **boots**.
- A greeter that hands off to KDE on Wayland in bounded QEMU proof is **usable (bounded)**.
- Nothing is **validated** until it repeats reliably on the intended target class.

---

## 4. Current State Assessment

### 4.1 What Exists and Works

| Component | Location | Status | Detail |
|---|---|---|---|
| system D-Bus | `config/redbear-full.toml` | ✅ usable (bounded) | `12_dbus.service` starts `dbus-daemon --system` on the active desktop target |
| login1 compatibility | `local/recipes/system/redbear-sessiond/` | ✅ scaffold | Minimal `org.freedesktop.login1` broker for KWin |
| seat backend | `config/redbear-full.toml` | ✅ builds, wired | `13_seatd.service`; session env exports `LIBSEAT_BACKEND=seatd` |
| display VT activation | `29_activate_console.service` in desktop configs | ✅ usable (bounded) | `inputd -A 3` activates desktop VT |
| fallback text login | `30_console.service` | ✅ boots | `getty 2` on VT2 |
| debug console | `31_debug_console.service` | ✅ boots | `getty /scheme/debug/no-preserve -J` |
| direct KDE session launcher | `/usr/bin/redbear-kde-session` | ✅ builds, experimental | Starts session bus if needed, then `exec kwin_wayland_wrapper --drm` |
| authentication daemon | `local/recipes/system/redbear-authd/` | ✅ builds, experimental | Local-user auth boundary with `/etc/passwd` / `/etc/shadow` / `/etc/group` parsing plus SHA-crypt and Argon2 verification |
| session launcher boundary | `local/recipes/system/redbear-session-launch/` | ✅ builds, experimental | User-session bootstrap with bounded environment/runtime-dir setup |
| greeter daemon scaffold | `local/recipes/system/redbear-greeter/` | ✅ builds, experimental | Root-owned greeter orchestrator, socket protocol, bounded restart policy |
| greeter config fragment | `config/redbear-greeter-services.toml` | ✅ builds, experimental | Adds `19_redbear-authd.service`, `20_greeter.service`, compatibility `20_display.service`, and fallback console dependencies |
| bounded validation launcher | `/usr/bin/redbear-validation-session` | ✅ retained helper | Still available for older bounded validation flows, but no longer the primary `redbear-full` display-service path |
| branding assets | `local/Assets/images/` | ✅ present | `Red Bear OS loading background.png`, `Red Bear OS icon.png` |

### 4.2 What Exists But Is Incomplete

| Component | Status | Gap |
|---|---|---|
| `redbear-sessiond` seat switching | ✅ boundedly implemented | `Seat.SwitchTo` now delegates to `inputd -A <vt>`; remaining compositor stability is not blocked on a seat-switch no-op anymore |
| KDE runtime services | ⚠️ partial | D-Bus substrate exists, but broader Plasma session services remain incomplete |
| `redbear-full` greeter flow | ✅ bounded proof passes | Packaged UI, auth/session plumbing, and bounded compositor-backed greeter proof now work end to end; the old `kwin_wayland` page-fault path is gone, and current QEMU now stops at a clean no-usable-DRM limitation below the greeter slice |
| greeter runtime validation | ✅ bounded proof passes | `redbear-greeter-check` + `test-greeter-qemu.sh` now pass hello, invalid-login, and validation-only successful-login return-to-greeter flow |

### 4.3 What Does Not Exist

| Missing piece | Why it matters |
|---|---|
| display-manager package integration | no SDDM/greetd/lightdm/ly path in repo |

### 4.4 Baseline Conclusion

The current Red Bear desktop path can now **own a bounded login flow**, and the active greeter/login
implementation bar in this plan is substantially met.

The remaining blocker to a stronger desktop-runtime claim is now evidence-backed as **below this
greeter slice**: the old `kwin_wayland` crash path has been eliminated, and current QEMU now reaches
clean `No suitable DRM devices have been found` exits instead. That means the follow-on work has
shifted to the parent desktop/Wayland/runtime plans rather than to missing core greeter/auth/session-boundary pieces here.

Future work beyond this plan should continue **without** replacing the current seat/session substrate
and without removing existing console recovery paths.

---

## 5. Decision Record: Login-Manager Direction

### 5.1 Recommendation

The best fit for Red Bear OS **today** is a **Red Bear-native minimal single-session greeter/launcher**.

This is closer in class to **greetd-style minimal orchestration** than to **SDDM-style full desktop
manager behavior**, but the forward path should be **Red Bear-specific**, not a generic Linux deployment.

### 5.2 Why Not SDDM First

SDDM is the standard answer for a conventional KDE distribution, but Red Bear is not yet a conventional
Linux-shaped session/auth environment.

The repo evidence today shows:

- no SDDM integration,
- no PAM path,
- no mature general-purpose display-manager substrate,
- a deliberately minimal `login1` compatibility layer,
- a fixed single desktop direction.

Adopting SDDM first would force Red Bear to emulate a broader environment before the current narrower
session path is runtime-trusted.

### 5.3 Ranked Direction

| Rank | Direction | Verdict |
|---|---|---|
| 1 | Red Bear-native minimal greeter | **Primary** |
| 2 | Current direct session launcher | bring-up baseline only |
| 3 | SDDM-class integration | future option after session/auth substrate matures |
| 4 | GDM/LightDM/elogind-shaped path | reject |

### 5.4 Future Revisit Trigger

Revisit SDDM-class integration only if Red Bear later decides it needs:

- richer multi-user semantics,
- session chooser behavior,
- broader desktop-manager policy surface,
- significantly fuller login/session accounting than the current `redbear-sessiond` contract.

---

## 6. Architecture Principles

### 6.1 One Desktop, One Session Path

The greeter must launch exactly one session target:

- **KDE on Wayland**

There is no session chooser in v1.

### 6.2 Keep the Existing Session Substrate

Reuse existing pieces rather than replacing them:

- `dbus-daemon`
- `redbear-sessiond`
- `seatd`
- `inputd`
- `redbear-kde-session`

The greeter layer sits **above** them.

`seatd` remains the **seat/device authority** for this design. The greeter stack consumes the existing
seat/libseat path; it does not introduce a second seat/session-manager authority.

### 6.3 Separate Login UX, Authentication, and Session Bootstrap

Do not collapse these roles into one process.

| Component | Responsibility |
|---|---|
| `redbear-sessiond` | login1/session compatibility for KWin |
| `redbear-greeterd` | login flow orchestration |
| `redbear-greeter-ui` | graphical login UX |
| `redbear-authd` | credential verification and privilege boundary |
| `redbear-session-launch` | drop privileges, set env, start user session |

`redbear-sessiond` is therefore **not** the login/auth/session-launch authority. It remains the
KWin-facing session compatibility broker defined by the D-Bus plan.

### 6.4 Avoid a PAM Clone

Red Bear should not build a new generic PAM/NSS plugin ecosystem merely to satisfy display-manager
expectations. For this path, use a narrow local account model first.

### 6.5 Stop-and-Start Handoff Is Acceptable

The greeter and the user session do not need an in-place seamless transition in v1.

It is acceptable to:

1. stop the greeter UI,
2. start the user session cleanly,
3. return to the greeter after session exit.

### 6.6 Branding Is Part of the Product Surface

Use committed Red Bear assets as the default greeter look.

Source-of-truth art files in the repo are:

- background: `local/Assets/images/Red Bear OS loading background.png`
- icon: `local/Assets/images/Red Bear OS icon.png`

At runtime, the greeter must use **installed asset paths**, not source-tree paths.

---

## 7. Architecture Design

### 7.1 Stack Overview

```text
┌────────────────────────────────────────────────────────────────────┐
│ KDE Wayland user session                                          │
│ redbear-kde-session → kwin_wayland → later Plasma services        │
├────────────────────────────────────────────────────────────────────┤
│ redbear-session-launch                                            │
│ drop privileges, set env, start session bus, exec session         │
├────────────────────────────────────────────────────────────────────┤
│ redbear-authd                         redbear-greeter-ui          │
│ local auth + privilege boundary       Qt6/QML login surface       │
├────────────────────────────────────────────────────────────────────┤
│ redbear-greeterd                                                   │
│ login state machine, VT3 ownership, auth/session orchestration     │
├────────────────────────────────────────────────────────────────────┤
│ dbus-daemon --system    redbear-sessiond    seatd    inputd        │
├────────────────────────────────────────────────────────────────────┤
│ Redox schemes / system services                                    │
│ scheme:input, scheme:acpi, scheme:drm, debug scheme, etc.          │
└────────────────────────────────────────────────────────────────────┘
```

### 7.2 Boot-to-Login Sequence

```text
boot
  → 12_dbus.service               (system D-Bus)
  → 13_redbear-sessiond.service   (login1 subset)
  → 13_seatd.service              (seat backend)
  → 20_greeter.service            (start redbear-greeterd on VT3)
  → 29_activate_console.service   (inputd -A 3)
  → 30_console.service            (fallback getty 2 on VT2)
  → 31_debug_console.service      (debug getty)
  → redbear-greeter-ui shows login surface on VT3
  → successful login
  → redbear-session-launch
  → dbus-run-session -- redbear-kde-session
  → kwin_wayland
```

### 7.3 Session Return Path

```text
user session exits or crashes
  → redbear-greeterd observes session root exit
  → greeter-specific cleanup
  → reactivate VT3
  → respawn redbear-greeter-ui
  → return to login surface
```

### 7.4 Why This Shape Fits the Repo

- matches existing `VT=3` display path
- preserves fallback text login on VT2
- reuses `redbear-sessiond` instead of replacing it
- does not assume a broader Linux-style session-manager stack than the repo currently has
- avoids dead-end graphical boot behavior by preserving text/debug fallback paths

---

## 8. Component Specifications

### 8.1 `redbear-greeterd`

**Type:** root-owned orchestrator daemon

**Responsibilities:**

- own the login state machine,
- own greeter/UI lifecycle,
- talk to `redbear-authd`,
- start the user session via `redbear-session-launch`,
- monitor the session root process,
- return to greeter after logout/session crash.

**Must not do:**

- parse or verify passwords directly unless `redbear-authd` is intentionally collapsed into it,
- render the login UI,
- absorb generic session-manager policy.

### 8.2 `redbear-greeter-ui`

**Type:** unprivileged Qt6/QML frontend

**Responsibilities:**

- render Red Bear background and icon,
- collect username/password,
- present Login / Shutdown / Reboot,
- show bounded status (`Authenticating`, `Login failed`, `Starting session`).

**Must not do:**

- read `/etc/shadow`,
- own power/device/session policy,
- choose alternate desktop sessions.

### 8.3 `redbear-authd`

**Type:** privileged authentication daemon

**Responsibilities:**

- read local user data,
- verify password hashes,
- check lock/disable rules,
- perform narrow privileged actions (`login`, optional `shutdown`, optional `reboot`),
- spawn `redbear-session-launch` for a verified user.

**Must not do:**

- own the greeter UI,
- own compositor startup policy,
- become a general identity platform.

`redbear-authd` is the **only** component in this plan allowed to read password-hash data
(` /etc/shadow`-equivalent runtime content). Neither UI nor session launcher may touch it.

### 8.4 `redbear-session-launch`

**Type:** small bootstrap tool

**Responsibilities:**

- create/fix `XDG_RUNTIME_DIR`,
- drop to target uid/gid and supplementary groups,
- construct a minimal KDE/Wayland environment,
- launch the user session bus,
- exec `redbear-kde-session`.

`redbear-session-launch` is intentionally thin. It must not duplicate KDE session policy already owned
by `redbear-kde-session`.

### 8.5 `redbear-sessiond`

This plan does **not** replace `redbear-sessiond`.

It remains responsible for:

- `org.freedesktop.login1` subset for KWin,
- session/seat compatibility surface,
- bounded power/sleep integration already assigned in the D-Bus plan.

---

## 9. Protocols and Session Contracts

### 9.1 UI ↔ Greeter Daemon Protocol

Transport:

- Unix socket at `/run/redbear-greeterd.sock`
- JSON messages, versioned

Minimum message set:

```json
{ "type": "hello", "version": 1 }
{ "type": "submit_login", "username": "alice", "password": "secret" }
{ "type": "request_shutdown" }
{ "type": "request_reboot" }
```

Example reply:

```json
{ "type": "hello_ok", "background": "/usr/share/redbear/greeter/background.png", "icon": "/usr/share/redbear/greeter/icon.png", "session_name": "KDE on Wayland" }
```

### 9.2 Greeter Daemon ↔ Auth Daemon Protocol

Transport:

- Unix socket at `/run/redbear-authd.sock`
- JSON messages, versioned

Minimum message set:

```json
{ "type": "authenticate", "request_id": 17, "username": "alice", "password": "secret", "vt": 3 }
{ "type": "start_session", "request_id": 17, "username": "alice", "session": "kde-wayland" }
```

### 9.3 State Machine

`redbear-greeterd` uses this state set:

1. `Starting`
2. `GreeterReady`
3. `Authenticating`
4. `LaunchingSession`
5. `SessionRunning`
6. `ReturningToGreeter`
7. `PowerAction`
8. `FatalError`

Rules:

- one greeter UI process at a time,
- one session launch in flight,
- one supported session only: `kde-wayland`,
- greeter UI never survives into `SessionRunning`.

### 9.4 Local Account Storage Contract

Use a simple Unix-like model first:

- `/etc/passwd`
- `/etc/shadow`
- `/etc/group`

This plan explicitly rejects inventing a new account database format for v1.

The local-account model is a **runtime contract**. Source-tree examples or provisioning helpers may live
elsewhere, but the greeter/auth path must interact only with installed runtime account files.

### 9.5 Session-Launch Environment

`redbear-session-launch` should set a minimal explicit environment:

- `HOME`
- `USER`
- `LOGNAME`
- `SHELL`
- `PATH=/usr/bin:/bin`
- `XDG_RUNTIME_DIR=/run/user/$UID`
- `XDG_SESSION_TYPE=wayland`
- `XDG_CURRENT_DESKTOP=KDE`
- `XDG_SESSION_ID=c1`
- `KDE_FULL_SESSION=true`
- `WAYLAND_DISPLAY=wayland-0`
- `XDG_SEAT=seat0`
- `XDG_VTNR=3`
- `LIBSEAT_BACKEND=seatd`
- `SEATD_SOCK=/run/seatd.sock`

Preferred launch form:

```text
dbus-run-session -- redbear-kde-session
```

If `dbus-run-session` proves unreliable on Red Bear, use the current `dbus-launch` pattern as a bounded
fallback.

### 9.6 Branding Contract

Stage the current assets at stable runtime paths:

- `/usr/share/redbear/greeter/background.png`
- `/usr/share/redbear/greeter/icon.png`

Use:

- `Red Bear OS loading background.png` as the full-screen wallpaper
- `Red Bear OS icon.png` above the login form

The greeter runtime must reference only the installed `/usr/share/redbear/greeter/*` paths.
`local/Assets/...` remains the source-of-truth location in the repo, not a runtime lookup path.

### 9.7 Failure and Fallback Contract

Greeter failure must never create a dead-end boot surface.

Required behavior:

- VT2 `getty` remains available as text recovery,
- debug `getty` remains available,
- greeter failures return control to a recoverable state,
- repeated greeter/UI restart failures must stop escalating after a bounded retry count,
- the system must prefer a reachable fallback console over an infinite graphical restart loop.

---

## 10. Phased Implementation

> **Current implementation note:** the repo has now crossed the bounded proof bar through the core
> G0–G4 path and parts of G5. The phase breakdown below remains useful as an ownership and acceptance
> model, but it should be read as an active status ladder rather than as an untouched future-only plan.

### Phase G0 — Scope Freeze and Wiring Baseline (✅ boundedly complete)

**Goal:** Freeze the architectural split and identify the tracked desktop profile(s) that will own the
greeter path.

| # | Task | Acceptance criteria |
|---|---|---|
| G0.1 | Freeze component boundaries | `sessiond`, `greeterd`, `authd`, `session-launch` responsibilities documented without overlap |
| G0.2 | Freeze single-session policy | Only `kde-wayland` is named as the supported graphical session |
| G0.3 | Freeze branding inputs | Runtime asset paths and source asset files documented |

**Exit criteria:**

- architecture split is documented,
- session policy is explicit,
- asset source of truth is explicit.

### Phase G1 — Service Skeleton and Boot Wiring (✅ boundedly complete)

**Goal:** Add daemon/package skeletons and init wiring without claiming a usable login flow.

| # | Task | Acceptance criteria |
|---|---|---|
| G1.1 | Create recipe skeletons | `redbear-greeter`, `redbear-authd`, `redbear-session-launch`, and shared `redbear-login-protocol` build and stage |
| G1.2 | Add config fragment | A tracked config fragment wires `20_greeter.service` and supporting files |
| G1.3 | Replace direct display launch in the chosen profile | Desktop profile starts `redbear-greeterd` instead of directly starting `redbear-kde-session` |
| G1.4 | Keep text/debug recovery path | VT2 `getty` and debug `getty` still boot |

**Exit criteria:**

- packages build,
- boot wiring is in place,
- image still boots,
- fallback text surfaces remain reachable.

### Phase G2 — Auth Foundation (✅ boundedly complete)

**Goal:** Prove the local account/authentication boundary independent of the full greeter UI.

| # | Task | Acceptance criteria |
|---|---|---|
| G2.1 | Implement passwd/shadow parsing | Local users can be parsed from the chosen account files |
| G2.2 | Implement password verification | Valid and invalid credentials are distinguished correctly in tests |
| G2.3 | Implement lock/disable rules | Locked/disabled users are rejected predictably |
| G2.4 | Implement session-spawn authorization boundary | Only `redbear-authd` can approve session launch |
| G2.5 | Implement bounded failure handling | Retry throttling / lockout policy is documented and covered by tests |

**Exit criteria:**

- auth parser tests pass,
- credential checks pass,
- negative cases pass,
- no UI process reads auth data,
- repeated auth failure behavior is bounded and explicit.

### Phase G3 — Greeter UI and Daemon State Machine (✅ boundedly complete)

**Goal:** Bring up the graphical greeter surface and daemon orchestration.

| # | Task | Acceptance criteria |
|---|---|---|
| G3.1 | Start greeter UI on VT3 | QEMU image reaches a Red Bear-branded graphical greeter surface |
| G3.2 | Implement UI/daemon socket protocol | UI can submit login and power requests |
| G3.3 | Implement daemon state machine | State transitions are test-covered for success and failure paths |
| G3.4 | Implement bounded login error UX | Invalid credentials return cleanly to `GreeterReady` |
| G3.5 | Implement failure fallback behavior | Greeter/UI restart failure yields reachable fallback behavior rather than infinite restart |

**Exit criteria:**

- greeter surface boots,
- UI/daemon protocol works,
- failure returns to the login screen,
- no session starts yet without auth success,
- fallback console path remains reachable under greeter failure.

### Phase G4 — Session Handoff to KDE on Wayland (✅ boundedly complete for the current bounded proof)

**Goal:** Replace direct session startup with authenticated session launch.

| # | Task | Acceptance criteria |
|---|---|---|
| G4.1 | Implement `redbear-session-launch` env/bootstrap path | Session runs with correct uid/gid/groups/runtime dir/env |
| G4.2 | Implement greeter teardown before session launch | Greeter UI exits before KDE session becomes active |
| G4.3 | Implement session-monitor return path | Session exit returns to the greeter |
| G4.4 | Keep bounded D-Bus/sessiond compatibility intact | KWin still sees the required login1 subset |

**Exit criteria:**

- successful login reaches `redbear-kde-session`,
- session uses intended env/runtime dir,
- session exit returns to greeter,
- fallback VT2 login still works.

### Phase G5 — Desktop Integration and Product Surface Hardening (⚠️ partial / follow-on)

**Goal:** Move from “bounded login proof” to a product-quality Red Bear login surface.

| # | Task | Acceptance criteria |
|---|---|---|
| G5.1 | Implement reboot/shutdown path | Greeter can trigger bounded power actions |
| G5.2 | Hardening | rate limiting, buffer clearing, socket permission checks, retry behavior |
| G5.3 | Packaging and profile cleanup | Target desktop profile wiring is canonical and documented |
| G5.4 | Validation tooling | scripted QEMU/runtime proof exists for greeter boot/login/logout loop |

**Exit criteria:**

- login loop is repeatable,
- power actions are bounded and explicit,
- hardening checks pass,
- documentation matches shipped surface.

**Current state:**

- G5.1 is present in bounded form through the greeter power-action path,
- G5.3 is substantially present for the tracked `redbear-full` profile wiring,
- G5.4 exists as `local/scripts/test-greeter-qemu.sh` plus in-target `redbear-greeter-check`,
- the remaining open part is moving from bounded proof to stronger desktop-runtime trust and broader
  compositor/session stability evidence.

### Critical Path

```text
G0 (scope)
  → G1 (wiring)
  → G2 (auth boundary)
  → G3 (greeter surface)
  → G4 (session handoff)
  → G5 (product hardening)
```

---

## 11. Testing and Validation

### 11.1 Unit and Component Tests

| Component | Tests |
|---|---|
| `redbear-login-protocol` | message encoding/decoding, version checks |
| `redbear-authd` | passwd parsing, shadow parsing, hash verification, lockout logic |
| `redbear-session-launch` | env construction, runtime-dir creation, argument validation |
| `redbear-greeterd` | state transitions, socket protocol handling, session-monitor behavior |
| `redbear-greeter-ui` | smoke only; no auth logic in UI tests |

### 11.2 Integration Checks

The first bounded integration proofs should answer these questions in order:

1. does the image boot to a graphical greeter surface on VT3?
2. does invalid login return to the greeter surface?
3. does valid login reach `redbear-kde-session`?
4. does session exit return to the greeter?
5. do VT2 and debug login remain available as recovery paths?
6. does greeter failure still leave a recoverable console path instead of looping forever?

### 11.3 Suggested Validation Commands / Harnesses

This plan now has a bounded QEMU/runtime harness in the repo and should continue to follow the same
proof style as other Red Bear runtime validation flows.

Current surfaces:

- `local/scripts/test-greeter-qemu.sh`
- in-target checker `redbear-greeter-check`

The exact script names are still implementation details, but the proof style should match existing
bounded runtime validation patterns already used elsewhere in the repo.

### 11.4 Definition of Done

This plan is only substantially complete when **all** of the following are true:

- a Red Bear-branded graphical greeter boots on the tracked KDE desktop path,
- credentials are verified through a narrow privileged boundary,
- valid login reaches KDE on Wayland,
- invalid login returns cleanly to the greeter,
- session exit returns to the greeter,
- VT2 fallback and debug console remain available,
- greeter/UI failure does not trap the machine in an unrecoverable restart loop,
- the bounded login/logout proof repeats reliably on the intended target class.

**Current status against this bar:** the bounded QEMU greeter proof now satisfies the greeter/login
implementation bar in this plan. The remaining blocker to stronger desktop-session claims reproduces
under direct `dbus-run-session -- redbear-kde-session` as well, so it no longer points to a missing
greeter/auth/session-boundary implementation inside this plan.

---

## 12. Risks and Mitigations

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---:|---:|---|
| R1 | `redbear-sessiond` login1 subset proves too thin for stable KWin session ownership | Medium | High | keep greeter plan explicitly dependent on D-Bus/sessiond validation; widen only the needed contract |
| R2 | Auth layer grows into a PAM replacement by accident | Medium | High | freeze v1 to local users + passwd/shadow only |
| R3 | Greeter UI becomes privileged by convenience | Medium | High | keep UI unprivileged and enforce daemon/auth socket boundary |
| R4 | VT/session handoff is flaky on real targets | Medium | High | keep VT2 fallback path and validate QEMU before broader claims |
| R5 | Profile ownership confusion (`redbear-kde` vs `redbear-full`) delays integration | High | Medium | keep profile naming a policy question separate from greeter architecture |
| R6 | Branding/assets are staged inconsistently | Low | Medium | stage stable runtime paths under `/usr/share/redbear/greeter/` |
| R7 | Session launch inherits too much ambient environment | Medium | Medium | start from a clean explicit environment in `redbear-session-launch` |
| R8 | Greeter restart policy creates boot loops | Medium | High | bound retries and prefer console fallback after repeated failures |

---

## 13. Relationship to Other Plans

| Document | Role relative to this plan |
|---|---|
| `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md` | Parent desktop-path authority; this plan fills the graphical login boundary beneath it |
| `local/docs/DBUS-INTEGRATION-PLAN.md` | Parent session/D-Bus authority for `redbear-sessiond` and related service model |
| `local/docs/DESKTOP-STACK-CURRENT-STATUS.md` | Current truth source for what the desktop stack actually builds/boots today |
| `local/docs/WAYLAND-IMPLEMENTATION-PLAN.md` | Wayland/compositor subsystem plan beneath the desktop path |
| `docs/07-RED-BEAR-OS-IMPLEMENTATION-PLAN.md` | Repo-wide product/profile/workstream framing |

This document does **not** replace any of the above. It fills a missing subsystem-planning gap:
the login/greeter boundary between a booted desktop substrate and a real KDE session surface.

---

## 14. File and Recipe Inventory

### 14.1 Existing Files This Plan Builds On

- `config/redbear-full.toml`
- `config/redbear-greeter-services.toml`
- `local/recipes/system/redbear-sessiond/`
- `local/recipes/system/redbear-dbus-services/`
- `local/Assets/images/Red Bear OS loading background.png`
- `local/Assets/images/Red Bear OS icon.png`

### 14.2 Proposed New Recipe Layout

```text
local/recipes/system/
├── redbear-authd/
├── redbear-login-protocol/
├── redbear-session-launch/
└── redbear-greeter/
```

Current implementation status:

- `redbear-authd/` — implemented (experimental, target-side recipe build proven)
- `redbear-session-launch/` — implemented (experimental, target-side recipe build proven)
- `redbear-greeter/` — implemented as an experimental bounded surface; daemon, Qt/QML UI, compositor wrapper, staged assets, and bounded runtime checks now exist, while broader KDE runtime trust still remains open
- `redbear-login-protocol/` — implemented as a shared local crate for greeter/auth/checker protocol types
- The previous guest-side Qt shared-plugin metadata blocker is now fixed: `libqminimal.so` and `qwayland-org.kde.kwin.qpa.so` load successfully in the guest once the Redox toolchain's stale `elf.h` is synchronized with relibc's corrected ELF64 typedefs.
- Current remaining desktop-runtime blocker below the greeter slice: on current QEMU the compositor no longer page-faults, but still exits cleanly when no usable DRM device can be opened; the greeter's bounded QEMU proof still passes through hello, invalid-login, and validation-only successful-login return-to-greeter flow.

### 14.3 Proposed New Runtime Files

```text
/usr/bin/redbear-greeterd
/usr/bin/redbear-greeter-ui
/usr/bin/redbear-authd
/usr/bin/redbear-session-launch
/usr/share/redbear/greeter/background.png
/usr/share/redbear/greeter/icon.png
/run/redbear-greeterd.sock
/run/redbear-authd.sock
/usr/bin/redbear-greeter-check
```

Bounded validation helper currently landed:

```text
local/scripts/test-greeter-qemu.sh
```

### 14.4 Proposed Config Fragment

This plan expects a tracked config include fragment such as:

```text
config/redbear-greeter-services.toml
```

That fragment should own:

- package inclusions for greeter/auth/session-launch,
- `20_greeter.service`,
- any bounded init-service overrides needed to replace direct session startup.

The greeter **recipe**, not the config fragment, should own staged runtime artifacts such as:

- `/usr/bin/redbear-greeter-ui`
- `/usr/share/redbear/greeter/background.png`
- `/usr/share/redbear/greeter/icon.png`
- compositor/helper payloads that the greeter package installs under `/usr/share/redbear/greeter/`

---

## 15. Open Questions

1. Which tracked profile should own the canonical desktop greeter path first:
   `redbear-kde`, `redbear-full`, or a unified future target?
2. Which password-hash scheme should Red Bear standardize on for v1 local users?
3. Should reboot/shutdown requests go through `redbear-authd` or a separate narrow power helper?
4. Is `dbus-run-session` reliable enough on Red Bear, or should the current `dbus-launch` path remain the first shipped session-bus strategy?
5. At what point should the project consider SDDM-class integration again, if ever?

Current answer to (1): **`redbear-full` first**, with `redbear-live-full` inheriting that path for
live media.

Current answer to (2): **traditional `/etc/shadow` SHA-512-crypt / SHA-256-crypt first** (`$6$` / `$5$`),
with narrower support preferred over premature multi-format sprawl.

Free/libre policy note for (2): the current verifier path uses the pure-Rust `sha-crypt` crate,
which is licensed `MIT OR Apache-2.0`; for Red Bear policy purposes it is treated under the MIT
option, keeping the greeter/login stack within a free/open-source dependency surface. The intended
implementation direction remains **pure-Rust verification crates first**, not `crypt(3)` FFI.

Current answer to (3): **through `redbear-authd` in the first cut**, to preserve one narrow privileged
boundary until runtime evidence justifies a separate helper.

Current answer to (4): **`dbus-run-session` remains the preferred first shipped path**, with fallback
conservatism retained in validation/docs until broader runtime proof exists.

Current answer to (5): **not before the Red Bear-native greeter path is runtime-trusted and the
session/auth substrate is materially stronger than it is today.**
