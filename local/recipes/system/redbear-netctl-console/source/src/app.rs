use termion::event::Key;

use crate::backend::{ConsoleBackend, IpMode, Profile, ScanResult, SecurityKind, WifiRuntimeState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Profiles,
    Scan,
    Fields,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Description,
    Interface,
    Ssid,
    Security,
    Key,
    IpMode,
    Address,
    Gateway,
    Dns,
}

impl Field {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Name => "Profile",
            Self::Description => "Description",
            Self::Interface => "Interface",
            Self::Ssid => "SSID",
            Self::Security => "Security",
            Self::Key => "Key",
            Self::IpMode => "IP",
            Self::Address => "Address",
            Self::Gateway => "Gateway",
            Self::Dns => "DNS",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorState {
    pub field: Field,
    pub buffer: String,
}

pub struct App<B: ConsoleBackend> {
    backend: B,
    pub profiles: Vec<String>,
    pub selected_profile: usize,
    pub draft: Profile,
    pub scans: Vec<ScanResult>,
    pub selected_scan: usize,
    pub selected_field: usize,
    pub focus: Focus,
    pub editor: Option<EditorState>,
    pub active_profile: Option<String>,
    pub status: WifiRuntimeState,
    pub message: String,
    pub dirty: bool,
    pub should_quit: bool,
}

impl<B: ConsoleBackend> App<B> {
    pub fn new(backend: B) -> Result<Self, String> {
        let mut app = Self {
            backend,
            profiles: Vec::new(),
            selected_profile: 0,
            draft: Profile::default(),
            scans: Vec::new(),
            selected_scan: 0,
            selected_field: 0,
            focus: Focus::Profiles,
            editor: None,
            active_profile: None,
            status: WifiRuntimeState::default(),
            message: "Tab switches panes. r scans. s saves. c connects. d disconnects.".to_string(),
            dirty: false,
            should_quit: false,
        };
        app.reload_profiles()?;
        app.refresh_status();
        Ok(app)
    }

    pub fn visible_fields(&self) -> Vec<Field> {
        let mut fields = vec![
            Field::Name,
            Field::Description,
            Field::Interface,
            Field::Ssid,
            Field::Security,
        ];

        if matches!(self.draft.security, SecurityKind::Wpa2Psk) {
            fields.push(Field::Key);
        }

        fields.push(Field::IpMode);

        if matches!(self.draft.ip_mode, IpMode::Static) {
            fields.extend([Field::Address, Field::Gateway, Field::Dns]);
        }

        fields
    }

    pub fn selected_field(&self) -> Field {
        let fields = self.visible_fields();
        fields[self.selected_field.min(fields.len().saturating_sub(1))]
    }

    pub fn field_value(&self, field: Field) -> String {
        match field {
            Field::Name => self.draft.name.clone(),
            Field::Description => self.draft.description.clone(),
            Field::Interface => self.draft.interface.clone(),
            Field::Ssid => self.draft.ssid.clone(),
            Field::Security => self.draft.security.as_str().to_string(),
            Field::Key => mask_secret(&self.draft.key),
            Field::IpMode => self.draft.ip_mode.as_str().to_string(),
            Field::Address => self.draft.address.clone(),
            Field::Gateway => self.draft.gateway.clone(),
            Field::Dns => self.draft.dns.clone(),
        }
    }

    pub fn handle_key(&mut self, key: Key) {
        if self.editor.is_some() {
            self.handle_editor_key(key);
            return;
        }

        match key {
            Key::Char('q') => self.should_quit = true,
            Key::Char('\t') => self.cycle_focus(true),
            Key::BackTab => self.cycle_focus(false),
            Key::Char('r') => self.scan(),
            Key::Char('s') => self.save_current_profile(),
            Key::Char('a') => self.activate_current_profile(),
            Key::Char('c') => self.connect_current_profile(),
            Key::Char('d') => self.disconnect_current_profile(),
            Key::Char('n') => self.start_new_profile(),
            Key::Up | Key::Char('k') => self.move_selection(-1),
            Key::Down | Key::Char('j') => self.move_selection(1),
            Key::Left | Key::Char('h') => self.adjust_field_enum(false),
            Key::Right | Key::Char('l') => self.adjust_field_enum(true),
            Key::Char('\n') => self.activate_current_focus(),
            _ => {}
        }
    }

