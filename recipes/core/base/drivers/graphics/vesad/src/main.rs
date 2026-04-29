extern crate orbclient;
extern crate syscall;

use driver_graphics::GraphicsScheme;
use event::{user_data, EventQueue};
use std::collections::HashMap;
use std::env;
use std::os::fd::AsRawFd;

use crate::scheme::{FbAdapter, FrameBuffer};

mod scheme;

fn main() {
    common::init();
    daemon::Daemon::new(daemon);
}
fn daemon(daemon: daemon::Daemon) -> ! {
    if env::var("FRAMEBUFFER_WIDTH").is_err() {
        println!("vesad: No boot framebuffer");
        daemon.ready();
        std::process::exit(0);
    }

    let width = usize::from_str_radix(
        &env::var("FRAMEBUFFER_WIDTH").expect("FRAMEBUFFER_WIDTH not set"),
        16,
    )
    .expect("failed to parse FRAMEBUFFER_WIDTH");
    let height = usize::from_str_radix(
        &env::var("FRAMEBUFFER_HEIGHT").expect("FRAMEBUFFER_HEIGHT not set"),
        16,
    )
    .expect("failed to parse FRAMEBUFFER_HEIGHT");
    let phys = usize::from_str_radix(
        &env::var("FRAMEBUFFER_ADDR").expect("FRAMEBUFFER_ADDR not set"),
        16,
    )
    .expect("failed to parse FRAMEBUFFER_ADDR");
    let stride = usize::from_str_radix(
        &env::var("FRAMEBUFFER_STRIDE").expect("FRAMEBUFFER_STRIDE not set"),
        16,
    )
    .expect("failed to parse FRAMEBUFFER_STRIDE");

    println!(
        "vesad: {}x{} stride {} at 0x{:X}",
        width, height, stride, phys
    );

    if phys == 0 {
        println!("vesad: Boot framebuffer at address 0");
        daemon.ready();
        std::process::exit(0);
    }

    let mut framebuffers = vec![unsafe { FrameBuffer::new(phys, width, height, stride) }];

    //TODO: ideal maximum number of outputs?
    let bootloader_env = std::fs::read_to_string("/scheme/sys/env")
        .expect("failed to read env")
        .lines()
        .map(|line| {
            let (env, value) = line.split_once('=').unwrap();
            (env.to_owned(), value.to_owned())
        })
        .collect::<HashMap<String, String>>();
    for i in 1..1024 {
        match bootloader_env.get(&format!("FRAMEBUFFER{}", i)) {
            Some(var) => match unsafe { FrameBuffer::parse(&var) } {
                Some(fb) => {
                    println!(
                        "vesad: framebuffer {}: {}x{} stride {} at 0x{:X}",
                        i, fb.width, fb.height, fb.stride, fb.phys
                    );
                    framebuffers.push(fb);
                }
                None => {
                    eprintln!("vesad: framebuffer {}: failed to parse '{}'", i, var);
                }
            },
            None => break,
        };
    }

    let mut scheme =
        GraphicsScheme::new(FbAdapter { framebuffers }, "display.vesa".to_owned(), true);

    user_data! {
        enum Source {
            Input,
            Scheme,
        }
    }

    let event_queue: EventQueue<Source> =
        EventQueue::new().expect("vesad: failed to create event queue");
    event_queue
        .subscribe(
            scheme.inputd_event_handle().as_raw_fd() as usize,
            Source::Input,
            event::EventFlags::READ,
        )
        .unwrap();
    event_queue
        .subscribe(
            scheme.event_handle().raw(),
            Source::Scheme,
            event::EventFlags::READ,
        )
        .unwrap();

    libredox::call::setrens(0, 0).expect("vesad: failed to enter null namespace");

    daemon.ready();

    let all = [Source::Input, Source::Scheme];
    for event in all
        .into_iter()
        .chain(event_queue.map(|e| e.expect("vesad: failed to get next event").user_data))
    {
        match event {
            Source::Input => scheme.handle_vt_events(),
            Source::Scheme => {
                scheme
                    .tick()
                    .expect("vesad: failed to handle scheme events");
            }
        }
    }

    panic!();
}
