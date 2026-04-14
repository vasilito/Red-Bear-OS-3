use std::collections::{BTreeMap, VecDeque};
use std::mem::size_of;
use std::mem::MaybeUninit;
use std::ptr;

use syscall::data::Stat;
use syscall::error::{Error, Result, EBADF, EBUSY, EFAULT, EINVAL, ENOENT, ENOTTY, EROFS};
use syscall::flag::{
    EventFlags, F_GETFD, F_GETFL, F_SETFD, F_SETFL, MODE_DIR, MODE_FILE, O_RDONLY, SEEK_CUR,
    SEEK_END, SEEK_SET,
};

use crate::device::{DeviceKind, InputDevice};
use crate::translate;
use crate::types::{
    ioc_dir, ioc_nr, ioc_size, ioc_type, is_evdev_ioctl, AbsInfo, InputEvent, InputId,
    EVDEV_IOCTL_TYPE, EVIOCGABS, EVIOCGEFFECTS, EVIOCGID, EVIOCGKEY, EVIOCGLED, EVIOCGNAME,
    EVIOCGPROP, EVIOCGRAB, EVIOCGVERSION, EVIOCRMFF, EVIOCSABS, EVIOCSCLOCKID, EVIOCSFF, EV_ABS,
    EV_KEY, EV_LED, EV_MSC, EV_REL, EV_REP, EV_VERSION, IOC_READ,
};

struct Handle {
    kind: HandleKind,
    offset: usize,
}

enum HandleKind {
    Root,
    Device {
        device_idx: usize,
        events: VecDeque<InputEvent>,
    },
}

pub struct EvdevScheme {
    next_id: usize,
    handles: BTreeMap<usize, Handle>,
    devices: Vec<InputDevice>,
    grabbed_by: BTreeMap<usize, usize>,
    mouse_buttons: [bool; 3],
    touchpad_position: (i32, i32),
    touchpad_touching: bool,
    next_tracking_id: i32,
    current_tracking_id: i32,
}

impl EvdevScheme {
    pub fn new() -> Self {
        let mut scheme = EvdevScheme {
            next_id: 0,
            handles: BTreeMap::new(),
            devices: Vec::new(),
            grabbed_by: BTreeMap::new(),
            mouse_buttons: [false; 3],
            touchpad_position: (0, 0),
            touchpad_touching: false,
            next_tracking_id: 1,
            current_tracking_id: -1,
        };
        scheme.devices.push(InputDevice::new_keyboard(0));
        scheme.devices.push(InputDevice::new_mouse(1));
        scheme.devices.push(InputDevice::new_touchpad(2));
        scheme
    }

    fn device_index(&self, kind: DeviceKind) -> Option<usize> {
        self.devices.iter().position(|d| d.kind == kind)
    }

    fn current_tracking_id(&self) -> i32 {
        if self.touchpad_touching {
            self.current_tracking_id
        } else {
            -1
        }
    }

    fn queue_device_events(&mut self, kind: DeviceKind, events: &[InputEvent]) {
        if events.is_empty() {
            return;
        }

        let Some(device_idx) = self.device_index(kind) else {
            return;
        };

        for event in events {
            if event.event_type == EV_KEY {
                self.devices[device_idx].update_key_state(event.code, event.value != 0);
            } else if event.event_type == EV_LED {
                self.devices[device_idx].update_led_state(event.code, event.value != 0);
            }
        }

        let grabbed_handle = self.grabbed_by.get(&device_idx).copied();

        for (handle_id, handle) in self.handles.iter_mut() {
            if let HandleKind::Device {
                device_idx: handle_device_idx,
                events: handle_events,
            } = &mut handle.kind
            {
                if *handle_device_idx == device_idx {
                    if let Some(grabbed_id) = grabbed_handle {
                        if *handle_id != grabbed_id {
                            continue;
                        }
                    }
                    handle_events.extend(events.iter().copied());
                }
            }
        }
    }

    fn pop_handle_bytes(events: &mut VecDeque<InputEvent>, buf: &mut [u8]) -> usize {
        let event_count = buf.len() / InputEvent::SIZE;
        let mut written = 0;

        for _ in 0..event_count {
            let Some(event) = events.pop_front() else {
                break;
            };

            let bytes = event.to_bytes();
            buf[written..written + InputEvent::SIZE].copy_from_slice(&bytes);
            written += InputEvent::SIZE;
        }

        written
    }

