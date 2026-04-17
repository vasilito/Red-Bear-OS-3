# Red Bear OS Wi‑Fi Validation Runbook

This runbook is the canonical operator path for exercising the current bounded Intel Wi‑Fi stack on
either a real Red Bear OS target or a VFIO-backed Red Bear guest.

It does **not** claim that Wi‑Fi is fully solved. Its job is to make the remaining hardware/runtime
validation step reproducible and evidence-oriented.

## Goal

Produce one or both of the following from a real target execution:

- a successful bounded Wi‑Fi lifecycle run (`redbear-phase5-wifi-check`)
- a structured evidence bundle (`redbear-phase5-wifi-capture`) for debugging real failures

## Path A — Bare Metal Runtime Validation

Use this when Red Bear OS is booted on a real machine with a supported Intel Wi‑Fi device.

### In target runtime

For an interactive operator path before or alongside the packaged checkers, the new console client is:

```bash
redbear-netctl-console
```

It is a Redox-native **ncurses** terminal client, and it uses the same bounded `/scheme/wifictl`
and `/etc/netctl` surfaces as the scripted/operator flows.

```bash
redbear-phase5-wifi-run wifi-open-bounded wlan0 /tmp/redbear-phase5-wifi-capture.json
test-wifi-baremetal-runtime.sh
```

### Artifacts to preserve

- `/tmp/redbear-phase5-wifi-capture.json`
- terminal output from `redbear-phase5-wifi-check`
- terminal output from `test-wifi-baremetal-runtime.sh`
- any serial console log captured during the run

Recommended host-side naming after copying artifacts off the target:

- `wifi-baremetal-capture.json`
- `wifi-baremetal-serial.log`
- `wifi-baremetal-console.log`

Recommended staging pattern on the host:

```bash
run_dir=./wifi-baremetal-$(date +%Y%m%d-%H%M%S)
mkdir -p "$run_dir"
# copy the capture/log files into that directory
./local/scripts/package-wifi-validation-artifacts.sh \
  "${run_dir}.tar.gz" \
  "$run_dir"
```

Optional packaging step on the host:

```bash
./local/scripts/package-wifi-validation-artifacts.sh
```

The resulting tarball now includes a small manifest file with the packaged paths and file checksums
for regular files when `sha256sum` is available on the host.

Optional summary step on the host:

```bash
./local/scripts/summarize-wifi-validation-artifacts.sh ./wifi-baremetal-capture.json
# or
./local/scripts/summarize-wifi-validation-artifacts.sh ./wifi-validation-artifacts.tar.gz
# or use the packaged analyzer directly on the captured JSON
redbear-phase5-wifi-analyze ./wifi-baremetal-capture.json
```

Optional one-shot post-run step on the host:

```bash
./local/scripts/finalize-wifi-validation-run.sh \
  ./wifi-baremetal-capture.json \
  ./wifi-validation-artifacts.tar.gz \
  ./wifi-baremetal-serial.log \
  ./wifi-baremetal-console.log
```

## Path B — VFIO/QEMU Validation

Use this when a host can safely detach an Intel Wi‑Fi PCI function and pass it through to a Red Bear
guest.

### On the host

First, validate the host prerequisites:

```bash
sudo ./local/scripts/validate-wifi-vfio-host.sh \
  --host-pci 0000:xx:yy.z \
  --expect-driver iwlwifi
```

This preflight now exits non-zero when blockers are found, so it is safe to use as an automation
gate before attempting VFIO passthrough validation.

Then run the full passthrough validation wrapper:

```bash
sudo ./local/scripts/run-wifi-passthrough-validation.sh \
  --host-pci 0000:xx:yy.z \
  --host-driver iwlwifi \
  --artifact-dir ./wifi-validation-$(date +%Y%m%d-%H%M%S)
```

Default output artifacts from that wrapper:

- `./wifi-passthrough-capture.json`
- `./wifi-passthrough-capture.json.meta.json`

If `--artifact-dir` is provided, those files are written into that directory instead.

Recommended packaging step afterwards:

```bash
./local/scripts/package-wifi-validation-artifacts.sh \
  ./wifi-passthrough-artifacts.tar.gz \
  ./wifi-validation-YYYYMMDD-HHMMSS
```

That tarball also includes the manifest/checksum file described above.

Optional summary step afterwards:

```bash
./local/scripts/summarize-wifi-validation-artifacts.sh ./wifi-passthrough-artifacts.tar.gz
# or
redbear-phase5-wifi-analyze ./wifi-passthrough-capture.json
```

Optional one-shot post-run step afterwards:

```bash
./local/scripts/finalize-wifi-validation-run.sh \
  ./wifi-passthrough-capture.json \
  ./wifi-passthrough-artifacts.tar.gz \
  ./wifi-passthrough-capture.json.meta.json
```

For structured follow-up after a failed run, use:

- `local/docs/WIFI-VALIDATION-ISSUE-TEMPLATE.md`

You can override those paths explicitly if needed:

```bash
sudo ./local/scripts/run-wifi-passthrough-validation.sh \
  --host-pci 0000:xx:yy.z \
  --host-driver iwlwifi \
  --capture-output ./wifi-passthrough-capture.json \
  --metadata-output ./wifi-passthrough-capture.meta.json
```

The wrapper handles:

1. binding the selected device to `vfio-pci`
2. launching the Red Bear guest passthrough harness
3. running `redbear-phase5-network-check` and `redbear-phase5-wifi-run` inside the guest
4. collecting the packaged Wi‑Fi capture bundle back to the host
5. writing a host-side metadata sidecar for the run
6. restoring the host driver afterwards

`redbear-phase5-network-check` in that flow is the bounded `redbear-full` desktop/network plumbing
proof. It should not be read as closing the Wi‑Fi implementation plan's later Phase W5
runtime-reporting-and-recovery milestone by itself.

### Artifact to preserve

- `./wifi-passthrough-capture.json`
- `./wifi-passthrough-capture.meta.json`
- full terminal log from the wrapper invocation

Optional packaging step on the host:

```bash
./local/scripts/package-wifi-validation-artifacts.sh
```

## Minimum Evidence for a Real Runtime Attempt

At minimum, keep all of the following together:

- the capture JSON bundle
- the console output of the checker/wrapper
- the exact PCI BDF used for the Intel Wi‑Fi device
- whether the run was bare metal or VFIO/QEMU

## What Success Means Today

Current success is still **bounded** success:

- the Intel driver/runtime lifecycle can be exercised on a real target
- the Wi‑Fi control/profile/reporting stack can observe that lifecycle, including honest bounded
  pending/associating connect state when real association is not yet proven
- the default bounded validation profile is `wifi-open-bounded`, which intentionally avoids turning
  DHCP handoff into a false requirement for lifecycle-only validation
- the packaged runtime checker currently proves that bounded open-profile path by default; WPA2-PSK
  is implemented and covered by host/unit-level regressions, but is not yet the default packaged
  runtime validation path
- a structured evidence bundle is captured for debugging

This is **not yet** the same as:

- real AP scan/association proof
- real packet/data-path proof
- DHCP success over a true wireless link
- validated end-to-end Wi‑Fi connectivity

Those remain the next debugging targets after the first real target execution.
