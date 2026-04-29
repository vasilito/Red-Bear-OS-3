use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt::Debug;
use core::mem;
use hashbrown::HashMap;
use libredox::protocol::{NsDup, NsPermissions};
use log::{error, warn};
use redox_path::RedoxPath;
use redox_path::RedoxScheme;
use redox_rt::proc::FdGuard;
use redox_scheme::{
    CallerCtx, OpenResult, RequestKind, Response, SendFdRequest, SignalBehavior, Socket,
    scheme::{SchemeState, SchemeSync},
};
use syscall::Stat;
use syscall::dirent::{DirEntry, DirentBuf, DirentKind};
use syscall::{CallFlags, FobtainFdFlags, error::*, schemev2::NewFdFlags};

#[derive(Debug, Clone)]
struct Namespace {
    schemes: HashMap<String, Arc<FdGuard>>,
}

impl Namespace {
    fn fork(&self, buf: &[u8]) -> Result<Self> {
        let mut schemes = HashMap::new();
        let mut cursor = 0;
        while cursor < buf.len() {
            let len = read_num::<usize>(&buf[cursor..])?;
            cursor += mem::size_of::<usize>();
            let name = String::from_utf8(Vec::from(&buf[cursor..cursor + len]))
                .map_err(|_| Error::new(EINVAL))?;
            cursor += len;
            if name.ends_with('*') {
                let prefix = &name[..name.len() - 1];
                for (registered_name, fd) in &self.schemes {
                    if registered_name.starts_with(prefix) {
                        schemes.insert(registered_name.clone(), fd.clone());
                    }
                }
            } else {
                let Some(fd) = self.schemes.get(&name) else {
                    warn!("Scheme {} not found in namespace", name);
                    continue;
                };
                schemes.insert(name, fd.clone());
            }
        }
        Ok(Self { schemes })
    }
    fn get_scheme_fd(&self, scheme: &str) -> Option<&Arc<FdGuard>> {
        self.schemes.get(scheme)
    }
    fn remove_scheme(&mut self, scheme: &str) -> Option<()> {
        self.schemes.remove(scheme).map(|_| ())
    }
}

#[derive(Debug, Clone)]
struct NamespaceAccess {
    namespace: Rc<RefCell<Namespace>>,
    permission: NsPermissions,
}

impl NamespaceAccess {
    fn has_permission(&self, permission: NsPermissions) -> bool {
        self.permission.contains(permission)
    }
}

#[derive(Debug, Clone)]
struct SchemeRegister {
    target_namespace: Rc<RefCell<Namespace>>,
    scheme_name: String,
}

impl SchemeRegister {
    fn register(&self, fd: FdGuard) -> Result<()> {
        let mut ns = self.target_namespace.borrow_mut();
        if ns.schemes.contains_key(&self.scheme_name) {
            return Err(Error::new(EEXIST));
        }
        ns.schemes.insert(self.scheme_name.clone(), Arc::new(fd));
        Ok(())
    }
}

#[derive(Debug, Clone)]
enum Handle {
    Access(NamespaceAccess),
    Register(SchemeRegister),
    List(NamespaceAccess),
}

pub struct NamespaceScheme<'sock> {
    socket: &'sock Socket,
    handles: HashMap<usize, Handle>,
    root_namespace: Namespace,
    next_id: usize,
    scheme_creation_cap: FdGuard,
}

const HIGH_PERMISSIONS: NsPermissions = NsPermissions::SCHEME_CREATE;

impl<'sock> NamespaceScheme<'sock> {
    pub fn new(
        socket: &'sock Socket,
        schemes: HashMap<String, Arc<FdGuard>>,
        scheme_creation_cap: FdGuard,
    ) -> Self {
        Self {
            socket,
            handles: HashMap::new(),
            root_namespace: Namespace { schemes },
            next_id: 0,
            scheme_creation_cap,
        }
    }

    fn add_namespace(&mut self, id: usize, schemes: Namespace, permission: NsPermissions) {
        let handle = Handle::Access(NamespaceAccess {
            namespace: Rc::new(RefCell::new(schemes)),
            permission,
        });
        self.handles.insert(id, handle);
    }

    fn get_ns_access(&self, id: usize) -> Option<&NamespaceAccess> {
        let handle = self.handles.get(&id);
        match handle {
            Some(Handle::Access(access)) => Some(access),
            _ => None,
        }
    }

