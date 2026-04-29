use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::str::FromStr;
use hashbrown::HashMap;
use redox_scheme::Socket;

use syscall::CallFlags;
use syscall::data::{GlobalSchemes, KernelSchemeInfo};
use syscall::flag::{O_CLOEXEC, O_RDONLY, O_STAT};
use syscall::{EINTR, Error};

use redox_rt::proc::*;

use crate::KernelSchemeMap;

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }
    fn log(&self, record: &log::Record) {
        let file = record.file().unwrap_or("");
        let line = record.line().unwrap_or(0);
        let level = record.level();
        let msg = record.args();
        let _ = syscall::write(
            1,
            alloc::format!("[{file}:{line} {level}] {msg}\n").as_bytes(),
        );
    }
    fn flush(&self) {}
}

const KERNEL_METADATA_BASE: usize = crate::arch::USERMODE_END - syscall::KERNEL_METADATA_SIZE;

pub fn main() -> ! {
    let mut cursor = KERNEL_METADATA_BASE;
    let kernel_scheme_infos = unsafe {
        let base_ptr = cursor as *const u8;
        let infos_len = *(base_ptr as *const usize);
        let infos_ptr = base_ptr.add(core::mem::size_of::<usize>()) as *const KernelSchemeInfo;
        let slice = core::slice::from_raw_parts(infos_ptr, infos_len);
        cursor += core::mem::size_of::<usize>() // kernel scheme number size
            + infos_len // kernel scheme number
            * core::mem::size_of::<KernelSchemeInfo>();
        slice
    };
    let scheme_creation_cap = unsafe {
        let base_ptr = cursor as *const u8;
        FdGuard::new(*(base_ptr as *const usize))
    };

    let mut kernel_schemes = KernelSchemeMap::new(kernel_scheme_infos);

    let auth = kernel_schemes
        .0
        .remove(&GlobalSchemes::Proc)
        .expect("failed to get proc fd");

    let this_thr_fd = auth
        .dup(b"cur-context")
        .expect("failed to open open_via_dup")
        .to_upper()
        .unwrap();
    let this_thr_fd = unsafe { redox_rt::initialize_freestanding(this_thr_fd) };

    let mut env_bytes = [0_u8; 4096];
    let mut envs = {
        let fd = FdGuard::new(
            syscall::openat(
                kernel_schemes
                    .get(GlobalSchemes::Sys)
                    .expect("failed to get sys fd")
                    .as_raw_fd(),
                "env",
                O_RDONLY | O_CLOEXEC,
                0,
            )
            .expect("bootstrap: failed to open env"),
        );
        let bytes_read = fd
            .read(&mut env_bytes)
            .expect("bootstrap: failed to read env");

        if bytes_read >= env_bytes.len() {
            // TODO: Handle this, we can allocate as much as we want in theory.
            panic!("env is too large");
        }
        let env_bytes = &mut env_bytes[..bytes_read];

        env_bytes
            .split(|&c| c == b'\n')
            .filter(|var| !var.is_empty())
            .filter(|var| !var.starts_with(b"INITFS_"))
            .collect::<Vec<_>>()
    };
    envs.push(b"RUST_BACKTRACE=1");
    //envs.push(b"LD_DEBUG=all");
    envs.push(b"LD_LIBRARY_PATH=/scheme/initfs/lib");

    log::set_max_level(log::LevelFilter::Warn);

    if let Some(log_env) = envs
        .iter()
        .find_map(|var| var.strip_prefix(b"BOOTSTRAP_LOG_LEVEL="))
    {
        if let Ok(Ok(log_level)) = str::from_utf8(&log_env).map(|s| log::LevelFilter::from_str(s)) {
            log::set_max_level(log_level);
        }
    }

    let _ = log::set_logger(&Logger);

    unsafe extern "C" {
        // The linker script will define this as the location of the initfs header.
        static __initfs_header: u8;

        // The linker script will define this as the end of the executable (excluding initfs).
        static __bss_end: u8;
    }

    let initfs_start = core::ptr::addr_of!(__initfs_header);
    let initfs_length = unsafe {
        (*(core::ptr::addr_of!(__initfs_header) as *const redox_initfs::types::Header))
            .initfs_size
            .get() as usize
    };

    let (scheme_creation_cap, auth, kernel_schemes, initfs_fd) = spawn(
        "initfs daemon",
        auth,
        &this_thr_fd,
        scheme_creation_cap,
        kernel_schemes,
        false,
        |write_fd, socket, _, _| unsafe {
            crate::initfs::run(
                core::slice::from_raw_parts(initfs_start, initfs_length),
                write_fd,
                socket,
            );
        },
    );

    // Unmap initfs data as only the initfs scheme implementation needs it.
    unsafe {
        let executable_end = core::ptr::addr_of!(__bss_end)
            .add(core::ptr::addr_of!(__bss_end).align_offset(syscall::PAGE_SIZE));
        syscall::funmap(
            executable_end as usize,
            initfs_length.next_multiple_of(syscall::PAGE_SIZE)
                - (executable_end.offset_from(initfs_start) as usize),
        )
        .unwrap();
    }

    let (scheme_creation_cap, auth, kernel_schemes, proc_fd) = spawn(
        "process manager",
        auth,
        &this_thr_fd,
        scheme_creation_cap,
        kernel_schemes,
        true,
        |write_fd, socket, auth, mut kernel_schemes| {
            let event = kernel_schemes
                .0
                .remove(&GlobalSchemes::Event)
                .expect("failed to get event fd");
            drop(kernel_schemes);
            crate::procmgr::run(write_fd, socket, auth, event)
        },
    );

    let scheme_creation_cap_dup = scheme_creation_cap
        .dup(b"")
        .expect("failed to dup scheme creation cap");
    let (_, _, _, initns_fd) = spawn(
        "init namespace manager",
        auth,
        &this_thr_fd,
        scheme_creation_cap,
        kernel_schemes,
        false,
        |write_fd, socket, _, kernel_schemes| {
            let mut schemes = HashMap::default();
            for (scheme, fd) in kernel_schemes.0.into_iter() {
                schemes.insert(scheme.as_str().to_string(), Arc::new(fd));
            }
            schemes.insert(
                "proc".to_string(),
                // A bit dirty, but necessary as the parent process still needs access to it. Rust
                // doesn't know that the fd got cloned by fork.
                Arc::new(FdGuard::new(proc_fd.as_raw_fd())),
            );
            schemes.insert("initfs".to_string(), Arc::new(initfs_fd));

            crate::initnsmgr::run(write_fd, socket, schemes, scheme_creation_cap_dup)
        },
    );

    let (init_proc_fd, init_thr_fd) = unsafe { make_init(proc_fd.take()) };
    // from this point, this_thr_fd is no longer valid

    const CWD: &[u8] = b"/scheme/initfs";
    let cwd_fd = FdGuard::new(
        syscall::openat(initns_fd.as_raw_fd(), "/scheme/initfs", O_STAT, 0)
            .expect("failed to open cwd fd"),
    )
    .to_upper()
    .unwrap();
    let extrainfo = ExtraInfo {
        cwd: Some(CWD),
        sigprocmask: 0,
        sigignmask: 0,
        umask: redox_rt::sys::get_umask(),
        thr_fd: init_thr_fd.as_raw_fd(),
        proc_fd: init_proc_fd.as_raw_fd(),
        ns_fd: Some(initns_fd.take()),
        cwd_fd: Some(cwd_fd.as_raw_fd()),
    };

    let exe_path = "/scheme/initfs/bin/init";

    let image_file = FdGuard::new(
        syscall::openat(extrainfo.ns_fd.unwrap(), exe_path, O_RDONLY | O_CLOEXEC, 0)
            .expect("failed to open init"),
    )
    .to_upper()
    .unwrap();

    let FexecResult::Interp {
        path: interp_path,
        interp_override,
    } = fexec_impl(
        image_file,
        init_thr_fd,
        init_proc_fd,
        exe_path.as_bytes(),
        &[exe_path.as_bytes()],
        &envs,
        &extrainfo,
        None,
    )
    .expect("failed to execute init");

    // According to elf(5), PT_INTERP requires that the interpreter path be
    // null-terminated. Violating this should therefore give the "format error" ENOEXEC.
    let interp_cstr = CStr::from_bytes_with_nul(&interp_path).expect("interpreter not valid C str");
    let interp_file = FdGuard::new(
        syscall::openat(
            extrainfo.ns_fd.unwrap(), // initns, not initfs!
            interp_cstr.to_str().expect("interpreter not UTF-8"),
            O_RDONLY | O_CLOEXEC,
            0,
        )
        .expect("failed to open dynamic linker"),
    )
    .to_upper()
    .unwrap();

    fexec_impl(
        interp_file,
        init_thr_fd,
        init_proc_fd,
        exe_path.as_bytes(),
        &[exe_path.as_bytes()],
        &envs,
        &extrainfo,
        Some(interp_override),
    )
    .expect("failed to execute init");

    unreachable!()
}

