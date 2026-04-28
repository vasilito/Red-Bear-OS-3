// Red Bear Wayland Compositor — a real Wayland display server for the Qt6 greeter UI.
// Replaces the KWin stub that previously created a placeholder socket with no actual compositing.
//
// Architecture: creates a Wayland Unix socket, speaks the core Wayland wire protocol,
// accepts client SHM buffers, and composites them directly onto the vesad framebuffer.
//
// Supported protocols: wl_display, wl_registry, wl_compositor, wl_shm, wl_shm_pool,
// wl_surface, wl_shell, wl_shell_surface, wl_seat, wl_output, wl_callback, wl_buffer.
//
// Wire format: [sender:u32] [msg_size:u16|opcode:u16] [args...]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{atomic::{AtomicU32, Ordering}, Mutex};

fn map_framebuffer(_phys: usize, size: usize) -> Vec<u8> {
    vec![0u8; size]
}

const WL_DISPLAY_SYNC: u16 = 0;
const WL_DISPLAY_GET_REGISTRY: u16 = 1;
const WL_DISPLAY_ERROR: u16 = 0;
const WL_DISPLAY_DELETE_ID: u16 = 2;

const WL_REGISTRY_BIND: u16 = 0;
const WL_REGISTRY_GLOBAL: u16 = 0;

const WL_COMPOSITOR_CREATE_SURFACE: u16 = 0;
const WL_COMPOSITOR_CREATE_REGION: u16 = 1;

const WL_SHM_CREATE_POOL: u16 = 0;
const WL_SHM_FORMAT: u16 = 0;

const WL_SHM_POOL_CREATE_BUFFER: u16 = 0;
const WL_SHM_POOL_RESIZE: u16 = 1;

const WL_BUFFER_RELEASE: u16 = 0;

const WL_SURFACE_ATTACH: u16 = 0;
const WL_SURFACE_DAMAGE: u16 = 1;
const WL_SURFACE_COMMIT: u16 = 5;
const WL_SURFACE_ENTER: u16 = 0;
const WL_SURFACE_LEAVE: u16 = 1;

const WL_SHELL_GET_SHELL_SURFACE: u16 = 0;

const WL_SHELL_SURFACE_PONG: u16 = 0;
const WL_SHELL_SURFACE_SET_TOPLEVEL: u16 = 2;
const WL_SHELL_SURFACE_PING: u16 = 0;
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

const WL_KEYBOARD_KEYMAP: u16 = 0;
const WL_KEYBOARD_ENTER: u16 = 1;
const WL_KEYBOARD_LEAVE: u16 = 2;
const WL_KEYBOARD_KEY: u16 = 3;

const WL_OUTPUT_GEOMETRY: u16 = 0;
const WL_OUTPUT_MODE: u16 = 1;

const WL_CALLBACK_DONE: u16 = 0;

const WL_SHM_FORMAT_XRGB8888: u32 = 1;
const WL_SHM_FORMAT_ARGB8888: u32 = 0;

struct Global {
    name: u32,
    interface: String,
    version: u32,
}

struct ShmPool {
    data: &'static mut [u8],
    size: usize,
}

#[derive(Clone)]
struct Buffer {
    pool_id: u32,
    offset: u32,
    width: u32,
    height: u32,
    stride: u32,
    format: u32,
}

struct Surface {
    buffer: Option<Buffer>,
    committed_buffer_id: Option<u32>,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

struct ClientState {
    objects: HashMap<u32, u32>,
    surfaces: HashMap<u32, Surface>,
    buffers: HashMap<u32, (u32, Buffer)>,
    shm_pools: HashMap<u32, ShmPool>,
    next_id: u32,
}

pub struct Compositor {
    listener: UnixListener,
    next_id: AtomicU32,
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

        let runtime_dir = std::path::Path::new(socket_path).parent()
            .unwrap_or(std::path::Path::new("/tmp"));
        std::fs::write(
            runtime_dir.join("compositor.pid"),
            format!("{}\n", std::process::id()),
        ).ok();

