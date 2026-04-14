mod device;
mod scheme;
mod translate;
mod types;

use std::collections::VecDeque;
use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Read};
use std::mem::{size_of, MaybeUninit};
#[cfg(target_os = "redox")]
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::process;
#[cfg(not(target_os = "redox"))]
use std::thread;
#[cfg(not(target_os = "redox"))]
use std::time::Duration;

use log::{error, info, LevelFilter, Metadata, Record};
use orbclient::{Event, EventOption};
use redox_scheme::{Request, SignalBehavior, Socket};
use syscall::error::EAGAIN;
use syscall::flag::O_NONBLOCK;

use scheme::EvdevScheme;

#[cfg(target_os = "redox")]
use event::{EventFlags as QueueEventFlags, RawEventQueue};

#[cfg(target_os = "redox")]
const SCHEME_QUEUE_TOKEN: usize = 1;
#[cfg(target_os = "redox")]
const INPUT_QUEUE_TOKEN: usize = 2;

struct StderrLogger {
    level: LevelFilter,
}

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }
    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

struct InputConsumer {
    file: File,
    partial: Vec<u8>,
}

impl InputConsumer {
    fn open() -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(O_NONBLOCK as i32)
            .open("/scheme/input/consumer")
            .map_err(|e| format!("failed to open /scheme/input/consumer: {e}"))?;

        Ok(Self {
            file,
            partial: Vec::new(),
        })
    }

    #[cfg(target_os = "redox")]
    fn fd(&self) -> usize {
        self.file.as_raw_fd() as usize
    }

    fn read_available(&mut self, scheme: &mut EvdevScheme) -> Result<bool, String> {
        let event_size = size_of::<Event>();
        let mut buf = vec![0u8; event_size * 32];
        let mut progress = false;

        loop {
            match self.file.read(&mut buf) {
                Ok(0) => break,
                Ok(count) => {
                    progress = true;
                    self.partial.extend_from_slice(&buf[..count]);

                    while self.partial.len() >= event_size {
                        let event = read_event_from_bytes(&self.partial[..event_size]);
                        self.partial.drain(..event_size);
                        dispatch_input_event(event, scheme);
                    }
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(format!("failed to read /scheme/input/consumer: {err}")),
            }
        }

        Ok(progress)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SchemePoll {
    mounted: bool,
    progress: bool,
}

fn read_event_from_bytes(bytes: &[u8]) -> Event {
    let mut event = MaybeUninit::<Event>::uninit();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), event.as_mut_ptr() as *mut u8, bytes.len());
        event.assume_init()
    }
}

fn dispatch_input_event(event: Event, scheme: &mut EvdevScheme) {
    match event.to_option() {
        EventOption::Key(key) => scheme.feed_keyboard_event(key.scancode, key.pressed),
        EventOption::Mouse(mouse) => scheme.feed_touchpad_position(mouse.x, mouse.y),
        EventOption::MouseRelative(mouse) => scheme.feed_mouse_move(mouse.dx, mouse.dy),
        EventOption::Button(button) => {
            scheme.feed_mouse_buttons(button.left, button.middle, button.right)
        }
        EventOption::Scroll(scroll) => scheme.feed_mouse_scroll(scroll.x, scroll.y),
        _ => {}
    }
}

fn is_would_block_socket(err: &syscall::Error) -> bool {
    err.errno == EAGAIN
}

fn write_scheme_response(socket: &Socket, response: redox_scheme::Response) -> Result<(), String> {
    socket
        .write_response(response, SignalBehavior::Restart)
        .map_err(|e| format!("failed to write response: {e}"))?;
    Ok(())
}

fn handle_request(
    request: Request,
    scheme: &mut EvdevScheme,
    pending_requests: &mut VecDeque<Request>,
    socket: &Socket,
) -> Result<bool, String> {
    match request.handle_scheme_block_mut(scheme) {
        Ok(response) => {
            write_scheme_response(socket, response)?;
            Ok(true)
        }
        Err(request) => {
            pending_requests.push_back(request);
            Ok(true)
        }
    }
}

fn flush_pending_requests(
    scheme: &mut EvdevScheme,
    pending_requests: &mut VecDeque<Request>,
    socket: &Socket,
) -> Result<bool, String> {
    let mut progress = false;
    let pending_len = pending_requests.len();

    for _ in 0..pending_len {
        let Some(request) = pending_requests.pop_front() else {
            break;
        };

        match request.handle_scheme_block_mut(scheme) {
            Ok(response) => {
                write_scheme_response(socket, response)?;
                progress = true;
            }
            Err(request) => pending_requests.push_back(request),
        }
    }

    Ok(progress)
}

