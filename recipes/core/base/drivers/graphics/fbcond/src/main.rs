use event::EventQueue;
use inputd::ConsumerHandleEvent;
use libredox::errno::{EAGAIN, EINTR};
use orbclient::Event;
use redox_scheme::{
    scheme::{Op, SchemeResponse, SchemeState, SchemeSync},
    CallerCtx, RequestKind, Response, SignalBehavior, Socket,
};
use std::env;
use syscall::{EOPNOTSUPP, EVENT_READ};

use crate::scheme::{FbconScheme, Handle, VtIndex};

mod display;
mod scheme;
mod text;

fn main() {
    daemon::SchemeDaemon::new(daemon);
}
fn daemon(daemon: daemon::SchemeDaemon) -> ! {
    let vt_ids = env::args()
        .skip(1)
        .map(|arg| arg.parse().expect("invalid vt number"))
        .collect::<Vec<_>>();

    common::setup_logging(
        "graphics",
        "fbcond",
        "fbcond",
        common::output_level(),
        common::file_level(),
    );
    let mut event_queue = EventQueue::new().expect("fbcond: failed to create event queue");

    // FIXME listen for resize events from inputd and handle them

    let mut socket = Socket::nonblock().expect("fbcond: failed to create fbcon scheme");
    event_queue
        .subscribe(
            socket.inner().raw(),
            VtIndex::SCHEMA_SENTINEL,
            event::EventFlags::READ,
        )
        .expect("fbcond: failed to subscribe to scheme events");

    let mut state = SchemeState::new();
    let mut scheme = FbconScheme::new(&vt_ids, &mut event_queue);

    let _ = daemon.ready_sync_scheme(&socket, &mut scheme);

    // This is not possible for now as fbcond needs to open new displays at runtime for graphics
    // driver handoff. In the future inputd may directly pass a handle to the display instead.
    // libredox::call::setrens(0, 0).expect("fbcond: failed to enter null namespace");

    let mut blocked = Vec::new();

    // Handle all events that could have happened before registering with the event queue.
    handle_event(
        &mut socket,
        &mut scheme,
        &mut state,
        &mut blocked,
        VtIndex::SCHEMA_SENTINEL,
    );
    for vt_i in scheme.vts.keys().copied().collect::<Vec<_>>() {
        handle_event(&mut socket, &mut scheme, &mut state, &mut blocked, vt_i);
    }

    for event in event_queue {
        let event = event.expect("fbcond: failed to read event from event queue");
        handle_event(
            &mut socket,
            &mut scheme,
            &mut state,
            &mut blocked,
            event.user_data,
        );
    }

    std::process::exit(0);
}