    fn open_namespace_resource(
        &self,
        ns_access: &NamespaceAccess,
        reference: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        match reference {
            "scheme-creation-cap" => {
                if !ns_access.has_permission(NsPermissions::SCHEME_CREATE) {
                    error!("Permission denied to get scheme creation capability");
                    return Err(Error::new(EACCES));
                }
                Ok(syscall::dup(self.scheme_creation_cap.as_raw_fd(), &[])?)
            }
            _ => {
                error!("Unknown special reference: {}", reference);
                return Err(Error::new(EINVAL));
            }
        }
    }

    fn open_scheme_resource(
        &self,
        ns: &Namespace,
        scheme: &str,
        reference: &str,
        flags: usize,
        fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<usize> {
        let Some(cap_fd) = ns.get_scheme_fd(scheme) else {
            log::info!("Scheme {:?} not found in namespace", scheme);
            return Err(Error::new(ENODEV));
        };

        let scheme_fd = syscall::openat_with_filter(
            cap_fd.as_raw_fd(),
            reference,
            flags,
            fcntl_flags as usize,
            ctx.uid,
            ctx.gid,
        )?;

        Ok(scheme_fd)
    }

    fn fork_namespace(&mut self, namespace: Rc<RefCell<Namespace>>, names: &[u8]) -> Result<usize> {
        let new_id = self.next_id;
        let new_namespace = namespace.borrow().fork(names).map_err(|e| {
            error!("Failed to fork namespace {}: {}", new_id, e);
            e
        })?;
        self.add_namespace(
            new_id,
            new_namespace,
            NsPermissions::all().difference(HIGH_PERMISSIONS),
        );
        self.next_id += 1;
        Ok(new_id)
    }

    fn shrink_permissions(
        &mut self,
        mut ns: NamespaceAccess,
        permission: NsPermissions,
    ) -> Result<usize> {
        ns.permission = ns.permission.intersection(permission);
        let next_id = self.next_id;
        self.handles.insert(next_id, Handle::Access(ns));
        self.next_id += 1;
        Ok(next_id)
    }
}

impl<'sock> SchemeSync for NamespaceScheme<'sock> {
    fn openat(
        &mut self,
        fd: usize,
        path: &str,
        flags: usize,
        fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        let ns_access = {
            let handle = self.handles.get(&fd);
            match handle {
                Some(Handle::Access(access)) => Some(access),
                _ => None,
            }
        }
        .ok_or_else(|| {
            error!("Namespace with ID {} not found", fd);
            Error::new(ENOENT)
        })?;
        let redox_path = RedoxPath::from_absolute(path).ok_or(Error::new(EINVAL))?;
        let (scheme, reference) = redox_path.as_parts().ok_or(Error::new(EINVAL))?;

        let res_fd = match scheme.as_ref() {
            "namespace" => self.open_namespace_resource(
                ns_access,
                reference.as_ref(),
                flags,
                fcntl_flags,
                ctx,
            )?,
            "" => {
                if !ns_access.has_permission(NsPermissions::LIST) {
                    error!("Permission denied to list schemes in namespace {}", fd);
                    return Err(Error::new(EACCES));
                }

                let new_id = self.next_id;
                self.next_id += 1;

                self.handles.insert(new_id, Handle::List(ns_access.clone()));

                return Ok(OpenResult::ThisScheme {
                    number: new_id,
                    flags: NewFdFlags::empty(),
                });
            }
            _ => self.open_scheme_resource(
                &ns_access.namespace.borrow(),
                scheme.as_ref(),
                reference.as_ref(),
                flags,
                fcntl_flags,
                ctx,
            )?,
        };

        Ok(OpenResult::OtherScheme { fd: res_fd })
    }

