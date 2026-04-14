# INPUTD SCHEME API ENHANCEMENT DESIGN

**Target**: `recipes/core/base/source/drivers/inputd`
**Scope**: Userspace-only `inputd` scheme enhancement
**Date**: 2026-04-13

## 1. Goal

Enhance `inputd` so it can do all of the following without breaking any existing callers:

1. Let producers register under stable names such as `ps2-keyboard`, `ps2-mouse`, or `usb-hid0`.
2. Expose per-device consumer streams so services such as `evdevd` can subscribe to one device only.
3. Publish hotplug notifications for device add/remove.
4. Expose currently registered devices through the scheme root directory.

This is an **additive** design. Existing paths, existing event payloads, existing VT behavior, and existing display/control behavior must continue to work unchanged.

## 2. Current Implementation Summary

The current `inputd` implementation in `recipes/core/base/source/drivers/inputd/src/main.rs` has these important properties:

- `Handle` only supports `Producer`, `Consumer`, `Display`, `Control`, and `SchemeRoot`.
- `openat()` only recognizes `producer`, `consumer`, `consumer_bootlog`, `handle`, `handle_early`, and `control`.
- All producers write anonymous `orbclient::Event` bytes into the same `Handle::Producer` path.
- Legacy consumers are per-VT handles. `write()` only delivers input bytes to the **active VT** consumer set.
- `SchemeRoot` exists, but it is not a real directory yet: it does not enumerate entries.
- `lib.rs` only exposes `ProducerHandle`, `ConsumerHandle`, `DisplayHandle`, and `ControlHandle`.

Current callers confirm the limitation:

- `ps2d` opens one `ProducerHandle` and sends both keyboard and mouse events into the same stream.
- `usbhidd` also opens one `ProducerHandle` and sends keyboard/mouse/button/scroll data into the same stream.
- local `evdevd` reads `/scheme/input/consumer`, receives anonymous mixed `orbclient::Event` values, and manually translates them.

## 3. Design Principles

1. **Keep legacy behavior intact**: `/scheme/input/producer` and `/scheme/input/consumer` must keep working exactly as they do today.
2. **Do not change event payloads**: device-specific streams still carry serialized `orbclient::Event` values.
3. **Keep all logic in userspace**: no kernel changes, no new kernel scheme semantics.
4. **Make enumeration path-driven**: device names are visible as entries below `/scheme/input/`.
5. **Use explicit hotplug events**: device discovery and liveness must not depend on polling failed opens.

## 4. Scheme Path Layout

The enhanced namespace is:

```text
/scheme/input/                    — SchemeRoot (directory listing)
/scheme/input/producer            — Legacy producer (unchanged)
/scheme/input/producer/{name}     — Named producer: ps2-keyboard, ps2-mouse, usb-hid0
/scheme/input/consumer            — Legacy consumer (unchanged)
/scheme/input/{device_name}       — Per-device consumer: reads events from one named producer
/scheme/input/events              — Hotplug event stream
/scheme/input/handle/{display}    — Display handle (unchanged)
/scheme/input/control             — Control commands (unchanged)
```

Legacy-only paths that must remain valid even though they are not part of the new API surface:

```text
/scheme/input/consumer_bootlog    — Existing bootlog VT consumer
/scheme/input/handle_early/{display} — Existing early framebuffer handoff path
```

### 4.1 Root Directory Listing

`SchemeRoot` should become a real directory endpoint backed by `getdents`, not by overloading `read()`.

The root listing should expose:

- static entries: `producer`, `consumer`, `consumer_bootlog`, `events`, `handle`, `handle_early`, `control`
- one dynamic entry per registered device name from `devices`

That keeps the namespace honest while still allowing device enumeration from `/scheme/input/`.

`InputDeviceLister` in `lib.rs` should filter out the reserved static names and return only dynamic device entries.

## 5. Handle Model

The `Handle` enum in `main.rs` should become:

```rust
enum Handle {
    Producer,
    NamedProducer {
        name: String,
    },
    Consumer {
        events: EventFlags,
        pending: Vec<u8>,
        needs_handoff: bool,
        notified: bool,
        vt: usize,
    },
    DeviceConsumer {
        device_name: String,
        events: EventFlags,
        pending: Vec<u8>,
        notified: bool,
    },
    HotplugEvents {
        events: EventFlags,
        pending: Vec<u8>,
        notified: bool,
    },
    Display {
        events: EventFlags,
        pending: Vec<VtEvent>,
        notified: bool,
        device: String,
        is_earlyfb: bool,
    },
    Control,
    SchemeRoot,
}
```

