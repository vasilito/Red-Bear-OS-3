// Integration test: verifies the compositor's Wayland protocol implementation
// by starting a real compositor instance and connecting as a client.

use std::os::unix::net::UnixStream;
use std::io::{Read, Write};
use std::process::{Command, Child};
use std::time::Duration;
use std::thread;

struct WaylandClient {
    stream: UnixStream,
    next_id: u32,
}

impl WaylandClient {
    fn connect(socket_path: &str) -> std::io::Result<Self> {
        for _ in 0..20 {
            if std::path::Path::new(socket_path).exists() {
                let stream = UnixStream::connect(socket_path)?;
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                return Ok(Self { stream, next_id: 2 });
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "compositor socket did not appear"))
    }

    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_message(&mut self, object_id: u32, opcode: u16, payload: &[u8]) -> std::io::Result<()> {
        let size = 8 + payload.len();
        let mut msg = Vec::with_capacity(size);
        msg.extend_from_slice(&object_id.to_ne_bytes());
        let header = ((size as u32) << 16) | opcode as u32;
        msg.extend_from_slice(&header.to_ne_bytes());
        msg.extend_from_slice(payload);
        self.stream.write_all(&msg)
    }

    fn read_message(&mut self) -> std::io::Result<(u32, u16, Vec<u8>)> {
        let mut header = [0u8; 8];
        self.stream.read_exact(&mut header)?;
        let object_id = u32::from_ne_bytes([header[0], header[1], header[2], header[3]]);
        let size_opcode = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
        let size = ((size_opcode >> 16) & 0xFFFF) as usize;
        let opcode = (size_opcode & 0xFFFF) as u16;
        let mut payload = vec![0u8; size - 8];
        if size > 8 {
            self.stream.read_exact(&mut payload)?;
        }
        Ok((object_id, opcode, payload))
    }

    fn sync(&mut self) -> std::io::Result<u32> {
        let callback_id = self.alloc_id();
        self.send_message(1, 0, &callback_id.to_ne_bytes())?; // wl_display.sync
        Ok(callback_id)
    }

    fn get_registry(&mut self) -> std::io::Result<u32> {
        let registry_id = self.alloc_id();
        self.send_message(1, 1, &registry_id.to_ne_bytes())?; // wl_display.get_registry
        Ok(registry_id)
    }

    fn bind(&mut self, registry_id: u32, name: u32, iface: &str, version: u32) -> std::io::Result<u32> {
        let new_id = self.alloc_id();
        let iface_bytes = iface.as_bytes();
        let mut payload = Vec::with_capacity(4 + iface_bytes.len() + 1 + 4 + 4);
        payload.extend_from_slice(&name.to_ne_bytes());
        payload.extend_from_slice(iface_bytes);
        payload.push(0);
        payload.extend_from_slice(&version.to_ne_bytes());
        payload.extend_from_slice(&new_id.to_ne_bytes());
        self.send_message(registry_id, 0, &payload)?; // wl_registry.bind
        Ok(new_id)
    }
}

fn start_compositor(socket_path: &str) -> Child {
    let compositor_bin = std::env::var("COMPOSITOR_BIN")
        .unwrap_or_else(|_| "target/debug/redbear-compositor".into());
    
    let runtime_dir = std::path::Path::new(socket_path).parent().unwrap();
    std::fs::create_dir_all(runtime_dir).ok();
    
    let mut cmd = Command::new(&compositor_bin);
    cmd.env("WAYLAND_DISPLAY", socket_path.rsplit('/').next().unwrap_or("wayland-0"))
       .env("XDG_RUNTIME_DIR", runtime_dir)
       .env("FRAMEBUFFER_WIDTH", "1280")
       .env("FRAMEBUFFER_HEIGHT", "720")
       .env("FRAMEBUFFER_STRIDE", "5120")
       .env("FRAMEBUFFER_ADDR", "0x80000000");
    
    cmd.spawn().expect("failed to start compositor")
}