    pub fn reload_profiles(&mut self) -> Result<(), String> {
        self.active_profile = self.backend.active_profile_name()?;
        self.profiles = self.backend.list_wifi_profiles()?;

        if let Some(active) = &self.active_profile
            && let Some(index) = self.profiles.iter().position(|name| name == active)
        {
            self.selected_profile = index;
            self.load_selected_profile()?;
            return Ok(());
        }

        if let Some(index) = self
            .profiles
            .iter()
            .position(|name| name == &self.draft.name)
        {
            self.selected_profile = index;
            return Ok(());
        }

        if !self.profiles.is_empty() {
            self.selected_profile = 0;
            self.load_selected_profile()?;
        }

        Ok(())
    }

    pub fn refresh_status(&mut self) {
        self.status = self.backend.read_status(&self.draft.interface);
    }

    fn cycle_focus(&mut self, forward: bool) {
        self.focus = match (self.focus, forward) {
            (Focus::Profiles, true) => Focus::Scan,
            (Focus::Scan, true) => Focus::Fields,
            (Focus::Fields, true) => Focus::Profiles,
            (Focus::Profiles, false) => Focus::Fields,
            (Focus::Scan, false) => Focus::Profiles,
            (Focus::Fields, false) => Focus::Scan,
        };
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Profiles => adjust_index(&mut self.selected_profile, self.profiles.len(), delta),
            Focus::Scan => adjust_index(&mut self.selected_scan, self.scans.len(), delta),
            Focus::Fields => {
                let field_count = self.visible_fields().len();
                adjust_index(&mut self.selected_field, field_count, delta)
            }
        }
    }

    fn activate_current_focus(&mut self) {
        match self.focus {
            Focus::Profiles => {
                if let Err(err) = self.load_selected_profile() {
                    self.message = format!("Error: {err}");
                }
            }
            Focus::Scan => self.apply_selected_scan(),
            Focus::Fields => self.activate_selected_field(),
        }
    }

    fn activate_selected_field(&mut self) {
        match self.selected_field() {
            Field::Security => {
                self.draft.security = self.draft.security.next();
                self.selected_field = self
                    .selected_field
                    .min(self.visible_fields().len().saturating_sub(1));
                self.dirty = true;
            }
            Field::IpMode => {
                self.draft.ip_mode = self.draft.ip_mode.next();
                self.selected_field = self
                    .selected_field
                    .min(self.visible_fields().len().saturating_sub(1));
                self.dirty = true;
            }
            field => {
                self.editor = Some(EditorState {
                    field,
                    buffer: self.raw_field_value(field),
                });
            }
        }
    }

    fn adjust_field_enum(&mut self, forward: bool) {
        if self.focus != Focus::Fields {
            return;
        }

        match self.selected_field() {
            Field::Security => {
                self.draft.security = if forward {
                    self.draft.security.next()
                } else {
                    self.draft.security.previous()
                };
                self.selected_field = self
                    .selected_field
                    .min(self.visible_fields().len().saturating_sub(1));
                self.dirty = true;
            }
            Field::IpMode => {
                self.draft.ip_mode = if forward {
                    self.draft.ip_mode.next()
                } else {
                    self.draft.ip_mode.previous()
                };
                self.selected_field = self
                    .selected_field
                    .min(self.visible_fields().len().saturating_sub(1));
                self.dirty = true;
            }
            _ => {}
        }
    }

