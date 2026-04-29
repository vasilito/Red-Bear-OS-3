# Red Bear OS Greeter/Login System — Comprehensive Analysis

**Generated:** 2026-04-26
**Based on:** Source code analysis of `redbear-authd`, `redbear-greeter`, `redbear-sessiond`, `redbear-session-launch`, `redbear-login-protocol`, init service configuration, and the GREETER-LOGIN-IMPLEMENTATION-PLAN.md.

---

## 1. System Architecture

### 1.1 Component Topology

```
Qt6/QML Login Surface (redbear-greeter-ui, VT3)
    │  Unix socket /run/redbear-greeterd.sock (JSON, line-delimited)
    ↓
redbear-greeterd  (orchestrator daemon, root-owned, VT3)
    │  Unix socket /run/redbear-authd.sock (AuthRequest/AuthResponse JSON)
    ↓
redbear-authd  (privileged auth daemon, /etc/shadow verification)
    │  spawns via Command::
    ↓
redbear-session-launch  (uid/gid drop + env bootstrap)
    │  exec's
    ↓
dbus-run-session -- redbear-kde-session  →  kwin_wayland_wrapper --drm + plasmashell

(redbear-sessiond on system D-Bus → org.freedesktop.login1 for KWin device access)
```

**Key socket paths:**
| Socket | Owner | Mode | Purpose |
|--------|-------|------|---------|
| `/run/redbear-authd.sock` | root | 0o600 | greeterd → authd |
| `/run/redbear-greeterd.sock` | greeter user | 0o660 | greeter-ui → greeterd |
| `/run/redbear-sessiond-control.sock` | root | 0o600 | authd → sessiond (JSON SessiondUpdate) |
| `/run/seatd.sock` | root | 0o666 | seatd abstract namespace |

---

## 2. Password Verification (authd)

**Source:** `local/recipes/system/redbear-authd/source/src/main.rs` lines 101–214

**Storage:** Reads `/etc/passwd` (user/uid/gid/home/shell) and `/etc/shadow` (password hash).

**Format detection:** Both Redox-style (`;`-delimited) and Unix-style (`:`-delimited) passwd/shadow/group entries are auto-detected per-line (line 88–99 in authd main.rs).

**Hash verification (lines 183–193):**
```rust
fn verify_shadow_password(password: &str, shadow_hash: &str) -> Result<bool, VerifyError> {
    if shadow_hash.starts_with("$6$") || shadow_hash.starts_with("$5$") {
        // SHA-512 or SHA-256 crypt (sha-crypt crate, pure Rust)
        return Ok(ShaCrypt::default().verify_password(password.as_bytes(), shadow_hash).is_ok());
    }
    if shadow_hash.starts_with("$argon2") {
        // Argon2id (rust-argon2 crate)
        return Ok(verify_encoded(shadow_hash, password.as_bytes()).unwrap_or(false));
    }
    Err(VerifyError::UnsupportedHashFormat)
}
```

**Plain-text fallback:** Non-`$` hash strings are compared directly (line 213). Used for unshadowed entries.

**Lockout policy (lines 237–270):**
- 5 failures in 60s → 30-second lockout
- Rejects locked accounts (`!` or `*` prefix)
- UID < 1000 rejected (except UID 0)

**Approval system (lines 216–287):**
- Successful auth stores 15-second in-memory approval keyed to `username + VT`
- Session start requires valid (non-expired, VT-matched) approval ticket

---

## 3. Communication: UI ↔ greeterd ↔ authd

**Protocol:** `redbear-login-protocol` crate (`local/recipes/system/redbear-login-protocol/source/src/lib.rs`)

```rust
// greeterd → authd
AuthRequest::Authenticate { request_id, username, password, vt }
AuthRequest::StartSession { request_id, username, session: "kde-wayland", vt }
AuthRequest::PowerAction { request_id, action: "shutdown"|"reboot" }

// authd → greeterd
AuthResponse::AuthenticateResult { request_id, ok, message }
AuthResponse::SessionResult { request_id, ok, exit_code, message }
AuthResponse::PowerResult { request_id, ok, message }
AuthResponse::Error { request_id, message }
```

