mod keymap;
mod scheme;
mod xkb;

use std::env;
use std::io::Write;
use std::process;

use scheme::KeymapScheme;

fn log_msg(level: &str, msg: &str) {
    let _ = writeln!(std::io::stderr(), "[keymapd] {} {}", level, msg);
}

fn main() {
    let mut scheme = KeymapScheme::new();

    let builtins = keymap::BuiltinKeymaps::new();
    scheme.load_builtin(&builtins);

    let keymap_dir = match env::var("KEYMAP_DIR") {
        Ok(dir) => dir,
        Err(_) => "/etc/keymaps".to_string(),
    };
    if let Err(e) = scheme.load_from_dir(&keymap_dir) {
        log_msg("ERROR", &format!("failed to load keymaps from {}: {}", keymap_dir, e));
    }

    log_msg("INFO", &format!("loaded {} keymap(s)", scheme.keymap_count()));

    let socket = redox_scheme::Socket::nonblock("keymap")
        .expect("keymapd: failed to register scheme:keymap");
    log_msg("INFO", "registered scheme:keymap");

    loop {
        let request = match socket.next_request(redox_scheme::SignalBehavior::Restart) {
            Ok(Some(r)) => r,
            Ok(None) => {
                log_msg("INFO", "scheme unmounted, exiting");
                break;
            }
            Err(e) => {
                log_msg("ERROR", &format!("failed to read request: {}", e));
                process::exit(1);
            }
        };

        match request.handle_scheme_block_mut(&mut scheme) {
            Ok(response) => {
                if let Err(e) = socket.write_response(response, redox_scheme::SignalBehavior::Restart) {
                    log_msg("ERROR", &format!("failed to write response: {}", e));
                }
            }
            Err(_request) => {
                log_msg("ERROR", "unhandled scheme request");
            }
        }
    }
}
