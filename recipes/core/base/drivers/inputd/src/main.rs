//! `:input`
//!
//! A seperate scheme is required since all of the input from different input devices is required
//! to be combined into a single stream which is later going to be processed by the "consumer"
//! which usually is Orbital.
//!
//! ## Input Device ("producer")
//! Write events to `input:producer`.
//!
//! ## Input Consumer ("consumer")
//! Read events from `input:consumer`. Optionally, set the `EVENT_READ` flag to be notified when
//! events are available.

use core::mem::size_of;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::mem::transmute;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicUsize, Ordering};

use inputd::{ControlEvent, VtEvent, VtEventKind};

use libredox::errno::ESTALE;
use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult, Response, SignalBehavior, Socket};

use orbclient::{Event, EventOption};
use scheme_utils::{Blocking, FpathWriter, HandleMap};
use syscall::schemev2::NewFdFlags;
use syscall::{Error as SysError, EventFlags, EACCES, EBADF, EEXIST, EINVAL};

pub mod keymap;

use keymap::KeymapKind;

use crate::keymap::KeymapData;

enum Handle {
    Producer,
    Consumer {
        events: EventFlags,
        pending: Vec<u8>,
        /// We return an ESTALE error once to indicate that a handoff to a different graphics driver
        /// is necessary.
        needs_handoff: bool,
        notified: bool,
        vt: usize,
    },
    Display {
        events: EventFlags,
        pending: Vec<VtEvent>,
        notified: bool,
        device: String,
        /// Control of all VT's gets handed over from earlyfb devices to the first non-earlyfb device.
        is_earlyfb: bool,
    },
    Control,
    SchemeRoot,
}

struct InputScheme {
    handles: HandleMap<Handle>,

    next_vt_id: AtomicUsize,

    display: Option<String>,
    vts: BTreeSet<usize>,
    super_key: bool,
    active_vt: Option<usize>,
    active_keymap: KeymapData,
    lshift: bool,
    rshift: bool,

    has_new_events: bool,
}

impl InputScheme {
    fn new() -> Self {
        Self {
            handles: HandleMap::new(),

            next_vt_id: AtomicUsize::new(2), // VT 1 is reserved for the bootlog

            display: None,
            vts: BTreeSet::new(),
            super_key: false,
            active_vt: None,
            // TODO: configurable init?
            active_keymap: KeymapData::new(KeymapKind::US),
            lshift: false,
            rshift: false,
            has_new_events: false,
        }
    }

    fn switch_vt(&mut self, new_active: usize) {
        if let Some(active_vt) = self.active_vt {
            if new_active == active_vt {
                return;
            }
        }

        if !self.vts.contains(&new_active) {
            log::warn!("switch to non-existent VT #{new_active} was requested");
            return;
        }

        log::debug!(
            "switching from VT #{} to VT #{new_active}",
            self.active_vt.unwrap_or(0)
        );

        for handle in self.handles.values_mut() {
            match handle {
                Handle::Display {
                    pending,
                    notified,
                    device,
                    ..
                } => {
                    if self.display.as_deref() == Some(&*device) {
                        pending.push(VtEvent {
                            kind: VtEventKind::Activate,
                            vt: new_active,
                        });
                        *notified = false;
                    }
                }
                _ => continue,
            }
        }

        self.active_vt = Some(new_active);
    }

    fn switch_keymap(&mut self, new_active: usize) {
        if new_active == self.active_keymap.get_kind() as usize {
            return;
        }

        log::debug!(
            "switching from keymap #{} to keymap #{}",
            self.active_keymap.get_kind(),
            KeymapKind::from(new_active),
        );

        self.active_keymap = KeymapData::new(new_active.into());
    }
}

impl SchemeSync for InputScheme {
    fn scheme_root(&mut self) -> syscall::Result<usize> {
        Ok(self.handles.insert(Handle::SchemeRoot))
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> syscall::Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, Handle::SchemeRoot) {
            return Err(SysError::new(EACCES));
        }

        let mut path_parts = path.split('/');

        let command = path_parts.next().ok_or(SysError::new(EINVAL))?;