    fn handle_editor_key(&mut self, key: Key) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };

        match key {
            Key::Esc => self.editor = None,
            Key::Backspace => {
                editor.buffer.pop();
            }
            Key::Char('\n') => {
                let field = editor.field;
                let buffer = editor.buffer.clone();
                self.editor = None;
                self.commit_field(field, buffer);
            }
            Key::Char(ch) if !ch.is_control() => editor.buffer.push(ch),
            _ => {}
        }
    }

    fn commit_field(&mut self, field: Field, value: String) {
        match field {
            Field::Name => self.draft.name = value,
            Field::Description => self.draft.description = value,
            Field::Interface => {
                self.draft.interface = value;
                self.refresh_status();
            }
            Field::Ssid => self.draft.ssid = value,
            Field::Key => self.draft.key = value,
            Field::Address => self.draft.address = value,
            Field::Gateway => self.draft.gateway = value,
            Field::Dns => self.draft.dns = value,
            Field::Security | Field::IpMode => {}
        }

        self.dirty = true;
    }

    fn load_selected_profile(&mut self) -> Result<(), String> {
        let Some(name) = self.profiles.get(self.selected_profile).cloned() else {
            return Ok(());
        };
        self.draft = self.backend.load_profile(&name)?;
        self.editor = None;
        self.selected_field = 0;
        self.selected_scan = 0;
        self.scans.clear();
        self.refresh_status();
        self.message = format!("Loaded {name}");
        self.dirty = false;
        Ok(())
    }

    fn apply_selected_scan(&mut self) {
        let Some(scan) = self.scans.get(self.selected_scan).cloned() else {
            self.message = "No scan result selected".to_string();
            return;
        };
        self.draft.ssid = scan.ssid.clone();
        if let Some(security_hint) = scan.security_hint {
            self.draft.security = security_hint;
        }
        self.dirty = true;
        self.message = format!("Selected SSID {}", scan.ssid);
    }

    fn save_current_profile(&mut self) {
        match self.backend.save_profile(&self.draft) {
            Ok(()) => {
                self.dirty = false;
                self.message = format!("Saved {}", self.draft.name);
                if let Err(err) = self.reload_profiles() {
                    self.message = format!("Saved {}, but reload failed: {err}", self.draft.name);
                }
            }
            Err(err) => self.message = format!("Error: {err}"),
        }
    }

    fn activate_current_profile(&mut self) {
        match self.backend.save_profile(&self.draft) {
            Ok(()) => match self.backend.set_active_profile(&self.draft.name) {
                Ok(()) => {
                    self.active_profile = Some(self.draft.name.clone());
                    self.dirty = false;
                    self.message = format!("Activated {} for boot/profile reuse", self.draft.name);
                    let _ = self.reload_profiles();
                }
                Err(err) => self.message = format!("Error: {err}"),
            },
            Err(err) => self.message = format!("Error: {err}"),
        }
    }

    fn connect_current_profile(&mut self) {
        match self.backend.connect(&self.draft) {
            Ok(message) => {
                self.active_profile = Some(self.draft.name.clone());
                self.dirty = false;
                self.message = message;
                let _ = self.reload_profiles();
                self.refresh_status();
            }
            Err(err) => {
                self.refresh_status();
                self.message = format!("Error: {err}");
            }
        }
    }

    fn disconnect_current_profile(&mut self) {
        match self
            .backend
            .disconnect(Some(&self.draft.name), &self.draft.interface)
        {
            Ok(message) => {
                if self.active_profile.as_deref() == Some(self.draft.name.as_str()) {
                    self.active_profile = None;
                }
                self.message = message;
                let _ = self.reload_profiles();
                self.refresh_status();
            }
            Err(err) => {
                self.refresh_status();
                self.message = format!("Error: {err}");
            }
        }
    }

    fn scan(&mut self) {
        match self.backend.scan(&self.draft.interface) {
            Ok(scans) => {
                self.scans = scans;
                self.selected_scan = 0;
                self.refresh_status();
                self.message = if self.scans.is_empty() {
                    format!(
                        "Scan completed for {} with no visible results",
                        self.draft.interface
                    )
                } else {
                    format!(
                        "Scan completed for {} with {} result(s)",
                        self.draft.interface,
                        self.scans.len()
                    )
                };
            }
            Err(err) => {
                self.refresh_status();
                self.message = format!("Error: {err}");
            }
        }
    }

    fn start_new_profile(&mut self) {
        self.draft = Profile::default();
        self.scans.clear();
        self.selected_scan = 0;
        self.selected_field = 0;
        self.focus = Focus::Fields;
        self.editor = None;
        self.dirty = true;
        self.refresh_status();
        self.message = "Started a new Wi-Fi profile draft".to_string();
    }

    fn raw_field_value(&self, field: Field) -> String {
        match field {
            Field::Name => self.draft.name.clone(),
            Field::Description => self.draft.description.clone(),
            Field::Interface => self.draft.interface.clone(),
            Field::Ssid => self.draft.ssid.clone(),
            Field::Key => self.draft.key.clone(),
            Field::Address => self.draft.address.clone(),
            Field::Gateway => self.draft.gateway.clone(),
            Field::Dns => self.draft.dns.clone(),
            Field::Security => self.draft.security.as_str().to_string(),
            Field::IpMode => self.draft.ip_mode.as_str().to_string(),
        }
    }
}

