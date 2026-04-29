use anyhow::Context;
use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::io::{FromRawFd, IntoRawFd, RawFd};
use std::process::{Child, Command, ExitStatus, Stdio};

mod sys;

const DEFAULT_COLS: u32 = 80;
const DEFAULT_LINES: u32 = 30;

event::user_data! {
    enum EventData {
        Pty,
        Timer,
    }
}

fn handle(
    event_queue: event::EventQueue<EventData>,
    master_fd: RawFd,
    timeout_fd: RawFd,
    process: &mut Child,
) -> io::Result<ExitStatus> {
    let handle_event = |event: EventData| -> io::Result<bool> {
        match event {
            EventData::Pty => {
                let mut packet = [0; 4096];
                loop {
                    // Read data from PTY master
                    let count = match libredox::call::read(master_fd as usize, &mut packet) {
                        Ok(0) => return Ok(false),
                        Ok(count) => count,
                        Err(ref err) if err.errno() == libredox::errno::EAGAIN => return Ok(true),
                        Err(err) => return Err(err.into()),
                    };

                    // Write data to stdout
                    libredox::call::write(1, &packet[1..count])?;

                    for i in 1..count {
                        // Write byte to QEMU debugcon (Bochs compatible)
                        sys::debug_char(packet[i]);
                    }
                }
            }
            EventData::Timer => {
                let mut timespec = syscall::TimeSpec::default();
                libredox::call::read(timeout_fd as usize, &mut timespec)?;

                timespec.tv_sec += 1;
                libredox::call::write(timeout_fd as usize, &mut timespec)?;

                Ok(true)
            }
        }
    };

    if handle_event(EventData::Pty)? && handle_event(EventData::Timer)? {
        'events: loop {
            match process.try_wait() {
                Ok(status_opt) => match status_opt {
                    Some(status) => return Ok(status),
                    None => (),
                },
                Err(err) => match err.kind() {
                    io::ErrorKind::WouldBlock => (),
                    _ => return Err(err),
                },
            }

            let event = event_queue.next_event()?;
            if !handle_event(event.user_data)? {
                break 'events;
            }
        }
    }

    let _ = process.kill();
    process.wait()
}

fn getpty(columns: u32, lines: u32) -> io::Result<(RawFd, String)> {
    let master = libredox::call::open(
        "/scheme/pty",
        libredox::flag::O_CLOEXEC
            | libredox::flag::O_RDWR
            | libredox::flag::O_CREAT
            | libredox::flag::O_NONBLOCK,
        0,
    )?;

    if let Ok(winsize_fd) = libredox::call::dup(master, b"winsize") {
        let _ = libredox::call::write(
            winsize_fd,
            &redox_termios::Winsize {
                ws_row: lines as u16,
                ws_col: columns as u16,
            },
        );
        let _ = libredox::call::close(winsize_fd);
    }

    let mut buf: [u8; 4096] = [0; 4096];
    let count = libredox::call::fpath(master, &mut buf)?;
    Ok((master as RawFd, unsafe {
        String::from_utf8_unchecked(Vec::from(&buf[..count]))
    }))
}

fn inner() -> anyhow::Result<()> {
    common::acquire_port_io_rights()?;

    let config = fs::read_to_string("/etc/redoxerd").context("Failed to read /etc/redoxerd")?;
    let mut config_lines = config.lines();

    let (columns, lines) = (DEFAULT_COLS, DEFAULT_LINES);
    let (master_fd, pty) = getpty(columns, lines)?;

    let timeout_fd = libredox::call::open(
        "/scheme/time/4",
        libredox::flag::O_CLOEXEC | libredox::flag::O_RDWR | libredox::flag::O_NONBLOCK,
        0,
    )? as RawFd;

    let event_queue = event::EventQueue::new()?;
    event_queue.subscribe(master_fd as usize, EventData::Pty, event::EventFlags::READ)?;
    event_queue.subscribe(
        timeout_fd as usize,
        EventData::Timer,
        event::EventFlags::READ,
    )?;

    let slave_stdin = OpenOptions::new().read(true).open(&pty)?;
    let slave_stdout = OpenOptions::new().write(true).open(&pty)?;
    let slave_stderr = OpenOptions::new().write(true).open(&pty)?;

    let Some(name) = config_lines.next() else {
        anyhow::bail!("/etc/redoxerd does not specify command");
    };
    let mut command = Command::new(name);
    for arg in config_lines {
        command.arg(arg);
    }
    unsafe {
        command
            .stdin(Stdio::from_raw_fd(slave_stdin.into_raw_fd()))
            .stdout(Stdio::from_raw_fd(slave_stdout.into_raw_fd()))
            .stderr(Stdio::from_raw_fd(slave_stderr.into_raw_fd()))
            .env("COLUMNS", format!("{}", columns))
            .env("LINES", format!("{}", lines))
            .env("TERM", "xterm-256color")
            .env("TTY", &pty);
    }

    let mut process = command
        .spawn()
        .with_context(|| format!("Failed to spawn {command:?}"))?;
    let status = handle(event_queue, master_fd, timeout_fd, &mut process)
        .with_context(|| format!("Failed to run {name}"))?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("{name} failed with {}", status);
    }
}

fn main() {
    match inner() {
        Ok(()) => {
            // Exit with success using qemu device
            sys::exit_success();
        }
        Err(err) => {
            eprintln!("redoxerd: {:#}", err);

            // Wait a bit for the error message to get flushed through the tty subsystem before exiting.
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Exit with error using qemu device
            sys::exit_failure();
        }
    }
}
