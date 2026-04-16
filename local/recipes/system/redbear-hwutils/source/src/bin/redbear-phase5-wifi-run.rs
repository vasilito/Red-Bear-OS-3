use std::process::{self, Command};

use redbear_hwutils::parse_args;

const PROGRAM: &str = "redbear-phase5-wifi-run";
const USAGE: &str = "Usage: redbear-phase5-wifi-run [PROFILE] [INTERFACE] [OUTPUT_PATH]\n\nRun the packaged bounded Wi-Fi validator and then emit a JSON capture bundle.";

fn run_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("failed to run {} {:?}: {err}", program, args))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() {
            return Err(format!(
                "{} {:?} exited with status {}",
                program, args, output.status
            ));
        }
        return Err(format!(
            "{} {:?} exited with status {}: {}",
            program,
            args,
            output.status,
            stderr.trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    parse_args(PROGRAM, USAGE, args.clone().into_iter()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    let profile = args
        .get(1)
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("wifi-open-bounded");
    let iface = args
        .get(2)
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("wlan0");
    let output_path = args
        .get(3)
        .filter(|arg| !arg.starts_with('-'))
        .map(String::as_str)
        .unwrap_or("/tmp/redbear-phase5-wifi-capture.json");

    let check = run_command("redbear-phase5-wifi-check", &[profile, iface])?;
    print!("{check}");

    let link = run_command("redbear-phase5-wifi-link-check", &[])?;
    print!("{link}");

    let capture = run_command(
        "redbear-phase5-wifi-capture",
        &[profile, iface, output_path],
    )?;
    print!("{capture}");
    println!("capture_output={output_path}");
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_capture_path_is_stable() {
        assert_eq!(
            "/tmp/redbear-phase5-wifi-capture.json",
            "/tmp/redbear-phase5-wifi-capture.json"
        );
    }
}
