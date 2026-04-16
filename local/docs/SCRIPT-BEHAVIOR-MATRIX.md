# Red Bear OS Script Behavior Matrix

## Purpose

This document centralizes what the main repository scripts do and do not handle under the Red Bear
overlay model.

The goal is to remove guesswork from the sync/fetch/apply/build workflow.

## Matrix

| Script | Primary role | What it handles | What it does **not** guarantee |
|---|---|---|---|
| `local/scripts/sync-upstream.sh` | Refresh top-level upstream repo state | fetches upstream, reports conflict risk, rebases repo commits, reapplies build-system overlays via `apply-patches.sh` | does not automatically solve every subsystem overlay conflict; does not by itself make upstream WIP recipes safe shipping inputs |
| `local/scripts/apply-patches.sh` | Reapply durable Red Bear overlays | applies build-system patches, relinks recipe patch symlinks, relinks local recipe overlays into `recipes/` | does not fully rebase stale patch carriers; does not validate runtime behavior; does not decide WIP ownership for you |
| `local/scripts/build-redbear.sh` | Build Red Bear profiles from upstream base + local overlay | applies overlays, builds cookbook if needed, validates profile naming, launches the actual image build | does not guarantee every nested upstream source tree is fresh; does not replace explicit subsystem/runtime validation |
| `scripts/fetch-all-sources.sh` | Fetch recipe source inputs for builds | downloads recipe sources for upstream and local recipes, reports status/preflight, supports config-scoped fetches | does not mean fetched upstream WIP source is the durable shipping source of truth |
| `local/scripts/fetch-sources.sh` | Fetch local overlay source inputs | fetches local overlay recipe sources and keeps the local side ready for build work | does not decide whether upstream should replace the local overlay |
| `local/scripts/build-redbear-wifictl-redox.sh` | Build `redbear-wifictl` for the Redox target with the repo toolchain | prepends `prefix/x86_64-unknown-redox/sysroot/bin` to `PATH` and runs `cargo build --target x86_64-unknown-redox` in the `redbear-wifictl` crate | does not prove runtime Wi-Fi behavior; only closes the target-build environment gap for this crate |
| `local/scripts/test-iwlwifi-driver-runtime.sh` | Exercise the bounded Intel driver lifecycle inside a target runtime | validates bounded probe/prepare/init/activate/scan/connect/disconnect/retry surfaces for `redbear-iwlwifi` on a live target runtime | does not prove real AP association, packet flow, DHCP success over Wi-Fi, or end-to-end connectivity |
| `local/scripts/test-wifi-control-runtime.sh` | Exercise the bounded Wi-Fi control/profile lifecycle inside a target runtime | validates `/scheme/wifictl` control nodes, bounded connect/disconnect behavior, and profile-manager/runtime reporting surfaces on a live target runtime | does not prove real AP association or end-to-end connectivity |
| `local/scripts/test-wifi-baremetal-runtime.sh` | Exercise bounded Intel Wi-Fi runtime lifecycle on a target system | validates driver probe, control probe, bounded connect/disconnect, profile-manager start/stop via the `wifi-open-bounded` profile, Wi-Fi lifecycle reporting, and writes `/tmp/redbear-phase5-wifi-capture.json` on the target | does not prove real AP association, packet flow, DHCP success over Wi-Fi, or end-to-end hardware connectivity |
| `local/scripts/test-wifi-passthrough-qemu.sh` | Launch Red Bear with VFIO-passed Intel Wi-Fi hardware | boots a Red Bear guest with a passed-through Intel Wi-Fi PCI function, auto-runs the in-guest bounded Wi-Fi validation command, and can copy the packaged capture bundle back to a host-side file during `--check` | depends on host VFIO setup and still does not by itself guarantee real AP association or end-to-end Wi-Fi connectivity |
| `local/scripts/test-bluetooth-runtime.sh` | Compatibility guest entrypoint for the bounded Bluetooth Battery Level slice | runs the packaged `redbear-bluetooth-battery-check` helper inside a Redox guest or target runtime | does not run on the host and does not expand the Bluetooth support claim beyond the packaged checker’s bounded scope |
| `local/scripts/test-bluetooth-qemu.sh` | Launch or validate the bounded Bluetooth Battery Level slice in QEMU | boots `redbear-bluetooth-experimental`, auto-runs the packaged checker during `--check`, reruns it in one boot, and reruns it again after a clean reboot | does not prove real controller bring-up, generic BLE/GATT maturity, write/notify support, or real hardware Bluetooth behavior |
| `local/scripts/prepare-wifi-vfio.sh` | Prepare or restore an Intel Wi-Fi PCI function for passthrough | binds a chosen PCI function to `vfio-pci` or restores it to a specified host driver | does not verify guest Wi-Fi functionality and must be used carefully on a host with a safe detachable target device |
| `local/scripts/validate-wifi-vfio-host.sh` | Check whether a host looks ready for Wi-Fi VFIO testing | validates PCI presence, current driver, UEFI firmware, Red Bear image presence, QEMU/expect availability, VFIO module state, and IOMMU group visibility; exits non-zero when blockers are found | does not bind devices or prove the guest Wi-Fi stack works |
| `local/scripts/run-wifi-passthrough-validation.sh` | End-to-end host-side passthrough validation wrapper | prepares VFIO, runs the packaged in-guest Wi-Fi validation path, captures the guest JSON artifact to the host, writes a host-side metadata sidecar, and restores the host driver afterwards | still depends on real VFIO/hardware support and does not itself guarantee end-to-end Wi-Fi connectivity |
| `local/scripts/package-wifi-validation-artifacts.sh` | Bundle Wi-Fi validation evidence into one archive | packages common capture/log artifacts from bare-metal or VFIO validation runs into a single tarball | does not create missing artifacts or validate their contents |
| `local/scripts/summarize-wifi-validation-artifacts.sh` | Summarize Wi-Fi validation evidence quickly | extracts key runtime signals from a capture JSON or packaged tarball for fast triage | does not replace full artifact review or prove runtime correctness |
| `local/scripts/finalize-wifi-validation-run.sh` | One-shot post-run Wi-Fi triage helper | runs the packaged analyzer on a capture JSON and then packages the chosen artifacts into a tarball | still depends on a real target run having produced the capture/artifacts first |

