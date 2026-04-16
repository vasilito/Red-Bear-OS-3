# Wi‑Fi Validation Issue Template

Use this template after the first real bare-metal or VFIO-backed Intel Wi‑Fi validation run.

## Environment

- Run type: bare metal / VFIO-backed guest
- Host PCI BDF (if VFIO):
- Expected host driver before VFIO (if applicable):
- Red Bear profile: `wifi-open-bounded` / `wifi-dhcp` / other
- Interface: `wlan0` / other
- Intel device model:

## Commands Used

List the exact command(s) you ran, for example:

```bash
redbear-phase5-wifi-run wifi-open-bounded wlan0 /tmp/redbear-phase5-wifi-capture.json
```

or

```bash
./local/scripts/run-wifi-passthrough-validation.sh --host-pci 0000:xx:yy.z --host-driver iwlwifi --artifact-dir ./wifi-validation-YYYYMMDD-HHMMSS
```

## Expected Outcome

Describe what you expected to happen.

## Actual Outcome

Describe what actually happened.

## Artifact Paths

- Capture JSON:
- Metadata JSON (if VFIO):
- Packaged tarball (if created):
- Serial log:
- Console log:

## Analyzer Output

Paste the output of:

```bash
redbear-phase5-wifi-analyze <capture.json>
```

## Key Signals

- `driver_probe` result:
- `driver_status` result:
- `wifictl_probe` result:
- `wifictl_status` result:
- `netctl_status` result:
- `wifi_connect_result`:
- `wifi_disconnect_result`:
- `last_error`:

## Suspected Blocker Class

One or more of:

- device-detection
- firmware
- association-control-path
- disconnect-lifecycle
- dhcp-or-addressing
- reporting-surface
- runtime-failure
- bounded-lifecycle-pass-no-real-link-proof

## Notes

Anything else that seems relevant for reproducing or narrowing the issue.
