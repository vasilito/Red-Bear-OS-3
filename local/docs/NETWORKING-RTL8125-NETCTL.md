# Red Bear OS Networking: RTL8125 + netctl

## Native stack

Red Bear uses the native Redox wired networking path already present in the base tree:

`pcid-spawner` → native NIC daemon (`rtl8168d`, `e1000d`, `ixgbed`, `virtio-netd`) → `network.*`
scheme → `smolnetd` → `dhcpd` / `netcfg`.

This change keeps RTL8125 in that native path instead of trying to introduce a Linux netdevice,
`sk_buff`, or NAPI compatibility layer into `linux-kpi`.

## RTL8125 path

- Autoload now matches `10ec:8125` in `recipes/core/base/source/drivers/net/rtl8168d/config.toml`.
- The existing Realtek driver binary remains the autoload target (`rtl8168d`).
- The daemon names RTL8125 devices distinctly in its `network.*` scheme name suffix.

This is the narrowest viable implementation path in the current tree. It reuses the existing
userspace driver, PCI spawn, and netstack plumbing already proven for the native Realtek path.

## relibc networking surface

The Redox-facing libc networking surface was extended to stop reporting a fake `stub` interface:

- `net/if.h` now exposes a real `eth0`-based view of the active interface model
- `ifaddrs.h` now returns a populated `eth0` entry
- Redox `ioctl()` now answers the common read-only `SIOCGIF*` queries used by interface-aware apps
- `netinet/in.h` now includes `in6_pktinfo`
- a minimal `resolv.h` is now generated in relibc

This is intentionally aligned with the current single-active-interface design in `smolnetd` and
`netcfg`.

## netctl

Red Bear ships a Redox-native `netctl` compatibility command in `redbear-netctl`.

### Supported profile subset

- `Interface=eth0`
- `Connection=ethernet`
- `IP=dhcp`
- `IP=static`
- `Address=('a.b.c.d/prefix')`
- `Gateway='a.b.c.d'`
- `DNS=('a.b.c.d')`

### Supported commands

- `netctl list`
- `netctl status [profile]`
- `netctl start <profile>`
- `netctl stop <profile>`
- `netctl enable <profile>`
- `netctl disable [profile]`
- `netctl is-enabled [profile]`
- `netctl --boot`

Profiles live in `/etc/netctl`. Shipped examples live in `/etc/netctl/examples/`.

### Boot integration

Red Bear configs install `/usr/lib/init.d/12_netctl.service`, which runs:

```text
netctl --boot
```

If `/etc/netctl/active` contains a profile name, that profile is applied during boot after the
base networking services have started.

## Validation notes

- `redbear-netctl` was type-checked and smoke-tested with a fake runtime root by exercising:
  `list`, `enable`, `status`, and `start`.
- `rtl8168d` type-checks with the RTL8125 autoload configuration in place.
- relibc type-checks with the interface and header updates in place.
- `./local/scripts/validate-vm-network-baseline.sh` verifies the repo-level VM boot chain for
  `redbear-minimal`: `pcid-spawner` → `smolnetd` → `dhcpd` → `netctl --boot` → `wired-dhcp`.
- `./local/scripts/test-vm-network-qemu.sh` launches a VirtIO-backed QEMU run for the same Phase 2
  baseline and prints the in-guest validation commands to run.

## Remaining hardware validation

This repo change set wires RTL8125 through the native path, but real hardware validation is still
required for full confidence in packet I/O on specific RTL8125 revisions.
