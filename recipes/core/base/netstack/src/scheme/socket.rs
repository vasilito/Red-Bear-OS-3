use std::cell::RefCell;
use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ops::DerefMut;
use std::rc::Rc;
use std::str;

use libredox::flag::CLOCK_MONOTONIC;
use libredox::protocol::SocketCall;
use redox_scheme::{
    scheme::{register_scheme_inner, Op, SchemeSync},
    CallerCtx, OpenResult, Socket,
};
use scheme_utils::HandleMap;
use syscall::data::TimeSpec;
use syscall::flag::{EVENT_READ, EVENT_WRITE};
use syscall::schemev2::NewFdFlags;
use syscall::{
    Error as SyscallError, EventFlags as SyscallEventFlags, Result as SyscallResult, EINVAL,
    EOPNOTSUPP,
};

use super::Interface;
use crate::router::route_table::RouteTable;
use crate::scheme::smoltcp::iface::SocketHandle;
use crate::scheme::Router;
use crate::Smolnetd;
use smoltcp::socket::AnySocket;

use super::SocketSet;

const SO_RCVBUF: usize = 8;
const SO_SNDBUF: usize = 7;

pub struct Context {
    pub iface: Interface,
    pub route_table: Rc<RefCell<RouteTable>>,
}

pub struct NullFile {
    pub flags: usize,
    pub uid: u32,
    pub gid: u32,
    pub read_enabled: bool,
    pub write_enabled: bool,
}

pub struct SocketFile<DataT> {
    pub flags: usize,
    pub data: DataT,

    events: usize,
    socket_handle: SocketHandle,
    read_notified: bool,
    write_notified: bool,
    read_timeout: Option<TimeSpec>,
    write_timeout: Option<TimeSpec>,
    pub read_enabled: bool,
    pub write_enabled: bool,
}

impl<DataT> SocketFile<DataT> {
    pub fn clone_with_data(&self, data: DataT) -> SocketFile<DataT> {
        SocketFile {
            flags: self.flags,
            events: self.events,
            read_notified: false, // we still want to notify about this new socket
            write_notified: false,
            read_timeout: self.read_timeout,
            write_timeout: self.write_timeout,
            socket_handle: self.socket_handle,
            read_enabled: self.read_enabled,
            write_enabled: self.write_enabled,
            data,
        }
    }

    pub fn new_with_data(socket_handle: SocketHandle, data: DataT) -> SocketFile<DataT> {
        SocketFile {
            flags: 0,
            events: 0,
            read_notified: false,
            write_notified: false,
            read_timeout: None,
            write_timeout: None,
            read_enabled: true,
            write_enabled: true,
            socket_handle,
            data,
        }
    }
}

#[derive(Copy, Clone)]
enum Setting<SettingT: Copy> {
    HopLimit,
    ReadTimeout,
    WriteTimeout,
    #[allow(dead_code)]
    Other(SettingT),
}

pub struct SettingFile<SettingT: Copy> {
    fd: usize,
    socket_handle: SocketHandle,
    setting: Setting<SettingT>,
}

pub enum SchemeFile<SocketT>
where
    SocketT: SchemeSocket,
{
    Setting(SettingFile<SocketT::SettingT>),
    Socket(SocketFile<SocketT::DataT>),
}

impl<SocketT> SchemeFile<SocketT>
where
    SocketT: SchemeSocket,
{
    pub fn socket_handle(&self) -> SocketHandle {
        match *self {
            SchemeFile::Socket(SocketFile { socket_handle, .. })
            | SchemeFile::Setting(SettingFile { socket_handle, .. }) => socket_handle,
        }
    }

    pub fn events(&mut self, socket_set: &mut SocketSet) -> usize
    where
        SocketT: AnySocket<'static>,
    {
        let mut revents = 0;
        if let &mut SchemeFile::Socket(SocketFile {
            socket_handle,
            events,
            ref mut read_notified,
            ref mut write_notified,
            ref data,
            ..
        }) = self
        {
            let socket = socket_set.get_mut::<SocketT>(socket_handle);

            if events & syscall::EVENT_READ.bits() == syscall::EVENT_READ.bits()
                && (socket.can_recv(data) || !socket.may_recv())
            {
                if !*read_notified {
                    *read_notified = true;
                    revents |= EVENT_READ.bits();
                }
            } else {
                *read_notified = false;
            }

            if events & syscall::EVENT_WRITE.bits() == syscall::EVENT_WRITE.bits()
                && socket.can_send()
            {
                if !*write_notified {
                    *write_notified = true;
                    revents |= EVENT_WRITE.bits();
                }
            } else {
                *write_notified = false;
            }
        }
        revents
    }
}