    pub fn feed_keyboard_event(&mut self, scancode: u8, pressed: bool) {
        let events = translate::translate_keyboard(scancode, pressed);
        self.queue_device_events(DeviceKind::Keyboard, &events);
    }

    pub fn feed_mouse_move(&mut self, dx: i32, dy: i32) {
        let events = translate::translate_mouse_motion(dx, dy);
        self.queue_device_events(DeviceKind::Mouse, &events);
    }

    pub fn feed_mouse_scroll(&mut self, x: i32, y: i32) {
        let events = translate::translate_mouse_scroll(x, y);
        self.queue_device_events(DeviceKind::Mouse, &events);
    }

    pub fn feed_mouse_buttons(&mut self, left: bool, middle: bool, right: bool) {
        let old_buttons = self.mouse_buttons;
        let new_buttons = [left, middle, right];
        for (index, (&old, &new)) in old_buttons.iter().zip(new_buttons.iter()).enumerate() {
            if old != new {
                let events = translate::translate_mouse_button(index, new);
                self.queue_device_events(DeviceKind::Mouse, &events);
            }
        }
        self.mouse_buttons = new_buttons;

        let touching = left;
        if touching != self.touchpad_touching {
            if touching {
                self.current_tracking_id = self.next_tracking_id;
                self.next_tracking_id = self.next_tracking_id.saturating_add(1);
            }

            self.touchpad_touching = touching;
            let (x, y) = self.touchpad_position;
            let tracking_id = self.current_tracking_id();
            let events = translate::translate_touchpad_contact(x, y, touching, tracking_id);
            self.queue_device_events(DeviceKind::Touchpad, &events);
        }
    }

    pub fn feed_touchpad_position(&mut self, x: i32, y: i32) {
        self.touchpad_position = (x, y);
        let touching = self.touchpad_touching;
        let tracking_id = self.current_tracking_id();
        let events = translate::translate_touchpad_motion(x, y, touching, tracking_id);
        self.queue_device_events(DeviceKind::Touchpad, &events);
    }

    fn ioctl_name_len(cmd: u64) -> Option<usize> {
        if cmd == EVIOCGNAME || (ioc_type(cmd) == EVDEV_IOCTL_TYPE && ioc_nr(cmd) == 0x06) {
            let size = ioc_size(cmd);
            return Some(if size == 0 { 256 } else { size });
        }
        None
    }

    fn ioctl_bit_ev_and_len(cmd: u64) -> Option<(u8, usize)> {
        if !is_evdev_ioctl(cmd) {
            return None;
        }

        let nr = ioc_nr(cmd);
        if !(0x20..0x40).contains(&nr) {
            return None;
        }

        let size = ioc_size(cmd);
        Some(((nr - 0x20) as u8, size))
    }

    fn ioctl_abs_axis(cmd: u64) -> Option<u16> {
        if !is_evdev_ioctl(cmd) {
            return None;
        }

        let nr = ioc_nr(cmd);
        if !(0x40..0x80).contains(&nr) {
            return None;
        }

        Some((nr - 0x40) as u16)
    }

    fn device_bitmap(device: &InputDevice, ev: u8) -> Vec<u8> {
        match u16::from(ev) {
            0 => device.supported_event_types(),
            EV_KEY => device.supported_keys(),
            EV_REL => device.supported_rel(),
            EV_ABS => device.supported_abs(),
            EV_MSC => device.supported_msc(),
            EV_LED => device.supported_leds(),
            EV_REP => device.supported_rep(),
            _ => Vec::new(),
        }
    }

    unsafe fn write_value_to_user<T: Copy>(arg: usize, value: &T) -> Result<usize> {
        if arg == 0 {
            return Err(Error::new(EFAULT));
        }

        ptr::copy_nonoverlapping(
            value as *const T as *const u8,
            arg as *mut u8,
            size_of::<T>(),
        );
        Ok(size_of::<T>())
    }

