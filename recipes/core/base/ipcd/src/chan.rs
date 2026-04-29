use redox_scheme::{scheme::SchemeSync, CallerCtx, OpenResult, Response, SignalBehavior, Socket};
use scheme_utils::FpathWriter;
use std::{
    cmp,
    collections::{HashMap, VecDeque},
};
use syscall::{error::*, flag::*, schemev2::NewFdFlags, Error};

#[derive(Debug, Default)]
pub struct Client {
    buffer: Vec<u8>,
    remote: Connection,
}
#[derive(Debug, Default)]
pub struct Listener {
    path: Option<String>,
    awaiting: VecDeque<usize>,
}
#[derive(Debug)]
pub enum Extra {
    Client(Client),
    Listener(Listener),
    SchemeRoot,
}
impl Default for Extra {
    fn default() -> Self {
        Extra::Client(Client::default())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Connection {
    Waiting,
    Open(usize),
    Closed,
}
impl Default for Connection {
    fn default() -> Self {
        Connection::Waiting
    }
}

#[derive(Debug, Default)]
pub struct Handle {
    flags: usize,
    extra: Extra,
    path: Option<String>,
}
impl Handle {
    /// Duplicate this listener handle into one that is linked to the
    /// specified remote.
    /// Does NOT error if this is not a listener
    pub fn accept(&self, remote: usize) -> Self {
        Self {
            flags: self.flags,
            extra: Extra::Client(Client {
                remote: Connection::Open(remote),
                ..Client::default()
            }),
            ..Default::default()
        }
    }

    /// Mark this listener handle as having a connection which can be
    /// accepted, but only if it is ready to accept.
    /// Errors if this is not a listener
    pub fn connect(&mut self, other: usize) -> Result<()> {
        match self.extra {
            Extra::Listener(ref mut listener) => {
                listener.awaiting.push_back(other);
                Ok(())
            }
            _ => Err(Error::new(EBADF)),
        }
    }

    /// Error if this is not a listener
    pub fn require_listener(&mut self) -> Result<&mut Listener> {
        match self.extra {
            Extra::Listener(ref mut listener) => Ok(listener),
            _ => Err(Error::new(EBADF)),
        }
    }

    /// Error if this is not a client
    pub fn require_client(&mut self) -> Result<&mut Client> {
        match self.extra {
            Extra::Client(ref mut client) => Ok(client),
            _ => Err(Error::new(EBADF)),
        }
    }
}

pub struct ChanScheme<'sock> {
    handles: HashMap<usize, Handle>,
    listeners: HashMap<String, usize>,
    next_id: usize,
    socket: &'sock Socket,
}
impl<'sock> ChanScheme<'sock> {
    pub fn new(socket: &'sock Socket) -> Self {
        Self {
            handles: HashMap::new(),
            listeners: HashMap::new(),
            next_id: 0,
            socket,
        }
    }

    fn post_fevent(&self, id: usize, flags: usize) -> Result<()> {
        let fevent_response = Response::post_fevent(id, flags);
        match self
            .socket
            .write_response(fevent_response, SignalBehavior::Restart)
        {
            Ok(true) => Ok(()),                   // Write response success
            Ok(false) => Err(Error::new(EAGAIN)), // Write response failed, retry.
            Err(err) => Err(err),                 // Error writing response
        }
    }

    fn open(&mut self, path: &str, flags: usize) -> Result<OpenResult> {
        let new_id = self.next_id;
        let mut new = Handle::default();
        new.flags = flags;

        let create = flags & O_CREAT == O_CREAT;

        if create && !self.listeners.contains_key(path) {
            let mut listener = Listener::default();
            if !path.is_empty() {
                self.listeners.insert(String::from(path), new_id);
                listener.path = Some(String::from(path));
            }
            new.extra = Extra::Listener(listener);
        } else if create && flags & O_EXCL == O_EXCL {
            return Err(Error::new(EEXIST));
        } else {
            // Connect to existing if: O_CREAT isn't set or it already exists
            // and O_EXCL isn't set
            let listener_id = *self.listeners.get(path).ok_or(Error::new(ENOENT))?;
            let listener = self
                .handles
                .get_mut(&listener_id)
                .expect("orphan listener left over");
            listener.connect(new_id)?;

            // smoltcp sends writeable whenever a listener gets a
            // client, we'll do the same too (but also readable, why
            // not)
            self.post_fevent(listener_id, (EVENT_READ | EVENT_WRITE).bits())?;
        }

        self.handles.insert(new_id, new);
        self.next_id += 1;

        Ok(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::empty(),
        })
    }
}

