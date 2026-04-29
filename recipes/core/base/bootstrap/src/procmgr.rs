use core::cell::RefCell;
use core::cmp;
use core::mem::size_of;
use core::num::{NonZeroU8, NonZeroUsize};
use core::ops::Deref;
use core::ptr::NonNull;
use core::str::FromStr;
use core::sync::atomic::Ordering;
use core::task::Poll::{self, *};

use alloc::collections::VecDeque;
use alloc::collections::btree_map::BTreeMap;
use alloc::rc::{Rc, Weak};
use alloc::vec;
use alloc::vec::Vec;

use arrayvec::ArrayString;
use hashbrown::hash_map::{Entry, OccupiedEntry, VacantEntry};
use hashbrown::{DefaultHashBuilder, HashMap, HashSet};

use libredox::protocol::{
    ProcCall, ProcKillTarget, ProcMeta, RtSigInfo, SIGCHLD, SIGCONT, SIGHUP, SIGKILL, SIGSTOP,
    SIGTSTP, SIGTTIN, SIGTTOU, ThreadCall, WaitFlags,
};
use redox_rt::proc::FdGuard;
use redox_scheme::scheme::{IntoTag, Op, OpCall};
use redox_scheme::{
    CallerCtx, Id, OpenResult, Request, RequestKind, Response, SendFdRequest, SignalBehavior,
    Socket, Tag,
};
use slab::Slab;
use syscall::schemev2::NewFdFlags;
use syscall::{
    CallFlags, ContextStatus, ContextVerb, CtxtStsBuf, EACCES, EAGAIN, EBADF, EBADFD, ECANCELED,
    ECHILD, EEXIST, EINTR, EINVAL, ENOENT, ENOSYS, EOPNOTSUPP, EOWNERDEAD, EPERM, ERESTART, ESRCH,
    EWOULDBLOCK, Error, Event, EventFlags, FobtainFdFlags, MapFlags, O_ACCMODE, O_CREAT, O_RDONLY,
    PAGE_SIZE, ProcSchemeAttrs, Result, SenderInfo, SetSighandlerData, SigProcControl, Sigcontrol,
    sig_bit,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum VirtualId {
    KernelId(Id),
    // TODO: slab or something for better ID reuse
    InternalId(u64),
}

pub fn run(write_fd: FdGuard, socket: Socket, auth: FdGuard, event: FdGuard) -> ! {
    // TODO?
    let socket_ident = socket.inner().raw();

    let queue = RawEventQueue::new(event.as_raw_fd()).expect("failed to create event queue");
    drop(event);

    queue
        .subscribe(socket.inner().raw(), socket_ident, EventFlags::EVENT_READ)
        .expect("failed to listen to scheme socket events");

    let mut scheme = ProcScheme::new(auth, &queue);

    // send open-capability to bootstrap
    let new_id = scheme.handles.insert(Handle::SchemeRoot);
    let cap_fd = socket
        .create_this_scheme_fd(0, new_id, 0, 0)
        .expect("failed to issue procmgr root fd");

    log::debug!("process manager started");
    let _ = syscall::call_wo(
        write_fd.as_raw_fd(),
        &cap_fd.to_ne_bytes(),
        CallFlags::FD,
        &[],
    );
    drop(write_fd);

    let mut states = HashMap::<VirtualId, PendingState, DefaultHashBuilder>::new();
    let mut awoken = VecDeque::<VirtualId>::new();
    let mut new_awoken = VecDeque::new();

    'outer: loop {
        log::trace!("AWOKEN {awoken:#?}");
        while !awoken.is_empty() || !new_awoken.is_empty() {
            awoken.append(&mut new_awoken);
            for awoken in awoken.drain(..) {
                //log::trace!("ALL STATES {states:#?}, AWOKEN {awoken:#?}");
                let Entry::Occupied(state) = states.entry(awoken) else {
                    continue;
                };
                match scheme.work_on(state, &mut new_awoken) {
                    Ready(resp) => loop {
                        match socket.write_response(resp, SignalBehavior::Interrupt) {
                            Ok(false) => break 'outer,
                            Ok(_) => break,
                            Err(err) if err.errno == EINTR => continue,
                            Err(err) => {
                                panic!(
                                    "bootstrap: failed to write scheme response to kernel: {err}"
                                )
                            }
                        }
                    },
                    Pending => continue,
                }
            }
        }
        // TODO: multiple events?
        let event = queue.next_event().expect("failed to get next event");

        if event.data == socket_ident {
            'reqs: loop {
                let req = loop {
                    match socket.next_request(SignalBehavior::Interrupt) {
                        Ok(None) => break 'outer,
                        Ok(Some(req)) => break req,
                        Err(e) if e.errno == EINTR => continue,
                        // spurious event
                        Err(e) if e.errno == EWOULDBLOCK || e.errno == EAGAIN => break 'reqs,
                        Err(other) => {
                            panic!("bootstrap: failed to read scheme request from kernel: {other}")
                        }
                    }
                };
                log::trace!("REQ{req:#?}");
                let Ready(resp) =
                    handle_scheme(req, &socket, &mut scheme, &mut states, &mut awoken)
                else {
                    continue 'reqs;
                };
                loop {
                    match socket.write_response(resp, SignalBehavior::Interrupt) {
                        Ok(false) => break 'outer,
                        Ok(_) => break,
                        Err(err) if err.errno == EINTR => continue,
                        Err(err) => {
                            panic!("bootstrap: failed to write scheme response to kernel: {err}")
                        }
                    }
                }
            }
        } else if let Some(thread) = scheme.thread_lookup.get(&event.data) {
            let Some(thread_rc) = thread.upgrade() else {
                log::trace!("DEAD THREAD EVENT FROM {}", event.data,);
                continue;
            };
            let thread = thread_rc.borrow();
            let pid = thread.pid;
            let Some(proc_rc) = scheme.processes.get(&pid) else {
                // TODO(err)?
                continue;
            };
            let mut proc = proc_rc.borrow_mut();
            log::trace!("THREAD EVENT FROM {}, {}", event.data, thread.pid.0);
            let mut sts_buf = CtxtStsBuf::default();
            thread.status_hndl.read(&mut sts_buf).unwrap();

            let status = if sts_buf.status == ContextStatus::Dead as usize {
                // dont-care, already called explicit exit()
                0
            } else if sts_buf.status == ContextStatus::ForceKilled as usize {
                (SIGKILL << 8) as u16
            } else if sts_buf.status == ContextStatus::UnhandledExcp as usize {
                // TODO: translate arch-specific exception kind
                // TODO: generate coredump (or let some other process do that)
                // into signal (SIGSEGV, SIGBUS, SIGILL, SIGFPE)
                1
            } else {
                // spurious event
                continue;
            };

            log::trace!("--THREAD DIED {}, {}", event.data, thread.pid.0);

            if let Err(err) = scheme.queue.unsubscribe(event.data, event.data) {
                log::error!("failed to unsubscribe from fd {}: {err}", event.data);
            }
            scheme.thread_lookup.remove(&event.data);
            proc.threads.retain(|rc| !Rc::ptr_eq(rc, &thread_rc));

            if matches!(proc.status, ProcessStatus::Exiting { .. }) {
                log::trace!("WAKING UP {}", proc.awaiting_threads_term.len(),);
                awoken.extend(proc.awaiting_threads_term.drain(..)); // TODO(opt)
            } else if proc.threads.is_empty() {
                let internal_id = scheme.next_internal_id;
                scheme.next_internal_id += 1;
                let Entry::Vacant(entry) = states.entry(VirtualId::InternalId(internal_id)) else {
                    log::error!("internal ID reuse!");
                    continue;
                };
                drop(thread);
                drop(proc);
                let Pending = scheme.on_exit_start(pid, status, entry, &mut awoken, None) else {
                    unreachable!("not possible with tag=None");
                };
            }
        } else {
            log::warn!("TODO: UNKNOWN EVENT {event:?}");
        }
    }

    unreachable!()
}
fn handle_scheme<'a>(
    req: Request,
    socket: &'a Socket,
    scheme: &mut ProcScheme<'a>,
    states: &mut HashMap<VirtualId, PendingState>,
    awoken: &mut VecDeque<VirtualId>,
) -> Poll<Response> {
    match req.kind() {
        RequestKind::Call(req) => {
            let caller = req.caller();
            let req_id = VirtualId::KernelId(req.request_id());
            let op = match req.op() {
                Ok(op) => op,
                Err(req) => return Response::ready_err(ENOSYS, req),
            };
            match op {
                Op::OpenAt(op) => Ready(Response::open_dup_like(
                    scheme.on_openat(op.fd, op.path(), *op.flags(), op.fcntl_flags, &caller),
                    op,
                )),
                Op::Dup(op) => Ready(Response::open_dup_like(scheme.on_dup(op.fd, op.buf()), op)),
                Op::Read(mut op) => Ready(Response::new(
                    scheme.on_read(op.fd, op.offset, op.buf()),
                    op,
                )),
                Op::Call(op) => scheme.on_call(
                    {
                        // TODO: cleanup
                        states.remove(&req_id);
                        if let Entry::Vacant(entry) = states.entry(req_id) {
                            entry
                        } else {
                            unreachable!()
                        }
                    },
                    op,
                    awoken,
                ),
                Op::Fpath(mut op) => {
                    //TODO: fill in useful path?
                    let buf = op.buf();
                    let scheme_path = b"/scheme/proc/";
                    let scheme_bytes = core::cmp::min(scheme_path.len(), buf.len());
                    buf[..scheme_bytes].copy_from_slice(&scheme_path[..scheme_bytes]);
                    Response::ready_ok(scheme_bytes, op)
                }
                Op::Fsize { req, fd } => {
                    if let Handle::Ps(b) = &scheme.handles[fd] {
                        Response::ready_ok(b.len(), req)
                    } else {
                        Response::ready_err(EOPNOTSUPP, req)
                    }
                }
                Op::Fstat(mut op) => {
                    if let Handle::Ps(b) = &scheme.handles[op.fd] {
                        op.buf().st_size = b.len() as _;
                        op.buf().st_mode = syscall::MODE_FILE | 0o444;
                        Response::ready_ok(0, op)
                    } else {
                        Response::ready_err(EOPNOTSUPP, op)
                    }
                }
                _ => {
                    log::trace!("UNKNOWN: {op:?}");
                    Response::ready_err(ENOSYS, op)
                }
            }
        }
        RequestKind::Cancellation(req) => {
            if let Entry::Occupied(state) = states.entry(VirtualId::KernelId(req.id)) {
                match state.remove() {
                    PendingState::AwaitingStatusChange { op, .. } => {
                        Response::ready_err(ECANCELED, op)
                    }
                    // TODO: Test this by calling exit() on behalf of another process using the IPC
                    // call Exit, then cancel. Keep in mind this won't cancel the underlying exit, just
                    // detach the waiter from it.
                    PendingState::AwaitingThreadsTermination(pid, tag) => {
                        let resp = if let Some(tag) = tag {
                            Ready(Response::err(ECANCELED, tag))
                        } else {
                            Pending
                        };

                        let vid = VirtualId::InternalId(scheme.next_internal_id);
                        scheme.next_internal_id += 1;
                        states.insert(vid, PendingState::AwaitingThreadsTermination(pid, None));
                        awoken.push_back(vid);

                        resp
                    }
                    PendingState::Placeholder => {
                        log::warn!("State {:?} was placeholder!", req.id);
                        Pending
                    }
                }
            } else {
                log::warn!("Cancellation for unknown id {:?}", req.id);
                Pending
            }
        }
        RequestKind::OnClose { id } => {
            scheme.on_close(id);
            // no response associated
            Pending
        }
        RequestKind::SendFd(req) => Ready(scheme.on_sendfd(socket, req)),

        // ignore
        _ => Pending,
    }
}
#[derive(Debug)]
enum PendingState {
    AwaitingStatusChange {
        waiter: ProcessId,
        target: WaitpidTarget,
        flags: WaitFlags,
        op: OpCall,
    },
    AwaitingThreadsTermination(ProcessId, Option<Tag>),
    Placeholder,
}
/*impl IntoTag for PendingState {
    fn into_tag(self) -> Tag {
        match self {
            Self::AwaitingThreadsTermination(_, tag) => tag,
            Self::AwaitingStatusChange { op, .. } => op.into_tag(),
            Self::Placeholder => unreachable!(),
        }
    }
}*/

