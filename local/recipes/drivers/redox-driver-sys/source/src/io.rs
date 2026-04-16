#[cfg(all(target_arch = "x86_64", target_os = "redox"))]
use syscall as redox_syscall;

use crate::Result;

#[cfg(all(target_arch = "x86_64", target_os = "redox"))]
pub fn acquire_iopl() -> Result<()> {
    extern "C" {
        fn redox_cur_thrfd_v0() -> usize;
    }
    let kernel_fd = redox_syscall::dup(unsafe { redox_cur_thrfd_v0() }, b"open_via_dup")?;
    let res = libredox::call::call_wo(
        kernel_fd,
        &[],
        redox_syscall::CallFlags::empty(),
        &[redox_syscall::ProcSchemeVerb::Iopl as u64],
    );
    let _ = redox_syscall::close(kernel_fd);
    res.map(|_| ()).map_err(|e| e.into())
}

#[cfg(all(target_arch = "x86_64", not(target_os = "redox")))]
pub fn acquire_iopl() -> Result<()> {
    Err(crate::DriverError::Other(String::from(
        "acquire_iopl: only available on Redox",
    )))
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe { core::arch::asm!("inb {1:x}, {0}", out(reg_byte) val, in(reg) port) };
    val
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub fn outb(port: u16, val: u8) {
    unsafe { core::arch::asm!("outb {1:x}, {0}", in(reg_byte) val, in(reg) port) };
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe { core::arch::asm!("inl {1:x}, {0:e}", out(reg) val, in(reg) port) };
    val
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub fn outl(port: u16, val: u32) {
    unsafe { core::arch::asm!("outl {1:x}, {0:e}", in(reg) val, in(reg) port) };
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub fn inw(port: u16) -> u16 {
    let val: u16;
    unsafe { core::arch::asm!("inw {1:x}, {0:x}", out(reg) val, in(reg) port) };
    val
}

#[cfg(target_arch = "x86_64")]
#[inline]
pub fn outw(port: u16, val: u16) {
    unsafe { core::arch::asm!("outw {1:x}, {0:x}", in(reg) val, in(reg) port) };
}