impl<'sock> SchemeSync for ChanScheme<'sock> {
    fn scheme_root(&mut self) -> Result<usize> {
        let id = self.next_id;
        self.next_id += 1;

        self.handles.insert(
            id,
            Handle {
                flags: 0,
                extra: Extra::SchemeRoot,
                path: None,
            },
        );
        Ok(id)
    }
    //   ___  ____  _____ _   _
    //  / _ \|  _ \| ____| \ | |
    // | | | | |_) |  _| |  \| |
    // | |_| |  __/| |___| |\  |
    //  \___/|_|   |_____|_| \_|
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        let handle = self.handles.get(&dirfd).ok_or(Error::new(EBADF))?;

        if !matches!(handle.extra, Extra::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        self.open(path, flags)
    }
    fn dup(&mut self, id: usize, buf: &[u8], _ctx: &CallerCtx) -> Result<OpenResult> {
        match buf {
            b"listen" => {
                loop {
                    let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
                    let listener = handle.require_listener()?;
                    let listener_path = listener.path.clone();

                    break if let Some(remote_id) = listener.awaiting.pop_front() {
                        let new_id = self.next_id;
                        let mut new = handle.accept(remote_id);

                        // Hook the remote side, assuming it's still
                        // connected, up to this one so the connection is
                        // mutal.
                        let remote = match self.handles.get_mut(&remote_id) {
                            Some(client) => client,
                            None => continue, // Check next client
                        };
                        match remote.extra {
                            Extra::Client(ref mut client) => {
                                client.remote = Connection::Open(new_id);
                            }
                            Extra::Listener(_) => {
                                panic!("newly created handle can't possibly be a listener")
                            }
                            Extra::SchemeRoot => return Err(Error::new(EBADF)),
                        }
                        self.post_fevent(remote_id, EVENT_WRITE.bits())?;

                        new.path = listener_path;

                        self.handles.insert(new_id, new);
                        self.next_id += 1;

                        Ok(OpenResult::ThisScheme {
                            number: new_id,
                            flags: NewFdFlags::empty(),
                        })
                    } else if handle.flags & O_NONBLOCK == O_NONBLOCK {
                        Ok(OpenResult::WouldBlock)
                    } else {
                        Err(Error::new(EWOULDBLOCK))
                    };
                }
            }
            b"connect" => {
                let new_id = self.next_id;
                let new = Handle::default();

                let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
                handle.require_listener()?;
                handle.connect(new_id)?;

                // smoltcp sends writeable whenever a listener gets a
                // client, we'll do the same too (but also readable,
                // why not)
                self.post_fevent(id, (EVENT_READ | EVENT_WRITE).bits())?;

                self.handles.insert(new_id, new);
                self.next_id += 1;

                Ok(OpenResult::ThisScheme {
                    number: new_id,
                    flags: NewFdFlags::empty(),
                })
            }
            _ => {
                // If a buf is provided, different than "connect" / "listen",
                // turn the socket into a named socket.

                if buf.is_empty() {
                    return Err(Error::new(EBADF));
                }

                let path = core::str::from_utf8(buf).map_err(|_| Error::new(EBADF))?;

                let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
                if handle.path.is_some() {
                    return Err(Error::new(EBADF));
                }

                if matches!(handle.extra, Extra::SchemeRoot) {
                    return Err(Error::new(EBADF));
                }

                let flags = handle.flags;
                return self.open(path, flags);
            }
        }
    }

    //  ___ ___     ___      ____ _     ___  ____  _____
    // |_ _/ _ \   ( _ )    / ___| |   / _ \/ ___|| ____|
    //  | | | | |  / _ \/\ | |   | |  | | | \___ \|  _|
    //  | | |_| | | (_>  < | |___| |__| |_| |___) | |___
    // |___\___/   \___/\/  \____|_____\___/|____/|_____|

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let client = handle.require_client()?;