    unsafe fn write_bytes_to_user(arg: usize, bytes: &[u8]) -> Result<usize> {
        if arg == 0 {
            return Err(Error::new(EFAULT));
        }

        if !bytes.is_empty() {
            ptr::copy_nonoverlapping(bytes.as_ptr(), arg as *mut u8, bytes.len());
        }
        Ok(bytes.len())
    }

    unsafe fn read_value_from_user<T: Copy>(arg: usize) -> Result<T> {
        if arg == 0 {
            return Err(Error::new(EFAULT));
        }

        let mut value = MaybeUninit::<T>::uninit();
        ptr::copy_nonoverlapping(
            arg as *const u8,
            value.as_mut_ptr() as *mut u8,
            size_of::<T>(),
        );
        Ok(value.assume_init())
    }

    fn ioctl_abs_set_axis(cmd: u64) -> Option<u16> {
        if !is_evdev_ioctl(cmd) {
            return None;
        }

        let nr = ioc_nr(cmd);
        if !(0xc0..0x100).contains(&nr) {
            return None;
        }

        Some((nr - 0xc0) as u16)
    }

    fn device_prop_bitmap(device: &InputDevice) -> [u8; 64] {
        let bitmap = device.supported_props();
        let mut bytes = [0u8; 64];
        let copy_len = bitmap.len().min(bytes.len());
        if copy_len > 0 {
            bytes[..copy_len].copy_from_slice(&bitmap[..copy_len]);
        }
        bytes
    }
}

