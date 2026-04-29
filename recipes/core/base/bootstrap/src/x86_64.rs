use core::mem;
use syscall::{data::Map, flag::MapFlags, number::SYS_FMAP};

const STACK_SIZE: usize = 64 * 1024; // 64 KiB
pub const USERMODE_END: usize = 0x0000_8000_0000_0000;
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
    mov rax, {number}
    mov rdi, {fd}
    mov rsi, offset {map} # pointer to Map struct
    mov rdx, {map_size} # size of Map struct
    syscall

    # Test for success (nonzero value).
    cmp rax, 0
    jg 1f
    # (failure)
    ud2
    1:
    # Subtract 16 since all instructions seem to hate non-canonical RSP values :)
    lea rsp, [rax+{stack_size}-16]
    mov rbp, rsp

    # Stack has the same alignment as `size`.
    call start
    # `start` must never return.
    ud2
    ",
    fd = const usize::MAX, // dummy fd indicates anonymous map
    map = sym MAP,
    map_size = const mem::size_of::<Map>(),
    number = const SYS_FMAP,
    stack_size = const STACK_SIZE,
);
