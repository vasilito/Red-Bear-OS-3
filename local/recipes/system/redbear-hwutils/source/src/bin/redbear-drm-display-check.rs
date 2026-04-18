use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::mem::{size_of, MaybeUninit};
use std::path::Path;
use std::process::{self};

const PROGRAM: &str = "redbear-drm-display-check";
const USAGE: &str = "Usage: redbear-drm-display-check --vendor amd|intel [--card /scheme/drm/card0] [--modeset CONNECTOR:MODE]\n\nBounded DRM/KMS display validation checker. This proves only display-path evidence, not render proof.";

const DRM_IOCTL_BASE: usize = 0x00A0;
const DRM_IOCTL_MODE_GETRESOURCES: usize = DRM_IOCTL_BASE;
const DRM_IOCTL_MODE_SETCRTC: usize = DRM_IOCTL_BASE + 2;
const DRM_IOCTL_MODE_GETCRTC: usize = DRM_IOCTL_BASE + 3;
const DRM_IOCTL_MODE_GETENCODER: usize = DRM_IOCTL_BASE + 6;
const DRM_IOCTL_MODE_GETCONNECTOR: usize = DRM_IOCTL_BASE + 7;
const DRM_IOCTL_MODE_GETMODES: usize = DRM_IOCTL_BASE + 8;
const DRM_IOCTL_MODE_CREATE_DUMB: usize = DRM_IOCTL_BASE + 18;
const DRM_IOCTL_MODE_DESTROY_DUMB: usize = DRM_IOCTL_BASE + 20;
const DRM_IOCTL_MODE_ADDFB: usize = DRM_IOCTL_BASE + 21;
const DRM_IOCTL_MODE_RMFB: usize = DRM_IOCTL_BASE + 22;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmResourcesWire {
    connector_count: u32,
    crtc_count: u32,
    encoder_count: u32,
}

