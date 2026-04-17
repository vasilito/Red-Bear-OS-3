use std::{
    env,
    fs::File,
    io::{self, Read, Write},
    os::unix::io::{FromRawFd, RawFd},
    process,
    sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(feature = "redox")]
use fat_blockdev::RedoxDisk as SchemeDisk;
#[cfg(not(feature = "redox"))]
use fat_blockdev::FileDisk as SchemeDisk;

mod handle;
mod mount;
mod scheme;

pub static IS_UMT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn unmount_handler(_signal: usize) {
    IS_UMT.store(1, Ordering::SeqCst);
}

fn install_sigterm_handler() -> io::Result<()> {
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        if libc::sigemptyset(&mut action.sa_mask) != 0 {
            return Err(io::Error::last_os_error());
        }
        action.sa_flags = 0;
        action.sa_sigaction = unmount_handler as usize;

        if libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()) != 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

fn fork_process() -> io::Result<libc::pid_t> {
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(pid)
    }
}

fn make_pipe() -> io::Result<[i32; 2]> {
    let mut pipes = [0; 2];
    if unsafe { libc::pipe(pipes.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pipes)
}

#[cfg(target_os = "redox")]
fn capability_mode() {
    if let Err(err) = libredox::call::setrens(0, 0) {
        log::error!("fatd: failed to enter null namespace: {err}");
    }
}

#[cfg(not(target_os = "redox"))]
fn capability_mode() {}

fn usage() {
    eprintln!("fatd [--no-daemon|-d] <disk_path> <mountpoint>");
}

fn fail_usage(message: &str) -> ! {
    eprintln!("fatd: {message}");
    usage();
    process::exit(1);
}

fn run_mount(disk_path: &str, mountpoint: &str) -> Result<(), String> {
    let disk = SchemeDisk::open(disk_path).map_err(|err| format!("failed to open {disk_path}: {err}"))?;
    let disk = fscommon::BufStream::new(disk);
    let filesystem = fatfs::FileSystem::new(disk, fatfs::FsOptions::new())
        .map_err(|err| format!("failed to mount FAT on {disk_path}: {err}"))?;

    mount::mount(filesystem, mountpoint, |mounted_path| {
        capability_mode();
        log::info!("mounted FAT filesystem on {disk_path} to {mounted_path}");
    })
    .map_err(|err| format!("failed to serve scheme {mountpoint}: {err}"))
}

fn daemon(disk_path: &str, mountpoint: &str, mut status_pipe: Option<File>) -> i32 {
    IS_UMT.store(0, Ordering::SeqCst);

    if let Err(err) = install_sigterm_handler() {
        log::error!("failed to install SIGTERM handler: {err}");
        if let Some(pipe) = status_pipe.as_mut() {
            let _ = pipe.write_all(&[1]);
        }
        return 1;
    }

    match run_mount(disk_path, mountpoint) {
        Ok(()) => {
            if let Some(pipe) = status_pipe.as_mut() {
                let _ = pipe.write_all(&[0]);
            }
            0
        }
        Err(err) => {
            log::error!("{err}");
            if let Some(pipe) = status_pipe.as_mut() {
                let _ = pipe.write_all(&[1]);
            }
            1
        }
    }
}

fn main() {
    #[cfg(feature = "redox")]
    env_logger::init();

    let mut daemonize = true;
    let mut disk_path: Option<String> = None;
    let mut mountpoint: Option<String> = None;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--no-daemon" | "-d" => daemonize = false,
            _ if disk_path.is_none() => disk_path = Some(arg),
            _ if mountpoint.is_none() => mountpoint = Some(arg),
            _ => fail_usage("too many arguments provided"),
        }
    }

    let Some(disk_path) = disk_path else {
        fail_usage("no disk path provided");
    };
    let Some(mountpoint) = mountpoint else {
        fail_usage("no mountpoint provided");
    };

    if daemonize {
        let pipes = match make_pipe() {
            Ok(pipes) => pipes,
            Err(err) => {
                eprintln!("fatd: failed to create pipe: {err}");
                process::exit(1);
            }
        };

        let mut read = unsafe { File::from_raw_fd(pipes[0] as RawFd) };
        let write = unsafe { File::from_raw_fd(pipes[1] as RawFd) };

        match fork_process() {
            Ok(0) => {
                drop(read);
                process::exit(daemon(&disk_path, &mountpoint, Some(write)));
            }
            Ok(_pid) => {
                drop(write);
                let mut response = [1u8; 1];
                if let Err(err) = read.read_exact(&mut response) {
                    eprintln!("fatd: failed to read child status: {err}");
                    process::exit(1);
                }
                process::exit(i32::from(response[0]));
            }
            Err(err) => {
                eprintln!("fatd: failed to fork: {err}");
                process::exit(1);
            }
        }
    } else {
        log::info!("running fatd in foreground");
        process::exit(daemon(&disk_path, &mountpoint, None));
    }
}
