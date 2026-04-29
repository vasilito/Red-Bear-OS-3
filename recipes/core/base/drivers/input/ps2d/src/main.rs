#[macro_use]
extern crate bitflags;
extern crate orbclient;
extern crate syscall;

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::process;

use common::acquire_port_io_rights;
use event::{user_data, EventQueue};
use inputd::ProducerHandle;

use crate::state::Ps2d;

mod controller;
mod mouse;
mod state;
mod vm;

fn daemon(daemon: daemon::Daemon) -> ! {
    common::setup_logging(
        "input",
        "ps2",
        "ps2",
        common::output_level(),
        common::file_level(),
    );

    acquire_port_io_rights().expect("ps2d: failed to get I/O permission");

    let input = ProducerHandle::new().expect("ps2d: failed to open input producer");

    user_data! {
        enum Source {
            Keyboard,
            Mouse,
            Time,
        }
    }

    let event_queue: EventQueue<Source> =
        EventQueue::new().expect("ps2d: failed to create event queue");

    let mut key_file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(syscall::O_NONBLOCK as i32)
        .open("/scheme/serio/0")
        .expect("ps2d: failed to open /scheme/serio/0");

    event_queue
        .subscribe(
            key_file.as_raw_fd() as usize,
            Source::Keyboard,
            event::EventFlags::READ,
        )
        .unwrap();

    let mut mouse_file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(syscall::O_NONBLOCK as i32)
        .open("/scheme/serio/1")
        .expect("ps2d: failed to open /scheme/serio/1");

    event_queue
        .subscribe(
            mouse_file.as_raw_fd() as usize,
            Source::Mouse,
            event::EventFlags::READ,
        )
        .unwrap();

    let time_file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(syscall::O_NONBLOCK as i32)
        .open(format!("/scheme/time/{}", syscall::CLOCK_MONOTONIC))
        .expect("ps2d: failed to open /scheme/time");

    event_queue
        .subscribe(
            time_file.as_raw_fd() as usize,
            Source::Time,
            event::EventFlags::READ,
        )
        .unwrap();

    libredox::call::setrens(0, 0).expect("ps2d: failed to enter null namespace");

    daemon.ready();

    let mut ps2d = Ps2d::new(input, time_file);

    let mut data = [0; 256];
    for event in event_queue.map(|e| e.expect("ps2d: failed to get next event").user_data) {
        // There are some gotchas with ps/2 controllers that require this weird
        // way of doing things. You read key and mouse data from the same
        // place. There is a status register that may show you which the data
        // came from, but if it is even implemented it can have a race
        // condition causing keyboard data to be read as mouse data.
        //
        // Due to this, we have a kernel driver doing a small amount of work
        // to grab bytes and sort them based on the source

        let (file, keyboard) = match event {
            Source::Keyboard => (&mut key_file, true),
            Source::Mouse => (&mut mouse_file, false),
            Source::Time => {
                ps2d.time_event();
                continue;
            }
        };

        loop {
            let count = match file.read(&mut data) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => break,
            };
            for i in 0..count {
                ps2d.handle(keyboard, data[i]);
            }
        }
    }

    process::exit(0);
}

fn main() {
    daemon::Daemon::new(daemon);
}