        let handle_ty = match command {
            "producer" => Handle::Producer,
            "consumer" => {
                let vt = self.next_vt_id.fetch_add(1, Ordering::Relaxed);
                self.vts.insert(vt);

                if self.active_vt.is_none() {
                    self.switch_vt(vt);
                }
                Handle::Consumer {
                    events: EventFlags::empty(),
                    pending: Vec::new(),
                    needs_handoff: false,
                    notified: false,
                    vt,
                }
            }
            "consumer_bootlog" => {
                if !self.vts.insert(1) {
                    return Err(SysError::new(EEXIST));
                }

                self.switch_vt(1);
                Handle::Consumer {
                    events: EventFlags::empty(),
                    pending: Vec::new(),
                    needs_handoff: false,
                    notified: false,
                    vt: 1,
                }
            }
            "handle" | "handle_early" => {
                let display = path_parts.next().ok_or(SysError::new(EINVAL))?;

                let needs_handoff = match command {
                    "handle_early" => self.display.is_none(),
                    "handle" => self.handles.values().all(|handle| {
                        !matches!(
                            handle,
                            Handle::Display {
                                is_earlyfb: false,
                                ..
                            }
                        )
                    }),
                    _ => unreachable!(),
                };

                if needs_handoff {
                    self.has_new_events = true;
                    self.display = Some(display.to_owned());

                    for handle in self.handles.values_mut() {
                        match handle {
                            Handle::Consumer {
                                needs_handoff,
                                notified,
                                ..
                            } => {
                                *needs_handoff = true;
                                *notified = false;
                            }
                            _ => continue,
                        }
                    }
                }

                Handle::Display {
                    events: EventFlags::empty(),
                    pending: if let Some(active_vt) = self.active_vt {
                        vec![VtEvent {
                            kind: VtEventKind::Activate,
                            vt: active_vt,
                        }]
                    } else {
                        vec![]
                    },
                    notified: false,
                    device: display.to_owned(),
                    is_earlyfb: command == "handle_early",
                }
            }
            "control" => Handle::Control,

            _ => {
                log::error!("invalid path '{path}'");
                return Err(SysError::new(EINVAL));
            }
        };

        log::debug!("{path} channel has been opened");