pub type DupResult<T> = Option<(
    SchemeFile<T>,
    Option<(SocketHandle, <T as SchemeSocket>::DataT)>,
)>;

pub trait SchemeSocket
where
    Self: ::std::marker::Sized,
{
    type SchemeDataT;
    type DataT;
    type SettingT: Copy;

    fn new_scheme_data() -> Self::SchemeDataT;

    fn can_send(&self) -> bool;
    fn can_recv(&mut self, data: &Self::DataT) -> bool;
    fn may_recv(&self) -> bool;

    fn hop_limit(&self) -> u8;
    fn set_hop_limit(&mut self, hop_limit: u8);

    fn get_setting(
        file: &SocketFile<Self::DataT>,
        setting: Self::SettingT,
        buf: &mut [u8],
    ) -> SyscallResult<usize>;
    fn set_setting(
        file: &mut SocketFile<Self::DataT>,
        setting: Self::SettingT,
        buf: &[u8],
    ) -> SyscallResult<usize>;

    fn new_socket(
        sockets: &mut SocketSet,
        path: &str,
        uid: u32,
        data: &mut Self::SchemeDataT,
        context: &Context,
    ) -> SyscallResult<(SocketHandle, Self::DataT)>;

    fn close_file(
        &self,
        file: &SchemeFile<Self>,
        data: &mut Self::SchemeDataT,
    ) -> SyscallResult<()>;

    fn write_buf(&mut self, file: &mut SocketFile<Self::DataT>, buf: &[u8])
        -> SyscallResult<usize>;

    fn read_buf(
        &mut self,
        file: &mut SocketFile<Self::DataT>,
        buf: &mut [u8],
    ) -> SyscallResult<usize>;

    fn fpath(&self, file: &SchemeFile<Self>, buf: &mut [u8]) -> SyscallResult<usize>;

    fn dup(
        sockets: &mut SocketSet,
        file: &mut SchemeFile<Self>,
        path: &str,
        data: &mut Self::SchemeDataT,
    ) -> SyscallResult<DupResult<Self>>;

    fn handle_get_peer_name(
        &self,
        file: &SchemeFile<Self>,
        payload: &mut [u8],
    ) -> SyscallResult<usize>;

    fn handle_shutdown(&mut self, file: &mut SchemeFile<Self>, how: usize) -> SyscallResult<usize>;

    fn handle_recvmsg(
        &mut self,
        file: &mut SchemeFile<Self>,
        payload: &mut [u8],
        flags: usize,
    ) -> SyscallResult<usize>;

    fn get_sock_opt(
        &self,
        file: &SchemeFile<Self>,
        name: usize,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        // Return Err for default implementation
        Err(SyscallError::new(syscall::ENOPROTOOPT))
    }
}

pub enum Handle<SocketT>
where
    SocketT: SchemeSocket,
{
    SchemeRoot,
    Null(NullFile),
    File(SchemeFile<SocketT>),
}

pub struct SocketScheme<SocketT>
where
    SocketT: SchemeSocket + AnySocket<'static>,
{
    pub handles: HandleMap<Handle<SocketT>>,
    ref_counts: BTreeMap<SocketHandle, usize>,
    context: Context,
    pub socket_set: Rc<RefCell<SocketSet>>,
    pub scheme_file: Socket,
    scheme_data: SocketT::SchemeDataT,
    _phantom_socket: PhantomData<SocketT>,
}

