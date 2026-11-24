use std::{
    fmt,
    io::{self, BufRead, BufReader, Read},
    net::IpAddr,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::TailnetError;

const HOST: &str = "local-tailscaled.sock";
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSE: u64 = 64 << 20;

pub(super) const STATUS: &str = "/localapi/v0/status";

pub(super) fn whois_path(address: IpAddr) -> String {
    format!(
        "/localapi/v0/whois?addr={}",
        address.to_string().replace(':', "%3A")
    )
}

pub(super) fn get(path: &str) -> Result<String, TailnetError> {
    let deadline = Instant::now() + TIMEOUT;
    let (mut connection, token) = connect(deadline)?;
    let exchanged = connection
        .send(request(path, token.as_deref()).as_bytes())
        .and_then(|()| read_response(BufReader::new(connection.take(MAX_RESPONSE))));
    let (status, body) = exchanged.map_err(|error| match error.kind() {
        io::ErrorKind::InvalidData => TailnetError::Parse(error.to_string()),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => TailnetError::Api(format!(
            "Tailscale did not answer within {}s",
            TIMEOUT.as_secs()
        )),
        io::ErrorKind::UnexpectedEof => {
            TailnetError::Api("Tailscale closed the connection before it answered".to_owned())
        }
        _ => TailnetError::Api(format!("lost the connection to Tailscale: {error}")),
    })?;
    let body = String::from_utf8(body).map_err(|error| TailnetError::Parse(error.to_string()))?;
    if status == 200 {
        Ok(body)
    } else {
        Err(TailnetError::Api(format!(
            "Tailscale answered {status}: {}",
            body.lines().next().unwrap_or_default().trim()
        )))
    }
}

pub(super) fn request(path: &str, token: Option<&str>) -> String {
    let authorization = token.map_or_else(String::new, |token| {
        format!(
            "Authorization: Basic {}\r\n",
            STANDARD.encode(format!(":{token}"))
        )
    });
    format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\n{authorization}Connection: close\r\n\r\n")
}

pub(super) fn read_response(mut reader: impl BufRead) -> io::Result<(u16, Vec<u8>)> {
    let mut line = String::new();
    read_line(&mut reader, &mut line)?;
    let status = line
        .strip_prefix("HTTP/1.")
        .and_then(|rest| rest.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| invalid(format!("no HTTP status line in {:?}", line.trim_end())))?;
    let mut length = None;
    let mut chunked = false;
    loop {
        read_line(&mut reader, &mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(
                value
                    .parse::<u64>()
                    .map_err(|_| invalid(format!("bad Content-Length {value:?}")))?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.eq_ignore_ascii_case("chunked");
        }
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            read_line(&mut reader, &mut line)?;
            let size = line.trim_end().split(';').next().unwrap_or_default().trim();
            let size = u64::from_str_radix(size, 16)
                .map_err(|_| invalid(format!("bad chunk size {size:?}")))?;
            if size == 0 {
                break;
            }
            read_exactly(&mut reader, size, &mut body)?;
            read_line(&mut reader, &mut line)?;
        }
    } else if let Some(length) = length {
        read_exactly(&mut reader, length, &mut body)?;
    } else {
        reader.read_to_end(&mut body)?;
    }
    Ok((status, body))
}

fn read_line(reader: &mut impl BufRead, line: &mut String) -> io::Result<()> {
    line.clear();
    if reader.read_line(line)? == 0 {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    Ok(())
}

fn read_exactly(reader: &mut impl BufRead, size: u64, body: &mut Vec<u8>) -> io::Result<()> {
    let read = reader.take(size).read_to_end(body)?;
    if u64::try_from(read).ok() == Some(size) {
        Ok(())
    } else {
        Err(io::ErrorKind::UnexpectedEof.into())
    }
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(any(target_os = "macos", test))]
pub(super) fn sameuserproof(path: &str) -> Option<(u16, String)> {
    let (_, name) = path.split_once(".tailscale.ipn.macos/sameuserproof-")?;
    let (port, token) = name.split_once('-')?;
    let port = Some(port)
        .filter(|port| port.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|port| port.parse::<u16>().ok())
        .filter(|port| *port != 0)?;
    (!token.is_empty() && token.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .then(|| (port, token.to_owned()))
}

enum Place {
    #[cfg(target_os = "macos")]
    AppStore,
    #[cfg(target_os = "macos")]
    Standalone,
    #[cfg(unix)]
    Socket(&'static str),
    #[cfg(windows)]
    Pipe(&'static str),
}

#[cfg(target_os = "macos")]
const PLACES: &[Place] = &[
    Place::AppStore,
    Place::Standalone,
    Place::Socket("/var/run/tailscaled.socket"),
];

#[cfg(all(unix, not(target_os = "macos")))]
const PLACES: &[Place] = &[Place::Socket("/var/run/tailscale/tailscaled.sock")];

#[cfg(windows)]
const PLACES: &[Place] = &[Place::Pipe(
    r"\\.\pipe\ProtectedPrefix\Administrators\Tailscale\tailscaled",
)];

impl fmt::Display for Place {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(target_os = "macos")]
            Self::AppStore => formatter.write_str("IPNExtension process"),
            #[cfg(target_os = "macos")]
            Self::Standalone => formatter.write_str(macos::STANDALONE_PORT),
            #[cfg(unix)]
            Self::Socket(path) => formatter.write_str(path),
            #[cfg(windows)]
            Self::Pipe(path) => formatter.write_str(path),
        }
    }
}

impl Place {
    fn open(&self, deadline: Instant) -> io::Result<(Stream, Option<String>)> {
        left(deadline)?;
        match self {
            #[cfg(target_os = "macos")]
            Self::AppStore => {
                let (port, token) = macos::app_store_proof().ok_or(io::ErrorKind::NotFound)?;
                macos::localhost(port, deadline).map(|stream| (Stream::Tcp(stream), Some(token)))
            }
            #[cfg(target_os = "macos")]
            Self::Standalone => {
                let (port, token) = macos::standalone_proof()?;
                macos::localhost(port, deadline).map(|stream| (Stream::Tcp(stream), Some(token)))
            }
            #[cfg(unix)]
            Self::Socket(path) => std::os::unix::net::UnixStream::connect(path)
                .map(|stream| (Stream::Unix(stream), None)),
            #[cfg(windows)]
            Self::Pipe(path) => pipe::open(path, deadline).map(|file| (Stream::Pipe(file), None)),
        }
    }
}

fn connect(deadline: Instant) -> Result<(Connection, Option<String>), TailnetError> {
    let mut failure = None;
    for place in PLACES {
        match place.open(deadline) {
            Ok((stream, token)) => return Ok((Connection { stream, deadline }, token)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) if failure.is_none() => {
                failure = Some(match error.kind() {
                    io::ErrorKind::ConnectionRefused => {
                        format!("Tailscale is not running ({place} refuses connections)")
                    }
                    _ => format!("could not reach Tailscale at {place}: {error}"),
                });
            }
            Err(_) => {}
        }
    }
    Err(TailnetError::Api(failure.unwrap_or_else(|| {
        let looked: Vec<String> = PLACES.iter().map(|place| format!("no {place}")).collect();
        format!("Tailscale is not running ({})", looked.join(", "))
    })))
}

enum Stream {
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    #[cfg(target_os = "macos")]
    Tcp(std::net::TcpStream),
    #[cfg(windows)]
    Pipe(std::fs::File),
}

struct Connection {
    stream: Stream,
    deadline: Instant,
}

fn left(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::ErrorKind::TimedOut.into())
}

impl Connection {
    fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        use std::io::Write as _;

        match &mut self.stream {
            #[cfg(unix)]
            Stream::Unix(stream) => {
                stream.set_write_timeout(Some(left(self.deadline)?))?;
                stream.write_all(bytes)
            }
            #[cfg(target_os = "macos")]
            Stream::Tcp(stream) => {
                stream.set_write_timeout(Some(left(self.deadline)?))?;
                stream.write_all(bytes)
            }
            #[cfg(windows)]
            Stream::Pipe(file) => file.write_all(bytes),
        }
    }
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match &mut self.stream {
            #[cfg(unix)]
            Stream::Unix(stream) => {
                stream.set_read_timeout(Some(left(self.deadline)?))?;
                stream.read(buffer)
            }
            #[cfg(target_os = "macos")]
            Stream::Tcp(stream) => {
                stream.set_read_timeout(Some(left(self.deadline)?))?;
                stream.read(buffer)
            }
            #[cfg(windows)]
            Stream::Pipe(file) => pipe::read(file, buffer, self.deadline),
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{c_int, c_void},
        io,
        mem::MaybeUninit,
        net::{Ipv4Addr, SocketAddr, TcpStream},
        time::Instant,
    };

    use super::{left, sameuserproof};

    pub(super) const STANDALONE_PORT: &str = "/Library/Tailscale/ipnport";
    const STANDALONE_PROOF: &str = "/Library/Tailscale/sameuserproof-";
    const EXTENSION: &str = "IPNExtension";
    const PROC_UID_ONLY: u32 = 4;
    const PROC_PIDFDVNODEPATHINFO: c_int = 2;

    #[repr(C)]
    struct VnodeFdInfoWithPath {
        file: [u64; 3],
        path: libc::vnode_info_path,
    }

    pub(super) fn localhost(port: u16, deadline: Instant) -> io::Result<TcpStream> {
        TcpStream::connect_timeout(
            &SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            left(deadline)?,
        )
    }

    pub(super) fn standalone_proof() -> io::Result<(u16, String)> {
        let port = std::fs::read_link(STANDALONE_PORT)?
            .to_string_lossy()
            .into_owned();
        let number = port.parse::<u16>().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("it names port {port:?}"),
            )
        })?;
        let token = std::fs::read_to_string(format!("{STANDALONE_PROOF}{port}"))?;
        let token = token.trim();
        if token.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "its token file is empty",
            ));
        }
        Ok((number, token.to_owned()))
    }

    #[allow(
        unsafe_code,
        reason = "getuid, proc_listpids and proc_pidinfo are libc entry points with no safe wrapper"
    )]
    pub(super) fn app_store_proof() -> Option<(u16, String)> {
        let user = unsafe { libc::getuid() };
        let pids: Vec<libc::pid_t> = listed(|buffer, size| unsafe {
            libc::proc_listpids(PROC_UID_ONLY, user, buffer, size)
        });
        pids.into_iter()
            .filter(|pid| *pid > 0 && is_extension(*pid))
            .find_map(|pid| {
                let descriptors: Vec<libc::proc_fdinfo> = listed(|buffer, size| unsafe {
                    libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, buffer, size)
                });
                descriptors
                    .iter()
                    .filter(|descriptor| {
                        i64::from(descriptor.proc_fdtype) == i64::from(libc::PROX_FDTYPE_VNODE)
                    })
                    .find_map(|descriptor| {
                        vnode_path(pid, descriptor.proc_fd).and_then(|path| sameuserproof(&path))
                    })
            })
    }

    #[allow(
        unsafe_code,
        reason = "fill writes at most room bytes, and the kernel initialized the first count items"
    )]
    fn listed<T: Copy>(fill: impl Fn(*mut c_void, c_int) -> c_int) -> Vec<T> {
        let Ok(needed) = usize::try_from(fill(std::ptr::null_mut(), 0)) else {
            return Vec::new();
        };
        let mut items = Vec::<T>::with_capacity(needed / size_of::<T>() + 32);
        let Ok(room) = c_int::try_from(items.capacity() * size_of::<T>()) else {
            return Vec::new();
        };
        let written = usize::try_from(fill(items.as_mut_ptr().cast(), room)).unwrap_or(0);
        let count = (written / size_of::<T>()).min(items.capacity());
        unsafe { items.set_len(count) };
        items
    }

    #[allow(
        unsafe_code,
        reason = "proc_name is a libc entry point with no safe wrapper; it writes at most the buffer's length"
    )]
    fn is_extension(pid: libc::pid_t) -> bool {
        let mut name = [0_u8; 64];
        let length = unsafe { libc::proc_name(pid, name.as_mut_ptr().cast(), name.len() as u32) };
        usize::try_from(length)
            .ok()
            .and_then(|length| name.get(..length))
            .is_some_and(|name| name.starts_with(EXTENSION.as_bytes()))
    }

    #[allow(
        unsafe_code,
        reason = "proc_pidfdinfo fills a vnode_fdinfowithpath, which VnodeFdInfoWithPath mirrors"
    )]
    fn vnode_path(pid: libc::pid_t, descriptor: i32) -> Option<String> {
        let size = c_int::try_from(size_of::<VnodeFdInfoWithPath>()).ok()?;
        let mut info = MaybeUninit::<VnodeFdInfoWithPath>::zeroed();
        let written = unsafe {
            libc::proc_pidfdinfo(
                pid,
                descriptor,
                PROC_PIDFDVNODEPATHINFO,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        if written != size {
            return None;
        }
        let info = unsafe { info.assume_init() };
        let path: Vec<u8> = info
            .path
            .vip_path
            .iter()
            .flatten()
            .map(|character| *character as u8)
            .take_while(|byte| *byte != 0)
            .collect();
        String::from_utf8(path).ok()
    }
}

#[cfg(windows)]
mod pipe {
    use std::{
        fs::File,
        io::{self, Read as _},
        os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _},
        thread,
        time::{Duration, Instant},
    };

    use windows::Win32::{
        Foundation::{ERROR_BROKEN_PIPE, ERROR_PIPE_BUSY, HANDLE},
        Storage::FileSystem::SECURITY_IDENTIFICATION,
        System::Pipes::PeekNamedPipe,
    };

    const POLL: Duration = Duration::from_millis(5);

    pub(super) fn open(path: &str, deadline: Instant) -> io::Result<File> {
        loop {
            match File::options()
                .read(true)
                .write(true)
                .security_qos_flags(SECURITY_IDENTIFICATION.0)
                .open(path)
            {
                Err(error)
                    if error.raw_os_error() == i32::try_from(ERROR_PIPE_BUSY.0).ok()
                        && Instant::now() < deadline =>
                {
                    thread::sleep(POLL);
                }
                opened => return opened,
            }
        }
    }

    #[allow(
        unsafe_code,
        reason = "PeekNamedPipe is a raw Win32 entry point with no safe wrapper"
    )]
    pub(super) fn read(file: &mut File, buffer: &mut [u8], deadline: Instant) -> io::Result<usize> {
        loop {
            let mut available = 0_u32;
            let peeked = unsafe {
                PeekNamedPipe(
                    HANDLE(file.as_raw_handle()),
                    None,
                    0,
                    None,
                    Some(&raw mut available),
                    None,
                )
            };
            match peeked {
                Err(error) if error.code() == ERROR_BROKEN_PIPE.to_hresult() => return Ok(0),
                Err(error) => return Err(io::Error::other(error)),
                Ok(()) if available > 0 => return file.read(buffer),
                Ok(()) if Instant::now() >= deadline => return Err(io::ErrorKind::TimedOut.into()),
                Ok(()) => thread::sleep(POLL),
            }
        }
    }
}