        if let Connection::Open(remote_id) = client.remote {
            let remote = self.handles.get_mut(&remote_id).unwrap();
            match remote.extra {
                Extra::Client(ref mut client) => {
                    client.buffer.extend(buf);
                    if client.buffer.len() == buf.len() {
                        // Send readable only if it wasn't readable
                        // before
                        self.post_fevent(remote_id, EVENT_READ.bits())?;
                    }
                    Ok(buf.len())
                }
                Extra::Listener(_) => {
                    panic!("somehow, a client was connected to a listener directly")
                }
                Extra::SchemeRoot => panic!("somehow, a client was connected to a SchemeRoot"),
            }
        } else if client.remote == Connection::Closed {
            Err(Error::new(EPIPE))
        } else if (flags as usize) & O_NONBLOCK == O_NONBLOCK {
            Err(Error::new(EAGAIN))
        } else {
            Err(Error::new(EWOULDBLOCK))
        }
    }
    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with_legacy(buf, "chan", |w| {
            let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
            if let Extra::SchemeRoot = handle.extra {
                return Ok(());
            }
            w.push_str(handle.path.as_ref().ok_or(Error::new(EBADF))?);

            Ok(())
        })
    }
    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        self.handles.get(&id).ok_or(Error::new(EBADF)).and(Ok(()))
    }
    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        _offset: u64,
        flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let client = handle.require_client()?;

        if !client.buffer.is_empty() {
            let len = cmp::min(buf.len(), client.buffer.len());
            buf[..len].copy_from_slice(&client.buffer[..len]);
            client.buffer.drain(..len);
            Ok(len)
        } else if client.remote == Connection::Closed {
            // Remote dropped, send EOF
            Ok(0)
        } else if (flags as usize) & O_NONBLOCK == O_NONBLOCK {
            Err(Error::new(EAGAIN))
        } else {
            Err(Error::new(EWOULDBLOCK))
        }
    }
    fn on_close(&mut self, id: usize) {
        let handle = self
            .handles
            .remove(&id)
            .expect("handle pointing to nothing");

        match handle.extra {
            Extra::Client(client) => {
                if let Connection::Open(remote_id) = client.remote {
                    let remote = self.handles.get_mut(&remote_id).unwrap();

                    match remote.extra {
                        Extra::Client(ref mut client) => {
                            client.remote = Connection::Closed;
                            if client.buffer.is_empty() {
                                // Post readable on EOF only if it wasn't
                                // readable before
                                self.post_fevent(remote_id, EVENT_READ.bits()).unwrap();
                            }
                        }
                        Extra::Listener(_) => panic!("a client can't be connected to a listener!"),
                        Extra::SchemeRoot => {
                            panic!("a client can't be connected to a scheme root!")
                        }
                    }
                }
            }
            Extra::Listener(listener) => {
                if let Some(path) = listener.path {
                    self.listeners.remove(&path);
                }
            }
            Extra::SchemeRoot => {}
        }
    }

    //  ____   _    ____      _    __  __ _____ _____ _____ ____  ____
    // |  _ \ / \  |  _ \    / \  |  \/  | ____|_   _| ____|  _ \/ ___|
    // | |_) / _ \ | |_) |  / _ \ | |\/| |  _|   | | |  _| | |_) \___ \
    // |  __/ ___ \|  _ <  / ___ \| |  | | |___  | | | |___|  _ < ___) |
    // |_| /_/   \_\_| \_\/_/   \_\_|  |_|_____| |_| |_____|_| \_\____/

    fn fcntl(&mut self, id: usize, cmd: usize, arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        match cmd {
            F_GETFL => Ok(handle.flags),
            F_SETFL => {
                handle.flags = arg;
                Ok(0)
            }
            _ => Err(Error::new(EINVAL)),
        }
    }
    fn fevent(&mut self, id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let mut events = EventFlags::empty();
        match handle.extra {
            Extra::Client(ref client) => {
                if let Connection::Open(_) = client.remote {
                    events |= EVENT_WRITE;
                }
                if !client.buffer.is_empty() || client.remote == Connection::Closed {
                    events |= EVENT_READ;
                }
            }
            Extra::Listener(ref listener) => {
                if !listener.awaiting.is_empty() {
                    events |= EVENT_READ | EVENT_WRITE;
                }
            }
            Extra::SchemeRoot => {}
        }
        Ok(events)
    }
}