fn handle_event(
    socket: &mut Socket,
    scheme: &mut FbconScheme,
    state: &mut SchemeState,
    blocked: &mut Vec<(Op, CallerCtx)>,
    event: VtIndex,
) {
    match event {
        VtIndex::SCHEMA_SENTINEL => loop {
            let request = match socket.next_request(SignalBehavior::Restart) {
                Ok(Some(request)) => request,
                Ok(None) => {
                    // Scheme likely got unmounted
                    std::process::exit(0);
                }
                Err(err) if err.errno == EAGAIN => {
                    break;
                }
                Err(err) => panic!("fbcond: failed to read display scheme: {err}"),
            };

            match request.kind() {
                RequestKind::Call(req) => {
                    let caller = req.caller();
                    let mut op = match req.op() {
                        Ok(op) => op,
                        Err(req) => {
                            let _ = socket
                                .write_response(
                                    Response::err(EOPNOTSUPP, req),
                                    SignalBehavior::Restart,
                                )
                                .expect("fbcond: failed to write responses to fbcon scheme");
                            continue;
                        }
                    };
                    match op.handle_sync_dont_consume(&caller, scheme, state) {
                        SchemeResponse::Opened(Err(e)) | SchemeResponse::Regular(Err(e))
                            if libredox::error::Error::from(e).is_wouldblock()
                                && !op.is_explicitly_nonblock() =>
                        {
                            blocked.push((op, caller));
                        }
                        SchemeResponse::Regular(r) => {
                            let _ = socket
                                .write_response(Response::new(r, op), SignalBehavior::Restart)
                                .expect("fbcond: failed to write responses to fbcon scheme");
                        }
                        SchemeResponse::Opened(o) => {
                            let _ = socket
                                .write_response(
                                    Response::open_dup_like(o, op),
                                    SignalBehavior::Restart,
                                )
                                .expect("fbcond: failed to write responses to fbcon scheme");
                        }
                        SchemeResponse::RegularAndNotifyOnDetach(status) => {
                            let _ = socket
                                .write_response(
                                    Response::new_notify_on_detach(status, op),
                                    SignalBehavior::Restart,
                                )
                                .expect("fbcond: failed to write scheme");
                        }
                    }
                }
                RequestKind::OnClose { id } => {
                    scheme.on_close(id);
                }
                RequestKind::Cancellation(cancellation_request) => {
                    if let Some(i) = blocked
                        .iter()
                        .position(|(_op, caller)| caller.id == cancellation_request.id)
                    {
                        let (blocked_req, _) = blocked.remove(i);
                        let resp = Response::err(EINTR, blocked_req);
                        socket
                            .write_response(resp, SignalBehavior::Restart)
                            .expect("vesad: failed to write display scheme");
                    }
                }
                _ => {}
            }
        },
        vt_i => {
            let vt = scheme.vts.get_mut(&vt_i).unwrap();

            let mut events = [Event::new(); 16];
            loop {
                match vt
                    .display
                    .input_handle
                    .read_events(&mut events)
                    .expect("fbcond: Error while reading events")
                {
                    ConsumerHandleEvent::Events(&[]) => break,

                    ConsumerHandleEvent::Events(events) => {
                        for event in events {
                            vt.input(event)
                        }
                    }
                    ConsumerHandleEvent::Handoff => vt.handle_handoff(),
                }
            }
        }
    }

    // If there are blocked readers, try to handle them.
    {
        let mut i = 0;
        while i < blocked.len() {
            let (op, caller) = blocked
                .get_mut(i)
                .expect("vesad: Failed to get blocked request");
            let resp = match op.handle_sync_dont_consume(&caller, scheme, state) {
                SchemeResponse::Opened(Err(e)) | SchemeResponse::Regular(Err(e))
                    if libredox::error::Error::from(e).is_wouldblock()
                        && !op.is_explicitly_nonblock() =>
                {
                    i += 1;
                    continue;
                }
                SchemeResponse::Regular(r) => {
                    let (op, _) = blocked.remove(i);
                    Response::new(r, op)
                }
                SchemeResponse::Opened(o) => {
                    let (op, _) = blocked.remove(i);
                    Response::open_dup_like(o, op)
                }
                SchemeResponse::RegularAndNotifyOnDetach(status) => {
                    let (op, _) = blocked.remove(i);
                    Response::new_notify_on_detach(status, op)
                }
            };
            let _ = socket
                .write_response(resp, SignalBehavior::Restart)
                .expect("vesad: failed to write display scheme");
        }
    }

    for (handle_id, handle) in scheme.handles.iter_mut() {
        let handle = match handle {
            Handle::SchemeRoot => continue,
            Handle::Vt(handle) => handle,
        };

        if !handle.events.contains(EVENT_READ) {
            continue;
        }

        let can_read = scheme
            .vts
            .get(&handle.vt_i)
            .map_or(false, |console| console.can_read());

        if can_read {
            if !handle.notified_read {
                handle.notified_read = true;
                let response = Response::post_fevent(*handle_id, EVENT_READ.bits());
                socket
                    .write_response(response, SignalBehavior::Restart)
                    .expect("fbcond: failed to write display event");
            }
        } else {
            handle.notified_read = false;
        }
    }
}
