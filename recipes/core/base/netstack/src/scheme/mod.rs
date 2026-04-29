use crate::link::ethernet::EthernetLink;
use crate::link::LinkDevice;
use crate::link::{loopback::LoopbackDevice, DeviceList};
use crate::router::route_table::{RouteTable, Rule};
use crate::router::Router;
use crate::scheme::smoltcp::iface::SocketSet as SmoltcpSocketSet;
use crate::scheme::socket::{Handle, SchemeSocket, SocketScheme};
use libredox::flag;
use libredox::Fd;
use redox_scheme::{
    scheme::{IntoTag, Op, SchemeResponse, SchemeState, SchemeSync},
    CallerCtx, RequestKind, Response, SignalBehavior, Socket,
};
use smoltcp;
use smoltcp::iface::{Config, Interface as SmoltcpInterface};
use smoltcp::phy::Tracer;
use smoltcp::socket::AnySocket;
use smoltcp::time::{Duration, Instant};
use smoltcp::wire::{
    EthernetAddress, HardwareAddress, IpAddress, IpCidr, IpListenEndpoint, Ipv4Address,
};
use std::cell::RefCell;
use std::fs::File;
use std::io::{Read, Write};
use std::mem::size_of;
use std::os::fd::{FromRawFd, RawFd};
use std::rc::Rc;
use std::str::FromStr;
use syscall;
use syscall::data::TimeSpec;
use syscall::Error as SyscallError;

use self::icmp::IcmpScheme;
use self::ip::IpScheme;
use self::netcfg::NetCfgScheme;
use self::tcp::TcpScheme;
use self::udp::UdpScheme;
use crate::error::{Error, Result};

mod icmp;
mod ip;
mod netcfg;
mod socket;
mod tcp;
mod udp;

type SocketSet = SmoltcpSocketSet<'static>;
type Interface = Rc<RefCell<SmoltcpInterface>>;

const MAX_DURATION: Duration = Duration::from_micros(u64::MAX);
const MIN_DURATION: Duration = Duration::from_micros(0);

fn getcfg(key: &str) -> Result<String> {
    let mut value = String::new();
    let mut file = File::open(format!("/etc/net/{key}"))?;
    file.read_to_string(&mut value)?;
    Ok(value.trim().to_string())
}

pub struct Smolnetd {
    router_device: Tracer<Router>,
    iface: Interface,
    time_file: File,

    socket_set: Rc<RefCell<SocketSet>>,
    timer: ::std::time::Instant,

    ip_scheme: IpScheme,
    udp_scheme: UdpScheme,
    tcp_scheme: TcpScheme,
    icmp_scheme: IcmpScheme,
    netcfg_scheme: NetCfgScheme,
}

impl Smolnetd {
    pub const MAX_PACKET_SIZE: usize = 2048;
    pub const SOCKET_BUFFER_SIZE: usize = 128; //packets
    pub const MIN_CHECK_TIMEOUT: Duration = Duration::from_millis(10);
    pub const MAX_CHECK_TIMEOUT: Duration = Duration::from_millis(500);

