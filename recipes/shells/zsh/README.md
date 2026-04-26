# Zsh on Redox

Production recipe for Zsh 5.9 on Red Bear OS / Redox.

## Status

- **Builds:** yes
- **Runtime:** basic shell works; `times` builtin is a no-op stub
- **Blockers:** `times()` and `getrusage()` not yet in relibc

## Patch Summary

| File | Change | Reason |
|------|--------|--------|
| `configure.ac` | Remove `getrusage` from `AC_CHECK_FUNCS` | Avoids configure-time detection of missing function |
| `Src/builtin.c` | Stub `bin_times()` | `times()` unavailable in relibc |
| `Src/Builtins/rlimits.c` | Disable `set_resinfo()` / `free_resinfo()` | These depend on `getrusage()` |

## Configure Flags

- `--disable-gdbm` — avoid GNU dbm dependency
- `--disable-pcre` — avoid PCRE dependency
- `--disable-cap` — avoid POSIX capabilities dependency
- `zsh_cv_sys_elf=no` — skip ELF detection (not applicable on Redox)

## Dependencies

- `ncursesw` — wide-character terminal library

## Install Targets

Zsh uses non-standard install targets:

```
make install.bin install.modules install.fns DESTDIR="${COOKBOOK_STAGE}"
```

## Configuration Files

The recipe installs Manjaro-inspired system-wide zsh configuration:

| File | Purpose |
|------|---------|
| `/etc/zsh/zshenv` | Environment variables for all zsh shells |
| `/etc/zsh/zprofile` | Login shell profile (sources `/etc/profile` for compatibility) |
| `/etc/zsh/zshrc` | Interactive shell config: history, completion, colors, prompt, aliases |
| `/etc/skel/.zshrc` | Template for new users |
| `/etc/skel/.zprofile` | Template for new users (login shell) |

### Features (Manjaro-style)

- **Colored prompt**: green for user, red for root, with hostname and working directory
- **Right-side prompt**: shows exit code on error
- **Tab completion**: with menu selection, approximate matching, and colorized listings
- **History**: shared across sessions, ignores duplicates and leading-space entries
- **Aliases**: `ls`, `ll`, `la`, `grep`, `cp`, `mv`, `rm` with color and safety flags
- **Convenience**: `AUTO_CD`, `CORRECT`, `NO_BEEP`
- **Optional plugins**: `zsh-syntax-highlighting`, `zsh-autosuggestions` (loaded if available)

## Future Work

- Re-enable `times` builtin when relibc gains `times()` support
- Re-enable resource-limit info when relibc gains `getrusage()` support
- Evaluate enabling `gdbm`, `pcre`, or `cap` if those libraries are ported
- Package `zsh-syntax-highlighting` and `zsh-autosuggestions` plugins
