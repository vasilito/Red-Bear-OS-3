use std::collections::VecDeque;
use std::ops::ControlFlow;

use libredox::error::Error as LError;

use syscall::Result;
use syscall::error::{self as errno, Error};

use redox_scheme::scheme::{SchemeState, SchemeSync};
use redox_scheme::{Request, RequestKind, Response, SignalBehavior, Socket};

pub struct Blocking<'sock> {
    // TODO: VecDeque for both when it implements spare_capacity
    requests_read: Vec<Request>,
    responses_to_write: VecDeque<Response>,

    socket: &'sock Socket,
    state: SchemeState,
}

impl<'sock> Blocking<'sock> {
    pub fn new(socket: &'sock Socket, queue_size: usize) -> Self {
        Self {
            requests_read: Vec::with_capacity(queue_size),
            responses_to_write: VecDeque::with_capacity(queue_size),
            socket,
            state: SchemeState::new(),
        }
    }

    pub fn process_requests_nonblocking(
        &mut self,
        scheme: &mut impl SchemeSync,
    ) -> Result<ControlFlow<()>> {
        assert!(self.requests_read.is_empty());
        assert!(self.responses_to_write.is_empty());

        match self
            .socket
            .read_requests(&mut self.requests_read, SignalBehavior::Interrupt)
        {
            Ok(()) if self.requests_read.is_empty() => {
                unreachable!("blocking scheme read failed to read anything");
            }
            Ok(()) => {}
            Err(Error {
                errno: errno::EINTR | errno::EWOULDBLOCK | errno::EAGAIN,
            }) => return Ok(ControlFlow::Break(())),
            Err(err) => return Err(err),
        }

        for request in self.requests_read.drain(..) {
            match request.kind() {
                RequestKind::Call(req) => {
                    let response = req.handle_sync(scheme, &mut self.state);
                    self.responses_to_write.push_back(response);
                }
                RequestKind::Cancellation(_req) => {}
                RequestKind::OnClose { id } => {
                    // TODO: state.on_close()
                    scheme.on_close(id);
                }
                RequestKind::SendFd(sendfd_request) => {
                    let result = scheme.on_sendfd(&sendfd_request);
                    let response = Response::new(result, sendfd_request);
                    self.responses_to_write.push_back(response);
                }
                RequestKind::RecvFd(recvfd_request) => {
                    let result = scheme.on_recvfd(&recvfd_request);
                    let response = Response::open_dup_like(result, recvfd_request);
                    self.responses_to_write.push_back(response);
                }
                _ => {}
            }
        }

        match self
            .socket
            .write_responses(&mut self.responses_to_write, SignalBehavior::Restart)
        {
            Ok(()) if !self.responses_to_write.is_empty() => {
                panic!("failed to write all scheme responses");
            }
            Ok(()) => Ok(ControlFlow::Continue(())),
            Err(Error {
                errno: errno::EINTR | errno::EWOULDBLOCK | errno::EAGAIN,
            }) => {
                panic!("scheme response writing should always block");
            }
            Err(err) => return Err(LError::from(err).into()),
        }
    }

    pub fn process_requests_blocking(mut self, mut scheme: impl SchemeSync) -> Result<!> {
        loop {
            match self.process_requests_nonblocking(&mut scheme)? {
                ControlFlow::Continue(()) => {}
                ControlFlow::Break(()) => {
                    panic!("process_requests_blocking should not be used on non-blocking schemes");
                }
            }
        }
    }
}
