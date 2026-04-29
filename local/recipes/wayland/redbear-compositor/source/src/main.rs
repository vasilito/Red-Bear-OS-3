// Red Bear Wayland Compositor — bounded Wayland compositor proof scaffold.
// Replaces the KWin stub that previously created a placeholder socket.
//
// Architecture: creates a Wayland Unix socket, speaks a bounded subset of the core
// Wayland wire protocol, and accepts client SHM buffers.
//
// NOTE: This is a bounded proof scaffold, not a real compositor runtime proof.
// Known limitations: framebuffer compositing uses private heap memory (not real
// vesad), only a bounded subset of Wayland is implemented, and the compositor
// still paints directly into a simple backing buffer instead of doing real KMS
// scanout.
//
// Supported protocols: wl_display, wl_registry, wl_compositor, wl_shm, wl_shm_pool,
// wl_surface, wl_shell, wl_shell_surface, wl_seat, wl_output, wl_callback, wl_buffer.
//
// Wire format: [sender:u32] [msg_size:u16|opcode:u16] [args...]

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};

fn map_framebuffer(_phys: usize, size: usize) -> Vec<u8> {
    vec![0u8; size]
}

fn push_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(buf: &mut Vec<u8>, value: i32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn push_header(buf: &mut Vec<u8>, object_id: u32, opcode: u16, payload_len: usize) {
    push_u32(buf, object_id);
    let size = (8 + payload_len) as u32;
    push_u32(buf, (size << 16) | u32::from(opcode));
}

fn pad_to_4(buf: &mut Vec<u8>) {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn push_wayland_string(buf: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    push_u32(buf, (bytes.len() + 1) as u32);
    buf.extend_from_slice(bytes);
    buf.push(0);
    pad_to_4(buf);
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

fn recv_with_rights(
    stream: &mut UnixStream,
    data: &mut [u8],
) -> std::io::Result<(usize, VecDeque<RawFd>)> {
    let mut iov = libc::iovec {
        iov_base: data.as_mut_ptr().cast(),
        iov_len: data.len(),
    };
    let mut control = [0u8; 256];
    let mut header = libc::msghdr {
        msg_name: std::ptr::null_mut(),
        msg_namelen: 0,
        msg_iov: &mut iov,
        msg_iovlen: 1,
        msg_control: control.as_mut_ptr().cast(),
        msg_controllen: control.len(),
        msg_flags: 0,
    };

    let read = unsafe { libc::recvmsg(stream.as_raw_fd(), &mut header, 0) };
    if read < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut fds = VecDeque::new();
    let mut cmsg = unsafe { libc::CMSG_FIRSTHDR(&header) };
    while !cmsg.is_null() {
        let is_rights = unsafe {
            (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS
        };
        if is_rights {
            let data_len = unsafe { (*cmsg).cmsg_len as usize }
                .saturating_sub(mem::size_of::<libc::cmsghdr>());
            let fd_count = data_len / mem::size_of::<RawFd>();
            let data_ptr = unsafe { libc::CMSG_DATA(cmsg).cast::<RawFd>() };
            for index in 0..fd_count {
                fds.push_back(unsafe { *data_ptr.add(index) });
            }
        }
        cmsg = unsafe { libc::CMSG_NXTHDR(&header, cmsg) };
    }

    Ok((read as usize, fds))
}

const WL_DISPLAY_SYNC: u16 = 0;
const WL_DISPLAY_GET_REGISTRY: u16 = 1;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_DISPLAY_ERROR: u16 = 0;
const WL_DISPLAY_DELETE_ID: u16 = 2;

const WL_REGISTRY_BIND: u16 = 0;
const WL_REGISTRY_GLOBAL: u16 = 0;

const WL_COMPOSITOR_CREATE_SURFACE: u16 = 0;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_COMPOSITOR_CREATE_REGION: u16 = 1;

const WL_SHM_CREATE_POOL: u16 = 0;
const WL_SHM_FORMAT: u16 = 0;

const WL_SHM_POOL_CREATE_BUFFER: u16 = 0;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_SHM_POOL_RESIZE: u16 = 1;

const WL_BUFFER_RELEASE: u16 = 0;

const WL_SURFACE_ATTACH: u16 = 0;
const WL_SURFACE_DAMAGE: u16 = 1;
const WL_SURFACE_COMMIT: u16 = 5;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_SURFACE_ENTER: u16 = 0;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_SURFACE_LEAVE: u16 = 1;

const WL_SHELL_GET_SHELL_SURFACE: u16 = 0;

const WL_SHELL_SURFACE_PONG: u16 = 0;
const WL_SHELL_SURFACE_SET_TOPLEVEL: u16 = 2;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_SHELL_SURFACE_PING: u16 = 0;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_SHELL_SURFACE_CONFIGURE: u16 = 1;

const XDG_WM_BASE_DESTROY: u16 = 0;
const XDG_WM_BASE_GET_XDG_SURFACE: u16 = 2;
const XDG_WM_BASE_PONG: u16 = 3;

const XDG_SURFACE_DESTROY: u16 = 0;
const XDG_SURFACE_GET_TOPLEVEL: u16 = 1;
const XDG_SURFACE_ACK_CONFIGURE: u16 = 4;
const XDG_SURFACE_CONFIGURE: u16 = 0;

const XDG_TOPLEVEL_CONFIGURE: u16 = 0;

const WL_SEAT_GET_POINTER: u16 = 0;
const WL_SEAT_GET_KEYBOARD: u16 = 1;
const WL_SEAT_CAPABILITIES: u16 = 0;

// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_KEYBOARD_KEYMAP: u16 = 0;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_KEYBOARD_ENTER: u16 = 1;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_KEYBOARD_LEAVE: u16 = 2;
// Protocol constant: reserved for future implementation.
#[allow(dead_code)]
const WL_KEYBOARD_KEY: u16 = 3;

const WL_OUTPUT_GEOMETRY: u16 = 0;
const WL_OUTPUT_MODE: u16 = 1;

const WL_CALLBACK_DONE: u16 = 0;

const WL_SHM_FORMAT_XRGB8888: u32 = 1;
const WL_SHM_FORMAT_ARGB8888: u32 = 0;

const OBJECT_TYPE_WL_DISPLAY: u32 = 1;
const OBJECT_TYPE_WL_REGISTRY: u32 = 2;
const OBJECT_TYPE_WL_COMPOSITOR: u32 = 3;
const OBJECT_TYPE_WL_SHM: u32 = 4;
const OBJECT_TYPE_WL_SHELL: u32 = 5;
const OBJECT_TYPE_WL_SEAT: u32 = 6;
const OBJECT_TYPE_WL_OUTPUT: u32 = 7;
const OBJECT_TYPE_XDG_WM_BASE: u32 = 8;
const OBJECT_TYPE_WL_SURFACE: u32 = 9;
const OBJECT_TYPE_WL_BUFFER: u32 = 10;
const OBJECT_TYPE_WL_SHELL_SURFACE: u32 = 11;
const OBJECT_TYPE_XDG_SURFACE: u32 = 12;
const OBJECT_TYPE_XDG_TOPLEVEL: u32 = 13;
const OBJECT_TYPE_WL_SHM_POOL: u32 = 14;
const OBJECT_TYPE_WL_POINTER: u32 = 15;
const OBJECT_TYPE_WL_KEYBOARD: u32 = 16;

struct Global {
    name: u32,
    interface: String,
    version: u32,
}

struct ShmPool {
    file: std::fs::File,
    size: usize,
}

#[derive(Clone)]
struct Buffer {
    pool_id: u32,
    offset: u32,
    width: u32,
    height: u32,
    stride: u32,
    _format: u32,
}

#[derive(Clone)]
struct Surface {
    buffer: Option<Buffer>,
    committed_buffer_id: Option<u32>,
    x: u32,
    y: u32,
    _width: u32,
    _height: u32,
}

struct ClientState {
    objects: HashMap<u32, u32>,
    surfaces: HashMap<u32, Surface>,
    buffers: HashMap<u32, (u32, Buffer)>,
    shm_pools: HashMap<u32, ShmPool>,
    _next_id: u32,
}

pub struct Compositor {
    listener: UnixListener,
    next_id: AtomicU32,
    next_serial: AtomicU32,
    globals: Vec<Global>,
    fb_width: u32,
    fb_height: u32,
    fb_stride: u32,
    fb_data: Mutex<Vec<u8>>,
    clients: Mutex<HashMap<u32, ClientState>>,
}

impl Compositor {
    pub fn new(
        socket_path: &str,
        fb_phys: usize,
        fb_width: u32,
        fb_height: u32,
        fb_stride: u32,
    ) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path)?;

        let runtime_dir = std::path::Path::new(socket_path)
            .parent()
            .unwrap_or(std::path::Path::new("/tmp"));
        std::fs::write(
            runtime_dir.join("compositor.pid"),
            format!("{}\n", std::process::id()),
        )
        .ok();

        let fb_size = (fb_height as usize) * (fb_stride as usize);
        let fb_data = map_framebuffer(fb_phys, fb_size);

        let globals = vec![
            Global {
                name: 1,
                interface: "wl_compositor".into(),
                version: 4,
            },
            Global {
                name: 2,
                interface: "wl_shm".into(),
                version: 1,
            },
            Global {
                name: 3,
                interface: "wl_shell".into(),
                version: 1,
            },
            Global {
                name: 4,
                interface: "wl_seat".into(),
                version: 5,
            },
            Global {
                name: 5,
                interface: "wl_output".into(),
                version: 3,
            },
            Global {
                name: 6,
                interface: "xdg_wm_base".into(),
                version: 1,
            },
        ];

        Ok(Self {
            listener,
            next_id: AtomicU32::new(0x10000),
            next_serial: AtomicU32::new(1),
            globals,
            fb_width,
            fb_height,
            fb_stride,
            fb_data: Mutex::new(fb_data),
            clients: Mutex::new(HashMap::new()),
        })
    }

    fn alloc_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn next_serial(&self) -> u32 {
        self.next_serial.fetch_add(1, Ordering::Relaxed)
    }

    pub fn run(&mut self) -> std::io::Result<()> {
        eprintln!("redbear-compositor: listening on Wayland socket");
        let _ = std::fs::write(
            std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into())
                + "/compositor.status",
            "ready\n",
        );
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let client_id = self.alloc_id();
                    eprintln!("redbear-compositor: client {} connected", client_id);
                    self.clients.lock().unwrap().insert(
                        client_id,
                        ClientState {
                            objects: HashMap::new(),
                            surfaces: HashMap::new(),
                            buffers: HashMap::new(),
                            shm_pools: HashMap::new(),
                            _next_id: 1,
                        },
                    );
                    self.handle_client(client_id, stream);
                }
                Err(e) => eprintln!("redbear-compositor: accept error: {}", e),
            }
        }
        Ok(())
    }

    fn send_globals(&self, stream: &mut UnixStream, registry_id: u32) {
        // Advertise each global interface on the wl_registry object after get_registry.
        for global in &self.globals {
            let mut payload = Vec::new();
            push_u32(&mut payload, global.name);
            push_wayland_string(&mut payload, &global.interface);
            push_u32(&mut payload, global.version);

            let mut msg = Vec::with_capacity(8 + payload.len());
            push_header(&mut msg, registry_id, WL_REGISTRY_GLOBAL, payload.len());
            msg.extend_from_slice(&payload);
            let _ = stream.write_all(&msg);
        }
    }

    fn send_callback_done(&self, stream: &mut UnixStream, callback_id: u32, callback_data: u32) {
        let mut msg = Vec::with_capacity(12);
        push_header(&mut msg, callback_id, WL_CALLBACK_DONE, 4);
        push_u32(&mut msg, callback_data);
        let _ = stream.write_all(&msg);
    }

    fn handle_client(&self, client_id: u32, mut stream: UnixStream) {
        let mut buf = [0u8; 4096];
        loop {
            match recv_with_rights(&mut stream, &mut buf) {
                Ok((0, _)) => {
                    eprintln!("redbear-compositor: client {} disconnected", client_id);
                    self.clients.lock().unwrap().remove(&client_id);
                    break;
                }
                Ok((n, mut fds)) => {
                    if let Err(e) = self.dispatch(client_id, &buf[..n], &mut fds, &mut stream) {
                        eprintln!("redbear-compositor: dispatch error: {}", e);
                    }
                    while let Some(fd) = fds.pop_front() {
                        let _ = unsafe { libc::close(fd) };
                    }
                }
                Err(e) => {
                    eprintln!("redbear-compositor: read error: {}", e);
                    break;
                }
            }
        }
        self.clients.lock().unwrap().remove(&client_id);
    }

    fn dispatch(
        &self,
        client_id: u32,
        data: &[u8],
        fds: &mut VecDeque<RawFd>,
        stream: &mut UnixStream,
    ) -> Result<(), String> {
        let mut offset = 0;
        while offset + 8 <= data.len() {
            let object_id = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            // Wayland wire format: [object_id:u32][size:u16][opcode:u16]
            let size_opcode = u32::from_le_bytes([
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            let msg_size = ((size_opcode >> 16) & 0xFFFF) as usize;
            let opcode = (size_opcode & 0xFFFF) as u16;

            if msg_size < 8 || offset + msg_size > data.len() {
                return Err(format!(
                    "malformed message: object={} opcode={} size={}",
                    object_id, opcode, msg_size
                ));
            }

            let payload = &data[offset + 8..offset + msg_size];
            let object_type = if object_id == 1 {
                OBJECT_TYPE_WL_DISPLAY
            } else {
                self.clients
                    .lock()
                    .unwrap()
                    .get(&client_id)
                    .and_then(|client| client.objects.get(&object_id).copied())
                    .unwrap_or(0)
            };

            match object_type {
                OBJECT_TYPE_WL_DISPLAY => match opcode {
                    WL_DISPLAY_SYNC => {
                        let callback_id = if payload.len() >= 4 {
                            u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]])
                        } else {
                            self.alloc_id()
                        };
                        self.send_callback_done(stream, callback_id, 0);
                    }
                    WL_DISPLAY_DELETE_ID => {
                        if payload.len() >= 4 {
                            let obj_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.remove(&obj_id);
                                client.surfaces.remove(&obj_id);
                                client.buffers.remove(&obj_id);
                                client.shm_pools.remove(&obj_id);
                            }
                        }
                    }
                    WL_DISPLAY_GET_REGISTRY => {
                        if payload.len() >= 4 {
                            let registry_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let mut clients = self.clients.lock().unwrap();
                            let mut send_globals = false;
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(registry_id, OBJECT_TYPE_WL_REGISTRY);
                                send_globals = true;
                            }
                            drop(clients);
                            if send_globals {
                                self.send_globals(stream, registry_id);
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_REGISTRY => match opcode {
                    WL_REGISTRY_BIND => {
                        let mut cursor = 0;
                        let _name = read_u32(payload, &mut cursor)?;
                        let iface = read_wayland_string(payload, &mut cursor)?;
                        let _version = read_u32(payload, &mut cursor)?;
                        let new_id = read_u32(payload, &mut cursor)?;

                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            let type_id = match iface.as_str() {
                                "wl_compositor" => OBJECT_TYPE_WL_COMPOSITOR,
                                "wl_shm" => OBJECT_TYPE_WL_SHM,
                                "wl_shell" => OBJECT_TYPE_WL_SHELL,
                                "wl_seat" => OBJECT_TYPE_WL_SEAT,
                                "wl_output" => OBJECT_TYPE_WL_OUTPUT,
                                "xdg_wm_base" => OBJECT_TYPE_XDG_WM_BASE,
                                _ => 0,
                            };
                            client.objects.insert(new_id, type_id);
                            if iface == "wl_shm" {
                                self.send_shm_format(stream, new_id, WL_SHM_FORMAT_ARGB8888);
                                self.send_shm_format(stream, new_id, WL_SHM_FORMAT_XRGB8888);
                            }
                            if iface == "wl_output" {
                                self.send_output_info(stream, new_id);
                            }
                            if iface == "wl_seat" {
                                self.send_seat_capabilities(stream, new_id);
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_COMPOSITOR => match opcode {
                    WL_COMPOSITOR_CREATE_SURFACE => {
                        if payload.len() >= 4 {
                            let surface_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(surface_id, OBJECT_TYPE_WL_SURFACE);
                                client.surfaces.insert(
                                    surface_id,
                                    Surface {
                                        buffer: None,
                                        committed_buffer_id: None,
                                        x: 0,
                                        y: 0,
                                        _width: self.fb_width,
                                        _height: self.fb_height,
                                    },
                                );
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_SHM => match opcode {
                    WL_SHM_CREATE_POOL => {
                        if payload.len() >= 8 {
                            let pool_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let size = i32::from_le_bytes([
                                payload[4], payload[5], payload[6], payload[7],
                            ]);
                            let fd_val = fds.pop_front().ok_or_else(|| {
                                String::from("wl_shm.create_pool missing SCM_RIGHTS fd")
                            })?;
                            if size > 0 {
                                let file = unsafe { std::fs::File::from_raw_fd(fd_val) };
                                let mut clients = self.clients.lock().unwrap();
                                if let Some(client) = clients.get_mut(&client_id) {
                                    client.objects.insert(pool_id, OBJECT_TYPE_WL_SHM_POOL);
                                    client.shm_pools.insert(
                                        pool_id,
                                        ShmPool {
                                            file,
                                            size: size as usize,
                                        },
                                    );
                                }
                            } else {
                                let _ = unsafe { libc::close(fd_val) };
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_SHM_POOL => match opcode {
                    WL_SHM_POOL_CREATE_BUFFER => {
                        if payload.len() >= 20 {
                            let buffer_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let offset = u32::from_le_bytes([
                                payload[4], payload[5], payload[6], payload[7],
                            ]);
                            let width = u32::from_le_bytes([
                                payload[8],
                                payload[9],
                                payload[10],
                                payload[11],
                            ]);
                            let height = u32::from_le_bytes([
                                payload[12],
                                payload[13],
                                payload[14],
                                payload[15],
                            ]);
                            let stride = u32::from_le_bytes([
                                payload[16],
                                payload[17],
                                payload[18],
                                payload[19],
                            ]);
                            let format = if payload.len() >= 24 {
                                u32::from_le_bytes([
                                    payload[20],
                                    payload[21],
                                    payload[22],
                                    payload[23],
                                ])
                            } else {
                                WL_SHM_FORMAT_ARGB8888
                            };

                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(buffer_id, OBJECT_TYPE_WL_BUFFER);
                                client.buffers.insert(
                                    buffer_id,
                                    (
                                        object_id,
                                        Buffer {
                                            pool_id: object_id,
                                            offset,
                                            width,
                                            height,
                                            stride,
                                            _format: format,
                                        },
                                    ),
                                );
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_SURFACE => match opcode {
                    WL_SURFACE_ATTACH => {
                        if payload.len() >= 12 {
                            let buffer_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let _x = i32::from_le_bytes([
                                payload[4], payload[5], payload[6], payload[7],
                            ]);
                            let _y = i32::from_le_bytes([
                                payload[8],
                                payload[9],
                                payload[10],
                                payload[11],
                            ]);

                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                if let Some((pool_id, buffer)) =
                                    client.buffers.get(&buffer_id).cloned()
                                {
                                    if let Some(surface) = client.surfaces.get_mut(&object_id) {
                                        surface.buffer = Some(Buffer { pool_id, ..buffer });
                                    }
                                }
                            }
                        }
                    }
                    WL_SURFACE_COMMIT => {
                        let release_id = {
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                if let Some(surface) = client.surfaces.get_mut(&object_id) {
                                    let old_buffer = surface.committed_buffer_id.take();
                                    surface.committed_buffer_id =
                                        surface.buffer.as_ref().map(|b| {
                                            client
                                                .buffers
                                                .iter()
                                                .find(|(_, (_, buf))| {
                                                    buf.offset == b.offset && buf.width == b.width
                                                })
                                                .map(|(id, _)| *id)
                                                .unwrap_or(0)
                                        });
                                    let surface_snapshot = surface.clone();

                                    if let Some(ref buffer) = surface_snapshot.buffer {
                                        if let Some(pool) =
                                            client.shm_pools.get_mut(&buffer.pool_id)
                                        {
                                            self.composite_buffer(pool, buffer, &surface_snapshot);
                                        }
                                    }
                                    old_buffer
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        };

                        if let Some(buf_id) = release_id {
                            if buf_id != 0 {
                                self.send_buffer_release(stream, buf_id);
                            }
                        }
                    }
                    WL_SURFACE_DAMAGE => {
                        // No-op — we don't need damage tracking for a single-client greeter.
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_SHELL => match opcode {
                    WL_SHELL_GET_SHELL_SURFACE => {
                        if payload.len() >= 4 {
                            let new_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(new_id, OBJECT_TYPE_WL_SHELL_SURFACE);
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_SHELL_SURFACE => match opcode {
                    WL_SHELL_SURFACE_SET_TOPLEVEL | WL_SHELL_SURFACE_PONG => {
                        // No-op — we don't need window management for a single-client greeter.
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_SEAT => match opcode {
                    WL_SEAT_GET_POINTER | WL_SEAT_GET_KEYBOARD => {
                        if payload.len() >= 4 {
                            let new_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let object_type = match opcode {
                                WL_SEAT_GET_POINTER => OBJECT_TYPE_WL_POINTER,
                                WL_SEAT_GET_KEYBOARD => OBJECT_TYPE_WL_KEYBOARD,
                                _ => unreachable!(),
                            };
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(new_id, object_type);
                            }
                        }
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_XDG_WM_BASE => match opcode {
                    XDG_WM_BASE_GET_XDG_SURFACE => {
                        if payload.len() >= 4 {
                            let new_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(new_id, OBJECT_TYPE_XDG_SURFACE);
                            }
                        }
                    }
                    XDG_WM_BASE_DESTROY | XDG_WM_BASE_PONG => {
                        // No-op — the greeter keeps the shell global alive for the client lifetime.
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_XDG_SURFACE => match opcode {
                    XDG_SURFACE_DESTROY => {
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.remove(&object_id);
                        }
                    }
                    XDG_SURFACE_GET_TOPLEVEL => {
                        if payload.len() >= 4 {
                            let toplevel_id = u32::from_le_bytes([
                                payload[0], payload[1], payload[2], payload[3],
                            ]);
                            let mut clients = self.clients.lock().unwrap();
                            if let Some(client) = clients.get_mut(&client_id) {
                                client.objects.insert(toplevel_id, OBJECT_TYPE_XDG_TOPLEVEL);
                            }
                            drop(clients);
                            let serial = self.next_serial();
                            self.send_xdg_toplevel_configure(stream, toplevel_id);
                            self.send_xdg_surface_configure(stream, object_id, serial);
                        }
                    }
                    XDG_SURFACE_ACK_CONFIGURE => {
                        // Client acknowledged — ready for first commit.
                    }
                    _ => {
                        eprintln!(
                            "redbear-compositor: unhandled opcode {} on object {}",
                            opcode, object_id
                        );
                    }
                },
                OBJECT_TYPE_WL_OUTPUT
                | OBJECT_TYPE_WL_BUFFER
                | OBJECT_TYPE_XDG_TOPLEVEL
                | OBJECT_TYPE_WL_POINTER
                | OBJECT_TYPE_WL_KEYBOARD => {
                    eprintln!(
                        "redbear-compositor: unhandled opcode {} on object {}",
                        opcode, object_id
                    );
                }
                _ => {
                    eprintln!(
                        "redbear-compositor: unhandled object {} opcode {}",
                        object_id, opcode
                    );
                }
            }

            offset += msg_size;
        }
        Ok(())
    }

    fn composite_buffer(&self, pool: &mut ShmPool, buffer: &Buffer, surface: &Surface) {
        let mut fb = self.fb_data.lock().unwrap();
        let fb_stride = self.fb_stride as usize;
        let byte_count = buffer.height as usize * buffer.stride as usize;

        if buffer.offset as usize + byte_count > pool.size {
            return;
        }

        let mut src = vec![0u8; byte_count];
        if pool
            .file
            .seek(SeekFrom::Start(buffer.offset as u64))
            .is_err()
            || pool.file.read_exact(&mut src).is_err()
        {
            return;
        }
        let dst_x = surface.x as usize;
        let dst_y = surface.y as usize;

        for row in 0..buffer.height as usize {
            let src_row = row * buffer.stride as usize;
            let dst_row = (dst_y + row) * fb_stride + dst_x * 4;

            if dst_row + buffer.width as usize * 4 <= fb.len()
                && src_row + buffer.width as usize * 4 <= src.len()
            {
                for col in 0..buffer.width as usize {
                    let src_offset = src_row + col * 4;
                    let dst_offset = dst_row + col * 4;
                    if dst_offset + 4 <= fb.len() && src_offset + 4 <= src.len() {
                        fb[dst_offset] = src[src_offset + 2];
                        fb[dst_offset + 1] = src[src_offset + 1];
                        fb[dst_offset + 2] = src[src_offset];
                        fb[dst_offset + 3] = 0xFF;
                    }
                }
            }
        }
    }

    fn send_buffer_release(&self, stream: &mut UnixStream, buffer_id: u32) {
        let mut msg = Vec::with_capacity(8);
        push_header(&mut msg, buffer_id, WL_BUFFER_RELEASE, 0);
        let _ = stream.write_all(&msg);
    }

    fn send_xdg_surface_configure(&self, stream: &mut UnixStream, surface_id: u32, serial: u32) {
        let mut msg = Vec::with_capacity(12);
        push_header(&mut msg, surface_id, XDG_SURFACE_CONFIGURE, 4);
        push_u32(&mut msg, serial);
        let _ = stream.write_all(&msg);
    }

    fn send_xdg_toplevel_configure(&self, stream: &mut UnixStream, toplevel_id: u32) {
        let fb_w = self.fb_width as i32;
        let fb_h = self.fb_height as i32;
        let mut msg = Vec::with_capacity(20);
        push_header(&mut msg, toplevel_id, XDG_TOPLEVEL_CONFIGURE, 12);
        push_i32(&mut msg, fb_w);
        push_i32(&mut msg, fb_h);
        push_u32(&mut msg, 0);
        let _ = stream.write_all(&msg);
    }

    fn send_shm_format(&self, stream: &mut UnixStream, shm_id: u32, format: u32) {
        let mut msg = Vec::with_capacity(12);
        push_header(&mut msg, shm_id, WL_SHM_FORMAT, 4);
        push_u32(&mut msg, format);
        let _ = stream.write_all(&msg);
    }

    fn send_output_info(&self, stream: &mut UnixStream, output_id: u32) {
        // wl_output.geometry
        {
            let mut payload = Vec::new();
            push_i32(&mut payload, 0);
            push_i32(&mut payload, 0);
            push_i32(&mut payload, 0);
            push_i32(&mut payload, 0);
            push_i32(&mut payload, 0);
            push_wayland_string(&mut payload, "vesa");
            push_wayland_string(&mut payload, "fb0");
            push_i32(&mut payload, 0);

            let mut msg = Vec::with_capacity(8 + payload.len());
            push_header(&mut msg, output_id, WL_OUTPUT_GEOMETRY, payload.len());
            msg.extend_from_slice(&payload);
            let _ = stream.write_all(&msg);
        }
        // wl_output.mode
        {
            let mut msg = Vec::with_capacity(24);
            push_header(&mut msg, output_id, WL_OUTPUT_MODE, 16);
            push_u32(&mut msg, 0x2);
            push_i32(&mut msg, self.fb_width as i32);
            push_i32(&mut msg, self.fb_height as i32);
            push_i32(&mut msg, 60);
            let _ = stream.write_all(&msg);
        }
    }

    fn send_seat_capabilities(&self, stream: &mut UnixStream, seat_id: u32) {
        let mut msg = Vec::with_capacity(12);
        push_header(&mut msg, seat_id, WL_SEAT_CAPABILITIES, 4);
        push_u32(&mut msg, 0x3);
        let _ = stream.write_all(&msg);
    }
}

fn main() {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp/run/redbear-greeter".into());
    let socket_path = format!("{}/{}", runtime_dir, wayland_display);

    // Read framebuffer parameters from environment (set by bootloader → vesad)
    let fb_width: u32 = std::env::var("FRAMEBUFFER_WIDTH")
        .unwrap_or_else(|_| "1280".into())
        .parse()
        .unwrap_or(1280);
    let fb_height: u32 = std::env::var("FRAMEBUFFER_HEIGHT")
        .unwrap_or_else(|_| "720".into())
        .parse()
        .unwrap_or(720);
    let fb_stride: u32 = std::env::var("FRAMEBUFFER_STRIDE")
        .unwrap_or_else(|_| (fb_width * 4).to_string())
        .parse()
        .unwrap_or(fb_width * 4);
    let fb_phys_str = std::env::var("FRAMEBUFFER_ADDR").unwrap_or_else(|_| "0x80000000".into());
    let fb_phys =
        usize::from_str_radix(fb_phys_str.trim_start_matches("0x"), 16).unwrap_or(0x80000000);

    eprintln!(
        "redbear-compositor: fb {}x{} stride {} phys 0x{:X}",
        fb_width, fb_height, fb_stride, fb_phys
    );

    let socket_path_clone = socket_path.clone();
    match Compositor::new(&socket_path, fb_phys, fb_width, fb_height, fb_stride) {
        Ok(mut compositor) => {
            if let Err(e) = compositor.run() {
                eprintln!("redbear-compositor: {}", e);
            }
        }
        Err(e) => {
            eprintln!("redbear-compositor: failed to start: {}", e);
        }
    }

    let _ = std::fs::remove_file(&socket_path_clone);
    let _ = std::fs::remove_file(
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()) + "/compositor.status",
    );
}
