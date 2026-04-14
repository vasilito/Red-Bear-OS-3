use anyhow::{anyhow, bail, Context, Result};
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::time::Duration;

const ICMP_UDP_EVENT_LEN: usize = 12;
const TRACE_KIND_TIME_EXCEEDED: u8 = 1;
const TRACE_KIND_DST_UNREACHABLE: u8 = 2;
#[cfg(target_os = "redox")]
const DEFAULT_PAYLOAD_LEN: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceKind {
    TimeExceeded,
    DestinationUnreachable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub kind: TraceKind,
    pub code: u8,
    pub responder: Ipv4Addr,
    pub source_port: u16,
    pub dest_port: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProbeStatus {
    Hop,
    Reached,
    Unreachable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeReply {
    pub event: TraceEvent,
    pub status: ProbeStatus,
}

impl ProbeReply {
    pub fn hop(self) -> Ipv4Addr {
        self.event.responder
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProbeObservation {
    pub reply: Option<ProbeReply>,
    pub rtt: Duration,
}

pub fn resolve_destination(host: &str) -> Result<Ipv4Addr> {
    match (host, 0)
        .to_socket_addrs()?
        .find_map(|addr| match addr.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        }) {
        Some(ip) => Ok(ip),
        None => bail!("{host} did not resolve to an IPv4 destination"),
    }
}

pub fn destination_port(base_port: u16, sequence: usize) -> Result<u16> {
    let port = u32::from(base_port)
        .checked_add(u32::try_from(sequence).context("probe sequence overflow")?)
        .ok_or_else(|| anyhow!("destination port overflow"))?;

    u16::try_from(port).map_err(|_| anyhow!("destination port overflow"))
}

pub fn decode_icmp_udp_event(buf: &[u8]) -> Result<TraceEvent> {
    if buf.len() != ICMP_UDP_EVENT_LEN {
        bail!(
            "unexpected ICMP traceroute event length: expected {}, got {}",
            ICMP_UDP_EVENT_LEN,
            buf.len()
        );
    }

    let kind = match buf[0] {
        TRACE_KIND_TIME_EXCEEDED => TraceKind::TimeExceeded,
        TRACE_KIND_DST_UNREACHABLE => TraceKind::DestinationUnreachable,
        other => bail!("unknown ICMP traceroute event kind {other}"),
    };

    Ok(TraceEvent {
        kind,
        code: buf[1],
        responder: Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]),
        source_port: u16::from_be_bytes([buf[8], buf[9]]),
        dest_port: u16::from_be_bytes([buf[10], buf[11]]),
    })
}

pub fn classify_reply(event: TraceEvent, destination: Ipv4Addr) -> ProbeReply {
    let status = match event.kind {
        TraceKind::TimeExceeded => ProbeStatus::Hop,
        TraceKind::DestinationUnreachable if event.code == 3 && event.responder == destination => {
            ProbeStatus::Reached
        }
        TraceKind::DestinationUnreachable => ProbeStatus::Unreachable,
    };

    ProbeReply { event, status }
}

pub fn unreachable_label(code: u8) -> &'static str {
    match code {
        0 => "!N",
        1 => "!H",
        2 => "!PR",
        3 => "!P",
        4 => "!F",
        5 => "!SR",
        9 => "!X",
        10 => "!XH",
        13 => "!A",
        _ => "!U",
    }
}

pub fn format_reply_suffix(reply: ProbeReply) -> Option<&'static str> {
    match reply.status {
        ProbeStatus::Hop | ProbeStatus::Reached => None,
        ProbeStatus::Unreachable => Some(unreachable_label(reply.event.code)),
    }
}

pub fn probe(
    destination: Ipv4Addr,
    ttl: u8,
    dest_port: u16,
    timeout: Duration,
) -> Result<ProbeObservation> {
    redox_probe(destination, ttl, dest_port, timeout)
}

