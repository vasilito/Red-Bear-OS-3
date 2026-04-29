use scheme_utils::FpathWriter;
use smoltcp::iface::SocketHandle;
use smoltcp::socket::udp::{
    PacketBuffer as UdpSocketBuffer, PacketMetadata as UdpPacketMetadata, Socket as UdpSocket,
};
use smoltcp::wire::{IpEndpoint, IpListenEndpoint};
use std::str;
use syscall;
use syscall::{Error as SyscallError, Result as SyscallResult};

use super::socket::{Context, DupResult, SchemeFile, SchemeSocket, SocketFile};
use super::{parse_endpoint, SchemeWrapper, Smolnetd, SocketSet};
use crate::port_set::PortSet;
use crate::router::Router;
use libredox::flag;

const SO_SNDBUF: usize = 7;
const SO_RCVBUF: usize = 8;

pub type UdpScheme = SchemeWrapper<UdpSocket<'static>>;

impl<'a> SchemeSocket for UdpSocket<'a> {
    type SchemeDataT = PortSet;
    type DataT = IpListenEndpoint;
    type SettingT = ();

    fn new_scheme_data() -> Self::SchemeDataT {
        PortSet::new(49_152u16, 65_535u16).expect("Wrong UDP port numbers")
    }

    fn can_send(&self) -> bool {
        self.can_send()
    }

    fn can_recv(&mut self, data: &IpListenEndpoint) -> bool {
        loop {
            // If buffer is empty, we definitely can't recv
            if !UdpSocket::can_recv(self) {
                return false;
            }

            // If we are not connected to a specific remote, any packet is valid
            if !data.is_specified() {
                return true;
            }

            // If we are connected, peek at the packet.
            match self.peek() {
                Ok((_, meta)) => {
                    let source = meta.endpoint;
                    let connected_addr = data.addr.unwrap(); // Safe because is_specified() checked it

                    // Allow Broadcast special case (DHCP)
                    let is_broadcast = match connected_addr {
                        smoltcp::wire::IpAddress::Ipv4(ip) => {
                            ip == smoltcp::wire::Ipv4Address::BROADCAST
                        }
                        _ => false,
                    };

                    if !is_broadcast && !connected_addr.is_unspecified() {
                        if source.addr != connected_addr || source.port != data.port {
                            // Bad packet detected
                            // Remove it from the buffer immediately so poll() doesn't trigger
                            let _ = self.recv();
                            continue; // Loop again to check the next packet
                        }
                    }
                    // Packet is valid
                    return true;
                }
                Err(_) => return false,
            }
        }
    }

    fn may_recv(&self) -> bool {
        true
    }

    fn hop_limit(&self) -> u8 {
        self.hop_limit().unwrap_or(64)
    }

    fn set_hop_limit(&mut self, hop_limit: u8) {
        self.set_hop_limit(Some(hop_limit));
    }

    fn get_setting(
        _file: &SocketFile<Self::DataT>,
        _setting: Self::SettingT,
        _buf: &mut [u8],
    ) -> SyscallResult<usize> {
        Ok(0)
    }

    fn set_setting(
        _file: &mut SocketFile<Self::DataT>,
        _setting: Self::SettingT,
        _buf: &[u8],
    ) -> SyscallResult<usize> {
        Ok(0)
    }

    fn new_socket(
        socket_set: &mut SocketSet,
        path: &str,
        uid: u32,
        port_set: &mut Self::SchemeDataT,
        context: &Context,
    ) -> SyscallResult<(SocketHandle, Self::DataT)> {
        let mut parts = path.split('/');
        let remote_endpoint = parse_endpoint(parts.next().unwrap_or(""));
        let mut local_endpoint = parse_endpoint(parts.next().unwrap_or(""));

        if local_endpoint.port > 0 && local_endpoint.port <= 1024 && uid != 0 {
            return Err(SyscallError::new(syscall::EACCES));
        }

        let rx_buffer = UdpSocketBuffer::new(
            vec![UdpPacketMetadata::EMPTY; Smolnetd::SOCKET_BUFFER_SIZE],
            vec![0; Router::MTU * Smolnetd::SOCKET_BUFFER_SIZE],
        );
        let tx_buffer = UdpSocketBuffer::new(
            vec![UdpPacketMetadata::EMPTY; Smolnetd::SOCKET_BUFFER_SIZE],
            vec![0; Router::MTU * Smolnetd::SOCKET_BUFFER_SIZE],
        );
        let udp_socket = UdpSocket::new(rx_buffer, tx_buffer);

        // TODO: claim port with ethernet ip address
        if local_endpoint.port == 0 {
            local_endpoint.port = port_set
                .get_port()
                .ok_or_else(|| SyscallError::new(syscall::EINVAL))?;
        } else if !port_set.claim_port(local_endpoint.port) {
            return Err(SyscallError::new(syscall::EADDRINUSE));
        }

        let socket_handle = socket_set.add(udp_socket);

        let udp_socket = socket_set.get_mut::<UdpSocket>(socket_handle);

        if remote_endpoint.is_specified() {
            let local_endpoint_addr = match local_endpoint.addr {
                Some(addr) if addr.is_unspecified() => Some(addr),
                _ => {
                    // local ip is 0.0.0.0, resolve it
                    let route_table = context.route_table.borrow();
                    let addr = route_table
                        .lookup_src_addr(&remote_endpoint.addr.expect("Checked in is_specified"));
                    if matches!(addr, None) {
                        error!("Opening a TCP connection with a probably invalid source IP as no route have been found for destination: {}", remote_endpoint);
                    }
                    addr
                }
            };
            local_endpoint = IpListenEndpoint {
                addr: local_endpoint_addr,
                port: local_endpoint.port,
            };
        }

        udp_socket
            .bind(local_endpoint)
            .expect("Can't bind udp socket to local endpoint");

        Ok((socket_handle, remote_endpoint))
    }

