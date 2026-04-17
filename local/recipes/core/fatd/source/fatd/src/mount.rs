use std::io::{Read, Seek, Write};
use std::sync::atomic::Ordering;

use fscommon::BufStream;
use redox_scheme::{
    RequestKind, Response, SignalBehavior, Socket,
    scheme::{SchemeState, SchemeSync, register_sync_scheme},
};

use crate::{IS_UMT, scheme::FatScheme};

pub fn mount<D, T, F>(
    filesystem: fatfs::FileSystem<BufStream<D>>,
    mountpoint: &str,
    callback: F,
) -> syscall::error::Result<T>
where
    D: Read + Write + Seek,
    F: FnOnce(&str) -> T,
{
    let socket = Socket::create()?;

    let scheme_name = mountpoint.to_string();
    let mounted_path = format!("/scheme/{mountpoint}");

    let mut state = SchemeState::new();
    let mut scheme = FatScheme::new(scheme_name, mounted_path.clone(), filesystem);

    register_sync_scheme(&socket, mountpoint, &mut scheme)?;

    let result = callback(&mounted_path);

    while IS_UMT.load(Ordering::SeqCst) == 0 {
        let request = match socket.next_request(SignalBehavior::Restart)? {
            None => break,
            Some(request) => match request.kind() {
                RequestKind::Call(request) => request,
                RequestKind::SendFd(sendfd_request) => {
                    let response = Response::new(scheme.on_sendfd(&sendfd_request), sendfd_request);
                    if !socket.write_response(response, SignalBehavior::Restart)? {
                        break;
                    }
                    continue;
                }
                RequestKind::OnClose { id } => {
                    scheme.on_close(id);
                    state.on_close(id);
                    continue;
                }
                RequestKind::OnDetach { id, pid } => {
                    let Ok(inode) = scheme.inode(id) else {
                        log::warn!("OnDetach received unknown handle id={id}");
                        continue;
                    };
                    state.on_detach(id, inode, pid);
                    continue;
                }
                _ => continue,
            },
        };

        let response = request.handle_sync(&mut scheme, &mut state);
        if !socket.write_response(response, SignalBehavior::Restart)? {
            break;
        }
    }

    scheme.cleanup()?;
    Ok(result)
}
