# Intel HDA Implementation Plan

**Version:** 1.0 (2026-04-24)
**Status:** Draft execution plan
**Scope owner:** Audio subsystem (legacy HDA + Intel DSP decision path)

## Purpose

This document defines the concrete execution plan for implementing full Intel audio support in Red Bear OS, using Linux 7.0 source code in-tree as donor reference material.

"Full Intel support" is split into three tracks:

1. Legacy PCI HDA controller + analog codecs (current `ihdad` path)
2. HDMI/DP digital audio over HDA links
3. Modern Intel DSP-class platforms (SOF/AVS-class routing, not legacy-only HDA)

## Why This Plan Is Needed

Current in-tree evidence shows `ihdad` is an early implementation, not a complete Intel audio stack:

- Single-codec assumption in enumeration logic (`device.rs`)
- Unimplemented controller interrupt handler (`handle_controller_interrupt`)
- Fixed-format playback setup (44.1kHz / 16-bit / stereo)
- Incomplete scheme surface (`Handle::Todo`-centric behavior)
- No complete capture path integration in `audiod` (`TODO: audio input`)
- Historical hardware report: "No audio, HDA driver cannot find output pins"

## Current Stack Snapshot

### Driver and daemon surface

- `ihdad` registers `audiohw`
- `audiod` opens `/scheme/audiohw` and exposes `/scheme/audio`
- SDL backends use `/scheme/audio`

### Known contract constraints

- `audiod` mixes fixed-size buffers (`HW_BUFFER_SIZE = 512`)
- `ihdad` stream writes currently assume strict block sizing
- `ihdad` currently hardcodes one primary output format on setup

## Canonical Donor Sources (Linux 7.0 in-tree)

- Controller policy and quirks:
  - `build/linux-kernel-cache/linux-7.0/sound/hda/controllers/intel.c`
- Generic parser and fixup engine:
  - `build/linux-kernel-cache/linux-7.0/sound/hda/common/auto_parser.c`
- Core codec/controller plumbing:
  - `build/linux-kernel-cache/linux-7.0/sound/hda/common/`
- Vendor codec implementations:
  - `build/linux-kernel-cache/linux-7.0/sound/hda/codecs/`
- Intel DSP route-selection policy:
  - `build/linux-kernel-cache/linux-7.0/sound/hda/core/intel-dsp-config.c`
- Modern Intel DSP implementations:
  - `build/linux-kernel-cache/linux-7.0/sound/soc/sof/intel/`
  - `build/linux-kernel-cache/linux-7.0/sound/soc/intel/avs/`

## Execution Model

The plan is organized as issue-sized work packages (`HDA-001`..`HDA-012`).

### Phase A: Legacy HDA correctness (must complete first)

#### HDA-001 — Multi-codec and function-group support

**Goal:** Remove single-codec assumptions and support real codec topology.

**Files:**
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/node.rs`

**Acceptance criteria:**
- Codec enumeration includes all detected codecs
- Bring-up does not assume first codec is the audio path
- `audiohw:codec` dump reflects multi-codec topology

#### HDA-002 — Controller interrupts and unsolicited events

**Goal:** Implement real controller interrupt handling and unsol event dispatch.

**Files:**
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/cmdbuff.rs`

**Acceptance criteria:**
- `handle_controller_interrupt()` is non-stub
- Jack-related unsol events are observable and processed
- No interrupt-ack regressions under continuous playback

#### HDA-003 — Format/rate/channel negotiation

**Goal:** Replace fixed-format startup with negotiated stream format.

**Files:**
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/stream.rs`

**Acceptance criteria:**
- Driver selects supported stream format from capabilities
- Unsupported format requests fail deterministically
- Startup no longer assumes 44.1kHz/16-bit/stereo only

#### HDA-004 — Real scheme endpoint model (`pcmout`/`pcmin`)

**Goal:** Replace `Handle::Todo` behavior with structured stream handles.

**Files:**
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/audiod/src/scheme.rs`

**Acceptance criteria:**
- Distinct playback and capture endpoints exist
- Handle lifecycle and permissions are explicit
- Multiple clients can be supported without implicit index-0 fallback

#### HDA-005 — Capture and duplex path

**Goal:** Implement and validate simultaneous input/output.