    fn close_file(
        &self,
        file: &SchemeFile<Self>,
        port_set: &mut Self::SchemeDataT,
    ) -> SyscallResult<()> {
        if let SchemeFile::Socket(_) = *file {
            port_set.release_port(self.endpoint().port);
        }
        Ok(())
    }

    fn write_buf(
        &mut self,
        file: &mut SocketFile<Self::DataT>,
        buf: &[u8],
    ) -> SyscallResult<usize> {
        if !file.data.is_specified() {
            return Err(SyscallError::new(syscall::EADDRNOTAVAIL));
        }
        if !file.write_enabled {
            return Err(SyscallError::new(syscall::EPIPE));
        }
        if self.can_send() {
            let endpoint = file.data;
            let endpoint = IpEndpoint::new(
                endpoint
                    .addr
                    .expect("If we can send, this should be specified"),
                endpoint.port,
            );
            self.send_slice(buf, endpoint).expect("Can't send slice");
            Ok(buf.len())
        } else if file.flags & syscall::O_NONBLOCK == syscall::O_NONBLOCK {
            Err(SyscallError::new(syscall::EAGAIN))
        } else {
            Err(SyscallError::new(syscall::EWOULDBLOCK)) // internally scheduled to re-read
        }
    }

    fn read_buf(
        &mut self,
        file: &mut SocketFile<Self::DataT>,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        if !file.read_enabled {
            Ok(0)
        } else if self.can_recv(&file.data) {
            let (length, _) = self.recv_slice(buf).expect("Can't receive slice");
            Ok(length)
        } else if file.flags & syscall::O_NONBLOCK == syscall::O_NONBLOCK {
            Err(SyscallError::new(syscall::EAGAIN))
        } else {
            Err(SyscallError::new(syscall::EWOULDBLOCK)) // internally scheduled to re-read
        }
    }

    fn dup(
        socket_set: &mut SocketSet,
        file: &mut SchemeFile<Self>,
        path: &str,
        port_set: &mut Self::SchemeDataT,
    ) -> SyscallResult<DupResult<Self>> {
        let socket_handle = file.socket_handle();
        let file = match path {
            "listen" => {
                // there's no accept() for UDP
                return Err(SyscallError::new(syscall::EAFNOSUPPORT));
            }
            "disconnect" => {
                let remote_endpoint = IpListenEndpoint {
                    addr: None,
                    port: 0,
                };
                if let SchemeFile::Socket(ref udp_handle) = *file {
                    SchemeFile::Socket(udp_handle.clone_with_data(remote_endpoint))
                } else {
                    SchemeFile::Socket(SocketFile::new_with_data(socket_handle, remote_endpoint))
                }
            }
            _ => {
                let remote_endpoint = parse_endpoint(path);
                if let SchemeFile::Socket(ref udp_handle) = *file {
                    SchemeFile::Socket(udp_handle.clone_with_data(remote_endpoint))
                } else {
                    SchemeFile::Socket(SocketFile::new_with_data(socket_handle, remote_endpoint))
                }
            }
        };

        let endpoint = {
            let socket = socket_set.get::<UdpSocket>(socket_handle);
            socket.endpoint()
        };

        if let SchemeFile::Socket(_) = file {
            port_set.acquire_port(endpoint.port);
        }

        Ok(Some((file, None)))
    }

    fn fpath(&self, file: &SchemeFile<Self>, buf: &mut [u8]) -> SyscallResult<usize> {
        FpathWriter::with(buf, "udp", |w| {
            let unspecified = "0.0.0.0:0";

            // remote
            match file {
                SchemeFile::Socket(SocketFile { data: endpoint, .. }) => {
                    if endpoint.is_specified() {
                        write!(w, "{}", endpoint).unwrap()
                    } else {
                        write!(w, "0.0.0.0:{}", endpoint.port).unwrap()
                    }
                }
                _ => w.push_str(unspecified),
            }
            w.push_str("/");
            // local
            let endpoint = self.endpoint();
            if endpoint.is_specified() {
                write!(w, "{}", endpoint).unwrap()
            } else {
                write!(w, "0.0.0.0:{}", endpoint.port).unwrap()
            }

            Ok(())
        })
    }