    pub fn new(
        network_file: Fd,
        hardware_addr: EthernetAddress,
        ip_file: Socket,
        udp_file: Socket,
        tcp_file: Socket,
        icmp_file: Socket,
        time_file: Fd,
        netcfg_file: Socket,
    ) -> Result<Smolnetd> {
        let protocol_addrs = vec![
            //This is a placeholder IP for DHCP
            IpCidr::new(IpAddress::v4(0, 0, 0, 0), 8),
            IpCidr::new(IpAddress::v4(127, 0, 0, 1), 8),
        ];

        let default_gw = Ipv4Address::from_str(getcfg("ip_router").unwrap().trim())
            .expect("Can't parse the 'ip_router' cfg.");

        let devices = Rc::new(RefCell::new(DeviceList::default()));
        let route_table = Rc::new(RefCell::new(RouteTable::default()));
        let mut network_device = Tracer::new(
            Router::new(Rc::clone(&devices), Rc::clone(&route_table)),
            |_timestamp, printer| trace!("{}", printer),
        );

        let config = Config::new(HardwareAddress::Ip);
        let mut iface = SmoltcpInterface::new(config, &mut network_device, Instant::now());
        iface.update_ip_addrs(|ip_addrs| ip_addrs.extend(protocol_addrs));
        iface
            .routes_mut()
            .add_default_ipv4_route(default_gw)
            .expect("Failed to add default gateway");

        let iface = Rc::new(RefCell::new(iface));
        let socket_set = Rc::new(RefCell::new(SocketSet::new(vec![])));

        let loopback = LoopbackDevice::default();
        route_table.borrow_mut().insert_rule(Rule::new(
            "127.0.0.0/8".parse().unwrap(),
            None,
            Rc::clone(loopback.name()),
            "127.0.0.1".parse().unwrap(),
        ));

        let mut eth0 = EthernetLink::new("eth0", unsafe {
            File::from_raw_fd(network_file.into_raw() as RawFd)
        });
        eth0.set_mac_address(hardware_addr);

        devices.borrow_mut().push(loopback);
        devices.borrow_mut().push(eth0);

        Ok(Smolnetd {
            iface: Rc::clone(&iface),
            router_device: network_device,
            socket_set: Rc::clone(&socket_set),
            timer: ::std::time::Instant::now(),
            time_file: unsafe { File::from_raw_fd(time_file.into_raw() as RawFd) },
            ip_scheme: IpScheme::new(
                "ip",
                Rc::clone(&iface),
                Rc::clone(&route_table),
                Rc::clone(&socket_set),
                ip_file,
            )?,
            udp_scheme: UdpScheme::new(
                "udp",
                Rc::clone(&iface),
                Rc::clone(&route_table),
                Rc::clone(&socket_set),
                udp_file,
            )?,
            tcp_scheme: TcpScheme::new(
                "tcp",
                Rc::clone(&iface),
                Rc::clone(&route_table),
                Rc::clone(&socket_set),
                tcp_file,
            )?,
            icmp_scheme: IcmpScheme::new(
                "icmp",
                Rc::clone(&iface),
                Rc::clone(&route_table),
                Rc::clone(&socket_set),
                icmp_file,
            )?,
            netcfg_scheme: NetCfgScheme::new(
                Rc::clone(&iface),
                netcfg_file,
                Rc::clone(&route_table),
                Rc::clone(&devices),
            )?,
        })
    }

    pub fn on_network_scheme_event(&mut self) -> Result<()> {
        self.poll()?;
        Ok(())
    }

    pub fn on_ip_scheme_event(&mut self) -> Result<()> {
        self.ip_scheme.on_scheme_event()?;
        let _ = self.poll()?;
        Ok(())
    }

    pub fn on_udp_scheme_event(&mut self) -> Result<()> {
        self.udp_scheme.on_scheme_event()?;
        let _ = self.poll()?;
        Ok(())
    }

    pub fn on_tcp_scheme_event(&mut self) -> Result<()> {
        self.tcp_scheme.on_scheme_event()?;
        let _ = self.poll()?;
        Ok(())
    }

    pub fn on_icmp_scheme_event(&mut self) -> Result<()> {
        self.icmp_scheme.on_scheme_event()?;
        let _ = self.poll()?;
        Ok(())
    }

    pub fn on_time_event(&mut self) -> Result<()> {
        let timeout = self.poll()?;
        self.schedule_time_event(timeout)?;
        //TODO: Fix network scheme to ensure events are not missed
        self.on_network_scheme_event()
    }

    pub fn on_netcfg_scheme_event(&mut self) -> Result<()> {
        self.netcfg_scheme.on_scheme_event()?;
        Ok(())
    }

    fn schedule_time_event(&mut self, timeout: Duration) -> Result<()> {
        let mut time = TimeSpec::default();
        if self.time_file.read(&mut time)? < size_of::<TimeSpec>() {
            return Err(Error::from_syscall_error(
                syscall::Error::new(syscall::EBADF),
                "Can't read current time",
            ));
        }
        let mut time_ms = time.tv_sec * 1000i64 + i64::from(time.tv_nsec) / 1_000_000i64;
        time_ms += timeout.total_millis() as i64;
        time.tv_sec = time_ms / 1000;
        time.tv_nsec = ((time_ms % 1000) * 1_000_000) as i32;
        self.time_file
            .write_all(&time)
            .map_err(|e| Error::from_io_error(e, "Failed to write to time file"))?;
        Ok(())
    }

    fn poll(&mut self) -> Result<Duration> {
        let timeout = {
            let mut iter_limit = 10usize;
            let mut iface = self.iface.borrow_mut();
            let mut socket_set = self.socket_set.borrow_mut();

            loop {
                let timestamp = Instant::from(self.timer);
                if iter_limit == 0 {
                    break MIN_DURATION;
                }
                iter_limit -= 1;

                self.router_device.get_mut().poll(timestamp);

                // TODO: Check what if the bool returned by poll can be useful
                iface.poll(timestamp, &mut self.router_device, &mut socket_set);

                self.router_device.get_mut().dispatch(timestamp);

                if !self.router_device.get_ref().can_recv() {
                    match iface.poll_delay(timestamp, &socket_set) {
                        Some(delay) if delay == Duration::ZERO => {}
                        Some(delay) => break ::std::cmp::min(MAX_DURATION, delay),
                        None => break MAX_DURATION,
                    };
                }
            }
        };

        self.notify_sockets()?;

        Ok(::std::cmp::min(
            ::std::cmp::max(Smolnetd::MIN_CHECK_TIMEOUT, timeout),
            Smolnetd::MAX_CHECK_TIMEOUT,
        ))
    }

