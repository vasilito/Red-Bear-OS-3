use core::mem;
use syscall::{data::Map, flag::MapFlags, number::SYS_FMAP};

const STACK_SIZE: usize = 64 * 1024; // 64 KiB
pub const USERMODE_END: usize = 1 << 38; // Assuming Sv39
pub const STACK_START: usize = USERMODE_END - syscall::KERNEL_METADATA_SIZE - STACK_SIZE;

static MAP: Map = Map {
    offset: 0,
    size: STACK_SIZE,
    flags: MapFlags::PROT_READ
        .union(MapFlags::PROT_WRITE)
        .union(MapFlags::MAP_PRIVATE)
        .union(MapFlags::MAP_FIXED_NOREPLACE),
    address: STACK_START, // highest possible user address
};

core::arch::global_asm!(
    "
    .globl _start
_start:
    # Setup a stack.
    li a7, {number}
    li a0, {fd}
    la a1, {map} # pointer to Map struct
    li a2, {map_size} # size of Map struct
    ecall

    # Test for success (nonzero value).
    bne a0, x0, 2f
    # (failure)
    unimp
2:
    li sp, {stack_size}
    add sp, sp, a0
    mv fp, x0

    jal start
    # `start` must never return.
    unimp
    ",
    fd = const usize::MAX, // dummy fd indicates anonymous map
    map = sym MAP,
    map_size = const mem::size_of::<Map>(),
    number = const SYS_FMAP,
    stack_size = const STACK_SIZE,
);
