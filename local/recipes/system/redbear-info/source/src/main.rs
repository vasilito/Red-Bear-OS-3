use std::env;
use std::fs;
use std::path::Path;
use std::process;

const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const DIVIDER: &str = "═══════════════════════════════════════════════════════════════════";
const REDBEAR_META_README: &str = "/usr/share/doc/redbear-meta/README";

struct Component {
    name: &'static str,
    description: &'static str,
    category: &'static str,
    scheme_path: &'static str,
    binary_path: &'static str,
    test_hint: &'static str,
    dependencies: &'static [&'static str],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Table,
    Json,
    Test,
    Help,
}

struct Options {
    mode: OutputMode,
    verbose: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AvailabilityState {
    Available,
    Unavailable,
    BuiltIn,
}

struct ComponentStatus<'a> {
    component: &'a Component,
    state: AvailabilityState,
    available: bool,
    status_text: &'static str,
    scheme_exists: Option<bool>,
    binary_exists: Option<bool>,
}

const COMPONENTS: &[Component] = &[
    Component {
        name: "redbear-release",
        description: "OS identity (hostname, os-release, motd, banner)",
        category: "Branding",
        scheme_path: "",
        binary_path: "",
        test_hint: "cat /usr/lib/os-release",
        dependencies: &[],
    },
    Component {
        name: "ext4d",
        description: "ext4 scheme daemon",
        category: "Filesystem",
        scheme_path: "/scheme/ext4d",
        binary_path: "/usr/bin/ext4d",
        test_hint: "ls /scheme/ext4d/",
        dependencies: &[],
    },
    Component {
        name: "redox-driver-sys",
        description: "Safe Rust wrappers for scheme:memory, scheme:irq, scheme:pci",
        category: "Driver",
        scheme_path: "",
        binary_path: "",
        test_hint: "pkg list | grep redox-driver-sys",
        dependencies: &[],
    },
    Component {
        name: "linux-kpi",
        description:
            "Linux Kernel Programming Interface compatibility layer (C headers + Rust impl)",
        category: "Driver",
        scheme_path: "",
        binary_path: "",
        test_hint: "pkg list | grep linux-kpi",
        dependencies: &["redox-driver-sys"],
    },
    Component {
        name: "firmware-loader",
        description: "Loads GPU firmware blobs via scheme:firmware",
        category: "System",
        scheme_path: "/scheme/firmware",
        binary_path: "/usr/lib/drivers/firmware-loader",
        test_hint: "ls /scheme/firmware/amdgpu/",
        dependencies: &[],
    },
    Component {
        name: "redox-drm",
        description: "DRM display driver for AMD and Intel GPUs",
        category: "GPU",
        scheme_path: "/scheme/drm",
        binary_path: "/usr/bin/redox-drm",
        test_hint: "ls /scheme/drm/card0/",
        dependencies: &["redox-driver-sys", "linux-kpi"],
    },
    Component {
        name: "amdgpu",
        description: "AMD GPU driver (Display Core modesetting) via LinuxKPI",
        category: "GPU",
        scheme_path: "",
        binary_path: "/usr/lib/redox/drivers/libamdgpu_dc_redox.so",
        test_hint: "ls -la /usr/lib/redox/drivers/libamdgpu_dc_redox.so",
        dependencies: &["redox-driver-sys", "linux-kpi", "firmware-loader"],
    },
    Component {
        name: "evdevd",
        description: "Translates Redox input events to evdev protocol",
        category: "Input",
        scheme_path: "/scheme/evdev",
        binary_path: "/usr/lib/drivers/evdevd",
        test_hint: "ls /scheme/evdev/",
        dependencies: &[],
    },
    Component {
        name: "udev-shim",
        description: "udev-compatible device enumeration shim (PCI scanning)",
        category: "System",
        scheme_path: "/scheme/udev",
        binary_path: "/usr/lib/drivers/udev-shim",
        test_hint: "ls /scheme/udev/",
        dependencies: &[],
    },
    Component {
        name: "redbear-meta",
        description: "Umbrella meta-package depending on all Red Bear OS components",
        category: "System",
        scheme_path: "",
        binary_path: "",
        test_hint: "cat /usr/share/doc/redbear-meta/README",
        dependencies: &["redbear-release", "firmware-loader", "evdevd", "udev-shim"],
    },
];