impl<SocketT> SocketScheme<SocketT>
where
    SocketT: SchemeSocket + AnySocket<'static>,
{
    pub fn new(
        name: &str,
        iface: Interface,
        route_table: Rc<RefCell<RouteTable>>,
        socket_set: Rc<RefCell<SocketSet>>,
        scheme_file: Socket,
    ) -> SyscallResult<SocketScheme<SocketT>> {
        let mut scheme = SocketScheme {
            handles: HandleMap::new(),
            ref_counts: BTreeMap::new(),
            socket_set,
            scheme_data: SocketT::new_scheme_data(),
            scheme_file,
            _phantom_socket: PhantomData,
            context: Context { iface, route_table },
        };
        let cap_id = scheme.scheme_root()?;
        register_scheme_inner(&scheme.scheme_file, name, cap_id)?;
        Ok(scheme)
    }

    pub fn handle_block(&mut self, op: &Op) -> SyscallResult<Option<TimeSpec>> {
        let fd = op.file_id().expect("op is not fd based request");
        let (read_timeout, write_timeout) = {
            let handle = self.handles.get(fd)?;

            if let Handle::File(SchemeFile::Socket(ref scheme_file)) = *handle {
                Ok((scheme_file.read_timeout, scheme_file.write_timeout))
            } else {
                Err(SyscallError::new(syscall::EBADF))
            }
        }?;

        let mut timeout = match op {
            Op::Read(_) => write_timeout,
            Op::Write(_) => read_timeout,
            _ => None,
        };

        if let Some(ref mut timeout) = timeout {
            let cur_time = libredox::call::clock_gettime(CLOCK_MONOTONIC)?;
            *timeout = add_time(
                timeout,
                &TimeSpec {
                    tv_sec: cur_time.tv_sec,
                    tv_nsec: cur_time.tv_nsec as i32,
                },
            )
        }

        Ok(timeout)
    }

    fn get_setting(
        &mut self,
        fd: usize,
        setting: Setting<SocketT::SettingT>,
        buf: &mut [u8],
    ) -> SyscallResult<usize> {
        let file = self.handles.get_mut(fd)?;

        let file = match *file {
            Handle::File(SchemeFile::Socket(ref mut file)) => file,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };

        match setting {
            Setting::Other(setting) => SocketT::get_setting(file, setting, buf),
            Setting::HopLimit => {
                if let Some(hop_limit) = buf.get_mut(0) {
                    let socket_set = self.socket_set.borrow();
                    let socket = socket_set.get::<SocketT>(file.socket_handle);
                    *hop_limit = socket.hop_limit();
                    Ok(1)
                } else {
                    Err(SyscallError::new(syscall::EIO))
                }
            }
            Setting::ReadTimeout | Setting::WriteTimeout => {
                let timespec = match (setting, file.read_timeout, file.write_timeout) {
                    (Setting::ReadTimeout, Some(read_timeout), _) => read_timeout,
                    (Setting::WriteTimeout, _, Some(write_timeout)) => write_timeout,
                    _ => {
                        return Ok(0);
                    }
                };

                if buf.len() < mem::size_of::<TimeSpec>() {
                    Ok(0)
                } else {
                    let count = timespec.deref().read(buf).map_err(|err| {
                        SyscallError::new(err.raw_os_error().unwrap_or(syscall::EIO))
                    })?;
                    Ok(count)
                }
            }
        }
    }

    fn update_setting(
        &mut self,
        fd: usize,
        setting: Setting<SocketT::SettingT>,
        buf: &[u8],
    ) -> SyscallResult<usize> {
        let file = self.handles.get_mut(fd)?;
        let file = match *file {
            Handle::File(SchemeFile::Socket(ref mut file)) => file,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };
        match setting {
            Setting::ReadTimeout | Setting::WriteTimeout => {
                let (timeout, count) = {
                    if buf.len() < mem::size_of::<TimeSpec>() {
                        (None, 0)
                    } else {
                        let mut timespec = TimeSpec::default();
                        let count = timespec.deref_mut().write(buf).map_err(|err| {
                            SyscallError::new(err.raw_os_error().unwrap_or(syscall::EIO))
                        })?;
                        (Some(timespec), count)
                    }
                };
                match setting {
                    Setting::ReadTimeout => {
                        file.read_timeout = timeout;
                    }
                    Setting::WriteTimeout => {
                        file.write_timeout = timeout;
                    }
                    _ => {}
                };
                Ok(count)
            }
            Setting::HopLimit => {
                if let Some(hop_limit) = buf.get(0) {
                    let mut socket_set = self.socket_set.borrow_mut();
                    let socket = socket_set.get_mut::<SocketT>(file.socket_handle);
                    socket.set_hop_limit(*hop_limit);
                    Ok(1)
                } else {
                    Err(SyscallError::new(syscall::EIO))
                }
            }
            Setting::Other(setting) => SocketT::set_setting(file, setting, buf),
        }
    }

    fn call_inner(
        &mut self,
        fd: usize,
        payload: &mut [u8],
        metadata: &[u64],
        ctx: &CallerCtx,
    ) -> SyscallResult<usize> {
        // metadata to Vec<u8>
        let Some(verb) = SocketCall::try_from_raw(metadata[0] as usize) else {
            warn!("Invalid verb in metadata: {:?}", metadata);
            return Err(SyscallError::new(EINVAL));
        };
        match verb {
            // TODO
            // SocketCall::Bind => self.handle_bind(id, &payload),
            // SocketCall::Connect => self.handle_connect(id, &payload),
            SocketCall::SetSockOpt => {
                // currently not used
                // self.handle_setsockopt(id, metadata[1] as i32, &payload)
                // TODO: SO_REUSEADDR from null socket
                Ok(0)
            }
            SocketCall::GetSockOpt => {
                let handle = self.handles.get_mut(fd)?;

                match *handle {
                    Handle::File(ref mut file) => {
                        let mut socket_set = self.socket_set.borrow_mut();
                        let socket = socket_set.get_mut::<SocketT>(file.socket_handle());
                        SocketT::get_sock_opt(socket, file, metadata[1] as usize, payload)
                    }
                    Handle::Null(_) => {
                        // TODO
                        // The socket exists but hasn't been bound/connected yet.
                        // We return default values for buffer sizes to satisfy apps like iperf3.
                        // Figure out maybe a better way?
                        let name = metadata[1] as usize;
                        if name == SO_RCVBUF || name == SO_SNDBUF {
                            let val: i32 = (Router::MTU * Smolnetd::SOCKET_BUFFER_SIZE) as i32;
                            let bytes = val.to_ne_bytes();

                            if payload.len() < bytes.len() {
                                return Err(SyscallError::new(syscall::EINVAL));
                            }
                            payload[..bytes.len()].copy_from_slice(&bytes);
                            Ok(bytes.len())
                        } else {
                            Err(SyscallError::new(syscall::EINVAL))
                        }
                    }
                    Handle::SchemeRoot => Err(SyscallError::new(syscall::EBADF)),
                }
            }
            // SocketCall::SendMsg => self.handle_sendmsg(id, payload, ctx),
            // SocketCall::Unbind => self.handle_unbind(id),
            // SocketCall::GetToken => self.handle_get_token(id, payload),
            SocketCall::GetPeerName => {
                let file = self.handles.get_mut(fd)?;

                let file = match *file {
                    Handle::File(ref mut f) => f,
                    _ => return Err(SyscallError::new(syscall::EBADF)),
                };
                let mut socket_set = self.socket_set.borrow_mut();
                let socket = socket_set.get_mut::<SocketT>(file.socket_handle());

                SocketT::handle_get_peer_name(socket, file, payload)
            }
            SocketCall::RecvMsg => {
                let flags = metadata[1] as usize;
                let handle = self.handles.get_mut(fd)?;

                match *handle {
                    Handle::File(ref mut file) => {
                        let mut socket_set = self.socket_set.borrow_mut();
                        let socket = socket_set.get_mut::<SocketT>(file.socket_handle());

                        SocketT::handle_recvmsg(socket, file, payload, flags)
                    }
                    Handle::Null(_) => Err(SyscallError::new(syscall::EINVAL)),
                    Handle::SchemeRoot => Err(SyscallError::new(syscall::EBADF)),
                }
            }

            SocketCall::Shutdown => {
                let how = metadata[1] as usize;

                match self.handles.get_mut(fd)? {
                    Handle::File(file) => {
                        let mut socket_set = self.socket_set.borrow_mut();
                        let socket = socket_set.get_mut::<SocketT>(file.socket_handle());

                        SocketT::handle_shutdown(socket, file, how)
                    }
                    Handle::Null(null_file) => {
                        match how {
                            0 => null_file.read_enabled = false,
                            1 => null_file.write_enabled = false,
                            2 => {
                                null_file.read_enabled = false;
                                null_file.write_enabled = false;
                            }
                            _ => return Err(SyscallError::new(EINVAL)),
                        }
                        Ok(0)
                    }
                    Handle::SchemeRoot => Err(SyscallError::new(syscall::EBADF)),
                }
            }
            _ => Err(SyscallError::new(EOPNOTSUPP)),
        }
    }

    fn open_inner(
        &mut self,
        path: &str,
        flags: usize,
        uid: u32,
        gid: u32,
        read_enabled: bool,
        write_enabled: bool,
    ) -> SyscallResult<OpenResult> {
        if path.is_empty() {
            let null = NullFile {
                flags,
                uid,
                gid,
                read_enabled,
                write_enabled,
            };

            let id = self.handles.insert(Handle::Null(null));

            Ok(OpenResult::ThisScheme {
                number: id,
                flags: NewFdFlags::empty(),
            })
        } else {
            let (socket_handle, data) = SocketT::new_socket(
                &mut self.socket_set.borrow_mut(),
                path,
                uid,
                &mut self.scheme_data,
                &self.context,
            )?;

            let file = SchemeFile::Socket(SocketFile {
                flags,
                events: 0,
                socket_handle,
                read_notified: false,
                write_notified: false,
                write_timeout: None,
                read_timeout: None,
                read_enabled: read_enabled,
                write_enabled: write_enabled,
                data,
            });

            self.ref_counts.insert(socket_handle, 1);
            let id = self.handles.insert(Handle::File(file));

            Ok(OpenResult::ThisScheme {
                number: id,
                flags: NewFdFlags::empty(),
            })
        }
    }
}

