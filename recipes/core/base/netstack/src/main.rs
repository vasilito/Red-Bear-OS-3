#[macro_use]
extern crate log;
use std::process;

use anyhow::{anyhow, bail, Context, Result};
use event::{EventFlags, EventQueue};
use libredox::flag::{O_NONBLOCK, O_RDWR};
use libredox::Fd;

use redox_scheme::Socket;
use scheme::Smolnetd;
use smoltcp::wire::EthernetAddress;

mod buffer_pool;
mod error;
mod link;
mod logger;
mod port_set;
mod router;
mod scheme;

fn get_network_adapter() -> Result<String> {
    use std::fs;

    let mut adapters = vec![];

    for entry_res in fs::read_dir("/scheme")? {
        let Ok(entry) = entry_res else {
            continue;
        };

        let Ok(scheme) = entry.file_name().into_string() else {
            continue;
        };

        if !scheme.starts_with("network") {
            continue;
        }

        adapters.push(scheme);
    }

    if adapters.is_empty() {
        bail!("no network adapter found");
    } else {
        let adapter = adapters.remove(0);
        if !adapters.is_empty() {
            // FIXME allow using multiple network adapters at the same time
            warn!("Multiple network adapters found. Only {adapter} will be used");
        }
        Ok(adapter)
    }
}

fn run(daemon: daemon::Daemon) -> Result<()> {
    let adapter = get_network_adapter()?;
    trace!("opening {adapter}:");
    let network_fd = Fd::open(&format!("/scheme/{adapter}"), O_RDWR | O_NONBLOCK, 0)
        .map_err(|e| anyhow!("failed to open {adapter}: {e}"))?;

    let hardware_addr = std::fs::read(format!("/scheme/{adapter}/mac"))
        .map(|mac_address| EthernetAddress::from_bytes(&mac_address))
        .context("failed to get mac address from network adapter")?;

    trace!("opening ip scheme socket");
    let ip_fd = Socket::nonblock()
        .map_err(|e| anyhow!("failed to open create ip scheme socket: {:?}", e))?;

    trace!("opening udp scheme socket");
    let udp_fd =
        Socket::nonblock().map_err(|e| anyhow!("failed to open udp scheme socket: {:?}", e))?;

    trace!("opening tcp scheme socket");
    let tcp_fd =
        Socket::nonblock().map_err(|e| anyhow!("failed to open tcp scheme socket: {:?}", e))?;

    trace!("opening icmp scheme socket");
    let icmp_fd =
        Socket::nonblock().map_err(|e| anyhow!("failed to open icmp scheme socket: {:?}", e))?;

    trace!("opening netcfg scheme socket");
    let netcfg_fd =
        Socket::nonblock().map_err(|e| anyhow!("failed to open netcfg scheme socket {:?}", e))?;

    let time_path = format!("/scheme/time/{}", syscall::CLOCK_MONOTONIC);
    let time_fd = Fd::open(&time_path, O_RDWR, 0).context("failed to open /scheme/time")?;

    event::user_data! {
        enum EventSource {
            Network,
            Time,
            IpScheme,
            UdpScheme,
            TcpScheme,
            IcmpScheme,
            NetcfgScheme,
        }
    }

    let event_queue = EventQueue::<EventSource>::new()
        .map_err(|e| anyhow!("failed to create event queue: {:?}", e))?;

    daemon.ready();

    event_queue
        .subscribe(network_fd.raw(), EventSource::Network, EventFlags::READ)
        .map_err(|e| anyhow!("failed to listen to network events: {:?}", e))?;

    event_queue
        .subscribe(time_fd.raw(), EventSource::Time, EventFlags::READ)
        .map_err(|e| anyhow!("failed to listen to timer events: {:?}", e))?;

    event_queue
        .subscribe(ip_fd.inner().raw(), EventSource::IpScheme, EventFlags::READ)
        .context("failed to listen to ip scheme events")?;

    event_queue
        .subscribe(
            udp_fd.inner().raw(),
            EventSource::UdpScheme,
            EventFlags::READ,
        )
        .context("failed to listen to udp scheme events")?;

    event_queue
        .subscribe(
            tcp_fd.inner().raw(),
            EventSource::TcpScheme,
            EventFlags::READ,
        )
        .context("failed to listen to tcp scheme events")?;

    event_queue
        .subscribe(
            icmp_fd.inner().raw(),
            EventSource::IcmpScheme,
            EventFlags::READ,
        )
        .context("failed to listen to icmp scheme events")?;

    event_queue
        .subscribe(
            netcfg_fd.inner().raw(),
            EventSource::NetcfgScheme,
            EventFlags::READ,
        )
        .context("failed to listen to netcfg scheme events")?;

    let mut smolnetd = Smolnetd::new(
        network_fd,
        hardware_addr,
        ip_fd,
        udp_fd,
        tcp_fd,
        icmp_fd,
        time_fd,
        netcfg_fd,
    )
    .context("smolnetd: failed to initialize smolnetd")?;

    libredox::call::setrens(0, 0).context("smolnetd: failed to enter null namespace")?;

    let all = {
        use EventSource::*;
        [Network, Time, IpScheme, UdpScheme, IcmpScheme, NetcfgScheme].map(Ok)
    };

    for event_res in all
        .into_iter()
        .chain(event_queue.map(|r| r.map(|e| e.user_data)))
    {
        match event_res.map_err(|e| anyhow!("event result is error: {:?}", e))? {
            EventSource::Network => smolnetd.on_network_scheme_event(),
            EventSource::Time => smolnetd.on_time_event(),
            EventSource::IpScheme => smolnetd.on_ip_scheme_event(),
            EventSource::UdpScheme => smolnetd.on_udp_scheme_event(),
            EventSource::TcpScheme => smolnetd.on_tcp_scheme_event(),
            EventSource::IcmpScheme => smolnetd.on_icmp_scheme_event(),
            EventSource::NetcfgScheme => smolnetd.on_netcfg_scheme_event(),
        }
        .map_err(|e| error!("Received packet error: {:?}", e));
    }
    Ok(())
}

fn main() {
    daemon::Daemon::new(daemon_runner);
}

fn daemon_runner(daemon: daemon::Daemon) -> ! {
    logger::init_logger("smolnetd");

    if let Err(err) = run(daemon) {
        error!("smoltcpd: {}", err);
        process::exit(1);
    }
    process::exit(0);
}