fn main() {
    if let Err(err) = run() {
        eprintln!("redbear-info: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_args(env::args())?;

    if options.mode == OutputMode::Help {
        print_help();
        return Ok(());
    }

    let branding_available = has_red_bear_branding();
    let statuses = collect_statuses(branding_available);

    match options.mode {
        OutputMode::Table => print_table(&statuses, options.verbose),
        OutputMode::Json => print_json(&statuses),
        OutputMode::Test => print_tests(&statuses, options.verbose),
        OutputMode::Help => {}
    }

    Ok(())
}

fn parse_args<I>(args: I) -> Result<Options, String>
where
    I: IntoIterator<Item = String>,
{
    let mut mode = OutputMode::Table;
    let mut verbose = false;

    for arg in args.into_iter().skip(1) {
        match arg.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--json" => {
                if mode == OutputMode::Test {
                    return Err("cannot combine --json with --test".to_string());
                }
                mode = OutputMode::Json;
            }
            "--test" => {
                if mode == OutputMode::Json {
                    return Err("cannot combine --test with --json".to_string());
                }
                mode = OutputMode::Test;
            }
            "-h" | "--help" => mode = OutputMode::Help,
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok(Options { mode, verbose })
}

fn has_red_bear_branding() -> bool {
    match fs::read_to_string("/usr/lib/os-release") {
        Ok(contents) => contents.contains("Red Bear OS"),
        Err(_) => false,
    }
}

fn collect_statuses(branding_available: bool) -> Vec<ComponentStatus<'static>> {
    COMPONENTS
        .iter()
        .map(|component| inspect_component(component, branding_available))
        .collect()
}

fn inspect_component(
    component: &'static Component,
    branding_available: bool,
) -> ComponentStatus<'static> {
    let scheme_exists = if component.scheme_path.is_empty() {
        None
    } else {
        Some(Path::new(component.scheme_path).exists())
    };

    let binary_exists = if component.binary_path.is_empty() {
        None
    } else {
        Some(Path::new(component.binary_path).exists())
    };

    let (state, available, status_text) = if component.name == "redbear-release" {
        if branding_available {
            (AvailabilityState::Available, true, "available")
        } else {
            (AvailabilityState::Unavailable, false, "not configured")
        }
    } else if component.name == "redbear-meta" {
        if Path::new(REDBEAR_META_README).exists() {
            (AvailabilityState::Available, true, "available")
        } else {
            (AvailabilityState::Unavailable, false, "missing")
        }
    } else if let Some(exists) = scheme_exists {
        if exists {
            (AvailabilityState::Available, true, "available")
        } else {
            (AvailabilityState::Unavailable, false, "not running")
        }
    } else if let Some(exists) = binary_exists {
        if exists {
            (AvailabilityState::Available, true, "available")
        } else {
            (AvailabilityState::Unavailable, false, "missing")
        }
    } else {
        (AvailabilityState::BuiltIn, true, "built-in")
    };

    ComponentStatus {
        component,
        state,
        available,
        status_text,
        scheme_exists,
        binary_exists,
    }
}

