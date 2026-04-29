use std::{cmp, io};

use libredox::flag::O_NONBLOCK;
use libredox::Fd;
use redox_scheme::{
    scheme::{IntoTag, Op, SchemeResponse, SchemeState, SchemeSync},
    CallerCtx, OpenResult, RequestKind, Response, SignalBehavior, Socket,
};
use scheme_utils::{FpathWriter, HandleMap};
use syscall::schemev2::NewFdFlags;
use syscall::{
    Error, EventFlags, Result, Stat, EACCES, EAGAIN, EBADF, EINTR, EINVAL, EWOULDBLOCK, MODE_FILE,
};

pub trait NetworkAdapter {
    /// The [MAC address](https://en.wikipedia.org/wiki/MAC_address) of this
    /// network adapter.
    fn mac_address(&mut self) -> [u8; 6];

    /// The amount of network packets that can be read without blocking.
    fn available_for_read(&mut self) -> usize;

    /// Attempt to read a network packet without blocking.
    ///
    /// Returns `Ok(None)` when there is no pending network packet.
    fn read_packet(&mut self, buf: &mut [u8]) -> Result<Option<usize>>;

    /// Write a single network packet.
    // FIXME support back pressure on writes by returning EWOULDBLOCK or not
    // returning from the write syscall until there is room.
    fn write_packet(&mut self, buf: &[u8]) -> Result<usize>;
}

pub struct NetworkScheme<T: NetworkAdapter> {
    scheme: NetworkSchemeInner<T>,
    state: SchemeState,
    blocked: Vec<(Op, CallerCtx)>,
    socket: Socket,
}

fn post_fevent(socket: &Socket, id: usize, flags: usize) -> Result<()> {
    let fevent_response = Response::post_fevent(id, flags);
    match socket.write_response(fevent_response, SignalBehavior::Restart) {
        Ok(true) => Ok(()),                            // Write response success
        Ok(false) => Err(Error::new(syscall::EAGAIN)), // Write response failed, retry.
        Err(err) => Err(err),                          // Error writing response
    }
}

impl<T: NetworkAdapter> NetworkScheme<T> {
    pub fn new(
        adapter_fn: impl FnOnce() -> T,
        daemon: daemon::Daemon,
        scheme_name: String,
    ) -> Self {
        assert!(scheme_name.starts_with("network"));
        let socket = Socket::nonblock().expect("failed to create network scheme");
        let adapter = adapter_fn();
        let mut scheme = NetworkSchemeInner::new(adapter, scheme_name.clone());
        redox_scheme::scheme::register_sync_scheme(&socket, &scheme_name, &mut scheme)
            .expect("failed to regitster network scheme");
        daemon.ready();
        Self {
            scheme,
            state: SchemeState::new(),
            blocked: Vec::new(),
            socket,
        }
    }

    pub fn event_handle(&self) -> &Fd {
        self.socket.inner()
    }

    pub fn adapter(&self) -> &T {
        &self.scheme.adapter
    }

    pub fn adapter_mut(&mut self) -> &mut T {
        &mut self.scheme.adapter
    }