    fn handle_recvmsg(
        &mut self,
        file: &mut SchemeFile<Self>,
        how: &mut [u8],
        flags: usize,
    ) -> SyscallResult<usize> {
        //there is a separate flags argument for MSG_DONTWAIT which is call specific not socket-wide like socket_file.flags
        let socket_file = match file {
            SchemeFile::Socket(ref mut sock_f) => sock_f,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };

        if !socket_file.read_enabled {
            Ok(0)
        } else if self.can_recv(&socket_file.data) {
            let usize_length = core::mem::size_of::<usize>();
            let prepared_name_len = usize::from_le_bytes(
                how[0..usize_length]
                    .try_into()
                    .map_err(|_| SyscallError::new(syscall::EINVAL))?,
            );
            let prepared_whole_iov_size = usize::from_le_bytes(
                how[usize_length..2 * usize_length]
                    .try_into()
                    .map_err(|_| SyscallError::new(syscall::EINVAL))?,
            );
            let prepared_msg_controllen = usize::from_le_bytes(
                how[2 * usize_length..3 * usize_length]
                    .try_into()
                    .map_err(|_| SyscallError::new(syscall::EINVAL))?,
            );
            if 3 * usize_length
                + prepared_name_len
                + prepared_msg_controllen
                + prepared_whole_iov_size
                > how.len()
            {
                //expected returned buffer size is larger than provided -> return invalid
                return Err(SyscallError::new(syscall::EINVAL));
            }

            //the relibc deserialization functions expect NO GAPS between the name and payload slices
            //so the payload must be temporarily stored during recv_slice
            let mut payload_tmp = vec![0u8; prepared_whole_iov_size];
            let (length, address) = self
                .recv_slice(&mut payload_tmp)
                .expect("Can't recieve slice");

            //Address Handling
            let address_formatted = if prepared_name_len > 0 {
                format!(
                    "/scheme/udp/{}:{}",
                    address.endpoint.addr, address.endpoint.port
                )
            } else {
                String::from("")
            };
            how[..usize_length].copy_from_slice(&address_formatted.len().to_le_bytes());
            let payload_len_index = address_formatted.len() + usize_length;
            how[usize_length..payload_len_index].copy_from_slice(&address_formatted.as_bytes());

            //Payload Handling
            how[payload_len_index..payload_len_index + usize_length]
                .copy_from_slice(&(length as usize).to_le_bytes());
            how[payload_len_index + usize_length..payload_len_index + usize_length + length]
                .copy_from_slice(&payload_tmp[..length]);
            Ok(payload_len_index + usize_length + length)
        } else if socket_file.flags & syscall::O_NONBLOCK == syscall::O_NONBLOCK
            || flags & flag::MSG_DONTWAIT as usize != 0
        {
            Err(SyscallError::new(syscall::EAGAIN))
        } else {
            Err(SyscallError::new(syscall::EWOULDBLOCK)) // internally scheduled to re-read
        }
    }

    fn handle_get_peer_name(
        &self,
        file: &SchemeFile<Self>,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        let peer = match file {
            SchemeFile::Socket(SocketFile { data, .. }) => data,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };
        if peer.addr.is_some() || peer.port != 0 {
            self.fpath(file, buf)
        } else {
            Err(SyscallError::new(syscall::ENOTCONN))
        }
    }

    fn handle_shutdown(&mut self, file: &mut SchemeFile<Self>, how: usize) -> SyscallResult<usize> {
        let socket_file = match file {
            SchemeFile::Socket(ref mut file) => file,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };

        match how {
            0 => socket_file.read_enabled = false,  // SHUT_RD
            1 => socket_file.write_enabled = false, // SHUT_WR
            2 => {
                socket_file.read_enabled = false;
                socket_file.write_enabled = false;
            } // SHUT_RDWR
            _ => return Err(SyscallError::new(syscall::EINVAL)),
        }
        Ok(0)
    }

    fn get_sock_opt(
        &self,
        _file: &SchemeFile<Self>,
        name: usize,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        match name {
            SO_RCVBUF => {
                let val = self.payload_recv_capacity() as i32;
                let bytes = val.to_ne_bytes();
                if buf.len() < bytes.len() {
                    return Err(SyscallError::new(syscall::EINVAL));
                }
                buf[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
            SO_SNDBUF => {
                let val = self.payload_send_capacity() as i32;
                let bytes = val.to_ne_bytes();
                if buf.len() < bytes.len() {
                    return Err(SyscallError::new(syscall::EINVAL));
                }
                buf[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
            _ => Err(SyscallError::new(syscall::ENOPROTOOPT)),
        }
    }
}
