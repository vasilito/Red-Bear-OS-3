use std::arch::asm;

use rand_chacha::ChaCha20Rng;
use rand_core::RngCore;

pub const MODE_PERM: u16 = 0x0FFF;
pub const MODE_EXEC: u16 = 0o1;
pub const MODE_WRITE: u16 = 0o2;
pub const MODE_READ: u16 = 0o4;

#[cfg(target_arch = "x86_64")]
use raw_cpuid::CpuId;

use redox_scheme::{scheme::SchemeSync, CallerCtx, OpenResult, Socket};
use scheme_utils::{Blocking, FpathWriter, HandleMap};
use syscall::data::Stat;
use syscall::flag::{EventFlags, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_WRONLY};
use syscall::schemev2::NewFdFlags;
use syscall::{Error, Result, EACCES, EBADF, EEXIST, ENOENT, EPERM, MODE_CHR};

// Create an RNG Seed to create initial seed from the rdrand intel instruction
use rand_core::SeedableRng;
use sha2::{Digest, Sha256};

// This Daemon implements a Cryptographically Secure Random Number Generator
// that does not block on read - i.e. it is equivalent to linux /dev/urandom
// We do not implement blocking reads as per linux /dev/random for the reasons outlined
// here: https://www.2uo.de/myths-about-urandom/

// Default file access mode for PRNG
const DEFAULT_PRNG_MODE: u16 = 0o644;
// Rand crate recommends at least 256 bits of entropy to seed the RNG
const SEED_BYTES: usize = 32;

