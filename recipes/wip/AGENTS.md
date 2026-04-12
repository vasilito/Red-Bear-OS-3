# RECIPES/WIP — WORK-IN-PROGRESS PORTS

Experimental ports not yet ready for production. Wayland, KDE, GNOME, and driver WIP.

## STRUCTURE

```
recipes/wip/
├── wayland/              # 21 Wayland-related recipes
│   ├── libwayland/       # Wayland protocol library (builds with redox.patch)
│   ├── wayland-protocols/# Wayland protocol definitions
│   ├── wayland-rs/       # Rust Wayland bindings
│   ├── cosmic-comp/      # COSMIC compositor (no keyboard input yet)
│   ├── smallvil/         # Smithay-based compositor (basic, slow)
│   ├── libcosmic-wayland/# COSMIC Wayland client library
│   ├── winit-wayland/    # winit with Wayland backend
│   ├── softbuffer-wayland/# softbuffer with Wayland backend
│   ├── iced-wayland/     # Iced GUI with Wayland backend
│   ├── gtk3/             # GTK3 Wayland support
│   ├── wlroots/          # wlroots (not compiled/tested)
│   ├── sway/             # sway (not compiled/tested)
│   ├── hyprland/         # hyprland (not compiled/tested)
│   ├── xwayland/         # XWayland (partially patched)
│   └── seatd/            # Seat daemon (recipe exists, untested)
├── kde/                  # 9 KDE app recipes
│   ├── kde-dolphin/      # File manager (needs kio)
│   ├── kdenlive/         # Video editor (needs MLT)
│   ├── krita/            # Painting (needs Qt + OpenGL)
│   ├── kdevelop/         # IDE (needs Qt + kio)
│   └── ...               # okteta, ktorrent, ark, kamoso, kpatience
├── libs/                 # WIP libraries
│   └── tls/openssl3/     # OpenSSL 3.x port
├── monitors/             # System monitors
│   └── bottom/           # bottom system monitor
└── drivers/              # WIP driver ports (planned)
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Fix Wayland build | `wayland/libwayland/redox.patch` — stubs 7 POSIX APIs |
| Add Wayland compositor | `wayland/<name>/recipe.toml` — use `dependencies = ["libwayland"]` |
| Fix cosmic-comp | `wayland/cosmic-comp/` — missing libinput causes no keyboard |
| Work on smallvil | `wayland/smallvil/` — Smithay-based, already running |
| Port a KDE app | Copy existing recipe pattern, add `#TODO` header |
| Add Qt port | Create `wip/qt/qtbase/recipe.toml` (not yet started) |

## WAYLAND STATUS

- **libwayland**: Builds with redox.patch stubbing 7 POSIX APIs
- **cosmic-comp**: Partially working, no keyboard input (missing libinput)
- **smallvil**: Basic compositor running, poor performance
- **wlroots/sway/hyprland**: Not compiled or tested
- **xwayland**: Partially patched
- **Blockers**: POSIX gaps (M1), evdevd input (M2), DRM/KMS (M3)

## KDE STATUS

- 9 app recipes exist but all blocked on Qt6 + KDE Frameworks
- No qtbase recipe yet (Phase KDE-A prerequisite)
- See `docs/05-KDE-PLASMA-ON-REDOX.md` for full 3-phase plan

## CONVENTIONS

- ALL WIP recipes MUST start with `#TODO` explaining what's missing
- BLAKE3 hashes optional for WIP
- Test with `make r.<package>` before adding to config
- When ready: move from `wip/` to appropriate category, add BLAKE3 hash