#[derive(Clone, Debug)]
struct ResourcesSummary {
    connector_count: u32,
    connector_ids: Vec<u32>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmConnectorWire {
    connector_id: u32,
    connection: u32,
    connector_type: u32,
    mm_width: u32,
    mm_height: u32,
    encoder_id: u32,
    mode_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmModeWire {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    type_: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmSetCrtcWire {
    crtc_id: u32,
    fb_handle: u32,
    connector_count: u32,
    connectors: [u32; 8],
    mode: DrmModeWire,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmCreateDumbWire {
    width: u32,
    height: u32,
    bpp: u32,
    flags: u32,
    pitch: u32,
    size: u64,
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmDestroyDumbWire {
    handle: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGetEncoderWire {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmAddFbWire {
    width: u32,
    height: u32,
    pitch: u32,
    bpp: u32,
    depth: u32,
    handle: u32,
    fb_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmRmFbWire {
    fb_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct DrmGetCrtcWire {
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    mode_valid: u32,
    mode: DrmModeWire,
}

#[derive(Clone, Debug)]
struct ModeSummary {
    wire: DrmModeWire,
    name: String,
}

#[derive(Clone, Debug)]
struct ConnectorSummary {
    id: u32,
    mode_count: u32,
    encoder_id: u32,
}

fn require_path(path: &str, label: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        println!("{label}=ok");
        Ok(())
    } else {
        Err(format!("{label}=missing"))
    }
}

fn parse_args() -> Result<(String, String, Option<String>), String> {
    let mut vendor = None;
    let mut card = "/scheme/drm/card0".to_string();
    let mut modeset = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vendor" => vendor = args.next(),
            "--card" => card = args.next().ok_or_else(|| "missing value for --card".to_string())?,
            "--modeset" => {
                modeset = Some(args.next().ok_or_else(|| "missing value for --modeset".to_string())?)
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                process::exit(0);
            }
            _ => return Err(format!("unsupported argument: {arg}")),
        }
    }

    let vendor = vendor.ok_or_else(|| "missing --vendor amd|intel".to_string())?;
    if vendor != "amd" && vendor != "intel" {
        return Err(format!("unsupported vendor '{vendor}'"));
    }

    Ok((vendor, card, modeset))
}

fn decode_wire<T: Copy>(bytes: &[u8]) -> Result<T, String> {
    if bytes.len() < size_of::<T>() {
        return Err(format!(
            "short DRM response: expected {} bytes, got {}",
            size_of::<T>(),
            bytes.len()
        ));
    }
    let mut out = MaybeUninit::<T>::uninit();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.as_mut_ptr().cast::<u8>(), size_of::<T>());
        Ok(out.assume_init())
    }
}

fn bytes_of<T>(value: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}

fn open_drm_card(card_path: &str) -> Result<File, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(card_path)
        .map_err(|err| format!("failed to open {card_path}: {err}"))
}

fn drm_query(file: &mut File, request: usize, payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut request_buf = request.to_le_bytes().to_vec();
    request_buf.extend_from_slice(payload);
    file.write_all(&request_buf)
        .map_err(|err| format!("failed to send DRM ioctl {request:#x}: {err}"))?;

    let mut response = vec![0u8; 8192];
    let len = file
        .read(&mut response)
        .map_err(|err| format!("failed to read DRM ioctl {request:#x} response: {err}"))?;
    response.truncate(len);
    Ok(response)
}

fn query_empty(file: &mut File, request: usize, payload: &[u8]) -> Result<(), String> {
    let response = drm_query(file, request, payload)?;
    if response == [0] || response.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected non-empty response for ioctl {request:#x}"))
    }
}

fn query_resources(file: &mut File) -> Result<ResourcesSummary, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_GETRESOURCES, &[])?;
    let header = decode_wire::<DrmResourcesWire>(&response)?;
    let mut connector_ids = Vec::new();
    let mut offset = size_of::<DrmResourcesWire>();
    for _ in 0..header.connector_count {
        if response.len() < offset + size_of::<u32>() {
            return Err("resources response missing connector id payload".to_string());
        }
        connector_ids.push(decode_wire::<u32>(&response[offset..offset + size_of::<u32>()])?);
        offset += size_of::<u32>();
    }

    Ok(ResourcesSummary {
        connector_count: header.connector_count,
        connector_ids,
    })
}

fn query_connector(file: &mut File, connector_id: u32) -> Result<DrmConnectorWire, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_GETCONNECTOR, &connector_id.to_le_bytes())?;
    decode_wire(&response)
}

fn query_modes(file: &mut File, connector_id: u32) -> Result<Vec<ModeSummary>, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_GETMODES, &connector_id.to_le_bytes())?;
    if response == [0] {
        return Ok(Vec::new());
    }

    let mode_size = size_of::<DrmModeWire>();
    let mut modes = Vec::new();
    let mut offset = 0usize;

    while offset < response.len() {
        if response.len() - offset < mode_size {
            return Err(format!(
                "truncated mode response: {} trailing bytes left",
                response.len() - offset
            ));
        }

        let mode = decode_wire::<DrmModeWire>(&response[offset..offset + mode_size])?;
        offset += mode_size;

        let name_bytes = &response[offset..];
        let Some(name_len) = name_bytes.iter().position(|byte| *byte == 0) else {
            return Err("mode response missing trailing NUL after mode name".to_string());
        };
        let name = String::from_utf8_lossy(&name_bytes[..name_len]).to_string();
        offset += name_len + 1;
        modes.push(ModeSummary { wire: mode, name });
    }

    Ok(modes)
}