pub(crate) fn spawn(
    name: &str,
    auth: FdGuard,
    this_thr_fd: &FdGuardUpper,
    scheme_creation_cap: FdGuard,
    kernel_schemes: KernelSchemeMap,
    nonblock: bool,
    inner: impl FnOnce(FdGuard, Socket, FdGuard, KernelSchemeMap) -> !,
) -> (FdGuard, FdGuard, KernelSchemeMap, FdGuard) {
    let read = FdGuard::new(
        syscall::openat(
            kernel_schemes
                .get(GlobalSchemes::Pipe)
                .expect("failed to get pipe fd")
                .as_raw_fd(),
            "",
            O_CLOEXEC,
            0,
        )
        .expect("failed to open sync read pipe"),
    );

    // The write pipe will not inherit O_CLOEXEC, but is closed by the daemon later.
    let write = FdGuard::new(
        syscall::dup(read.as_raw_fd(), b"write").expect("failed to open sync write pipe"),
    );

    match fork_impl(&ForkArgs::Init {
        this_thr_fd,
        auth: &auth,
    }) {
        Err(err) => {
            panic!("Failed to fork in order to start {name}: {err}");
        }
        // Continue serving the scheme as the child.
        Ok(0) => {
            drop(read);

            let socket = Socket::create_inner(scheme_creation_cap.as_raw_fd(), nonblock)
                .expect("failed to open proc scheme socket");
            drop(scheme_creation_cap);

            inner(write, socket, auth, kernel_schemes)
        }
        // Return in order to execute init, as the parent.
        Ok(_) => {
            drop(write);

            let mut new_fd = usize::MAX;
            let fd_bytes = unsafe {
                core::slice::from_raw_parts_mut(
                    core::slice::from_mut(&mut new_fd).as_mut_ptr() as *mut u8,
                    core::mem::size_of::<usize>(),
                )
            };
            loop {
                match syscall::call_ro(
                    read.as_raw_fd(),
                    fd_bytes,
                    CallFlags::FD | CallFlags::FD_UPPER,
                    &[],
                ) {
                    Err(Error { errno: EINTR }) => continue,
                    _ => break,
                }
            }

            (
                scheme_creation_cap,
                auth,
                kernel_schemes,
                FdGuard::new(new_fd),
            )
        }
    }
}