fn adjust_index(index: &mut usize, len: usize, delta: isize) {
    if len == 0 {
        *index = 0;
        return;
    }

    if delta < 0 {
        *index = index.saturating_sub(delta.unsigned_abs());
    } else {
        *index = (*index + delta as usize).min(len.saturating_sub(1));
    }
}

fn mask_secret(secret: &str) -> String {
    if secret.is_empty() {
        "<empty>".to_string()
    } else {
        "•".repeat(secret.chars().count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::ScanResult;

    #[derive(Default)]
    struct MockBackend {
        profiles: Vec<String>,
        active_profile: Option<String>,
        loaded_profile: Profile,
        scans: Vec<ScanResult>,
    }

    impl ConsoleBackend for MockBackend {
        fn list_wifi_profiles(&self) -> Result<Vec<String>, String> {
            Ok(self.profiles.clone())
        }

        fn active_profile_name(&self) -> Result<Option<String>, String> {
            Ok(self.active_profile.clone())
        }

        fn load_profile(&self, _name: &str) -> Result<Profile, String> {
            Ok(self.loaded_profile.clone())
        }

        fn save_profile(&self, _profile: &Profile) -> Result<(), String> {
            Ok(())
        }

        fn set_active_profile(&self, _profile_name: &str) -> Result<(), String> {
            Ok(())
        }

        fn clear_active_profile(&self) -> Result<(), String> {
            Ok(())
        }

        fn read_status(&self, interface: &str) -> WifiRuntimeState {
            WifiRuntimeState {
                interface: interface.to_string(),
                status: "unknown".to_string(),
                address: "unconfigured".to_string(),
                ..WifiRuntimeState::default()
            }
        }

        fn scan(&self, _interface: &str) -> Result<Vec<ScanResult>, String> {
            Ok(self.scans.clone())
        }

        fn connect(&self, _profile: &Profile) -> Result<String, String> {
            Ok("applied wifi-profile via wlan0".to_string())
        }

        fn disconnect(
            &self,
            _profile_name: Option<&str>,
            _interface: &str,
        ) -> Result<String, String> {
            Ok("disconnected wlan0".to_string())
        }
    }

    #[test]
    fn visible_fields_follow_security_and_ip_mode() {
        let backend = MockBackend::default();
        let mut app = App::new(backend).unwrap();

        assert!(!app.visible_fields().contains(&Field::Key));
        assert!(!app.visible_fields().contains(&Field::Address));

        app.draft.security = SecurityKind::Wpa2Psk;
        app.draft.ip_mode = IpMode::Static;

        assert!(app.visible_fields().contains(&Field::Key));
        assert!(app.visible_fields().contains(&Field::Address));
        assert!(app.visible_fields().contains(&Field::Gateway));
        assert!(app.visible_fields().contains(&Field::Dns));
    }

    #[test]
    fn selecting_scan_applies_ssid_and_security_hint() {
        let backend = MockBackend {
            scans: vec![ScanResult {
                raw: "ssid=demo-open security=open".to_string(),
                ssid: "demo-open".to_string(),
                security_hint: Some(SecurityKind::Open),
            }],
            ..MockBackend::default()
        };
        let mut app = App::new(backend).unwrap();
        app.handle_key(Key::Char('r'));
        app.focus = Focus::Scan;
        app.handle_key(Key::Char('\n'));

        assert_eq!(app.draft.ssid, "demo-open");
        assert_eq!(app.draft.security, SecurityKind::Open);
    }
}