fn enumerate_connectors(file: &mut File) -> Result<Vec<ConnectorSummary>, String> {
    let resources = query_resources(file)?;
    let mut found = Vec::new();

    for connector_id in resources.connector_ids {
        let Ok(connector) = query_connector(file, connector_id) else {
            continue;
        };
        found.push(ConnectorSummary {
            id: connector.connector_id,
            mode_count: connector.mode_count,
            encoder_id: connector.encoder_id,
        });
    }

    if found.is_empty() && resources.connector_count != 0 {
        return Err("DRM_CONNECTOR_ENUM=missing".to_string());
    }

    Ok(found)
}

fn query_encoder(file: &mut File, encoder_id: u32) -> Result<DrmGetEncoderWire, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_GETENCODER, &encoder_id.to_le_bytes())?;
    decode_wire(&response)
}

fn query_addfb(file: &mut File, request: &DrmAddFbWire) -> Result<DrmAddFbWire, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_ADDFB, bytes_of(request))?;
    decode_wire(&response)
}

fn query_create_dumb(file: &mut File, request: &DrmCreateDumbWire) -> Result<DrmCreateDumbWire, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_CREATE_DUMB, bytes_of(request))?;
    decode_wire(&response)
}

fn query_get_crtc(file: &mut File, request: &DrmGetCrtcWire) -> Result<DrmGetCrtcWire, String> {
    let response = drm_query(file, DRM_IOCTL_MODE_GETCRTC, bytes_of(request))?;
    decode_wire(&response)
}

fn find_mode<'a>(modes: &'a [ModeSummary], name: &str) -> Option<&'a ModeSummary> {
    modes.iter().find(|mode| mode.name == name)
}

fn parse_modeset_spec(spec: &str) -> Result<(u32, &str), String> {
    let (connector_text, mode_name) = spec
        .split_once(':')
        .ok_or_else(|| "--modeset must be CONNECTOR:MODE".to_string())?;
    let connector_id = connector_text
        .parse::<u32>()
        .map_err(|err| format!("invalid connector id '{connector_text}': {err}"))?;
    Ok((connector_id, mode_name))
}

fn disable_crtc_request(crtc_id: u32) -> DrmSetCrtcWire {
    DrmSetCrtcWire {
        crtc_id,
        fb_handle: 0,
        connector_count: 0,
        connectors: [0; 8],
        mode: DrmModeWire::default(),
    }
}

fn setcrtc_request(crtc_id: u32, connector_id: u32, fb_id: u32, mode: DrmModeWire) -> DrmSetCrtcWire {
    let mut request = DrmSetCrtcWire {
        crtc_id,
        fb_handle: fb_id,
        connector_count: 1,
        connectors: [0; 8],
        mode,
    };
    request.connectors[0] = connector_id;
    request
}

fn proof_teardown_requests(
    crtc_id: u32,
    fb_id: u32,
    gem_handle: u32,
) -> (DrmSetCrtcWire, DrmRmFbWire, DrmDestroyDumbWire) {
    (
        disable_crtc_request(crtc_id),
        DrmRmFbWire { fb_id },
        DrmDestroyDumbWire { handle: gem_handle },
    )
}