fn read_scheme_requests(
    socket: &Socket,
    scheme: &mut EvdevScheme,
    pending_requests: &mut VecDeque<Request>,
) -> Result<SchemePoll, String> {
    let mut poll = SchemePoll {
        mounted: true,
        progress: false,
    };

    loop {
        match socket.next_request(SignalBehavior::Restart) {
            Ok(Some(request)) => {
                poll.progress =
                    handle_request(request, scheme, pending_requests, socket)? || poll.progress;
            }
            Ok(None) => {
                poll.mounted = false;
                break;
            }
            Err(err) if is_would_block_socket(&err) => break,
            Err(err) => return Err(format!("failed to read scheme request: {err}")),
        }
    }

    Ok(poll)
}

#[cfg(target_os = "redox")]
fn run_redox_event_loop(
    socket: &Socket,
    scheme: &mut EvdevScheme,
    input: &mut InputConsumer,
    pending_requests: &mut VecDeque<Request>,
) -> Result<(), String> {
    let event_queue =
        RawEventQueue::new().map_err(|e| format!("failed to create event queue: {e}"))?;
    event_queue
        .subscribe(
            socket.inner().raw(),
            SCHEME_QUEUE_TOKEN,
            QueueEventFlags::READ,
        )
        .map_err(|e| format!("failed to subscribe scheme socket: {e}"))?;
    event_queue
        .subscribe(input.fd(), INPUT_QUEUE_TOKEN, QueueEventFlags::READ)
        .map_err(|e| format!("failed to subscribe input consumer: {e}"))?;

    loop {
        let raw_event = event_queue
            .next_event()
            .map_err(|e| format!("failed to wait for events: {e}"))?;

        match raw_event.user_data {
            SCHEME_QUEUE_TOKEN => {
                let poll = read_scheme_requests(socket, scheme, pending_requests)?;
                if !poll.mounted {
                    info!("evdevd: scheme unmounted, exiting");
                    break;
                }
            }
            INPUT_QUEUE_TOKEN => {
                let _ = input.read_available(scheme)?;
            }
            _ => {}
        }

        let _ = flush_pending_requests(scheme, pending_requests, socket)?;
    }

    Ok(())
}

#[cfg(not(target_os = "redox"))]
fn run_host_event_loop(
    socket: &Socket,
    scheme: &mut EvdevScheme,
    input: &mut InputConsumer,
    pending_requests: &mut VecDeque<Request>,
) -> Result<(), String> {
    loop {
        let mut progress = input.read_available(scheme)?;

        let poll = read_scheme_requests(socket, scheme, pending_requests)?;
        if !poll.mounted {
            info!("evdevd: scheme unmounted, exiting");
            break;
        }
        progress |= poll.progress;
        progress |= flush_pending_requests(scheme, pending_requests, socket)?;

        if !progress {
            thread::sleep(Duration::from_millis(10));
        }
    }

    Ok(())
}

fn run() -> Result<(), String> {
    let mut scheme = EvdevScheme::new();
    let mut input = InputConsumer::open()?;
    let mut pending_requests = VecDeque::new();

    let socket =
        Socket::nonblock("evdev").map_err(|e| format!("failed to register evdev scheme: {}", e))?;
    info!("evdevd: registered scheme:evdev");
    info!("evdevd: consuming orbclient::Event from /scheme/input/consumer");

    #[cfg(target_os = "redox")]
    {
        run_redox_event_loop(&socket, &mut scheme, &mut input, &mut pending_requests)
    }

    #[cfg(not(target_os = "redox"))]
    {
        run_host_event_loop(&socket, &mut scheme, &mut input, &mut pending_requests)
    }
}

fn main() {
    let log_level = match env::var("EVDEVD_LOG").as_deref() {
        Ok("debug") => LevelFilter::Debug,
        Ok("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    };
    let _ = log::set_boxed_logger(Box::new(StderrLogger { level: log_level }));
    log::set_max_level(log_level);

    if let Err(e) = run() {
        error!("evdevd: fatal error: {}", e);
        process::exit(1);
    }
}
