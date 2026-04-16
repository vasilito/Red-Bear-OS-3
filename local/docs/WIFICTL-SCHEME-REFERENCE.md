# `wifictl:` Scheme Reference

This document describes the current bounded `/scheme/wifictl` surface exposed by
`redbear-wifictl`.

It is a reference for validation and debugging of the current Intel Wi‑Fi slice. It does **not**
imply that Wi‑Fi connectivity is fully supported.

## Root Layout

Top-level entries:

- `wifictl:/ifaces`
- `wifictl:/capabilities`

## Per-interface entries

For each interface under `wifictl:/ifaces/<iface>/`, the scheme currently exposes:

### Read-only status/state nodes

- `status`
- `link-state`
- `firmware-status`
- `transport-status`
- `transport-init-status`
- `activation-status`
- `connect-result`
- `disconnect-result`
- `scan-results`
- `last-error`

### Read/write profile/config nodes

- `ssid`
- `security`
- `key`

### Write-triggered control nodes

- `scan`
- `prepare`
- `transport-probe`
- `init-transport`
- `activate-nic`
- `connect`
- `disconnect`
- `retry`

## Current bounded lifecycle

The bounded Intel path currently treats the Wi‑Fi lifecycle as:

1. `prepare`
2. `transport-probe`
3. `init-transport`
4. `activate-nic`
5. `connect`
6. `disconnect`
7. `retry`

The scheme records the last reported bounded connect/disconnect metadata in `connect-result` and
`disconnect-result`.

## Interpretation guidance

- Presence of the scheme means the control surface exists, not that a real Wi‑Fi link is proven.
- `connect-result` and `disconnect-result` are lifecycle evidence surfaces, not proof of real AP
  authentication or real packet flow.
- `scan-results` may reflect bounded or synthetic runtime outcomes unless and until hardware-backed
  scan evidence is captured on a real target.

## Related documents

- `local/docs/WIFI-IMPLEMENTATION-PLAN.md`
- `local/docs/WIFI-VALIDATION-RUNBOOK.md`
- `local/docs/SCRIPT-BEHAVIOR-MATRIX.md`