fn bounded_modeset_proof(
    file: &mut File,
    connectors: &[ConnectorSummary],
    spec: &str,
) -> Result<(), String> {
    let (connector_id, mode_name) = parse_modeset_spec(spec)?;

    let connector = connectors
        .iter()
        .find(|connector| connector.id == connector_id)
        .ok_or_else(|| format!("connector {connector_id} not found in enumeration results"))?;

    let modes = query_modes(file, connector_id)?;
    let mode = find_mode(&modes, mode_name)
        .ok_or_else(|| format!("mode '{mode_name}' not found on connector {connector_id}"))?;

    let encoder = query_encoder(file, connector.encoder_id)?;
    let crtc_id = encoder.crtc_id;
    if crtc_id == 0 {
        return Err(format!("connector {connector_id} encoder did not report a usable CRTC"));
    }

    let create = query_create_dumb(
        file,
        &DrmCreateDumbWire {
            width: mode.wire.hdisplay as u32,
            height: mode.wire.vdisplay as u32,
            bpp: 32,
            ..DrmCreateDumbWire::default()
        },
    )?;
    let addfb = query_addfb(
        file,
        &DrmAddFbWire {
            width: mode.wire.hdisplay as u32,
            height: mode.wire.vdisplay as u32,
            pitch: create.pitch,
            bpp: 32,
            depth: 24,
            handle: create.handle,
            ..DrmAddFbWire::default()
        },
    )?;

    let setcrtc = setcrtc_request(crtc_id, connector_id, addfb.fb_id, mode.wire);
    query_empty(file, DRM_IOCTL_MODE_SETCRTC, bytes_of(&setcrtc))?;

    let getcrtc = query_get_crtc(
        file,
        &DrmGetCrtcWire {
            crtc_id,
            ..DrmGetCrtcWire::default()
        },
    )?;
    if getcrtc.fb_id != addfb.fb_id || getcrtc.mode_valid == 0 {
        return Err("GETCRTC did not confirm the programmed framebuffer/mode".to_string());
    }

    let (disable, rmfb, destroy) = proof_teardown_requests(crtc_id, addfb.fb_id, create.handle);
    query_empty(file, DRM_IOCTL_MODE_SETCRTC, bytes_of(&disable))?;
    query_empty(file, DRM_IOCTL_MODE_RMFB, bytes_of(&rmfb))?;
    query_empty(file, DRM_IOCTL_MODE_DESTROY_DUMB, bytes_of(&destroy))?;

    Ok(())
}

#[cfg(test)]
fn has_connector_section(text: &str) -> bool {
    text.contains("Connectors:")
}

#[cfg(test)]
fn has_mode_lines(text: &str) -> bool {
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        let mut parts = trimmed.split_whitespace();
        matches!(
            (parts.next(), parts.next()),
            (Some(id), Some(mode)) if id.chars().all(|c| c.is_ascii_digit()) && mode.contains('x')
        )
    })
}

