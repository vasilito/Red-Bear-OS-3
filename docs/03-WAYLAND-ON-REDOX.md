# 03 — Wayland on Redox: Concrete Implementation Path

## Goal

Get a working Wayland compositor on Redox OS that can run KDE Plasma applications.

## Current State

- `config/wayland.toml` exists — launches `cosmic-comp` or `smallvil` via `orbital-wayland`
- 21 Wayland recipes in `recipes/wip/wayland/` — most untested
- `libwayland` 1.24.0 builds with `redox.patch` that stubs out 7 POSIX APIs
- `smallvil` (Smithay) runs as basic compositor, performance poor
- `cosmic-comp` builds but has no keyboard input (missing libinput)
- `libdrm` builds with all GPU drivers disabled
- Mesa uses OSMesa (software rendering only)

---

## Step 1: Fix relibc POSIX Gaps (1-2 weeks)

### What to implement

These are the 7 APIs that libwayland's `redox.patch` removes. Each must be added
to `relibc` (repo: https://gitlab.redox-os.org/redox-os/relibc).

#### 1.1 `signalfd` / `signalfd4`

**Files to create/modify in relibc:**
```
src/header/signal/mod.rs        — add signalfd(), signalfd4()
src/header/signal/src.rs        — add SFD_CLOEXEC, SFD_NONBLOCK constants
src/header/signal/types.rs      — add signalfd_siginfo struct
src/platform/redox/mod.rs       — wire to kernel event scheme or userspace signal handler
```

**Implementation approach:**
```rust
// src/header/signal/mod.rs
pub fn signalfd(fd: c_int, mask: *const sigset_t, flags: c_int) -> c_int {
    // If fd == -1, create a new "signal FD" using event scheme
    // Register signal mask with the signal handling infrastructure
    // Return FD that becomes readable when signals arrive
    // Map to Redox: use event: scheme + signal userspace handler
}
```

**Approximate effort**: ~200 lines of Rust.

#### 1.2 `timerfd`

**Files to create in relibc:**
```
src/header/sys_timerfd/mod.rs   — NEW: timerfd_create(), timerfd_settime(), timerfd_gettime()
src/header/sys_timerfd/types.rs — NEW: itimerspec, TFD_CLOEXEC, TFD_NONBLOCK, TFD_TIMER_ABSTIME
src/platform/redox/mod.rs       — wire to time: scheme
```

**Implementation approach:**
```rust
// src/header/sys_timerfd/mod.rs
pub fn timerfd_create(clockid: c_int, flags: c_int) -> c_int {
    // Create a timer FD using Redox time: scheme
    // Return FD that becomes readable when timer fires
    // Read returns uint64_t count of expirations
}

pub fn timerfd_settime(fd: c_int, flags: c_int, new: *const itimerspec, old: *mut itimerspec) -> c_int {
    // Arm/disarm timer
    // Use time: scheme for absolute/relative timers
}
```

**Approximate effort**: ~300 lines of Rust.

#### 1.3 `eventfd`

**Files to create in relibc:**
```
src/header/sys_eventfd/mod.rs   — NEW: eventfd(), eventfd_read(), eventfd_write()
src/header/sys_eventfd/types.rs — EFD_CLOEXEC, EFD_NONBLOCK, EFD_SEMAPHORE
```

**Implementation approach:**
```rust
// Simplest of the three — just an atomic counter accessed via read/write
pub fn eventfd(initval: c_uint, flags: c_int) -> c_int {
    // Create a pipe-like FD backed by a shared atomic counter
    // read() blocks until counter > 0, returns counter, resets to 0
    // write() adds to counter
    // Use Redox pipe: scheme internally
}
```

**Approximate effort**: ~100 lines of Rust.

#### 1.4 `F_DUPFD_CLOEXEC`

**File to modify in relibc:**
```
src/header/fcntl/mod.rs — add F_DUPFD_CLOEXEC constant (value 0x40 on Linux x86_64)
src/platform/redox/alloc.rs — handle F_DUPFD_CLOEXEC in fcntl()
```

```rust
// In fcntl handler:
pub const F_DUPFD_CLOEXEC: c_int = 0x406; // Linux value

// In fcntl() match:
F_DUPFD_CLOEXEC => {
    let new_fd = syscall::dup(fd, None)?;
    // Set CLOEXEC flag on new_fd
    // Return new_fd
}
```

**Approximate effort**: ~20 lines.

#### 1.5 `MSG_CMSG_CLOEXEC` and `MSG_NOSIGNAL`

**Files to modify in relibc:**
```
src/header/sys_socket/mod.rs — add MSG_CMSG_CLOEXEC (0x40000000), MSG_NOSIGNAL (0x4000)
src/platform/redox/mod.rs    — handle in recvmsg/sendmsg
```

`MSG_NOSIGNAL`: suppress SIGPIPE on broken connection. On Redox, SIGPIPE handling
is already userspace — just don't send the signal when this flag is set.

`MSG_CMSG_CLOEXEC`: set CLOEXEC on FDs received via SCM_RIGHTS. Apply the flag
when processing ancillary data in recvmsg.

**Approximate effort**: ~50 lines.

#### 1.6 `open_memstream`

**File to modify in relibc:**
```
src/header/stdio/mod.rs — add open_memstream()
src/header/stdio/src.rs  — implementation
```

```rust
pub fn open_memstream(bufp: *mut *mut c_char, sizep: *mut usize) -> *mut FILE {
    // Create a write-only stream that dynamically grows a buffer
    // On close or flush, update *bufp and *sizep
    // Can be implemented using a backing Vec<u8> and custom FILE vtable
}
```

**Approximate effort**: ~200 lines.

### Verification

After implementing all 7 APIs:
1. Rebuild relibc: `./target/release/repo cook recipes/core/relibc`
2. Rebuild libwayland **without** `redox.patch` — it should compile natively
3. Test: `wayland-rs_simple_window` runs without crashes

---

## Step 2: evdev Input Daemon (4-6 weeks)

### Architecture

```
┌──────────────────┐     ┌──────────────────────┐     ┌──────────────┐
│  libinput         │────→│  /dev/input/eventX    │────→│  evdevd      │
│  (ported)         │     │  (character devices)  │     │  (daemon)    │
└──────────────────┘     └──────────────────────┘     └──────┬───────┘
                                                              │
                                                     reads Redox schemes:
                                                     input:, scheme:irq
```

### What to build

**New daemon: `evdevd`** (userspace, like all Redox drivers)

Create as a new recipe: `recipes/core/evdevd/`

**Source structure:**
```
evdevd/
├── Cargo.toml
├── src/
│   ├── main.rs          — daemon entry, scheme registration
│   ├── scheme.rs        — implements "evdev" scheme
│   ├── device.rs        — translates Redox events to input_event
│   └── ioctl.rs         — handles EVIOCG* ioctls
```

**Key implementation:**

```rust
// src/main.rs
fn main() {
    // 1. Open existing Redox input sources
    let keyboard = File::open("scheme:input/keyboard")?;
    let mouse = File::open("scheme:input/mouse")?;
    
    // 2. Create /dev/input symlinks (pointing to our scheme)
    // /dev/input/event0 → /scheme/evdev/keyboard
    // /dev/input/event1 → /scheme/evdev/mouse
    
    // 3. Register evdev scheme
    let scheme = File::create(":evdev")?;
    
    // 4. Event loop: read from Redox input schemes, translate, write to evdev clients
    loop {
        let redox_event = read_redox_event(&keyboard)?;
        let evdev_event = translate_to_input_event(redox_event);
        // Deliver to subscribed clients
    }
}
```

```rust
// src/ioctl.rs — implement evdev ioctls
fn handle_ioctl(fd: usize, request: usize, arg: usize) -> Result<usize> {
    match request {
        EVIOCGNAME => { /* write device name string to arg */ },
        EVIOCGBIT => { /* write supported event types bitmap to arg */ },
        EVIOCGABS => { /* write absinfo struct for absolute axes */ },
        EVIOCGRAB => { /* grab/exclusive access to device */ },
        EVIOCGPROP => { /* write device properties bitmap */ },
        _ => Err(syscall::Error::new(syscall::EINVAL)),
    }
}
```

**Also needed: udev shim**

Create `recipes/wip/wayland/udev-shim/` — a minimal udev implementation that:
- Enumerates `/dev/input/event*` devices
- Emits "add"/"remove" events via netlink-compatible socket
- Provides `udev_device_get_property_value()` for `ID_INPUT_*` properties

libinput needs this for hotplug. A minimal shim is ~500 lines of Rust.

**Then port libinput:**

Modify `recipes/wip/wayland/libinput/` (currently missing — create it):
```toml
[source]
tar = "https://gitlab.freedesktop.org/wayland/libinput/-/archive/1.27.0/libinput-1.27.0.tar.gz"
patches = ["redox.patch"]

[build]
template = "meson"
dependencies = [
    "evdevd",
    "libffi",
    "libwayland",
    "udev-shim",
    "mtdev",        # touchpad multi-touch
    "libevdev",     # evdev wrapper library
]
mesonflags = [
    "-Ddocumentation=false",
    "-Dtests=false",
    "-Ddebug-gui=false",
]
```

### Verification

1. Build and run `evdevd`
2. `cat /dev/input/event0` shows keyboard events
3. Build libinput against evdevd
4. `libinput list-devices` shows keyboard and mouse

---

## Step 3: DRM/KMS Scheme (8-12 weeks)

### Architecture

```
┌──────────────┐    ┌───────────────────┐    ┌────────────────┐
│  libdrm       │───→│  scheme:drm/card0  │───→│  drmd (daemon) │
│  (ported)     │    │  DRM ioctls via    │    │  GPU driver    │
│               │    │  scheme protocol   │    │  userspace     │
└──────────────┘    └───────────────────┘    └───────┬────────┘
                                                      │
                                            scheme:memory + scheme:irq
                                                      │
                                                  Hardware (GPU)
```

### What to build

**New daemon: `drmd`** (DRM daemon — starts with Intel support)

Create as: `recipes/core/drmd/`

**Source structure:**
```
drmd/
├── Cargo.toml
├── src/
│   ├── main.rs              — daemon entry, PCI enumeration
│   ├── scheme.rs            — registers "drm" scheme
│   ├── kms/
│   │   ├── mod.rs           — KMS object management
│   │   ├── crtc.rs          — CRTC implementation
│   │   ├── connector.rs     — connector (HDMI, DP, eDP)
│   │   ├── encoder.rs       — encoder
│   │   ├── plane.rs         — primary + cursor planes
│   │   └── framebuffer.rs   — framebuffer allocation
│   ├── gem/
│   │   ├── mod.rs           — GEM buffer management
│   │   └── dmabuf.rs        — DMA-BUF export/import
│   └── drivers/
│       ├── mod.rs           — driver trait
│       └── intel.rs         — Intel GPU driver (modesetting)
```

**Core DRM scheme protocol:**

```rust
// src/scheme.rs
// DRM scheme implements the same ioctls as Linux /dev/dri/card0
// but via Redox scheme read/write/packet protocol

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

**Intel driver (starting point):**

```rust
// src/drivers/intel.rs
// Based on public Intel GPU documentation:
// https://01.org/linuxgraphics/documentation/hardware-specification-prm

pub struct IntelDriver {
    mmio: *mut u8,              // Memory-mapped I/O registers (via scheme:memory)
    gtt_size: usize,            // Graphics Translation Table size
    framebuffer: PhysAddr,      // Current scanout buffer
}

impl IntelDriver {
    pub fn new(pci_dev: &PciDev) -> Result<Self> {
        // Map MMIO registers via scheme:memory/physical
        let mmio = map_physical_memory(pci_dev.bar[0], pci_dev.bar_size[0])?;
        
        // Initialize GTT (Graphics Translation Table)
        // Set up display pipeline
        
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

### Verification

1. `drmd` registers `scheme:drm/card0`
2. Port `modetest` (from libdrm tests) — shows connector info and modes
3. `modetest -M intel -s 0:1920x1080` sets a mode and shows test pattern

---

## Step 4: Working Wayland Compositor (4-6 weeks after Steps 1-3)

### Recommended: Smithay/smallvil first, then KWin

**Why Smithay first:**
- Pure Rust — no C++ toolchain issues
- Already has a Redox branch (`https://github.com/jackpot51/smithay`, branch `redox`)
- Smithay's input backend is pluggable — write a Redox-specific one
- Gets us a working compositor months before KWin is ported

**What to modify in Smithay:**

```
smithay/
├── src/backend/
│   ├── input/
│   │   └── redox.rs          — NEW: Redox input backend (reads evdev scheme)
│   ├── drm/
│   │   └── redox.rs          — NEW: Redox DRM backend (uses scheme:drm)
│   └── egl/
│       └── redox.rs          — NEW: Redox EGL display (uses Mesa)
```

**Redox input backend:**
```rust
// src/backend/input/redox.rs
pub struct RedoxInputBackend {
    devices: Vec<EvdevDevice>,  // opened from /dev/input/eventX
}

impl InputBackend for RedoxInputBackend {
    fn dispatch(&mut self) -> Vec<InputEvent> {
        // Read from all evdev devices via evdevd
        // Translate to Smithay's InternalEvent type
    }
}
```

**Redox DRM backend:**
```rust
// src/backend/drm/redox.rs
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

### Recipe to add/modify

```toml
# recipes/wip/wayland/smallvil/recipe.toml (modify existing)
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

### Verification

1. `smallvil` launches with DRM backend — takes over display
2. Keyboard and mouse work via evdevd
3. `libcosmic-wayland_application` renders a window on the compositor
4. Screenshot shows the window

---

## Step 5: Enable cosmic-comp and Other Compositors

Once Steps 1-4 are done:

1. **cosmic-comp**: Uncomment libinput dependency in recipe, rebuild
2. **wlroots**: Build with libdrm + libinput + GBM
3. **sway**: Should work once wlroots builds
4. **KWin**: See `05-KDE-PLASMA-ON-REDOX.md` for the full path

---

## Fastest Path Summary

```
Week 1-2:   Implement signalfd/timerfd/eventfd/etc in relibc
            → libwayland builds without patches

Week 3-8:   Build evdevd (input daemon) + udev shim
            → libinput works

Week 9-20:  Build drmd (DRM daemon) with Intel modesetting
            → libdrm works, modesetting functional

Week 21-26: Smithay Redox backends (input + DRM + EGL)
            → Working Wayland compositor with hardware display

Week 27+:   Port Qt, KDE Frameworks, Plasma Shell
            → KDE Plasma desktop (see doc 05)
```

**Key insight**: Steps 2 (evdev) and 3 (DRM) can run in parallel.
With 2 developers, the Wayland compositor is achievable in ~6 months.
