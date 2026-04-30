use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Read /scheme/sys/env for cmdline parameters
    let env_data = match fs::read_to_string("/scheme/sys/env") {
        Ok(d) => d,
        Err(_) => {
            // Fallback: read process environment
            let vars: Vec<String> = env::vars()
                .filter(|(k, _)| k.starts_with("CMDLINE_"))
                .map(|(k, v)| format!("{}={}", k.trim_start_matches("CMDLINE_"), v))
                .collect();
            if vars.is_empty() {
                eprintln!("cmdline: no parameters found");
                return;
            }
            vars.join("\n")
        }
    };

    if args.len() >= 3 && args[1] == "--get" {
        let key = &args[2];
        for line in env_data.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if k == key {
                    println!("{}", v);
                    return;
                }
            }
        }
        eprintln!("cmdline: {} not found", key);
        std::process::exit(1);
    }

    println!("{}", env_data);
}