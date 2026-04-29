// Red Bear Compositor Runtime Check — verifies the compositor and greeter surface are healthy.
// Usage: redbear-compositor-check [--verbose]

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::mem;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

const WL_DISPLAY_SYNC: u16 = 0;
const WL_DISPLAY_GET_REGISTRY: u16 = 1;
const WL_REGISTRY_BIND: u16 = 0;
const WL_REGISTRY_GLOBAL: u16 = 0;
const WL_COMPOSITOR_CREATE_SURFACE: u16 = 0;
const WL_SHM_CREATE_POOL: u16 = 0;
const WL_SHM_FORMAT: u16 = 0;
const WL_SHM_POOL_CREATE_BUFFER: u16 = 0;
const WL_SURFACE_ATTACH: u16 = 0;
const WL_SURFACE_COMMIT: u16 = 5;
const WL_CALLBACK_DONE: u16 = 0;
const XDG_WM_BASE_GET_XDG_SURFACE: u16 = 2;
const XDG_SURFACE_GET_TOPLEVEL: u16 = 1;
const XDG_SURFACE_ACK_CONFIGURE: u16 = 4;
const XDG_SURFACE_CONFIGURE: u16 = 0;
const XDG_TOPLEVEL_CONFIGURE: u16 = 0;
const WL_SHM_FORMAT_XRGB8888: u32 = 1;

