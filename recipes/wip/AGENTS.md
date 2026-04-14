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
| Fix Wayland build | `wayland/libwayland/redox.patch` — still carries POSIX compatibility workarounds |
| Add Wayland compositor | `wayland/<name>/recipe.toml` — use `dependencies = ["libwayland"]` |
| Fix cosmic-comp | `wayland/cosmic-comp/` — missing libinput causes no keyboard |
| Work on smallvil | `wayland/smallvil/` — Smithay-based, already running |
| Port a KDE app | Copy existing recipe pattern, add `#TODO` header |
| Add Qt port | Prefer the newer `local/recipes/qt/` / `local/recipes/kde/` work over this older note |

## WAYLAND STATUS

- **libwayland**: Builds with `redox.patch`; several POSIX-dependent code paths are still commented out there
- **cosmic-comp**: Partially working, no keyboard input (missing libinput)
- **smallvil**: Basic compositor running, poor performance
- **wlroots/sway/hyprland**: Not compiled or tested
- **xwayland**: Partially patched
- **Blockers**: downstream Wayland patch reduction, libinput/runtime input validation, DRM/KMS hardware/runtime validation

## KDE STATUS

- Older WIP KDE app notes here are stale relative to `local/recipes/kde/` and `config/redbear-kde.toml`
- See `docs/05-KDE-PLASMA-ON-REDOX.md` top-level status note plus `local/docs/QT6-PORT-STATUS.md` for current state

## CONVENTIONS

- ALL WIP recipes MUST start with `#TODO` explaining what's missing
- BLAKE3 hashes optional for WIP
- Test with `make r.<package>` before adding to config
- When ready: move from `wip/` to appropriate category, add BLAKE3 hash
