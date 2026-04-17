# Red Bear OS Bluetooth Validation Runbook

This runbook is the canonical operator path for exercising the current bounded Bluetooth Battery
Level slice on Red Bear OS.

It does **not** claim that Bluetooth is broadly solved. Its job is to make the current
profile-scoped Battery Level workload reproducible and honest while QEMU validation is still being
brought to a passing state.

## Goal

Produce one or both of the following:

- a successful bounded Bluetooth validation run via `redbear-bluetooth-battery-check`
- a repeatable QEMU/UEFI validation log via `./local/scripts/test-bluetooth-qemu.sh --check`

## Path A - Host-side QEMU validation

Use this when the host supports the repo's normal x86_64 QEMU/UEFI flow.

### On the host

Build the tracked Bluetooth profile first:

```bash
./local/scripts/build-redbear.sh redbear-bluetooth-experimental
```

Then run the automated QEMU harness:

```bash
./local/scripts/test-bluetooth-qemu.sh --check
```

What that harness is intended to do:

1. boots `redbear-bluetooth-experimental` in QEMU with `qemu-xhci`
2. logs in automatically on the serial console
3. runs `redbear-bluetooth-battery-check` twice in one boot
4. reboots the guest
5. runs `redbear-bluetooth-battery-check` again after the clean reboot

### Artifact to preserve

- the full terminal log from `./local/scripts/test-bluetooth-qemu.sh --check`
- any serial or CI log captured around the run

## Path B - Interactive guest validation

Use this when you want to inspect the runtime manually inside the guest.

### On the host

```bash
./local/scripts/test-bluetooth-qemu.sh
```

### Inside the guest

Run the packaged checker directly:

```bash
redbear-bluetooth-battery-check
```

The legacy guest helper remains as a compatibility wrapper:

```bash
test-bluetooth-runtime.sh
```

Useful supporting commands inside the guest:

```bash
redbear-btusb --status
redbear-btctl --status
redbear-info --verbose
```

## What success means today

Current success is still **bounded** success:

- the explicit-startup `redbear-btusb` and `redbear-btctl` path can be exercised in QEMU
- the packaged checker can be rerun repeatedly in one boot
- the checker covers daemon restart cleanup and disconnect stale-state cleanup within the current
  Battery Level slice
- the exact Battery Service / Battery Level UUID pair can be read through the bounded read-only
  workload and reported conservatively by `redbear-info`

Those are the **target** success conditions for the current QEMU proof. Until the harness exits
cleanly end to end, describe the validation state as “QEMU harness and packaged checker present,
validation still in progress.”

This is **not yet** the same as:

- real controller bring-up proof
- generic BLE or generic GATT maturity
- write support or notify support
- real pairing or broad reconnect semantics
- desktop Bluetooth parity, HID, audio, or passthrough-backed hardware claims
