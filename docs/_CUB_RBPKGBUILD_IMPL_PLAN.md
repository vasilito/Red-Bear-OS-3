Red Bear OS Packaging System — Formal Spec v0.1

0. Guiding Principles
Native-first
All builds target: x86_64-unknown-redox
All install artifacts: pkgar packages
Cookbook is the build engine
RBPKGBUILD = thin wrapper over Cookbook recipe
PKGBUILD is input, not execution
Never execute arbitrary PKGBUILD on host
Always convert → RBPKGBUILD → Cookbook
Single tool
cub = one CLI binary with subcommands/flags
Deterministic builds
Controlled env
No implicit host dependencies
Security-first
Sandbox builds
Review BUR packages before install

1. RBPKGBUILD Specification

1.1 File Format
Format: TOML
Filename: RBPKGBUILD
Versioned schema
format = 1

1.2 Top-Level Sections
format = 1

[package]
[source]
[dependencies]
[build]
[install]
[patches]
[compat]
[policy]

1.3 [package]
[package]
name = "ripgrep"
version = "14.1.0"
release = 1
description = "Fast recursive search tool"
homepage = "https://github.com/BurntSushi/ripgrep"
license = ["MIT", "Unlicense"]
architectures = ["x86_64-unknown-redox"]
maintainers = ["name <email>"]
Rules
name: lowercase, [a-z0-9-_]+
version: upstream version
release: integer (Red Bear-specific revision)
architectures: must include x86_64-unknown-redox

1.4 [source]
[source]
sources = [
  { type = "tar", url = "...", sha256 = "..." },
  { type = "git", url = "...", rev = "..." }
]
Supported types
tar
git
Rules
All sources must be verifiable (hash or commit)
No implicit downloads during build

1.5 [dependencies]
[dependencies]
build = ["cargo", "rust"]
runtime = []
check = []
optional = []
provides = []
conflicts = []
Rules
Names must resolve via system mapping
Must not reference Arch package names directly

1.6 [build]

Maps directly to Cookbook templates.

[build]
template = "cargo" # or configure, cmake, meson, custom
Optional fields
cargo
[build]
template = "cargo"
release = true
features = []
configure
[build]
template = "configure"
args = ["--prefix=/usr"]
cmake
[build]
template = "cmake"
build_dir = "build"
custom
[build]
template = "custom"

prepare = [
  "patch -p1 < patches/fix.patch"
]

build = [
  "make -j$CORES"
]

check = [
  "make test"
]

install = [
  "make DESTDIR=$DESTDIR install"
]

1.7 [install]

Declarative install mapping.

[install]
bins = [
  { from = "target/.../rg", to = "/usr/bin/rg" }
]

libs = []
headers = []
docs = ["README.md"]
man = []
Rules
All paths relative to build output
Must install into staged root (DESTDIR)

1.8 [patches]
[patches]
files = [
  "patches/0001-fix-redox.patch"
]

1.9 [compat]

Tracks conversion origin.

[compat]
imported_from = "aur"
original_pkgbuild = "PKGBUILD"
conversion_status = "partial" # full | partial | manual
target = "x86_64-unknown-redox"

1.10 [policy]
[policy]
allow_network = false
allow_tests = true
review_required = true

2. .RBSRCINFO (Metadata Cache)
Purpose
Fast search/index
No recipe parsing needed
Format (INI-like)
pkgname = ripgrep
pkgver = 14.1.0
pkgrel = 1
pkgdesc = Fast recursive search tool
arch = x86_64-unknown-redox

depends = 
makedepends = cargo rust

source = https://...
sha256sums = ...

provides = 
conflicts = 

3. BUR Repository Spec
Structure
ripgrep/
  RBPKGBUILD
  .RBSRCINFO
  patches/
  import/
    PKGBUILD
    report.txt

4. cub CLI Specification

4.1 General
Single binary: cub
Rust implementation
Subcommands via flags (not separate tools)

4.2 Core Commands
Search
cub -Ss <query>
Install
cub -S <package>

Resolution order:

official repo
BUR (RBPKGBUILD)
AUR import (optional flag)
Build local
cub -B .
Fetch recipe
cub -G <package>
Inspect
cub -Pi <package|RBPKGBUILD>
Update system
cub -Sua
Clean cache
cub -Sc
Convert AUR
cub --import-aur <url|name>

Outputs:

RBPKGBUILD
patches/
report.txt

5. PKGBUILD → RBPKGBUILD Conversion

5.1 Conversion Stages

Stage 1 — Parse

Extract:

pkgname
pkgver
depends
source
functions

Stage 2 — Normalize
Resolve arrays
Expand variables
Strip bash constructs

Stage 3 — Map

PKGBUILD	RBPKGBUILD
pkgname	package.name
pkgver	package.version
depends	dependencies.runtime
makedepends	dependencies.build
source	source.sources

Stage 4 — Detect build system

Patterns:

Pattern	Template
cargo build	cargo
./configure	configure
cmake	cmake
meson	meson
none	custom

Stage 5 — Generate RBPKGBUILD
Fill required fields
Insert detected template
Add compat section

Stage 6 — Patch generation

If:

/usr/lib/systemd
/proc
systemctl

→ generate:

patch stub
warning entry
Stage 7 — Report

report.txt

Conversion: PARTIAL

Warnings:
- Uses systemd
- Hardcoded /usr/lib

Actions required:
- Patch install paths
- Remove systemctl usage

5.2 Conversion Modes
Mode	Description
full	fully automated
partial	needs patches
manual	user intervention

6. Build Execution
Environment
TARGET=x86_64-unknown-redox
GNU_TARGET=x86_64-redox

DESTDIR=/build/stage
PREFIX=/usr

CORES=8
Sandbox Rules
No network after fetch
Isolated filesystem
No host writes
Controlled PATH
Execution Flow
cub
 → parse RBPKGBUILD
 → generate Cookbook recipe
 → run build
 → stage files
 → create pkgar
 → install

7. Dependency Mapping
Mapping file
[mapping]
glibc = "relibc-compat"
base-devel = "build-base"

8. Error Handling
Hard Fail
Missing source hash
Unknown build template
Unsupported architecture
Soft Fail (warn)
Linux-specific paths
missing tests
partial conversion

9. Security Model
BUR = untrusted
Require review on first install
Signed packages preferred
Build sandbox enforced

10. MVP Scope
MUST implement
RBPKGBUILD parser
Cookbook adapter
pkgar integration
cub CLI core commands
basic AUR conversion
dependency mapping
sandbox build
MUST NOT implement yet
full PKGBUILD shell compatibility
split packages
pacman compatibility
hook system

11. Final Definition

RBPKGBUILD is a declarative Red Bear build wrapper over Cookbook recipes.
cub is a Rust CLI tool that manages installation, building, and conversion from AUR PKGBUILD into RBPKGBUILD, using BUR as the community repository.
All builds target x86_64-unknown-redox and produce pkgar packages.