#[test]
fn test_compositor_globals() {
    let socket = "/tmp/test-redbear-compositor.sock";
    let _ = std::fs::remove_file(socket);
    
    let mut compositor = start_compositor(socket);
    
    let mut client = WaylandClient::connect(socket).expect("failed to connect");
    
    // Get registry
    let _registry = client.get_registry().expect("get_registry failed");
    
    // Read global events
    let mut globals = Vec::new();
    for _ in 0..6 {
        match client.read_message() {
            Ok((_obj_id, opcode, payload)) => {
                assert_eq!(opcode, 0); // wl_registry.global
                let name = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let iface_end = payload[4..].iter().position(|&b| b == 0).unwrap_or(0);
                let iface = std::str::from_utf8(&payload[4..4+iface_end]).unwrap();
                globals.push((name, iface.to_string()));
            }
            Err(e) => {
                eprintln!("read error: {}", e);
                break;
            }
        }
    }
    
    assert!(globals.iter().any(|(_, i)| i == "wl_compositor"), "wl_compositor missing");
    assert!(globals.iter().any(|(_, i)| i == "wl_shm"), "wl_shm missing");
    assert!(globals.iter().any(|(_, i)| i == "wl_shell"), "wl_shell missing");
    assert!(globals.iter().any(|(_, i)| i == "wl_seat"), "wl_seat missing");
    assert!(globals.iter().any(|(_, i)| i == "wl_output"), "wl_output missing");
    assert!(globals.iter().any(|(_, i)| i == "xdg_wm_base"), "xdg_wm_base missing");
    
    compositor.kill().ok();
    let _ = std::fs::remove_file(socket);
}

#[test]
fn test_compositor_shm_formats() {
    let socket = "/tmp/test-redbear-compositor-shm.sock";
    let _ = std::fs::remove_file(socket);
    
    let mut compositor = start_compositor(socket);
    let mut client = WaylandClient::connect(socket).expect("failed to connect");
    
    let registry = client.get_registry().expect("get_registry failed");
    
    // Read globals to find wl_shm name
    let mut shm_name = 0u32;
    for _ in 0..6 {
        let (_, _, payload) = client.read_message().expect("read failed");
        let name = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let iface_end = payload[4..].iter().position(|&b| b == 0).unwrap_or(0);
        let iface = std::str::from_utf8(&payload[4..4+iface_end]).unwrap();
        if iface == "wl_shm" { shm_name = name; break; }
    }
    
    assert_ne!(shm_name, 0, "wl_shm global not found");
    
    // Bind wl_shm
    let _shm = client.bind(registry, shm_name, "wl_shm", 1).expect("bind shm failed");
    
    // Should receive format events
    let mut formats = Vec::new();
    for _ in 0..3 {
        match client.read_message() {
            Ok((_, opcode, payload)) => {
                if opcode == 0 && payload.len() >= 4 {
                    let format = u32::from_ne_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    formats.push(format);
                }
            }
            Err(_) => break,
        }
    }
    
    assert!(!formats.is_empty(), "no wl_shm.format events received");
    
    compositor.kill().ok();
    let _ = std::fs::remove_file(socket);
}

#[test]
fn test_compositor_sync_roundtrip() {
    let socket = "/tmp/test-redbear-compositor-sync.sock";
    let _ = std::fs::remove_file(socket);
    
    let mut compositor = start_compositor(socket);
    let mut client = WaylandClient::connect(socket).expect("failed to connect");
    
    let callback_id = client.sync().expect("sync failed");
    
    // Should receive callback.done
    let (obj_id, opcode, payload) = client.read_message().expect("read failed");
    assert_eq!(obj_id, callback_id, "callback id mismatch");
    assert_eq!(opcode, 0, "expected callback.done (opcode 0)");
    assert_eq!(payload.len(), 4, "callback.done payload should be 4 bytes");
    
    compositor.kill().ok();
    let _ = std::fs::remove_file(socket);
}