#[cfg(target_os = "redox")]
fn redox_probe(
    destination: Ipv4Addr,
    ttl: u8,
    dest_port: u16,
    timeout: Duration,
) -> Result<ProbeObservation> {
    use libredox::{flag, Fd};
    use std::fs::File;
    use std::io::Write;
    use std::mem;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
    use std::thread;
    use std::time::Instant;

    fn open_udp_socket(destination: Ipv4Addr, dest_port: u16) -> Result<OwnedFd> {
        let socket = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
        if socket < 0 {
            return Err(std::io::Error::last_os_error()).context("failed to create UDP socket");
        }

        let socket = unsafe { OwnedFd::from_raw_fd(socket) };
        let mut addr: libc::sockaddr_in = unsafe { mem::zeroed() };
        addr.sin_family = libc::AF_INET as libc::sa_family_t;
        addr.sin_port = dest_port.to_be();
        addr.sin_addr = libc::in_addr {
            s_addr: u32::from_ne_bytes(destination.octets()),
        };

        let rc = unsafe {
            libc::connect(
                socket.as_raw_fd(),
                &addr as *const libc::sockaddr_in as *const libc::sockaddr,
                mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error()).context("failed to connect UDP socket");
        }

        Ok(socket)
    }

    fn udp_local_port(socket: RawFd) -> Result<u16> {
        let mut addr: libc::sockaddr_in = unsafe { mem::zeroed() };
        let mut len = mem::size_of::<libc::sockaddr_in>() as libc::socklen_t;
        let rc = unsafe {
            libc::getsockname(
                socket,
                &mut addr as *mut libc::sockaddr_in as *mut libc::sockaddr,
                &mut len,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error()).context("failed to query UDP source port");
        }

        Ok(u16::from_be(addr.sin_port))
    }

    fn set_hop_limit(socket: RawFd, ttl: u8) -> Result<()> {
        let raw = syscall::dup(socket as usize, b"hop_limit")
            .map_err(|err| anyhow!("failed to open hop_limit setting: {err}"))?;
        let mut hop_limit = unsafe { File::from_raw_fd(raw as RawFd) };
        hop_limit
            .write_all(&[ttl])
            .context("failed to set UDP hop_limit")
    }

    fn open_icmp_socket(destination: Ipv4Addr, source_port: u16) -> Result<Fd> {
        let path = format!("/scheme/icmp/udp/{destination}/{source_port}");
        Fd::open(&path, flag::O_RDWR | flag::O_NONBLOCK, 0)
            .map_err(|err| anyhow!("failed to open {path}: {err}"))
    }

    fn send_probe(socket: RawFd, payload: &[u8]) -> Result<()> {
        let written = unsafe { libc::send(socket, payload.as_ptr().cast(), payload.len(), 0) };
        if written < 0 {
            return Err(std::io::Error::last_os_error()).context("failed to send UDP probe");
        }
        if written as usize != payload.len() {
            bail!(
                "short UDP probe write: expected {}, got {}",
                payload.len(),
                written
            );
        }
        Ok(())
    }

    fn wait_for_icmp_event(icmp_fd: &mut Fd, timeout: Duration) -> Result<Option<TraceEvent>> {
        let started = Instant::now();
        let mut raw = [0_u8; ICMP_UDP_EVENT_LEN];

        loop {
            match icmp_fd.read(&mut raw) {
                Ok(0) => bail!("ICMP traceroute socket closed unexpectedly"),
                Ok(len) if len == raw.len() => return decode_icmp_udp_event(&raw).map(Some),
                Ok(len) => bail!(
                    "unexpected ICMP traceroute event length: expected {}, got {}",
                    raw.len(),
                    len
                ),
                Err(err) if err.is_wouldblock() => {
                    if started.elapsed() >= timeout {
                        return Ok(None);
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => return Err(err).context("failed reading ICMP traceroute event"),
            }
        }
    }

    let socket = open_udp_socket(destination, dest_port)?;
    let source_port = udp_local_port(socket.as_raw_fd())?;
    let mut icmp_fd = open_icmp_socket(destination, source_port)?;
    set_hop_limit(socket.as_raw_fd(), ttl)?;

    let payload = vec![0x42_u8; DEFAULT_PAYLOAD_LEN];
    let started = Instant::now();
    send_probe(socket.as_raw_fd(), &payload)?;
    let reply =
        wait_for_icmp_event(&mut icmp_fd, timeout)?.map(|event| classify_reply(event, destination));

    Ok(ProbeObservation {
        reply,
        rtt: started.elapsed(),
    })
}

#[cfg(not(target_os = "redox"))]
fn redox_probe(
    _destination: Ipv4Addr,
    _ttl: u8,
    _dest_port: u16,
    _timeout: Duration,
) -> Result<ProbeObservation> {
    bail!("redbear-traceroute probing is only available when built for Redox")
}

#[cfg(test)]
mod tests {
    use super::{
        classify_reply, decode_icmp_udp_event, destination_port, format_reply_suffix, ProbeStatus,
        TraceEvent, TraceKind,
    };
    use std::net::Ipv4Addr;

    #[test]
    fn decodes_time_exceeded_event() {
        let raw = [1, 0, 0, 0, 192, 0, 2, 1, 0xa4, 0x10, 0x82, 0x9a];
        let event = decode_icmp_udp_event(&raw).expect("expected event decode");

        assert_eq!(event.kind, TraceKind::TimeExceeded);
        assert_eq!(event.responder, Ipv4Addr::new(192, 0, 2, 1));
        assert_eq!(event.source_port, 42_000);
        assert_eq!(event.dest_port, 33_434);
    }

    #[test]
    fn classifies_target_port_unreachable_as_reached() {
        let event = TraceEvent {
            kind: TraceKind::DestinationUnreachable,
            code: 3,
            responder: Ipv4Addr::new(203, 0, 113, 9),
            source_port: 42_000,
            dest_port: 33_434,
        };

        let reply = classify_reply(event, Ipv4Addr::new(203, 0, 113, 9));
        assert_eq!(reply.status, ProbeStatus::Reached);
        assert_eq!(format_reply_suffix(reply), None);
    }

    #[test]
    fn classifies_non_target_unreachable_as_terminal_error() {
        let event = TraceEvent {
            kind: TraceKind::DestinationUnreachable,
            code: 1,
            responder: Ipv4Addr::new(192, 0, 2, 7),
            source_port: 42_000,
            dest_port: 33_434,
        };

        let reply = classify_reply(event, Ipv4Addr::new(203, 0, 113, 9));
        assert_eq!(reply.status, ProbeStatus::Unreachable);
        assert_eq!(format_reply_suffix(reply), Some("!H"));
    }

    #[test]
    fn destination_port_rejects_overflow() {
        assert!(destination_port(u16::MAX, 1).is_err());
    }
}
