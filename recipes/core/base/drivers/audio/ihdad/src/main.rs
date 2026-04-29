use redox_scheme::scheme::register_sync_scheme;
use redox_scheme::Socket;
use scheme_utils::ReadinessBased;
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::usize;

use event::{user_data, EventQueue};
use pcid_interface::irq_helpers::pci_allocate_interrupt_vector;
use pcid_interface::PciFunctionHandle;

pub mod hda;

/*
VEND:PROD
Virtualbox   8086:2668
QEMU ICH9    8086:293E
82801H ICH8  8086:284B
*/

fn main() {
    pcid_interface::pci_daemon(daemon);
}

fn daemon(daemon: daemon::Daemon, mut pcid_handle: PciFunctionHandle) -> ! {
    let pci_config = pcid_handle.config();

    let mut name = pci_config.func.name();
    name.push_str("_ihda");

    common::setup_logging(
        "audio",
        "pci",
        &name,
        common::output_level(),
        common::file_level(),
    );

    log::info!("IHDA {}", pci_config.func.display());

    let address = unsafe { pcid_handle.map_bar(0) }.ptr.as_ptr() as usize;

    let irq_file = pci_allocate_interrupt_vector(&mut pcid_handle, "ihdad");

    {
        let vend_prod: u32 = ((pci_config.func.full_device_id.vendor_id as u32) << 16)
            | (pci_config.func.full_device_id.device_id as u32);

        user_data! {
            enum Source {
                Irq,
                Scheme,
            }
        }

        let event_queue =
            EventQueue::<Source>::new().expect("ihdad: Could not create event queue.");
        let socket = Socket::nonblock().expect("ihdad: failed to create socket");
        let mut device = unsafe {
            hda::IntelHDA::new(address, vend_prod).expect("ihdad: failed to allocate device")
        };
        let mut readiness_based = ReadinessBased::new(&socket, 16);

        register_sync_scheme(&socket, "audiohw", &mut device)
            .expect("ihdad: failed to register audiohw scheme to namespace");
        daemon.ready();

        event_queue
            .subscribe(
                socket.inner().raw(),
                Source::Scheme,
                event::EventFlags::READ,
            )
            .unwrap();
        event_queue
            .subscribe(
                irq_file.irq_handle().as_raw_fd() as usize,
                Source::Irq,
                event::EventFlags::READ,
            )
            .unwrap();

        libredox::call::setrens(0, 0).expect("ihdad: failed to enter null namespace");

        let all = [Source::Irq, Source::Scheme];

        for event in all
            .into_iter()
            .chain(event_queue.map(|e| e.expect("failed to get next event").user_data))
        {
            match event {
                Source::Irq => {
                    let mut irq = [0; 8];
                    irq_file.irq_handle().read(&mut irq).unwrap();

                    if !device.irq() {
                        continue;
                    }
                    irq_file.irq_handle().write(&mut irq).unwrap();

                    readiness_based
                        .poll_all_requests(&mut device)
                        .expect("ihdad: failed to poll requests");
                    readiness_based
                        .write_responses()
                        .expect("ihdad: failed to write to socket");

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
                        .expect("ihdad: failed to read from socket");
                    readiness_based
                        .write_responses()
                        .expect("ihdad: failed to write to socket");

                    /*
                    let next_read = device.borrow().next_read();
                    if next_read > 0 {
                    return Ok(Some(next_read));
                    }
                    */
                }
            }
        }

        std::process::exit(0);
    }
}