impl redox_scheme::SchemeBlockMut for EvdevScheme {
    fn open(&mut self, path: &str, _flags: usize, _uid: u32, _gid: u32) -> Result<Option<usize>> {
        let cleaned = path.trim_matches('/');

        let kind = if cleaned.is_empty() {
            HandleKind::Root
        } else if let Some(rest) = cleaned.strip_prefix("event") {
            let idx: usize = rest
                .trim_end_matches('/')
                .parse()
                .map_err(|_| Error::new(ENOENT))?;
            if idx >= self.devices.len() {
                return Err(Error::new(ENOENT));
            }
            HandleKind::Device {
                device_idx: idx,
                events: VecDeque::new(),
            }
        } else {
            return Err(Error::new(ENOENT));
        };

        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, Handle { kind, offset: 0 });
        Ok(Some(id))
    }

    fn read(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;

        match &mut handle.kind {
            HandleKind::Root => {
                let mut listing = String::new();
                for (i, _dev) in self.devices.iter().enumerate() {
                    listing.push_str(&format!("event{}\n", i));
                }
                let bytes = listing.as_bytes();
                if handle.offset >= bytes.len() {
                    return Ok(Some(0));
                }
                let remaining = &bytes[handle.offset..];
                let to_copy = remaining.len().min(buf.len());
                buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
                handle.offset += to_copy;
                Ok(Some(to_copy))
            }
            HandleKind::Device { events, .. } => {
                if !events.is_empty() && buf.len() < InputEvent::SIZE {
                    return Err(Error::new(EINVAL));
                }

                let written = Self::pop_handle_bytes(events, buf);
                handle.offset += written;
                Ok(if written == 0 { None } else { Some(written) })
            }
        }
    }

    fn write(&mut self, id: usize, _buf: &[u8]) -> Result<Option<usize>> {
        let _ = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        Err(Error::new(EROFS))
    }

    fn seek(&mut self, id: usize, pos: isize, whence: usize) -> Result<Option<isize>> {
        let handle = self.handles.get_mut(&id).ok_or(Error::new(EBADF))?;
        let new_offset = match whence {
            SEEK_SET => pos as isize,
            SEEK_CUR => handle.offset as isize + pos,
            SEEK_END => pos,
            _ => return Err(Error::new(EINVAL)),
        };
        if new_offset < 0 {
            return Err(Error::new(EINVAL));
        }
        handle.offset = new_offset as usize;
        Ok(Some(new_offset))
    }

    fn fstat(&mut self, id: usize, stat: &mut Stat) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        match &handle.kind {
            HandleKind::Root => {
                stat.st_mode = MODE_DIR | 0o555;
            }
            HandleKind::Device { .. } => {
                stat.st_mode = MODE_FILE | 0o444;
            }
        }
        Ok(Some(0))
    }

    fn close(&mut self, id: usize) -> Result<Option<usize>> {
        self.grabbed_by.retain(|_, grabbed_id| *grabbed_id != id);
        self.handles.remove(&id);
        Ok(Some(0))
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8]) -> Result<Option<usize>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;
        let path = match &handle.kind {
            HandleKind::Root => "evdev:".to_string(),
            HandleKind::Device { device_idx, .. } => format!("evdev:event{}", device_idx),
        };
        let bytes = path.as_bytes();
        let to_copy = bytes.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&bytes[..to_copy]);
        Ok(Some(to_copy))
    }

    fn fcntl(&mut self, id: usize, cmd_raw: usize, arg: usize) -> Result<Option<usize>> {
        let device_idx = match &self.handles.get(&id).ok_or(Error::new(EBADF))?.kind {
            HandleKind::Root => None,
            HandleKind::Device { device_idx, .. } => Some(*device_idx),
        };

        match cmd_raw {
            F_GETFL => return Ok(Some(O_RDONLY)),
            F_GETFD => return Ok(Some(0)),
            F_SETFL | F_SETFD => {
                return Ok(Some(0));
            }
            _ => {}
        }

        let Some(idx) = device_idx else {
            return Err(Error::new(EINVAL));
        };

        if cmd_raw == EVIOCGRAB as usize {
            let grab = unsafe { Self::read_value_from_user::<i32>(arg)? };
            return match grab {
                0 => {
                    if self.grabbed_by.get(&idx) == Some(&id) {
                        self.grabbed_by.remove(&idx);
                    }
                    Ok(Some(0))
                }
                1 => match self.grabbed_by.get(&idx).copied() {
                    Some(grabbed_id) if grabbed_id != id => Err(Error::new(EBUSY)),
                    _ => {
                        self.grabbed_by.insert(idx, id);
                        Ok(Some(0))
                    }
                },
                _ => Err(Error::new(EINVAL)),
            };
        }

        if cmd_raw == EVIOCSCLOCKID as usize {
            return Ok(Some(0));
        }
        let cmd = cmd_raw as u64;

        if matches!(cmd, EVIOCSFF | EVIOCRMFF | EVIOCGEFFECTS) {
            return Err(Error::new(ENOTTY));
        }

        if cmd == EVIOCSABS || Self::ioctl_abs_set_axis(cmd).is_some() {
            let axis = Self::ioctl_abs_set_axis(cmd).unwrap_or(0);
            let abs_info = unsafe { Self::read_value_from_user::<AbsInfo>(arg)? };
            self.devices[idx].set_abs_info(axis, abs_info);
            return Ok(Some(0));
        }

        let device = &self.devices[idx];

        if cmd == EVIOCGVERSION {
            let version = EV_VERSION;
            return unsafe { Self::write_value_to_user(arg, &version).map(Some) };
        }

        if cmd == EVIOCGID {
            let input_id: InputId = device.input_id;
            return unsafe { Self::write_value_to_user(arg, &input_id).map(Some) };
        }

        if cmd == EVIOCGKEY {
            let key_state = device.key_state;
            return unsafe { Self::write_bytes_to_user(arg, &key_state).map(Some) };
        }

        if cmd == EVIOCGLED {
            let led_state = device.led_state;
            return unsafe { Self::write_bytes_to_user(arg, &led_state).map(Some) };
        }

        if cmd == EVIOCGPROP {
            let props = Self::device_prop_bitmap(device);
            return unsafe { Self::write_bytes_to_user(arg, &props).map(Some) };
        }

        if let Some(name_len) = Self::ioctl_name_len(cmd) {
            let mut bytes = vec![0u8; name_len];
            let name = device.name.as_bytes();
            let copy_len = name.len().min(bytes.len().saturating_sub(1));
            if copy_len > 0 {
                bytes[..copy_len].copy_from_slice(&name[..copy_len]);
            }
            return unsafe { Self::write_bytes_to_user(arg, &bytes).map(Some) };
        }

        if let Some((ev, len)) = Self::ioctl_bit_ev_and_len(cmd) {
            let bitmap = Self::device_bitmap(device, ev);
            let out_len = if len == 0 {
                bitmap.len()
            } else {
                len.max(bitmap.len()).min(len)
            };
            let mut bytes = vec![0u8; out_len];
            let copy_len = bitmap.len().min(bytes.len());
            if copy_len > 0 {
                bytes[..copy_len].copy_from_slice(&bitmap[..copy_len]);
            }
            return unsafe { Self::write_bytes_to_user(arg, &bytes).map(Some) };
        }

        if cmd == EVIOCGABS || Self::ioctl_abs_axis(cmd).is_some() {
            let axis = Self::ioctl_abs_axis(cmd).unwrap_or(0);
            let abs_info: AbsInfo = device.abs_info(axis);
            return unsafe { Self::write_value_to_user(arg, &abs_info).map(Some) };
        }

        if is_evdev_ioctl(cmd) && ioc_dir(cmd) == IOC_READ {
            return Err(Error::new(EINVAL));
        }

        Err(Error::new(EINVAL))
    }

    fn fevent(&mut self, id: usize, flags: EventFlags) -> Result<Option<EventFlags>> {
        let handle = self.handles.get(&id).ok_or(Error::new(EBADF))?;

        let readiness = match &handle.kind {
            HandleKind::Root => flags,
            HandleKind::Device { events, .. } if !events.is_empty() => {
                flags & EventFlags::EVENT_READ
            }
            HandleKind::Device { .. } => EventFlags::empty(),
        };

        Ok(Some(readiness))
    }
}

