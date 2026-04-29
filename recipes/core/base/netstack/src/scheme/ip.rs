use scheme_utils::FpathWriter;
use smoltcp::iface::SocketHandle;
use smoltcp::socket::raw::{
    PacketBuffer as RawSocketBuffer, PacketMetadata as RawPacketMetadata, Socket as RawSocket,
};
use smoltcp::wire::{IpProtocol, IpVersion};
use std::str;
use syscall;
use syscall::{Error as SyscallError, Result as SyscallResult};

use crate::router::Router;

use super::socket::{Context, DupResult, SchemeFile, SchemeSocket, SocketFile};
use super::{SchemeWrapper, Smolnetd, SocketSet};

pub type IpScheme = SchemeWrapper<RawSocket<'static>>;

impl<'a> SchemeSocket for RawSocket<'a> {
    type SchemeDataT = ();
    type DataT = ();
    type SettingT = ();

    fn new_scheme_data() -> Self::SchemeDataT {
        ()
    }

    fn can_send(&self) -> bool {
        self.can_send()
    }

    fn can_recv(&mut self, _data: &Self::DataT) -> bool {
        smoltcp::socket::raw::Socket::can_recv(self)
    }

    fn may_recv(&self) -> bool {
        true
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

    fn hop_limit(&self) -> u8 {
        0
    }

    fn set_hop_limit(&mut self, _hop_limit: u8) {}

    fn new_socket(
        socket_set: &mut SocketSet,
        path: &str,
        uid: u32,
        _: &mut Self::SchemeDataT,
        _context: &Context,
    ) -> SyscallResult<(SocketHandle, Self::DataT)> {
        if uid != 0 {
            return Err(SyscallError::new(syscall::EACCES));
        }
        let proto =
            u8::from_str_radix(path, 16).or_else(|_| Err(SyscallError::new(syscall::ENOENT)))?;

        let rx_buffer = RawSocketBuffer::new(
            vec![RawPacketMetadata::EMPTY; Smolnetd::SOCKET_BUFFER_SIZE],
            vec![0; Router::MTU * Smolnetd::SOCKET_BUFFER_SIZE],
        );
        let tx_buffer = RawSocketBuffer::new(
            vec![RawPacketMetadata::EMPTY; Smolnetd::SOCKET_BUFFER_SIZE],
            vec![0; Router::MTU * Smolnetd::SOCKET_BUFFER_SIZE],
        );
        let ip_socket = RawSocket::new(
            IpVersion::Ipv4,
            IpProtocol::from(proto),
            rx_buffer,
            tx_buffer,
        );

        let socket_handle = socket_set.add(ip_socket);
        Ok((socket_handle, ()))
    }

    fn close_file(&self, _: &SchemeFile<Self>, _: &mut Self::SchemeDataT) -> SyscallResult<()> {
        Ok(())
    }

    fn write_buf(
        &mut self,
        file: &mut SocketFile<Self::DataT>,
        buf: &[u8],
    ) -> SyscallResult<usize> {
        if !file.write_enabled {
            return Err(SyscallError::new(syscall::EPIPE));
        } else if self.can_send() {
            self.send_slice(buf).expect("Can't send slice");
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
            let length = self.recv_slice(buf).expect("Can't receive slice");
            Ok(length)
        } else if file.flags & syscall::O_NONBLOCK == syscall::O_NONBLOCK {
            Err(SyscallError::new(syscall::EAGAIN))
        } else {
            Err(SyscallError::new(syscall::EWOULDBLOCK)) // internally scheduled to re-read
        }
    }

    fn dup(
        _socket_set: &mut SocketSet,
        _file: &mut SchemeFile<Self>,
        _path: &str,
        _: &mut Self::SchemeDataT,
    ) -> SyscallResult<DupResult<Self>> {
        Err(SyscallError::new(syscall::EBADF))
    }

    fn fpath(&self, _file: &SchemeFile<Self>, buf: &mut [u8]) -> SyscallResult<usize> {
        FpathWriter::with(buf, "ip", |w| {
            write!(w, "{}", self.ip_protocol()).unwrap();
            Ok(())
        })
    }

    fn handle_recvmsg(
        &mut self,
        file: &mut SchemeFile<Self>,
        how: &mut [u8],
        flags: usize,
    ) -> SyscallResult<usize> {
        return Err(SyscallError::new(syscall::EOPNOTSUPP));
    }

    fn handle_get_peer_name(
        &self,
        file: &SchemeFile<Self>,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        self.fpath(file, buf)
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
}
