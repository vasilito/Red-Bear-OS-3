use libredox::{flag, Fd};
use redox_scheme::scheme::register_sync_scheme;
use redox_scheme::Socket;
use scheme_utils::ReadinessBased;
use std::{env, usize};

use event::{user_data, EventQueue};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub mod device;

fn main() {
    daemon::Daemon::new(daemon);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn daemon(daemon: daemon::Daemon) -> ! {
    let mut args = env::args().skip(1);

    let addr_str = args.next().unwrap_or("220".to_string());
    let addr = u16::from_str_radix(&addr_str, 16).expect("sb16: failed to parse address");

    println!(" + sb16 at 0x{:X}\n", addr);

    common::setup_logging(
        "audio",
        "pci",
        "sb16",
        common::output_level(),
        common::file_level(),
    );

    common::acquire_port_io_rights().expect("sb16d: failed to acquire port IO rights");

    let socket = Socket::nonblock().expect("sb16d: failed to create socket");
    let mut device = unsafe { device::Sb16::new(addr).expect("sb16d: failed to allocate device") };
    let mut readiness_based = ReadinessBased::new(&socket, 16);

    //TODO: error on multiple IRQs?
    let irq_file = match device.irqs.first() {
        Some(irq) => Fd::open(&format!("/scheme/irq/{}", irq), flag::O_RDWR, 0)
            .expect("sb16d: failed to open IRQ file"),
        None => panic!("sb16d: no IRQs found"),
    };
    user_data! {
        enum Source {
            Irq,
            Scheme,
        }
    }

    let event_queue = EventQueue::<Source>::new().expect("sb16d: Could not create event queue.");
    event_queue
        .subscribe(irq_file.raw(), Source::Irq, event::EventFlags::READ)
        .unwrap();
    event_queue
        .subscribe(
            socket.inner().raw(),
            Source::Scheme,
            event::EventFlags::READ,
        )
        .unwrap();

    register_sync_scheme(&socket, "sb16d", &mut device)
        .expect("sb16d: failed to register audiohw scheme to namespace");

    daemon.ready();

    libredox::call::setrens(0, 0).expect("sb16d: failed to enter null namespace");

    let all = [Source::Irq, Source::Scheme];

    for event in all
        .into_iter()
        .chain(event_queue.map(|e| e.expect("sb16d: failed to get next event").user_data))
    {
        match event {
            Source::Irq => {
                let mut irq = [0; 8];
                irq_file.read(&mut irq).unwrap();

                if !device.irq() {
                    continue;
                }
                irq_file.write(&mut irq).unwrap();

                readiness_based
                    .poll_all_requests(&mut device)
                    .expect("sb16d: failed to poll requests");
                readiness_based
                    .write_responses()
                    .expect("sb16d: failed to write to socket");

                /*
                let next_read = device_irq.next_read();
                if next_read > 0 {
                    return Ok(Some(next_read));
                }
                */
            }
            Source::Scheme => {
                readiness_based
                    .read_and_process_requests(&mut device)
                    .expect("sb16d: failed to read from socket");
                readiness_based
                    .write_responses()
                    .expect("sb16d: failed to write to socket");
            }
        }
    }

    std::process::exit(0);
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn daemon(daemon: daemon::Daemon) -> ! {
    unimplemented!()
}
