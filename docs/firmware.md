# Firmware in Red Bear OS

## Purpose

This document defines the Red Bear firmware policy.

Firmware is treated as third-party runtime content, not as normal project source code.

## Basic Rules

- firmware is third-party
- firmware licenses vary by vendor and artifact
- firmware remains under its own licenses
- firmware is redistributed unmodified
- firmware is loaded at runtime from the filesystem
- firmware should not be embedded into driver binaries

## Source and Packaging Model

Red Bear should package firmware separately from the core OS logic.

Recommended package-group model:

- `firmware-base`
- `firmware-intel`
- `firmware-amd`
- `firmware-wifi`

The current Red Bear package path for the broad upstream firmware corpus is:

- `local/recipes/system/redbear-firmware/`

That package is intended to stage firmware under:

- `/lib/firmware/`

License metadata should remain clearly separated inside the firmware tree, for example under:

- `/lib/firmware/LICENSES/`

## Licensing and Redistribution

The practical downstream model is the same one used by Linux distributions:

- Linux distributions ship `linux-firmware` as a separate package
- the operating system itself can remain under its own license
- firmware stays under the vendor license documented in `WHENCE` and related license files

Red Bear should follow the same model.

Do not claim a single Red Bear repo-wide license applies to the firmware blobs themselves.

## What Red Bear Must Not Do

- do not claim firmware is MIT just because Red Bear OS code is MIT-like or permissive
- do not remove vendor license files or `WHENCE`
- do not modify firmware blobs
- do not merge firmware blobs into normal source trees without clear separation
- do not assume every blob is redistributable without checking upstream `WHENCE` / license metadata

## Runtime Loading Rule

Drivers and userspace daemons should request firmware from the filesystem at runtime.

For Red Bear, the canonical runtime path is:

- `/lib/firmware/...`

The current helper daemon for that model is:

- `firmware-loader` providing `scheme:firmware`

This keeps the architecture cleaner and legally safer than embedding blobs into binaries.

## Upstream References

- upstream firmware source: `linux-firmware`
- upstream license and redistribution metadata: `WHENCE`
- vendor-specific license files: `LICENCE.*`, `LICENSE*`

## Bottom Line

Red Bear can distribute a Linux-firmware-derived firmware package, but it must do so as separate
firmware content with its own license metadata, installed under `/lib/firmware/`, and loaded at
runtime rather than compiled into project binaries.
