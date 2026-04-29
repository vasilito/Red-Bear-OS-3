use std::collections::{HashMap, VecDeque};
use std::ops::ControlFlow;

use libredox::error::Error as LError;

use syscall::Result;
use syscall::error::{self as errno, ECANCELED, EIO, EOPNOTSUPP, Error};

use redox_scheme::scheme::{Op, SchemeResponse, SchemeState, SchemeSync};
use redox_scheme::{CallerCtx, Id, Request, RequestKind, Response, SignalBehavior, Socket};

pub struct ReadinessBased<'sock> {
    // TODO: VecDeque for both when it implements spare_capacity
    requests_read: Vec<Request>,
    responses_to_write: VecDeque<Response>,

    states: HashMap<Id, (CallerCtx, Op)>,
    ready_queue: VecDeque<Id>,

    socket: &'sock Socket,
    state: SchemeState,
}
impl<'sock> ReadinessBased<'sock> {
    pub fn new(socket: &'sock Socket, queue_size: usize) -> Self {
        Self {
            requests_read: Vec::with_capacity(queue_size),
            responses_to_write: VecDeque::with_capacity(queue_size),
            states: HashMap::new(),
            socket,
            ready_queue: VecDeque::new(),
            state: SchemeState::new(),
        }
    }
    pub fn read_and_process_requests(&mut self, scheme: &mut impl SchemeSync) -> Result<()> {
        assert!(self.requests_read.is_empty());

        match self
            .socket
            .read_requests(&mut self.requests_read, SignalBehavior::Interrupt)
        {
            Ok(()) if self.requests_read.is_empty() => {
                unreachable!("blocking scheme read failed to read anything");
            }
            Ok(())
            | Err(Error {
                errno: errno::EINTR | errno::EWOULDBLOCK | errno::EAGAIN,
            }) => {}
            Err(err) => return Err(err),
        }

        for request in self.requests_read.drain(..) {
            let req = match request.kind() {
                RequestKind::Call(c) => c,
                RequestKind::Cancellation(req) => {
                    if let Some((_caller, op)) = self.states.remove(&req.id) {
                        self.responses_to_write
                            .push_back(Response::err(ECANCELED, op));
                    }
                    continue;
                }
                RequestKind::OnClose { id } => {
                    // TODO: state.on_close()
                    scheme.on_close(id);
                    continue;
                }
                RequestKind::SendFd(sendfd_request) => {
                    let result = scheme.on_sendfd(&sendfd_request);
                    let response = Response::new(result, sendfd_request);
                    self.responses_to_write.push_back(response);
                    continue;
                }
                RequestKind::RecvFd(recvfd_request) => {
                    let result = scheme.on_recvfd(&recvfd_request);
                    let caller = recvfd_request.caller();

                    if let Err(Error {
                        errno: errno::EWOULDBLOCK,
                    }) = result
                    {
                        self.states.insert(caller.id, (caller, recvfd_request.op()));
                        continue;
                    }
                    let response = Response::open_dup_like(result, recvfd_request);
                    self.responses_to_write.push_back(response);
                    continue;
                }
                _ => continue,
            };
            let caller = req.caller();
            let mut op = match req.op() {
                Ok(op) => op,
                Err(req) => {
                    self.responses_to_write
                        .push_back(Response::err(EOPNOTSUPP, req));
                    continue;
                }
            };
            let resp = match op.handle_sync_dont_consume(&caller, scheme, &mut self.state) {
                SchemeResponse::Opened(Err(Error {
                    errno: errno::EWOULDBLOCK,
                }))
                | SchemeResponse::Regular(Err(Error {
                    errno: errno::EWOULDBLOCK,
                })) if !op.is_explicitly_nonblock() => {
                    self.states.insert(caller.id, (caller, op));
                    continue;
                }
                SchemeResponse::Regular(r) => Response::new(r, op),
                SchemeResponse::RegularAndNotifyOnDetach(status) => {
                    Response::new_notify_on_detach(status, op)
                }
                SchemeResponse::Opened(o) => Response::open_dup_like(o, op),
            };
            self.responses_to_write.push_back(resp);
        }

        Ok(())
    }
    // TODO: Doesn't scale. Instead, provide an API for some form of queue.
    // TODO: panic if id isn't present?
    pub fn poll_request(&mut self, id: Id, scheme: &mut impl SchemeSync) -> Result<bool> {
        Ok(
            match Self::poll_request_inner(id, scheme, &mut self.state, &mut self.states)? {
                ControlFlow::Continue((caller, op)) => {
                    self.states.insert(id, (caller, op));
                    false
                }
                ControlFlow::Break(resp) => {
                    self.responses_to_write.push_back(resp);
                    true
                }
            },
        )
    }

    fn poll_request_inner(
        id: Id,
        scheme: &mut impl SchemeSync,
        state: &mut SchemeState,
        states: &mut HashMap<Id, (CallerCtx, Op)>,
    ) -> Result<ControlFlow<Response, (CallerCtx, Op)>> {
        let (caller, mut op) = states.remove(&id).ok_or(Error::new(EIO))?;
        let resp = match op.handle_sync_dont_consume(&caller, scheme, state) {
            SchemeResponse::Opened(Err(Error {
                errno: errno::EWOULDBLOCK,
            }))
            | SchemeResponse::Regular(Err(Error {
                errno: errno::EWOULDBLOCK,
            })) if !op.is_explicitly_nonblock() => {
                return Ok(ControlFlow::Continue((caller, op)));
            }
            SchemeResponse::Regular(r) => Response::new(r, op),
            SchemeResponse::Opened(o) => Response::open_dup_like(o, op),
            SchemeResponse::RegularAndNotifyOnDetach(status) => {
                Response::new_notify_on_detach(status, op)
            }
        };
        Ok(ControlFlow::Break(resp))
    }
    pub fn poll_ready_requests(&mut self, scheme: &mut impl SchemeSync) -> Result<()> {
        for id in self.ready_queue.drain(..) {
            match Self::poll_request_inner(id, scheme, &mut self.state, &mut self.states)? {
                ControlFlow::Break(resp) => {
                    self.responses_to_write.push_back(resp);
                }
                ControlFlow::Continue((caller, op)) => {
                    self.states.insert(id, (caller, op));
                }
            }
        }
        Ok(())
    }
    pub fn poll_all_requests(&mut self, scheme: &mut impl SchemeSync) -> Result<()> {
        // TODO: implement waker-like API
        self.ready_queue.clear();
        self.ready_queue.extend(self.states.keys().copied());
        self.poll_ready_requests(scheme)
    }
    pub fn write_responses(&mut self) -> Result<()> {
        match self
            .socket
            .write_responses(&mut self.responses_to_write, SignalBehavior::Restart)
        {
            Ok(()) if !self.responses_to_write.is_empty() => {
                panic!("failed to write all scheme responses");
            }
            Ok(()) => Ok(()),
            Err(Error {
                errno: errno::EINTR | errno::EWOULDBLOCK | errno::EAGAIN,
            }) => {
                panic!("scheme response writing should always block");
            }
            Err(err) => return Err(LError::from(err).into()),
        }
    }
}