/// Create a true random seed for the RNG if hardware support is present.
/// On Intel x64 from rdrand instruction.
/// On AArch64 from RNDRRS system register.
/// Will seed with a zero (insecure) if getting support is not present.
fn create_rdrand_seed() -> [u8; SEED_BYTES] {
    let mut rng = [0; SEED_BYTES];
    let mut have_seeded = false;
    #[cfg(target_arch = "x86_64")]
    {
        if CpuId::new().get_feature_info().unwrap().has_rdrand() {
            for i in 0..SEED_BYTES / 8 {
                // We get 8 bytes at a time from rdrand instruction
                let rand: u64;
                unsafe {
                    asm!("rdrand rax", out("rax") rand);
                }

                rng[i * 8..(i * 8 + 8)].copy_from_slice(&rand.to_le_bytes());
            }
            have_seeded = true;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        fn is_cpu_feature_detected(feature: &str) -> bool {
            if let Ok(cpu) = std::fs::read_to_string("/scheme/sys/cpu") {
                return cpu
                    .lines()
                    .find_map(|s| s.strip_prefix("Features:"))
                    .map(|s| s.split(" ").find(|s| s == &feature))
                    .is_some();
            }
            false
        }
        if is_cpu_feature_detected("rand") {
            let mut failure = false;
            for i in 0..SEED_BYTES / 8 {
                // We get 8 bytes at a time from RNDRRS register
                let rand: u64;
                unsafe {
                    asm!("mrs {}, s3_3_c2_c4_1", out(reg) rand); // rndrrs
                }
                failure |= rand == 0;
                rng[i * 8..(i * 8 + 8)].copy_from_slice(&rand.to_le_bytes());
            }
            have_seeded = !failure;
        }
    } // TODO integrate alternative entropy sources
    if !have_seeded {
        println!("randd: Seeding failed, no entropy source.  Random numbers on this platform are NOT SECURE");
    }
    rng
}

/// Contains information about an open file
struct OpenFileInfo {
    o_flags: usize,
    /// Flags used when opening file.
    uid: u32,
    gid: u32,
    file_stat: Stat,
}

impl OpenFileInfo {
    /// Tests if the current user has enough permissions to view the file, op is the operation,
    /// like read and write, these modes are MODE_EXEC, MODE_READ, and MODE_WRITE
    /// Copied from redoxfs
    fn permission(&self, op: u16) -> bool {
        let mut perm = self.file_stat.st_mode & 0o7;
        if self.uid == self.file_stat.st_uid {
            // If self.mode is 101100110, >> 6 would be 000000101
            // 0o7 is octal for 111, or, when expanded to 9 digits is 000000111
            perm |= (self.file_stat.st_mode >> 6) & 0o7;
            // Since we erased the GID and OTHER bits when >>6'ing, |= will keep those bits in place.
        }
        if self.gid == self.file_stat.st_gid || self.file_stat.st_gid == 0 {
            perm |= (self.file_stat.st_mode >> 3) & 0o7;
        }
        if self.uid == 0 {
            //set the `other` bits to 111
            perm |= 0o7;
        }
        perm & op == op
    }
    fn o_flag_set(&self, f: usize) -> bool {
        return (f & self.o_flags) == f;
    }
}

enum Handle {
    File(OpenFileInfo),
    SchemeRoot,
}

impl Handle {
    fn as_file(&self) -> Option<&OpenFileInfo> {
        match self {
            Self::File(info) => Some(info),
            _ => None,
        }
    }
}

/// Struct to represent the rand scheme.
struct RandScheme {
    prng: ChaCha20Rng,
    // ChaCha20 is a Cryptographically Secure PRNG
    // https://docs.rs/rand/0.5.0/rand/prng/chacha/struct.ChaChaRng.html
    // Allows 2^64 streams of random numbers, which we will equate with file numbers
    prng_stat: Stat,
    handles: HandleMap<Handle>,
}

impl RandScheme {
    /// Create new rand scheme from a message socket
    fn new() -> RandScheme {
        RandScheme {
            prng: ChaCha20Rng::from_seed(create_rdrand_seed()),
            prng_stat: Stat {
                st_mode: MODE_CHR | DEFAULT_PRNG_MODE,
                st_gid: 0,
                st_uid: 0,
                ..Default::default()
            },
            handles: HandleMap::new(),
        }
    }

    /// Gets the open file info for a file descriptor if it is open - error otherwise.
    fn get_fd(&self, fd: usize) -> Result<&OpenFileInfo> {
        // Check we've got a valid file descriptor
        let handle = self.handles.get(fd)?;
        handle.as_file().ok_or(Error::new(EBADF))
    }
    /// Checks to see if the op (MODE_READ, MODE_WRITE) can be performed on the open file
    /// descriptor - Will return the open file info if successful, and error if the file
    /// descriptor is invalid, or the permission is denied.
    fn can_perform_op_on_fd(&self, fd: usize, op: u16) -> Result<&OpenFileInfo> {
        let file_info = self.get_fd(fd)?;
        if !file_info.permission(op) {
            return Err(Error::new(EPERM));
        }
        Ok(file_info)
    }
    /// Reseed the CSPRNG with the supplied entropy.
    /// TODO add this to an entropy pool and give a limited estimate to the amount of entropy
    /// TODO consider having trusted and untrusted entropy URIs, with different permissions.
    fn reseed_prng(&mut self, entropy: &[u8]) {
        // Need to fill a fixed size array for the from_seed, so we'll do 256 bit
        // array and has the entropy into it.
        let mut digest = Sha256::new();
        digest.input(entropy);
        let hash = digest.result();
        let mut entropy_array: [u8; SEED_BYTES] = [0; SEED_BYTES];
        entropy_array.copy_from_slice(hash.as_slice());
        self.prng = ChaCha20Rng::from_seed(entropy_array);
    }

    fn open_inner(&mut self, path: &str, flags: usize, ctx: &CallerCtx) -> Result<OpenResult> {
        // We are only allowing
        // reads/writes from /scheme/rand/ and /scheme/rand/urandom - the root directory on its own is passed as an empty slice
        if path != "" && path != "/urandom" {
            return Err(Error::new(ENOENT));
        }

        if flags & (O_CREAT | O_EXCL) == O_CREAT | O_EXCL {
            return Err(Error::new(EEXIST));
        }

        let open_file_info = OpenFileInfo {
            o_flags: flags,
            file_stat: self.prng_stat,
            uid: ctx.uid,
            gid: ctx.gid,
        };

        if (open_file_info.o_flag_set(O_RDONLY) || open_file_info.o_flag_set(O_RDWR))
            && !open_file_info.permission(MODE_READ)
        {
            return Err(Error::new(EPERM));
        }
        if (open_file_info.o_flag_set(O_WRONLY) || open_file_info.o_flag_set(O_RDWR))
            && !open_file_info.permission(MODE_WRITE)
        {
            return Err(Error::new(EPERM));
        }

        let id = self.handles.insert(Handle::File(open_file_info));

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }
}
#[test]
fn test_scheme_perms() {
    use syscall::{O_CLOEXEC, O_STAT};

    let mut ctx = CallerCtx {
        pid: 0,
        uid: 1,
        gid: 1,
        id: unsafe { std::mem::zeroed() }, // Id doesn't have a public constructor
    };

    let mut scheme = RandScheme::new();
    scheme.prng_stat.st_mode = MODE_CHR | 0o200;
    scheme.prng_stat.st_uid = 1;
    scheme.prng_stat.st_gid = 1;
    assert!(scheme.open_inner("/", O_RDWR, &ctx).is_err());
    assert!(scheme.open_inner("/", O_RDONLY, &ctx).is_err());

    scheme.prng_stat.st_mode = MODE_CHR | 0o400;
    let mut fd = match scheme.open("", O_RDONLY, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_ok());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_err());
    scheme.on_close(fd);

    assert!(scheme.open_inner("", O_WRONLY, &ctx).is_err());
    assert!(scheme.open_inner("", O_RDWR, &ctx).is_err());

    scheme.prng_stat.st_mode = MODE_CHR | 0o600;
    fd = match scheme.open_inner("", O_RDWR, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_ok());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_ok());
    scheme.on_close(fd);

    ctx.uid = 2;
    ctx.gid = 2;
    fd = match scheme.open_inner("", O_STAT, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_err());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_err());
    scheme.on_close(fd);
    fd = match scheme.open_inner("", O_STAT | O_CLOEXEC, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_err());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_err());
    scheme.on_close(fd);

    // Try another user in group (no group perms)
    ctx.uid = 2;
    ctx.gid = 1;
    fd = match scheme.open_inner("", O_STAT | O_CLOEXEC, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_err());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_err());
    scheme.on_close(fd);
    scheme.prng_stat.st_mode = MODE_CHR | 0o660;
    fd = match scheme.open_inner("", O_STAT | O_CLOEXEC, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_ok());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_ok());
    scheme.on_close(fd);

    // Check root can do anything
    scheme.prng_stat.st_mode = MODE_CHR | 0o000;
    ctx.uid = 0;
    ctx.gid = 0;
    fd = match scheme.open_inner("", O_STAT | O_CLOEXEC, &ctx).unwrap() {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_ok());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_ok());
    scheme.on_close(fd);

    // Check the rand:/urandom URL (Equivalent to rand:/)
    scheme.prng_stat.st_mode = MODE_CHR | 0o660;
    ctx.uid = 2;
    ctx.gid = 1;
    fd = match scheme
        .open_inner("/urandom", O_STAT | O_CLOEXEC, &ctx)
        .unwrap()
    {
        OpenResult::ThisScheme { number, .. } => number,
        _ => panic!(),
    };
    assert!(scheme.can_perform_op_on_fd(fd, MODE_READ).is_ok());
    assert!(scheme.can_perform_op_on_fd(fd, MODE_WRITE).is_ok());
    scheme.on_close(fd);
}

