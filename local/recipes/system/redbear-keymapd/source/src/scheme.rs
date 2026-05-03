use std::collections::HashMap;
use std::io;

use syscall::data::Stat;
use syscall::error::{Error, Result, EBADF, EINVAL, ENOENT};
use syscall::flag::{MODE_DIR, MODE_FILE, SEEK_CUR, SEEK_END, SEEK_SET};

use crate::keymap::Keymap;

enum HandleKind {
    Root,
    Active,
    List,
    Keymap { name: String },
}

struct Handle {
    kind: HandleKind,
    offset: usize,
}

pub struct KeymapScheme {
    next_id: usize,
    handles: HashMap<usize, Handle>,
    keymaps: HashMap<String, Keymap>,
    active_keymap: String,
}

impl KeymapScheme {
    pub fn new() -> Self {
        KeymapScheme {
            next_id: 0,
            handles: HashMap::new(),
            keymaps: HashMap::new(),
            active_keymap: "us".to_string(),
        }
    }

    pub fn load_builtin(&mut self, builtins: &crate::keymap::BuiltinKeymaps) {
        for (name, km) in [
            ("us", &builtins.us),
            ("gb", &builtins.gb),
            ("dvorak", &builtins.dvorak),
            ("azerty", &builtins.azerty),
            ("bepo", &builtins.bepo),
            ("it", &builtins.it),
        ] {
            self.keymaps.insert(name.to_string(), km.clone());
        }
    }

    pub fn load_from_dir(&mut self, dir: &str) -> io::Result<()> {
        let entries = std::fs::read_dir(dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "json") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let json_str = std::fs::read_to_string(&path)?;
                if let Ok(km) = Keymap::from_json(&name, &json_str) {
                    self.keymaps.insert(name, km);
                }
            }
        }
        Ok(())
    }

    pub fn load_xkb(&mut self, xkb_dir: &str, layout: &str, variant: Option<&str>) -> io::Result<()> {
        let km = crate::xkb::load_xkb_keymap(xkb_dir, layout, variant)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let name = match variant {
            Some(v) => format!("{}({})", layout, v),
            None => layout.to_string(),
        };
        self.keymaps.insert(name, km);
        Ok(())
    }

    pub fn keymap_count(&self) -> usize {
        self.keymaps.len()
    }

    fn active_keymap(&self) -> &Keymap {
        self.keymaps
            .get(&self.active_keymap)
            .or_else(|| self.keymaps.get("us"))
            .expect("at least one keymap must be loaded")
    }

    pub fn translate(&self, scancode: u8, shift: bool, altgr: bool) -> char {
        self.active_keymap().get_char(scancode, shift, altgr)
    }
}

impl redox_scheme::SchemeBlockMut for KeymapScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let cleaned = path.trim_matches('/');

        let kind = if cleaned.is_empty() {
            HandleKind::Root
        } else if cleaned == "active" {
            HandleKind::Active
        } else if cleaned == "list" {
            HandleKind::List
        } else if let Some(name) = cleaned.strip_prefix("keymap/") {
            let name = name.trim_end_matches('/').to_string();
            if !self.keymaps.contains_key(&name) {
                return Err(Error::new(ENOENT));
            }
            HandleKind::Keymap { name }
        } else if self.keymaps.contains_key(cleaned) {
            let name = cleaned.to_string();
            if let Some(km) = self.keymaps.get(&name) {
                let _ = km;
            }
            HandleKind::Keymap { name }
        } else if cleaned.starts_with("set/") {
            let requested = &cleaned[4..];
            if !self.keymaps.contains_key(requested) {
                return Err(Error::new(ENOENT));
            }
            self.active_keymap = requested.to_string();
            HandleKind::Active
        } else {
            return Err(Error::new(ENOENT));
        };

        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, Handle { kind, offset: 0 });
        Ok(Some(id))
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;

        let content: Vec<u8> = match &handle.kind {
            HandleKind::Root => {
                let mut listing = String::new();
                listing.push_str("active\nlist\n");
                for name in self.keymaps.keys() {
                    listing.push_str(&format!("keymap/{}\n", name));
                }
                listing.into_bytes()
            }
            HandleKind::Active => self.active_keymap.clone().into_bytes(),
            HandleKind::List => {
                let mut listing = String::new();
                for (i, name) in self.keymaps.keys().enumerate() {
                    if i > 0 {
                        listing.push('\n');
                    }
                    listing.push_str(name);
                }
                listing.push('\n');
                listing.into_bytes()
            }
            HandleKind::Keymap { name } => {
                let km = self.keymaps.get(name).ok_or(Error::new(ENOENT))?;
                format!(
                    "name={}\nentries={}\ncompose={}\ndead_keys={}\n",
                    km.name,
                    km.entries.len(),
                    km.compose.len(),
                    km.dead_keys.len()
                )
                .into_bytes()
            }
        };

        if handle.offset >= content.len() {
            return Ok(Some(0));
        }
        let remaining = &content[handle.offset..];
        let to_copy = remaining.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
        handle.offset += to_copy;
        Ok(Some(to_copy))
    }

    fn write(&mut self, id: usize, buf: &[u8]) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        match &handle.kind {
            HandleKind::Active => {
                let name = String::from_utf8_lossy(buf);
                let name = name.trim();
                if self.keymaps.contains_key(name) {
                    self.active_keymap = name.to_string();
                    Ok(Some(buf.len()))
                } else {
                    Err(Error::new(ENOENT))
                }
            }
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn seek(&mut self, id: usize, pos: isize, whence: usize) -> Result<Option<isize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let new_offset = match whence {
            SEEK_SET => pos as isize,
            SEEK_CUR => handle.offset as isize + pos,
            SEEK_END => pos,
            _ => return Err(Error::new(EINVAL)),
        };
        if new_offset < 0 {
            return Err(Error::new(EINVAL));
        }
        handle.offset = new_offset as usize;
        Ok(Some(new_offset))
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        match &handle.kind {
            HandleKind::Root => {
                stat.st_mode = MODE_DIR | 0o555;
            }
            _ => {
                stat.st_mode = MODE_FILE | 0o644;
            }
        }
        Ok(Some(0))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        self.handles.remove(&id);
        Ok(Some(0))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        let path = match &handle.kind {
            HandleKind::Root => "keymap:".to_string(),
            HandleKind::Active => "keymap:active".to_string(),
            HandleKind::List => "keymap:list".to_string(),
            HandleKind::Keymap { name } => format!("keymap:keymap/{}", name),
        };
        let bytes = path.as_bytes();
        let to_copy = bytes.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&bytes[..to_copy]);
        Ok(Some(to_copy))
    }
}
