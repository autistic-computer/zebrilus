use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsString,
    fmt,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock, Weak,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
};

use parking_lot::Mutex;
use zz_protocol::{
    AgentImage, AgentSessionOpKind, ClientFileOperation, ClientFileRequest, ClientFileResponse,
    ClientHello, ClientInstanceId, ClientKind, ClientPath, CommandInvocation, CommandRequest,
    CommandResponse, ConfigOverrideEntry, GuiResponse, InputMessage, MAX_CLIENT_ENVIRONMENT_BYTES,
    MAX_CLIENT_ENVIRONMENT_ENTRIES, MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES, MAX_CLIENT_FILE_BYTES,
    MAX_CLIENT_WORKING_DIRECTORY_BYTES, MAX_PASTE_UPLOAD_CHUNK_BYTES, MIN_DAEMON_REVISION,
    PANE_FRAME_CAPABILITY, PROTOCOL_VERSION, PaneId, PasteUploadPurpose, PreparedCommand,
    PreparedCommandResult, ProtocolError, ProtocolMessage, REVISION, RawText, ServerError,
    ServerHello, StdoutClaim, encode_key_input_into, encode_protocol_message_into,
    read_protocol_message_into,
};
use zz_protocol::{
    ClientEnvironmentBlob, EXEC_CAPABILITY, ExecFlags, ExecOutcome, ExecRequest, ExecResume,
    ExecResumeKind,
};

mod command;
mod interactive;

const CLIENT_FILE_READ_ERRNO: i32 = 5;

#[cfg(any(target_os = "linux", target_os = "macos"))]
static STDIN_WAS_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[used]
#[cfg_attr(target_os = "linux", unsafe(link_section = ".init_array"))]
#[cfg_attr(target_os = "macos", unsafe(link_section = "__DATA,__mod_init_func"))]
#[allow(
    unsafe_code,
    reason = "capture descriptor validity before Rust sanitizes standard input"
)]
static CAPTURE_STDIN: extern "C" fn() = capture_stdin;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(
    unsafe_code,
    reason = "fcntl and errno inspect fd 0 without allocating or changing it"
)]
extern "C" fn capture_stdin() {
    unsafe {
        if libc::fcntl(0, libc::F_GETFD) == -1 {
            #[cfg(target_os = "linux")]
            let error = *libc::__errno_location();
            #[cfg(target_os = "macos")]
            let error = *libc::__error();
            STDIN_WAS_CLOSED.store(error == libc::EBADF, Ordering::Relaxed);
        }
    }
}

static CLIENT_INSTANCE_ID: OnceLock<ClientInstanceId> = OnceLock::new();

fn client_instance_id() -> ClientInstanceId {
    *CLIENT_INSTANCE_ID.get_or_init(|| ClientInstanceId(getrandom::u64().unwrap_or(1).max(1)))
}
use zz_terminal::TerminalColorScheme;

#[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
use crate::endpoint::{SshForward, SshLogin, connect_kept};
#[cfg(any(target_os = "ios", target_os = "android"))]
use crate::russh_client::{RusshForward, RusshStream};
use crate::{
    DaemonError, diagnostic_elapsed_us, diagnostic_timer,
    endpoint::Endpoint,
    transport::{LocalStream, LocalTransport, Transport, TransportStream},
};

pub(crate) enum ClientStream {
    Local(LocalStream),
    #[cfg(any(target_os = "ios", target_os = "android"))]
    Ssh(RusshStream),
    #[cfg(any(target_os = "ios", target_os = "android"))]
    Tcp(std::net::TcpStream),
}

impl Read for ClientStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Local(stream) => stream.read(buffer),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(stream) => stream.read(buffer),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Tcp(stream) => stream.read(buffer),
        }
    }
}

impl Write for ClientStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Local(stream) => stream.write(buffer),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(stream) => stream.write(buffer),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Tcp(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Local(stream) => stream.flush(),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(stream) => stream.flush(),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Tcp(stream) => stream.flush(),
        }
    }
}

#[cfg(unix)]
impl ClientStream {
    fn shutdown(&self) -> io::Result<()> {
        match self {
            Self::Local(stream) => stream.shutdown(),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(_) => Ok(()),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Tcp(stream) => stream.shutdown(std::net::Shutdown::Both),
        }
    }
}

impl TransportStream for ClientStream {
    #[cfg(unix)]
    fn receive_fd(&self) -> io::Result<std::os::fd::OwnedFd> {
        match self {
            Self::Local(stream) => stream.receive_fd(),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(_) | Self::Tcp(_) => Err(io::Error::from(io::ErrorKind::Unsupported)),
        }
    }

    #[cfg(unix)]
    fn read_ready(&self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Local(stream) => stream.read_ready(buffer),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(_) | Self::Tcp(_) => Err(io::Error::from(io::ErrorKind::Unsupported)),
        }
    }

    fn try_clone(&self) -> io::Result<Self> {
        match self {
            Self::Local(stream) => stream.try_clone().map(Self::Local),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Ssh(stream) => stream.try_clone().map(Self::Ssh),
            #[cfg(any(target_os = "ios", target_os = "android"))]
            Self::Tcp(stream) => stream.try_clone().map(Self::Tcp),
        }
    }
}

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandOutcome {
    pub stdout: RawText,
    pub stderr: String,
    pub exit_code: u8,
    pub stdout_claim: StdoutClaim,
    pub client_exit: bool,
}

pub type ExecClassifier<'a> =
    &'a dyn Fn(&[CommandInvocation], &[PreparedCommand]) -> Option<ExecResumeKind>;

pub struct ExecChain<'a> {
    pub commands: Vec<CommandInvocation>,
    pub spawned_server_id: Option<u64>,
    pub expect_server_id: Option<u64>,
    pub resume: Option<ExecClassifier<'a>>,
    pub prepared: bool,
    pub last: bool,
}

impl ExecChain<'_> {
    #[must_use]
    pub fn new(commands: Vec<CommandInvocation>) -> Self {
        Self {
            commands,
            spawned_server_id: None,
            expect_server_id: None,
            resume: None,
            prepared: false,
            last: false,
        }
    }
}

#[derive(Debug)]
pub enum ExecChainEnd {
    Ran { exit_code: u8 },
    Failed(DaemonError),
    Resume(ExecResume),
    Rejected(ServerError),
    ServerMismatch,
}

pub struct CommandClient {
    stdin_enabled: bool,
    stdin_spent: bool,
    file_grants: Option<Vec<FileGrant>>,
    stderr_handler: Option<fn(&str)>,
    stdout_handler: Option<Arc<dyn Fn(&RawText) + Send + Sync>>,
    link: CommandLink,
    route: CommandRoute,
    server_id: Option<u64>,
    #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
    _ssh_forward: Option<SshForward>,
    #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
    _tailnet_forward: Option<crate::tailnet::TailnetForward>,
    #[cfg(windows)]
    _wsl_forward: Option<crate::wsl::WslForward>,
    #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
    unverified: Option<crate::endpoint::SshEndpoint>,
    #[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
    read_only: bool,
}

