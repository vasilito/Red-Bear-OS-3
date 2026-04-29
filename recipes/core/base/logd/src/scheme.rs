use std::collections::{BTreeMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::mem;
use std::os::fd::{FromRawFd, RawFd};
use std::sync::mpsc::{self, Sender};

use redox_scheme::scheme::SchemeSync;
use redox_scheme::{CallerCtx, OpenResult, SendFdRequest, Socket};
use scheme_utils::{FpathWriter, HandleMap};
use syscall::error::*;
use syscall::schemev2::NewFdFlags;

pub enum LogHandle {
    Log {
        context: Box<str>,
        bufs: BTreeMap<usize, Vec<u8>>,
    },
    AddSink,
    SchemeRoot,
}

pub struct LogScheme<'sock> {
    socket: &'sock Socket,
    kernel_debug: File,
    output_tx: Sender<OutputCmd>,
    handles: HandleMap<LogHandle>,
}

enum OutputCmd {
    Log(Vec<u8>),
    AddSink(usize),
}

impl<'sock> LogScheme<'sock> {
    pub fn new(socket: &'sock Socket) -> Self {
        let kernel_debug = OpenOptions::new()
            .write(true)
            .open("/scheme/debug")
            .unwrap();

        let mut kernel_sys_log = std::fs::File::open("/scheme/sys/log").unwrap();

        let (output_tx, output_rx) = mpsc::channel::<OutputCmd>();

        std::thread::spawn(move || {
            let mut files: Vec<File> = vec![];
            let mut logs = VecDeque::new();
            for cmd in output_rx {
                match cmd {
                    OutputCmd::Log(line) => {
                        for file in &mut files {
                            let _ = file.write(&line);
                            let _ = file.flush();
                        }
                        logs.push_back(line);
                        // Keep a limited amount of logs for backfilling to bound memory usage
                        while logs.len() > 1000 {
                            logs.pop_front();
                        }
                    }
                    OutputCmd::AddSink(log_fd) => {
                        let mut file = unsafe { File::from_raw_fd(log_fd as RawFd) };
                        for line in &logs {
                            let _ = file.write(line);
                            let _ = file.flush();
                        }

                        files.push(file)
                    }
                }
            }
        });

        let output_tx2 = output_tx.clone();
        std::thread::spawn(move || {
            let mut handle_buf = vec![];
            let mut buf = [0; 4096];
            buf[.."kernel: ".len()].copy_from_slice(b"kernel: ");
            loop {
                let n = kernel_sys_log.read(&mut buf["kernel: ".len()..]).unwrap();
                if n == 0 {
                    // FIXME currently possible as /scheme/log/kernel presents a snapshot of the log queue
                    break;
                }
                Self::write_logs(&output_tx2, &mut handle_buf, "kernel", &buf, None);
            }
        });

        LogScheme {
            socket,
            kernel_debug,
            output_tx,
            handles: HandleMap::new(),
        }
    }

    fn write_logs(
        output_tx: &Sender<OutputCmd>,
        handle_buf: &mut Vec<u8>,
        context: &str,
        buf: &[u8],
        mut kernel_debug: Option<&mut File>,
    ) {
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];

            if handle_buf.is_empty() && !context.is_empty() {
                handle_buf.extend_from_slice(context.as_bytes());
                handle_buf.extend_from_slice(b": ");
            }

            handle_buf.push(b);

            if b == b'\n' {
                if let Some(kernel_debug) = kernel_debug.as_mut() {
                    // Writing to the kernel debug log never blocks
                    let _ = kernel_debug.write(handle_buf);
                    let _ = kernel_debug.flush();
                }

                output_tx
                    .send(OutputCmd::Log(mem::take(handle_buf)))
                    .unwrap();
            }

            i += 1;
        }
    }
}

impl<'sock> SchemeSync for LogScheme<'sock> {
    fn scheme_root(&mut self) -> Result<usize> {
        Ok(self.handles.insert(LogHandle::SchemeRoot))
    }
    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<OpenResult> {
        if !matches!(self.handles.get(dirfd)?, LogHandle::SchemeRoot) {
            return Err(Error::new(EACCES));
        }

        let id = if path == "add_sink" {
            self.handles.insert(LogHandle::AddSink)
        } else {
            self.handles.insert(LogHandle::Log {
                context: path.to_string().into_boxed_str(),
                bufs: BTreeMap::new(),
            })
        };

        Ok(OpenResult::ThisScheme {
            number: id,
            flags: NewFdFlags::empty(),
        })
    }

    fn read(
        &mut self,
        id: usize,
        _buf: &mut [u8],
        _offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> Result<usize> {
        let _handle = self.handles.get(id)?;

        // TODO

        Ok(0)
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        _offset: u64,
        _flags: u32,
        ctx: &CallerCtx,
    ) -> Result<usize> {
        let (context, bufs) = match self.handles.get_mut(id)? {
            LogHandle::Log { context, bufs } => (context, bufs),
            LogHandle::SchemeRoot | LogHandle::AddSink => return Err(Error::new(EBADF)),
        };

        let handle_buf = bufs.entry(ctx.pid).or_insert_with(|| Vec::new());

        Self::write_logs(
            &self.output_tx,
            handle_buf,
            context,
            buf,
            Some(&mut self.kernel_debug),
        );

        Ok(buf.len())
    }

    fn on_sendfd(&mut self, sendfd_request: &SendFdRequest) -> Result<usize> {
        let id = sendfd_request.id();

        if !matches!(self.handles.get(id)?, LogHandle::AddSink) {
            return Err(Error::new(EBADF));
        }

        let mut new_fd = usize::MAX;
        if let Err(e) = sendfd_request.obtain_fd(
            &self.socket,
            syscall::FobtainFdFlags::CLOEXEC,
            std::slice::from_mut(&mut new_fd),
        ) {
            return Err(e);
        }
        self.output_tx.send(OutputCmd::AddSink(new_fd)).unwrap();

        Ok(1)
    }

    fn fcntl(&mut self, id: usize, _cmd: usize, _arg: usize, _ctx: &CallerCtx) -> Result<usize> {
        let _handle = self.handles.get(id)?;

        Ok(0)
    }

    fn fpath(&mut self, id: usize, buf: &mut [u8], _ctx: &CallerCtx) -> Result<usize> {
        FpathWriter::with(buf, "log", |w| {
            w.push_str(match self.handles.get(id)? {
                LogHandle::Log { context, .. } => context,
                LogHandle::AddSink => "add_sink",
                LogHandle::SchemeRoot => return Err(Error::new(EBADF)),
            });
            Ok(())
        })
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> Result<()> {
        let _handle = self.handles.get(id)?;

        //TODO: flush remaining data?

        Ok(())
    }

    fn on_close(&mut self, id: usize) {
        self.handles.remove(id);
    }
}