    fn dup(&mut self, id: usize, buf: &[u8], _ctx: &CallerCtx) -> Result<OpenResult> {
        let ns_access = self.get_ns_access(id).ok_or_else(|| {
            error!("Namespace with ID {} not found", id);
            Error::new(ENOENT)
        })?;

        let raw_kind = read_num::<usize>(buf)?;
        let Some(kind) = NsDup::try_from_raw(raw_kind) else {
            error!("Unknown dup kind: {}", raw_kind);
            return Err(Error::new(EINVAL));
        };
        let payload = &buf[mem::size_of::<NsDup>()..];
        let new_id = match kind {
            NsDup::ForkNs => {
                let ns = ns_access.namespace.clone();
                let _ = ns_access;
                self.fork_namespace(ns, payload)?
            }
            NsDup::ShrinkPermissions => self.shrink_permissions(
                ns_access.clone(),
                NsPermissions::from_bits_truncate(read_num::<usize>(payload)?),
            )?,
            NsDup::IssueRegister => {
                let name = core::str::from_utf8(payload).map_err(|_| Error::new(EINVAL))?;
                let scheme_name = RedoxScheme::new(name).ok_or_else(|| {
                    error!("Invalid scheme name: {}", name);
                    Error::new(EINVAL)
                })?;

                if !ns_access.has_permission(NsPermissions::INSERT) {
                    error!(
                        "Permission denied to issue register capability for namespace {}",
                        id
                    );
                    return Err(Error::new(EACCES));
                }
                let new_id = self.next_id;
                let register_cap = Handle::Register(SchemeRegister {
                    target_namespace: ns_access.namespace.clone(),
                    scheme_name: scheme_name.as_ref().to_string(),
                });
                self.handles.insert(new_id, register_cap);
                self.next_id += 1;
                new_id
            }
        };

        Ok(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::empty(),
        })
    }

    fn unlinkat(&mut self, fd: usize, path: &str, flags: usize, ctx: &CallerCtx) -> Result<()> {
        let ns_access = self.get_ns_access(fd).ok_or_else(|| {
            error!("Namespace with ID {} not found", fd);
            Error::new(ENOENT)
        })?;
        let mut ns = ns_access.namespace.borrow_mut();

        let redox_path = RedoxPath::from_absolute(path).ok_or(Error::new(EINVAL))?;
        let (scheme, reference) = redox_path.as_parts().ok_or(Error::new(EINVAL))?;
        if reference.as_ref().is_empty() {
            if !ns_access.has_permission(NsPermissions::DELETE) {
                error!("Permission denied to remove scheme for namespace {}", fd);
                return Err(Error::new(EACCES));
            }
            match ns.remove_scheme(scheme.as_ref()) {
                Some(_) => return Ok(()),
                None => {
                    error!("Scheme {} not found in namespace", scheme);
                    return Err(Error::new(ENODEV));
                }
            }
        }
        let Some(cap_fd) = ns.get_scheme_fd(scheme.as_ref()) else {
            error!("Scheme {} not found in namespace", scheme);
            return Err(Error::new(ENODEV));
        };

        syscall::unlinkat_with_filter(cap_fd.as_raw_fd(), reference, flags, ctx.uid, ctx.gid)?;

        Ok(())
    }

    fn on_close(&mut self, id: usize) {
        self.handles.remove(&id);
    }

    fn on_sendfd(&mut self, sendfd_request: &SendFdRequest) -> Result<usize> {
        let namespace_id = sendfd_request.id();
        let num_fds = sendfd_request.num_fds();

        let handle = self.handles.get(&namespace_id).ok_or_else(|| {
            error!("Namespace with ID {} not found", namespace_id);
            Error::new(ENOENT)
        })?;
        let Handle::Register(register_cap) = handle else {
            error!(
                "Handle with ID {} is not a register capability",
                namespace_id
            );
            return Err(Error::new(EACCES));
        };

        if num_fds == 0 {
            return Ok(0);
        }
        if num_fds > 1 {
            error!("Can only send one fd at a time");
            return Err(Error::new(EINVAL));
        }
        let mut new_fd = usize::MAX;
        if let Err(e) = sendfd_request.obtain_fd(
            &self.socket,
            FobtainFdFlags::UPPER_TBL,
            core::slice::from_mut(&mut new_fd),
        ) {
            error!("on_sendfd: obtain_fd failed with error: {:?}", e);
            return Err(e);
        }
        register_cap.register(FdGuard::new(new_fd))?;

        Ok(num_fds)
    }