#[cfg(test)]
mod tests {
    use redox_scheme::SchemeBlockMut;

    use super::EvdevScheme;
    use crate::types::{
        AbsInfo, InputEvent, ABS_MT_SLOT, EVIOCGEFFECTS, EVIOCGPROP, EVIOCGRAB, EVIOCRMFF,
        EVIOCSABS, EVIOCSFF, INPUT_PROP_POINTER,
    };

    fn open_device(scheme: &mut EvdevScheme, index: usize) -> usize {
        scheme
            .open(&format!("event{index}"), 0, 0, 0)
            .expect("open should succeed")
            .expect("device handle id")
    }

    fn read_events(scheme: &mut EvdevScheme, id: usize) -> Option<usize> {
        let mut buf = vec![0u8; InputEvent::SIZE * 8];
        scheme.read(id, &mut buf).expect("read should succeed")
    }

    #[test]
    fn eviocgrab_routes_events_only_to_grabbing_handle() {
        let mut scheme = EvdevScheme::new();
        let first = open_device(&mut scheme, 0);
        let second = open_device(&mut scheme, 0);

        let grab = 1i32;
        scheme
            .fcntl(first, EVIOCGRAB as usize, (&grab as *const i32) as usize)
            .expect("grab should succeed");

        let err = scheme
            .fcntl(second, EVIOCGRAB as usize, (&grab as *const i32) as usize)
            .expect_err("second grab should fail");
        assert_eq!(err.errno, syscall::error::EBUSY);

        scheme.feed_keyboard_event(0x1E, true);

        assert!(read_events(&mut scheme, first).is_some());
        assert_eq!(read_events(&mut scheme, second), None);

        let release = 0i32;
        scheme
            .fcntl(first, EVIOCGRAB as usize, (&release as *const i32) as usize)
            .expect("release should succeed");

        scheme.feed_keyboard_event(0x30, true);

        assert!(read_events(&mut scheme, first).is_some());
        assert!(read_events(&mut scheme, second).is_some());
    }

    #[test]
    fn closing_grabbed_handle_releases_grab() {
        let mut scheme = EvdevScheme::new();
        let first = open_device(&mut scheme, 0);
        let second = open_device(&mut scheme, 0);

        let grab = 1i32;
        scheme
            .fcntl(first, EVIOCGRAB as usize, (&grab as *const i32) as usize)
            .expect("grab should succeed");
        scheme.close(first).expect("close should succeed");

        scheme.feed_keyboard_event(0x1E, true);

        assert!(read_events(&mut scheme, second).is_some());
    }

    #[test]
    fn eviocgrab_is_scoped_to_each_device() {
        let mut scheme = EvdevScheme::new();
        let keyboard = open_device(&mut scheme, 0);
        let mouse = open_device(&mut scheme, 1);

        let grab = 1i32;

        scheme
            .fcntl(keyboard, EVIOCGRAB as usize, (&grab as *const i32) as usize)
            .expect("keyboard grab should succeed");
        scheme
            .fcntl(mouse, EVIOCGRAB as usize, (&grab as *const i32) as usize)
            .expect("mouse grab should also succeed");
    }

