use syscall::flag::MapFlags;

mod offsets {
    unsafe extern "C" {
        // text (R-X)
        static __text_start: u8;
        static __text_end: u8;
        // rodata (R--)
        static __rodata_start: u8;
        static __rodata_end: u8;
        // data+bss (RW-)
        static __data_start: u8;
        static __bss_end: u8;
    }
    pub fn text() -> (usize, usize) {
        unsafe {
            (
                &__text_start as *const u8 as usize,
                &__text_end as *const u8 as usize,
            )
        }
    }
    pub fn rodata() -> (usize, usize) {
        unsafe {
            (
                &__rodata_start as *const u8 as usize,
                &__rodata_end as *const u8 as usize,
            )
        }
    }
    pub fn data_and_bss() -> (usize, usize) {
        unsafe {
            (
                &__data_start as *const u8 as usize,
                &__bss_end as *const u8 as usize,
            )
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn start() -> ! {
    // Remap self, from the previous RWX

    let (text_start, text_end) = offsets::text();
    let (rodata_start, rodata_end) = offsets::rodata();
    let (data_start, data_end) = offsets::data_and_bss();

    // NOTE: Assuming the debug scheme root fd is always placed at this position
    let debug_fd = syscall::UPPER_FDTBL_TAG + syscall::data::GlobalSchemes::Debug as usize;
    let _ = syscall::openat(debug_fd, "", syscall::O_RDONLY, 0); // stdin
    let _ = syscall::openat(debug_fd, "", syscall::O_WRONLY, 0); // stdout
    let _ = syscall::openat(debug_fd, "", syscall::O_WRONLY, 0); // stderr

    unsafe {
        let _ = syscall::mprotect(4096, 4096, MapFlags::PROT_READ | MapFlags::MAP_PRIVATE)
            .expect("mprotect failed for initfs header page");

        let _ = syscall::mprotect(
            text_start,
            text_end - text_start,
            MapFlags::PROT_READ | MapFlags::PROT_EXEC | MapFlags::MAP_PRIVATE,
        )
        .expect("mprotect failed for .text");
        let _ = syscall::mprotect(
            rodata_start,
            rodata_end - rodata_start,
            MapFlags::PROT_READ | MapFlags::MAP_PRIVATE,
        )
        .expect("mprotect failed for .rodata");
        let _ = syscall::mprotect(
            data_start,
            data_end - data_start,
            MapFlags::PROT_READ | MapFlags::PROT_WRITE | MapFlags::MAP_PRIVATE,
        )
        .expect("mprotect failed for .data/.bss");
        let _ = syscall::mprotect(
            data_end,
            crate::arch::STACK_START - data_end,
            MapFlags::PROT_READ | MapFlags::MAP_PRIVATE,
        )
        .expect("mprotect failed for rest of memory");
    }

    crate::exec::main();
}
