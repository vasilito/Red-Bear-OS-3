use libredox::protocol::ProcMeta;
use std::fmt::Debug;
use std::{cmp, convert::TryInto, mem};
use syscall::{error::*, Error};

pub mod dgram;
pub mod stream;

// TODO: Remove this when Rust libc crate is updated to include SCM_CREDENTIALS
const SCM_CREDENTIALS: i32 = 2;

const MAX_DGRAM_MSG_LEN: usize = 65536;
const MIN_RECV_MSG_LEN: usize = mem::size_of::<usize>() * 2; // name_len, payload_len,
const CMSG_HEADER_LEN_IN_STREAM: usize = CMSG_LEVEL_SIZE + CMSG_TYPE_SIZE + CMSG_DATA_LEN_SIZE;
const CMSG_LEVEL_SIZE: usize = mem::size_of::<i32>();
const CMSG_TYPE_SIZE: usize = mem::size_of::<i32>();
const CMSG_DATA_LEN_SIZE: usize = mem::size_of::<usize>();
const PID_SIZE: usize = mem::size_of::<i32>();
const UID_SIZE: usize = mem::size_of::<i32>();
const GID_SIZE: usize = mem::size_of::<i32>();

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Credential {
    pid: i32,
    uid: i32,
    gid: i32,
}
impl Credential {
    fn new(pid: i32, uid: i32, gid: i32) -> Self {
        Self { pid, uid, gid }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct AncillaryData {
    cred: Credential,
    num_fds: usize,
    name: Option<String>,
}
impl AncillaryData {
    fn new(cred: Credential, name: Option<String>) -> Self {
        Self {
            cred,
            name,
            ..Default::default()
        }
    }
}

struct AncillaryDataHeader {
    level: i32,
    c_type: i32,
    data_len: usize,
}
impl AncillaryDataHeader {
    fn from_stream(stream: &[u8]) -> Result<Option<Self>> {
        let mut cursor = 0;
        if cursor + CMSG_HEADER_LEN_IN_STREAM > stream.len() {
            return Ok(None);
        }

        // cmsg entry format: [level(i32)][type(i32)][data_len(usize)][data]
        let cmsg_level = read_num::<i32>(&stream[cursor..])?;
        cursor += mem::size_of::<i32>();

        let cmsg_type = read_num::<i32>(&stream[cursor..])?;
        cursor += mem::size_of::<i32>();

        let cmsg_data_len = read_num::<usize>(&stream[cursor..])?;

        Ok(Some(Self {
            level: cmsg_level,
            c_type: cmsg_type,
            data_len: cmsg_data_len,
        }))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct DataPacket {
    payload: Vec<u8>,
    ancillary_data: AncillaryData,
    // Only for stream packets
    read_offset: usize,
    ancillary_taken: bool,
}

impl DataPacket {
    fn new(payload: Vec<u8>, ancillary_data: AncillaryData) -> Self {
        Self {
            payload,
            ancillary_data,
            read_offset: 0,
            ancillary_taken: false,
        }
    }

    fn len(&self) -> usize {
        self.payload.len()
    }

    fn from_stream(stream: &[u8], name: Option<String>, cred: Credential) -> Result<Self> {
        let mut cursor: usize = 0;
        let payload_len = read_num::<usize>(&stream[cursor..])?;
        cursor += mem::size_of::<usize>();
        let payload = stream
            .get(cursor..cursor + payload_len)
            .ok_or_else(|| {
                eprintln!("Message::from_stream: Stream too short for payload. Expected len {}, actual remaining len {}", payload_len, stream.len() - cursor);
                Error::new(EINVAL)
            })?;
        cursor += payload_len;

        // Create a new message with the payload and credentials
        let mut message = Self::new(payload.to_vec(), AncillaryData::new(cred, name));

        while let Some(cmsg_header) = AncillaryDataHeader::from_stream(&stream[cursor..])? {
            cursor += CMSG_HEADER_LEN_IN_STREAM;
            let data_stream = stream
                .get(cursor..cursor + cmsg_header.data_len)
                .ok_or_else(|| {
                    eprintln!("Message::from_stream: Stream too short for ancillary data. Expected len {}, actual remaining len {}", cmsg_header.data_len, stream.len() - cursor);
                    Error::new(EINVAL)
                })?;

            match (cmsg_header.level, cmsg_header.c_type) {
                (libc::SOL_SOCKET, libc::SCM_RIGHTS) => {
                    // Handle file descriptor passing
                    let num_fds = read_num::<usize>(data_stream)?;
                    message.ancillary_data.num_fds += num_fds;
                }
                (libc::SOL_SOCKET, SCM_CREDENTIALS) => {}
                _ => {
                    eprintln!(
                        "Message::from_stream: Unsupported cmsg type received. level: {}, type: {}",
                        cmsg_header.level, cmsg_header.c_type
                    );
                    return Err(Error::new(EOPNOTSUPP));
                }
            }
            cursor += cmsg_header.data_len;
        }
        Ok(message)
    }
}

trait NumFromBytes: Sized + Debug {
    fn from_le_bytes_slice(buffer: &[u8]) -> Result<Self, Error>;
}

macro_rules! num_from_bytes_impl {
    ($($t:ty),*) => {
        $(
            impl NumFromBytes for $t {
                fn from_le_bytes_slice(buffer: &[u8]) -> Result<Self, Error> {
                    let size = mem::size_of::<Self>();
                    let buffer_slice = buffer.get(..size).and_then(|s| s.try_into().ok());

                    if let Some(slice) = buffer_slice {
                        Ok(Self::from_le_bytes(slice))
                    } else {
                        eprintln!(
                            "read_num: buffer is too short to read num of size {} (buffer len: {})",
                            size, buffer.len()
                        );
                        Err(Error::new(EINVAL))
                    }
                }
            }
        )*
    };
}

num_from_bytes_impl!(i32, u32, u64, usize);

fn read_num<T>(buffer: &[u8]) -> Result<T, Error>
where
    T: NumFromBytes,
{
    T::from_le_bytes_slice(buffer)
}

fn get_uid_gid_from_pid(cap_fd: usize, target_pid: usize) -> Result<(u32, u32, u32)> {
    let mut buffer = [0u8; mem::size_of::<ProcMeta>()];
    let _ = libredox::call::get_proc_credentials(cap_fd, target_pid, &mut buffer).map_err(|e| {
        eprintln!(
            "Failed to get process credentials for pid {}: {:?}",
            target_pid, e
        );
        Error::new(EINVAL)
    })?;
    let mut cursor = 0;
    let pid = read_num::<u32>(&buffer[cursor..])?;
    cursor += mem::size_of::<u32>() * 3;
    let uid = read_num::<u32>(&buffer[cursor..])?;
    cursor += mem::size_of::<u32>() * 3;
    let gid = read_num::<u32>(&buffer[cursor..])?;
    Ok((pid, uid, gid))
}

fn read_msghdr_info(stream: &mut [u8]) -> Result<(usize, usize, usize)> {
    if stream.len() < mem::size_of::<usize>() * 3 {
        eprintln!(
            "get_msghdr_info: stream buffer is too small to read headers. len: {}",
            stream.len()
        );
        return Err(Error::new(EINVAL));
    }
    let mut cursor: usize = 0;
    let prepared_name_len = read_num::<usize>(&stream[cursor..])?;
    cursor += mem::size_of::<usize>();
    let prepared_whole_iov_size = read_num::<usize>(&stream[cursor..])?;
    cursor += mem::size_of::<usize>();
    let prepared_msg_controllen = read_num::<usize>(&stream[cursor..])?;
    cursor += mem::size_of::<usize>();
    // Clear the stream buffer
    stream[..cursor].copy_from_slice(&[0u8; mem::size_of::<usize>() * 3]);
    Ok((
        prepared_name_len,
        prepared_whole_iov_size,
        prepared_msg_controllen,
    ))
}

struct MsgWriter<'a> {
    buffer: &'a mut [u8],
    written_len: usize,
}
impl<'a> MsgWriter<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            written_len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.written_len
    }

    fn write_name(
        &mut self,
        name: Option<String>,
        name_buf_size: usize,
        name_write_fn: fn(&String, &mut [u8]) -> Result<usize>,
    ) -> Result<()> {
        if self.buffer.len() < self.written_len + mem::size_of::<usize>() {
            eprintln!("MsgWriter::write_name: Buffer too small to write name length. written_len: {}, buffer_len: {}", self.written_len, self.buffer.len());
            self.written_len = self.buffer.len();
            return Ok(());
        }
        if let Some(name) = name {
            let copy_len = cmp::min(
                name_buf_size,
                self.buffer.len() - self.written_len - mem::size_of::<usize>(),
            );
            if name_buf_size > 0 && copy_len < name.len() {
                eprintln!("MsgWriter::write_name: Name will be truncated. Full length: {}, buffer available: {}", name.len(), copy_len);
            }
            let name_len = name_write_fn(
                &name,
                &mut self.buffer[self.written_len + mem::size_of::<usize>()
                    ..self.written_len + mem::size_of::<usize>() + copy_len],
            )?;
            self.buffer[self.written_len..self.written_len + mem::size_of::<usize>()]
                .copy_from_slice(&name_len.to_le_bytes());
            self.written_len += mem::size_of::<usize>() + name_len;
            Ok(())
        } else {
            self.written_len += mem::size_of::<usize>();
            Ok(())
        }
    }

    fn write_payload(&mut self, payload: &[u8], full_len: usize, iov_size: usize) -> Result<usize> {
        let Some(payload_len_buffer) = self
            .buffer
            .get_mut(self.written_len..self.written_len + mem::size_of::<usize>())
        else {
            eprintln!("MsgWriter::write_payload: Buffer too small to write payload length. written_len: {}, buffer_len: {}", self.written_len, self.buffer.len());
            self.written_len = self.buffer.len();
            return Ok(0);
        };
        payload_len_buffer.copy_from_slice(&full_len.to_le_bytes());
        self.written_len += mem::size_of::<usize>();

        let copy_len = cmp::min(iov_size, full_len);
        if copy_len < full_len {
            eprintln!("MsgWriter::write_payload: Payload will be truncated. Full length: {}, buffer available: {}", full_len, copy_len);
        }
        let Some(payload_buffer) = self
            .buffer
            .get_mut(self.written_len..self.written_len + copy_len)
        else {
            eprintln!("MsgWriter::write_payload: Buffer too small to write payload data. written_len: {}, buffer_len: {}", self.written_len, self.buffer.len());
            self.written_len = self.buffer.len();
            return Ok(0);
        };
        payload_buffer.copy_from_slice(&payload[..copy_len]);
        self.written_len += copy_len;
        Ok(copy_len)
    }

    fn write_cmsg(&mut self, level: i32, c_type: i32, data: &[u8]) -> bool {
        let data_len = data.len();

        let Some(remaining_buf) = self.buffer.get_mut(self.written_len..) else {
            eprintln!("CmsgWriter::write_cmsg: No remaining buffer space at all.");
            return false;
        };
        if remaining_buf.len() < CMSG_HEADER_LEN_IN_STREAM {
            eprintln!("CmsgWriter::write_cmsg: Not enough space for cmsg header. remaining: {}, needed: {}", remaining_buf.len(), CMSG_HEADER_LEN_IN_STREAM);
            // Fill the remaining buffer with 1s to indicate Imcomplete CMSG header
            remaining_buf.fill(1);
            self.written_len += remaining_buf.len();
            return false;
        }
        let mut cursor = 0;
        remaining_buf[cursor..cursor + CMSG_LEVEL_SIZE].copy_from_slice(&level.to_le_bytes());
        cursor += CMSG_LEVEL_SIZE;
        remaining_buf[cursor..cursor + CMSG_TYPE_SIZE].copy_from_slice(&c_type.to_le_bytes());
        cursor += CMSG_TYPE_SIZE;
        remaining_buf[cursor..cursor + CMSG_DATA_LEN_SIZE].copy_from_slice(&data_len.to_le_bytes());
        cursor += CMSG_DATA_LEN_SIZE;

        if remaining_buf.len() < cursor + data_len {
            eprintln!(
                "CmsgWriter::write_cmsg: Not enough space for cmsg data. remaining: {}, needed: {}",
                remaining_buf.len(),
                cursor + data_len
            );
            // Fill the remaining buffer with 1s to indicate Imcomplete CMSG data
            remaining_buf.fill(1);
            self.written_len += remaining_buf.len();
            return false;
        }
        remaining_buf[cursor..cursor + data_len].copy_from_slice(data);
        self.written_len += cursor + data_len;

        true
    }

    fn write_rights(&mut self, num_fds: usize) -> bool {
        let data = num_fds.to_le_bytes();
        if num_fds == 0 {
            return true;
        }
        self.write_cmsg(libc::SOL_SOCKET, libc::SCM_RIGHTS, &data)
    }

    fn write_credentials(&mut self, credential: &Credential) -> bool {
        let mut data = [0u8; PID_SIZE + UID_SIZE + GID_SIZE];
        data[..PID_SIZE].copy_from_slice(&credential.pid.to_le_bytes());
        data[PID_SIZE..PID_SIZE + UID_SIZE].copy_from_slice(&credential.uid.to_le_bytes());
        data[PID_SIZE + UID_SIZE..PID_SIZE + UID_SIZE + GID_SIZE]
            .copy_from_slice(&credential.gid.to_le_bytes());
        self.write_cmsg(libc::SOL_SOCKET, SCM_CREDENTIALS, &data)
    }
}

fn path_buf_to_str(path_buf: &[u8]) -> Result<&str> {
    match std::str::from_utf8(path_buf) {
        Ok("") => Err(Error::new(EINVAL)),
        Ok(s) => Ok(s),
        Err(_) => Err(Error::new(EINVAL)),
    }
}