enum CommandLink {
    Exec {
        reader: ProtocolReceiver<LocalStream>,
        writer: ProtocolSender<LocalStream>,
        answered: bool,
        spent: bool,
    },
    Legacy {
        reader: ProtocolReceiver<LocalStream>,
        writer: ProtocolSender<LocalStream>,
        server_id: u64,
    },
}

struct CommandRoute {
    socket: PathBuf,
    display: String,
    facts: EndpointFactsScope,
    send_origin: bool,
}

fn attach_session_command(
    session: String,
    detach_others: bool,
    read_only: bool,
    client_flags: Option<&str>,
) -> CommandInvocation {
    let mut args = Vec::new();
    if detach_others {
        args.push("-d".to_owned());
    }
    if read_only {
        args.push("-r".to_owned());
    }
    if let Some(client_flags) = client_flags {
        args.extend(["-f".to_owned(), client_flags.to_owned()]);
    }
    if !session.is_empty() {
        args.extend(["-t".to_owned(), session]);
    }
    CommandInvocation::new("attach-session", args)
}

fn stdout_or_exit(outcome: CommandOutcome) -> Result<String, DaemonError> {
    if outcome.exit_code == 0 {
        Ok(outcome.stdout.to_string())
    } else {
        Err(DaemonError::CommandExit {
            output: outcome.stdout,
            exit_code: outcome.exit_code,
        })
    }
}

fn exec_maybe_unsupported(error: &DaemonError) -> bool {
    match error {
        DaemonError::Protocol(ProtocolError::Decode(_)) => true,
        DaemonError::Protocol(ProtocolError::Io(error)) | DaemonError::Io(error) => {
            error.kind() == io::ErrorKind::UnexpectedEof
        }
        _ => false,
    }
}

fn command_failure(error: ServerError, output: RawText) -> DaemonError {
    let error = DaemonError::Server(error);
    if output.is_empty() {
        error
    } else {
        DaemonError::CommandFailed {
            output,
            error: Box::new(error),
        }
    }
}

fn exec_environment(facts: EndpointFactsScope) -> ClientEnvironmentBlob {
    exec_environment_with(scoped_environment(facts, std::env::vars_os()).chain(sender(facts)))
}

fn sender(facts: EndpointFactsScope) -> Option<(OsString, OsString)> {
    if !facts.is_remote() {
        return None;
    }
    let pane = std::env::var("ZZ_PANE").ok()?.parse().ok()?;
    let fact = zz_protocol::agent_mention::sender_fact(
        pane,
        std::env::var("ZZ_AGENT").ok().as_deref(),
        short_device_name().as_deref(),
    );
    Some((
        zz_protocol::agent_mention::SENDER_VARIABLE.into(),
        fact.into(),
    ))
}

const REMOTE_ENVIRONMENT: [&str; 5] = ["TERM", "COLORTERM", "LANG", "LC_ALL", "LC_CTYPE"];

fn scoped_environment(
    facts: EndpointFactsScope,
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> impl Iterator<Item = (OsString, OsString)> {
    let remote = facts.is_remote();
    environment
        .into_iter()
        .filter(move |(name, _)| !remote || REMOTE_ENVIRONMENT.iter().any(|keep| name == keep))
}

fn exec_environment_with<I, K, V>(environment: I) -> ClientEnvironmentBlob
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<OsString>,
    V: Into<OsString>,
{
    let mut blob = Vec::new();
    let mut count = 0usize;
    let mut total = 0usize;
    for (name, value) in environment {
        if count == MAX_CLIENT_ENVIRONMENT_ENTRIES {
            break;
        }
        let name = RawText::from_os_str(&name.into()).into_bytes();
        let value = RawText::from_os_str(&value.into()).into_bytes();
        if name.is_empty() || name.contains(&b'=') || name.contains(&0) || value.contains(&0) {
            continue;
        }
        let entry_bytes = name.len() + 1 + value.len();
        if entry_bytes > MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES
            || total + entry_bytes > MAX_CLIENT_ENVIRONMENT_BYTES
        {
            continue;
        }
        blob.extend_from_slice(&name);
        blob.push(b'=');
        blob.extend_from_slice(&value);
        blob.push(0);
        count += 1;
        total += entry_bytes;
    }
    ClientEnvironmentBlob::from_bytes(blob)
}

fn exec_feature_mask() -> u32 {
    crate::terminal_features::terminal_feature_mask(
        client_terminal_flags()
            .features
            .iter()
            .filter(|spec| spec.len() <= MAX_CLIENT_FEATURE_SPEC_BYTES)
            .take(MAX_CLIENT_FEATURE_SPECS)
            .map(String::as_str),
    ) | LEARNED_TERMINAL_FEATURES.load(Ordering::Relaxed)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum FileGrant {
    Read(PathBuf),
    Write(PathBuf),
}

fn file_grants(commands: &[CommandInvocation]) -> Vec<FileGrant> {
    commands
        .iter()
        .filter_map(|command| {
            let path = PathBuf::from(command.args.last()?.to_os_string());
            match zz_protocol::canonical_command(&command.name) {
                "load-buffer" => Some(FileGrant::Read(path)),
                "save-buffer" => Some(FileGrant::Write(path)),
                _ => None,
            }
        })
        .collect()
}

fn granted<'a>(grants: &'a [FileGrant], request: &ClientFileRequest) -> Option<&'a Path> {
    let asked = Path::new(&request.path);
    grants.iter().find_map(|grant| {
        let ((FileGrant::Read(path), ClientFileOperation::Read)
        | (FileGrant::Write(path), ClientFileOperation::Write { .. })) =
            (grant, &request.operation)
        else {
            return None;
        };
        let named = path.strip_prefix(".").unwrap_or(path);
        (!named.as_os_str().is_empty() && asked.ends_with(named)).then_some(path.as_path())
    })
}

const FILE_NOT_GRANTED: &str =
    "a remote host may only use the file load-buffer or save-buffer names on this command line";

