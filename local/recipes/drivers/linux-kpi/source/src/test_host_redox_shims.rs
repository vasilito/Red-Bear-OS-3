#[cfg(all(test, not(target_os = "redox")))]
#[no_mangle]
pub extern "C" fn redox_open_v1(
    _path_base: *const u8,
    _path_len: usize,
    _flags: u32,
    _mode: u16,
) -> usize {
    usize::MAX
}

#[cfg(all(test, not(target_os = "redox")))]
#[no_mangle]
pub extern "C" fn redox_close_v1(_fd: usize) -> usize {
    0
}

#[cfg(all(test, not(target_os = "redox")))]
#[no_mangle]
pub extern "C" fn redox_sys_call_v0(
    _fd: usize,
    _payload: *mut u8,
    _payload_len: usize,
    _flags: usize,
    _metadata: *const u64,
    _metadata_len: usize,
) -> usize {
    usize::MAX
}