The packaged companion command for those scripts is `redbear-phase5-wifi-check`, which performs the
bounded in-target Wi-Fi lifecycle checks from inside the guest/runtime itself.

The packaged Bluetooth companion command is `redbear-bluetooth-battery-check`, which performs the
bounded Bluetooth Battery Level checks from inside the guest/runtime itself, including repeated
helper runs, daemon-restart coverage, failure-path honesty checks, and stale-state cleanup checks
within the current slice boundary.

The packaged evidence companion is `redbear-phase5-wifi-capture`, which collects the bounded driver,
control, profile-manager, reporting, interface-listing, and scheme-state surfaces — plus `lspci`
and active-profile contents — into a single JSON artifact.

The packaged link-oriented companion is `redbear-phase5-wifi-link-check`, which focuses on whether
the target runtime is exposing interface/address/default-route signals in addition to the bounded
Wi-Fi lifecycle state.

For Redox-target Rust builds of Wi-Fi components such as `redbear-wifictl`, a missing
`x86_64-unknown-redox-gcc` on `PATH` should first be treated as a host toolchain/path issue if the
repo already contains `prefix/x86_64-unknown-redox/sysroot/bin/x86_64-unknown-redox-gcc`.

## Policy Mapping

### Upstream sync

Use `local/scripts/sync-upstream.sh` when the goal is to refresh the top-level upstream Redox base.

This is a repository sync operation, not a guarantee that every local subsystem overlay is already
rebased cleanly.

### Overlay reapplication

Use `local/scripts/apply-patches.sh` when the goal is to reconstruct Red Bear’s overlay on top of a
fresh upstream tree.

This is the core durable-state recovery path.

### Build execution

Use `local/scripts/build-redbear.sh` when the goal is to build a tracked Red Bear profile from the
current upstream base plus local overlay.

### Source refresh

Use `scripts/fetch-all-sources.sh` and `local/scripts/fetch-sources.sh` when the goal is to refresh
recipe source inputs, but do not confuse fetched upstream WIP source with a trusted shipping source.

## WIP Rule in Script Terms

If a subsystem is still upstream WIP, the scripts should be interpreted this way:

- fetching upstream WIP source is allowed and useful,
- syncing upstream WIP source is allowed and useful,
- but shipping decisions should still prefer the local overlay until upstream promotion and reevaluation happen.

That means “script fetched it successfully” is not the same as “Red Bear should now ship upstream’s
WIP version directly.”