        let fd = self.handles.insert(handle_ty);
        Ok(OpenResult::ThisScheme {
            number: fd,
            flags: NewFdFlags::empty(),
        })
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> syscall::Result<usize> {
        let display = self.display.as_ref().ok_or(SysError::new(EINVAL))?;
        FpathWriter::with(buf, display, |w| {
            let handle = self.handles.get(id)?;

            if let Handle::Consumer { vt, .. } = handle {
                write!(w, "{vt}").unwrap();
                Ok(())
            } else {
                Err(SysError::new(EINVAL))
            }
        })
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        let handle = self.handles.get_mut(id)?;

        match handle {
            Handle::Consumer {
                pending,
                needs_handoff,
                ..
            } => {
                if *needs_handoff {
                    *needs_handoff = false;
                    // Indicates that handoff to a new graphics driver is necessary.
                    return Err(SysError::new(ESTALE));
                }

                let copy = core::cmp::min(pending.len(), buf.len());

                for (i, byte) in pending.drain(..copy).enumerate() {
                    buf[i] = byte;
                }

                Ok(copy)
            }

            Handle::Display { pending, .. } => {
                if buf.len() % size_of::<VtEvent>() == 0 {
                    let copy = core::cmp::min(pending.len(), buf.len() / size_of::<VtEvent>());

                    for (i, event) in pending.drain(..copy).enumerate() {
                        buf[i * size_of::<VtEvent>()..(i + 1) * size_of::<VtEvent>()]
                            .copy_from_slice(&unsafe {
                                transmute::<VtEvent, [u8; size_of::<VtEvent>()]>(event)
                            });
                    }
                    Ok(copy * size_of::<VtEvent>())
                } else {
                    log::error!("display tried to read incorrectly sized event");
                    return Err(SysError::new(EINVAL));
                }
            }

            Handle::Producer => {
                log::error!("producer tried to read");
                return Err(SysError::new(EINVAL));
            }
            Handle::Control => {
                log::error!("control tried to read");
                return Err(SysError::new(EINVAL));
            }
            Handle::SchemeRoot => return Err(SysError::new(EBADF)),
        }
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        self.has_new_events = true;

        let handle = self.handles.get_mut(id)?;

        match handle {
            Handle::Control => {
                if buf.len() != size_of::<ControlEvent>() {
                    log::error!("control tried to write incorrectly sized command");
                    return Err(SysError::new(EINVAL));
                }

                // SAFETY: We have verified the size of the buffer above.
                let cmd = unsafe { &*buf.as_ptr().cast::<ControlEvent>() };

                match cmd.kind {
                    1 => self.switch_vt(cmd.data),
                    2 => self.switch_keymap(cmd.data),
                    k => {
                        log::warn!("unknown control {}", k);
                    }
                }

                return Ok(buf.len());
            }

            Handle::Consumer { .. } => {
                log::error!("consumer tried to write");
                return Err(SysError::new(EINVAL));
            }
            Handle::Display { .. } => {
                log::error!("display tried to write");
                return Err(SysError::new(EINVAL));
            }
            Handle::Producer => {}
            Handle::SchemeRoot => return Err(SysError::new(EBADF)),
        }

        if buf.len() == 1 && buf[0] > 0xf4 {
            return Ok(1);
        }

        let mut events = Cow::from(unsafe {
            core::slice::from_raw_parts(
                buf.as_ptr() as *const Event,
                buf.len() / size_of::<Event>(),
            )
        });

        for i in 0..events.len() {
            let mut new_active_opt = None;
            match events[i].to_option() {
                EventOption::Key(mut key_event) => match key_event.scancode {
                    f @ orbclient::K_F1..=orbclient::K_F10 if self.super_key => {
                        new_active_opt = Some((f - 0x3A) as usize);
                    }
                    orbclient::K_F11 if self.super_key => {
                        new_active_opt = Some(11);
                    }
                    orbclient::K_F12 if self.super_key => {
                        new_active_opt = Some(12);
                    }
                    orbclient::K_SUPER => {
                        self.super_key = key_event.pressed;
                    }
                    orbclient::K_LEFT_SHIFT => {
                        self.lshift = key_event.pressed;
                    }
                    orbclient::K_RIGHT_SHIFT => {
                        self.rshift = key_event.pressed;
                    }

                    key => {
                        let shift = self.lshift | self.rshift;
                        let ev = self.active_keymap.get_char(key, shift);
                        key_event.character = ev;
                        events.to_mut()[i] = key_event.to_event();
                    }
                },

                _ => continue,
            }

            if let Some(new_active) = new_active_opt {
                self.switch_vt(new_active);
            }
        }

        let handle = self.handles.get_mut(id)?;
        assert!(matches!(handle, Handle::Producer));

        let buf = unsafe {
            core::slice::from_raw_parts(
                (events.as_ptr()) as *const u8,
                events.len() * size_of::<Event>(),
            )
        };

        if let Some(active_vt) = self.active_vt {
            for handle in self.handles.values_mut() {
                match handle {
                    Handle::Consumer {
                        pending,
                        notified,
                        vt,
                        ..
                    } => {
                        if *vt != active_vt {
                            continue;
                        }

                        pending.extend_from_slice(buf);
                        *notified = false;
                    }
                    _ => continue,
                }
            }
        }

        Ok(buf.len())
    }

    fn fevent(
        &mut self,
        id: usize,
        flags: syscall::EventFlags,
        _ctx: &CallerCtx,
    ) -> syscall::Result<syscall::EventFlags> {
        match self.handles.get_mut(id)? {
            Handle::Consumer {
                ref mut events,
                ref mut notified,
                ..
            } => {
                *events = flags;
                *notified = false;
                Ok(EventFlags::empty())
            }
            Handle::Display {
                ref mut events,
                ref mut notified,
                ..
            } => {
                *events = flags;
                *notified = false;
                Ok(EventFlags::empty())
            }
            Handle::Producer | Handle::Control => {
                log::error!("producer or control tried to use an event queue");
                Err(SysError::new(EINVAL))
            }
            Handle::SchemeRoot => Err(SysError::new(EBADF)),
        }
    }