    fn getdents<'buf>(
        &mut self,
        id: usize,
        mut buf: DirentBuf<&'buf mut [u8]>,
        opaque_offset: u64,
    ) -> Result<DirentBuf<&'buf mut [u8]>> {
        let Handle::List(ns_access) = self.handles.get(&id).ok_or(Error::new(EBADF))? else {
            return Err(Error::new(ENOTDIR));
        };

        if !ns_access.has_permission(NsPermissions::LIST) {
            return Err(Error::new(EACCES));
        }

        let ns = ns_access.namespace.borrow();

        let opaque_offset = opaque_offset as usize;
        for (i, (name, _)) in ns.schemes.iter().enumerate().skip(opaque_offset) {
            if name.is_empty() {
                continue;
            }
            if let Err(err) = buf.entry(DirEntry {
                kind: DirentKind::Unspecified,
                name: &name.clone(),
                inode: 0,
                next_opaque_id: i as u64 + 1,
            }) {
                if err.errno == EINVAL && i > opaque_offset {
                    // POSIX allows partial result of getdents
                    break;
                } else {
                    return Err(err);
                }
            }
        }

        Ok(buf)
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        let resource_stat = match self.handles.get(&id).ok_or(Error::new(EBADF))? {
            Handle::List(_) => Stat {
                st_mode: 0o444 | syscall::MODE_DIR,
                st_uid: 0,
                st_gid: 0,
                st_size: 0,
                ..Default::default()
            },
            Handle::Access(_) | Handle::Register(_) => Stat {
                st_mode: 0o666 | syscall::MODE_FILE,
                st_uid: 0,
                st_gid: 0,
                st_size: 0,
                ..Default::default()
            },
        };
        *stat = resource_stat;
        Ok(())
    }
}

trait NumFromBytes: Sized + Debug {
    fn from_le_bytes_slice(buffer: &[u8]) -> Result<Self, Error>;
}

macro_rules! num_from_bytes_impl {
    ($($t:ty),*) => {
        $(
            impl NumFromBytes for $t {
                fn from_le_bytes_slice(buffer: &[u8]) -> Result<Self, Error> {
                    let size = mem::size_of::<Self>();
                    let buffer_slice = buffer.get(..size).and_then(|s| s.try_into().ok());

                    if let Some(slice) = buffer_slice {
                        Ok(Self::from_le_bytes(slice))
                    } else {
                        error!(
                            "read_num: buffer is too short to read num of size {} (buffer len: {})",
                            size, buffer.len()
                        );
                        Err(Error::new(EINVAL))
                    }
                }
            }
        )*
    };
}

num_from_bytes_impl!(usize);

fn read_num<T>(buffer: &[u8]) -> Result<T, Error>
where
    T: NumFromBytes,
{
    T::from_le_bytes_slice(buffer)
}

pub fn run(
    sync_pipe: FdGuard,
    socket: Socket,
    schemes: HashMap<String, Arc<FdGuard>>,
    scheme_creation_cap: FdGuard,
) -> ! {
    let mut state = SchemeState::new();
    let mut scheme = NamespaceScheme::new(&socket, schemes, scheme_creation_cap);

    // send namespace fd to bootstrap
    let new_id = scheme.next_id;
    scheme.add_namespace(new_id, scheme.root_namespace.clone(), NsPermissions::all());
    scheme.next_id += 1;
    let cap_fd = scheme
        .socket
        .create_this_scheme_fd(0, new_id, 0, 0)
        .expect("nsmgr: failed to create namespace fd");
    let _ = syscall::call_wo(
        sync_pipe.as_raw_fd(),
        &cap_fd.to_ne_bytes(),
        CallFlags::FD,
        &[],
    );
    drop(sync_pipe);

    log::info!("bootstrap: namespace scheme start!");
    loop {
        let Some(req) = socket
            .next_request(SignalBehavior::Restart)
            .expect("bootstrap: failed to read scheme request from kernel")
        else {
            break;
        };
        match req.kind() {
            RequestKind::Call(req) => {
                let resp = req.handle_sync(&mut scheme, &mut state);

                if !socket
                    .write_response(resp, SignalBehavior::Restart)
                    .expect("bootstrap: failed to write scheme response to kernel")
                {
                    break;
                }
            }
            RequestKind::OnClose { id } => scheme.on_close(id),
            RequestKind::SendFd(sendfd_request) => {
                let result = scheme.on_sendfd(&sendfd_request);
                let resp = Response::new(result, sendfd_request);
                if !socket
                    .write_response(resp, SignalBehavior::Restart)
                    .expect("bootstrap: failed to write scheme response to kernel")
                {
                    break;
                }
            }

            _ => (),
        }
    }

    unreachable!()
}