fn print_table(statuses: &[ComponentStatus<'_>], verbose: bool) {
    let name_width = statuses
        .iter()
        .map(|status| status.component.name.len())
        .max()
        .unwrap_or(0);
    let category_width = statuses
        .iter()
        .map(|status| status.component.category.len())
        .max()
        .unwrap_or(0);

    println!("Red Bear OS Component Status");
    println!("{DIVIDER}");
    println!();

    for status in statuses {
        println!(
            "  {} {:name_width$}  [{:category_width$}]  {}",
            colorize(marker_for(status), marker_color(status)),
            status.component.name,
            status.component.category,
            colorize(status.status_text, status_color(status)),
            name_width = name_width,
            category_width = category_width,
        );
        println!("    {}", status.component.description);
        println!("    Test: {}", status.component.test_hint);

        if verbose {
            println!(
                "    Dependencies: {}",
                format_dependencies(status.component.dependencies)
            );
        }

        println!();
    }

    println!("{DIVIDER}");
    println!(
        "{}/{} components available",
        available_count(statuses),
        statuses.len()
    );
}

fn print_tests(statuses: &[ComponentStatus<'_>], verbose: bool) {
    println!("Red Bear OS Runtime Test Hints");
    println!("{DIVIDER}");
    println!();

    let mut printed = 0usize;

    for status in statuses.iter().filter(|status| status.available) {
        println!(
            "  {} {:<16} {}",
            colorize("●", GREEN),
            status.component.name,
            status.component.test_hint,
        );

        if verbose {
            println!(
                "    Dependencies: {}",
                format_dependencies(status.component.dependencies)
            );
        }

        printed += 1;
    }

    if printed == 0 {
        println!("  No available Red Bear OS components detected.");
    }

    println!();
    println!("{DIVIDER}");
    println!("{} test command(s) ready", printed);
}

fn print_json(statuses: &[ComponentStatus<'_>]) {
    let mut output = String::new();

    output.push_str("{\n");
    output.push_str("  \"summary\": {\n");
    output.push_str(&format!(
        "    \"available\": {},\n    \"total\": {}\n",
        available_count(statuses),
        statuses.len()
    ));
    output.push_str("  },\n");
    output.push_str("  \"components\": [\n");

    for (index, status) in statuses.iter().enumerate() {
        output.push_str("    {\n");
        push_json_field(&mut output, "name", status.component.name, true);
        push_json_field(
            &mut output,
            "description",
            status.component.description,
            true,
        );
        push_json_field(&mut output, "category", status.component.category, true);
        push_json_field(
            &mut output,
            "scheme_path",
            status.component.scheme_path,
            true,
        );
        push_json_field(
            &mut output,
            "binary_path",
            status.component.binary_path,
            true,
        );
        push_json_field(&mut output, "test_hint", status.component.test_hint, true);
        output.push_str("      \"dependencies\": ");
        push_json_array(&mut output, status.component.dependencies);
        output.push_str(",\n");
        output.push_str(&format!(
            "      \"available\": {},\n",
            bool_to_json(status.available)
        ));
        push_json_field(&mut output, "status", status.status_text, true);
        push_json_optional_bool(&mut output, "scheme_exists", status.scheme_exists, true);
        push_json_optional_bool(&mut output, "binary_exists", status.binary_exists, false);
        output.push_str("\n    }");

        if index + 1 != statuses.len() {
            output.push(',');
        }

        output.push('\n');
    }

    output.push_str("  ]\n");
    output.push('}');
    println!("{output}");
}

fn print_help() {
    println!("Usage: redbear-info [--verbose|-v] [--json|--test]");
    println!();
    println!("Enumerate Red Bear OS custom components and report runtime availability.");
    println!();
    println!("Options:");
    println!("  -v, --verbose  Show component dependencies");
    println!("      --json     Print machine-readable JSON");
    println!("      --test     Print runtime test commands for available components");
    println!("  -h, --help     Show this help message");
}

fn available_count(statuses: &[ComponentStatus<'_>]) -> usize {
    statuses.iter().filter(|status| status.available).count()
}

fn format_dependencies(dependencies: &[&str]) -> String {
    if dependencies.is_empty() {
        "none".to_string()
    } else {
        dependencies.join(", ")
    }
}

fn marker_for(status: &ComponentStatus<'_>) -> &'static str {
    if status.available {
        "●"
    } else {
        "○"
    }
}

fn marker_color(status: &ComponentStatus<'_>) -> &'static str {
    match status.state {
        AvailabilityState::Available | AvailabilityState::BuiltIn => GREEN,
        AvailabilityState::Unavailable => status_color(status),
    }
}

fn status_color(status: &ComponentStatus<'_>) -> &'static str {
    match status.state {
        AvailabilityState::Available | AvailabilityState::BuiltIn => GREEN,
        AvailabilityState::Unavailable if status.status_text == "not running" => YELLOW,
        AvailabilityState::Unavailable => RED,
    }
}

fn colorize(text: &str, color: &str) -> String {
    format!("{color}{text}{RESET}")
}

fn bool_to_json(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

fn push_json_field(output: &mut String, key: &str, value: &str, trailing_comma: bool) {
    output.push_str("      ");
    push_json_string(output, key);
    output.push_str(": ");
    push_json_string(output, value);

    if trailing_comma {
        output.push(',');
    }

    output.push('\n');
}

fn push_json_array(output: &mut String, values: &[&str]) {
    output.push('[');

    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        push_json_string(output, value);
    }

    output.push(']');
}

fn push_json_optional_bool(
    output: &mut String,
    key: &str,
    value: Option<bool>,
    trailing_comma: bool,
) {
    output.push_str("      ");
    push_json_string(output, key);
    output.push_str(": ");

    match value {
        Some(flag) => output.push_str(bool_to_json(flag)),
        None => output.push_str("null"),
    }

    if trailing_comma {
        output.push(',');
    }

    output.push('\n');
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');

    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(ch),
        }
    }

    output.push('"');
}
