//! uds scheme for handling Unix Domain Socket stream communication

use super::{
    get_uid_gid_from_pid, path_buf_to_str, read_msghdr_info, read_num, AncillaryData, Credential,
    DataPacket, MsgWriter, MIN_RECV_MSG_LEN,
};

use libc::{ucred, AF_UNIX};
use libredox::protocol::SocketCall;
use rand::prelude::*;
use redox_scheme::{
    scheme::SchemeSync, CallerCtx, OpenResult, RecvFdRequest, Response, SendFdRequest,
    SignalBehavior, Socket as SchemeSocket,
};
use scheme_utils::FpathWriter;
use std::{
    cell::RefCell,
    cmp,
    collections::{HashMap, HashSet, VecDeque},
    mem,
    rc::Rc,
    slice,
};
use syscall::{error::*, flag::*, schemev2::NewFdFlags, Error, Stat};

#[derive(Clone, Copy, Default)]
struct MsgFlags(libc::c_int);

impl MsgFlags {
    fn nonblock(&self) -> bool {
        self.0 & libc::MSG_DONTWAIT == libc::MSG_DONTWAIT
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Connection {
    peer: usize,
    packets: VecDeque<DataPacket>,
    fds: VecDeque<usize>,

    is_peer_shutdown: bool,
}
impl Connection {
    fn new(peer: usize) -> Self {
        Self {
            peer,
            ..Default::default()
        }
    }

    fn drop_fds(&mut self, num_fd: usize) -> Result<()> {
        for i in 0..num_fd {
            if self.fds.pop_front().is_none() {
                eprintln!("Connection::drop_fds: Attempted to drop FD #{} of {}, but fd queue is empty. State inconsistency.", i + 1, num_fd);
                return Err(Error::new(EPROTO));
            }
        }
        Ok(())
    }

    fn can_read(&self) -> bool {
        !self.packets.is_empty() || !self.fds.is_empty() || self.is_peer_shutdown
    }

    fn serialize_to_msgstream(
        &mut self,
        stream: &mut [u8],
        name_buf_size: usize,
        iov_size: usize,
        options: HashSet<i32>,
    ) -> Result<usize> {
        let mut name: Option<String> = None;
        let mut payload_buffer: Vec<u8> = Vec::with_capacity(iov_size);
        let mut ancillary_data_buffer: VecDeque<AncillaryData> = VecDeque::new();
        let mut total_copied_len = 0;
        let mut user_buf_offset = 0;

        while user_buf_offset < iov_size {
            let Some(packet) = self.packets.front_mut() else {
                // No more packets to read
                break;
            };

            let packet_rem_payload = &packet.payload[packet.read_offset..];

            let user_buf_rem_len = iov_size - user_buf_offset;

            let copied_len = cmp::min(packet_rem_payload.len(), user_buf_rem_len);
            if copied_len == 0 {
                // No more data to read from this packet
                break;
            }
            payload_buffer.extend_from_slice(&packet_rem_payload[..copied_len]);

            if !packet.ancillary_taken {
                name = name.or_else(|| packet.ancillary_data.name.take());
                ancillary_data_buffer.push_back(packet.ancillary_data.clone());
                packet.ancillary_taken = true; // Mark ancillary data as taken
            }

            packet.read_offset += copied_len;
            user_buf_offset += copied_len;
            total_copied_len += copied_len;
            if packet.read_offset >= packet.payload.len() {
                // If the packet is fully read, remove it from the queue
                self.packets.pop_front();
            }
        }

        let mut msg_writer = MsgWriter::new(stream);

        msg_writer.write_name(name, name_buf_size, UdsStreamScheme::fpath_inner)?;

        let full_len = cmp::min(total_copied_len, iov_size);
        msg_writer.write_payload(&payload_buffer, full_len, iov_size)?;

        let mut num_fds = 0;
        for ancillary_data in ancillary_data_buffer.iter() {
            num_fds += ancillary_data.num_fds;
        }
        if !msg_writer.write_rights(num_fds) {
            eprintln!(
                "serialize_to_msgstream: Buffer too small for SCM_RIGHTS, dropping {} FDs.",
                num_fds
            );
            self.drop_fds(num_fds)?;
        }

        for option in options {
            let result = match option {
                libc::SO_PASSCRED => {
                    let mut success = true;
                    for data in &ancillary_data_buffer {
                        if !msg_writer.write_credentials(&data.cred) {
                            success = false;
                            break;
                        }
                    }
                    success
                }
                _ => {
                    eprintln!(
                        "serialize_to_msgstream: Unsupported socket option for serialization: {}",
                        option
                    );
                    return Err(Error::new(EOPNOTSUPP));
                }
            };
            if !result {
                eprintln!("serialize_to_msgstream: Buffer too small for ancillary data, stopping further serialization.");
                break;
            }
        }

        Ok(msg_writer.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum State {
    Unbound,
    Bound,
    Listening,
    Connecting,
    Accepted,
    Established,
    Closed,
}

impl Default for State {
    fn default() -> Self {
        Self::Unbound
    }
}

#[derive(Debug)]
pub struct Socket {
    primary_id: usize,
    path: Option<String>,
    options: HashSet<i32>,
    flags: usize,
    state: State,
    awaiting: VecDeque<usize>,
    connection: Option<Connection>,
    issued_token: Option<u64>,
    ucred: ucred,
}

impl Socket {
    fn new(
        id: usize,
        path: Option<String>,
        state: State,
        options: HashSet<i32>,
        flags: usize,
        connection: Option<Connection>,
        ctx: &CallerCtx,
    ) -> Self {
        Self {
            primary_id: id,
            path,
            state,
            options,
            flags,
            awaiting: VecDeque::new(),
            connection,
            issued_token: None,
            //TODO: when should ucred be updated? man 7 unix for SO_PEERCRED says on connect, listen, or socketpair
            ucred: ucred {
                pid: ctx.pid as _,
                uid: ctx.uid as _,
                gid: ctx.gid as _,
            },
        }
    }

    fn events(&self) -> EventFlags {
        let mut ready = EventFlags::empty();
        if let Some(connection) = &self.connection {
            if connection.can_read() {
                ready |= EVENT_READ;
            }
            //TODO: block on write buffer
            ready |= EVENT_WRITE;
        }
        match self.state {
            State::Listening => {
                if !self.awaiting.is_empty() {
                    ready |= EVENT_READ;
                }
            }
            State::Closed => {
                ready |= EVENT_READ;
            }
            _ => {}
        }
        ready
    }

    fn accept(
        &mut self,
        primary_id: usize,
        awaiting_client_id: usize,
        ctx: &CallerCtx,
    ) -> Result<Self> {
        if !self.is_listening() {
            eprintln!(
                "accept(id: {}): Accept called on a non-listening socket.",
                self.primary_id
            );
            return Err(Error::new(EINVAL));
        }
        Ok(Self::new(
            primary_id,
            self.path.clone(),
            State::Established,
            self.options.clone(),
            self.flags,
            Some(Connection::new(awaiting_client_id)),
            ctx,
        ))
    }

    fn establish(&mut self, new_socket: &mut Self, peer: usize) -> Result<()> {
        if self.state != State::Connecting {
            eprintln!(
                "establish(id: {}): Cannot establish connection in state: {:?}",
                self.primary_id, self.state
            );
            return Err(Error::new(EINVAL));
        }
        self.state = State::Accepted;
        if let Some(conn) = &mut self.connection {
            if conn.peer != peer {
                // client is expecting other connection
                return Err(Error::new(EAGAIN));
            }
            conn.peer = new_socket.primary_id;
            if let Some(ref mut new_conn) = &mut new_socket.connection {
                new_conn.packets.append(&mut conn.packets);
            }
        } else {
            // client is dead
            return Err(Error::new(EAGAIN));
        }
        Ok(())
    }

    fn connect(&mut self, other: &mut Socket) -> Result<()> {
        match self.state {
            State::Unbound | State::Bound => {
                // If the socket is unbound or bound, wait for the listener to start listening.
                if other.flags & O_NONBLOCK != O_NONBLOCK {
                    // If the connecting target is not a listening,
                    // the connecting socket will block until the socket
                    // is ready to accept.
                    return Err(Error::new(EWOULDBLOCK));
                }
            }
            State::Listening => {
                // If the socket is already listening, it can accept connections.
            }
            _ => return Err(Error::new(ECONNREFUSED)),
        }
        self.connect_unchecked(other);
        Ok(())
    }

    fn connect_unchecked(&mut self, other: &mut Socket) {
        self.awaiting.push_back(other.primary_id);
        other.state = State::Connecting;
        other.connection = Some(Connection::new(self.primary_id));
    }

    fn is_listening(&self) -> bool {
        self.state == State::Listening
    }

    fn require_connection(&mut self) -> Result<&mut Connection> {
        if let Some(connection) = &mut self.connection {
            Ok(connection)
        } else {
            eprintln!(
                "Socket (id: {}): connection is None in require_connection",
                self.primary_id
            );
            Err(Error::new(EPROTO))
        }
    }

    fn require_connected_connection(&mut self, msg_flags: MsgFlags) -> Result<&mut Connection> {
        match self.state {
            State::Established | State::Accepted => self.require_connection(),
            State::Connecting => {
                if self.flags & O_NONBLOCK == O_NONBLOCK || msg_flags.nonblock() {
                    Err(Error::new(EAGAIN))
                } else {
                    Err(Error::new(EWOULDBLOCK))
                }
            }
            State::Closed => Err(Error::new(EPIPE)),
            _ => Err(Error::new(ENOTCONN)),
        }
    }

    fn start_listening(&mut self) -> Result<()> {
        if !matches!(self.state, State::Unbound | State::Bound) {
            eprintln!(
                "start_listening(id: {}): Socket cannot listen in state {:?}.",
                self.primary_id, self.state
            );
            return Err(Error::new(EINVAL));
        }
        self.state = State::Listening;
        Ok(())
    }

    fn serialize_to_msgstream(
        &mut self,
        msg_flags: MsgFlags,
        stream: &mut [u8],
        name_buf_size: usize,
        iov_size: usize,
    ) -> Result<usize> {
        let options = self.options.clone();
        let connection = self.require_connected_connection(msg_flags)?;
        connection.serialize_to_msgstream(stream, name_buf_size, iov_size, options)
    }
}

enum Handle {
    Socket(Rc<RefCell<Socket>>),
    SchemeRoot,
}

impl Handle {
    fn as_socket(&self) -> Option<&Rc<RefCell<Socket>>> {
        if let Self::Socket(socket) = self {
            Some(socket)
        } else {
            None
        }
    }
    fn is_scheme_root(&self) -> bool {
        matches!(self, Self::SchemeRoot)
    }
}

pub struct UdsStreamScheme<'sock> {
    handles: HashMap<usize, Handle>,
    next_id: usize,
    socket_paths: HashMap<String, Rc<RefCell<Socket>>>,
    socket_tokens: HashMap<u64, Rc<RefCell<Socket>>>,
    socket: &'sock SchemeSocket,
    proc_creds_capability: usize,
    rng: SmallRng,
}

impl<'sock> UdsStreamScheme<'sock> {
    pub fn new(socket: &'sock SchemeSocket) -> Result<Self> {
        Ok(Self {
            handles: HashMap::new(),
            next_id: 0,
            socket_paths: HashMap::new(),
            socket_tokens: HashMap::new(),
            socket,
            proc_creds_capability: {
                libredox::call::open(
                    "/scheme/proc/proc-creds-capability",
                    libredox::flag::O_RDONLY,
                    0,
                )?
            },
            rng: rand::make_rng(),
        })
    }

    fn post_fevent(&self, id: usize, flags: EventFlags) -> Result<()> {
        /*TODO: filter out unnecessary flags?
        if let Ok(socket_rc) = self.get_socket(id) {
            let socket = socket_rc.borrow();
            let socket_flags = socket.events();
        }
        */
        let fevent_response = Response::post_fevent(id, flags.bits());
        match self
            .socket
            .write_response(fevent_response, SignalBehavior::Restart)
        {
            Ok(true) => Ok(()),                   // Write response success
            Ok(false) => Err(Error::new(EAGAIN)), // Write response failed, retry.
            Err(err) => Err(err),                 // Error writing response
        }
    }

    fn get_socket(&self, id: usize) -> Result<&Rc<RefCell<Socket>>, Error> {
        self.handles
            .get(&id)
            .and_then(Handle::as_socket)
            .ok_or(Error::new(EBADF))
    }

    fn insert_socket(&mut self, id: usize, socket: Rc<RefCell<Socket>>) {
        self.handles.insert(id, Handle::Socket(socket));
    }

    fn get_connected_peer(&self, id: usize) -> Result<(usize, Rc<RefCell<Socket>>), Error> {
        let mut socket = self.get_socket(id)?.borrow_mut();

        let remote_id = socket.require_connection()?.peer;
        let remote_rc = self.get_socket(remote_id).map_err(|e| {
            eprintln!("get_connected_peer(id: {}): Peer socket (id: {}) has vanished. Original error: {:?}", id, remote_id, e);
            Error::new(EPIPE)
        })?;

        if remote_rc.borrow().state == State::Closed {
            eprintln!(
                "get_connected_peer(id: {}): Attempted to interact with a closed peer (id: {}).",
                id, remote_id
            );
            return Err(Error::new(ECONNREFUSED));
        }

        Ok((remote_id, remote_rc.clone()))
    }

    fn handle_unnamed_socket(&mut self, flags: usize, ctx: &CallerCtx) -> usize {
        let new_id = self.next_id;
        let new = Socket::new(
            new_id,
            None,
            State::Unbound,
            HashSet::new(),
            flags,
            None,
            ctx,
        );
        self.insert_socket(new_id, Rc::new(RefCell::new(new)));
        self.next_id += 1;
        new_id
    }

    fn call_inner(
        &mut self,
        id: usize,
        payload: &mut [u8],
        metadata: &[u64],
        ctx: &CallerCtx,
    ) -> Result<usize> {
        let Some(verb) =
            SocketCall::try_from_raw(*metadata.get(0).ok_or(Error::new(EINVAL))? as usize)
        else {
            eprintln!("call_inner: Invalid verb in metadata: {:?}", metadata);
            return Err(Error::new(EINVAL));
        };
        match verb {
            SocketCall::Bind => self.handle_bind(id, &payload),
            SocketCall::Connect => self.handle_connect(id, &payload),
            SocketCall::SetSockOpt => self.handle_setsockopt(
                id,
                *metadata.get(1).ok_or(Error::new(EINVAL))? as i32,
                &payload,
            ),
            SocketCall::GetSockOpt => self.handle_getsockopt(
                id,
                *metadata.get(1).ok_or(Error::new(EINVAL))? as i32,
                payload,
            ),
            SocketCall::SendMsg => self.handle_sendmsg(
                id,
                metadata
                    .get(1)
                    .map(|x| MsgFlags(*x as _))
                    .unwrap_or_default(),
                payload,
                ctx,
            ),
            SocketCall::RecvMsg => self.handle_recvmsg(
                id,
                metadata
                    .get(1)
                    .map(|x| MsgFlags(*x as _))
                    .unwrap_or_default(),
                payload,
            ),
            SocketCall::Unbind => self.handle_unbind(id),
            SocketCall::GetToken => self.handle_get_token(id, payload),
            SocketCall::GetPeerName => self.handle_get_peer_name(id, payload),
            _ => Err(Error::new(EOPNOTSUPP)),
        }
    }

    fn handle_bind(&mut self, id: usize, path_buf: &[u8]) -> Result<usize> {
        let path = path_buf_to_str(path_buf)?;

        if self.socket_paths.contains_key(path) {
            eprintln!("handle_bind: Path '{}' is already in use.", path);
            return Err(Error::new(EADDRINUSE));
        }

        let socket_rc = self.get_socket(id)?.clone();
        let path_owned: String;
        let token: u64;
        {
            let mut socket = socket_rc.borrow_mut();

            if socket.state != State::Unbound {
                eprintln!(
                    "handle_bind(id: {}): Socket is already bound or connected (state: {:?})",
                    id, socket.state
                );
                return Err(Error::new(EINVAL));
            }

            path_owned = path.to_string();
            socket.path = Some(path_owned.clone());
            socket.state = State::Bound;
            token = self.rng.next_u64();
            socket.issued_token = Some(token);

            //TODO: Hack since relibc does not listen()
            socket.start_listening()?;
        }

        self.socket_paths.insert(path_owned, socket_rc.clone());
        self.socket_tokens.insert(token, socket_rc);

        Ok(0)
    }

    // There are three phases of connecting a socket:
    //
    // Phase 1: The listener is bound but not yet listening.
    //          The client is trying to connect.
    //          If the listener is not listening, the listener will
    //          refuse to connect until the listener starts listening.
    //
    // Phase 2: The listener is now listening.
    //          The client is still trying to connect.
    //          The client pushes its ID to the listener's awaiting queue
    //          and sets its state to `Connecting`.
    //          The client will be blocked from receiving messages,
    //          but now allowed to send messages.
    //
    // Phase 3: The listener accepts the client, changes its state to `Established`,
    //          and then changes the client's state to `Accepted`.
    //          The client detects that its state has changed to `Accepted`
    //          and changes its own state to `Established`.
    //
    // After these three phases, the socket connection is considered established.
    fn handle_connect(&mut self, id: usize, token_buf: &[u8]) -> Result<usize> {
        let token = read_num::<u64>(token_buf)?;
        let (listener_id, connecting_res) = {
            let listener_rc = self
                .socket_tokens
                .get(&token)
                .ok_or_else(|| Error::new(ECONNREFUSED))?
                .clone();

            let client_rc = self.get_socket(id)?.clone();
            let mut client = client_rc.borrow_mut();

            // Phase 1: listener is bound but not yet listening
            let mut listener = listener_rc.borrow_mut();
            let listener_id = listener.primary_id;

            let connecting_res = if client.flags & O_NONBLOCK == O_NONBLOCK {
                Err(Error::new(EAGAIN))
            } else {
                Err(Error::new(EWOULDBLOCK))
            };

            match client.state {
                State::Connecting => {
                    if client
                        .connection
                        .as_ref()
                        .is_some_and(|c| c.peer == listener_id)
                    {
                        // No op
                        return connecting_res;
                    }
                }
                State::Established => {
                    return Err(Error::new(EISCONN));
                }
                State::Accepted => {
                    // Phase 3: Socket is already connected
                    client.state = State::Established;
                    return Ok(0);
                }
                _ => {}
            }

            // Phase 2: listener is now listening
            listener.connect(&mut client)?;

            (listener_id, connecting_res)
        };
        // smoltcp sends writeable whenever a listener gets a
        // client, we'll do the same too (but also readable, why
        // not)
        self.post_fevent(listener_id, EVENT_READ | EVENT_WRITE)?;
        connecting_res
    }

    fn handle_setsockopt(&mut self, id: usize, option: i32, value_slice: &[u8]) -> Result<usize> {
        let socket_rc = self.get_socket(id)?;
        let mut socket = socket_rc.borrow_mut();

        match option {
            libc::SO_PASSCRED => {
                let value = read_num::<i32>(value_slice)?;
                if value != 0 {
                    socket.options.insert(libc::SO_PASSCRED);
                } else {
                    socket.options.remove(&libc::SO_PASSCRED);
                }
                Ok(value_slice.len())
            }
            libc::SO_SNDBUF => {
                // FIXME: implement
                Ok(0)
            }
            _ => {
                eprintln!(
                    "socket_setsockopt(id: {}): Unsupported option: {}",
                    id, option
                );
                Err(Error::new(ENOPROTOOPT))
            }
        }
    }

    fn handle_getsockopt(&mut self, id: usize, option: i32, payload: &mut [u8]) -> Result<usize> {
        let mut write_value = |value: &[u8]| -> Result<usize> {
            if payload.len() < value.len() {
                eprintln!(
                    "socket_getsockopt(id: {}, option: {}): payload buffer is too small. len: {} < {}",
                    id,
                    option,
                    payload.len(),
                    value.len()
                );
                return Err(Error::new(ENOBUFS));
            }
            payload.fill(0);
            payload[..value.len()].copy_from_slice(&value);
            Ok(value.len())
        };
        match option {
            libc::SO_DOMAIN => write_value(&AF_UNIX.to_le_bytes()),
            libc::SO_PEERCRED => {
                let (_, remote_rc) = self.get_connected_peer(id)?;
                let remote = remote_rc.borrow();
                write_value(unsafe {
                    slice::from_raw_parts(
                        &remote.ucred as *const ucred as *const u8,
                        mem::size_of::<ucred>(),
                    )
                })
            }
            libc::SO_SNDBUF => {
                //TODO: default value on Linux, should we use something else?
                let value: libc::c_int = 212992;
                write_value(&value.to_le_bytes())
            }
            _ => {
                eprintln!(
                    "socket_getsockopt(id: {}): Unsupported option: {}",
                    id, option
                );
                Err(Error::new(ENOPROTOOPT))
            }
        }
    }

    fn handle_sendmsg(
        &mut self,
        id: usize,
        msg_flags: MsgFlags,
        msg_stream: &[u8],
        ctx: &CallerCtx,
    ) -> Result<usize> {
        if msg_stream.is_empty() {
            eprintln!("msg_stream is empty, returning EINVAL.");
            return Err(Error::new(EINVAL));
        }

        let (bytes_written, remote_id) = {
            let name = self.get_socket(id)?.borrow().path.clone();
            let (remote_id, remote_rc) = self.get_connected_peer(id)?;
            let mut socket = remote_rc.borrow_mut();
            let connection = socket.require_connected_connection(msg_flags)?;
            let (pid, uid, gid) = get_uid_gid_from_pid(self.proc_creds_capability, ctx.pid)?;

            let packet = DataPacket::from_stream(
                msg_stream,
                name,
                Credential::new(pid as i32, uid as i32, gid as i32),
            )?;

            let payload_len = packet.len();

            // sendmsg(2) on `SOCK_STREAM` with zero-byte payload is a no-op
            // even if ancillary data is present. Note that this does not apply
            // to `SOCK_DGRAM`.
            if payload_len == 0 {
                return Ok(0);
            }

            connection.packets.push_back(packet);
            (payload_len, remote_id)
        };

        self.post_fevent(remote_id, EVENT_READ)?;
        Ok(bytes_written)
    }

    fn handle_recvmsg(
        &mut self,
        id: usize,
        msg_flags: MsgFlags,
        msg_stream: &mut [u8],
    ) -> Result<usize> {
        let socket_rc = self.get_socket(id)?;
        let mut socket = socket_rc.borrow_mut();
        let flags = socket.flags;
        let connection = match &mut socket.state {
            State::Established | State::Accepted => socket.require_connection()?,
            State::Closed => {
                // Remote dropped, send EOF
                return Self::write_eof(msg_stream);
            }
            State::Listening => {
                eprintln!("socket_recvmsg: Called on a listening socket, returning EOPNOTSUPP.");
                return Err(Error::new(EOPNOTSUPP));
            }
            _ => return Err(Error::new(ENOTCONN)),
        };

        if connection.packets.is_empty() {
            return if connection.is_peer_shutdown {
                // EOF, no data to read
                return Self::write_eof(msg_stream);
            } else if (flags as usize) & O_NONBLOCK == O_NONBLOCK || msg_flags.nonblock() {
                Err(Error::new(EAGAIN))
            } else {
                Err(Error::new(EWOULDBLOCK))
            };
        }
        Self::recvmsg_inner(&mut socket, msg_flags, msg_stream)
    }

    fn write_eof(buffer: &mut [u8]) -> Result<usize> {
        // Write EOF to the buffer
        let target = buffer.get_mut(..MIN_RECV_MSG_LEN).ok_or_else(|| {
            eprintln!("write_eof: Buffer is too small to write EOF, returning EINVAL.");
            Error::new(EINVAL)
        })?;
        target.fill(0); // Fill the buffer with zeros to indicate EOF
        Ok(MIN_RECV_MSG_LEN)
    }

    fn recvmsg_inner(
        socket: &mut Socket,
        msg_flags: MsgFlags,
        msg_stream: &mut [u8],
    ) -> Result<usize> {
        let (prepared_name_len, prepared_whole_iov_size, _) = read_msghdr_info(msg_stream)?;

        let written_len = socket.serialize_to_msgstream(
            msg_flags,
            msg_stream,
            prepared_name_len,
            prepared_whole_iov_size,
        )?;

        Ok(written_len)
    }

    fn handle_unbind(&mut self, id: usize) -> Result<usize> {
        let path_opt = {
            let socket_rc = self.get_socket(id)?;
            let mut socket = socket_rc.borrow_mut();

            if socket.state != State::Bound {
                return Err(Error::new(EINVAL));
            }

            socket.state = State::Unbound;
            socket.path.take()
        };

        if let Some(path) = path_opt {
            self.socket_paths.remove(&path);
        }

        Ok(0)
    }

    fn handle_get_token(&self, id: usize, payload: &mut [u8]) -> Result<usize> {
        let socket_rc = self.get_socket(id)?;
        let Some(token) = socket_rc.borrow().issued_token else {
            return Err(Error::new(EINVAL));
        };
        let token_bytes = token.to_le_bytes();
        let token_bytes_len = token_bytes.len();
        if payload.len() < token_bytes_len {
            eprintln!(
                "handle_get_token(id: {}): Payload buffer is too small for token.",
                id
            );
            return Err(Error::new(ENOBUFS));
        }
        payload[..token_bytes_len].copy_from_slice(&token_bytes);
        return Ok(token_bytes_len);
    }

    fn handle_get_peer_name(&self, id: usize, payload: &mut [u8]) -> Result<usize> {
        let (_, socket_rc) = self.get_connected_peer(id)?;
        let socket_borrow = socket_rc.borrow();
        match socket_borrow.path.as_ref() {
            Some(path_string) => Self::fpath_inner(path_string, payload),
            None => {
                let empty_path = "".to_string();
                Self::fpath_inner(&empty_path, payload)
            }
        }
    }

    fn accept_connection(
        &mut self,
        listener_socket: &mut Socket,
        client_id: usize,
        ctx: &CallerCtx,
    ) -> Result<Option<OpenResult>> {
        let (new_id, new) = {
            let Ok(client_rc) = self.get_socket(client_id) else {
                return Ok(None); // Client socket has been closed, nothing to accept
            };
            let new_id = self.next_id;
            let mut new = listener_socket.accept(new_id, client_id, ctx)?;

            let mut client_socket = client_rc.borrow_mut();
            client_socket.establish(&mut new, listener_socket.primary_id)?;
            (new_id, new)
        };

        self.next_id += 1;
        self.insert_socket(new_id, Rc::new(RefCell::new(new)));
        self.post_fevent(client_id, EVENT_READ | EVENT_WRITE)?;
        Ok(Some(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::empty(),
        }))
    }

    fn handle_accept(
        &mut self,
        id: usize,
        socket: &mut Socket,
        ctx: &CallerCtx,
    ) -> Result<Option<OpenResult>> {
        let flags = socket.flags;
        if !socket.is_listening() {
            eprintln!(
                "socket_accept: Socket state is not Listening for id: {}",
                id
            );
            return Err(Error::new(EINVAL));
        }
        loop {
            // Try to accept a waiting connection
            let Some(client_id) = socket.awaiting.pop_front() else {
                if flags & O_NONBLOCK == O_NONBLOCK {
                    return Err(Error::new(EAGAIN));
                } else {
                    return Err(Error::new(EWOULDBLOCK));
                }
            };
            return match self.accept_connection(socket, client_id, ctx) {
                Ok(conn) => Ok(conn),
                Err(Error { errno: EAGAIN }) => continue,
                Err(e) => Err(e),
            };
        }
    }

    // Transition a Bound or Unbound socket to the Listening state.
    fn handle_start_listening(&mut self, socket_rc: &Rc<RefCell<Socket>>) -> Result<()> {
        let path = {
            let mut socket = socket_rc.borrow_mut();
            socket.start_listening()?;
            socket.path.clone()
        };

        if let Some(path) = path {
            if let Some(existing_socket_rc) = self.socket_paths.get(&path) {
                if !Rc::ptr_eq(socket_rc, existing_socket_rc) {
                    eprintln!("handle_start_listening: Path '{}' is already in use.", path);
                    return Err(Error::new(EADDRINUSE));
                }
            }
            self.socket_paths.insert(path, socket_rc.clone());
        }
        Ok(())
    }

    // Handle a `dup` call for `b"listen"`.
    // If the socket is not yet listening, it transitions it to the Listening state.
    // If it is already listening, it tries to accept a pending connection.
    fn handle_listen(&mut self, id: usize, ctx: &CallerCtx) -> Result<OpenResult> {
        loop {
            let socket_rc = self.get_socket(id)?.clone();
            let is_listening = socket_rc.borrow().is_listening();

            if is_listening {
                let mut socket = socket_rc.borrow_mut();
                match self.handle_accept(id, &mut socket, ctx)? {
                    Some(result) => return Ok(result),
                    None => continue,
                }
            } else {
                self.handle_start_listening(&socket_rc)?;
                continue;
            }
        }
    }

    fn handle_connect_socketpair(&mut self, id: usize, ctx: &CallerCtx) -> Result<OpenResult> {
        let new_id = self.next_id;
        let flags = self.get_socket(id)?.borrow().flags;
        let mut new = Socket::new(
            new_id,
            None,
            State::Unbound,
            HashSet::new(),
            flags,
            None,
            ctx,
        );
        {
            let socket_rc = self.get_socket(id)?;
            let mut socket = socket_rc.borrow_mut();

            if socket.state == State::Closed {
                eprintln!(
                    "socket_connect_socketpair: Base socket {} is already closed.",
                    id
                );
                return Err(Error::new(EPIPE));
            }
            socket.connect_unchecked(&mut new);
        }

        // smoltcp sends writeable whenever a listener gets a
        // client, we'll do the same too (but also readable,
        // why not)
        self.post_fevent(id, EVENT_READ | EVENT_WRITE)?;

        self.insert_socket(new_id, Rc::new(RefCell::new(new)));

        self.next_id += 1;

        Ok(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::empty(),
        })
    }

    fn handle_recvfd(&mut self, id: usize) -> Result<OpenResult> {
        let socket_rc = self.get_socket(id)?;
        let mut socket = socket_rc.borrow_mut();

        match socket.state {
            State::Established | State::Accepted => {
                let connection = socket.require_connected_connection(MsgFlags::default())?;
                let fd = connection.fds.pop_front().ok_or(Error::new(EWOULDBLOCK))?;
                Ok(OpenResult::OtherScheme { fd })
            }
            State::Closed => Err(Error::new(EPIPE)),
            State::Listening => Err(Error::new(EOPNOTSUPP)),
            _ => Err(Error::new(ENOTCONN)),
        }
    }

    fn write_inner(
        &mut self,
        sender_id: usize,
        receiver_id: usize,
        buf: &[u8],
        ctx: &CallerCtx,
    ) -> Result<usize> {
        {
            let receiver_rc = self.get_socket(receiver_id)?;
            let mut receiver = receiver_rc.borrow_mut();
            let name = receiver.path.clone();

            let connection = if receiver.is_listening() {
                // not accepted yet, park the data to client until accept() handle it
                let receiver_rc = self.get_socket(sender_id)?;
                receiver = receiver_rc.borrow_mut();
                receiver.require_connection()?
            } else {
                receiver.require_connected_connection(MsgFlags::default())?
            };

            if !buf.is_empty() {
                // Send readable only if it wasn't readable before
                let ancillary_data = AncillaryData::new(
                    Credential::new(ctx.pid as i32, ctx.uid as i32, ctx.gid as i32),
                    name,
                );
                let packet = DataPacket::new(buf.to_vec(), ancillary_data);
                connection.packets.push_back(packet);
            }
        }

        self.post_fevent(receiver_id, EVENT_READ)?;

        Ok(buf.len())
    }

    fn sendfd_inner(
        &mut self,
        receiver_id: usize,
        sendfd_request: &SendFdRequest,
    ) -> Result<usize> {
        let mut new_fds = Vec::new();
        new_fds.resize(sendfd_request.num_fds(), usize::MAX);
        if let Err(e) =
            sendfd_request.obtain_fd(&self.socket, FobtainFdFlags::UPPER_TBL, &mut new_fds)
        {
            eprintln!("sendfd_inner: obtain_fd failed with error: {:?}", e);
            return Err(e);
        }
        {
            let receiver_rc = self.get_socket(receiver_id)?;
            let mut receiver = receiver_rc.borrow_mut();

            let connection = receiver.require_connected_connection(MsgFlags::default())?;
            for new_fd in &new_fds {
                connection.fds.push_back(*new_fd);
            }
        }

        self.post_fevent(receiver_id, EVENT_READ)?;

        Ok(new_fds.len())
    }

    fn recvfd_inner(&mut self, recvfd_request: &RecvFdRequest) -> Result<OpenResult> {
        let socket_id = recvfd_request.id();
        let socket_rc = self.get_socket(socket_id)?;
        let mut socket = socket_rc.borrow_mut();

        if recvfd_request.num_fds() == 0 {
            return Ok(OpenResult::OtherSchemeMultiple { num_fds: 0 });
        }

        match socket.state {
            State::Established | State::Accepted => {
                let connection = socket.require_connected_connection(MsgFlags::default())?;

                if connection.fds.len() < recvfd_request.num_fds() {
                    return if connection.is_peer_shutdown {
                        Ok(OpenResult::OtherSchemeMultiple { num_fds: 0 }) // EOF, no data to read
                    } else if (socket.flags as usize) & O_NONBLOCK == O_NONBLOCK {
                        Err(Error::new(EAGAIN))
                    } else {
                        Ok(OpenResult::WouldBlock)
                    };
                }

                let fds: Vec<usize> = connection.fds.drain(..recvfd_request.num_fds()).collect();
                if let Err(e) = recvfd_request.move_fd(&self.socket, FmoveFdFlags::empty(), &fds) {
                    eprintln!("recvfd_inner: move_fd failed with error: {:?}", e);
                    return Err(Error::new(EPROTO));
                }

                Ok(OpenResult::OtherSchemeMultiple {
                    num_fds: recvfd_request.num_fds(),
                })
            }
            State::Closed => Err(Error::new(EPIPE)),
            State::Listening => Err(Error::new(EOPNOTSUPP)),
            _ => Err(Error::new(ENOTCONN)),
        }
    }

    fn read_inner(connection: &mut Connection, buf: &mut [u8], flags: u32) -> Result<usize> {
        let mut total_copied_len = 0;
        let mut user_buf_offset = 0;

        while user_buf_offset < buf.len() {
            let Some(packet) = connection.packets.front_mut() else {
                // No more packets to read
                break;
            };

            let packet_rem_payload = &packet.payload[packet.read_offset..];

            let user_buf_rem_len = buf.len() - user_buf_offset;

            let copied_len = cmp::min(packet_rem_payload.len(), user_buf_rem_len);
            if copied_len == 0 {
                // No more data to read from this packet
                break;
            }
            buf[user_buf_offset..user_buf_offset + copied_len]
                .copy_from_slice(&packet_rem_payload[..copied_len]);

            if packet.read_offset == 0 {
                packet.ancillary_taken = true; // Mark ancillary data as taken
            }

            packet.read_offset += copied_len;
            user_buf_offset += copied_len;
            total_copied_len += copied_len;
            if packet.read_offset >= packet.payload.len() {
                // If the packet is fully read, remove it from the queue
                connection.packets.pop_front();
            }
        }

        if total_copied_len > 0 {
            Ok(total_copied_len)
        } else if connection.is_peer_shutdown {
            Ok(0) // EOF, no data to read
        } else if (flags as usize) & O_NONBLOCK == O_NONBLOCK {
            Err(Error::new(EAGAIN))
        } else {
            Err(Error::new(EWOULDBLOCK))
        }
    }

    fn handle_listening_closure(&mut self, socket_rc: Rc<RefCell<Socket>>) {
        let socket = socket_rc.borrow();
        if let Some(path) = &socket.path {
            self.socket_paths.remove(path);
        }

        if let Some(token) = &socket.issued_token {
            self.socket_tokens.remove(&token);
        }

        // Notify all waiting clients about listener closure
        for client_id in &socket.awaiting {
            if let Ok(client_rc) = self.get_socket(*client_id) {
                {
                    let mut client = client_rc.borrow_mut();
                    client.state = State::Closed;
                }
                let _ = self.post_fevent(*client_id, EVENT_READ);
            }
        }
    }

    fn handle_other_closure(&mut self, socket_rc: Rc<RefCell<Socket>>) {
        // If this is the last reference to the socket, it's safe to remove the socket path.
        let mut socket = socket_rc.borrow_mut();
        if matches!(socket.state, State::Established | State::Accepted) {
            let Ok(connection) = socket.require_connection() else {
                return;
            };
            let Ok(remote_rc) = self.get_socket(connection.peer) else {
                return;
            };
            let remote_id = {
                let mut remote = remote_rc.borrow_mut();
                let Ok(connection) = remote.require_connection() else {
                    return;
                };
                connection.is_peer_shutdown = true;
                remote.primary_id
            };
            let _ = self.post_fevent(remote_id, EVENT_READ);
        }

        if let Some(path) = socket.path.take() {
            // If this is the last reference to the socket, remove the path from the registry
            self.socket_paths.remove(&path);
        }
        if let Some(token) = socket.issued_token {
            self.socket_tokens.remove(&token);
        }
        socket.state = State::Closed;
    }

    fn fpath_inner(path: &String, buf: &mut [u8]) -> Result<usize> {
        FpathWriter::with(buf, "uds_stream", |w| {
            w.push_str(path);
            Ok(())
        })
    }
}

impl<'sock> SchemeSync for UdsStreamScheme<'sock> {
    fn scheme_root(&mut self) -> Result<usize> {
        let new_id = self.next_id;
        self.handles.insert(new_id, Handle::SchemeRoot);
        self.next_id += 1;
        Ok(new_id)
    }
    fn openat(
        &mut self,
        fd: usize,
        path: &str,
        mut flags: usize,
        fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        {
            let Some(handle) = self.handles.get(&fd) else {
                return Err(Error::new(EBADF));
            };
            if !handle.is_scheme_root() {
                eprintln!(
                    "openat(fd: {}, path: '{}'): fd is not an open capability.",
                    fd, path
                );
                return Err(Error::new(EACCES));
            }
        }

        flags |= fcntl_flags as usize;

        let new_id = if path.is_empty() {
            if flags & O_CREAT == O_CREAT {
                self.handle_unnamed_socket(flags, ctx)
            } else {
                eprintln!(
                    "open(path: '{}'): Attempting to open an unnamed socket without O_CREAT.",
                    path
                );
                return Err(Error::new(EINVAL));
            }
        } else {
            eprintln!(
                "open(path: '{}'): Attempting to open a named socket, which is not supported.",
                path
            );
            return Err(Error::new(EINVAL));
        };
        Ok(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::empty(),
        })
    }

    fn call(
        &mut self,
        id: usize,
        payload: &mut [u8],
        metadata: &[u64],
        ctx: &CallerCtx,
    ) -> Result<usize> {
        self.call_inner(id, payload, metadata, ctx)
    }

    fn dup(&mut self, id: usize, buf: &[u8], ctx: &CallerCtx) -> Result<OpenResult> {
        match buf {
            b"listen" => self.handle_listen(id, ctx),
            b"connect" => self.handle_connect_socketpair(id, ctx),
            b"recvfd" => self.handle_recvfd(id),
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        ctx: &CallerCtx,
    ) -> Result<usize> {
        let (receiver_id, _) = self.get_connected_peer(id)?;
        self.write_inner(id, receiver_id, buf, ctx)
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        match self.handles.get(&id).ok_or(Error::new(EBADF))? {
            Handle::SchemeRoot => Ok(Self::fpath_inner(&String::new(), buf)?),
            Handle::Socket(socket_rc) => {
                let socket = socket_rc.borrow();
                let empty = String::new();
                let path = socket.path.as_ref().unwrap_or(&empty);
                Ok(Self::fpath_inner(path, buf)?)
            }
        }
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        self.get_socket(id).and(Ok(()))
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        _offset: u64,
        flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let socket_rc = self.get_socket(id)?;
        let mut socket = socket_rc.borrow_mut();
        match socket.state {
            State::Established | State::Accepted | State::Connecting => {
                let connection = socket.require_connected_connection(MsgFlags::default())?;
                Self::read_inner(connection, buf, flags)
            }
            State::Closed => Ok(0),
            State::Listening => Err(Error::new(EOPNOTSUPP)),
            _ => Err(Error::new(ENOTCONN)),
        }
    }

    fn on_sendfd(&mut self, sendfd_request: &SendFdRequest) -> Result<usize> {
        let id = sendfd_request.id();
        let (receiver_id, _) = self.get_connected_peer(id)?;

        self.sendfd_inner(receiver_id, sendfd_request)
    }

    fn on_recvfd(&mut self, recvfd_request: &RecvFdRequest) -> Result<OpenResult> {
        self.recvfd_inner(recvfd_request)
    }

    fn on_close(&mut self, id: usize) {
        let Some(Handle::Socket(socket_rc)) = self.handles.remove(&id) else {
            return;
        };

        let state = socket_rc.borrow().state;
        match state {
            State::Listening => {
                self.handle_listening_closure(socket_rc);
            }
            _ => {
                self.handle_other_closure(socket_rc);
            }
        }
    }

    fn fcntl(&mut self, id: usize, cmd: usize, arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let socket_rc = self.get_socket(id)?;
        let mut socket = socket_rc.borrow_mut();
        match cmd {
            F_GETFL => Ok(socket.flags),
            F_SETFL => {
                socket.flags = arg;
                Ok(0)
            }
            _ => Err(Error::new(EINVAL)),
        }
    }

    fn fevent(&mut self, id: usize, flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        let socket_rc = self.get_socket(id)?;
        let socket = socket_rc.borrow();
        Ok(socket.events() & flags)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        self.get_socket(id)?;

        *stat = Stat {
            st_mode: MODE_SOCK,
            ..Default::default()
        };

        Ok(())
    }
}