#[derive(Debug)]
pub struct Page<T> {
    ptr: NonNull<T>,
    off: u16,
}
impl<T> Page<T> {
    pub fn map(fd: &FdGuard, req_offset: usize, displacement: u16) -> Result<Self> {
        Ok(Self {
            off: displacement,
            ptr: NonNull::new(unsafe {
                syscall::fmap(
                    fd.as_raw_fd(),
                    &syscall::Map {
                        offset: req_offset,
                        size: PAGE_SIZE,
                        flags: MapFlags::PROT_READ | MapFlags::PROT_WRITE | MapFlags::MAP_SHARED,
                        address: 0,
                    },
                )? as *mut T
            })
            .unwrap(),
        })
    }
}
impl<T> Deref for Page<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.ptr.as_ptr().byte_add(self.off.into()) }
    }
}
impl<T> Drop for Page<T> {
    fn drop(&mut self) {
        unsafe {
            let _ = syscall::funmap(self.ptr.as_ptr() as usize, PAGE_SIZE);
        }
    }
}

const NAME_CAPAC: usize = 32;

#[derive(Debug)]
struct Process {
    threads: Vec<Rc<RefCell<Thread>>>,
    pid: ProcessId,
    ppid: ProcessId,
    pgid: ProcessId,
    sid: ProcessId,
    name: ArrayString<NAME_CAPAC>,
    prio: u32,

    ruid: u32,
    euid: u32,
    suid: u32,
    rgid: u32,
    egid: u32,
    sgid: u32,

    status: ProcessStatus,
    disabled_setpgid: bool,

    awaiting_threads_term: Vec<VirtualId>,

    waitpid: BTreeMap<WaitpidKey, (ProcessId, WaitpidStatus)>,
    waitpid_waiting: VecDeque<VirtualId>,

    sig_pctl: Option<Page<SigProcControl>>,
    rtqs: Vec<VecDeque<RtSigInfo>>,
}
#[derive(Copy, Clone, Debug)]
struct WaitpidKey {
    pid: Option<ProcessId>,
    pgid: Option<ProcessId>,
}

// TODO: Is this valid? (transitive?)
impl Ord for WaitpidKey {
    fn cmp(&self, other: &WaitpidKey) -> cmp::Ordering {
        // If both have pid set, compare that
        if let Some(s_pid) = self.pid {
            if let Some(o_pid) = other.pid {
                return s_pid.cmp(&o_pid);
            }
        }

        // If both have pgid set, compare that
        if let Some(s_pgid) = self.pgid {
            if let Some(o_pgid) = other.pgid {
                return s_pgid.cmp(&o_pgid);
            }
        }

        // If either has pid set, it is greater
        if self.pid.is_some() {
            return cmp::Ordering::Greater;
        }

        if other.pid.is_some() {
            return cmp::Ordering::Less;
        }

        // If either has pgid set, it is greater
        if self.pgid.is_some() {
            return cmp::Ordering::Greater;
        }

        if other.pgid.is_some() {
            return cmp::Ordering::Less;
        }

        // If all pid and pgid are None, they are equal
        cmp::Ordering::Equal
    }
}

