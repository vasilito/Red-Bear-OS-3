#![feature(iter_next_chunk)]
#![feature(if_let_guard)]
#![feature(non_exhaustive_omitted_patterns_lint)]

use std::collections::BTreeMap;

use log::{debug, info, trace, warn};
use pci_types::capability::PciCapability;
use pci_types::{
    Bar as TyBar, CommandRegister, EndpointHeader, HeaderType, PciAddress,
    PciHeader as TyPciHeader, PciPciBridgeHeader,
};
use redox_scheme::scheme::register_sync_scheme;
use scheme_utils::Blocking;

use crate::cfg_access::Pcie;
use pcid_interface::{FullDeviceId, LegacyInterruptLine, PciBar, PciFunction, PciRom};

mod cfg_access;
mod driver_handler;
mod scheme;

pub struct Func {
    inner: PciFunction,

    capabilities: Vec<PciCapability>,
    endpoint_header: EndpointHeader,
    enabled: bool,
}

fn handle_parsed_header(
    pcie: &Pcie,
    tree: &mut BTreeMap<PciAddress, Func>,
    endpoint_header: EndpointHeader,
    full_device_id: FullDeviceId,
) {
    let mut bars = [PciBar::None; 6];
    let mut skip = false;
    for i in 0..6 {
        if skip {
            skip = false;
            continue;
        }
        match endpoint_header.bar(i, pcie) {
            Some(TyBar::Io { port }) => bars[i as usize] = PciBar::Port(port.try_into().unwrap()),
            Some(TyBar::Memory32 {
                address,
                size,
                prefetchable: _,
            }) => {
                bars[i as usize] = PciBar::Memory32 {
                    addr: address,
                    size,
                }
            }
            Some(TyBar::Memory64 {
                address,
                size,
                prefetchable: _,
            }) => {
                bars[i as usize] = PciBar::Memory64 {
                    addr: address,
                    size,
                };
                skip = true; // Each 64bit memory BAR occupies two slots
            }
            None => bars[i as usize] = PciBar::None,
        }
    }

    let mut string = String::new();
    for (i, bar) in bars.iter().enumerate() {
        if !bar.is_none() {
            string.push_str(&format!(" {i}={}", bar.display()));
        }
    }

    if !string.is_empty() {
        debug!("    BAR{}", string);
    }

    //TODO: submit to pci_types
    let get_rom = |pci_address, offset| -> Option<PciRom> {
        use pci_types::ConfigRegionAccess;

        const ROM_ENABLED: u32 = 1;
        const ROM_ADDRESS_MASK: u32 = 0xfffff800;

        let data = unsafe { pcie.read(pci_address, offset) };
        let enabled = (data & ROM_ENABLED) == ROM_ENABLED;
        let addr = data & ROM_ADDRESS_MASK;

        let size = unsafe {
            pcie.write(
                pci_address,
                offset,
                ROM_ADDRESS_MASK | if enabled { ROM_ENABLED } else { 0 },
            );
            let mut readback = pcie.read(pci_address, offset);
            pcie.write(pci_address, offset, data);

            /*
             * If the entire readback value is zero, the BAR is not implemented, so we return `None`.
             */
            if readback == 0x0 {
                return None;
            }

            readback &= ROM_ADDRESS_MASK;
            1 << readback.trailing_zeros()
        };

        Some(PciRom {
            addr,
            size,
            enabled,
        })
    };

    let rom = get_rom(endpoint_header.header().address(), 0x30);
    if let Some(rom) = rom {
        debug!("    ROM={:08X}", rom.addr);
    }

    let capabilities = if endpoint_header.status(pcie).has_capability_list() {
        endpoint_header.capabilities(pcie).collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    debug!(
        "PCI DEVICE CAPABILITIES for {}: {:?}",
        endpoint_header.header().address(),
        capabilities
    );

    let func = Func {
        inner: pcid_interface::PciFunction {
            addr: endpoint_header.header().address(),
            bars,
            rom,
            legacy_interrupt_line: None, // Will be filled in when enabling the device
            full_device_id: full_device_id.clone(),
        },

        capabilities,
        endpoint_header,
        enabled: false,
    };

    tree.insert(func.inner.addr, func);
}

fn enable_function(
    pcie: &Pcie,
    endpoint_header: &mut EndpointHeader,
    capabilities: &mut [PciCapability],
) -> Option<LegacyInterruptLine> {
    // Enable bus mastering, memory space, and I/O space
    endpoint_header.update_command(pcie, |cmd| {
        cmd | CommandRegister::BUS_MASTER_ENABLE
            | CommandRegister::MEMORY_ENABLE
            | CommandRegister::IO_ENABLE
    });

    // Disable MSI and MSI-X in case a previous driver instance enabled them.
    for capability in capabilities {
        match capability {
            PciCapability::Msi(capability) => {
                capability.set_enabled(false, pcie);
            }
            PciCapability::MsiX(capability) => {
                capability.set_enabled(false, pcie);
            }
            _ => {}
        }
    }

    // Set IRQ line to 9 if not set
    let mut irq = 0xFF;
    let mut interrupt_pin = 0xFF;

    endpoint_header.update_interrupt(pcie, |(pin, mut line)| {
        if line == 0xFF {
            line = 9;
        }
        irq = line;
        interrupt_pin = pin;
        (pin, line)
    });

    let legacy_interrupt_enabled = match interrupt_pin {
        0 => false,
        1 | 2 | 3 | 4 => true,

        other => {
            warn!("pcid: invalid interrupt pin: {}", other);
            false
        }
    };

    if legacy_interrupt_enabled {
        let pci_address = endpoint_header.header().address();
        let dt_address = ((pci_address.bus() as u32) << 16)
            | ((pci_address.device() as u32) << 11)
            | ((pci_address.function() as u32) << 8);
        let addr = [
            dt_address & pcie.interrupt_map_mask[0],
            0u32,
            0u32,
            interrupt_pin as u32 & pcie.interrupt_map_mask[3],
        ];
        let mapping = pcie
            .interrupt_map
            .iter()
            .find(|x| x.addr == addr[0..3] && x.interrupt == addr[3]);
        let phandled = if let Some(mapping) = mapping {
            Some((
                mapping.parent_phandle,
                mapping.parent_interrupt,
                mapping.parent_interrupt_cells,
            ))
        } else {
            None
        };
        if mapping.is_some() {
            debug!("found mapping: addr={:?} => {:?}", addr, phandled);
        }

        Some(LegacyInterruptLine { irq, phandled })
    } else {
        None
    }
}

fn main() {
    common::init();
    daemon::Daemon::new(daemon);
}

fn daemon(daemon: daemon::Daemon) -> ! {
    common::setup_logging(
        "bus",
        "pci",
        "pcid",
        common::output_level(),
        common::file_level(),
    );

    let pcie = Pcie::new();

    info!("PCI SG-BS:DV.F VEND:DEVI CL.SC.IN.RV");

    let mut scheme = scheme::PciScheme::new(pcie);
    let socket = redox_scheme::Socket::create().expect("failed to open pci scheme socket");
    let handler = Blocking::new(&socket, 16);

    {
        match libredox::Fd::open("/scheme/acpi/register_pci", libredox::flag::O_WRONLY, 0) {
            Ok(register_pci) => {
                let access_id = scheme.access();

                let access_fd = socket
                    .create_this_scheme_fd(0, access_id, syscall::O_RDWR, 0)
                    .expect("failed to issue this resource");
                let access_bytes = access_fd.to_ne_bytes();
                let _ = register_pci
                    .call_wo(
                        &access_bytes,
                        syscall::CallFlags::WRITE | syscall::CallFlags::FD,
                        &[],
                    )
                    .expect("failed to send pci_fd to acpid");
            }
            Err(err) => {
                if err.errno() == libredox::errno::ENODEV {
                    debug!("pcid: acpid not found. Running without ACPI integration.");
                } else {
                    warn!("pcid: failed to open acpid register_pci (error: {}). Running without ACPI integration.", err);
                }
            }
        }
    }

    // FIXME Use full ACPI for enumerating the host bridges. MCFG only describes the first
    // host bridge, while multi-processor systems likely have a host bridge for each CPU.
    // See also https://www.kernel.org/doc/html/latest/PCI/acpi-info.html
    // Bus 0x80 is scanned for compatibility with newer (Arrow Lake) Intel CPUs where PCH devices
    // are there. This workaround may not be required if we had ACPI bus enumeration.
    let mut bus_nums = vec![0, 0x80];
    let mut bus_i = 0;
    while bus_i < bus_nums.len() {
        let bus_num = bus_nums[bus_i];
        bus_i += 1;

        for dev_num in 0..32 {
            scan_device(
                &mut scheme.tree,
                &scheme.pcie,
                &mut bus_nums,
                bus_num,
                dev_num,
            );
        }
    }
    debug!("Enumeration complete, now starting pci scheme");

    register_sync_scheme(&socket, "pci", &mut scheme)
        .expect("failed to register pci scheme to namespace");

    let _ = daemon.ready();

    handler
        .process_requests_blocking(scheme)
        .expect("pcid: failed to process requests");
}

fn scan_device(
    tree: &mut BTreeMap<PciAddress, Func>,
    pcie: &Pcie,
    bus_nums: &mut Vec<u8>,
    bus_num: u8,
    dev_num: u8,
) {
    for func_num in 0..8 {
        let header = TyPciHeader::new(PciAddress::new(0, bus_num, dev_num, func_num));

        let (vendor_id, device_id) = header.id(pcie);
        if vendor_id == 0xffff && device_id == 0xffff {
            if func_num == 0 {
                trace!("PCI {:>02X}:{:>02X}: no dev", bus_num, dev_num);
                return;
            }

            continue;
        }

        let (revision, class, subclass, interface) = header.revision_and_class(pcie);
        let full_device_id = FullDeviceId {
            vendor_id,
            device_id,
            class,
            subclass,
            interface,
            revision,
        };

        info!("PCI {} {}", header.address(), full_device_id.display());

        let has_multiple_functions = header.has_multiple_functions(pcie);

        match header.header_type(pcie) {
            HeaderType::Endpoint => {
                handle_parsed_header(
                    pcie,
                    tree,
                    EndpointHeader::from_header(header, pcie).unwrap(),
                    full_device_id,
                );
            }
            HeaderType::PciPciBridge => {
                let bridge_header = PciPciBridgeHeader::from_header(header, pcie).unwrap();
                bus_nums.push(bridge_header.secondary_bus_number(pcie));
            }
            ty => {
                warn!("pcid: unknown header type: {ty:?}");
            }
        }

        if func_num == 0 && !has_multiple_functions {
            return;
        }
    }
}