fn answer_command_file(
    request: &ClientFileRequest,
    grants: Option<&[FileGrant]>,
    stdin_enabled: bool,
    stdin_spent: &mut bool,
) -> ClientFileResponse {
    let readable = stdin_enabled && !*stdin_spent;
    let result = match request.operation {
        ClientFileOperation::ReadStdin { binary } if readable => {
            *stdin_spent = true;
            read_command_stdin(binary)
        }
        ClientFileOperation::ReadStdinChunk if readable => {
            let chunk = read_command_stdin_chunk();
            *stdin_spent = !matches!(&chunk, Ok(chunk) if !chunk.is_empty());
            chunk
        }
        _ => {
            return match grants {
                None => answer_client_file(request, Path::new(&request.path)),
                Some(grants) => match granted(grants, request) {
                    Some(path) => answer_client_file(request, path),
                    None => ClientFileResponse {
                        request_id: request.request_id,
                        data: Vec::new(),
                        error: Some(FILE_NOT_GRANTED.to_owned()),
                    },
                },
            };
        }
    };
    ClientFileResponse {
        request_id: request.request_id,
        data: result.as_ref().cloned().unwrap_or_default(),
        error: result.err(),
    }
}

pub struct InteractiveClient {
    reader: Mutex<ProtocolReceiver<ClientStream>>,
    writer: Arc<Mutex<ProtocolSender<ClientStream>>>,
    hello: ServerHello,
    #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
    ssh_forward: Option<SshForward>,
    #[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
    tailnet_forward: Option<crate::tailnet::TailnetForward>,
    #[cfg(windows)]
    wsl_forward: Option<crate::wsl::WslForward>,
    #[cfg(any(target_os = "ios", target_os = "android"))]
    russh_forward: Option<RusshForward>,
}

pub struct TracedMessage<'a>(pub &'a ProtocolMessage);

impl fmt::Debug for TracedMessage<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ProtocolMessage::AgentAnswerQuestion {
                pane,
                request_id,
                answers,
            } => formatter
                .debug_struct("AgentAnswerQuestion")
                .field("pane", pane)
                .field("request_id", request_id)
                .field(
                    "answers",
                    &answers
                        .iter()
                        .map(|answer| (&answer.id, answer.answers.len()))
                        .collect::<Vec<_>>(),
                )
                .finish(),
            message => message.fmt(formatter),
        }
    }
}

struct ProtocolSender<S> {
    stream: S,
    frame: Vec<u8>,
    daemon_capabilities: Vec<String>,
}

struct ProtocolReceiver<S> {
    stream: io::BufReader<S>,
    frame: Vec<u8>,
    pending: VecDeque<ProtocolMessage>,
    #[cfg(unix)]
    ready: Option<ReadyFrames>,
    #[cfg(unix)]
    skipped_decode: bool,
}

#[cfg(all(test, unix, not(any(target_os = "ios", target_os = "android"))))]
#[path = "ctrl_client_tests.rs"]
mod ctrl_client_tests;

#[cfg(unix)]
#[derive(Default)]
struct ReadyFrames {
    bytes: Vec<u8>,
    consumed: usize,
}

#[cfg(unix)]
impl ReadyFrames {
    fn message(&mut self) -> Result<Option<Box<ProtocolMessage>>, ProtocolError> {
        let remaining = &self.bytes[self.consumed..];
        let Some(prefix) = remaining.get(..4) else {
            return Ok(None);
        };
        let length = u32::from_le_bytes(prefix.try_into().expect("four-byte prefix")) as usize;
        if length > zz_protocol::MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge(length));
        }
        if length < 4 {
            return Err(ProtocolError::Truncated);
        }
        let Some(frame) = remaining.get(..length + 4) else {
            return Ok(None);
        };
        let message = zz_protocol::decode_protocol_frame(frame);
        self.consumed += length + 4;
        message.map(|message| Some(Box::new(message)))
    }

    fn read(
        &mut self,
        scratch: &mut Vec<u8>,
        read: impl FnOnce(&mut [u8]) -> io::Result<usize>,
    ) -> io::Result<usize> {
        if scratch.len() < RECEIVE_BUFFER_BYTES {
            scratch.resize(RECEIVE_BUFFER_BYTES, 0);
        }
        let read = read(&mut scratch[..RECEIVE_BUFFER_BYTES])?;
        if read != 0 {
            if self.consumed != 0 {
                self.bytes.drain(..self.consumed);
                self.consumed = 0;
            }
            self.bytes.extend_from_slice(&scratch[..read]);
        }
        Ok(read)
    }
}

const RECEIVE_BUFFER_BYTES: usize = 64 * 1024;

type Connected<S> = (ProtocolReceiver<S>, ProtocolSender<S>, ServerHello);

impl<S: TransportStream> ProtocolReceiver<S> {
    fn new(stream: S) -> Self {
        Self {
            stream: io::BufReader::with_capacity(RECEIVE_BUFFER_BYTES, stream),
            frame: Vec::new(),
            pending: VecDeque::new(),
            #[cfg(unix)]
            ready: None,
            #[cfg(unix)]
            skipped_decode: false,
        }
    }

    #[cfg(unix)]
    fn try_recv_decodable(
        &mut self,
        read_socket: bool,
    ) -> Result<Option<(Box<ProtocolMessage>, bool)>, DaemonError> {
        loop {
            match self.try_recv(read_socket) {
                Err(DaemonError::Protocol(ProtocolError::Decode(error))) => {
                    self.skipped_decode = true;
                    log::warn!(
                        target: "zz_daemon::diagnostics::client",
                        "skipping an undecodable daemon message: {error}"
                    );
                }
                Ok(Some(message)) => {
                    return Ok(Some((message, std::mem::take(&mut self.skipped_decode))));
                }
                Ok(None) => return Ok(None),
                Err(error) => return Err(error),
            }
        }
    }

