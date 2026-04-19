# RECIPES/WIP — WORK-IN-PROGRESS PORTS

Experimental ports not yet ready for production. Wayland, KDE, GNOME, and driver WIP.

## OWNERSHIP RULE FOR UPSTREAM WIP

In Red Bear OS, an upstream recipe or subsystem being marked **WIP** changes how we treat it.

### What WIP means for Red Bear

If an upstream recipe, package group, or subsystem is still WIP:

1. Red Bear treats that area as a **local project** rather than a first-class upstream dependency
2. we may study, import, and refresh from the upstream WIP recipe
3. but the version we fix, validate, and ship should live in the Red Bear overlay (`local/recipes/`,
   `local/patches/`, `local/docs/`), not in trust of the upstream WIP tree alone

### What happens when upstream promotes it

If upstream later removes the WIP status and the recipe becomes a first-class supported Redox
package, Red Bear should reevaluate immediately:

- prefer the upstream recipe where it now solves the same problem adequately
- reduce or remove the local Red Bear copy/patches if they are no longer needed
- keep only the Red Bear-specific integration delta that upstream still does not solve

### Practical implication

`recipes/wip/` is therefore not “safe upstream ownership” for Red Bear shipping decisions. For this
project, upstream WIP is a **source of inputs and ideas**, but stable Red Bear delivery should come
from the local overlay until upstream promotes that work.

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
│   └── xwayland/         # XWayland (partially patched)
├── services/
│   └── seatd/            # Seat daemon recipe (service category, runtime trust still open)
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
| Fix Wayland build | `wayland/libwayland/redox.patch` plus recipe-time source rewrites — residual Redox compatibility and scanner/build handling remain |
| Add Wayland compositor | `wayland/<name>/recipe.toml` — use `dependencies = ["libwayland"]` |
| Inspect cosmic-comp status | `wayland/cosmic-comp/` — historical partial bring-up; not the active forward path |
| Inspect smallvil history | `wayland/smallvil/` — historical bounded validation compositor reference only |
| Port a KDE app | Copy existing recipe pattern, add `#TODO` header |
| Add Qt port | Prefer the newer `local/recipes/qt/` / `local/recipes/kde/` work over this older note |

## WAYLAND STATUS

- **libwayland**: Builds with a smaller Redox patch plus recipe-time source rewriting; runtime trust is still incomplete
- **cosmic-comp**: Historical partial bring-up note only; current runtime/session status is not trusted enough for support claims
- **smallvil**: Historical bounded validation compositor reference only; no longer part of the active forward desktop workflow
- **wlroots/sway/hyprland**: Not compiled or tested
- **xwayland**: Partially patched
- **Blockers**: runtime substrate trust, complete compositor session proof, libinput/seatd runtime validation, and DRM/KMS hardware/runtime validation

## KDE STATUS

- Older WIP KDE app notes here are stale relative to `local/recipes/kde/` and `config/redbear-kde.toml`
- See `docs/05-KDE-PLASMA-ON-REDOX.md` top-level status note plus `local/docs/QT6-PORT-STATUS.md` for current state

## CONVENTIONS

- ALL WIP recipes MUST start with `#TODO` explaining what's missing
- BLAKE3 hashes optional for WIP
- Test with `make r.<package>` before adding to config
- When ready: move from `wip/` to appropriate category, add BLAKE3 hash
- If Red Bear depends on a WIP subsystem long-term, prefer moving the maintained shipping version
  under `local/recipes/` and documenting the delta in `local/docs/`