fn push_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(buf: &mut Vec<u8>, value: i32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_wayland_string(buf: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    push_u32(buf, (bytes.len() + 1) as u32);
    buf.extend_from_slice(bytes);
    buf.push(0);
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn read_u32(data: &[u8], cursor: &mut usize) -> Result<u32, String> {
    if *cursor + 4 > data.len() {
        return Err(String::from("unexpected end of message while reading u32"));
    }

    let value = u32::from_le_bytes([
        data[*cursor],
        data[*cursor + 1],
        data[*cursor + 2],
        data[*cursor + 3],
    ]);
    *cursor += 4;
    Ok(value)
}

fn read_wayland_string(data: &[u8], cursor: &mut usize) -> Result<String, String> {
    let length = read_u32(data, cursor)? as usize;
    if length == 0 {
        return Ok(String::new());
    }
    if *cursor + length > data.len() {
        return Err(String::from(
            "unexpected end of message while reading string",
        ));
    }

    let bytes = &data[*cursor..*cursor + length];
    let string_len = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    *cursor += length;
    while *cursor % 4 != 0 {
        *cursor += 1;
    }

    std::str::from_utf8(&bytes[..string_len])
        .map(str::to_owned)
        .map_err(|err| format!("invalid UTF-8 in Wayland string: {err}"))
}

struct WaylandProbe {
    stream: UnixStream,
    next_id: u32,
}

impl WaylandProbe {
    fn connect(socket_path: &str) -> Result<Self, String> {
        if !std::path::Path::new(socket_path).exists() {
            return Err(format!("Wayland socket {} does not exist", socket_path));
        }

        let stream = UnixStream::connect(socket_path)
            .map_err(|e| format!("failed to connect to {}: {}", socket_path, e))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| format!("failed to set timeout: {}", e))?;

        Ok(Self { stream, next_id: 2 })
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_message(&mut self, object_id: u32, opcode: u16, payload: &[u8]) -> Result<(), String> {
        let size = 8 + payload.len();
        let mut msg = Vec::with_capacity(size);
        push_u32(&mut msg, object_id);
        push_u32(&mut msg, ((size as u32) << 16) | u32::from(opcode));
        msg.extend_from_slice(payload);
        self.stream
            .write_all(&msg)
            .map_err(|e| format!("write failed: {}", e))
    }

    fn send_message_with_fds(
        &mut self,
        object_id: u32,
        opcode: u16,
        payload: &[u8],
        fds: &[i32],
    ) -> Result<(), String> {
        if fds.is_empty() {
            return self.send_message(object_id, opcode, payload);
        }

        let size = 8 + payload.len();
        let mut msg = Vec::with_capacity(size);
        push_u32(&mut msg, object_id);
        push_u32(&mut msg, ((size as u32) << 16) | u32::from(opcode));
        msg.extend_from_slice(payload);

        let mut iov = libc::iovec {
            iov_base: msg.as_mut_ptr().cast(),
            iov_len: msg.len(),
        };
        let control_len =
            unsafe { libc::CMSG_SPACE((fds.len() * mem::size_of::<i32>()) as u32) as usize };
        let mut control = vec![0u8; control_len];
        let header = libc::msghdr {
            msg_name: std::ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: &mut iov,
            msg_iovlen: 1,
            msg_control: control.as_mut_ptr().cast(),
            msg_controllen: control.len(),
            msg_flags: 0,
        };

        unsafe {
            let cmsg = libc::CMSG_FIRSTHDR(&header);
            if cmsg.is_null() {
                return Err(String::from("failed to allocate SCM_RIGHTS header"));
            }
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN((fds.len() * mem::size_of::<i32>()) as u32) as _;
            std::ptr::copy_nonoverlapping(
                fds.as_ptr().cast::<u8>(),
                libc::CMSG_DATA(cmsg).cast::<u8>(),
                fds.len() * mem::size_of::<i32>(),
            );
        }

        let written = unsafe { libc::sendmsg(self.stream.as_raw_fd(), &header, 0) };
        if written < 0 {
            return Err(format!(
                "sendmsg failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        if written as usize != msg.len() {
            return Err(format!(
                "short sendmsg write: expected {}, got {}",
                msg.len(),
                written
            ));
        }

        Ok(())
    }

    fn read_message(&mut self) -> Result<(u32, u16, Vec<u8>), String> {
        let mut header = [0u8; 8];
        self.stream
            .read_exact(&mut header)
            .map_err(|e| format!("read failed: {}", e))?;
        let object_id = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let size_opcode = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let size = ((size_opcode >> 16) & 0xFFFF) as usize;
        let opcode = (size_opcode & 0xFFFF) as u16;
        let mut payload = vec![0u8; size.saturating_sub(8)];
        if size > 8 {
            self.stream
                .read_exact(&mut payload)
                .map_err(|e| format!("read payload failed: {}", e))?;
        }
        Ok((object_id, opcode, payload))
    }

    fn sync(&mut self) -> Result<u32, String> {
        let callback_id = self.alloc_id();
        self.send_message(1, WL_DISPLAY_SYNC, &callback_id.to_le_bytes())?;
        Ok(callback_id)
    }

    fn get_registry(&mut self) -> Result<u32, String> {
        let registry_id = self.alloc_id();
        self.send_message(1, WL_DISPLAY_GET_REGISTRY, &registry_id.to_le_bytes())?;
        Ok(registry_id)
    }

    fn bind(
        &mut self,
        registry_id: u32,
        name: u32,
        interface: &str,
        version: u32,
    ) -> Result<u32, String> {
        let new_id = self.alloc_id();
        let mut payload = Vec::new();
        push_u32(&mut payload, name);
        push_wayland_string(&mut payload, interface);
        push_u32(&mut payload, version);
        push_u32(&mut payload, new_id);
        self.send_message(registry_id, WL_REGISTRY_BIND, &payload)?;
        Ok(new_id)
    }
}

fn collect_globals(probe: &mut WaylandProbe) -> Result<HashMap<String, u32>, String> {
    let registry_id = probe.get_registry()?;
    let mut globals = HashMap::new();

    for _ in 0..6 {
        let (object_id, opcode, payload) = probe.read_message()?;
        if object_id != registry_id || opcode != WL_REGISTRY_GLOBAL {
            return Err(format!(
                "unexpected registry event: object={} opcode={}",
                object_id, opcode
            ));
        }

        let mut cursor = 0;
        let name = read_u32(&payload, &mut cursor)?;
        let interface = read_wayland_string(&payload, &mut cursor)?;
        let _version = read_u32(&payload, &mut cursor)?;
        globals.insert(interface, name);
    }

    Ok(globals)
}

fn expect_shm_formats(probe: &mut WaylandProbe, shm_id: u32) -> Result<(), String> {
    let mut formats = Vec::new();

    for _ in 0..2 {
        let (object_id, opcode, payload) = probe.read_message()?;
        if object_id != shm_id || opcode != WL_SHM_FORMAT || payload.len() != 4 {
            return Err(format!(
                "unexpected wl_shm event: object={} opcode={} payload_len={}",
                object_id,
                opcode,
                payload.len()
            ));
        }
        formats.push(u32::from_le_bytes([
            payload[0], payload[1], payload[2], payload[3],
        ]));
    }

    if !formats.contains(&0) || !formats.contains(&1) {
        return Err(format!("wl_shm.format list incomplete: {:?}", formats));
    }

    Ok(())
}

fn expect_xdg_configure(
    probe: &mut WaylandProbe,
    toplevel_id: u32,
    xdg_surface_id: u32,
) -> Result<u32, String> {
    let (object_id, opcode, payload) = probe.read_message()?;
    if object_id != toplevel_id || opcode != XDG_TOPLEVEL_CONFIGURE {
        return Err(format!(
            "unexpected xdg_toplevel event: object={} opcode={}",
            object_id, opcode
        ));
    }
    if payload.len() < 12 {
        return Err(format!(
            "short xdg_toplevel.configure payload: {} bytes",
            payload.len()
        ));
    }

    let states_len =
        u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]) as usize;
    if payload.len() != 12 + states_len {
        return Err(format!(
            "invalid xdg_toplevel.configure payload length: {} (states_len={})",
            payload.len(),
            states_len
        ));
    }
    if states_len % 4 != 0 {
        return Err(format!(
            "invalid xdg_toplevel.configure states array length: {}",
            states_len
        ));
    }

    let (object_id, opcode, payload) = probe.read_message()?;
    if object_id != xdg_surface_id || opcode != XDG_SURFACE_CONFIGURE || payload.len() != 4 {
        return Err(format!(
            "unexpected xdg_surface event: object={} opcode={} payload_len={}",
            object_id,
            opcode,
            payload.len()
        ));
    }

    Ok(u32::from_le_bytes([
        payload[0], payload[1], payload[2], payload[3],
    ]))
}

fn exercise_shm_pool(probe: &mut WaylandProbe, shm_id: u32, surface_id: u32) -> Result<(), String> {
    let temp_path = std::env::temp_dir().join(format!(
        "redbear-compositor-check-{}-{}.shm",
        std::process::id(),
        surface_id
    ));
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(&temp_path)
        .map_err(|err| format!("failed to create temp SHM file: {err}"))?;
    file.set_len(16)
        .map_err(|err| format!("failed to size temp SHM file: {err}"))?;

    let mut file = file;
    file.write_all(&[
        0x40, 0x40, 0xFF, 0xFF, 0x40, 0x40, 0xFF, 0xFF, 0x40, 0x40, 0xFF, 0xFF, 0x40, 0x40, 0xFF,
        0xFF,
    ])
    .map_err(|err| format!("failed to seed temp SHM file: {err}"))?;

    let pool_id = probe.alloc_id();
    let mut payload = Vec::new();
    push_u32(&mut payload, pool_id);
    push_i32(&mut payload, 16);
    probe.send_message_with_fds(shm_id, WL_SHM_CREATE_POOL, &payload, &[file.as_raw_fd()])?;

    let buffer_id = probe.alloc_id();
    let mut payload = Vec::new();
    push_u32(&mut payload, buffer_id);
    push_u32(&mut payload, 0);
    push_i32(&mut payload, 2);
    push_i32(&mut payload, 2);
    push_i32(&mut payload, 8);
    push_u32(&mut payload, WL_SHM_FORMAT_XRGB8888);
    probe.send_message(pool_id, WL_SHM_POOL_CREATE_BUFFER, &payload)?;

    let mut payload = Vec::new();
    push_u32(&mut payload, buffer_id);
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 0);
    probe.send_message(surface_id, WL_SURFACE_ATTACH, &payload)?;
    probe.send_message(surface_id, WL_SURFACE_COMMIT, &[])?;

    let callback_id = probe.sync()?;
    let (object_id, opcode, payload) = probe.read_message()?;
    let _ = std::fs::remove_file(&temp_path);

    if object_id != callback_id || opcode != WL_CALLBACK_DONE || payload.len() != 4 {
        return Err(format!(
            "unexpected callback response after SHM commit: object={} opcode={} payload_len={}",
            object_id,
            opcode,
            payload.len()
        ));
    }

    Ok(())
}

fn check_wayland_socket() -> Result<(), String> {
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp/run/redbear-greeter".into());
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let socket_path = format!("{}/{}", runtime_dir, display);

    let mut probe = WaylandProbe::connect(&socket_path)?;
    let globals = collect_globals(&mut probe)?;

    let registry_id = 2;
    let compositor_name = *globals
        .get("wl_compositor")
        .ok_or_else(|| String::from("wl_compositor global missing"))?;
    let shm_name = *globals
        .get("wl_shm")
        .ok_or_else(|| String::from("wl_shm global missing"))?;
    let xdg_name = *globals
        .get("xdg_wm_base")
        .ok_or_else(|| String::from("xdg_wm_base global missing"))?;

    let compositor_id = probe.bind(registry_id, compositor_name, "wl_compositor", 4)?;
    let shm_id = probe.bind(registry_id, shm_name, "wl_shm", 1)?;
    let xdg_wm_base_id = probe.bind(registry_id, xdg_name, "xdg_wm_base", 1)?;

    expect_shm_formats(&mut probe, shm_id)?;

    let surface_id = probe.alloc_id();
    probe.send_message(
        compositor_id,
        WL_COMPOSITOR_CREATE_SURFACE,
        &surface_id.to_le_bytes(),
    )?;

    let xdg_surface_id = probe.alloc_id();
    let mut payload = Vec::new();
    push_u32(&mut payload, xdg_surface_id);
    push_u32(&mut payload, surface_id);
    probe.send_message(xdg_wm_base_id, XDG_WM_BASE_GET_XDG_SURFACE, &payload)?;

    let toplevel_id = probe.alloc_id();
    probe.send_message(
        xdg_surface_id,
        XDG_SURFACE_GET_TOPLEVEL,
        &toplevel_id.to_le_bytes(),
    )?;
    let serial = expect_xdg_configure(&mut probe, toplevel_id, xdg_surface_id)?;
    probe.send_message(
        xdg_surface_id,
        XDG_SURFACE_ACK_CONFIGURE,
        &serial.to_le_bytes(),
    )?;

    exercise_shm_pool(&mut probe, shm_id, surface_id)
}

fn check_binaries() -> Result<(), Vec<String>> {
    let mut missing = Vec::new();
    for bin in &[
        "/usr/bin/redbear-compositor",
        "/usr/bin/redbear-greeterd",
        "/usr/bin/redbear-greeter-ui",
        "/usr/bin/redbear-authd",
        "/usr/bin/kwin_wayland_wrapper",
    ] {
        if !std::path::Path::new(bin).exists() {
            missing.push(bin.to_string());
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn check_framebuffer() -> Result<(), String> {
    let width = std::env::var("FRAMEBUFFER_WIDTH").unwrap_or_default();
    let height = std::env::var("FRAMEBUFFER_HEIGHT").unwrap_or_default();
    let addr = std::env::var("FRAMEBUFFER_ADDR").unwrap_or_default();

    if width.is_empty() || height.is_empty() || addr.is_empty() {
        return Err(
            "FRAMEBUFFER_* environment not set — bootloader didn't provide framebuffer".into(),
        );
    }

    let w: u32 = width
        .parse()
        .map_err(|_| format!("invalid FRAMEBUFFER_WIDTH: {}", width))?;
    let h: u32 = height
        .parse()
        .map_err(|_| format!("invalid FRAMEBUFFER_HEIGHT: {}", height))?;

    if w == 0 || h == 0 {
        return Err("framebuffer dimensions are zero".into());
    }

    Ok(())
}

fn check_services() -> Result<(), Vec<String>> {
    let mut issues = Vec::new();
    let checks = [
        ("/run/seatd.sock", "seatd socket"),
        ("/run/redbear-authd.sock", "authd socket"),
        ("/run/dbus/system_bus_socket", "D-Bus system bus"),
        ("/scheme/drm/card0", "DRM device"),
    ];
    for (path, name) in checks {
        if !std::path::Path::new(path).exists() {
            issues.push(format!("{} not found at {}", name, path));
        }
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn main() {
    let verbose = std::env::args().any(|a| a == "--verbose");
    let mut exit = 0i32;

    macro_rules! check {
        ($label:expr, $check:expr) => {
            match $check {
                Ok(()) => {
                    if verbose {
                        println!("  PASS {}", $label);
                    }
                }
                Err(e) => {
                    eprintln!("  FAIL {}: {}", $label, e);
                    exit = 1;
                }
            }
        };
        ($label:expr, $check:expr, vec) => {
            match $check {
                Ok(()) => {
                    if verbose {
                        println!("  PASS {}", $label);
                    }
                }
                Err(errs) => {
                    for e in errs {
                        eprintln!("  FAIL {}: {}", $label, e);
                    }
                    exit = 1;
                }
            }
        };
    }

    println!("redbear-compositor-check: verifying compositor surface");

    if verbose {
        println!("  Checking binaries...");
    }
    check!("greeter binaries present", check_binaries(), vec);

    if verbose {
        println!("  Checking framebuffer...");
    }
    check!("framebuffer environment", check_framebuffer());

    if verbose {
        println!("  Checking services...");
    }
    check!("runtime services", check_services(), vec);

    if verbose {
        println!("  Checking Wayland protocol features...");
    }
    check!("Wayland protocol surface", check_wayland_socket());

    if exit == 0 {
        println!("redbear-compositor-check: all checks passed");
    } else {
        eprintln!(
            "redbear-compositor-check: {} check(s) failed",
            if exit == 1 { "1" } else { "some" }
        );
        std::process::exit(1);
    }
}