impl<SocketT> SchemeSync for SocketScheme<SocketT>
where
    SocketT: SchemeSocket + AnySocket<'static>,
{
    fn scheme_root(&mut self) -> SyscallResult<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }

    fn openat(
        &mut self,
        fd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> SyscallResult<OpenResult> {
        match self.handles.get(fd)? {
            Handle::SchemeRoot => self.open_inner(path, flags, ctx.uid, ctx.gid, true, true),
            _ => Err(SyscallError::new(syscall::EACCES)),
        }
    }

    fn call(
        &mut self,
        id: usize,
        payload: &mut [u8],
        metadata: &[u64],
        ctx: &CallerCtx,
    ) -> SyscallResult<usize> {
        self.call_inner(id, payload, metadata, ctx)
    }

    fn on_close(&mut self, fd: usize) {
        let Some(handle) = self.handles.remove(fd) else {
            return;
        };

        // incorrect, and kernel can't send close until all references are gone
        /*self.wait_queue.retain(
            |&WaitHandle {
                 packet: SyscallPacket { a, .. },
                 ..
             }| a != fd,
        );*/

        match handle {
            Handle::SchemeRoot => return,
            Handle::Null(_) => return,
            Handle::File(scheme_file) => {
                let socket_handle = scheme_file.socket_handle();
                let mut socket_set = self.socket_set.borrow_mut();

                let socket = socket_set.get::<SocketT>(socket_handle);
                let _ = socket.close_file(&scheme_file, &mut self.scheme_data);

                let remove = match self.ref_counts.entry(socket_handle) {
                    Entry::Vacant(_) => {
                        warn!("Closing a socket_handle with no ref");
                        true
                    }
                    Entry::Occupied(mut e) => {
                        if *e.get() == 0 {
                            warn!("Closing a socket_handle with no ref");
                            e.remove();
                            true
                        } else {
                            *e.get_mut() -= 1;
                            if *e.get() == 0 {
                                e.remove();
                                true
                            } else {
                                false
                            }
                        }
                    }
                };

                if remove {
                    socket_set.remove(socket_handle);
                }
            }
        }
    }

    fn write(
        &mut self,
        fd: usize,
        buf: &[u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> SyscallResult<usize> {
        let (fd, setting) = {
            let file = self.handles.get_mut(fd)?;

            match *file {
                Handle::File(SchemeFile::Setting(ref setting_handle)) => {
                    (setting_handle.fd, setting_handle.setting)
                }
                Handle::File(SchemeFile::Socket(ref mut file)) => {
                    let mut socket_set = self.socket_set.borrow_mut();
                    let socket = socket_set.get_mut::<SocketT>(file.socket_handle);
                    let ret = SocketT::write_buf(socket, file, buf);
                    match ret {
                        Err(e) if e.errno == syscall::EWOULDBLOCK || e.errno == syscall::EAGAIN => {
                        }
                        _ => file.write_notified = false,
                    }
                    return ret;
                }
                _ => return Err(SyscallError::new(syscall::EBADF)),
            }
        };
        self.update_setting(fd, setting, buf)
    }

    fn read(
        &mut self,
        fd: usize,
        buf: &mut [u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> SyscallResult<usize> {
        let (fd, setting) = {
            let file = self.handles.get_mut(fd)?;
            match *file {
                Handle::File(SchemeFile::Setting(ref setting_handle)) => {
                    (setting_handle.fd, setting_handle.setting)
                }
                Handle::File(SchemeFile::Socket(ref mut file)) => {
                    let mut socket_set = self.socket_set.borrow_mut();
                    let socket = socket_set.get_mut::<SocketT>(file.socket_handle);

                    let ret = SocketT::read_buf(socket, file, buf);
                    match ret {
                        Err(e) if e.errno == syscall::EWOULDBLOCK || e.errno == syscall::EAGAIN => {
                        }
                        _ => file.read_notified = false,
                    }

                    return ret;
                }
                _ => return Err(SyscallError::new(syscall::EBADF)),
            }
        };
        self.get_setting(fd, setting, buf)
    }

    fn dup(&mut self, fd: usize, buf: &[u8], _ctx: &CallerCtx) -> SyscallResult<OpenResult> {
        let path = str::from_utf8(buf).or_else(|_| Err(SyscallError::new(syscall::EINVAL)))?;

        let new_file = {
            let handle = self.handles.get_mut(fd)?;

            let file = match *handle {
                Handle::SchemeRoot => return Err(SyscallError::new(syscall::EBADF)),
                Handle::Null(ref null) => {
                    let (flags, uid, gid, read_enabled, write_enabled) = (
                        null.flags,
                        null.uid,
                        null.gid,
                        null.read_enabled,
                        null.write_enabled,
                    );
                    // dup from empty path to a new path
                    return self.open_inner(path, flags, uid, gid, read_enabled, write_enabled);
                }
                Handle::File(ref mut file) => file,
            };

            let socket_handle = file.socket_handle();

            let (new_handle, update_with) = match path {
                "hop_limit" => (
                    SchemeFile::Setting(SettingFile {
                        socket_handle,
                        fd,
                        setting: Setting::HopLimit,
                    }),
                    None,
                ),
                "read_timeout" => (
                    SchemeFile::Setting(SettingFile {
                        socket_handle,
                        fd,
                        setting: Setting::ReadTimeout,
                    }),
                    None,
                ),
                "write_timeout" => (
                    SchemeFile::Setting(SettingFile {
                        socket_handle,
                        fd,
                        setting: Setting::WriteTimeout,
                    }),
                    None,
                ),
                _ => match SocketT::dup(
                    &mut self.socket_set.borrow_mut(),
                    file,
                    path,
                    &mut self.scheme_data,
                )? {
                    Some(some) => some,
                    None => return Err(SyscallError::new(syscall::EWOULDBLOCK)),
                },
            };

            if let Some((socket_handle, data)) = update_with {
                if let SchemeFile::Socket(ref mut file) = *file {
                    // We replace the socket_handle pointed by file so update the ref_counts
                    // accordingly
                    self.ref_counts
                        .entry(file.socket_handle)
                        .and_modify(|e| *e = e.saturating_sub(1))
                        .or_insert(0);

                    *self
                        .ref_counts
                        .entry(new_handle.socket_handle())
                        .or_insert(0) += 1;

                    file.socket_handle = socket_handle;
                    file.data = data;
                }
            }
            *self
                .ref_counts
                .entry(new_handle.socket_handle())
                .or_insert(0) += 1;
            new_handle
        };

        let id = self.handles.insert(Handle::File(new_file));

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn fevent(
        &mut self,
        fd: usize,
        events: SyscallEventFlags,
        _ctx: &CallerCtx,
    ) -> SyscallResult<SyscallEventFlags> {
        let file = self.handles.get_mut(fd)?;

        let file = match *file {
            Handle::File(ref mut f) => f,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };

        match *file {
            SchemeFile::Setting(_) => return Err(SyscallError::new(syscall::EBADF)),
            SchemeFile::Socket(ref mut file) => {
                file.events = events.bits();
                file.read_notified = false; // resend missed events
                file.write_notified = false;
            }
        }
        let mut socket_set = self.socket_set.borrow_mut();
        let revents = SyscallEventFlags::from_bits_truncate(file.events(&mut socket_set));
        Ok(revents)
    }

    fn fsync(&mut self, fd: usize, _ctx: &CallerCtx) -> SyscallResult<()> {
        {
            let _file = self.handles.get_mut(fd)?;
        }
        Ok(())
        // TODO Implement fsyncing
        // self.0.network_fsync()
    }

    fn fpath(&mut self, fd: usize, buf: &mut [u8], _ctx: &CallerCtx) -> SyscallResult<usize> {
        let file = self.handles.get_mut(fd)?;

        let file = match *file {
            Handle::File(ref mut f) => f,
            _ => return Err(SyscallError::new(syscall::EBADF)),
        };

        let socket_set = self.socket_set.borrow();
        let socket = socket_set.get::<SocketT>(file.socket_handle());

        socket.fpath(file, buf)
    }

    fn fcntl(
        &mut self,
        fd: usize,
        cmd: usize,
        arg: usize,
        _ctx: &CallerCtx,
    ) -> SyscallResult<usize> {
        let handle = self.handles.get_mut(fd)?;

        match *handle {
            Handle::File(SchemeFile::Socket(ref mut socket_file)) => match cmd {
                syscall::F_GETFL => Ok(socket_file.flags),
                syscall::F_SETFL => {
                    socket_file.flags = arg & !syscall::O_ACCMODE;
                    Ok(0)
                }
                _ => Err(SyscallError::new(syscall::EINVAL)),
            },
            Handle::Null(ref mut null) => match cmd {
                syscall::F_GETFL => Ok(null.flags),
                syscall::F_SETFL => {
                    null.flags = arg & !syscall::O_ACCMODE;
                    Ok(0)
                }
                _ => Err(SyscallError::new(syscall::EINVAL)),
            },
            _ => Err(SyscallError::new(syscall::EBADF)),
        }
    }
}

fn add_time(a: &TimeSpec, b: &TimeSpec) -> TimeSpec {
    let mut secs = a.tv_sec + b.tv_sec;
    let mut nsecs = a.tv_nsec + b.tv_nsec;

    secs += i64::from(nsecs) / 1_000_000_000;
    nsecs %= 1_000_000_000;

    TimeSpec {
        tv_sec: secs,
        tv_nsec: nsecs,
    }
}