    fn notify_sockets(&mut self) -> Result<()> {
        self.ip_scheme.notify_sockets()?;
        self.udp_scheme.notify_sockets()?;
        self.tcp_scheme.notify_sockets()?;
        self.icmp_scheme.notify_sockets()
    }
}

fn post_fevent(socket: &Socket, id: usize, flags: usize) -> syscall::error::Result<()> {
    let fevent_response = Response::post_fevent(id, flags);
    match socket.write_response(fevent_response, SignalBehavior::Restart) {
        Ok(true) => Ok(()), // Write response success
        Ok(false) => Err(syscall::error::Error::new(syscall::EAGAIN)), // Write response failed, retry.
        Err(err) => Err(err),                                          // Error writing response
    }
}

fn parse_endpoint(socket: &str) -> IpListenEndpoint {
    let mut socket_parts = socket.split(':');
    let host = Ipv4Address::from_str(socket_parts.next().unwrap_or(""))
        .ok()
        .filter(|addr| !addr.is_unspecified())
        .map(IpAddress::Ipv4);

    let port = socket_parts
        .next()
        .unwrap_or("")
        .parse::<u16>()
        .unwrap_or(0);
    IpListenEndpoint { addr: host, port }
}

struct WaitHandle {
    until: Option<TimeSpec>,
    cancelling: bool,
    packet: (Op, CallerCtx),
}

type WaitQueue = Vec<WaitHandle>;