impl PartialOrd for WaitpidKey {
    fn partial_cmp(&self, other: &WaitpidKey) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for WaitpidKey {
    fn eq(&self, other: &WaitpidKey) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl Eq for WaitpidKey {}
#[derive(Debug, Clone, Copy)]
enum ProcessStatus {
    PossiblyRunnable,
    Stopped(usize),
    Exiting {
        signal: Option<NonZeroU8>,
        status: u8,
    },
    Exited {
        signal: Option<NonZeroU8>,
        status: u8,
    },
}
#[derive(Debug)]
struct Thread {
    fd: FdGuard,
    status_hndl: FdGuard,
    pid: ProcessId,
    sig_ctrl: Option<Page<Sigcontrol>>,
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ProcessId(usize);

const INIT_PID: ProcessId = ProcessId(1);

struct ProcScheme<'a> {
    processes: HashMap<ProcessId, Rc<RefCell<Process>>, DefaultHashBuilder>,
    groups: HashMap<ProcessId, Rc<RefCell<Pgrp>>>,
    sessions: HashSet<ProcessId, DefaultHashBuilder>,
    handles: Slab<Handle>,

    thread_lookup: HashMap<usize, Weak<RefCell<Thread>>>,

    next_internal_id: u64,

    init_claimed: bool,
    next_id: ProcessId,

    queue: &'a RawEventQueue,
    auth: FdGuard,
}
#[derive(Debug, Default)]
struct Pgrp {
    processes: Vec<Weak<RefCell<Process>>>,
}
#[derive(Clone, Copy, Debug)]
enum WaitpidStatus {
    Continued,
    Stopped {
        signal: NonZeroU8,
    },
    Terminated {
        signal: Option<NonZeroU8>,
        status: u8,
    },
}

#[derive(Debug)]
enum Handle {
    Init,
    Proc(ProcessId),

    // Needs to be weak so the thread is owned only by the process. Otherwise there would be a
    // cyclic reference since the underlying context's file table almost certainly contains the
    // thread fd itself, linked to this handle.
    Thread(Weak<RefCell<Thread>>),

    // TODO: stateless API, perhaps using intermediate daemon for providing a file-like API
    Ps(Vec<u8>),

    // A handle that grants the holder the capability to obtain process credentials.
    ProcCredsCapability,

    // A handle that grants the holder the capability to open process scheme resource.
    SchemeRoot,
}

#[derive(Clone, Copy, Debug)]
enum WaitpidTarget {
    SingleProc(ProcessId),
    ProcGroup(ProcessId),
    AnyChild,
    AnyGroupMember,
}
// TODO(feat): Add 'syscall' backend for redox-event so it can act both as library-ABI frontend and
// backend
struct RawEventQueue(FdGuard);
impl RawEventQueue {
    pub fn new(cap_fd: usize) -> Result<Self> {
        syscall::openat(cap_fd, "", O_CREAT, 0)
            .map(FdGuard::new)
            .map(Self)
    }
    pub fn subscribe(&self, fd: usize, ident: usize, flags: EventFlags) -> Result<()> {
        self.0.write(&Event {
            id: fd,
            data: ident,
            flags,
        })?;
        Ok(())
    }
    pub fn unsubscribe(&self, fd: usize, ident: usize) -> Result<()> {
        self.subscribe(fd, ident, EventFlags::empty())
    }
    pub fn next_event(&self) -> Result<Event> {
        let mut event = Event::default();
        let read = self.0.read(&mut event)?;
        assert_eq!(
            read,
            size_of::<Event>(),
            "event queue EOF currently undefined"
        );
        Ok(event)
    }
}

impl<'a> ProcScheme<'a> {
    pub fn new(auth: FdGuard, queue: &'a RawEventQueue) -> ProcScheme<'a> {
        ProcScheme {
            processes: HashMap::new(),
            groups: HashMap::new(),
            sessions: HashSet::new(),
            thread_lookup: HashMap::new(),
            handles: Slab::new(),
            init_claimed: false,
            next_id: ProcessId(2),
            next_internal_id: 1,
            queue,
            auth,
        }
    }
    fn new_id(&mut self) -> ProcessId {
        let id = self.next_id;
        self.next_id.0 += 1;
        id
    }
    fn on_sendfd(&mut self, socket: &Socket, req: SendFdRequest) -> Response {
        match self.handles[req.id()] {
            ref mut st @ Handle::Init => {
                let mut fd_out = usize::MAX;
                if let Err(e) = req.obtain_fd(
                    socket,
                    FobtainFdFlags::empty(),
                    core::slice::from_mut(&mut fd_out),
                ) {
                    return Response::new(Err(e), req);
                };
                let fd = FdGuard::new(fd_out);

                // TODO: Use global thread id etc. rather than reusing fd for identifier?
                self.queue
                    .subscribe(fd_out, fd_out, EventFlags::EVENT_READ)
                    .expect("TODO");
                let status_hndl = fd
                    .dup(alloc::format!("auth-{}-status", self.auth.as_raw_fd()).as_bytes())
                    .expect("TODO");

                let thread = Rc::new(RefCell::new(Thread {
                    fd,
                    status_hndl,
                    pid: INIT_PID,
                    sig_ctrl: None,
                }));
                let thread_weak = Rc::downgrade(&thread);
                let process = Rc::new(RefCell::new(Process {
                    threads: vec![thread],
                    pid: INIT_PID,
                    ppid: INIT_PID,
                    sid: INIT_PID,
                    pgid: INIT_PID,
                    ruid: 0,
                    euid: 0,
                    suid: 0,
                    rgid: 0,
                    egid: 0,
                    sgid: 0,
                    name: ArrayString::<32>::from_str("[init]").unwrap(),
                    prio: 20,

                    status: ProcessStatus::PossiblyRunnable,
                    disabled_setpgid: false,
                    awaiting_threads_term: Vec::new(),
                    waitpid: BTreeMap::new(),
                    waitpid_waiting: VecDeque::new(),

                    sig_pctl: None,
                    rtqs: Vec::new(),
                }));
                self.groups.insert(
                    INIT_PID,
                    Rc::new(RefCell::new(Pgrp {
                        processes: vec![Rc::downgrade(&process)],
                    })),
                );
                self.processes.insert(INIT_PID, process);
                self.sessions.insert(INIT_PID);

                self.thread_lookup.insert(fd_out, thread_weak);

                *st = Handle::Proc(INIT_PID);
                Response::ok(0, req)
            }
            _ => Response::err(EBADF, req),
        }
    }
    fn fork(&mut self, parent_pid: ProcessId) -> Result<ProcessId> {
        let child_pid = self.new_id();

        let proc_guard = self.processes.get(&parent_pid).ok_or(Error::new(EBADFD))?;

        let Process {
            pgid,
            sid,
            ruid,
            euid,
            suid,
            rgid,
            egid,
            sgid,
            name,
            prio,
            ..
        } = *proc_guard.borrow();

        let new_ctxt_fd = self.auth.dup(b"new-context")?;
        let status_fd =
            new_ctxt_fd.dup(alloc::format!("auth-{}-status", self.auth.as_raw_fd()).as_bytes())?;

        let thread_ident = new_ctxt_fd.as_raw_fd();
        self.queue
            .subscribe(thread_ident, thread_ident, EventFlags::EVENT_READ)
            .expect("TODO");

        let thread = Rc::new(RefCell::new(Thread {
            fd: new_ctxt_fd,
            status_hndl: status_fd,
            pid: child_pid,
            sig_ctrl: None, // TODO
        }));
        let thread_weak = Rc::downgrade(&thread);
        let new_process = Rc::new(RefCell::new(Process {
            threads: vec![thread],
            ppid: parent_pid,
            pid: child_pid,
            pgid,
            sid,
            ruid,
            euid,
            suid,
            rgid,
            egid,
            sgid,
            name,
            prio,

            status: ProcessStatus::PossiblyRunnable,
            disabled_setpgid: false,
            awaiting_threads_term: Vec::new(),

            waitpid: BTreeMap::new(),
            waitpid_waiting: VecDeque::new(),

            sig_pctl: None, // TODO
            rtqs: Vec::new(),
        }));
        if let Err(err) = new_process
            .borrow_mut()
            .sync_kernel_attrs(&self.auth)
        {
            log::warn!("Failed to set kernel attrs when forking: {err}");
        }

        if let Some(group) = self.groups.get(&pgid) {
            group
                .borrow_mut()
                .processes
                .push(Rc::downgrade(&new_process));
        }

        self.processes.insert(child_pid, new_process);
        self.thread_lookup.insert(thread_ident, thread_weak);
        Ok(child_pid)
    }
    fn new_thread(&mut self, pid: ProcessId) -> Result<Rc<RefCell<Thread>>> {
        // TODO: deduplicate code with fork
        let proc_rc = self.processes.get_mut(&pid).ok_or(Error::new(EBADFD))?;
        let mut proc = proc_rc.borrow_mut();

        let ctxt_fd = self.auth.dup(b"new-context")?;

        // TODO: sync_kernel_attrs?
        let attr_fd =
            ctxt_fd.dup(alloc::format!("auth-{}-attrs", self.auth.as_raw_fd()).as_bytes())?;
        attr_fd.write(&ProcSchemeAttrs {
            pid: pid.0 as u32,
            euid: proc.euid,
            egid: proc.egid,
            prio: proc.prio,
            debug_name: arraystring_to_bytes(proc.name),
        })?;

        let status_hndl =
            ctxt_fd.dup(alloc::format!("auth-{}-status", self.auth.as_raw_fd()).as_bytes())?;

        let ident = ctxt_fd.as_raw_fd();
        self.queue
            .subscribe(ident, ident, EventFlags::EVENT_READ)
            .expect("TODO");

        let thread = Rc::new(RefCell::new(Thread {
            fd: ctxt_fd,
            status_hndl,
            pid,
            sig_ctrl: None,
        }));
        let thread_weak = Rc::downgrade(&thread);
        proc.threads.push(Rc::clone(&thread));
        self.thread_lookup.insert(ident, thread_weak);
        Ok(thread)
    }
    fn on_openat(
        &mut self,
        fd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        match self.handles[fd] {
            Handle::SchemeRoot => {}
            _ => return Err(Error::new(EACCES)),
        };
        let path = path.trim_start_matches('/');
        Ok(match path {
            "init" => {
                if core::mem::replace(&mut self.init_claimed, true) {
                    return Err(Error::new(EEXIST));
                }
                OpenResult::ThisScheme {
                    number: self.handles.insert(Handle::Init),
                    flags: NewFdFlags::empty(),
                }
            }
            "ps" => {
                let data = self.ps_data(ctx)?;
                OpenResult::ThisScheme {
                    number: self.handles.insert(Handle::Ps(data)),
                    flags: NewFdFlags::POSITIONED,
                }
            }
            "proc-creds-capability" => {
                if ctx.uid != 0 {
                    return Err(Error::new(EACCES));
                }
                if flags & O_ACCMODE != O_RDONLY {
                    return Err(Error::new(EINVAL));
                }
                OpenResult::ThisScheme {
                    number: self.handles.insert(Handle::ProcCredsCapability),
                    flags: NewFdFlags::empty(),
                }
            }

            _ => return Err(Error::new(ENOENT)),
        })
    }
    fn read_process_metadata(&self, pid: ProcessId, buf: &mut [u8]) -> Result<usize> {
        let proc_rc = self.processes.get(&pid).ok_or(Error::new(ESRCH))?;
        let process = proc_rc.borrow();
        let metadata = ProcMeta {
            pid: pid.0 as u32,
            pgid: process.pgid.0 as u32,
            ppid: process.ppid.0 as u32,
            ruid: process.ruid,
            euid: process.euid,
            suid: process.suid,
            rgid: process.rgid,
            egid: process.egid,
            sgid: process.sgid,
            ens: 1,
            rns: 1,
        };
        *buf.get_mut(..size_of::<ProcMeta>())
            .and_then(|b| plain::from_mut_bytes(b).ok())
            .ok_or(Error::new(EBADF))? = metadata;
        Ok(size_of::<ProcMeta>())
    }
    fn on_read(&mut self, id: usize, offset: u64, buf: &mut [u8]) -> Result<usize> {
        match self.handles[id] {
            Handle::Proc(pid) => self.read_process_metadata(pid, buf),
            Handle::Ps(ref src_buf) => {
                let src_buf = usize::try_from(offset)
                    .ok()
                    .and_then(|o| src_buf.get(o..))
                    .unwrap_or(&[]);
                let len = src_buf.len().min(buf.len());
                buf[..len].copy_from_slice(&src_buf[..len]);
                Ok(len)
            }
            Handle::Init | Handle::Thread(_) | Handle::ProcCredsCapability | Handle::SchemeRoot => {
                return Err(Error::new(EBADF));
            }
        }
    }
    fn on_dup(&mut self, old_id: usize, buf: &[u8]) -> Result<OpenResult> {
        log::trace!("Dup request");
        match self.handles[old_id] {
            Handle::Proc(pid) => match buf {
                b"fork" => {
                    log::trace!("Forking {pid:?}");
                    let child_pid = self.fork(pid)?;
                    Ok(OpenResult::ThisScheme {
                        number: self.handles.insert(Handle::Proc(child_pid)),
                        flags: NewFdFlags::empty(),
                    })
                }
                b"new-thread" => {
                    let thread = self.new_thread(pid)?;
                    Ok(OpenResult::ThisScheme {
                        number: self.handles.insert(Handle::Thread(Rc::downgrade(&thread))),
                        flags: NewFdFlags::empty(),
                    })
                }
                w if w.starts_with(b"thread-") => {
                    let idx = core::str::from_utf8(&w["thread-".len()..])
                        .ok()
                        .and_then(|s| s.parse::<usize>().ok())
                        .ok_or(Error::new(EINVAL))?;
                    let process = self.processes.get(&pid).ok_or(Error::new(EBADFD))?.borrow();
                    let thread = Rc::downgrade(process.threads.get(idx).ok_or(Error::new(ENOENT))?);

                    return Ok(OpenResult::ThisScheme {
                        number: self.handles.insert(Handle::Thread(thread)),
                        flags: NewFdFlags::empty(),
                    });
                }
                _ => return Err(Error::new(EINVAL)),
            },
            Handle::Thread(ref thread_weak) => {
                let thread_rc = thread_weak.upgrade().ok_or(Error::new(EOWNERDEAD))?;
                let thread = thread_rc.borrow();

                // By forwarding all dup calls to the kernel, this fd is now effectively the same
                // as the underlying fd since that fd can't do anything itself.
                Ok(OpenResult::OtherScheme {
                    fd: thread.fd.dup(buf)?.take(),
                })
            }
            Handle::Init | Handle::Ps(_) | Handle::ProcCredsCapability | Handle::SchemeRoot => {
                Err(Error::new(EBADF))
            }
        }
    }
    fn on_call(
        &mut self,
        state: VacantEntry<VirtualId, PendingState, DefaultHashBuilder>,
        mut op: OpCall,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Poll<Response> {
        let id = op.fd;
        let (payload, metadata) = op.payload_and_metadata();
        match self.handles[id] {
            Handle::Init => Response::ready_err(EBADF, op),
            Handle::Thread(ref thr_weak) => {
                let Some(thr) = thr_weak.upgrade() else {
                    return Response::ready_err(EOWNERDEAD, op);
                };
                let Some(verb) = ThreadCall::try_from_raw(metadata[0] as usize) else {
                    return Response::ready_err(EINVAL, op);
                };
                match verb {
                    ThreadCall::SyncSigTctl => Ready(Response::new(
                        Self::on_sync_sigtctl(&mut *thr.borrow_mut()).map(|()| 0),
                        op,
                    )),
                    ThreadCall::SignalThread => Ready(Response::new(
                        self.on_kill_thread(&thr, metadata[1] as u8, awoken)
                            .map(|()| 0),
                        op,
                    )),
                }
            }
            Handle::Proc(fd_pid) => {
                let Some(verb) = ProcCall::try_from_raw(metadata[0] as usize) else {
                    log::trace!("Invalid proc call: {metadata:?}");
                    return Response::ready_err(EINVAL, op);
                };
                match verb {
                    ProcCall::Exit => self.on_exit_start(
                        fd_pid,
                        metadata[1] as u16,
                        state,
                        awoken,
                        Some(op.into_tag()),
                    ),
                    ProcCall::Waitpid | ProcCall::Waitpgid => {
                        let req_pid = ProcessId(metadata[1] as usize);
                        let target = match (verb, metadata[1] == 0) {
                            (ProcCall::Waitpid, true) => WaitpidTarget::AnyChild,
                            (ProcCall::Waitpid, false) => WaitpidTarget::SingleProc(req_pid),
                            (ProcCall::Waitpgid, true) => WaitpidTarget::AnyGroupMember,
                            (ProcCall::Waitpgid, false) => WaitpidTarget::ProcGroup(req_pid),
                            _ => unreachable!(),
                        };
                        let flags = match WaitFlags::from_bits(metadata[2] as usize) {
                            Some(fl) => fl,
                            None => {
                                return Response::ready_err(EINVAL, op);
                            }
                        };
                        let state = state.insert_entry(PendingState::AwaitingStatusChange {
                            waiter: fd_pid,
                            target,
                            flags,
                            op,
                        });
                        self.work_on(state, awoken)
                    }
                    ProcCall::Setpgid => {
                        let target_pid = NonZeroUsize::new(metadata[1] as usize)
                            .map_or(fd_pid, |n| ProcessId(n.get()));

                        let new_pgid = NonZeroUsize::new(metadata[2] as usize)
                            .map_or(target_pid, |n| ProcessId(n.get()));
                        if new_pgid.0 == usize::wrapping_neg(1) {
                            Ready(Response::new(
                                self.on_getpgid(fd_pid, target_pid).map(|ProcessId(p)| p),
                                op,
                            ))
                        } else {
                            Ready(Response::new(
                                self.on_setpgid(fd_pid, target_pid, new_pgid, awoken)
                                    .map(|()| 0),
                                op,
                            ))
                        }
                    }
                    ProcCall::Getsid => {
                        let req_pid = NonZeroUsize::new(metadata[1] as usize)
                            .map_or(fd_pid, |n| ProcessId(n.get()));
                        Ready(Response::new(
                            self.on_getsid(fd_pid, req_pid).map(|ProcessId(s)| s),
                            op,
                        ))
                    }
                    ProcCall::Getppid => Ready(Response::new(
                        self.on_getppid(fd_pid).map(|ProcessId(p)| p),
                        op,
                    )),
                    ProcCall::Setsid => Ready(Response::new(
                        self.on_setsid(fd_pid, awoken).map(|()| 0),
                        op,
                    )),
                    ProcCall::SetResugid => Ready(Response::new(
                        self.on_setresugid(fd_pid, payload).map(|()| 0),
                        op,
                    )),
                    ProcCall::Kill | ProcCall::Sigq => {
                        let (payload, metadata) = op.payload_and_metadata();
                        let target = ProcKillTarget::from_raw(metadata[1] as usize);
                        let Some(signal) = u8::try_from(metadata[2]).ok().filter(|s| *s <= 64)
                        else {
                            return Response::ready_err(EINVAL, op);
                        };
                        let mode = match verb {
                            ProcCall::Kill => KillMode::Idempotent,
                            ProcCall::Sigq => KillMode::Queued({
                                let mut buf = [0_u8; size_of::<RtSigInfo>()];
                                if payload.len() != size_of::<RtSigInfo>() {
                                    return Response::ready_err(EINVAL, op);
                                }
                                buf.copy_from_slice(payload);
                                *plain::from_bytes(&buf).unwrap()
                            }),
                            _ => unreachable!(),
                        };

                        Ready(Response::new(
                            self.on_kill(fd_pid, target, signal, mode, awoken)
                                .map(|()| 0),
                            op,
                        ))
                    }
                    ProcCall::SyncSigPctl => {
                        Ready(Response::new(self.on_sync_sigpctl(fd_pid).map(|()| 0), op))
                    }
                    ProcCall::Sigdeq => Ready(Response::new(
                        self.on_sigdeq(fd_pid, payload).map(|()| 0),
                        op,
                    )),
                    ProcCall::Rename => Ready(Response::new(
                        self.on_proc_rename(fd_pid, payload).map(|()| 0),
                        op,
                    )),
                    ProcCall::DisableSetpgid => {
                        if let Some(proc) = self.processes.get(&fd_pid) {
                            proc.borrow_mut().disabled_setpgid = true;
                            Response::ready_ok(0, op)
                        } else {
                            Response::ready_err(ESRCH, op)
                        }
                    }
                    ProcCall::GetProcCredentials => Response::ready_err(EACCES, op),

                    // setrens is no longer implemented as procmgr call
                    // FIXME remove this ProcCall variant
                    ProcCall::Setrens => Response::ready_err(EINVAL, op),
                    ProcCall::SetProcPriority => {
                        let target_pid = NonZeroUsize::new(metadata[1] as usize).map_or(fd_pid, |n| ProcessId(n.get()));

                        let new_prio = metadata[2] as u32;

                        Ready(Response::new(
                            self.on_setprocprio(fd_pid, target_pid, new_prio).map(|()| 0),
                            op
                        ))
                    },
                    ProcCall::GetProcPriority => {
                        let target_pid = NonZeroUsize::new(metadata[1] as usize)
                            .map_or(fd_pid, |n| ProcessId(n.get()));

                        Ready(Response::new(
                            self.on_getprocprio(fd_pid, target_pid).map(|prio| prio as usize),
                            op,
                        ))
                    },

                }
            }
            Handle::Ps(_) => Response::ready_err(EOPNOTSUPP, op),
            Handle::ProcCredsCapability => {
                let Some(verb) = ProcCall::try_from_raw(metadata[0] as usize) else {
                    log::trace!("Invalid proc call: {metadata:?}");
                    return Response::ready_err(EINVAL, op);
                };
                match verb {
                    ProcCall::GetProcCredentials => Ready(Response::new(
                        self.read_process_metadata(ProcessId(metadata[1] as usize), payload),
                        op,
                    )),
                    _ => Response::ready_err(EINVAL, op),
                }
            }
            Handle::SchemeRoot => Response::ready_err(EBADF, op),
        }
    }
    fn on_getpgid(&mut self, caller_pid: ProcessId, target_pid: ProcessId) -> Result<ProcessId> {
        log::trace!("GETPGID from {caller_pid:?} target {target_pid:?}");
        let caller_proc = self
            .processes
            .get(&caller_pid)
            .ok_or(Error::new(ESRCH))?
            .borrow();
        let target_proc = self
            .processes
            .get(&target_pid)
            .ok_or(Error::new(ESRCH))?
            .borrow();

        // Although not required, POSIX allows the impl to forbid getting the pgid of processes
        // outside of the caller's session.
        if caller_proc.sid != target_proc.sid && caller_proc.euid != 0 {
            return Err(Error::new(EPERM));
        }

        Ok(target_proc.pgid)
    }
    fn on_setsid(&mut self, caller_pid: ProcessId, awoken: &mut VecDeque<VirtualId>) -> Result<()> {
        // TODO: more efficient?
        // POSIX: any other process's pgid matches the caller pid
        if self
            .processes
            .iter()
            .any(|(pid, rc)| *pid != caller_pid && rc.borrow().pgid == caller_pid)
        {
            return Err(Error::new(EPERM));
        }

        let caller_proc_rc = self.processes.get(&caller_pid).ok_or(Error::new(ESRCH))?;
        let mut caller_proc = caller_proc_rc.borrow_mut();
        let mut parent = (caller_proc.ppid != caller_pid)
            .then(|| {
                self.processes
                    .get(&caller_proc.ppid)
                    .map(|p| p.borrow_mut())
            })
            .ok_or(Error::new(ESRCH))?;

        // POSIX: already a process group leader
        if caller_proc.pgid == caller_pid {
            return Err(Error::new(EPERM));
        }

        Self::set_pgid(
            caller_proc_rc,
            &mut *caller_proc,
            parent.as_deref_mut(),
            &mut self.groups,
            caller_pid,
            awoken,
        )?;
        caller_proc.sid = caller_pid;

        // TODO: Remove controlling terminal
        Ok(())
    }
    fn on_getppid(&mut self, caller_pid: ProcessId) -> Result<ProcessId> {
        log::trace!("GETPPID {caller_pid:?}");
        let ppid = self
            .processes
            .get(&caller_pid)
            .ok_or(Error::new(ESRCH))?
            .borrow()
            .ppid;
        log::trace!("GETPPID {caller_pid:?} -> {ppid:?}");
        Ok(ppid)
    }
    fn on_getsid(&mut self, caller_pid: ProcessId, req_pid: ProcessId) -> Result<ProcessId> {
        let caller_proc = self
            .processes
            .get(&caller_pid)
            .ok_or(Error::new(ESRCH))?
            .borrow();
        let requested_proc = self
            .processes
            .get(&req_pid)
            .ok_or(Error::new(ESRCH))?
            .borrow();

        // POSIX allows, but does not require, the implementation to forbid getting the session ID of processes outside
        // the current session.
        if caller_proc.sid != requested_proc.sid && caller_proc.euid != 0 {
            return Err(Error::new(EPERM));
        }

        Ok(requested_proc.sid)
    }
    fn on_setpgid(
        &mut self,
        caller_pid: ProcessId,
        target_pid: ProcessId,
        new_pgid: ProcessId,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Result<()> {
        let caller_proc = self.processes.get(&caller_pid).ok_or(Error::new(ESRCH))?;
        let caller_sid = caller_proc.borrow().sid;

        let proc_rc = self.processes.get(&target_pid).ok_or(Error::new(ESRCH))?;
        let mut proc = proc_rc.borrow_mut();

        if proc.ppid != caller_pid && target_pid != caller_pid {
            return Err(Error::new(ESRCH));
        }

        let mut parent = if proc.ppid == target_pid {
            None // init
        } else {
            Some(
                self.processes
                    .get(&proc.ppid)
                    .ok_or(Error::new(ESRCH))?
                    .borrow_mut(),
            )
        };

        // Session leaders cannot have their pgid changed.
        if proc.sid == target_pid {
            return Err(Error::new(EPERM));
        }

        // Cannot change the pgid of a process in a different session.
        if caller_sid != proc.sid {
            return Err(Error::new(EPERM));
        }

        // New pgid must either already exit, or be the same as the target pid.
        if new_pgid != target_pid && !self.groups.contains_key(&new_pgid) {
            return Err(Error::new(EPERM));
        }

        // After execv(), i.e. ProcCall::DisableSetpgid, setpgid where target_pid is a child
        // process of the calling process, shall return EACCESS.
        if proc.ppid == caller_pid && proc.disabled_setpgid {
            return Err(Error::new(EACCES));
        }

        if proc.pgid == new_pgid {
            return Ok(());
        }

        Self::set_pgid(
            proc_rc,
            &mut *proc,
            parent.as_deref_mut(),
            &mut self.groups,
            new_pgid,
            awoken,
        )?;

        Ok(())
    }
    fn set_pgid(
        proc_rc: &Rc<RefCell<Process>>,
        proc: &mut Process,
        parent: Option<&mut Process>,
        groups: &mut HashMap<ProcessId, Rc<RefCell<Pgrp>>>,
        new_pgid: ProcessId,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Result<()> {
        let old_pgid = proc.pgid;
        assert_ne!(old_pgid, new_pgid);

        if let Some(parent) = parent {
            // Some waitpid waiters may end up waiting for no children, if a child sets its pgid
            // and the parent was waiting with a pgid filter. Ensure the waiter is awoken and
            // possibly returns ECHILD.
            awoken.extend(parent.waitpid_waiting.drain(..));
        }

        let proc_weak = Rc::downgrade(proc_rc);
        let shall_remove = {
            let mut old_group = groups.get(&old_pgid).ok_or(Error::new(ESRCH))?.borrow_mut();
            old_group.processes.retain(|w| !Weak::ptr_eq(w, &proc_weak));
            old_group.processes.is_empty()
        };
        if shall_remove {
            groups.remove(&old_pgid);
        }
        groups
            .entry(new_pgid)
            .or_default()
            .borrow_mut()
            .processes
            .push(proc_weak);
        proc.pgid = new_pgid;
        Ok(())
    }
    fn on_exit_start(
        &mut self,
        pid: ProcessId,
        status: u16,
        state: VacantEntry<VirtualId, PendingState, DefaultHashBuilder>,
        awoken: &mut VecDeque<VirtualId>,
        tag: Option<Tag>,
    ) -> Poll<Response> {
        log::trace!("ON_EXIT_START {pid:?} status {status:#x}");
        let Some(proc_rc) = self.processes.get(&pid) else {
            return if let Some(tag) = tag {
                Response::ready_err(EBADFD, tag)
            } else {
                Pending
            };
        };
        let mut process_guard = proc_rc.borrow_mut();
        let process = &mut *process_guard;

        match process.status {
            ProcessStatus::Stopped(_) | ProcessStatus::PossiblyRunnable => (),
            //ProcessStatus::Exiting => return Pending,
            ProcessStatus::Exiting { .. } => {
                return if let Some(tag) = tag {
                    Response::ready_err(EAGAIN, tag)
                } else {
                    Pending
                };
            }
            ProcessStatus::Exited { .. } => {
                return if let Some(tag) = tag {
                    Response::ready_err(ESRCH, tag)
                } else {
                    Pending
                };
            }
        }

        // Forbid the caller from giving statuses corresponding to e.g. WIFCONTINUED which exit()
        // obviously can never be.

        log::trace!("Killed with raw status {status:?}");

        // TODO: Are WIFEXITED and WIFSIGNALED mutually exclusive?
        let (status, signal) = if status & 0xff == status {
            (status as u8, None)
        } else {
            // TODO: Only allow valid and catchable signal numbers.
            let sig = (status >> 8) as u8;
            if !matches!(sig, 1..=64) {
                return if let Some(tag) = tag {
                    Response::ready_err(EINVAL, tag)
                } else {
                    Pending
                };
            }
            (0, NonZeroU8::new(sig))
        };

        process.status = ProcessStatus::Exiting { status, signal };
        if !process.threads.is_empty() {
            // terminate all threads (possibly including the caller, resulting in EINTR and a
            // to-be-ignored cancellation request to this scheme).
            for thread in &process.threads {
                let thread = thread.borrow_mut();
                // TODO: cancel all threads anyway on error?
                if let Err(err) = thread.status_hndl.write(&usize::MAX.to_ne_bytes()) {
                    if let Some(tag) = tag {
                        return Response::ready_err(err.errno, tag);
                    }
                }
            }

            log::trace!("EXIT PENDING");
            //self.debug();
            // TODO: check?
            process.awaiting_threads_term.push(*state.key());
        }
        drop(process_guard);
        self.work_on(
            state.insert_entry(PendingState::AwaitingThreadsTermination(pid, tag)),
            awoken,
        )
    }
    fn on_waitpid(
        &mut self,
        this_pid: ProcessId,
        target: WaitpidTarget,
        flags: WaitFlags,
        req_id: VirtualId,
    ) -> Poll<Result<(usize, i32)>> {
        if matches!(
            target,
            WaitpidTarget::AnyChild | WaitpidTarget::AnyGroupMember
        ) {
            // Check for existence of child.
            // TODO(opt): inefficient, keep refcount?
            if !self.processes.values().any(|p| p.borrow().ppid == this_pid) {
                return Ready(Err(Error::new(ECHILD)));
            }
        }

        let proc_rc = self.processes.get(&this_pid).ok_or(Error::new(EBADFD))?;

        log::trace!("PROCS {:#?}", self.processes);

        let mut proc_guard = proc_rc.borrow_mut();
        let proc = &mut *proc_guard;

        let recv_nonblock = |waitpid: &mut BTreeMap<WaitpidKey, (ProcessId, WaitpidStatus)>,
                             key: &WaitpidKey|
         -> Option<(ProcessId, WaitpidStatus)> {
            if let Some((pid, sts)) = waitpid.get(key).map(|(k, v)| (*k, *v)) {
                waitpid.remove(key);
                /*while let Some((_, new_sts)) = waitpid.remove(&WaitpidKey { pid: Some(pid), pgid: None }) {
                    sts = new_sts;
                }*/
                Some((pid, sts))
            } else {
                None
            }
        };
        let grim_reaper =
            |w_pid: ProcessId, status: WaitpidStatus, scheme: &mut ProcScheme| match status {
                WaitpidStatus::Continued => {
                    if flags.contains(WaitFlags::WCONTINUED) {
                        Ready((w_pid.0, 0xffff))
                    } else {
                        Pending
                    }
                }
                WaitpidStatus::Stopped { signal } => {
                    if flags.contains(WaitFlags::WUNTRACED) {
                        Ready((w_pid.0, 0x7f | (i32::from(signal.get()) << 8)))
                    } else {
                        Pending
                    }
                }
                WaitpidStatus::Terminated { signal, status } => {
                    scheme.reap(w_pid);
                    Ready((
                        w_pid.0,
                        i32::from(signal.map_or(0, NonZeroU8::get)) | (i32::from(status) << 8),
                    ))
                }
            };

        match target {
            WaitpidTarget::AnyChild | WaitpidTarget::AnyGroupMember => {
                let kv = (if matches!(target, WaitpidTarget::AnyChild) {
                    proc.waitpid.first_key_value()
                } else {
                    proc.waitpid.get_key_value(&WaitpidKey {
                        pid: None,
                        pgid: Some(proc.pgid),
                    })
                })
                .map(|(k, v)| (*k, *v));
                if let Some((wid, (w_pid, status))) = kv {
                    let _ = proc.waitpid.remove(&wid);
                    drop(proc_guard);
                    grim_reaper(w_pid, status, self).map(Ok)
                } else if flags.contains(WaitFlags::WNOHANG) {
                    Ready(Ok((0, 0)))
                } else {
                    proc.waitpid_waiting.push_back(req_id);
                    Pending
                }
            }
            WaitpidTarget::SingleProc(pid) => {
                if this_pid == pid {
                    return Ready(Err(Error::new(EINVAL)));
                }
                let target_proc_rc = self.processes.get(&pid).ok_or(Error::new(ECHILD))?;
                let target_proc = target_proc_rc.borrow_mut();

                if target_proc.ppid != this_pid {
                    return Ready(Err(Error::new(ECHILD)));
                }
                let key = WaitpidKey {
                    pid: Some(pid),
                    pgid: None,
                };
                if let ProcessStatus::Exited { status, signal } = target_proc.status {
                    let _ = recv_nonblock(&mut proc.waitpid, &key);
                    drop(proc_guard);
                    drop(target_proc);
                    grim_reaper(pid, WaitpidStatus::Terminated { signal, status }, self).map(Ok)
                } else {
                    let res = recv_nonblock(&mut proc.waitpid, &key);
                    if let Some((w_pid, status)) = res {
                        drop(proc_guard);
                        drop(target_proc);
                        grim_reaper(w_pid, status, self).map(Ok)
                    } else if flags.contains(WaitFlags::WNOHANG) {
                        Ready(Ok((0, 0)))
                    } else {
                        proc.waitpid_waiting.push_back(req_id);
                        Pending
                    }
                }
            }
            WaitpidTarget::ProcGroup(pgid) => {
                if let Some(group_rc) = self.groups.get(&pgid) {
                    let group = group_rc.borrow();
                    if !group
                        .processes
                        .iter()
                        .filter_map(Weak::upgrade)
                        .filter(|r| !Rc::ptr_eq(r, proc_rc))
                        .any(|p| p.borrow().ppid == this_pid)
                    {
                        return Ready(Err(Error::new(ECHILD)));
                    }
                } else {
                    return Ready(Err(Error::new(ECHILD)));
                }

                let key = WaitpidKey {
                    pid: None,
                    pgid: Some(pgid),
                };
                if let Some(&(w_pid, status)) = proc.waitpid.get(&key) {
                    let _ = proc.waitpid.remove(&key);
                    drop(proc_guard);
                    grim_reaper(w_pid, status, self).map(Ok)
                } else if flags.contains(WaitFlags::WNOHANG) {
                    Ready(Ok((0, 0)))
                } else {
                    proc.waitpid_waiting.push_back(req_id);
                    Pending
                }
            }
        }
    }
    fn reap(&mut self, pid: ProcessId) {
        let Entry::Occupied(entry) = self.processes.entry(pid) else {
            return;
        };
        let pgid = {
            let proc = entry.get().borrow();
            if !proc.threads.is_empty() {
                log::error!(
                    "reaping process (pid {pid:?} with remaining threads: {:#?}",
                    proc.threads
                );
                return;
            }
            proc.pgid
        };
        let proc_rc = entry.remove();
        let proc_weak = Rc::downgrade(&proc_rc);

        let Entry::Occupied(group) = self.groups.entry(pgid) else {
            log::error!("Process missing from its group");
            return;
        };
        group
            .get()
            .borrow_mut()
            .processes
            .retain(|p| !Weak::ptr_eq(&proc_weak, p));
        if group.get().borrow_mut().processes.is_empty() {
            group.remove();
        }
        // TODO: notify parent's other waiters if ECHILD would now occur?
    }
    fn on_setresugid(&mut self, pid: ProcessId, raw_buf: &[u8]) -> Result<()> {
        let [new_ruid, new_euid, new_suid, new_rgid, new_egid, new_sgid] = {
            let raw_ids: [u32; 6] = plain::slice_from_bytes::<u32>(raw_buf)
                .unwrap()
                .try_into()
                .map_err(|_| Error::new(EINVAL))?;
            raw_ids.map(|i| if i == u32::MAX { None } else { Some(i) })
        };
        let mut proc = self
            .processes
            .get(&pid)
            .ok_or(Error::new(ESRCH))?
            .borrow_mut();

        if proc.euid != 0 {
            if ![new_ruid, new_euid, new_suid]
                .iter()
                .filter_map(|x| *x)
                .all(|new_id| [proc.ruid, proc.euid, proc.suid].contains(&new_id))
            {
                return Err(Error::new(EPERM));
            }
            if ![new_rgid, new_egid, new_sgid]
                .iter()
                .filter_map(|x| *x)
                .all(|new_id| [proc.rgid, proc.egid, proc.sgid].contains(&new_id))
            {
                return Err(Error::new(EPERM));
            }
        }

        if let Some(new_ruid) = new_ruid {
            proc.ruid = new_ruid;
        }
        if let Some(new_euid) = new_euid {
            proc.euid = new_euid;
        }
        if let Some(new_suid) = new_suid {
            proc.suid = new_suid;
        }
        if let Some(new_rgid) = new_rgid {
            proc.rgid = new_rgid;
        }
        if let Some(new_egid) = new_egid {
            proc.egid = new_egid;
        }
        if let Some(new_sgid) = new_sgid {
            proc.sgid = new_sgid;
        }
        if let Err(err) = proc.sync_kernel_attrs(&self.auth) {
            log::warn!("Failed to sync proc attrs in setresugid: {err}");
        }
        Ok(())
    }
    fn ancestors(&self, pid: ProcessId) -> impl Iterator<Item = ProcessId> + '_ {
        struct Iter<'a> {
            cur: Option<ProcessId>,
            procs: &'a HashMap<ProcessId, Rc<RefCell<Process>>, DefaultHashBuilder>,
        }
        impl Iterator for Iter<'_> {
            type Item = ProcessId;

            fn next(&mut self) -> Option<Self::Item> {
                let proc = self.procs.get(&self.cur?)?;
                let ppid = proc.borrow().ppid;
                self.cur = Some(ppid);
                Some(ppid)
            }
        }
        Iter {
            cur: Some(pid),
            procs: &self.processes,
        }
    }
    fn work_on(
        &mut self,
        mut state_entry: OccupiedEntry<VirtualId, PendingState, DefaultHashBuilder>,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Poll<Response> {
        let req_id = *state_entry.key();
        let state = state_entry.get_mut();
        let this_state = core::mem::replace(state, PendingState::Placeholder);
        match this_state {
            PendingState::Placeholder => return Pending, // unreachable!(),
            PendingState::AwaitingThreadsTermination(current_pid, tag) => {
                let Some(proc_rc) = self.processes.get(&current_pid) else {
                    return if let Some(tag) = tag {
                        Response::ready_err(ESRCH, tag)
                    } else {
                        state_entry.remove();
                        Pending
                    };
                };
                let mut proc_guard = proc_rc.borrow_mut();
                let proc = &mut *proc_guard;

                if proc.threads.is_empty() {
                    log::trace!("WORKING ON AWAIT TERM");
                    let (signal, status) = match proc.status {
                        ProcessStatus::Exiting { signal, status } => (signal, status),
                        ProcessStatus::Exited { .. } => {
                            return if let Some(tag) = tag {
                                Response::ready_ok(0, tag)
                            } else {
                                state_entry.remove();
                                Pending
                            };
                        }
                        _ => {
                            return if let Some(tag) = tag {
                                Response::ready_err(ESRCH, tag) // TODO?
                            } else {
                                state_entry.remove();
                                Pending
                            };
                        }
                    };
                    // TODO: Properly remove state
                    state_entry.remove();

                    proc.status = ProcessStatus::Exited { signal, status };

                    let (ppid, pgid) = (proc.ppid, proc.pgid);
                    drop(proc_guard);

                    if let Some(parent_rc) = self.processes.get(&ppid) {
                        // When a process exits, the parent is sent SIGCHLD. The process has no threads
                        // at this point.
                        if let Err(err) = self.on_send_sig(
                            current_pid,
                            KillTarget::Proc(ppid),
                            SIGCHLD as u8,
                            &mut false,
                            KillMode::Idempotent,
                            false, // stop_or_continue
                            awoken,
                        ) {
                            log::error!("failed to send SIGCHLD to parent PID {ppid:?}: {err}");
                        }

                        if let Some(init_rc) = self.processes.get(&INIT_PID) {
                            awoken.extend(init_rc.borrow_mut().waitpid_waiting.drain(..));

                            // TODO(opt): Store list of children in each process?
                            let children_iter = || {
                                self.processes
                                    .values()
                                    .filter(|p| !Rc::ptr_eq(p, init_rc))
                                    .filter(|p| p.borrow().ppid == current_pid)
                            };

                            // TODO(opt): Avoid allocation?
                            let affected_pgids = children_iter()
                                .map(|child_rc| {
                                    let child_pgid = child_rc.borrow().pgid;
                                    (child_pgid, self.pgrp_is_orphaned(child_pgid))
                                })
                                .chain(Some((pgid, self.pgrp_is_orphaned(pgid))))
                                .collect::<HashMap<ProcessId, Option<bool>>>();

                            // Transfer children to init
                            for child_rc in children_iter() {
                                let mut child = child_rc.borrow_mut();
                                log::trace!(
                                    "Reparenting {:?} (ppid {:?}) => {:?}",
                                    child.pid,
                                    child.ppid,
                                    INIT_PID
                                );
                                child.ppid = INIT_PID;
                                init_rc.borrow_mut().waitpid.append(&mut child.waitpid);
                                drop(child);
                            }
                            // Check if any process group ID would become orphaned as a result of
                            // this exit.
                            for (affected_pgid, was_orphaned) in affected_pgids {
                                let is_orphaned = self.pgrp_is_orphaned(affected_pgid);

                                if !was_orphaned.unwrap_or(false)
                                    && is_orphaned.unwrap_or(false)
                                    && let Some(group) =
                                        self.groups.get(&affected_pgid).map(|r| r.borrow())
                                {
                                    for process_rc in
                                        group.processes.iter().filter_map(|w| Weak::upgrade(&w))
                                    {
                                        if !matches!(
                                            process_rc.borrow().status,
                                            ProcessStatus::Stopped(_)
                                        ) {
                                            continue;
                                        }
                                        let sighup_pid = process_rc.borrow().pid;
                                        log::trace!("SENDING SIGCONT TO {sighup_pid:?}");
                                        if let Err(err) = self.on_send_sig(
                                            INIT_PID,
                                            KillTarget::Proc(sighup_pid),
                                            SIGCONT as u8,
                                            &mut false,
                                            KillMode::Idempotent,
                                            false,
                                            awoken,
                                        ) {
                                            log::warn!(
                                                "Failed to send newly-orphaned-pgid SIGHUP to PID {sighup_pid:?}: {err}"
                                            );
                                        }
                                        log::trace!("SENDING SIGHUP TO {sighup_pid:?}");
                                        if let Err(err) = self.on_send_sig(
                                            INIT_PID,
                                            KillTarget::Proc(sighup_pid),
                                            SIGHUP as u8,
                                            &mut false,
                                            KillMode::Idempotent,
                                            false,
                                            awoken,
                                        ) {
                                            log::warn!(
                                                "Failed to send newly-orphaned-pgid SIGHUP to PID {sighup_pid:?}: {err}"
                                            );
                                        }
                                    }
                                }
                            }
                        }

                        let mut parent = parent_rc.borrow_mut();

                        parent.waitpid.insert(
                            WaitpidKey {
                                pid: Some(current_pid),
                                pgid: Some(pgid),
                            },
                            (current_pid, WaitpidStatus::Terminated { signal, status }),
                        );
                        log::trace!("AWAKING WAITPID {:?}", parent.waitpid_waiting);
                        // TODO(opt): inefficient
                        awoken.extend(parent.waitpid_waiting.drain(..));
                    }
                    if let Some(tag) = tag {
                        Ready(Response::new(Ok(0), tag))
                    } else {
                        // state was removed earlier
                        Pending
                    }
                } else {
                    log::trace!("WAITING AGAIN");
                    proc.awaiting_threads_term.push(req_id);
                    *state = PendingState::AwaitingThreadsTermination(current_pid, tag);
                    Pending
                }
            }
            PendingState::AwaitingStatusChange {
                waiter,
                target,
                flags,
                mut op,
            } => {
                log::trace!("WAITPID {req_id:?}, {waiter:?}: {target:?} flags {flags:?}");
                let res = self.on_waitpid(waiter, target, flags, req_id);
                log::trace!(
                    "WAITPID {req_id:?}, {waiter:?}: {target:?} flags {flags:?} -> {res:?}"
                );

                match res {
                    Ready(Ok((pid, status))) => {
                        if let Ok(status_out) = plain::from_mut_bytes::<i32>(op.payload()) {
                            *status_out = status;
                        }
                        Response::ready_ok(pid, op)
                    }
                    Ready(Err(e)) => Response::ready_err(e.errno, op),
                    Pending => {
                        *state = PendingState::AwaitingStatusChange {
                            waiter,
                            target,
                            flags,
                            op,
                        };
                        Pending
                    }
                }
            }
        }
    }
    fn debug(&self) {
        log::trace!("PROCESSES\n{:#?}", self.processes);
        log::trace!("HANDLES\n{:#?}", self.handles);
    }
    fn on_kill_thread(
        &mut self,
        thread: &Rc<RefCell<Thread>>,
        signal: u8,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Result<()> {
        let mut killed_self = false;

        let stop_or_continue = false;

        let caller_pid = thread.borrow().pid; // TODO(feat): allow this to be specified?

        self.on_send_sig(
            caller_pid,
            KillTarget::Thread(Rc::clone(thread)),
            signal,
            &mut killed_self,
            KillMode::Idempotent,
            stop_or_continue,
            awoken,
        )?;

        if killed_self {
            // TODO: is this the most accurate error code?
            Err(Error::new(ERESTART))
        } else {
            Ok(())
        }
    }
    fn on_kill(
        &mut self,
        caller_pid: ProcessId,
        target: ProcKillTarget,
        signal: u8,
        mode: KillMode,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Result<()> {
        log::trace!("KILL(from {caller_pid:?}) TARGET {target:?} {signal} {mode:?}");

        // if this is set and we would otherwise have succeeded, return EINTR so it can check its
        // own mask
        let mut killed_self = false;

        // SIGCHLD to parent are not generated by on_kill, but by on_send_sig itself
        let stop_or_continue = false;

        let match_grp = match target {
            ProcKillTarget::SingleProc(pid) => {
                self.on_send_sig(
                    caller_pid,
                    KillTarget::Proc(ProcessId(pid)),
                    signal,
                    &mut killed_self,
                    mode,
                    stop_or_continue,
                    awoken,
                )?;
                return if killed_self {
                    Err(Error::new(ERESTART))
                } else {
                    Ok(())
                };
            }
            ProcKillTarget::All => None,
            ProcKillTarget::ProcGroup(grp) => Some(ProcessId(grp)),
            ProcKillTarget::ThisGroup => Some(
                self.processes
                    .get(&caller_pid)
                    .ok_or(Error::new(ESRCH))?
                    .borrow()
                    .pgid,
            ),
        };
        log::trace!("match group {match_grp:?}");

        let mut err_opt = None;
        // Number of processes successfully signaled.
        let mut num_succeeded = 0;

        for (pid, proc_rc) in self.processes.iter() {
            if match_grp.map_or(false, |g| proc_rc.borrow().pgid != g) {
                continue;
            }
            let res = self.on_send_sig(
                caller_pid,
                KillTarget::Proc(*pid),
                signal,
                &mut killed_self,
                mode,
                stop_or_continue,
                awoken,
            );
            match res {
                Ok(()) => num_succeeded += 1,
                Err(err) => err_opt = Some(err),
            }
        }

        // > POSIX Issue 8: The `kill()` function is successful if the process has
        // > permission to send `sig` to *any* of the processes specified by
        // > `pid`.
        //
        // Thus, if *at least one* process was successfully signaled, `kill()`
        // returns success.
        if num_succeeded == 0 {
            if let Some(err) = err_opt {
                Err(err)
            } else {
                // No process or process group could be found corrsponding to
                // that specified by `target`.
                Err(Error::new(ESRCH))
            }
        } else if killed_self {
            Err(Error::new(ERESTART))
        } else {
            Ok(())
        }
    }
    fn on_send_sig(
        &self,
        caller_pid: ProcessId,
        target: KillTarget,
        signal: u8,
        killed_self: &mut bool,
        mode: KillMode,
        stop_or_continue: bool,
        awoken: &mut VecDeque<VirtualId>,
    ) -> Result<()> {
        log::trace!("SEND_SIG(from {caller_pid:?}) TARGET {target:?} {signal} {mode:?}");
        let sig = usize::from(signal);

        if sig > 64 {
            return Err(Error::new(EINVAL));
        }

        let sig_group = (sig - 1) / 32;
        let sig_idx = (sig - 1) % 32;

        let target_pid = match target {
            KillTarget::Proc(pid) => pid,
            KillTarget::Thread(ref thread) => thread.borrow().pid,
        };
        let target_proc_rc = self.processes.get(&target_pid).ok_or(Error::new(ESRCH))?;

        let sender = SenderInfo {
            pid: caller_pid.0 as u32,
            ruid: self
                .processes
                .get(&caller_pid)
                .ok_or(Error::new(ESRCH))?
                .borrow()
                .ruid,
        };

        enum SendResult {
            LacksPermission,
            Succeeded,
            SucceededSigchld {
                orig_signal: NonZeroU8,
                ppid: ProcessId,
                pgid: ProcessId,
            },
            SucceededSigcont {
                ppid: ProcessId,
                pgid: ProcessId,
            },
            FullQ,
            Invalid,
        }
        let (caller_euid, caller_ruid) = {
            let caller = self
                .processes
                .get(&caller_pid)
                .ok_or(Error::new(ESRCH))?
                .borrow();
            (caller.euid, caller.ruid)
        };

        let result = (|| {
            // XXX: It's not currently possible for procmgr to know what thread called, so the
            // EINTR will be coarser. That shouldn't affect program logic though, since the
            // trampoline always checks the masks anyway.
            // TODO(feat): allow regular kill (alongside thread-kill) to operate on *thread fds*?
            let is_self = target_pid == caller_pid;

            let mut target_proc_guard = target_proc_rc.borrow_mut();
            let mut target_proc = &mut *target_proc_guard;

            if caller_euid != 0
                && caller_euid != target_proc.ruid
                && caller_ruid != target_proc.ruid
            {
                return SendResult::LacksPermission;
            }

            // If sig = 0, test that process exists and can be signalled, but don't send any
            // signal.
            let Some(nz_signal) = NonZeroU8::new(signal) else {
                return SendResult::Succeeded;
            };

            // Similarly, don't send anything for already exiting or exited processes. It would be
            // bad if e.g. SIGCONT could cause these to become PossiblyRunnable again.
            if matches!(
                target_proc.status,
                ProcessStatus::Exited { .. } | ProcessStatus::Exiting { .. }
            ) {
                return SendResult::Succeeded;
            }

            let Some(mut sig_pctl) = target_proc.sig_pctl.as_ref() else {
                log::trace!("No pctl {caller_pid:?} => {target_pid:?}");
                return SendResult::Invalid;
            };
            log::trace!("PCTL {:#x?}", &**sig_pctl);
            log::trace!(
                "STS {:?} NTHRD {}",
                target_proc.status,
                target_proc.threads.len()
            );

            if sig == SIGCONT
                && let ProcessStatus::Stopped(_sig) = target_proc.status
            {
                // Convert stopped processes to blocked if sending SIGCONT, regardless of whether
                // SIGCONT is blocked or ignored. It can however be controlled whether the process
                // will additionally ignore, defer, or handle that signal.
                target_proc.status = ProcessStatus::PossiblyRunnable;

                if !sig_pctl.signal_will_ign(SIGCONT, false) {
                    sig_pctl
                        .pending
                        .fetch_or(sig_bit(SIGCONT), Ordering::Relaxed);
                }

                // TODO: which threads should become Runnable?
                for thread_rc in target_proc.threads.iter() {
                    let thread = thread_rc.borrow_mut();
                    if let Some(ref tctl) = thread.sig_ctrl {
                        tctl.word[0].fetch_and(
                            !(sig_bit(SIGSTOP)
                                | sig_bit(SIGTTIN)
                                | sig_bit(SIGTTOU)
                                | sig_bit(SIGTSTP)),
                            Ordering::Relaxed,
                        );
                    }
                    thread
                        .status_hndl
                        .write(&(ContextVerb::Unstop as usize).to_ne_bytes())
                        .expect("TODO");
                }
                // POSIX XSI allows but does not reqiure SIGCHLD to be sent when SIGCONT occurs.
                return SendResult::SucceededSigcont {
                    ppid: target_proc.ppid,
                    pgid: target_proc.pgid,
                };
            }
            let is_conditional_stop = matches!(sig, SIGTTIN | SIGTTOU | SIGTSTP);
            if sig == SIGSTOP
                || (is_conditional_stop
                    && target_proc
                        .sig_pctl
                        .as_ref()
                        .map_or(false, |proc| proc.signal_will_stop(sig)))
            {
                if is_conditional_stop {
                    let pgid = target_proc.pgid;
                    drop(target_proc_guard);

                    if self.pgrp_is_orphaned(pgid).unwrap_or(true) {
                        // POSIX requires that processes in orphaned process groups never be stopped in
                        // due to SIGTTIN/SIGTTOU/SIGTSTP.
                        return SendResult::Succeeded;
                    }

                    target_proc_guard = target_proc_rc.borrow_mut();
                    target_proc = &mut *target_proc_guard;
                    sig_pctl = target_proc.sig_pctl.as_mut().expect("already checked");
                }

                target_proc.status = ProcessStatus::Stopped(sig);

                for thread in &target_proc.threads {
                    let thread = thread.borrow();
                    match thread
                        .status_hndl
                        .write(&(ContextVerb::Stop as usize).to_ne_bytes())
                    {
                        Ok(_) => (),
                        // TODO: Write a test that this actually results in the thread eventually
                        // being removed from `threads`. A "dead thread" event should already have
                        // been triggered, but it is possible that happens during this code, or
                        // just before that event.
                        //
                        // Thread has state Dead, so ignore.
                        Err(Error { errno: EOWNERDEAD }) => continue,
                        Err(other) => {
                            log::error!(
                                "Unexpected error when stopping context: {other}, pid {target_pid:?} thread fd {}",
                                thread.status_hndl.as_raw_fd()
                            );
                            continue;
                        }
                    }
                    if let Some(ref tctl) = thread.sig_ctrl {
                        tctl.word[0].fetch_and(!sig_bit(SIGCONT), Ordering::Relaxed);
                    }
                }

                // TODO: Actually wait for, or IPI the context first, then clear bit. Not atomically safe otherwise?
                sig_pctl
                    .pending
                    .fetch_and(!sig_bit(SIGCONT), Ordering::Relaxed);

                log::trace!("SUCCEEDED SIGCHILD MY_PID {target_pid:?}");
                return SendResult::SucceededSigchld {
                    orig_signal: nz_signal,
                    ppid: target_proc.ppid,
                    pgid: target_proc.pgid,
                };
            }
            if sig == SIGKILL {
                for thread in &target_proc.threads {
                    let thread = thread.borrow();
                    thread
                        .status_hndl
                        .write(&(ContextVerb::ForceKill as usize).to_ne_bytes())
                        .expect("TODO");
                }

                *killed_self |= is_self;

                // exit() will signal the parent, rather than immediately in kill()
                return SendResult::Succeeded;
            }
            if !sig_pctl.signal_will_ign(sig, stop_or_continue) {
                match target {
                    KillTarget::Thread(ref thread_rc) => {
                        let thread = thread_rc.borrow();
                        let Some(ref tctl) = thread.sig_ctrl else {
                            log::trace!("No tctl");
                            return SendResult::Invalid;
                        };

                        tctl.sender_infos[sig_idx].store(sender.raw(), Ordering::Relaxed);
                        let bit = 1 << sig_idx;

                        let _was_new = tctl.word[sig_group].fetch_or(bit, Ordering::Release);
                        if (tctl.word[sig_group].load(Ordering::Relaxed) >> 32) & bit != 0 {
                            *killed_self |= is_self;
                            thread
                                .status_hndl
                                .write(&(ContextVerb::Interrupt as usize).to_ne_bytes())
                                .expect("TODO");
                        }
                    }
                    KillTarget::Proc(proc) => {
                        match mode {
                            KillMode::Queued(arg) => {
                                if sig_group != 1 {
                                    log::trace!("Out of range");
                                    return SendResult::Invalid;
                                }
                                let rtidx = sig_idx;
                                //log::trace!("QUEUEING {arg:?} RTIDX {rtidx}");
                                if rtidx >= target_proc.rtqs.len() {
                                    target_proc.rtqs.resize_with(rtidx + 1, VecDeque::new);
                                }
                                let rtq = target_proc.rtqs.get_mut(rtidx).unwrap();

                                // TODO(feat): configurable limit?
                                if rtq.len() >= 32 {
                                    return SendResult::FullQ;
                                }

                                rtq.push_back(arg);
                            }
                            KillMode::Idempotent => {
                                if sig_pctl.pending.load(Ordering::Acquire) & sig_bit(sig) != 0 {
                                    // If already pending, do not send this signal. While possible that
                                    // another thread is concurrently clearing pending, and that other
                                    // spuriously awoken threads would benefit from actually receiving
                                    // this signal, there is no requirement by POSIX for such signals
                                    // not to be mergeable. So unless the signal handler is observed to
                                    // happen-before this syscall, it can be ignored. The pending bits
                                    // would certainly have been cleared, thus contradicting this
                                    // already reached statement.
                                    return SendResult::Succeeded;
                                }

                                if sig_group != 0 {
                                    log::trace!("Invalid sig group");
                                    return SendResult::Invalid;
                                }
                                sig_pctl.sender_infos[sig_idx]
                                    .store(sender.raw(), Ordering::Relaxed);
                            }
                        }

                        sig_pctl.pending.fetch_or(sig_bit(sig), Ordering::Release);

                        for thread in target_proc.threads.iter() {
                            let thread = thread.borrow();
                            let Some(ref tctl) = thread.sig_ctrl else {
                                continue;
                            };
                            log::trace!("TCTL {:#x?}", &**tctl);
                            if (tctl.word[sig_group].load(Ordering::Relaxed) >> 32) & (1 << sig_idx)
                                != 0
                            {
                                thread
                                    .status_hndl
                                    .write(&(ContextVerb::Interrupt as usize).to_ne_bytes())
                                    .expect("TODO");
                                *killed_self |= is_self;
                                break;
                            }
                        }
                    }
                }
                SendResult::Succeeded
            } else {
                // Discard signals if sighandler is unset. This includes both special contexts such
                // as bootstrap, and child processes or threads that have not yet been started.
                // This is semantically equivalent to having all signals except SIGSTOP and SIGKILL
                // blocked/ignored (SIGCONT can be ignored and masked, but will always continue
                // stopped processes first).
                SendResult::Succeeded
            }
        })();

        match result {
            // TODO: succeed even if *some* (when group/all procs is specified) fail?
            SendResult::LacksPermission => return Err(Error::new(EPERM)),

            SendResult::Succeeded => (),
            SendResult::FullQ => return Err(Error::new(EAGAIN)),
            SendResult::Invalid => {
                log::trace!("Invalid signal configuration for {target_pid:?}");
                return Err(Error::new(ESRCH));
            }
            SendResult::SucceededSigchld {
                ppid,
                pgid,
                orig_signal,
            } => {
                {
                    let mut parent = self
                        .processes
                        .get(&ppid)
                        .ok_or(Error::new(ESRCH))?
                        .borrow_mut();
                    parent.waitpid.insert(
                        WaitpidKey {
                            pid: Some(target_pid),
                            pgid: Some(pgid),
                        },
                        (
                            target_pid,
                            WaitpidStatus::Stopped {
                                signal: orig_signal,
                            },
                        ),
                    );
                    awoken.extend(parent.waitpid_waiting.drain(..));
                }
                // TODO(err): Just ignore EINVAL (missing signal config), otherwise handle error?
                if ppid != INIT_PID {
                    log::trace!("SIGCHLDing {ppid:?}");
                    if let Err(err) = self.on_send_sig(
                        INIT_PID, // caller, TODO?
                        KillTarget::Proc(ppid),
                        SIGCHLD as u8,
                        killed_self,
                        KillMode::Idempotent,
                        true, // stop_or_continue
                        awoken,
                    ) {
                        log::trace!("failed to SIGCHLD parent (SIGSTOP): {err}");
                    }
                }
            }
            SendResult::SucceededSigcont { ppid, pgid } => {
                {
                    let mut parent = self
                        .processes
                        .get(&ppid)
                        .ok_or(Error::new(ESRCH))?
                        .borrow_mut();
                    parent.waitpid.insert(
                        WaitpidKey {
                            pid: Some(target_pid),
                            pgid: Some(pgid),
                        },
                        (target_pid, WaitpidStatus::Continued),
                    );
                    awoken.extend(parent.waitpid_waiting.drain(..));
                }
                // POSIX XSI allows but does not require SIGCONT to send signals to the parent.
                // TODO(err): Just ignore EINVAL (missing signal config), otherwise handle error?
                if ppid != INIT_PID {
                    if let Err(err) = self.on_send_sig(
                        INIT_PID, // caller, TODO?
                        KillTarget::Proc(ppid),
                        SIGCHLD as u8,
                        killed_self,
                        KillMode::Idempotent,
                        true, // stop_or_continue
                        awoken,
                    ) {
                        log::trace!("failed to SIGCHLD parent (SIGCONT): {err}");
                    }
                }
            }
        }

        Ok(())
    }
    fn real_tctl_pctl_intra_page_offsets(fd: &FdGuard) -> Result<[u16; 2]> {
        let mut buf = SetSighandlerData::default();
        fd.read(&mut buf)?;
        Ok([
            (buf.thread_control_addr % PAGE_SIZE) as u16,
            (buf.proc_control_addr % PAGE_SIZE) as u16,
        ])
    }
    fn on_proc_rename(&mut self, pid: ProcessId, new_name_raw: &[u8]) -> Result<()> {
        let name_len = new_name_raw
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(new_name_raw.len());

        let new_name =
            core::str::from_utf8(&new_name_raw[..name_len]).map_err(|_| Error::new(EINVAL))?;
        let mut proc = self
            .processes
            .get(&pid)
            .ok_or(Error::new(ESRCH))?
            .borrow_mut();

        proc.name = ArrayString::from_str(&new_name[..new_name.len().min(NAME_CAPAC)]).unwrap();
        if let Err(err) = proc.sync_kernel_attrs(&self.auth) {
            log::warn!("Failed to set kernel attrs when renaming proc: {err}");
        }
        Ok(())
    }
    fn on_sync_sigtctl(thread: &mut Thread) -> Result<()> {
        log::trace!("Sync tctl {:?}", thread.pid);
        let sigcontrol_fd = thread.fd.dup(b"sighandler")?;
        let [tctl_off, _] = Self::real_tctl_pctl_intra_page_offsets(&sigcontrol_fd)?;
        log::trace!("read intra offsets");
        thread
            .sig_ctrl
            .replace(Page::map(&sigcontrol_fd, 0, tctl_off)?);
        Ok(())
    }
    fn on_sync_sigpctl(&mut self, pid: ProcessId) -> Result<()> {
        log::trace!("Sync pctl {pid:?}");
        let mut proc = self
            .processes
            .get(&pid)
            .ok_or(Error::new(ESRCH))?
            .borrow_mut();
        let any_thread = proc.threads.first().ok_or(Error::new(EINVAL))?;
        let sigcontrol_fd = any_thread.borrow().fd.dup(b"sighandler")?;
        let [_, pctl_off] = Self::real_tctl_pctl_intra_page_offsets(&sigcontrol_fd)?;
        proc.sig_pctl
            .replace(Page::map(&sigcontrol_fd, PAGE_SIZE, pctl_off)?);
        Ok(())
    }
    fn on_sigdeq(&mut self, pid: ProcessId, payload: &mut [u8]) -> Result<()> {
        let sig_idx = {
            let bytes = <[u8; 4]>::try_from(payload.get(..4).ok_or(Error::new(EINVAL))?).unwrap();
            u32::from_ne_bytes(bytes)
        };
        log::trace!("SIGDEQ {pid:?} idx {sig_idx}");
        let dst = payload
            .get_mut(..size_of::<RtSigInfo>())
            .ok_or(Error::new(EINVAL))?;
        if sig_idx >= 32 {
            return Err(Error::new(EINVAL));
        }
        let mut proc = self
            .processes
            .get_mut(&pid)
            .ok_or(Error::new(ESRCH))?
            .borrow_mut();
        let proc = &mut *proc;

        let pctl = proc.sig_pctl.as_ref().ok_or(Error::new(EBADF))?;

        let q = proc
            .rtqs
            .get_mut(sig_idx as usize)
            .ok_or(Error::new(EAGAIN))?;
        let Some(front) = q.pop_front() else {
            return Err(Error::new(EAGAIN));
        };

        if q.is_empty() {
            pctl.pending
                .fetch_and(!(1 << (32 + sig_idx as usize)), Ordering::Relaxed);
        }
        dst.copy_from_slice(unsafe { plain::as_bytes(&front) });
        Ok(())
    }
    fn on_close(&mut self, id: usize) {
        if self.handles.try_remove(id).is_none() {
            log::error!("on_close for nonexistent handle, id={id}");
        }
    }
    fn pgrp_is_orphaned(&self, grp: ProcessId) -> Option<bool> {
        let group = self.groups.get(&grp)?.borrow();

        let mut still_true = true;

        for process_rc in group.processes.iter().filter_map(Weak::upgrade) {
            let process = process_rc.borrow();
            let Some(parent_rc) = self.processes.get(&process_rc.borrow().ppid) else {
                // TODO(err): what to do here?
                continue;
            };
            let parent = parent_rc.borrow();

            // POSIX defines orphaned process groups as those where
            //
            // forall process in group, parent = process.parent,
            //   parent's pgid == process's pgid
            //   OR
            //   parent's session id != process's session id
            let cond = parent.pgid == process.pgid || parent.sid != process.sid;
            if !cond {
                log::trace!(
                    "COUNTEREXAMPLE: process {:#?} parent {:#?}",
                    process,
                    parent
                );
            }
            still_true &= cond;
        }
        Some(still_true)
    }
    fn ps_data(&mut self, _ctx: &CallerCtx) -> Result<Vec<u8>> {
        // TODO: enforce uid == 0?

        let mut string = alloc::format!(
            "{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<8}{:<16}\n",
            "PID",
            "PGID",
            "PPID",
            "SID",
            "RUID",
            "RGID",
            "EUID",
            "EGID",
            "NTHRD",
            "STATUS",
            "NAME",
        );
        for (pid, process_rc) in self.processes.iter() {
            let process = process_rc.borrow();
            let status = match process.status {
                ProcessStatus::PossiblyRunnable => "R",
                ProcessStatus::Stopped(_) => "S",
                ProcessStatus::Exiting { .. } => "E",
                ProcessStatus::Exited { .. } => "X",
            };
            use core::fmt::Write;
            writeln!(
                string,
                "{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<6}{:<8}{:<16}",
                pid.0,
                process.pgid.0,
                process.ppid.0,
                process.sid.0,
                process.ruid,
                process.rgid,
                process.euid,
                process.egid,
                process.threads.len(),
                status,
                process.name,
            )
            .unwrap();
        }

        // Useful for debugging memory leaks.
        log::trace!("NEXT FD: {}", {
            let nextfd = syscall::dup(0, &[]).unwrap();
            let _ = syscall::close(nextfd);
            nextfd
        });
        log::trace!("{} processes", self.processes.len());
        log::trace!("{} groups", self.groups.len());
        log::trace!("{} sessions", self.sessions.len());
        log::trace!("{} handles", self.handles.len());
        log::trace!("{} thread_lookup", self.thread_lookup.len());
        log::trace!("{} next_id", self.next_internal_id);

        Ok(string.into_bytes())
    }

    fn on_setprocprio(
        &mut self,
        caller_pid: ProcessId,
        target_pid: ProcessId,
        new_prio: u32,
    ) -> Result<()> {
        if new_prio >= 40 {
            return Err(Error::new(EINVAL));
        }

        let caller_euid = self.processes.get(&caller_pid).ok_or(Error::new(ESRCH))?.borrow().euid;

        let target_rc = self.processes.get(&target_pid).ok_or(Error::new(ESRCH))?;
        let mut target = target_rc.borrow_mut();

        if caller_euid != 0 && caller_euid != target.euid {
            return Err(Error::new(EPERM));
        }

        target.prio = new_prio;

        if let Err(err) = target.sync_kernel_attrs(&self.auth) {
            log::warn!("Failed to sync proc attrs in setprocprio: {err}");
        }
        Ok(())
    }

    fn on_getprocprio(
        &self,
        caller_pid: ProcessId,
        target_pid: ProcessId,
    ) -> Result<u32> {
        let target_rc = self.processes.get(&target_pid).ok_or(Error::new(ESRCH))?;
        Ok(target_rc.borrow().prio)
    }
}

#[derive(Clone, Copy, Debug)]
enum KillMode {
    Idempotent,
    Queued(RtSigInfo),
}
#[derive(Debug)]
enum KillTarget {
    Proc(ProcessId),
    Thread(Rc<RefCell<Thread>>),
}
fn arraystring_to_bytes<const C: usize>(s: ArrayString<C>) -> [u8; C] {
    let mut buf = [0_u8; C];
    let min = buf.len().min(s.len());
    buf[..min].copy_from_slice(&s.as_bytes()[..min]);
    buf
}

impl Process {
    fn sync_kernel_attrs(&mut self, auth: &FdGuard) -> Result<()> {
        // TODO: continue with other threads if one fails?
        for thread_rc in &self.threads {
            let mut thread = thread_rc.borrow_mut();
            thread.sync_kernel_attrs(self, auth)?;
        }
        Ok(())
    }
}

impl Thread {
    fn sync_kernel_attrs(&mut self, process: &Process, auth: &FdGuard) -> Result<()> {
        let attr_fd = self
            .fd
            .dup(alloc::format!("auth-{}-attrs", auth.as_raw_fd()).as_bytes())?;
        attr_fd.write(&ProcSchemeAttrs {
            pid: process.pid.0 as u32,
            euid: process.euid,
            egid: process.egid,
            prio: process.prio,
            debug_name: arraystring_to_bytes(process.name),
        })?;
        Ok(())
    }
}
