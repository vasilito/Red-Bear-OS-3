use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::process;

use termion::event::Key;

use redbear_netctl_console::app::{App, Focus};
use redbear_netctl_console::backend::FsBackend;

#[link(name = "ncursesw")]
unsafe extern "C" {
    static mut stdscr: *mut c_void;

    fn initscr() -> *mut c_void;
    fn raw() -> c_int;
    fn noecho() -> c_int;
    fn keypad(win: *mut c_void, bf: c_int) -> c_int;
    fn curs_set(visibility: c_int) -> c_int;
    fn endwin() -> c_int;
    fn erase() -> c_int;
    fn getch() -> c_int;
    fn clrtoeol() -> c_int;
    fn mvaddnstr(y: c_int, x: c_int, s: *const c_char, n: c_int) -> c_int;
    fn refresh() -> c_int;
}

fn main() {
    if let Err(err) = run() {
        // SAFETY: best-effort terminal restore on failure path.
        unsafe {
            let _ = endwin();
        }
        eprintln!("redbear-netctl-console: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    if std::env::args()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        print_help();
        return Ok(());
    }

    let backend = FsBackend::from_env();
    let mut app = App::new(backend)?;

    // SAFETY: ncurses global terminal initialization/teardown is process-global and called once.
    unsafe {
        let _ = initscr();
        let _ = raw();
        let _ = noecho();
        let _ = keypad(stdscr, 1);
        let _ = curs_set(0);
    }

    loop {
        render(&app);
        if app.should_quit {
            break;
        }

        // SAFETY: ncurses is initialized above; getch reads one key event.
        let ch = unsafe { getch() };
        if let Some(key) = map_key(ch) {
            app.handle_key(key);
        }
    }

    // SAFETY: matching teardown for the ncurses session started above.
    unsafe {
        let _ = endwin();
    }
    Ok(())
}

fn map_key(ch: i32) -> Option<Key> {
    match ch {
        9 => Some(Key::Char('\t')),
        10 | 13 => Some(Key::Char('\n')),
        27 => Some(Key::Esc),
        127 | 8 => Some(Key::Backspace),
        c if (32..=126).contains(&c) => Some(Key::Char(c as u8 as char)),
        _ => None,
    }
}

fn render(app: &App<FsBackend>) {
    // SAFETY: ncurses is initialized for the lifetime of the main loop.
    unsafe {
        let _ = erase();
    }

    let width = 120usize;

    draw_line(
        0,
        &format!(
            "Red Bear Netctl Console [ncurses]  focus={}  active={}{}",
            focus_label(app.focus),
            app.active_profile.as_deref().unwrap_or("<none>"),
            if app.dirty { "  dirty=*" } else { "" }
        ),
    );
    draw_line(1, &truncate(&app.message, width));

    let mut line = 3;
    draw_line(line, "Profiles");
    line += 1;
    if app.profiles.is_empty() {
        draw_line(line, "  <no saved profiles>");
        line += 1;
    } else {
        for (idx, profile) in app.profiles.iter().enumerate().take(6) {
            let marker = if idx == app.selected_profile && app.focus == Focus::Profiles {
                '>'
            } else {
                ' '
            };
            let active = if app.active_profile.as_deref() == Some(profile.as_str()) {
                "*"
            } else {
                " "
            };
            draw_line(
                line,
                &truncate(&format!("{}{} {}", marker, active, profile), width),
            );
            line += 1;
        }
    }

    line += 1;
    draw_line(line, "Scan Results");
    line += 1;
    if app.scans.is_empty() {
        draw_line(line, "  <none; press r to scan>");
        line += 1;
    } else {
        for (idx, scan) in app.scans.iter().enumerate().take(6) {
            let marker = if idx == app.selected_scan && app.focus == Focus::Scan {
                '>'
            } else {
                ' '
            };
            draw_line(
                line,
                &truncate(&format!("{} {}", marker, scan.label()), width),
            );
            line += 1;
        }
    }

    line += 1;
    draw_line(line, "Profile Draft");
    line += 1;
    for field in app.visible_fields() {
        let marker = if app.focus == Focus::Fields && app.selected_field() == field {
            '>'
        } else {
            ' '
        };
        draw_line(
            line,
            &truncate(
                &format!(
                    "{} {:<12} {}",
                    marker,
                    field.label(),
                    app.field_value(field)
                ),
                width,
            ),
        );
        line += 1;
    }

    if let Some(editor) = &app.editor {
        line += 1;
        draw_line(
            line,
            &truncate(
                &format!("Editing {}: {}", editor.field.label(), editor.buffer),
                width,
            ),
        );
        line += 1;
        draw_line(line, "Enter saves. Esc cancels. Backspace deletes.");
        line += 1;
    }

    line += 1;
    draw_line(line, "Runtime Status");
    line += 1;
    for status_line in [
        format!("iface={} addr={}", app.status.interface, app.status.address),
        format!(
            "status={} link={}",
            app.status.status, app.status.link_state
        ),
        format!(
            "fw={} transport={} init={} activation={}",
            app.status.firmware_status,
            app.status.transport_status,
            app.status.transport_init_status,
            app.status.activation_status
        ),
        format!("connect={}", app.status.connect_result),
        format!("disconnect={}", app.status.disconnect_result),
        format!("last_error={}", app.status.last_error),
    ] {
        draw_line(line, &truncate(&status_line, width));
        line += 1;
    }

    // SAFETY: drawing to stdscr is valid while ncurses session is active.
    unsafe {
        draw_raw_line(
            28,
            "Keys: Tab switch panes | Enter select/edit | h/l cycle | j/k move | r scan | s save | a activate | c connect | d disconnect | n new | q quit",
        );
        let _ = refresh();
    }
}

fn draw_line(row: i32, text: &str) {
    // SAFETY: drawing to stdscr is valid while ncurses session is active.
    unsafe {
        draw_raw_line(row, text);
    }
}

fn truncate(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn focus_label(focus: Focus) -> &'static str {
    match focus {
        Focus::Profiles => "profiles",
        Focus::Scan => "scan",
        Focus::Fields => "fields",
    }
}

unsafe fn move_add_line(row: i32, text: &str) {
    let sanitized = text.replace('\0', "?");
    if let Ok(cstr) = CString::new(sanitized) {
        let _ = unsafe { mvaddnstr(row, 0, cstr.as_ptr(), i32::MAX) };
    }
}

unsafe fn draw_raw_line(row: i32, text: &str) {
    let blank = " ".repeat(140);
    unsafe { move_add_line(row, &blank) };
    unsafe { move_add_line(row, text) };
    let _ = unsafe { clrtoeol() };
}

fn print_help() {
    println!(
        "Usage: redbear-netctl-console\n\nA ncurses-based console client for the bounded Red Bear Wi-Fi/netctl flow.\n\nKeys:\n  Tab              switch panes\n  Enter            load profile, apply scan result, or edit selected field\n  h / l            cycle Security or IP mode on the selected field\n  j / k            move selection down / up\n  r                scan with /scheme/wifictl\n  s                save draft to /etc/netctl/<profile>\n  a                save draft and write /etc/netctl/active\n  c                save + connect through the bounded wifictl/netctl flow\n  d                disconnect current interface and clear /etc/netctl/active when it matches\n  n                start a new Wi-Fi profile draft\n  q                quit\n\nEnvironment overrides match redbear-netctl tests:\n  REDBEAR_NETCTL_PROFILE_DIR\n  REDBEAR_NETCTL_ACTIVE\n  REDBEAR_WIFICTL_ROOT\n  REDBEAR_NETCFG_ROOT\n  REDBEAR_DHCPD_CMD\n  REDBEAR_DHCPD_WAIT_MS\n  REDBEAR_DHCPD_POLL_MS"
    );
}