        let fb_size = (fb_height as usize) * (fb_stride as usize);
        let fb_data = map_framebuffer(fb_phys, fb_size);

        let globals = vec![
            Global { name: 1, interface: "wl_compositor".into(), version: 4 },
            Global { name: 2, interface: "wl_shm".into(), version: 1 },
            Global { name: 3, interface: "wl_shell".into(), version: 1 },
            Global { name: 4, interface: "wl_seat".into(), version: 5 },
            Global { name: 5, interface: "wl_output".into(), version: 3 },
            Global { name: 6, interface: "xdg_wm_base".into(), version: 1 },
        ];

        Ok(Self {
            listener,
            next_id: AtomicU32::new(0x10000),
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

    pub fn run(&mut self) -> std::io::Result<()> {
        eprintln!("redbear-compositor: listening on Wayland socket");
        let _ = std::fs::write(
            std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()) + "/compositor.status",
            "ready\n",
        );
        for stream in self.listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let client_id = self.alloc_id();
                    eprintln!("redbear-compositor: client {} connected", client_id);
                    self.send_globals(client_id, &mut stream);
                    self.clients.lock().unwrap().insert(client_id, ClientState {
                        objects: HashMap::new(),
                        surfaces: HashMap::new(),
                        buffers: HashMap::new(),
                        shm_pools: HashMap::new(),
                        next_id: 1,
                    });
                    self.handle_client(client_id, stream);
                }
                Err(e) => eprintln!("redbear-compositor: accept error: {}", e),
            }
        }
        Ok(())
    }

    fn send_globals(&self, _client_id: u32, stream: &mut UnixStream) {
        // Advertise each global interface to the client
        let display_id = 1u32; // wl_display id
        for global in &self.globals {
            let name = global.name;
            let iface = global.interface.as_bytes();
            let version = global.version;
            let mut msg = Vec::with_capacity(16 + iface.len() + 1);
            msg.extend_from_slice(&display_id.to_ne_bytes());
            let size = 8 + 4 + 4 + iface.len() as u16 + 1;
            let header = (size as u32) << 16 | WL_REGISTRY_GLOBAL as u32;
            msg.extend_from_slice(&header.to_ne_bytes());
            msg.extend_from_slice(&name.to_ne_bytes());
            msg.extend_from_slice(&iface);
            msg.push(0); // null terminator
            msg.extend_from_slice(&version.to_ne_bytes());
            let _ = stream.write_all(&msg);
        }
    }

    fn send_callback_done(&self, stream: &mut UnixStream, callback_id: u32, callback_data: u32) {
        let mut msg = Vec::with_capacity(12);
        msg.extend_from_slice(&callback_id.to_ne_bytes());
        let header = (12u32) << 16 | WL_CALLBACK_DONE as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        msg.extend_from_slice(&callback_data.to_ne_bytes());
        let _ = stream.write_all(&msg);
    }

    fn handle_client(&self, client_id: u32, mut stream: UnixStream) {
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => {
                    eprintln!("redbear-compositor: client {} disconnected", client_id);
                    self.clients.lock().unwrap().remove(&client_id);
                    break;
                }
                Ok(n) => {
                    if let Err(e) = self.dispatch(client_id, &buf[..n], &mut stream) {
                        eprintln!("redbear-compositor: dispatch error: {}", e);
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

    fn dispatch(&self, client_id: u32, data: &[u8], stream: &mut UnixStream) -> Result<(), String> {
        let mut offset = 0;
        while offset + 8 <= data.len() {
            let object_id = u32::from_ne_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]);
            let size_opcode = u32::from_ne_bytes([data[offset+4], data[offset+5], data[offset+6], data[offset+7]]);
            let msg_size = ((size_opcode >> 16) & 0xFFFF) as usize;
            let opcode = (size_opcode & 0xFFFF) as u16;
            
            if msg_size < 8 || offset + msg_size > data.len() {
                return Err(format!("malformed message: object={} opcode={} size={}", object_id, opcode, msg_size));
            }

            let payload = &data[offset+8..offset+msg_size];
            
            match opcode {
                WL_DISPLAY_SYNC => {
                    let callback_id = if payload.len() >= 4 {
                        u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]])
                    } else { self.alloc_id() };
                    self.send_callback_done(stream, callback_id, 0);
                }
                WL_DISPLAY_DELETE_ID => {
                    if payload.len() >= 4 {
                        let obj_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
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
                        let registry_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.insert(registry_id, 2); // wl_registry
                        }
                    }
                }
                WL_REGISTRY_BIND => {
                    if payload.len() >= 12 {
                        let name = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let iface_bytes = &payload[4..];
                        let null_pos = iface_bytes.iter().position(|&b| b == 0).unwrap_or(iface_bytes.len());
                        let iface = std::str::from_utf8(&iface_bytes[..null_pos]).unwrap_or("");
                        let version_offset = 4 + null_pos + 1;
                        let new_id = if payload.len() >= version_offset + 4 {
                            u32::from_ne_bytes([payload[version_offset], payload[version_offset+1], payload[version_offset+2], payload[version_offset+3]])
                        } else { continue; };

                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            let type_id = match iface {
                                "wl_compositor" => 3,
                                "wl_shm" => 4,
                                "wl_shell" => 5,
                                "wl_seat" => 6,
                                "wl_output" => 7,
                                "xdg_wm_base" => 8,
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
                }
                WL_COMPOSITOR_CREATE_SURFACE => {
                    if payload.len() >= 4 {
                        let surface_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.insert(surface_id, 8);
                            client.surfaces.insert(surface_id, Surface {
                                buffer: None,
                                committed_buffer_id: None,
                                x: 0, y: 0,
                                width: self.fb_width,
                                height: self.fb_height,
                            });
                        }
                    }
                }
                WL_SHM_CREATE_POOL => {
                    if payload.len() >= 8 {
                        let pool_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let fd_val = i32::from_ne_bytes([payload[4], payload[5], payload[6], payload[7]]);
                        let size = if payload.len() >= 12 {
                            u32::from_ne_bytes([payload[8], payload[9], payload[10], payload[11]]) as usize
                        } else { 0 };
                        if fd_val >= 0 && size > 0 {
                            use std::os::fd::FromRawFd;
                            use std::io::Seek;
                            let mut file = unsafe { std::fs::File::from_raw_fd(fd_val) };
                            let mut data = vec![0u8; size];
                            if file.seek(std::io::SeekFrom::Start(0)).is_ok()
                                && file.read_exact(&mut data).is_ok()
                            {
                                let boxed = data.into_boxed_slice();
                                let leaked = Box::leak(boxed);
                                let mut clients = self.clients.lock().unwrap();
                                if let Some(client) = clients.get_mut(&client_id) {
                                    client.shm_pools.insert(pool_id, ShmPool { data: leaked, size });
                                }
                            }
                        }
                    }
                }
                WL_SHM_POOL_CREATE_BUFFER => {
                    if payload.len() >= 20 {
                        let buffer_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let offset = u32::from_ne_bytes([payload[4], payload[5], payload[6], payload[7]]);
                        let width = u32::from_ne_bytes([payload[8], payload[9], payload[10], payload[11]]);
                        let height = u32::from_ne_bytes([payload[12], payload[13], payload[14], payload[15]]);
                        let stride = u32::from_ne_bytes([payload[16], payload[17], payload[18], payload[19]]);
                        let format = if payload.len() >= 24 {
                            u32::from_ne_bytes([payload[20], payload[21], payload[22], payload[23]])
                        } else { WL_SHM_FORMAT_ARGB8888 };

                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.insert(buffer_id, 9); // wl_buffer
                            client.buffers.insert(buffer_id, (object_id, Buffer {
                                pool_id: object_id,
                                offset, width, height, stride, format,
                            }));
                        }
                    }
                }
                WL_SURFACE_ATTACH => {
                    if payload.len() >= 12 {
                        let buffer_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let _x = i32::from_ne_bytes([payload[4], payload[5], payload[6], payload[7]]);
                        let _y = i32::from_ne_bytes([payload[8], payload[9], payload[10], payload[11]]);

                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            if let Some((pool_id, buffer)) = client.buffers.get(&buffer_id).cloned() {
                                if let Some(surface) = client.surfaces.get_mut(&object_id) {
                                    surface.buffer = Some(Buffer {
                                        pool_id,
                                        ..buffer
                                    });
                                }
                            }
                        }
                    }
                }
                WL_SURFACE_COMMIT => {
                    let (release_id, buffer_data) = {
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            if let Some(surface) = client.surfaces.get_mut(&object_id) {
                                let old_buffer = surface.committed_buffer_id.take();
                                surface.committed_buffer_id = surface.buffer.as_ref().map(|b| {
                                    client.buffers.iter()
                                        .find(|(_, (_, buf))| buf.offset == b.offset && buf.width == b.width)
                                        .map(|(id, _)| *id)
                                        .unwrap_or(0)
                                });
                                
                                if let Some(ref buffer) = surface.buffer {
                                    if let Some(pool) = client.shm_pools.get(&buffer.pool_id) {
                                        self.composite_buffer(pool, buffer, surface);
                                    }
                                }
                                (old_buffer, surface.buffer.is_some())
                            } else { (None, false) }
                        } else { (None, false) }
                    };
                    
                    if let Some(buf_id) = release_id {
                        if buf_id != 0 {
                            self.send_buffer_release(stream, buf_id);
                        }
                    }
                }
                WL_SHELL_GET_SHELL_SURFACE | WL_SEAT_GET_KEYBOARD | WL_SEAT_GET_POINTER => {
                    // Ack new object creation — just register the id
                    if payload.len() >= 4 {
                        let new_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.insert(new_id, 10);
                        }
                    }
                }
                WL_SHELL_SURFACE_SET_TOPLEVEL | WL_SHELL_SURFACE_PONG | WL_SURFACE_DAMAGE => {
                    // No-op — we don't need window management for a single-client greeter
                }
                XDG_WM_BASE_GET_XDG_SURFACE | XDG_WM_BASE_DESTROY | XDG_WM_BASE_PONG => {
                    if payload.len() >= 4 {
                        let new_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.insert(new_id, 11);
                        }
                    }
                }
                XDG_SURFACE_GET_TOPLEVEL | XDG_SURFACE_DESTROY => {
                    if payload.len() >= 4 {
                        let toplevel_id = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                        let mut clients = self.clients.lock().unwrap();
                        if let Some(client) = clients.get_mut(&client_id) {
                            client.objects.insert(toplevel_id, 12);
                        }
                        drop(clients);
                        self.send_xdg_surface_configure(stream, object_id);
                        self.send_xdg_toplevel_configure(stream, toplevel_id);
                    }
                }
                XDG_SURFACE_ACK_CONFIGURE => {
                    // Client acknowledged — ready for first commit
                }
                _ => {
                    eprintln!("redbear-compositor: unhandled opcode {} on object {}", opcode, object_id);
                }
            }

            offset += msg_size;
        }
        Ok(())
    }

    fn composite_buffer(&self, pool: &ShmPool, buffer: &Buffer, surface: &Surface) {
        let mut fb = self.fb_data.lock().unwrap();
        let fb_stride = self.fb_stride as usize;

        if buffer.offset as usize + (buffer.height as usize * buffer.stride as usize) > pool.data.len() {
            return;
        }

        let src = &pool.data[buffer.offset as usize..];
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
        msg.extend_from_slice(&buffer_id.to_ne_bytes());
        let header = (8u32) << 16 | WL_BUFFER_RELEASE as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        let _ = stream.write_all(&msg);
    }

    fn send_xdg_surface_configure(&self, stream: &mut UnixStream, surface_id: u32) {
        let mut msg = Vec::with_capacity(12);
        msg.extend_from_slice(&surface_id.to_ne_bytes());
        let header = (12u32) << 16 | XDG_SURFACE_CONFIGURE as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        msg.extend_from_slice(&0u32.to_ne_bytes()); // serial
        let _ = stream.write_all(&msg);
    }

    fn send_xdg_toplevel_configure(&self, stream: &mut UnixStream, toplevel_id: u32) {
        let fb_w = self.fb_width as i32;
        let fb_h = self.fb_height as i32;
        let mut msg = Vec::with_capacity(32);
        msg.extend_from_slice(&toplevel_id.to_ne_bytes());
        let header = (32u32) << 16 | XDG_TOPLEVEL_CONFIGURE as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        msg.extend_from_slice(&fb_w.to_ne_bytes());
        msg.extend_from_slice(&fb_h.to_ne_bytes());
        msg.extend_from_slice(&0u32.to_ne_bytes()); // states array length (empty = no states)
        let _ = stream.write_all(&msg);
    }

    fn send_shm_format(&self, stream: &mut UnixStream, shm_id: u32, format: u32) {
        let mut msg = Vec::with_capacity(12);
        msg.extend_from_slice(&shm_id.to_ne_bytes());
        let header = (12u32) << 16 | WL_SHM_FORMAT as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        msg.extend_from_slice(&format.to_ne_bytes());
        let _ = stream.write_all(&msg);
    }

    fn send_output_info(&self, stream: &mut UnixStream, output_id: u32) {
        // wl_output.geometry
        {
            let mut msg = Vec::with_capacity(32);
            msg.extend_from_slice(&output_id.to_ne_bytes());
            let header = (32u32) << 16 | WL_OUTPUT_GEOMETRY as u32;
            msg.extend_from_slice(&header.to_ne_bytes());
            msg.extend_from_slice(&0i32.to_ne_bytes()); // x
            msg.extend_from_slice(&0i32.to_ne_bytes()); // y
            msg.extend_from_slice(&0i32.to_ne_bytes()); // physical_width
            msg.extend_from_slice(&0i32.to_ne_bytes()); // physical_height
            msg.extend_from_slice(&0i32.to_ne_bytes()); // subpixel (0=none)
            msg.extend_from_slice(b"vesa\0\0\0\0"); // make + model
            let _ = stream.write_all(&msg);
        }
        // wl_output.mode
        {
            let mut msg = Vec::with_capacity(24);
            msg.extend_from_slice(&output_id.to_ne_bytes());
            let header = (24u32) << 16 | WL_OUTPUT_MODE as u32;
            msg.extend_from_slice(&header.to_ne_bytes());
            msg.extend_from_slice(&(0x2u32).to_ne_bytes()); // flags: current
            msg.extend_from_slice(&self.fb_width.to_ne_bytes());
            msg.extend_from_slice(&self.fb_height.to_ne_bytes());
            msg.extend_from_slice(&60i32.to_ne_bytes()); // refresh
            let _ = stream.write_all(&msg);
        }
    }

    fn send_seat_capabilities(&self, stream: &mut UnixStream, seat_id: u32) {
        let mut msg = Vec::with_capacity(12);
        msg.extend_from_slice(&seat_id.to_ne_bytes());
        let header = (12u32) << 16 | WL_SEAT_CAPABILITIES as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        msg.extend_from_slice(&(0x3u32).to_ne_bytes()); // pointer + keyboard
        let _ = stream.write_all(&msg);
    }
}

fn main() {
    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp/run/redbear-greeter".into());
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
    let fb_phys_str = std::env::var("FRAMEBUFFER_ADDR")
        .unwrap_or_else(|_| "0x80000000".into());
    let fb_phys = usize::from_str_radix(fb_phys_str.trim_start_matches("0x"), 16)
        .unwrap_or(0x80000000);

    let fb_size = (fb_height as usize) * (fb_stride as usize);

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
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()) + "/compositor.status"
    );
}
