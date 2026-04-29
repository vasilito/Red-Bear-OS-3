use event::{EventFlags, EventQueue};
use redox_scheme::scheme::register_sync_scheme;
use redox_scheme::Socket;
use scheme_utils::ReadinessBased;

mod chan;
mod shm;
mod uds;

use self::chan::ChanScheme;
use self::shm::ShmScheme;
use self::uds::dgram::UdsDgramScheme;
use self::uds::stream::UdsStreamScheme;

fn main() {
    daemon::Daemon::new(daemon_runner);
}

fn daemon_runner(daemon: daemon::Daemon) -> ! {
    // TODO: Better error handling
    match inner(daemon) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            println!("ipcd failed: {error}");
            std::process::exit(1);
        }
    }
}

fn inner(daemon: daemon::Daemon) -> anyhow::Result<()> {
    event::user_data! {
        enum EventSource {
            ChanSocket,
            ShmSocket,
            UdsStreamSocket,
            UdsDgramSocket,
        }
    }

    // Prepare chan scheme
    let chan_socket =
        Socket::nonblock().map_err(|e| anyhow::anyhow!("failed to create chan scheme: {e}"))?;
    let mut chan = ChanScheme::new(&chan_socket);
    let mut chan_handler = ReadinessBased::new(&chan_socket, 16);

    // Prepare shm scheme
    let shm_socket =
        Socket::nonblock().map_err(|e| anyhow::anyhow!("failed to create shm socket: {e}"))?;
    let mut shm = ShmScheme::new();
    let mut shm_handler = ReadinessBased::new(&shm_socket, 16);

    // Prepare uds stream scheme
    let uds_stream_socket = Socket::nonblock()
        .map_err(|e| anyhow::anyhow!("failed to create uds stream scheme: {e}"))?;
    let mut uds_stream = UdsStreamScheme::new(&uds_stream_socket)
        .map_err(|e| anyhow::anyhow!("failed to create uds stream scheme: {e}"))?;
    let mut uds_stream_handler = ReadinessBased::new(&uds_stream_socket, 16);

    // Prepare uds dgram scheme
    let uds_dgram_socket = Socket::nonblock()
        .map_err(|e| anyhow::anyhow!("failed to create uds dgram scheme: {e}"))?;
    let mut uds_dgram = UdsDgramScheme::new(&uds_dgram_socket)
        .map_err(|e| anyhow::anyhow!("failed to create uds dgram scheme: {e}"))?;
    let mut uds_dgram_handler = ReadinessBased::new(&uds_dgram_socket, 16);

    register_sync_scheme(&chan_socket, "chan", &mut chan)
        .map_err(|e| anyhow::anyhow!("failed to register chan scheme: {e}"))?;
    register_sync_scheme(&shm_socket, "shm", &mut shm)
        .map_err(|e| anyhow::anyhow!("failed to register shm scheme: {e}"))?;
    register_sync_scheme(&uds_stream_socket, "uds_stream", &mut uds_stream)
        .map_err(|e| anyhow::anyhow!("failed to register uds stream scheme: {e}"))?;
    register_sync_scheme(&uds_dgram_socket, "uds_dgram", &mut uds_dgram)
        .map_err(|e| anyhow::anyhow!("failed to register uds dgram scheme: {e}"))?;

    daemon.ready();

    // Create event listener for both files
    let event_queue = EventQueue::<EventSource>::new()
        .map_err(|e| anyhow::anyhow!("failed to create event queue: {e}"))?;
    event_queue
        .subscribe(
            chan_socket.inner().raw(),
            EventSource::ChanSocket,
            EventFlags::READ,
        )
        .map_err(|e| anyhow::anyhow!("failed to subscribe chan socket: {e}"))?;
    event_queue
        .subscribe(
            shm_socket.inner().raw(),
            EventSource::ShmSocket,
            EventFlags::READ,
        )
        .map_err(|e| anyhow::anyhow!("failed to subscribe shm socket: {e}"))?;
    event_queue
        .subscribe(
            uds_stream_socket.inner().raw(),
            EventSource::UdsStreamSocket,
            EventFlags::READ,
        )
        .map_err(|e| anyhow::anyhow!("failed to subscribe uds stream socket: {e}"))?;
    event_queue
        .subscribe(
            uds_dgram_socket.inner().raw(),
            EventSource::UdsDgramSocket,
            EventFlags::READ,
        )
        .map_err(|e| anyhow::anyhow!("failed to subscribe uds dgram socket: {e}"))?;

    libredox::call::setrens(0, 0)?;

    loop {
        let event = event_queue
            .next_event()
            .map_err(|e| anyhow::anyhow!("error occured in event queue: {e}"))?;

        match event.user_data {
            EventSource::ChanSocket => {
                // Channel scheme
                chan_handler.read_and_process_requests(&mut chan)?;
                chan_handler
                    .poll_all_requests(&mut chan)
                    .map_err(|e| anyhow::anyhow!("error occured in poll_all_requests: {e}"))?;
                chan_handler.write_responses()?;
            }
            EventSource::ShmSocket => {
                // Shared memory scheme
                shm_handler.read_and_process_requests(&mut shm)?;
                // shm is not a blocking scheme
                shm_handler.write_responses()?;
            }
            EventSource::UdsStreamSocket => {
                // Unix Domain Socket Stream scheme
                uds_stream_handler.read_and_process_requests(&mut uds_stream)?;
                uds_stream_handler
                    .poll_all_requests(&mut uds_stream)
                    .map_err(|e| anyhow::anyhow!("error occured in poll_all_requests: {e}"))?;
                uds_stream_handler.write_responses()?;
            }
            EventSource::UdsDgramSocket => {
                // Unix Domain Socket Dgram scheme
                uds_dgram_handler.read_and_process_requests(&mut uds_dgram)?;
                uds_dgram_handler
                    .poll_all_requests(&mut uds_dgram)
                    .map_err(|e| anyhow::anyhow!("error occured in poll_all_requests: {e}"))?;
                uds_dgram_handler.write_responses()?;
            }
        }
    }
}