Notes:

- `Producer` remains the legacy anonymous producer path.
- `NamedProducer` only needs the registered name. Device ID lookup stays in shared scheme state.
- `DeviceConsumer` is byte-oriented like the legacy consumer, but without VT or handoff state.
- `HotplugEvents` stores serialized variable-length hotplug records in `pending`.
- `SchemeRoot` remains a dedicated handle variant, but now supports directory enumeration.

## 6. Scheme Open Semantics

`openat()` should parse paths as follows:

### 6.1 Existing Paths

- `producer` with no child component → `Handle::Producer`
- `consumer` → current VT consumer allocation logic
- `consumer_bootlog` → current VT 1 logic
- `handle/{display}` → unchanged
- `handle_early/{display}` → unchanged
- `control` → unchanged

### 6.2 New Paths

- `producer/{name}` → `Handle::NamedProducer { name }`
- `events` → `Handle::HotplugEvents { ... }`
- any other top-level non-reserved path component → `Handle::DeviceConsumer { device_name, ... }`

### 6.3 Name Validation

Named producer registration must reject:

- empty names
- names containing `/`
- reserved names: `producer`, `consumer`, `consumer_bootlog`, `events`, `handle`, `handle_early`, `control`
- duplicate live names already present in `devices`

Recommended error behavior:

- invalid name → `EINVAL`
- duplicate name → `EEXIST`
- open of `/scheme/input/{device_name}` for a currently unknown device → `ENOENT`

## 7. State Management

`InputScheme` should add:

```rust
devices: BTreeMap<String, u32>,
next_device_id: AtomicUsize,
```

Purpose:

- `devices` maps device name → current device ID
- `next_device_id` allocates monotonically increasing IDs

Behavior:

1. When `NamedProducer` opens successfully:
   - allocate `device_id = next_device_id.fetch_add(1, Ordering::SeqCst) as u32`
   - insert `devices.insert(name.clone(), device_id)`
   - serialize a `DEVICE_ADD` hotplug message
   - append it to every `Handle::HotplugEvents.pending`
   - set `notified = false` on those hotplug handles
   - set `has_new_events = true`

2. When `NamedProducer` closes:
   - remove the entry from `devices`
   - serialize a `DEVICE_REMOVE` hotplug message with the removed ID and name
   - append it to every `Handle::HotplugEvents.pending`
   - set `notified = false`
   - set `has_new_events = true`

3. Device IDs are never reused. If `ps2-keyboard` disappears and later comes back, it gets a new `device_id`.

No additional kernel state is required. This is ordinary daemon-side bookkeeping.

## 8. Event Routing Logic

The existing preprocessing path in `write()` must remain in place:

- special Super+Fn VT switching behavior stays in `inputd`
- keymap translation still happens in `inputd`
- the emitted payload remains serialized `orbclient::Event`

After that preprocessing step, routing changes as follows.

### 8.1 Legacy Producer

Input written to `/scheme/input/producer` follows the current legacy route:

- deliver to the existing legacy consumer path
- preserve current active-VT behavior
- do **not** deliver to any `DeviceConsumer`
- do **not** generate hotplug events

### 8.2 Named Producer

Input written to `/scheme/input/producer/{name}` must be fanned out to:

1. the matching `DeviceConsumer` handles where `device_name == name`
2. the existing legacy consumer path used by Orbital and other old clients

That means named producers are **supersets** of legacy routing, not replacements.

### 8.3 Device Consumer

`/scheme/input/{device_name}` only receives events from the named producer with the exact same name.

It must never receive:

- anonymous legacy producer traffic
- events from other named producers
- display or control events

### 8.4 Routing Sketch

```text
legacy producer write
  -> existing input normalization
  -> legacy VT consumer fan-out only

named producer write(name)
  -> existing input normalization
  -> device consumers for name
  -> legacy VT consumer fan-out
```

Implementation-wise, the simplest approach is:

1. detect whether the writer is `Producer` or `NamedProducer { name }`
2. run the existing event transformation code once
3. serialize transformed `Event` values once
4. if named, append to matching `DeviceConsumer.pending`
5. append to the legacy consumer path using the current active-VT logic
6. clear `notified` on affected readers and set `has_new_events = true`

