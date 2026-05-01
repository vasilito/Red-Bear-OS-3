use std::convert::TryFrom;
use std::fs::File;
use std::mem;
use std::ops::ControlFlow;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;

use ::acpi::aml::op_region::{RegionHandler, RegionSpace};
use event::{EventFlags, RawEventQueue};
use redox_scheme::{scheme::register_sync_scheme, Socket};
use scheme_utils::Blocking;

mod acpi;
mod aml_physmem;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod ec;

mod scheme;

fn daemon(daemon: daemon::Daemon) -> ! {
    common::setup_logging(
        "misc",
        "acpi",
        "acpid",
        common::output_level(),
        common::file_level(),
    );

    log::info!("acpid start");

    let rxsdt_raw_data: Arc<[u8]> = match std::fs::read("/scheme/kernel.acpi/rxsdt") {
        Ok(data) => data.into(),
        Err(err) => {
            log::error!("acpid: failed to read `/scheme/kernel.acpi/rxsdt`: {}", err);
            std::process::exit(1);
        }
    };

    if rxsdt_raw_data.is_empty() {
        log::info!("System doesn't use ACPI");
        daemon.ready();
        std::process::exit(0);
    }

    let sdt = match self::acpi::Sdt::new(rxsdt_raw_data) {
        Ok(sdt) => sdt,
        Err(err) => {
            log::error!("acpid: failed to parse [RX]SDT: {:?}", err);
            std::process::exit(1);
        }
    };

    let mut thirty_two_bit;
    let mut sixty_four_bit;

    let physaddrs_iter = match &sdt.signature {
        b"RSDT" => {
            thirty_two_bit = sdt
                .data()
                .chunks(mem::size_of::<u32>())
                // TODO: With const generics, the compiler has some way of doing this for static sizes.
                .map(|chunk| <[u8; mem::size_of::<u32>()]>::try_from(chunk).unwrap())
                .map(|chunk| u32::from_le_bytes(chunk))
                .map(u64::from);

            &mut thirty_two_bit as &mut dyn Iterator<Item = u64>
        }
        b"XSDT" => {
            sixty_four_bit = sdt
                .data()
                .chunks(mem::size_of::<u64>())
                .map(|chunk| <[u8; mem::size_of::<u64>()]>::try_from(chunk).unwrap())
                .map(|chunk| u64::from_le_bytes(chunk));

            &mut sixty_four_bit as &mut dyn Iterator<Item = u64>
        }
        _ => {
            log::error!("acpid: expected [RX]SDT from kernel to be RSDT or XSDT");
            std::process::exit(1);
        }
    };

    let region_handlers: Vec<(RegionSpace, Box<dyn RegionHandler + 'static>)> = vec![
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        (RegionSpace::EmbeddedControl, Box::new(ec::Ec::new())),
    ];
    let acpi_context = self::acpi::AcpiContext::init(physaddrs_iter, region_handlers);

    // TODO: I/O permission bitmap?
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if let Err(err) = common::acquire_port_io_rights() {
        log::error!("acpid: failed to set I/O privilege level to Ring 3: {:?}", err);
        std::process::exit(1);
    }

    let shutdown_pipe = match File::open("/scheme/kernel.acpi/kstop") {
        Ok(f) => f,
        Err(err) => {
            log::error!("acpid: failed to open `/scheme/kernel.acpi/kstop`: {}", err);
            std::process::exit(1);
        }
    };

    let mut event_queue = match RawEventQueue::new() {
        Ok(q) => q,
        Err(err) => {
            log::error!("acpid: failed to create event queue: {:?}", err);
            std::process::exit(1);
        }
    };
    let socket = match Socket::nonblock() {
        Ok(s) => s,
        Err(err) => {
            log::error!("acpid: failed to create scheme socket: {:?}", err);
            std::process::exit(1);
        }
    };

    let mut scheme = self::scheme::AcpiScheme::new(&acpi_context, &socket);
    let mut handler = Blocking::new(&socket, 16);

    if let Err(err) = event_queue
        .subscribe(shutdown_pipe.as_raw_fd() as usize, 0, EventFlags::READ)
    {
        log::error!("acpid: failed to register shutdown pipe for event queue: {:?}", err);
        std::process::exit(1);
    }
    if let Err(err) = event_queue
        .subscribe(socket.inner().raw(), 1, EventFlags::READ)
    {
        log::error!("acpid: failed to register scheme socket for event queue: {:?}", err);
        std::process::exit(1);
    }

    if let Err(err) = register_sync_scheme(&socket, "acpi", &mut scheme) {
        log::error!("acpid: failed to register acpi scheme to namespace: {:?}", err);
        std::process::exit(1);
    }

    daemon.ready();

    if let Err(err) = libredox::call::setrens(0, 0) {
        log::error!("acpid: failed to enter null namespace: {}", err);
        std::process::exit(1);
    }

    let mut mounted = true;
    while mounted {
        let event = match event_queue.next().transpose() {
            Ok(Some(ev)) => ev,
            Ok(None) => break,
            Err(err) => {
                log::error!("acpid: failed to read event file: {:?}", err);
                break;
            }
        };

        if event.fd == socket.inner().raw() {
            loop {
                match handler.process_requests_nonblocking(&mut scheme) {
                    Ok(flow) => match flow {
                        ControlFlow::Continue(()) => {}
                        ControlFlow::Break(()) => break,
                    },
                    Err(err) => {
                        log::error!("acpid: failed to process requests: {:?}", err);
                        break;
                    }
                }
            }
        } else if event.fd == shutdown_pipe.as_raw_fd() as usize {
            log::info!("Received shutdown request from kernel.");
            mounted = false;
        } else {
            log::debug!("Received request to unknown fd: {}", event.fd);
            continue;
        }
    }

    drop(shutdown_pipe);
    drop(event_queue);

    acpi_context.set_global_s_state(5);

    unreachable!("System should have shut down before this is entered");
}

fn main() {
    common::init();
    daemon::Daemon::new(daemon);
}