impl SchemeSync for RandScheme {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        flags: usize,
        _fcntl_flags: u32,
        ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        self.open_inner(path, flags, ctx)
    }

    /* Resource operations */
    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        // Check fd and permissions
        self.can_perform_op_on_fd(id, MODE_READ)?;

        // Setting the stream will ensure that if two clients are reading concurrently, they won't get the same numbers
        self.prng.set_stream(id as u64); // Should probably find a way to re-instate the counter for this stream, but
                                         // not doing so won't make the output any less 'random'
        self.prng.fill_bytes(buf);

        Ok(buf.len())
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        // Check fd and permissions
        self.can_perform_op_on_fd(id, MODE_WRITE)?;

        // TODO - when we support other entropy sources, just add this to an entropy pool
        // TODO - consider having trusted and untrusted entropy writing paths
        // We have a healthy mistrust of the entropy we're being given, so we won't seed just with
        // that as the resulting numbers would be predictable based on this input
        // we'll take 512 bits (arbitrary) from the current PRNG, and seed with that
        // and the supplied data.

        let mut rng_buf: [u8; SEED_BYTES] = [0; SEED_BYTES];
        self.prng.fill_bytes(&mut rng_buf);
        let mut rng_vec = Vec::new();
        rng_vec.extend(&rng_buf);
        rng_vec.extend(buf);
        self.reseed_prng(&rng_vec);
        Ok(buf.len())
    }

    fn fchmod(&mut self, id: usize, mode: u16, ctx: &CallerCtx) -> Result<()> {
        // Check fd and permissions
        let file_info = self.get_fd(id)?;
        // only root and owner can chmod
        if ctx.uid != file_info.file_stat.st_uid && ctx.uid != 0 {
            return Err(Error::new(EPERM));
        }

        self.prng_stat.st_mode = MODE_CHR | (mode & MODE_PERM); // Apply mask
        Ok(())
    }

    fn fchown(&mut self, id: usize, uid: u32, gid: u32, ctx: &CallerCtx) -> Result<()> {
        // Check fd and permissions
        let file_info = self.get_fd(id)?;
        // only root and owner can fchown
        if ctx.uid != file_info.file_stat.st_uid && ctx.uid != 0 {
            return Err(Error::new(EPERM));
        }

        self.prng_stat.st_uid = uid;
        self.prng_stat.st_gid = gid;
        Ok(())
    }

    fn fcntl(&mut self, _id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        // Just ignore this.
        Ok(0)
    }

    fn fevent(&mut self, _id: usize, _flags: EventFlags, _ctx: &CallerCtx) -> Result<EventFlags> {
        Ok(EventFlags::EVENT_READ)
    }
    fn fpath(&mut self, _file: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with(buf, "rand", |_| Ok(()))
    }

    fn fstat(&mut self, file: usize, stat: &mut Stat, _ctx: &CallerCtx) -> Result<()> {
        // Check fd and permissions
        self.can_perform_op_on_fd(file, MODE_READ)?;

        *stat = self.prng_stat.clone();

        Ok(())
    }

    fn on_close(&mut self, file: usize) {
        // just remove the file descriptor from the open descriptors
        self.handles.remove(file);
    }
}

fn daemon(daemon: daemon::SchemeDaemon) -> ! {
    let socket = Socket::create().expect("randd: failed to create rand scheme");

    let mut scheme = RandScheme::new();
    let handler = Blocking::new(&socket, 16);

    let _ = daemon.ready_sync_scheme(&socket, &mut scheme);

    libredox::call::setrens(0, 0).expect("randd: failed to enter null namespace");

    handler
        .process_requests_blocking(scheme)
        .expect("randd: failed to process events from zero scheme");
}

fn main() {
    daemon::SchemeDaemon::new(daemon);
}
