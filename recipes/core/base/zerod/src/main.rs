use redox_scheme::Socket;

use scheme::ZeroScheme;
use scheme_utils::Blocking;

mod scheme;

enum Ty {
    Null,
    Zero,
}

fn main() {
    daemon::SchemeDaemon::new(daemon);
}

fn daemon(daemon: daemon::SchemeDaemon) -> ! {
    let ty = match &*std::env::args().nth(1).unwrap() {
        "null" => Ty::Null,
        "zero" => Ty::Zero,
        _ => panic!("needs to be called with either null or zero as argument"),
    };

    let socket = Socket::create().expect("zerod: failed to create zero scheme");
    let mut zero_scheme = ZeroScheme(ty);
    let zero_handler = Blocking::new(&socket, 16);

    let _ = daemon.ready_sync_scheme(&socket, &mut zero_scheme);

    libredox::call::setrens(0, 0).expect("zerod: failed to enter null namespace");

    zero_handler
        .process_requests_blocking(zero_scheme)
        .expect("zerod: failed to process events from zero scheme");
}