```rust
// UI → greeterd
GreeterRequest::SubmitLogin { username, password }

// greeterd → UI
GreeterResponse::LoginResult { ok, state, message }
GreeterResponse::ActionResult { ok, message }
```

**greeterd state machine:**
```
Starting → GreeterReady → Authenticating → LaunchingSession → SessionRunning
                                                        ↓
                                                  ReturningToGreeter → GreeterReady
                                                        ↓
                                                    FatalError (after 3 restarts/60s)
```

---

## 4. Session Launch

**Source:** `local/recipes/system/redbear-session-launch/source/src/main.rs` lines 352–385

1. Reads `/etc/passwd` + `/etc/group` for uid/gid/groups
2. Creates `XDG_RUNTIME_DIR` (`/run/user/$UID` or `/tmp/run/user/$UID`), chown 0700
3. Builds clean env: `HOME`, `USER`, `LOGNAME`, `SHELL`, `PATH=/usr/bin:/bin`, `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY=wayland-0`, `XDG_SEAT=seat0`, `XDG_VTNR`, `LIBSEAT_BACKEND=seatd`, `SEATD_SOCK=/run/seatd.sock`, `XDG_SESSION_TYPE=wayland`, `XDG_CURRENT_DESKTOP=KDE`, `KDE_FULL_SESSION=true`, `XDG_SESSION_ID=c1`
4. `env_clear()` → setuid + setgid + setgroups
5. `exec /usr/bin/dbus-run-session -- /usr/bin/redbear-kde-session`
6. Fallback: direct `redbear-kde-session` if `dbus-run-session` absent

**redbear-kde-session** (from `docs/05-KDE-PLASMA-ON-REDOX.md`):
```bash
export WAYLAND_DISPLAY=wayland-0
export XDG_RUNTIME_DIR=/tmp/run/user/0
dbus-daemon --system &
eval $(dbus-launch --sh-syntax)
kwin_wayland_wrapper --drm &
sleep 2 && plasmashell &
```

---

## 5. Init Service Wiring

**From `config/redbear-full.toml`:**

```
Service order:
  12_dbus.service                    (system D-Bus)
  13_redbear-sessiond.service        (org.freedesktop.login1 broker)
  13_seatd.service                   (seat management)
  19_redbear-authd.service           (auth daemon, /usr/bin/redbear-authd)
  20_greeter.service                 (greeterd, /usr/bin/redbear-greeterd, VT=3)
  29_activate_console.service        (inputd -A 2 → VT2 fallback)
  30_console.service                 (getty 2, respawn)
  31_debug_console.service           (getty debug, respawn)
```

`20_greeter.service`:
```toml
cmd = "/usr/bin/redbear-greeterd"
envs = { VT = "3", REDBEAR_GREETER_USER = "greeter" }
type = "oneshot_async"
```

**Greeter user account** (redbear-full.toml):
```toml
[users.greeter]
password = ""
uid = 101
gid = 101
home = "/nonexistent"
shell = "/usr/bin/ion"
```

---

## 6. D-Bus Integration

**redbear-sessiond** — `org.freedesktop.login1` on **system D-Bus** via `zbus`:
- `Manager.ListSessions`, `Manager.GetSeat`, `PrepareForShutdown` signal
- `Seat.SwitchTo(vt)` → `inputd -A <vt>`
- `Session.TakeDevice`/`ReleaseDevice` → DRM/input device fd passing
- `Session.TakeControl`/`ReleaseControl`
- Service file: `/usr/share/dbus-1/system-services/org.freedesktop.login1.service`

**authd and greeterd are NOT D-Bus activated** — started directly by init services.

**greeter compositor** starts a **session D-Bus** via `dbus-launch`.

---

## 7. Quality and Robustness Assessment

### 7.1 Strengths

