#![no_std]
#![no_main]
#![allow(internal_features)]
#![feature(core_intrinsics, str_from_raw_parts, never_type)]

#[cfg(target_arch = "aarch64")]
#[path = "aarch64.rs"]
pub mod arch;

#[cfg(target_arch = "x86")]
#[path = "i686.rs"]
pub mod arch;

#[cfg(target_arch = "x86_64")]
#[path = "x86_64.rs"]
pub mod arch;

#[cfg(target_arch = "riscv64")]
#[path = "riscv64.rs"]
pub mod arch;

pub mod exec;
pub mod initfs;
pub mod initnsmgr;
pub mod procmgr;
pub mod start;

extern crate alloc;

use core::cell::UnsafeCell;

use alloc::collections::btree_map::BTreeMap;
use redox_rt::proc::FdGuard;
use syscall::data::Map;
use syscall::data::{GlobalSchemes, KernelSchemeInfo};
use syscall::flag::MapFlags;

#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;

    struct Writer;

    impl Write for Writer {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            syscall::write(1, s.as_bytes())
                .map_err(|_| core::fmt::Error)
                .map(|_| ())
        }
    }

    let _ = writeln!(&mut Writer, "{}", info);
    core::intrinsics::abort();
}

const HEAP_OFF: usize = arch::USERMODE_END / 2;

struct Allocator;
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

struct AllocStateInner {
    heap: Option<linked_list_allocator::Heap>,
    heap_top: usize,
}
struct AllocState(UnsafeCell<AllocStateInner>);
unsafe impl Send for AllocState {}
unsafe impl Sync for AllocState {}
static ALLOC_STATE: AllocState = AllocState(UnsafeCell::new(AllocStateInner {
    heap: None,
    heap_top: HEAP_OFF + SIZE,
}));

const SIZE: usize = 1024 * 1024;
const HEAP_INCREASE_BY: usize = SIZE;

unsafe impl alloc::alloc::GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let state = unsafe { &mut (*ALLOC_STATE.0.get()) };
        let heap = state.heap.get_or_insert_with(|| {
            state.heap_top = HEAP_OFF + SIZE;
            let _ = unsafe {
                syscall::fmap(
                    !0,
                    &Map {
                        offset: 0,
                        size: SIZE,
                        address: HEAP_OFF,
                        flags: MapFlags::PROT_WRITE
                            | MapFlags::PROT_READ
                            | MapFlags::MAP_PRIVATE
                            | MapFlags::MAP_FIXED_NOREPLACE,
                    },
                )
            }
            .expect("failed to map initial heap");
            unsafe { linked_list_allocator::Heap::new(HEAP_OFF as *mut u8, SIZE) }
        });

        match heap.allocate_first_fit(layout) {
            Ok(p) => p.as_ptr(),
            Err(_) => {
                if layout.size() > HEAP_INCREASE_BY || layout.align() > 4096 {
                    return core::ptr::null_mut();
                }

                let _ = unsafe {
                    syscall::fmap(
                        !0,
                        &Map {
                            offset: 0,
                            size: HEAP_INCREASE_BY,
                            address: state.heap_top,
                            flags: MapFlags::PROT_WRITE
                                | MapFlags::PROT_READ
                                | MapFlags::MAP_PRIVATE
                                | MapFlags::MAP_FIXED_NOREPLACE,
                        },
                    )
                }
                .expect("failed to extend heap");
                unsafe { heap.extend(HEAP_INCREASE_BY) };
                state.heap_top += HEAP_INCREASE_BY;

                return unsafe { self.alloc(layout) };
            }
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        unsafe {
            (&mut *ALLOC_STATE.0.get())
                .heap
                .as_mut()
                .unwrap()
                .deallocate(core::ptr::NonNull::new(ptr).unwrap(), layout)
        }
    }
}

pub struct KernelSchemeMap(BTreeMap<GlobalSchemes, FdGuard>);
impl KernelSchemeMap {
    fn new(kernel_scheme_infos: &[KernelSchemeInfo]) -> Self {
        let mut map = BTreeMap::new();
        for info in kernel_scheme_infos {
            if let Some(scheme_id) = GlobalSchemes::try_from_raw(info.scheme_id) {
                map.insert(scheme_id, FdGuard::new(info.fd));
            }
        }
        Self(map)
    }
    fn get(&self, scheme: GlobalSchemes) -> Option<&FdGuard> {
        self.0.get(&scheme)
    }
}