**Files:**
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/stream.rs`
- `recipes/core/base/source/audiod/src/main.rs`
- `recipes/core/base/source/audiod/src/scheme.rs`

**Acceptance criteria:**
- Capture endpoint is functional
- Duplex playback/capture runs stably for bounded runtime tests
- `audiod` input TODO is removed

### Phase B: Parser, fixups, and quirk-driven stability

#### HDA-006 — Generic parser + fixup framework

**Goal:** Add parser/fixup framework equivalent to Linux generic HDA model.

**Files:**
- New parser/fixup module(s) under `ihdad/src/hda/`
- Integration in `device.rs`

**Acceptance criteria:**
- Pin/path selection is parser-driven, not heuristic-only
- Fixups can be applied by device identity and pin/config criteria
- Targeted fixup can resolve known "no output pins" class failures

#### HDA-007 — Audio quirk data pipeline

**Goal:** Add audio quirk extraction and runtime loading pattern aligned with current quirks system.

**Files:**
- `local/scripts/extract-linux-quirks.py` (extend for HDA tables)
- `local/recipes/drivers/redox-driver-sys/source/src/quirks/mod.rs` (add audio quirk model)
- `local/recipes/system/redbear-quirks/source/quirks.d/` (add audio quirk TOML)

**Acceptance criteria:**
- Audio quirk entries load from `/etc/quirks.d`
- Driver behavior can be changed by data without code edits
- At least MSI/probe/position/power policy classes represented

#### HDA-008 — Controller policy parity slice

**Goal:** Add minimum policy knobs parity with Linux HDA controller behavior.

**Files:**
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/drivers/audio/ihdad/src/main.rs`

**Initial parity targets:**
- MSI policy
- single-command fallback policy
- codec probe mask
- DMA position-fix policy
- jack poll fallback policy

**Acceptance criteria:**
- Policies are configurable and observable
- Policy defaults can be influenced by quirk data

### Phase C: Digital audio completeness

#### HDA-009 — HDMI/DP audio path

**Goal:** Implement digital codec path handling including ELD and sink constraints.

**Files:**
- New digital-audio module(s) in `ihdad/src/hda/`
- Integration points in `device.rs`

**Acceptance criteria:**
- HDMI/DP codec path is detected and usable on supported hardware/VMs
- ELD-informed format/channel limitations are honored

### Phase D: Modern Intel audio (DSP-class)

#### HDA-010 — Intel audio route dispatcher

**Goal:** Add driver-selection logic equivalent to Linux `intel-dsp-config` principles.

**Files:**
- New dispatcher logic in audio/pcid integration path
- `recipes/core/base/source/drivers/audio/ihdad/config.toml` and related registration surfaces

**Acceptance criteria:**
- cAVS/SOF-class devices are not incorrectly routed to legacy-only behavior
- Route decision uses bounded platform traits (PCI class/prog-if + board traits)

#### HDA-011 — SOF/AVS-class implementation track

**Goal:** Provide a modern Intel DSP-capable driver path separate from legacy `ihdad`.

**Donor roots:**
- `sound/soc/sof/intel`
- `sound/soc/intel/avs`

**Acceptance criteria:**
- At least one Intel cAVS/SOF-class machine can produce bounded playback
- Legacy HDA path remains intact on legacy devices

### Phase E: Desktop ecosystem compatibility

#### HDA-012 — PipeWire/PulseAudio compatibility bridge

**Goal:** Bridge Redox native audio to desktop software expecting PipeWire/PulseAudio APIs.

**Acceptance criteria:**
- KDE desktop audio consumers can produce sound through compatibility layer
- Scope and claim language remains bounded (no overclaim)

## Validation Gates

### G1 — Legacy HDA playback stability

- Environment: QEMU HDA and at least one bare-metal Intel HDA device
- Criteria:
  - Sustained playback duration threshold met
  - No IRQ storm, no driver lockup
  - No repeated timeout errors from CORB/RIRB paths

### G2 — Jack event behavior

- Environment: bare metal with physical jack
- Criteria:
  - Plug/unplug detected
  - Route switches correctly
  - Speaker mute policy behaves as expected

### G3 — Duplex stability

- Environment: bare metal preferred; QEMU for baseline
- Criteria:
  - Capture + playback concurrently
  - No starvation/deadlock in scheme processing

### G4 — Quirk efficacy

- Criteria:
  - At least 3 hardware-specific issues fixed by data-driven quirks
  - Fixes do not require permanent ad hoc branches in main flow

### G5 — Modern Intel path

- Environment: Intel cAVS/SOF-class system
- Criteria:
  - Route dispatcher selects modern path
  - Bounded playback success via DSP-capable path

## Risk and Dependency Notes

1. **Main risk:** Treating SOF/AVS systems as legacy HDA-only.
2. **Main technical debt risk:** Hardcoded policy instead of quirk-backed data.
3. **Integration dependency:** `audiod` contract must evolve in lockstep with `ihdad` stream model.
4. **Desktop dependency:** KDE audio integration remains blocked without compatibility bridge even if kernel/driver path works.

## Initial Prioritization (strict order)

1. HDA-001 through HDA-005
2. HDA-006 through HDA-008
3. HDA-009
4. HDA-010 and HDA-011
5. HDA-012

## HDA-001 Implementation Blueprint

This section defines the concrete first code slice for `HDA-001` (multi-codec + function-group support).

### Objective

Remove hardcoded codec `0` traversal and make codec/AFG/widget discovery data-driven from `STATESTS`.