| Area | Assessment | Detail |
|------|------------|--------|
| **Hash algorithm** | ✅ Excellent | SHA-512 (`$6$`), SHA-256 (`$5$`), Argon2id — all pure-Rust crates, no MD5/DES |
| **Constant-time comparison** | ✅ Good | `sha-crypt::verify_password` and `argon2::verify_encoded` are constant-time by design |
| **Approval windowing** | ✅ Good | 15s approval between auth and session start, VT-bound |
| **Lockout policy** | ✅ Good | 5 attempts / 60s → 30s lockout |
| **Socket permissions** | ✅ Good | authd socket = 0o600, greeterd socket = 0o660 |
| **UID restriction** | ✅ Good | UID < 1000 (non-root) disallowed |
| **Restart bounding** | ✅ Good | 3 restarts/60s → FatalError, fallback consoles preserved |
| **Protocol type safety** | ✅ Good | `redbear-login-protocol` crate is single source of truth for all JSON types |
| **Plain-text fallback** | ⚠️ Acceptable | Non-`$` hash strings compared directly — OK for initial dev users |
| **Fail-closed on unknown hash** | ✅ Good | `UnsupportedHashFormat` → login rejected, not bypassed |
| **Greeter isolates UI crash** | ✅ Good | compositor survives UI crash; respawns UI only |

### 7.2 Weaknesses and Risks

| # | Issue | Severity | Location | Impact |
|---|-------|----------|-----------|--------|
| W1 | **No PAM integration** | Medium | authd is custom narrow auth | Limits enterprise use, no pluggable auth modules |
| W2 | **Approval in-memory only** | Medium | authd `HashMap` | authd crash → approvals lost; session start fails after crash |
| W3 | **No password quality enforcement** | Low | authd only checks lockout | Weak passwords accepted (acceptable for Phase 2) |
| W4 | **Hardcoded `kde-wayland` session** | Low | authd line 301, session-launch line 335 | No session chooser, no alternative desktops |
| W5 | **greeterd not respawned by init** | Medium | `20_greeter.service` type=oneshot_async | If greeterd crashes, system stuck at console (no auto-recovery) |
| W6 | **No seatd watchdog** | Medium | seatd service has no internal restart | seatd crash → compositor immediately fails |
| W7 | **Static device_map.rs** | Medium | major/minor hardcoded table | Non-static hardware (USB GPUs, etc.) not discovered |
| W8 | **No session tracking via D-Bus** | Low | authd → sessiond via raw JSON socket | `SetSession`/`ResetSession` bypass login1 surface |
| W9 | **Power action fallbacks missing** | Low | authd calls `/usr/bin/shutdown`, `/usr/bin/reboot` | May not exist on Redox; failure is silent |
| W10 | **greeterd socket path hardcoded** | Low | `/run/redbear-greeterd.sock` vs XDG_RUNTIME_DIR | Works for single-seat; breaks in multi-seat |
| W11 | **greeter init service is `true` stub** | **Critical** | `redbear-greeter-services.toml` → `20_greeter.service cmd = "true"` | Real greeter only in `redbear-full.toml`; mini/grub don't have it |
| W12 | ~~redbear-greeter-compositor missing from image~~(resolved) | Low | Recipe installs to both `/usr/bin/` and `/usr/share/redbear/greeter/`; main.rs checks both | compositor binary available via both paths |
| W13 | ~~dbus-run-session may not exist in image~~(resolved) | Low | dbus in redbear-mini config (inherit by redbear-full); session-launch prefers `/usr/bin/dbus-run-session`; dbus recipe installs it | D-Bus session bus available |

### 7.3 Greeter Login-Screen Prerequisites (most resolved; bounded QEMU proof now passes)

*Note: As of 2026-04-29, the bounded `redbear-full` QEMU greeter proof passes (`GREETER_HELLO=ok`, `GREETER_VALID=ok`). Most items below are satisfied by the active config; remaining items are "verify via build."*

| Blocker | Source | Fix |
|---------|--------|-----|
| greeter init service stub in greeter-services.toml | `20_greeter.service cmd = "true"` | Use `redbear-full.toml` service definition (already correct there) |
| ~~compositor binary path mismatch~~ (resolved) | Recipe installs to both `/usr/bin/` and `/usr/share/redbear/greeter/`; greeterd checks both | No action needed |
| seatd package in config | seatd = {} now present in redbear-full.toml packages section | Rebuild to include seatd in image |
| redbear-authd now in config | authd recipe in redbear-full config | Verify authd binary reaches image via build |
| redbear-sessiond now in config | sessiond inherited from redbear-mini config | Verify sessiond binary reaches image via build |
| greeter user account present in config | `[users.greeter]` in redbear-full config | Verify greeter user uid=101 in /etc/passwd in image after build |
| compositor requires DRM but QEMU has none | `kwin_wayland_wrapper --drm` fails in VM | Use `--virtual` in VM; compositor script already handles this |