    fn on_close(&mut self, id: usize) {
        match self.handles.remove(id).unwrap() {
            Handle::Consumer { vt, .. } => {
                self.vts.remove(&vt);
                if self.active_vt == Some(vt) {
                    if let Some(&new_vt) = self.vts.last() {
                        self.switch_vt(new_vt);
                    } else {
                        self.active_vt = None;
                    }
                }
            }
            _ => {}
        }
    }
}

fn daemon(daemon: daemon::SchemeDaemon) -> anyhow::Result<()> {
    // Create the ":input" scheme.
    let socket_file = Socket::create()?;
    let mut scheme = InputScheme::new();
    let mut handler = Blocking::new(&socket_file, 16);

    let _ = daemon.ready_sync_scheme(&socket_file, &mut scheme);

    loop {
        scheme.has_new_events = false;
        match handler.process_requests_nonblocking(&mut scheme)? {
            ControlFlow::Continue(()) => {}
            ControlFlow::Break(()) => unreachable!("scheme should be non-blocking"),
        }

        if !scheme.has_new_events {
            continue;
        }

        for (id, handle) in scheme.handles.iter_mut() {
            match handle {
                Handle::Consumer {
                    events,
                    pending,
                    needs_handoff,
                    ref mut notified,
                    ..
                } => {
                    if (!*needs_handoff && pending.is_empty())
                        || *notified
                        || !events.contains(EventFlags::EVENT_READ)
                    {
                        continue;
                    }

                    // Notify the consumer that we have some events to read. Yum yum.
                    socket_file.write_response(
                        Response::post_fevent(*id, EventFlags::EVENT_READ.bits()),
                        SignalBehavior::Restart,
                    )?;

                    *notified = true;
                }
                Handle::Display {
                    events,
                    pending,
                    ref mut notified,
                    ..
                } => {
                    if pending.is_empty() || *notified || !events.contains(EventFlags::EVENT_READ) {
                        continue;
                    }

                    // Notify the consumer that we have some events to read. Yum yum.
                    socket_file.write_response(
                        Response::post_fevent(*id, EventFlags::EVENT_READ.bits()),
                        SignalBehavior::Restart,
                    )?;

                    *notified = true;
                }
                _ => {}
            }
        }
    }
}

fn daemon_runner(redox_daemon: daemon::SchemeDaemon) -> ! {
    daemon(redox_daemon).unwrap();
    unreachable!();
}

const HELP: &str = r#"
inputd [-K keymap|-A vt|--keymaps]
   -A vt       : set current virtual display
   -K keymap   : set keyboard mapping
   --keymaps   : list available keyboard mappings
"#;

fn main() {
    let mut args = std::env::args().skip(1);

    if let Some(val) = args.next() {
        // TODO: Get current VT or keymap
        match val.as_ref() {
            // Activates a VT.
            "-A" => {
                let vt = args.next().unwrap().parse::<usize>().unwrap();

                let mut handle =
                    inputd::ControlHandle::new().expect("inputd: failed to open control handle");
                handle
                    .activate_vt(vt)
                    .expect("inputd: failed to activate VT");
            }
            // Activates a keymap.
            "-K" => {
                let arg = if let Some(a) = args.next() {
                    a
                } else {
                    eprintln!("Error: Option -K requires a layout argument.");
                    std::process::exit(1);
                };

                let vt: KeymapKind = arg.to_ascii_lowercase().parse().unwrap_or_else(|_| {
                    eprintln!("inputd: unrecognized keymap code (see: inputd --keymaps)");
                    std::process::exit(1);
                });

                let mut handle =
                    inputd::ControlHandle::new().expect("inputd: failed to open control handle");
                handle
                    .activate_keymap(vt as usize)
                    .expect("inputd: failed to activate keymap");
            }
            // List available keymaps
            "--keymaps" => {
                // TODO: configurable KeymapKind using files
                for key in vec!["dvorak", "us", "gb", "azerty", "bepo", "it"] {
                    println!("{}", key);
                }
            }
            "--help" => {
                println!("{}", HELP);
            }

            _ => panic!("inputd: invalid argument: {}", val),
        }
    } else {
        common::setup_logging(
            "input",
            "inputd",
            "inputd",
            common::output_level(),
            common::file_level(),
        );

        daemon::SchemeDaemon::new(daemon_runner);
    }
}