    #[test]
    fn eviocgprop_reports_pointer_capability_for_pointer_devices() {
        let mut scheme = EvdevScheme::new();
        let keyboard = open_device(&mut scheme, 0);
        let mouse = open_device(&mut scheme, 1);
        let touchpad = open_device(&mut scheme, 2);

        let mut keyboard_props = [0u8; 64];
        let mut mouse_props = [0u8; 64];
        let mut touchpad_props = [0u8; 64];

        let keyboard_len = scheme
            .fcntl(
                keyboard,
                EVIOCGPROP as usize,
                keyboard_props.as_mut_ptr() as usize,
            )
            .expect("keyboard props ioctl should succeed")
            .expect("keyboard props length");
        let mouse_len = scheme
            .fcntl(
                mouse,
                EVIOCGPROP as usize,
                mouse_props.as_mut_ptr() as usize,
            )
            .expect("mouse props ioctl should succeed")
            .expect("mouse props length");
        let touchpad_len = scheme
            .fcntl(
                touchpad,
                EVIOCGPROP as usize,
                touchpad_props.as_mut_ptr() as usize,
            )
            .expect("touchpad props ioctl should succeed")
            .expect("touchpad props length");

        let pointer_mask = 1u8 << INPUT_PROP_POINTER;
        assert_eq!(keyboard_len, 64);
        assert_eq!(mouse_len, 64);
        assert_eq!(touchpad_len, 64);
        assert_eq!(keyboard_props[0] & pointer_mask, 0);
        assert_ne!(mouse_props[0] & pointer_mask, 0);
        assert_ne!(touchpad_props[0] & pointer_mask, 0);
    }

    #[test]
    fn eviocsabs_overrides_default_abs_info() {
        let mut scheme = EvdevScheme::new();
        let touchpad = open_device(&mut scheme, 2);

        let abs_info = AbsInfo {
            value: 7,
            minimum: -10,
            maximum: 1234,
            fuzz: 2,
            flat: 3,
            resolution: 4,
        };
        let mut reported = AbsInfo::default();

        scheme
            .fcntl(
                touchpad,
                EVIOCSABS as usize,
                (&abs_info as *const AbsInfo) as usize,
            )
            .expect("set abs info should succeed");
        scheme
            .fcntl(
                touchpad,
                crate::types::eviocgabs(crate::types::ABS_X as u8) as usize,
                (&mut reported as *mut AbsInfo) as usize,
            )
            .expect("get abs info should succeed");

        assert_eq!(reported.value, abs_info.value);
        assert_eq!(reported.minimum, abs_info.minimum);
        assert_eq!(reported.maximum, abs_info.maximum);
        assert_eq!(reported.fuzz, abs_info.fuzz);
        assert_eq!(reported.flat, abs_info.flat);
        assert_eq!(reported.resolution, abs_info.resolution);
    }

    #[test]
    fn multitouch_slot_abs_info_reports_nine_slots() {
        let mut scheme = EvdevScheme::new();
        let touchpad = open_device(&mut scheme, 2);
        let mut reported = AbsInfo::default();

        scheme
            .fcntl(
                touchpad,
                crate::types::eviocgabs(ABS_MT_SLOT as u8) as usize,
                (&mut reported as *mut AbsInfo) as usize,
            )
            .expect("get mt slot abs info should succeed");

        assert_eq!(reported.minimum, 0);
        assert_eq!(reported.maximum, 9);
    }

    #[test]
    fn force_feedback_ioctls_return_enotty() {
        let mut scheme = EvdevScheme::new();
        let mouse = open_device(&mut scheme, 1);

        for cmd in [
            EVIOCSFF as usize,
            EVIOCRMFF as usize,
            EVIOCGEFFECTS as usize,
        ] {
            let err = scheme
                .fcntl(mouse, cmd, 0)
                .expect_err("force feedback ioctl should fail");
            assert_eq!(err.errno, syscall::error::ENOTTY);
        }
    }
}
