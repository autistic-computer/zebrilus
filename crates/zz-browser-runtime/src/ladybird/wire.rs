use super::messages::server;
use rustix::{
    fs::{MemfdFlags, memfd_create},
    net::{
        RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
        SendAncillaryMessage, SendFlags, recvmsg, sendmsg,
    },
};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, IoSlice, IoSliceMut, Read, Write},
    mem::MaybeUninit,
    os::fd::AsFd,
    os::unix::{fs::FileExt, net::UnixStream},
};

pub(super) fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "unsupported Ladybird 2026.47.7 IPC message",
    )
}

pub(super) fn send(socket: &mut UnixStream, message: u32, body: &[u8]) -> io::Result<()> {
    let size = u32::try_from(body.len() + 8).map_err(|_| invalid())?;
    let shared = size > 64 * 1024;
    let mut header = [0; 12];
    header[0] = if shared { 2 } else { 0 };
    header[4..8].copy_from_slice(&size.to_le_bytes());
    header[8] = u8::from(shared);
    let mut payload = Vec::with_capacity(size as usize);
    payload.extend(server::MAGIC.to_le_bytes());
    payload.extend(message.to_le_bytes());
    payload.extend_from_slice(body);
    if shared {
        let mut file = File::from(memfd_create("zz-browser-message", MemfdFlags::CLOEXEC)?);
        file.write_all(&payload)?;
        let mut storage = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
        let mut control = SendAncillaryBuffer::new(&mut storage);
        let files = [file.as_fd()];
        if !control.push(SendAncillaryMessage::ScmRights(&files)) {
            return Err(invalid());
        }
        let sent = loop {
            match sendmsg(
                &*socket,
                &[IoSlice::new(&header)],
                &mut control,
                SendFlags::NOSIGNAL,
            ) {
                Err(rustix::io::Errno::INTR) => (),
                value => break value?,
            }
        };
        socket.write_all(&header[sent..])
    } else {
        let mut packet = header.to_vec();
        packet.extend(payload);
        socket.write_all(&packet)
    }
}

pub(super) struct Reader {
    pub socket: UnixStream,
    pending: VecDeque<File>,
}

pub(super) struct Message {
    pub endpoint: u32,
    pub id: u32,
    pub payload: Vec<u8>,
    pub files: Vec<File>,
}

impl Reader {
    pub fn new(socket: UnixStream) -> Self {
        Self {
            socket,
            pending: VecDeque::new(),
        }
    }

    fn receive(&mut self, mut buf: &mut [u8]) -> io::Result<()> {
        while !buf.is_empty() {
            let mut storage = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(256))];
            let mut control = RecvAncillaryBuffer::new(&mut storage);
            let result = recvmsg(
                &self.socket,
                &mut [IoSliceMut::new(buf)],
                &mut control,
                RecvFlags::CMSG_CLOEXEC,
            );
            let received = match result {
                Err(rustix::io::Errno::INTR) => continue,
                value => value?,
            };
            if received.bytes == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if received.flags.contains(ReturnFlags::CTRUNC) {
                return Err(invalid());
            }
            for message in control.drain() {
                if let RecvAncillaryMessage::ScmRights(files) = message {
                    self.pending.extend(files.map(File::from));
                }
            }
            if self.pending.len() > 512 {
                return Err(invalid());
            }
            buf = &mut buf[received.bytes..];
        }
        Ok(())
    }

    pub fn next(&mut self) -> io::Result<Message> {
        loop {
            let mut header = [0; 12];
            self.receive(&mut header)?;
            let mut fields = &header[4..];
            let len = fields.u32()? as usize;
            let count = fields.u32()? as usize;
            if header[0] == 1 && len == 0 {
                continue;
            }
            if !(8..=32 * 1024 * 1024).contains(&len) || count > 512 {
                return Err(invalid());
            }
            let mut payload = vec![0; len];
            if header[0] == 0 {
                self.receive(&mut payload)?;
            }
            if count > self.pending.len() {
                return Err(invalid());
            }
            let mut files: Vec<_> = self.pending.drain(..count).collect();
            match header[0] {
                0 => (),
                2 => files
                    .pop()
                    .ok_or_else(invalid)?
                    .read_exact_at(&mut payload, 0)?,
                _ => return Err(invalid()),
            }
            let mut fields = payload.as_slice();
            return Ok(Message {
                endpoint: fields.u32()?,
                id: fields.u32()?,
                payload,
                files,
            });
        }
    }
}

pub(super) trait Fields<'a> {
    fn field(&mut self, count: usize) -> io::Result<&'a [u8]>;
    fn byte(&mut self) -> io::Result<u8>;
    fn u32(&mut self) -> io::Result<u32>;
    fn u64(&mut self) -> io::Result<u64>;
    fn string(&mut self) -> io::Result<&'a str>;
    fn title(&mut self) -> io::Result<String>;
}

impl<'a> Fields<'a> for &'a [u8] {
    fn field(&mut self, count: usize) -> io::Result<&'a [u8]> {
        self.split_off(..count).ok_or_else(invalid)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.field(1)?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        let mut b = [0; 4];
        self.read_exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn u64(&mut self) -> io::Result<u64> {
        let mut b = [0; 8];
        self.read_exact(&mut b)?;
        Ok(u64::from_le_bytes(b))
    }
    fn string(&mut self) -> io::Result<&'a str> {
        let n = self.u32()? as usize;
        std::str::from_utf8(self.field(n)?).map_err(|_| invalid())
    }
    fn title(&mut self) -> io::Result<String> {
        let latin = self.byte()? != 0;
        let n = self.u32()? as usize;
        if latin {
            return Ok(self.field(n)?.iter().copied().map(char::from).collect());
        }
        Ok(char::decode_utf16(
            self.field(n * 2)?
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| u16::from_le_bytes([v[0], v[1]])),
        )
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_payloads_on_either_side_of_the_inline_limit() {
        let (mut client, engine) = UnixStream::pair().unwrap();
        let mut reader = Reader::new(engine);
        for len in [65528, 65529] {
            let body = vec![0x9a; len];
            send(&mut client, server::VIEW_LOAD, &body).unwrap();
            let msg = reader.next().unwrap();
            assert_eq!(msg.endpoint, server::MAGIC);
            assert_eq!(msg.id, server::VIEW_LOAD);
            assert_eq!(&msg.payload[8..], body);
            assert!(msg.files.is_empty());
        }
    }
}