pub struct SchemeWrapper<SocketT>
where
    SocketT: SchemeSocket + AnySocket<'static>,
{
    scheme: socket::SocketScheme<SocketT>,
    state: SchemeState,
    wait_queue: WaitQueue,
}
impl<SocketT> SchemeWrapper<SocketT>
where
    SocketT: SchemeSocket + AnySocket<'static>,
{
    pub fn new(
        name: &str,
        iface: Interface,
        route_table: Rc<RefCell<RouteTable>>,
        socket_set: Rc<RefCell<SocketSet>>,
        scheme_file: Socket,
    ) -> Result<Self> {
        Ok(Self {
            scheme: SocketScheme::<SocketT>::new(name, iface, route_table, socket_set, scheme_file)
                .map_err(|e| {
                    Error::from_syscall_error(e, &format!("failed to initialize {} scheme", name))
                })?,
            state: SchemeState::new(),
            wait_queue: Vec::new(),
        })
    }
    pub fn on_scheme_event(&mut self) -> Result<Option<()>> {
        let result = loop {
            let request = match self
                .scheme
                .scheme_file
                .next_request(SignalBehavior::Restart)
            {
                Ok(Some(req)) => req,
                Ok(None) => {
                    break Some(());
                }
                Err(error)
                    if error.errno == syscall::EWOULDBLOCK || error.errno == syscall::EAGAIN =>
                {
                    break None;
                }
                Err(other) => {
                    return Err(Error::from_syscall_error(
                        other,
                        "failed to receive new request",
                    ))
                }
            };

            let req = match request.kind() {
                RequestKind::Call(c) => c,
                RequestKind::OnClose { id } => {
                    self.scheme.on_close(id);
                    continue;
                }
                RequestKind::Cancellation(req) => {
                    if let Some(idx) = self
                        .wait_queue
                        .iter()
                        .position(|q| q.packet.0.req_id() == req.id)
                    {
                        self.wait_queue[idx].cancelling = true;
                    }
                    continue;
                }
                _ => {
                    continue;
                }
            };
            let caller = req.caller();
            let mut op = match req.op() {
                Ok(op) => op,
                Err(req) => {
                    self.scheme
                        .scheme_file
                        .write_response(
                            Response::err(syscall::EOPNOTSUPP, req),
                            SignalBehavior::Restart,
                        )
                        .map_err(|e| {
                            Error::from_syscall_error(e.into(), "failed to write response")
                        })?;
                    continue;
                }
            };
            let resp = match op.handle_sync_dont_consume(&caller, &mut self.scheme, &mut self.state)
            {
                SchemeResponse::Opened(Err(SyscallError {
                    errno: syscall::EWOULDBLOCK,
                }))
                | SchemeResponse::Regular(Err(SyscallError {
                    errno: syscall::EWOULDBLOCK,
                })) if !op.is_explicitly_nonblock() => {
                    match self.scheme.handle_block(&op) {
                        Ok(timeout) => {
                            self.wait_queue.push(WaitHandle {
                                until: timeout,
                                cancelling: false,
                                packet: (op, caller),
                            });
                        }
                        Err(err) => {
                            let _ = self
                                .scheme
                                .scheme_file
                                .write_response(
                                    Response::err(err.errno, op),
                                    SignalBehavior::Restart,
                                )
                                .map_err(|e| {
                                    Error::from_syscall_error(e.into(), "failed to write response")
                                })?;
                            return Err(Error::from_syscall_error(
                                err,
                                "Can't handle blocked socket",
                            ));
                        }
                    }
                    continue;
                }
                SchemeResponse::Regular(r) => Response::new(r, op),
                SchemeResponse::Opened(o) => Response::open_dup_like(o, op),
                SchemeResponse::RegularAndNotifyOnDetach(status) => {
                    Response::new_notify_on_detach(status, op)
                }
            };
            let _ = self
                .scheme
                .scheme_file
                .write_response(resp, SignalBehavior::Restart)
                .map_err(|e| Error::from_syscall_error(e.into(), "failed to write response"))?;
        };
        Ok(result)
    }

    pub fn notify_sockets(&mut self) -> Result<()> {
        let cur_time = libredox::call::clock_gettime(flag::CLOCK_MONOTONIC)
            .map_err(|e| Error::from_syscall_error(e.into(), "Can't get time"))?;

        // Notify non-blocking sockets
        let scheme = &mut self.scheme;
        let state = &mut self.state;

        for (&fd, handle) in scheme.handles.iter_mut() {
            let Handle::File(file) = handle else {
                continue;
            };
            let events = {
                let mut socket_set = scheme.socket_set.borrow_mut();
                file.events(&mut socket_set)
            };
            if events > 0 {
                post_fevent(&scheme.scheme_file, fd, events)
                    .map_err(|e| Error::from_syscall_error(e.into(), "failed to post fevent"))?;
            }
        }
        // Wake up blocking queue
        let queue = &mut self.wait_queue;
        let mut i = 0;
        while i < queue.len() {
            let handle = &mut queue[i];
            let (op, caller) = &mut handle.packet;
            let res = op.handle_sync_dont_consume(caller, scheme, state);

            match res {
                SchemeResponse::Opened(Err(SyscallError {
                    errno: syscall::EWOULDBLOCK,
                }))
                | SchemeResponse::Regular(Err(SyscallError {
                    errno: syscall::EWOULDBLOCK,
                })) if !op.is_explicitly_nonblock() => {
                    if handle.cancelling {
                        let (op, _) = queue.swap_remove(i).packet;
                        scheme
                            .scheme_file
                            .write_response(
                                Response::err(syscall::ECANCELED, op),
                                SignalBehavior::Restart,
                            )
                            .map_err(|e| {
                                Error::from_syscall_error(e.into(), "failed to write response")
                            })?;
                        continue;
                    }
                    match handle.until {
                        Some(until)
                            if (until.tv_sec < cur_time.tv_sec
                                || (until.tv_sec == cur_time.tv_sec
                                    && i64::from(until.tv_nsec) < i64::from(cur_time.tv_nsec))) =>
                        {
                            let (op, _) = queue.swap_remove(i).packet;
                            let _ = scheme
                                .scheme_file
                                .write_response(
                                    Response::err(syscall::ETIMEDOUT, op),
                                    SignalBehavior::Restart,
                                )
                                .map_err(|e| {
                                    Error::from_syscall_error(e.into(), "failed to write response")
                                })?;
                        }
                        _ => {
                            i += 1;
                        }
                    }
                }
                SchemeResponse::Regular(r) => {
                    let (op, _) = queue.swap_remove(i).packet;
                    let _ = scheme
                        .scheme_file
                        .write_response(Response::new(r, op), SignalBehavior::Restart)
                        .map_err(|e| {
                            Error::from_syscall_error(e.into(), "failed to write response")
                        })?;
                }
                SchemeResponse::Opened(o) => {
                    let (op, _) = queue.swap_remove(i).packet;
                    let _ = scheme
                        .scheme_file
                        .write_response(Response::open_dup_like(o, op), SignalBehavior::Restart)
                        .map_err(|e| {
                            Error::from_syscall_error(e.into(), "failed to write response")
                        })?;
                }
                SchemeResponse::RegularAndNotifyOnDetach(status) => {
                    let (op, _) = queue.swap_remove(i).packet;
                    let _ = scheme
                        .scheme_file
                        .write_response(
                            Response::new_notify_on_detach(status, op),
                            SignalBehavior::Restart,
                        )
                        .map_err(|e| {
                            Error::from_syscall_error(e.into(), "failed to write response")
                        })?;
                }
            }
        }

        Ok(())
    }
}
