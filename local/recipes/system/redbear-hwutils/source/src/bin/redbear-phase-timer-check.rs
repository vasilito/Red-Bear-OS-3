use std::path::Path;
use std::process;
use std::thread;
use std::time::Duration;

use libredox::{Fd, flag};
use redbear_hwutils::parse_args;
use syscall::data::TimeSpec;

const PROGRAM: &str = "redbear-phase-timer-check";
const USAGE: &str = "Usage: redbear-phase-timer-check\n\nRun the bounded timer-source proof check inside the guest.";

fn require_path(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("present={path}");
        Ok(())
    } else {
        Err(format!("missing {path}"))
    }
}

fn monotonic_path() -> Result<String, String> {
    let numeric = format!("/scheme/time/{}", flag::CLOCK_MONOTONIC);
    if require_path(&numeric).is_ok() {
        return Ok(numeric);
    }

    let symbolic = "/scheme/time/CLOCK_MONOTONIC".to_string();
    if require_path(&symbolic).is_ok() {
        return Ok(symbolic);
    }

    Err(format!("missing {numeric} and {symbolic}"))
}

fn read_timespec(fd: &Fd) -> Result<TimeSpec, String> {
    let mut time = TimeSpec::default();
    let bytes = libredox::call::read(fd.raw(), &mut time)
        .map_err(|err| format!("failed to read monotonic time: {err}"))?;
    if bytes < core::mem::size_of::<TimeSpec>() {
        return Err(format!("short read from time scheme: {bytes} bytes"));
    }
    Ok(time)
}

fn timespec_to_nanos(time: &TimeSpec) -> i128 {
    i128::from(time.tv_sec) * 1_000_000_000i128 + i128::from(time.tv_nsec)
}

fn run() -> Result<(), String> {
    parse_args(PROGRAM, USAGE, std::env::args()).map_err(|err| {
        if err.is_empty() {
            process::exit(0);
        }
        err
    })?;

    println!("=== Red Bear OS Timer Runtime Check ===");

    let time_path = monotonic_path()?;

    let time_fd = Fd::open(&time_path, flag::O_RDWR, 0)
        .map_err(|err| format!("failed to open {time_path}: {err}"))?;

    let first = read_timespec(&time_fd)?;
    thread::sleep(Duration::from_millis(50));
    let second = read_timespec(&time_fd)?;

    println!("monotonic_first_sec={}", first.tv_sec);
    println!("monotonic_first_nsec={}", first.tv_nsec);
    println!("monotonic_second_sec={}", second.tv_sec);
    println!("monotonic_second_nsec={}", second.tv_nsec);

    let delta_ns = timespec_to_nanos(&second) - timespec_to_nanos(&first);
    println!("monotonic_delta_ns={delta_ns}");

    if delta_ns <= 0 {
        return Err("monotonic timer did not advance".to_string());
    }

    println!("monotonic_progress=ok");
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}