---

## 8. File Path Reference

| Artifact | Path |
|---|---|
| authd binary | `/usr/bin/redbear-authd` |
| authd socket | `/run/redbear-authd.sock` |
| greeterd socket | `/run/redbear-greeterd.sock` |
| greeterd binary | `/usr/bin/redbear-greeterd` |
| greeter-ui binary | `/usr/bin/redbear-greeter-ui` |
| compositor script | `/usr/bin/redbear-greeter-compositor` |
| compositor (share) | `/usr/share/redbear/greeter/redbear-greeter-compositor` |
| session-launch binary | `/usr/bin/redbear-session-launch` |
| sessiond binary | `/usr/bin/redbear-sessiond` |
| greeterd init service | `/usr/lib/init.d/20_greeter.service` |
| authd init service | `/usr/lib/init.d/19_redbear-authd.service` |
| sessiond init service | `/usr/lib/init.d/13_redbear-sessiond.service` |
| seatd init service | `/usr/lib/init.d/13_seatd.service` |
| greeter background | `/usr/share/redbear/greeter/background.png` |
| greeter icon | `/usr/share/redbear/greeter/icon.png` |
| sessiond control socket | `/run/redbear-sessiond-control.sock` |
| seatd socket | `/run/seatd.sock` |
| passwd file | `/etc/passwd` (redox `;` or unix `:` delimited) |
| shadow file | `/etc/shadow` |
| group file | `/etc/group` |
| greeter user account | uid=101, gid=101 in /etc/passwd |

---

## 9. Improvement Recommendations (Priority Order)

### P0 — Make Greeter Actually Reach Login Screen

1. **Fix greeter init service**: Ensure `20_greeter.service` in `redbear-full.toml` (not the stub in greeter-services.toml) is the canonical one. greeter-services.toml is a bounded proof fragment; the real service lives in redbear-full.toml.
2. **Verify all 5 greeter packages are in redbear-full.toml**: `seatd`, `redbear-authd`, `redbear-sessiond`, `redbear-session-launch`, `redbear-greeter`
3. **Verify compositor binary at `/usr/bin/redbear-greeter-compositor`** in the built image
4. **Verify greeter user (uid=101) exists** in /etc/passwd in image
5. **Add compositor fallback** to `--virtual` when `--drm` fails (script already does this)

### P1 — Hardening

6. **Add respawn to greeterd init service**: `type = "oneshot_async", respawn = true` — greeterd crash shouldn't leave system at console
7. **Add seatd respawn**: same logic
8. **Fix redbear-sessiond `Seat::SwitchTo`** to return error rather than silently ignore failures
9. **Add watchdog for greeterd** — if greeterd crashes, init should restart it

### P2 — Security Hardening

10. **Add password quality enforcement**: minimum length, entropy check before accepting
11. **Rate-limit by source IP/VT**: prevent VT-based brute force
12. **Add audit log for auth failures**: log to syslog or dedicated auth log
13. **Add session listing via control socket**: currently authd writes `SetSession`/`ResetSession` but there's no readback mechanism

### P3 — Architectural

14. **Implement `TakeDevice`/`ReleaseDevice` fully**: current session.rs has the skeleton but device fd passing needs verification
15. **Dynamic device enumeration**: replace static device_map.rs with udev-shim runtime queries
16. **Add greeter watchdog daemon**: separate from greeterd, monitors and restarts it
17. **D-Bus activate greeterd and authd**: remove init service startup dependency, use D-Bus activation instead
18. **Add power action binaries**: create `/usr/bin/shutdown` and `/usr/bin/reboot` symlinks or wrappers that call init system
19. **Implement `PrepareForShutdown`/`PrepareForSleep` signals**: for session cleanup on system power events

### P4 — Future

20. **Add PAM integration** via a minimal PAM-like module system in authd
21. **Add session chooser** (console vs kde-wayland) via greeter UI
22. **Multi-seat support**: XDG_RUNTIME_DIR per seat, greeterd socket per seat
23. **Fingerprint/webauthn support**: extend authd protocol + greeter UI

---

*End of Analysis*