### Current hotspots

- `ihdad` codec discovery currently hardcodes `let codec: u8 = 0` during enumeration.
- Widget addressing and lists (`outputs`, `inputs`, pins) are global vectors not grouped by codec/function group.
- The scheme dump path (`audiohw:codec`) assumes a single codec payload.

### Files to edit

- `recipes/core/base/source/drivers/audio/ihdad/src/hda/device.rs`
- `recipes/core/base/source/drivers/audio/ihdad/src/hda/node.rs` (only if helper fields/methods are needed)

### Step-by-step patch plan

1. Introduce per-codec topology container in `device.rs`.

   Add internal structures:

   - `CodecTopology`
     - `codec_addr: CodecAddr`
     - `afgs: Vec<NodeAddr>`
     - `widget_map: HashMap<WidgetAddr, HDANode>`
     - `outputs: Vec<WidgetAddr>`
     - `inputs: Vec<WidgetAddr>`
     - `output_pins: Vec<WidgetAddr>`
     - `input_pins: Vec<WidgetAddr>`
     - `beep_addr: Option<WidgetAddr>`

   - `IntelHDA` field:
     - `codecs_topology: HashMap<CodecAddr, CodecTopology>`

2. Replace global widget collections with codec-scoped accessors.

   Keep existing fields temporarily for migration safety, but make enumeration write to `codecs_topology` first.
   After compile + smoke pass, remove stale globals (`outputs`, `inputs`, `widget_map`, pin vectors, `beep_addr`).

3. Refactor `enumerate()` to iterate all detected codecs.

   - Use `self.codecs` as source (populated in `reset_controller()` from `STATESTS`).
   - For each codec:
     - Read root node `(codec, 0)`.
     - Iterate all function groups in root range.
     - Filter audio function groups (`function_group_type` audio class).
     - Enumerate widgets and classify into per-codec topology lists.

4. Add safe codec selection helper for playback bring-up.

   Add helper:
   - `fn pick_primary_codec_for_output(&self) -> Option<CodecAddr>`

   Selection policy v1:
   - First codec with at least one `output_pin` and one `AudioOutput` widget.
   - Stable tie-breaker: lowest codec address.

5. Make `find_best_output_pin()` codec-aware.

   Change signature from global behavior to:
   - `fn find_best_output_pin(&mut self, codec: CodecAddr) -> Result<WidgetAddr>`

   Ensure all widget lookups use the selected codec topology map.

6. Update path walk helpers to consume codec-scoped maps.

   - `find_path_to_dac()` should use the selected codec topology `widget_map`.
   - Avoid `.unwrap()` on map lookups in traversal; return `None`/`Err(ENODEV)` on missing nodes.

7. Update `configure()` to use selected codec.

   - Choose codec via `pick_primary_codec_for_output()`.
   - Call `find_best_output_pin(codec)`.
   - Resolve DAC path only within that codec.

8. Update codec dump endpoint to expose all codecs.

   - Keep `openat("codec")` behavior, but include per-codec sections in output.
   - Optional follow-up: add `codec/<n>` path support; not required for first slice.

9. Guard rails for no-audio cases.

   - If codecs are present but no valid output topology found, return structured `ENODEV`.
   - Do not panic on `No output pins`.

### Non-goals for HDA-001

- Jack unsolicited handling (`HDA-002`)
- Capture stream enablement (`HDA-005`)
- Policy quirks (`HDA-008`)
- HDMI/DP ELD path (`HDA-009`)

### Compile + runtime checks for this slice

1. Build driver package:

   - `./target/release/repo cook recipes/core/base`
   - or full base target flow already used by this tree

2. Boot in QEMU with HDA enabled:

   - validate `ihdad` starts without panic
   - read codec dump from `audiohw:codec`

3. Verify acceptance:

   - Multiple codec entries are shown when available
   - Single-codec machines still work
   - No regression in existing playback path on QEMU ICH9 HDA

### Exit criteria for closing HDA-001

- Enumeration is no longer hardcoded to codec 0.
- Playback path can choose a valid codec deterministically.
- Codec dump includes all detected codecs.
- `ihdad` no longer panics when output pins are missing on codec 0 but present on another codec.

## Claim Language Policy

Until G1-G5 gates are met, support claims must remain bounded:

- Use: "builds", "enumerates", "bounded playback proof"
- Avoid: "full Intel audio support" or broad compatibility claims

## Related Documents

- `local/docs/LINUX-BORROWING-RUST-IMPLEMENTATION-PLAN.md`
- `local/docs/QUIRKS-SYSTEM.md`
- `local/docs/QUIRKS-IMPROVEMENT-PLAN.md`
- `local/docs/CONSOLE-TO-KDE-DESKTOP-PLAN.md`
- `docs/05-KDE-PLASMA-ON-REDOX.md`
- `recipes/core/base/source/drivers/COMMUNITY-HW.md`