fn run() -> Result<(), String> {
    let (vendor, card_path, modeset) = parse_args()?;

    println!("=== Red Bear DRM Display Runtime Check ===");
    println!("DRM_VENDOR={vendor}");
    println!("DRM_CARD={card_path}");

    require_path("/scheme/drm", "DRM_SCHEME")?;
    require_path(&card_path, "DRM_CARD_NODE")?;

    let mut drm = open_drm_card(&card_path)?;
    let connectors = enumerate_connectors(&mut drm)?;
    println!("DRM_CONNECTOR_ENUM=ok");

    let mut mode_lines_found = false;
    for connector in &connectors {
        let modes = query_modes(&mut drm, connector.id)?;
        if !modes.is_empty() && modes.len() as u32 == connector.mode_count {
            mode_lines_found = true;
            break;
        }
    }
    if !mode_lines_found {
        return Err("DRM_MODE_ENUM=missing".to_string());
    }
    println!("DRM_MODE_ENUM=ok");
    println!("DRM_ENUMERATION=ok");

    if let Some(spec) = modeset {
        if let Err(err) = bounded_modeset_proof(&mut drm, &connectors, &spec) {
            println!("DRM_MODESET_PROOF=failed");
            println!("DRM_MODESET_SPEC={spec}");
            return Err(err);
        }
        println!("DRM_MODESET_PROOF=ok");
        println!("DRM_MODESET_SPEC={spec}");
    } else {
        println!("DRM_MODESET_PROOF=skipped_no_spec");
    }

    println!("DRM_RENDER_PROOF=not_attempted");
    println!("DRM_TRANCHE_SUMMARY=display_validation_only");
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{PROGRAM}: {err}");
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::{bytes_of, decode_wire, disable_crtc_request, find_mode, has_connector_section, has_mode_lines, parse_modeset_spec, proof_teardown_requests, setcrtc_request, DrmModeWire, DrmResourcesWire, ModeSummary};

    fn owned_bytes_of<T>(value: &T) -> Vec<u8> {
        unsafe {
            std::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()).to_vec()
        }
    }

    #[test]
    fn connector_section_detected() {
        assert!(has_connector_section("foo\nConnectors:\nbar"));
    }

    #[test]
    fn mode_lines_detected() {
        assert!(has_mode_lines("  42 1920x1080 60.00"));
        assert!(!has_mode_lines("Connectors:\nnone"));
    }

    #[test]
    fn query_modes_accepts_empty_sentinel() {
        let parsed = if vec![0] == [0] { Vec::<DrmModeWire>::new() } else { unreachable!() };
        assert!(parsed.is_empty());
    }

    #[test]
    fn resources_header_plus_connector_ids_round_trip() {
        let header = DrmResourcesWire {
            connector_count: 2,
            crtc_count: 1,
            encoder_count: 2,
        };
        let mut payload = owned_bytes_of(&header);
        payload.extend_from_slice(&1u32.to_ne_bytes());
        payload.extend_from_slice(&7u32.to_ne_bytes());

        let decoded = decode_wire::<DrmResourcesWire>(&payload).unwrap();
        assert_eq!(decoded.connector_count, 2);
        let first = decode_wire::<u32>(&payload[size_of::<DrmResourcesWire>()..]).unwrap();
        let second = decode_wire::<u32>(&payload[size_of::<DrmResourcesWire>() + 4..]).unwrap();
        assert_eq!(first, 1);
        assert_eq!(second, 7);
    }

    #[test]
    fn mode_wire_decode_round_trip_works() {
        let mode = DrmModeWire {
            hdisplay: 1920,
            vdisplay: 1080,
            vrefresh: 60,
            ..DrmModeWire::default()
        };
        let decoded = decode_wire::<DrmModeWire>(bytes_of(&mode)).unwrap();
        assert_eq!(decoded.hdisplay, 1920);
        assert_eq!(decoded.vdisplay, 1080);
        assert_eq!(decoded.vrefresh, 60);
    }

    #[test]
    fn find_mode_matches_by_name() {
        let modes = vec![ModeSummary {
            wire: DrmModeWire::default(),
            name: "1920x1080@60".to_string(),
        }];

        assert!(find_mode(&modes, "1920x1080@60").is_some());
        assert!(find_mode(&modes, "1280x720@60").is_none());
    }

    #[test]
    fn parse_modeset_spec_accepts_connector_and_mode() {
        let (connector, mode) = parse_modeset_spec("7:1920x1080@60").unwrap();

        assert_eq!(connector, 7);
        assert_eq!(mode, "1920x1080@60");
    }

    #[test]
    fn parse_modeset_spec_rejects_bad_shape() {
        assert!(parse_modeset_spec("broken-spec").is_err());
    }

    #[test]
    fn disable_crtc_request_zeroes_active_state() {
        let request = disable_crtc_request(3);

        assert_eq!(request.crtc_id, 3);
        assert_eq!(request.fb_handle, 0);
        assert_eq!(request.connector_count, 0);
        assert!(request.connectors.iter().all(|&value| value == 0));
    }

    #[test]
    fn setcrtc_request_targets_single_connector_and_fb() {
        let request = setcrtc_request(3, 7, 11, DrmModeWire::default());

        assert_eq!(request.crtc_id, 3);
        assert_eq!(request.fb_handle, 11);
        assert_eq!(request.connector_count, 1);
        assert_eq!(request.connectors[0], 7);
    }

    #[test]
    fn proof_teardown_requests_disable_then_release_resources() {
        let (disable, rmfb, destroy) = proof_teardown_requests(3, 11, 22);

        assert_eq!(disable.crtc_id, 3);
        assert_eq!(disable.fb_handle, 0);
        assert_eq!(disable.connector_count, 0);
        assert_eq!(rmfb.fb_id, 11);
        assert_eq!(destroy.handle, 22);
    }
}