    #[cfg(unix)]
    fn try_recv(&mut self, read_socket: bool) -> Result<Option<Box<ProtocolMessage>>, DaemonError> {
        use std::io::BufRead as _;
        if let Some(message) = self.pending.pop_front() {
            return Ok(Some(Box::new(message)));
        }
        let ready = self.ready.get_or_insert_with(ReadyFrames::default);
        let buffered = self.stream.buffer();
        ready.bytes.extend_from_slice(buffered);
        let count = buffered.len();
        self.stream.consume(count);
        loop {
            if let Some(message) = ready.message()? {
                return Ok(Some(message));
            }
            if !read_socket {
                return Ok(None);
            }
            match ready.read(&mut self.frame, |buffer| {
                self.stream.get_ref().read_ready(buffer)
            }) {
                Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into()),
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    fn recv_decodable(&mut self) -> Result<(ProtocolMessage, bool), DaemonError> {
        #[cfg(unix)]
        let mut skipped = std::mem::take(&mut self.skipped_decode);
        #[cfg(not(unix))]
        let mut skipped = false;
        loop {
            match self.recv() {
                Err(DaemonError::Protocol(ProtocolError::Decode(error))) => {
                    skipped = true;
                    log::warn!(
                        target: "zz_daemon::diagnostics::client",
                        "skipping an undecodable daemon message: {error}"
                    );
                }
                result => return result.map(|message| (message, skipped)),
            }
        }
    }

    fn recv(&mut self) -> Result<ProtocolMessage, DaemonError> {
        if let Some(message) = self.pending.pop_front() {
            return Ok(message);
        }
        #[cfg(unix)]
        if let Some(ready) = self.ready.as_mut() {
            loop {
                if let Some(message) = ready.message()? {
                    return Ok(*message);
                }
                if ready.read(&mut self.frame, |buffer| self.stream.read(buffer))? == 0 {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
            }
        }
        let started = diagnostic_timer();
        let message = read_protocol_message_into(&mut self.stream, &mut self.frame)?;
        log::trace!(
            target: "zz_daemon::diagnostics::protocol",
            "recv bytes={} frame_capacity={} elapsed_us={} message={message:#?}",
            self.frame.len(),
            self.frame.capacity(),
            diagnostic_elapsed_us(started),
        );
        Ok(message)
    }
}

impl<S: TransportStream> ProtocolSender<S> {
    fn new(stream: S) -> Self {
        Self {
            stream,
            frame: Vec::new(),
            daemon_capabilities: Vec::new(),
        }
    }

    fn admit(&self, message: &ProtocolMessage) -> Result<(), DaemonError> {
        match message.required_capability() {
            Some(capability)
                if !self
                    .daemon_capabilities
                    .iter()
                    .any(|name| name == capability) =>
            {
                Err(DaemonError::DaemonLacksCapability { capability })
            }
            _ => Ok(()),
        }
    }

    fn send(&mut self, message: &ProtocolMessage) -> Result<(), DaemonError> {
        self.admit(message)?;
        self.send_encoded(message, |frame| {
            encode_protocol_message_into(message, frame)
        })
    }

    fn send_encoded(
        &mut self,
        message: &impl std::fmt::Debug,
        encode: impl FnOnce(&mut Vec<u8>) -> Result<(), ProtocolError>,
    ) -> Result<(), DaemonError> {
        let started = diagnostic_timer();
        let encode_started = diagnostic_timer();
        encode(&mut self.frame)?;
        let encode_us = diagnostic_elapsed_us(encode_started);
        let write_started = diagnostic_timer();
        self.stream.write_all(&self.frame)?;
        let write_us = diagnostic_elapsed_us(write_started);
        let flush_started = diagnostic_timer();
        self.stream.flush()?;
        let flush_us = diagnostic_elapsed_us(flush_started);
        log::trace!(
            target: "zz_daemon::diagnostics::protocol",
            "send bytes={} frame_capacity={} encode_us={} write_us={} flush_us={} elapsed_us={} message={message:#?}",
            self.frame.len(),
            self.frame.capacity(),
            encode_us,
            write_us,
            flush_us,
            diagnostic_elapsed_us(started),
        );
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EndpointFactsScope {
    None,
    LocalHostWorkingDirectory,
    LocalControlTerminalIdentity,
    PortableTerminalSize,
    LocalHostWorkingDirectoryAndTerminal,
}

fn flow_window_wanted(remote: bool) -> bool {
    match std::env::var(crate::FLOW_WINDOW_ENVIRONMENT_VARIABLE).as_deref() {
        Ok("on") => true,
        Ok("off") => false,
        _ => remote,
    }
}

impl EndpointFactsScope {
    fn is_remote(self) -> bool {
        matches!(self, Self::None | Self::PortableTerminalSize)
    }

    fn includes_working_directory(self) -> bool {
        matches!(
            self,
            Self::LocalHostWorkingDirectory
                | Self::LocalControlTerminalIdentity
                | Self::LocalHostWorkingDirectoryAndTerminal
        )
    }

    fn includes_terminal_size(self) -> bool {
        matches!(
            self,
            Self::PortableTerminalSize | Self::LocalHostWorkingDirectoryAndTerminal
        )
    }

    fn includes_tty(self) -> bool {
        matches!(
            self,
            Self::LocalControlTerminalIdentity | Self::LocalHostWorkingDirectoryAndTerminal
        )
    }

    fn tty_scope(self) -> Option<CallerTtyScope> {
        match self {
            Self::LocalControlTerminalIdentity => Some(CallerTtyScope::StandardInput),
            Self::LocalHostWorkingDirectoryAndTerminal => Some(CallerTtyScope::StandardStreams),
            Self::None | Self::LocalHostWorkingDirectory | Self::PortableTerminalSize => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallerTtyScope {
    StandardInput,
    StandardStreams,
}

pub const DEFAULT_CELL_WIDTH_PX: u32 = 8;
pub const DEFAULT_CELL_HEIGHT_PX: u32 = 16;

#[must_use]
pub fn cell_pixel_extent(pixels: u16, cells: u16, fallback: u32) -> u32 {
    if pixels == 0 || cells == 0 {
        fallback
    } else {
        (u32::from(pixels) / u32::from(cells)).max(1)
    }
}

#[cfg(unix)]
fn caller_terminal_size() -> Option<(u16, u16, u32, u32)> {
    let size = rustix::termios::tcgetwinsize(std::io::stdout()).ok()?;
    (size.ws_col > 0 && size.ws_row > 0).then(|| {
        (
            size.ws_col,
            size.ws_row,
            cell_pixel_extent(size.ws_xpixel, size.ws_col, DEFAULT_CELL_WIDTH_PX),
            cell_pixel_extent(size.ws_ypixel, size.ws_row, DEFAULT_CELL_HEIGHT_PX),
        )
    })
}

#[cfg(not(unix))]
fn caller_terminal_size() -> Option<(u16, u16, u32, u32)> {
    None
}

#[cfg(unix)]
fn caller_tty(scope: CallerTtyScope) -> Option<String> {
    use std::os::fd::AsFd as _;

    let stdin = std::io::stdin();
    if scope == CallerTtyScope::StandardInput {
        return rustix::termios::ttyname(stdin.as_fd(), Vec::new())
            .ok()?
            .into_string()
            .ok();
    }
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    for fd in [stdin.as_fd(), stdout.as_fd(), stderr.as_fd()] {
        if let Ok(name) = rustix::termios::ttyname(fd, Vec::new()) {
            return name.into_string().ok();
        }
    }
    None
}

#[cfg(not(unix))]
fn caller_tty(_scope: CallerTtyScope) -> Option<String> {
    None
}

fn terminal_facts_capabilities(
    scope: EndpointFactsScope,
    nested: bool,
    capabilities: &mut Vec<String>,
) {
    terminal_facts_capabilities_with(
        scope,
        nested,
        caller_terminal_size,
        caller_tty,
        capabilities,
    );
}

fn startup_config_owner_capability(
    kind: ClientKind,
    startup_config_owner: bool,
    capabilities: &mut Vec<String>,
) {
    if kind == ClientKind::Control && startup_config_owner {
        capabilities.push(ClientHello::STARTUP_CONFIG_OWNER_CAPABILITY.to_owned());
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientTerminalFlags {
    pub utf8: bool,
    pub features: Vec<String>,
}

static CLIENT_TERMINAL_FLAGS: std::sync::OnceLock<ClientTerminalFlags> = std::sync::OnceLock::new();

pub fn set_client_terminal_flags(flags: ClientTerminalFlags) {
    let _ = CLIENT_TERMINAL_FLAGS.set(flags);
}

fn client_terminal_flags() -> &'static ClientTerminalFlags {
    CLIENT_TERMINAL_FLAGS.get_or_init(ClientTerminalFlags::default)
}

pub fn client_terminal_colour_count() -> u32 {
    let flags = client_terminal_flags();
    crate::terminal_features::terminal_colour_count(
        &std::env::var("TERM").unwrap_or_default(),
        &std::env::var("COLORTERM").unwrap_or_default(),
        crate::terminal_features::terminal_feature_mask(flags.features.iter().map(String::as_str))
            | LEARNED_TERMINAL_FEATURES.load(Ordering::Relaxed)
            | NEGOTIATED_TERMINAL_FEATURES.load(Ordering::Relaxed),
    )
}

#[must_use]
pub fn client_takes_utf8_terminal() -> bool {
    client_terminal_flags().utf8 || client_takes_utf8(|name| std::env::var_os(name))
}

fn client_utf8_capability(capabilities: &mut Vec<String>) {
    if client_takes_utf8_terminal() {
        capabilities.push(ClientHello::CLIENT_UTF8_CAPABILITY.to_owned());
    }
}

static LEARNED_TERMINAL_FEATURES: AtomicU32 = AtomicU32::new(0);

static NEGOTIATED_TERMINAL_FEATURES: AtomicU32 = AtomicU32::new(0);

pub fn adopt_negotiated_terminal_features(features: &[String]) {
    NEGOTIATED_TERMINAL_FEATURES.fetch_or(
        crate::terminal_features::terminal_feature_mask(features.iter().map(String::as_str)),
        Ordering::Relaxed,
    );
}

static INTERACTIVE_WRITER: Mutex<Option<Weak<Mutex<ProtocolSender<ClientStream>>>>> =
    Mutex::new(None);

fn register_interactive_writer(writer: &Arc<Mutex<ProtocolSender<ClientStream>>>) {
    *INTERACTIVE_WRITER.lock() = Some(Arc::downgrade(writer));
    report_learned_terminal_features(LEARNED_TERMINAL_FEATURES.load(Ordering::Relaxed));
}

pub fn learn_client_terminal_features(features: &str) {
    let mask = crate::terminal_features::terminal_feature_mask([features]);
    if mask == 0 {
        return;
    }
    let learned = LEARNED_TERMINAL_FEATURES.fetch_or(mask, Ordering::Relaxed) | mask;
    report_learned_terminal_features(learned);
}

#[must_use]
pub fn client_terminal_feature_mask() -> u32 {
    crate::terminal_features::terminal_feature_mask(
        client_terminal_flags().features.iter().map(String::as_str),
    ) | LEARNED_TERMINAL_FEATURES.load(Ordering::Relaxed)
        | NEGOTIATED_TERMINAL_FEATURES.load(Ordering::Relaxed)
}

fn report_learned_terminal_features(learned: u32) {
    if learned == 0 {
        return;
    }
    let Some(writer) = INTERACTIVE_WRITER.lock().as_ref().and_then(Weak::upgrade) else {
        return;
    };
    let features = crate::terminal_features::terminal_features_list(learned)
        .split(',')
        .map(str::to_owned)
        .collect();
    let _ = writer
        .lock()
        .send(&ProtocolMessage::ClientTerminalFeatures { features });
}

pub fn report_terminal_type(term_type: &str) {
    if term_type.is_empty() || term_type.len() > zz_protocol::MAX_CLIENT_TERMINAL_TYPE_BYTES {
        return;
    }
    let Some(writer) = INTERACTIVE_WRITER.lock().as_ref().and_then(Weak::upgrade) else {
        return;
    };
    let _ = writer.lock().send(&ProtocolMessage::ClientTerminalType {
        term_type: term_type.to_owned(),
    });
}

const MAX_CLIENT_FEATURE_SPECS: usize = 16;
const MAX_CLIENT_FEATURE_SPEC_BYTES: usize = 200;

fn client_learned_features_capability(capabilities: &mut Vec<String>) {
    let learned = LEARNED_TERMINAL_FEATURES.load(Ordering::Relaxed);
    if learned == 0 {
        return;
    }
    capabilities.push(format!(
        "{}{}",
        ClientHello::CLIENT_FEATURES_CAPABILITY_PREFIX,
        crate::terminal_features::terminal_features_list(learned)
    ));
}

fn client_features_capabilities(features: &[String], capabilities: &mut Vec<String>) {
    capabilities.extend(
        features
            .iter()
            .filter(|spec| spec.len() <= MAX_CLIENT_FEATURE_SPEC_BYTES)
            .take(MAX_CLIENT_FEATURE_SPECS)
            .map(|spec| format!("{}{spec}", ClientHello::CLIENT_FEATURES_CAPABILITY_PREFIX)),
    );
}

fn client_takes_utf8(lookup: impl Fn(&str) -> Option<OsString>) -> bool {
    if lookup("TMUX").is_some() {
        return true;
    }
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .filter_map(&lookup)
        .find(|value| !value.is_empty())
        .is_some_and(|value| {
            let value = value.to_string_lossy().to_ascii_uppercase();
            value.contains("UTF-8") || value.contains("UTF8")
        })
}

fn terminal_facts_capabilities_with(
    scope: EndpointFactsScope,
    nested: bool,
    terminal_size: impl FnOnce() -> Option<(u16, u16, u32, u32)>,
    tty: impl FnOnce(CallerTtyScope) -> Option<String>,
    capabilities: &mut Vec<String>,
) {
    if scope.includes_terminal_size()
        && let Some((columns, rows, cell_width_px, cell_height_px)) = terminal_size()
    {
        capabilities.push(format!(
            "{}{columns}x{rows}",
            ClientHello::CLIENT_SIZE_CAPABILITY_PREFIX
        ));
        capabilities.push(format!(
            "{}{cell_width_px}x{cell_height_px}",
            ClientHello::CLIENT_CELL_CAPABILITY_PREFIX
        ));
    }
    if let Some(tty_scope) = scope.tty_scope()
        && let Some(tty) = tty(tty_scope)
    {
        capabilities.push(format!(
            "{}{tty}",
            ClientHello::CLIENT_TTY_CAPABILITY_PREFIX
        ));
    }
    if scope.includes_tty() && nested {
        capabilities.push(ClientHello::CLIENT_NESTED_CAPABILITY.to_owned());
    }
}

fn client_working_directory(
    scope: EndpointFactsScope,
    current_dir: impl FnOnce() -> Option<PathBuf>,
) -> Option<ClientPath> {
    scope
        .includes_working_directory()
        .then(current_dir)
        .flatten()
        .as_deref()
        .and_then(ClientPath::from_path)
        .filter(|working_directory| working_directory.len() <= MAX_CLIENT_WORKING_DIRECTORY_BYTES)
}

fn logical_current_dir() -> Option<PathBuf> {
    let current = std::env::current_dir().ok()?;
    let Some(declared) = std::env::var_os("PWD").filter(|value| !value.is_empty()) else {
        return Some(current);
    };
    let declared = PathBuf::from(declared);
    match (declared.canonicalize(), current.canonicalize()) {
        (Ok(left), Ok(right)) if left == right => Some(declared),
        _ => Some(current),
    }
}

fn client_environment(facts: EndpointFactsScope) -> Vec<RawText> {
    client_environment_with(scoped_environment(facts, std::env::vars_os()).chain(sender(facts)))
}

fn client_environment_with<I, K, V>(environment: I) -> Vec<RawText>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<OsString>,
    V: Into<OsString>,
{
    let environment = environment
        .into_iter()
        .map(|(name, value)| {
            (
                RawText::from_os_str(&name.into()).into_bytes(),
                RawText::from_os_str(&value.into()).into_bytes(),
            )
        })
        .collect::<BTreeMap<Vec<u8>, Vec<u8>>>();
    let mut snapshot = Vec::new();
    let mut total_bytes = 0usize;
    for (name, value) in environment {
        if snapshot.len() == MAX_CLIENT_ENVIRONMENT_ENTRIES {
            break;
        }
        if name.is_empty() || name.contains(&b'=') || name.contains(&0) || value.contains(&0) {
            continue;
        }
        let Some(entry_bytes) = name
            .len()
            .checked_add(1)
            .and_then(|length| length.checked_add(value.len()))
        else {
            continue;
        };
        if entry_bytes > MAX_CLIENT_ENVIRONMENT_ENTRY_BYTES {
            continue;
        }
        let Some(next_total_bytes) = total_bytes.checked_add(entry_bytes) else {
            continue;
        };
        if next_total_bytes > MAX_CLIENT_ENVIRONMENT_BYTES {
            continue;
        }
        let mut entry = name;
        entry.push(b'=');
        entry.extend_from_slice(&value);
        snapshot.push(RawText::from_bytes(entry));
        total_bytes = next_total_bytes;
    }
    snapshot
}

fn materialize_welcome<S: TransportStream>(
    reader: &mut ProtocolReceiver<S>,
    welcome: zz_protocol::Welcome,
) -> Result<ServerHello, DaemonError> {
    let mut hello = ServerHello {
        protocol_version: welcome.protocol_version,
        server_id: welcome.server_id,
        client_id: welcome.client_id,
        client_instance_id: welcome.client_instance_id,
        capabilities: welcome.capability_strings(),
        appearance: zz_terminal::TerminalAppearance::default(),
        appearance_provenance: zz_terminal::AppearanceProvenance::default(),
        mux_options: zz_protocol::MuxOptions::default(),
        status: zz_protocol::StatusLine::default(),
        key_tables: Vec::new(),
        clock: welcome.clock,
        platform: welcome.platform,
        revision: welcome.revision,
    };
    let mut pings = Vec::new();
    let batch = loop {
        let message = reader.recv()?;
        if let ProtocolMessage::Batch(batch) = message {
            break batch;
        }
        if let ProtocolMessage::Ping { .. } = message {
            pings.push(message);
            continue;
        }
        if let ProtocolMessage::CommandResponse(CommandResponse::Error { error, .. }) = message {
            return Err(DaemonError::Server(error));
        }
    };
    for message in batch.messages()? {
        if let ProtocolMessage::Event(zz_protocol::Event { payload, .. }) = message {
            match payload {
                zz_protocol::EventPayload::AppearanceChanged {
                    appearance,
                    provenance,
                } => {
                    hello.appearance = *appearance;
                    hello.appearance_provenance = provenance;
                }
                zz_protocol::EventPayload::MuxOptionsChanged { options } => {
                    hello.mux_options = options;
                }
                zz_protocol::EventPayload::MuxOptionsPatched { options } => {
                    for (key, value) in options.iter() {
                        hello
                            .mux_options
                            .set(key, value.value.clone(), value.source);
                    }
                }
                zz_protocol::EventPayload::StatusChanged { status } => hello.status = status,
                zz_protocol::EventPayload::KeyTablesChanged { tables } => hello.key_tables = tables,
                _ => {}
            }
        }
    }
    reader.pending.push_back(ProtocolMessage::Batch(batch));
    reader.pending.extend(pings);
    Ok(hello)
}

#[expect(clippy::too_many_arguments)]
fn connect_stream<S: TransportStream>(
    stream: S,
    endpoint_display: impl fmt::Display,
    kind: ClientKind,
    device_name: Option<String>,
    color_scheme: Option<TerminalColorScheme>,
    client_has_terminal: bool,
    send_origin: bool,
    client_facts: EndpointFactsScope,
) -> Result<Connected<S>, DaemonError> {
    connect_stream_with_startup_owner(
        stream,
        endpoint_display,
        kind,
        device_name,
        color_scheme,
        client_has_terminal,
        send_origin,
        false,
        &[],
        client_facts,
    )
}

#[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
fn attach_resendable(attach: Option<&zz_protocol::AttachOperation>) -> bool {
    let Some(zz_protocol::AttachOperation::Commands(commands)) = attach else {
        return true;
    };
    commands.iter().all(|command| {
        let name = zz_protocol::canonical_command(&command.invocation.name);
        name == "attach-session"
            || zz_protocol::catalog_command_spec(name)
                .is_some_and(|spec| !spec.mutates(&command.invocation.args))
    })
}

#[expect(clippy::too_many_arguments)]
fn connect_stream_with_startup_owner<S: TransportStream>(
    stream: S,
    endpoint_display: impl fmt::Display,
    kind: ClientKind,
    device_name: Option<String>,
    color_scheme: Option<TerminalColorScheme>,
    client_has_terminal: bool,
    send_origin: bool,
    startup_config_owner: bool,
    extra_capabilities: &[&str],
    client_facts: EndpointFactsScope,
) -> Result<Connected<S>, DaemonError> {
    connect_stream_hello(
        stream,
        endpoint_display,
        kind,
        device_name,
        color_scheme,
        client_has_terminal,
        send_origin,
        startup_config_owner,
        extra_capabilities,
        client_facts,
        None,
    )
}

#[expect(clippy::too_many_arguments)]
fn connect_stream_hello<S: TransportStream>(
    stream: S,
    endpoint_display: impl fmt::Display,
    kind: ClientKind,
    device_name: Option<String>,
    color_scheme: Option<TerminalColorScheme>,
    client_has_terminal: bool,
    send_origin: bool,
    startup_config_owner: bool,
    extra_capabilities: &[&str],
    client_facts: EndpointFactsScope,
    attach: Option<zz_protocol::AttachOperation>,
) -> Result<Connected<S>, DaemonError> {
    let started = diagnostic_timer();
    let mut reader = ProtocolReceiver::new(stream.try_clone()?);
    let mut writer = ProtocolSender::new(stream);
    let mut capabilities = if kind == ClientKind::Command {
        std::env::var(crate::STARTUP_REENTRY_ENVIRONMENT_VARIABLE)
            .ok()
            .filter(|token| !token.is_empty())
            .map(|token| {
                vec![format!(
                    "{}{}",
                    crate::STARTUP_REENTRY_CAPABILITY_PREFIX,
                    token
                )]
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    startup_config_owner_capability(kind, startup_config_owner, &mut capabilities);
    client_utf8_capability(&mut capabilities);
    client_features_capabilities(&client_terminal_flags().features, &mut capabilities);
    client_learned_features_capability(&mut capabilities);
    if client_facts.is_remote() {
        capabilities.push(ClientHello::CLIENT_REMOTE_CAPABILITY.to_owned());
    }
    if kind == ClientKind::Interactive && flow_window_wanted(client_facts.is_remote()) {
        capabilities.push(zz_protocol::FLOW_WINDOW_CAPABILITY.to_owned());
    }
    if kind == ClientKind::Interactive && client_has_terminal {
        capabilities.push(ClientHello::CLIENT_TERMINAL_CAPABILITY.to_owned());
        if matches!(
            client_facts,
            EndpointFactsScope::None | EndpointFactsScope::LocalHostWorkingDirectory
        ) {
            capabilities.push(ClientHello::CLIENT_NATIVE_TERMINAL_SEARCH_CAPABILITY.to_owned());
            capabilities.push(ClientHello::CLIENT_NATIVE_CHOOSER_CAPABILITY.to_owned());
        }
    }
    capabilities.extend(
        extra_capabilities
            .iter()
            .chain(zz_protocol::CLIENT_CAPABILITY_NAMES)
            .map(|capability| (*capability).to_owned()),
    );
    terminal_facts_capabilities(
        client_facts,
        std::env::var_os("TMUX").is_some_and(|value| !value.is_empty()),
        &mut capabilities,
    );
    let client_hello = ClientHello {
        protocol_version: PROTOCOL_VERSION,
        client_instance_id: client_instance_id(),
        kind,
        device_name,
        capabilities,
        color_scheme,
        origin: (send_origin && kind == ClientKind::Command)
            .then(|| std::env::var("ZZ_PANE").ok())
            .flatten()
            .and_then(|pane| pane.parse().ok()),
        working_directory: client_working_directory(client_facts, logical_current_dir),
        environment: client_environment(client_facts),
        process_id: std::process::id(),
        revision: REVISION,
    };
    if kind == ClientKind::Command {
        writer.send(&ProtocolMessage::ClientHello(client_hello))?;
    } else {
        let mut hello = zz_protocol::Hello::from_client(client_hello);
        hello.attach = attach;
        if kind == ClientKind::Control {
            hello.subscriptions.tree = zz_protocol::TreeSubscription::All;
        } else if client_facts.includes_terminal_size() {
            hello.subscriptions = zz_protocol::Subscriptions::terminal();
        }
        hello.viewport = client_facts
            .includes_terminal_size()
            .then(caller_terminal_size)
            .flatten()
            .map(
                |(columns, rows, cell_width_px, cell_height_px)| zz_protocol::ClientViewport {
                    columns,
                    rows,
                    cell_width_px,
                    cell_height_px,
                },
            );
        writer.send(&ProtocolMessage::Hello(hello))?;
    }
    let hello = match reader.recv()? {
        ProtocolMessage::Welcome(welcome) => materialize_welcome(&mut reader, welcome)?,
        ProtocolMessage::ServerHello(hello) => *hello,
        ProtocolMessage::CommandResponse(CommandResponse::Error { error, .. }) => {
            return Err(DaemonError::Server(error));
        }
        _ => {
            return Err(DaemonError::Server(ServerError::Internal(
                "daemon did not send ServerHello".to_owned(),
            )));
        }
    };
    if hello.revision < MIN_DAEMON_REVISION {
        return Err(DaemonError::DaemonTooOld {
            daemon: hello.revision,
            minimum: MIN_DAEMON_REVISION,
        });
    }
    if kind == ClientKind::Interactive
        && [PANE_FRAME_CAPABILITY, zz_protocol::CONTROL_CAPABILITY]
            .iter()
            .any(|required| {
                !hello
                    .capabilities
                    .iter()
                    .any(|capability| capability == required)
            })
    {
        return Err(DaemonError::Protocol(ProtocolError::VersionMismatch {
            expected: PROTOCOL_VERSION,
            received: hello.protocol_version,
        }));
    }
    writer.daemon_capabilities.clone_from(&hello.capabilities);
    log::debug!(
        target: "zz_daemon::diagnostics::client",
        "connected path={endpoint_display} kind={kind:?} server_hello={hello:#?} elapsed_us={}",
        diagnostic_elapsed_us(started),
    );
    Ok((reader, writer, hello))
}

pub fn short_device_name() -> Option<String> {
    let host = crate::process_info::host_name()?;
    host.trim()
        .split('.')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

const STDIN_CHUNK_BYTES: usize = 16 * 1024;

fn caller_stdin() -> Result<std::fs::File, String> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if STDIN_WAS_CLOSED.load(Ordering::Relaxed) {
        eprintln!(
            "[err] evsig_cb: recv: {}",
            crate::strerror_text(&io::Error::from_raw_os_error(libc::EBADF))
        );
        std::process::exit(1);
    }
    let stdin = io::stdin();
    #[cfg(unix)]
    let duplicate = std::os::fd::AsFd::as_fd(&stdin).try_clone_to_owned();
    #[cfg(windows)]
    let duplicate = std::os::windows::io::AsHandle::as_handle(&stdin).try_clone_to_owned();
    duplicate
        .map(std::fs::File::from)
        .map_err(|error| crate::strerror_text(&error))
}

fn stdin_read_error() -> String {
    errno_text(CLIENT_FILE_READ_ERRNO)
}

#[cfg(unix)]
fn errno_text(errno: i32) -> String {
    crate::strerror_text(&io::Error::from_raw_os_error(errno))
}

#[cfg(not(unix))]
fn errno_text(errno: i32) -> String {
    match errno {
        5 => "Input/output error",
        9 => "Bad file descriptor",
        _ => "Unknown error",
    }
    .to_owned()
}

fn read_command_stdin(binary: bool) -> Result<Vec<u8>, String> {
    read_command_stdin_from(caller_stdin()?, binary)
}

fn read_command_stdin_chunk() -> Result<Vec<u8>, String> {
    read_command_stdin_chunk_from(caller_stdin()?)
}

fn read_command_stdin_chunk_from(mut reader: impl Read) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    let _signal = StdinReadSignal::install().map_err(|error| error.to_string())?;
    let mut chunk = vec![0; STDIN_CHUNK_BYTES];
    loop {
        match reader.read(&mut chunk) {
            Ok(read) => {
                chunk.truncate(read);
                return Ok(chunk);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Err(stdin_read_error()),
        }
    }
}

fn read_command_stdin_from(reader: impl Read, binary: bool) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    let _signal = StdinReadSignal::install().map_err(|error| error.to_string())?;
    let limit = zz_protocol::MAX_AGENT_SEND_BYTES;
    let mut payload = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut payload)
        .map_err(|_| stdin_read_error())?;
    if payload.len() > limit {
        return Err(format!("standard input exceeds {limit} bytes"));
    }
    if !binary && std::str::from_utf8(&payload).is_err() {
        return Err("could not read standard input: stream did not contain valid UTF-8".to_owned());
    }
    Ok(payload)
}

#[cfg(unix)]
struct StdinReadSignal(libc::sigaction);

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "sigaction saves and restores the exact previous disposition"
)]
impl StdinReadSignal {
    fn install() -> io::Result<Self> {
        unsafe extern "C" fn terminate(_: libc::c_int) {
            unsafe { libc::_exit(0) };
        }
        unsafe {
            let mut previous = std::mem::zeroed();
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = terminate as *const () as usize;
            libc::sigemptyset(&raw mut action.sa_mask);
            if libc::sigaction(libc::SIGTERM, &raw const action, &raw mut previous) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(previous))
        }
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "restore the saved signal disposition when the scope ends"
)]
impl Drop for StdinReadSignal {
    fn drop(&mut self) {
        unsafe { libc::sigaction(libc::SIGTERM, &raw const self.0, std::ptr::null_mut()) };
    }
}

fn answer_client_file(request: &ClientFileRequest, path: &Path) -> ClientFileResponse {
    let (data, error) = match &request.operation {
        ClientFileOperation::ReadStdin { .. } | ClientFileOperation::ReadStdinChunk => {
            (Vec::new(), Some(errno_text(9)))
        }
        ClientFileOperation::Read => match read_client_file(path) {
            Ok(data) => (data, None),
            Err(error) => (Vec::new(), Some(error)),
        },
        ClientFileOperation::Write { append, data } => {
            (Vec::new(), write_client_file(path, data, *append).err())
        }
    };
    ClientFileResponse {
        request_id: request.request_id,
        data,
        error,
    }
}

fn read_client_file(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| crate::strerror_text(&error))?;
    let limit = u64::try_from(MAX_CLIENT_FILE_BYTES).unwrap_or(u64::MAX);
    let mut data = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(|_| errno_text(CLIENT_FILE_READ_ERRNO))?;
    Ok(data)
}

fn write_client_file(path: &Path, data: &[u8], append: bool) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(path)
        .map_err(|error| crate::strerror_text(&error))?;
    file.write_all(data)
        .and_then(|()| file.flush())
        .map_err(|error| crate::strerror_text(&error))
}

#[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
pub(crate) fn tailnet_failure(
    endpoint: &crate::endpoint::TailnetEndpoint,
    error: &crate::tailnet::TailnetError,
) -> DaemonError {
    crate::EndpointError::Tailnet {
        target: endpoint.to_string(),
        reason: error.to_string(),
    }
    .into()
}

#[cfg(any(unix, windows))]
fn tailnet_closed(endpoint: &crate::endpoint::TailnetEndpoint, error: DaemonError) -> DaemonError {
    let closed = matches!(
        &error,
        DaemonError::Protocol(zz_protocol::ProtocolError::Io(io)) | DaemonError::Io(io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
            )
    );
    if !closed {
        return error;
    }
    crate::EndpointError::Tailnet {
        target: endpoint.to_string(),
        reason: "it closed the connection before zz answered; run `zz tailnet status` there to see whether it lets this machine in".to_owned(),
    }
    .into()
}

#[cfg(all(any(unix, windows), not(any(target_os = "ios", target_os = "android"))))]
fn start_tailnet_forward(
    endpoint: &crate::endpoint::TailnetEndpoint,
) -> Result<(crate::tailnet::TailnetForward, LocalStream), DaemonError> {
    let forward = crate::tailnet::TailnetForward::start(endpoint)
        .map_err(|error| tailnet_failure(endpoint, &error))?;
    let stream = LocalTransport::connect(forward.socket())?;
    Ok((forward, stream))
}

#[cfg(windows)]
fn start_wsl_forward(
    endpoint: &crate::endpoint::WslEndpoint,
) -> Result<(crate::wsl::WslForward, LocalStream), DaemonError> {
    let forward = crate::wsl::WslForward::start(endpoint)?;
    let stream = LocalTransport::connect(forward.local_socket())?;
    Ok((forward, stream))
}

#[cfg(test)]
mod connection_tests;

#[cfg(any(target_os = "ios", target_os = "android"))]
fn connect_tailnet_tcp(
    endpoint: &crate::endpoint::TailnetEndpoint,
) -> Result<std::net::TcpStream, DaemonError> {
    use std::net::ToSocketAddrs as _;
    let failure = |reason: String| -> DaemonError {
        crate::EndpointError::Tailnet {
            target: endpoint.to_string(),
            reason,
        }
        .into()
    };
    let port = endpoint
        .port
        .unwrap_or(crate::endpoint::TailnetEndpoint::DEFAULT_PORT);
    let addresses = (endpoint.peer.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| {
            failure(format!(
                "cannot find {} ({error}); is the Tailscale app on?",
                endpoint.peer
            ))
        })?;
    let mut last = None;
    for address in addresses {
        match std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_secs(5)) {
            Ok(stream) => {
                let _ = stream.set_nodelay(true);
                return Ok(stream);
            }
            Err(error) => last = Some(format!("{address}: {error}")),
        }
    }
    Err(failure(format!(
        "nothing answers ({}); is zz running there with tailnet on?",
        last.unwrap_or_else(|| "no address".to_owned())
    )))
}
