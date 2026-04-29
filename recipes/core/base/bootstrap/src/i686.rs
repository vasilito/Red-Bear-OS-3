use core::mem;
use syscall::{data::Map, flag::MapFlags, number::SYS_FMAP};

const STACK_SIZE: usize = 64 * 1024; // 64 KiB
pub const USERMODE_END: usize = 0x8000_0000;
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
    mov eax, {number}
    mov ebx, {fd}
    mov ecx, offset {map} # pointer to Map struct
    mov edx, {map_size} # size of Map struct
    int 0x80

    # Test for success (nonzero value).
    cmp eax, 0
    jg 1f
    # (failure)
    ud2
    1:
    # Subtract 16 since all instructions seem to hate non-canonical ESP values :)
    lea esp, [eax+{stack_size}-16]
    mov ebp, esp

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
