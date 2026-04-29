//! The audio daemon for RedoxOS.
use std::mem::MaybeUninit;
use std::ptr::addr_of_mut;
use std::sync::{Arc, Mutex};
use std::{mem, process, slice, thread};

use anyhow::Context;
use ioslice::IoSlice;
use libredox::flag;
use libredox::{error::Result, Fd};

use redox_scheme::Socket;
use scheme_utils::ReadinessBased;

use daemon::SchemeDaemon;

use self::scheme::AudioScheme;

mod scheme;

extern "C" fn sigusr_handler(_sig: usize) {}

fn thread(scheme: Arc<Mutex<AudioScheme>>, pid: usize, hw_file: Fd) -> Result<()> {
    loop {
        let buffer = scheme.lock().unwrap().buffer();
        let buffer_u8 = unsafe {
            slice::from_raw_parts(buffer.as_ptr() as *const u8, mem::size_of_val(&buffer))
        };

        // Wake up the scheme thread
        libredox::call::kill(pid, libredox::flag::SIGUSR1 as u32)?;

        hw_file.write(&buffer_u8)?;
    }
}

fn daemon(daemon: SchemeDaemon) -> anyhow::Result<()> {
    // Handle signals from the hw thread

    let new_sigaction = unsafe {
        let mut sigaction = MaybeUninit::<libc::sigaction>::uninit();
        addr_of_mut!((*sigaction.as_mut_ptr()).sa_flags).write(0);
        libc::sigemptyset(addr_of_mut!((*sigaction.as_mut_ptr()).sa_mask));
        addr_of_mut!((*sigaction.as_mut_ptr()).sa_sigaction).write(sigusr_handler as usize);
        sigaction.assume_init()
    };
    libredox::call::sigaction(flag::SIGUSR1, Some(&new_sigaction), None)?;

    let pid = libredox::call::getpid()?;

    let hw_file = Fd::open("/scheme/audiohw", flag::O_WRONLY | flag::O_CLOEXEC, 0)?;

    let socket = Socket::create().context("failed to create scheme")?;

    let scheme = Arc::new(Mutex::new(AudioScheme::new()));

    let _ = daemon.ready_sync_scheme(&socket, &mut *scheme.lock().unwrap());

    // Enter a constrained namespace
    let ns = libredox::call::mkns(&[
        IoSlice::new(b"memory"),
        IoSlice::new(b"rand"), // for HashMap
    ])
    .context("failed to make namespace")?;
    libredox::call::setns(ns).context("failed to set namespace")?;

    // Spawn a thread to mix and send audio data
    let scheme_thread = scheme.clone();
    let _thread = thread::spawn(move || thread(scheme_thread, pid, hw_file));

    let mut readiness = ReadinessBased::new(&socket, 16);

    loop {
        readiness.read_and_process_requests(&mut *scheme.lock().unwrap())?;
        readiness.poll_all_requests(&mut *scheme.lock().unwrap())?;
        readiness.write_responses()?;
    }
}

fn main() {
    SchemeDaemon::new(inner);
}

fn inner(x: SchemeDaemon) -> ! {
    match daemon(x) {
        Ok(()) => {
            process::exit(0);
        }
        Err(err) => {
            eprintln!("audiod: {}", err);
            process::exit(1);
        }
    }
}