    /// Process pending and new requests.
    ///
    /// This needs to be called each time there is a new event on the scheme
    /// file and each time a new network packet has been received by the
    /// driver.
    // FIXME maybe split into one method for events on the scheme fd and one
    // to call when an irq is received to indicate that blocked requests can
    // be processed.
    pub fn tick(&mut self) -> io::Result<()> {
        // Handle any blocked requests
        let mut i = 0;
        while i < self.blocked.len() {
            let (op, caller) = &mut self.blocked[i];
            let res = op.handle_sync_dont_consume(caller, &mut self.scheme, &mut self.state);
            match res {
                SchemeResponse::Opened(Err(Error {
                    errno: syscall::EWOULDBLOCK,
                }))
                | SchemeResponse::Regular(Err(Error {
                    errno: syscall::EWOULDBLOCK,
                })) if !op.is_explicitly_nonblock() => {
                    i += 1;
                }
                SchemeResponse::Regular(r) => {
                    let (op, _) = self.blocked.remove(i);
                    let _ = self
                        .socket
                        .write_response(Response::new(r, op), SignalBehavior::Restart)
                        .expect("driver-network: failed to write scheme");
                }
                SchemeResponse::Opened(o) => {
                    let (op, _) = self.blocked.remove(i);
                    let _ = self
                        .socket
                        .write_response(Response::open_dup_like(o, op), SignalBehavior::Restart)
                        .expect("driver-network: failed to write scheme");
                }
                SchemeResponse::RegularAndNotifyOnDetach(status) => {
                    let (op, _) = self.blocked.remove(i);
                    let _ = self
                        .socket
                        .write_response(
                            Response::new_notify_on_detach(status, op),
                            SignalBehavior::Restart,
                        )
                        .expect("driver-network: failed to write scheme");
                }
            }
        }

        // Handle new scheme requests
        loop {
            let request = match self.socket.next_request(SignalBehavior::Restart) {
                Ok(Some(request)) => request,
                Ok(None) => {
                    // Scheme likely got unmounted
                    std::process::exit(0);
                }
                Err(err) if err.errno == EAGAIN => break,
                Err(err) => return Err(err.into()),
            };

            let req = match request.kind() {
                RequestKind::Call(c) => c,
                RequestKind::OnClose { id } => {
                    self.scheme.on_close(id);
                    continue;
                }
                RequestKind::Cancellation(req) => {
                    if let Some(i) = self.blocked.iter().position(|q| q.0.req_id() == req.id) {
                        let (blocked_req, _) = self.blocked.remove(i);
                        let resp = Response::new(Err(Error::new(EINTR)), blocked_req);
                        self.socket.write_response(resp, SignalBehavior::Restart)?;
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
                    self.socket.write_response(
                        Response::err(syscall::EOPNOTSUPP, req),
                        SignalBehavior::Restart,
                    )?;
                    continue;
                }
            };

            let resp = match op.handle_sync_dont_consume(&caller, &mut self.scheme, &mut self.state)
            {
                SchemeResponse::Opened(Err(Error {
                    errno: syscall::EWOULDBLOCK,
                }))
                | SchemeResponse::Regular(Err(Error {
                    errno: syscall::EWOULDBLOCK,
                })) if !op.is_explicitly_nonblock() => {
                    self.blocked.push((op, caller));
                    continue;
                }
                SchemeResponse::Regular(r) => Response::new(r, op),
                SchemeResponse::Opened(o) => Response::open_dup_like(o, op),
                SchemeResponse::RegularAndNotifyOnDetach(status) => {
                    Response::new_notify_on_detach(status, op)
                }
            };
            let _ = self.socket.write_response(resp, SignalBehavior::Restart)?;
        }

        // Notify readers about incoming events
        let available_for_read = self.scheme.adapter.available_for_read();
        if available_for_read > 0 {
            for &handle_id in self.scheme.handles.keys() {
                post_fevent(&self.socket, handle_id, syscall::flag::EVENT_READ.bits())?;
            }
            return Ok(());
        }

        Ok(())
    }
}

struct NetworkSchemeInner<T: NetworkAdapter> {
    adapter: T,
    scheme_name: String,
    handles: HandleMap<Handle>,
}

enum Handle {
    Data,
    Mac,
    SchemeRoot,
}

impl<T: NetworkAdapter> NetworkSchemeInner<T> {
    pub fn new(adapter: T, scheme_name: String) -> Self {
        Self {
            adapter,
            scheme_name,
            handles: HandleMap::new(),
        }
    }
}

impl<T: NetworkAdapter> SchemeSync for NetworkSchemeInner<T> {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }

    fn openat(
        &mut self,
        fd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(fd)?, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }
        if ctx.uid != 0 {
            return Err(Error::new(EACCES));
        }

        let (handle, flags) = match path {
            "" => (Handle::Data, NewFdFlags::empty()),
            "mac" => (Handle::Mac, NewFdFlags::POSITIONED),
            _ => return Err(Error::new(EINVAL)),
        };

        let id = self.handles.insert(handle);
        Ok(OpenResult::ThisScheme { number: id, flags })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get_mut(id)?;

        match *handle {
            Handle::Data => {}
            Handle::Mac => {
                let data = &self.adapter.mac_address()[offset as usize..];
                let i = cmp::min(buf.len(), data.len());
                buf[..i].copy_from_slice(&data[..i]);
                return Ok(i);
            }
            _ => return Err(Error::new(EBADF)),
        };

        match self.adapter.read_packet(buf)? {
            Some(count) => Ok(count),
            None => {
                if fcntl_flags & O_NONBLOCK as u32 != 0 {
                    Err(Error::new(EAGAIN))
                } else {
                    Err(Error::new(EWOULDBLOCK))
                }
            }
        }
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get(id)?;

        match handle {
            Handle::Data => {}
            Handle::Mac { .. } => return Err(Error::new(EINVAL)),
            _ => return Err(Error::new(EBADF)),
        }

        Ok(self.adapter.write_packet(buf)?)
    }

    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let _handle = self.handles.get(id)?;
        Ok(EventFlags::empty())
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with(buf, &self.scheme_name, |w| {
            let path = match self.handles.get(id)? {
                Handle::Data { .. } => "",
                Handle::Mac { .. } => "mac",
                _ => "",
            };
            write!(w, "{path}").unwrap();
            Ok(())
        })
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let handle = self.handles.get(id)?;

        match handle {
            Handle::Data { .. } => {
                stat.st_mode = MODE_FILE | 0o700;
            }
            Handle::Mac { .. } => {
                stat.st_mode = MODE_FILE | 0o400;
                stat.st_size = 6;
            }
            _ => return Err(Error::new(EBADF)),
        }

        Ok(())
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _handle = self.handles.get(id)?;
        Ok(())
    }

    fn on_close(&mut self, id: usize) {
        self.handles.remove(id);
    }
}