## 9. Hotplug Event Stream

`/scheme/input/events` is a read-only stream of variable-length hotplug records.

### 9.1 Binary Format

```rust
#[repr(C)]
struct InputHotplugEvent {
    kind: u32,       // 1 = DEVICE_ADD, 2 = DEVICE_REMOVE
    device_id: u32,  // Unique device identifier
    name_len: u32,   // Length of device name
    _reserved: u32,  // Future use
}
// Followed by name_len bytes of UTF-8 device name
```

Constants:

```rust
const DEVICE_ADD: u32 = 1;
const DEVICE_REMOVE: u32 = 2;
```

### 9.2 Stream Semantics

- The stream is append-only and ordered by daemon observation.
- Each record is serialized as header bytes followed by UTF-8 name bytes.
- `read()` drains raw bytes from `pending`.
- Because records are variable-length, callers must handle partial reads.
- `HotplugHandle` in `lib.rs` should hide this by buffering partial bytes until one full record is available.

### 9.3 Notification Model

`Handle::HotplugEvents` participates in `fevent(EVENT_READ)` exactly like other readable handles:

- when at least one serialized hotplug record is pending and the handle is subscribed to `EVENT_READ`, post a read event
- after a successful read drains the buffer, notification becomes edge-triggered again

## 10. Scheme Root Enumeration

Enumeration should be implemented with `getdents()` on `Handle::SchemeRoot`.

Recommended behavior:

- `scheme_root()` still creates a `Handle::SchemeRoot`
- `getdents()` emits static entries plus one entry per `devices` key
- `read()` on `SchemeRoot` stays invalid (`EBADF` or `EISDIR` are both acceptable if applied consistently)
- `openat()` continues to require a valid `SchemeRoot` dirfd

Example visible entries after `ps2d` registers keyboard and mouse:

```text
producer
consumer
consumer_bootlog
events
handle
handle_early
control
ps2-keyboard
ps2-mouse
```

This gives normal filesystem-style discovery while keeping old endpoints visible.

## 11. `lib.rs` Public API Changes

The public API should be extended, not replaced.

### 11.1 Existing Types Stay

- `ProducerHandle`
- `ConsumerHandle`
- `DisplayHandle`
- `ControlHandle`

Their existing constructors and behavior remain unchanged.

### 11.2 New Types

```rust
pub struct NamedProducerHandle(File);
pub struct DeviceConsumerHandle(File);
pub struct HotplugHandle {
    file: File,
    partial: Vec<u8>,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct HotplugEventHeader {
    pub kind: u32,
    pub device_id: u32,
    pub name_len: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone)]
pub struct HotplugEvent {
    pub kind: u32,
    pub device_id: u32,
    pub name: String,
}

pub struct InputDeviceLister;
```

### 11.3 Constructors

```rust
impl NamedProducerHandle {
    pub fn new(name: &str) -> io::Result<Self>;
}

impl DeviceConsumerHandle {
    pub fn new(device_name: &str) -> io::Result<Self>;
}

impl HotplugHandle {
    pub fn new() -> io::Result<Self>;
}
```

Path mapping:

- `NamedProducerHandle::new("ps2-keyboard")` → `/scheme/input/producer/ps2-keyboard`
- `DeviceConsumerHandle::new("ps2-keyboard")` → `/scheme/input/ps2-keyboard`
- `HotplugHandle::new()` → `/scheme/input/events`

### 11.4 Read/Write Shape

Recommended API shape:

```rust
impl NamedProducerHandle {
    pub fn write_event(&mut self, event: orbclient::Event) -> io::Result<()>;
}

pub enum DeviceConsumerHandleEvent<'a> {
    Events(&'a [Event]),
}

impl DeviceConsumerHandle {
    pub fn event_handle(&self) -> BorrowedFd<'_>;
    pub fn read_events<'a>(&self, events: &'a mut [Event])
        -> io::Result<DeviceConsumerHandleEvent<'a>>;
}

impl HotplugHandle {
    pub fn event_handle(&self) -> BorrowedFd<'_>;
    pub fn read_event(&mut self) -> io::Result<Option<HotplugEvent>>;
}
```

`DeviceConsumerHandle` deliberately mirrors `ConsumerHandle`, but it does not need `Handoff` support because VT display handoff is unrelated to per-device streams.

### 11.5 Device Enumeration Helper

`InputDeviceLister` should provide a safe wrapper around scheme-root directory reads, for example:

