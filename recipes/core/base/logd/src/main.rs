use redox_scheme::Socket;
use scheme_utils::Blocking;

use crate::scheme::LogScheme;

mod scheme;

fn daemon(daemon: daemon::SchemeDaemon) -> ! {
    let socket = Socket::create().expect("logd: failed to create log scheme");

    let mut scheme = LogScheme::new(&socket);
    let handler = Blocking::new(&socket, 16);

    let _ = daemon.ready_sync_scheme(&socket, &mut scheme);

    libredox::call::setrens(0, 0).expect("logd: failed to enter null namespace");

    handler
        .process_requests_blocking(scheme)
        .expect("logd: failed to process requests");
}

fn main() {
    daemon::SchemeDaemon::new(daemon);
}
