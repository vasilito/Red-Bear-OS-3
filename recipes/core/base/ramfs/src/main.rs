use std::env;

mod filesystem;
mod scheme;

use scheme_utils::Blocking;

use self::scheme::Scheme;

fn main() {
    daemon::SchemeDaemon::new(daemon);
}

fn daemon(daemon: daemon::SchemeDaemon) -> ! {
    let scheme_name = env::args().nth(1).expect("Usage:\n\tramfs SCHEME_NAME");

    let socket = redox_scheme::Socket::create().expect("ramfs: failed to create socket");

    let mut scheme = Scheme::new(scheme_name.clone()).expect("ramfs: failed to initialize scheme");
    let handler = Blocking::new(&socket, 16);

    let _ = daemon.ready_sync_scheme(&socket, &mut scheme);

    libredox::call::setrens(0, 0).expect("ramfs: failed to enter null namespace");

    handler
        .process_requests_blocking(scheme)
        .expect("ramfs: failed to process events from zero scheme");
}