```rust
impl InputDeviceLister {
    pub fn list() -> io::Result<Vec<String>>;
}
```

Behavior:

- read `/scheme/input/` as a directory
- drop reserved static entries
- return only currently registered device names

This keeps callers out of scheme-internal filtering logic.

## 12. Producer Lifecycle and Consumer Behavior

### 12.1 Named Producer Registration

Opening `/scheme/input/producer/{name}` is both:

- creation of a producer handle
- registration of `{name}` as a live device

Closing the fd unregisters the device.

This matches current scheme style well because `inputd` already uses `on_close()` to clean up VT consumers.

### 12.2 Device Consumer Lifetime

Per-device consumer handles are name-based subscriptions.

- open succeeds only while the device name is currently registered
- once open, the handle remains attached to that name
- if the producer disappears, no more events arrive for that handle
- if the same name is registered again later, the handle resumes receiving events for that name
- the hotplug stream is how clients notice that the underlying producer instance changed

This keeps `DeviceConsumer` simple and avoids introducing a second handle teardown protocol.

## 13. Migration Path

### 13.1 `ps2d`

`ps2d` is the first caller that should adopt the new API because it already has a clean split between keyboard and mouse sources.

Recommended startup logic:

1. Try `NamedProducerHandle::new("ps2-keyboard")`
2. Try `NamedProducerHandle::new("ps2-mouse")`
3. If both succeed, run in named mode
4. If either fails, close any partially opened named handle and fall back to one legacy `ProducerHandle::new()`

Routing:

- keyboard scancodes → `ps2-keyboard`
- mouse move / absolute move / button / scroll events → `ps2-mouse`

This preserves compatibility with old `inputd` while immediately enabling per-device consumers on new `inputd`.

### 13.2 `evdevd`

Once the scheme exists, local `evdevd` can move from `/scheme/input/consumer` to:

- `InputDeviceLister::list()` to discover devices
- `DeviceConsumerHandle::new(name)` for device-local streams
- `HotplugHandle::new()` to watch add/remove

It can keep the legacy consumer path as a fallback for older systems.

### 13.3 `usbhidd`

`usbhidd` can remain legacy initially, then later migrate to named producers such as `usb-hid0`, `usb-hid1`, or more specific per-interface names.

## 14. Backward Compatibility Requirements

All of the following must continue to work unchanged:

- `/scheme/input/producer`
- `/scheme/input/consumer`
- `/scheme/input/consumer_bootlog`
- `/scheme/input/handle/{display}`
- `/scheme/input/handle_early/{display}`
- `/scheme/input/control`
- current `ProducerHandle`, `ConsumerHandle`, `DisplayHandle`, and `ControlHandle` APIs
- current active-VT routing and graphics handoff behavior

Compatibility rules:

1. Old producers still emit anonymous events into the legacy stream.
2. Old consumers still receive the same event format and VT behavior.
3. New named producers additionally feed the legacy stream, so old consumers continue to see those events.
4. No caller is forced to understand hotplug or enumeration.

## 15. Non-Goals

This design does **not** include:

- capability discovery (`keyboard` vs `mouse` metadata)
- kernel support or syscall ABI changes
- replacing `orbclient::Event` with a new event format
- changing VT ownership, display handoff, or control command semantics
- automatic migration of existing daemons

## 16. Implementation Checklist

Another developer implementing this design should be able to proceed in this order:

1. extend `Handle` and `InputScheme` state
2. teach `openat()` to parse `producer/{name}`, `events`, and dynamic device names
3. add root `getdents()` support for `SchemeRoot`
4. refactor `write()` so producer type is detected before routing
5. fan out named-producer events to matching `DeviceConsumer` handles and the existing legacy path
6. add hotplug queue serialization helpers
7. extend `fevent()` and daemon notification loop for `DeviceConsumer` and `HotplugEvents`
8. add cleanup in `on_close()` for `NamedProducer`
9. extend `lib.rs` with the new handle types and directory lister
10. migrate `ps2d` with a named-producer-first, legacy-fallback strategy

## 17. Final Outcome

After this enhancement:

- Orbital and any other legacy consumer continue to work as-is.
- `ps2d` and future drivers can publish stable device names.
- `evdevd` and similar services can subscribe to exactly one device stream.
- userspace can enumerate live input devices and react to hotplug events.

That solves the current anonymity problem without changing the kernel or breaking the existing Redox input stack.
