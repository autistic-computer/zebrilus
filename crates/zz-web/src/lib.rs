use std::{
    fmt::Write as _,
    io,
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::{
        State,
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use interprocess::local_socket::{
    GenericFilePath,
    tokio::{Stream, prelude::*},
};
use percent_encoding::percent_decode_str;
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::io::ReaderStream;
use zz_daemon_client::{DaemonError, DaemonRoute, Endpoint, EndpointError, HostEntry};
use zz_protocol::{MAX_ENCODED_FRAME_BYTES, MAX_FRAME_BYTES};

const TOKEN_BYTES: usize = 32;
const COOKIE_MAX_AGE_SECONDS: u32 = 30 * 24 * 60 * 60;
const LOCKED_MESSAGE: &str = "Open the link zz-web printed when it started.\n";
const LOCAL_HOST: &str = "local";
const LOCAL_TITLE: &str = "This computer";
const DAEMON_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_REPLY_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CLOSE_REASON_BYTES: usize = 123;
const SIGN_IN_CLOSE_CODE: u16 = 4401;
const UNREACHABLE_CLOSE_CODE: u16 = 4502;

pub struct GatewayConfig {
    pub bind: SocketAddr,
    pub assets: PathBuf,
    pub socket: PathBuf,
    pub hosts: Vec<HostEntry>,
}

pub struct Gateway {
    listener: TcpListener,
    address: SocketAddr,
    token: String,
    router: Router,
}

struct GatewayState {
    address: SocketAddr,
    assets: PathBuf,
    socket: PathBuf,
    hosts: Vec<HostEntry>,
    token: String,
    cookie: String,
}

impl Gateway {
    pub async fn bind(config: GatewayConfig) -> io::Result<Self> {
        if !config.bind.ip().is_loopback() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the browser gateway must bind to a loopback address",
            ));
        }
        let assets = tokio::fs::canonicalize(config.assets).await?;
        if !tokio::fs::metadata(&assets).await?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the asset path must be a directory",
            ));
        }
        let listener = TcpListener::bind(config.bind).await?;
        let address = listener.local_addr()?;
        let token = new_token()?;
        let state = Arc::new(GatewayState {
            address,
            assets,
            socket: config.socket,
            hosts: config.hosts,
            token: token.clone(),
            cookie: format!("zz-web-{}", address.port()),
        });
        let router = Router::new()
            .route("/ws", get(upgrade))
            .route("/hosts", get(hosts))
            .fallback(asset)
            .with_state(state);
        Ok(Self {
            listener,
            address,
            token,
            router,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub fn open_url(&self) -> String {
        format!("http://{}/?token={}", self.address, self.token)
    }

    pub async fn serve(self) -> io::Result<()> {
        axum::serve(self.listener, self.router).await
    }
}

impl GatewayState {
    fn target(&self, query: Option<&str>) -> Result<Target, StatusCode> {
        let mut names = query
            .into_iter()
            .flat_map(|query| query.split('&'))
            .filter_map(|pair| pair.strip_prefix("host="));
        let Some(name) = names.next() else {
            return Ok(Target::Local);
        };
        if names.next().is_some() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let name = percent_decode_str(name)
            .decode_utf8()
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        if name == LOCAL_HOST {
            return Ok(Target::Local);
        }
        self.hosts
            .iter()
            .find(|host| host.name == name)
            .cloned()
            .map(Target::Host)
            .ok_or(StatusCode::NOT_FOUND)
    }

    fn accepts_host(&self, headers: &HeaderMap) -> bool {
        let Some(host) = single_header(headers, header::HOST) else {
            return false;
        };
        host == self.address.to_string() || host == format!("localhost:{}", self.address.port())
    }

    fn accepts_origin(&self, headers: &HeaderMap) -> bool {
        self.accepts_host(headers)
            && single_header(headers, header::HOST)
                .zip(single_header(headers, header::ORIGIN))
                .is_some_and(|(host, origin)| origin == format!("http://{host}"))
    }

    fn holds_token(&self, headers: &HeaderMap) -> bool {
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|value| value.split(';'))
            .filter_map(|pair| pair.trim().split_once('='))
            .any(|(name, value)| name == self.cookie && same_secret(value, &self.token))
    }

    fn open(&self, query: &str) -> Response {
        let (tokens, rest): (Vec<&str>, Vec<&str>) = query
            .split('&')
            .partition(|pair| pair.starts_with("token="));
        let opened = matches!(tokens.as_slice(), [pair] if pair
            .strip_prefix("token=")
            .is_some_and(|token| same_secret(token, &self.token)));
        if !opened {
            return locked();
        }
        let location = if rest.is_empty() {
            "/".to_owned()
        } else {
            format!("/?{}", rest.join("&"))
        };
        let cookie = format!(
            "{}={}; Path=/; Max-Age={COOKIE_MAX_AGE_SECONDS}; HttpOnly; SameSite=Strict",
            self.cookie, self.token
        );
        let (Ok(location), Ok(cookie)) = (
            HeaderValue::from_str(&location),
            HeaderValue::from_str(&cookie),
        ) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let mut response = StatusCode::SEE_OTHER.into_response();
        let headers = response.headers_mut();
        headers.insert(header::LOCATION, location);
        headers.insert(header::SET_COOKIE, cookie);
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        headers.insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
        response
    }
}

fn new_token() -> io::Result<String> {
    let mut bytes = [0; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes
        .iter()
        .fold(String::with_capacity(TOKEN_BYTES * 2), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        }))
}

fn same_secret(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0, |difference, (left, right)| difference | (left ^ right))
            == 0
}

fn locked() -> Response {
    let mut response = (StatusCode::FORBIDDEN, LOCKED_MESSAGE).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn single_header(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

#[derive(serde::Serialize)]
struct HostRow<'a> {
    name: &'a str,
    title: &'a str,
    local: bool,
}

async fn hosts(State(state): State<Arc<GatewayState>>, headers: HeaderMap) -> Response {
    if !state.accepts_host(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.holds_token(&headers) {
        return locked();
    }
    let hosts: Vec<HostRow<'_>> = std::iter::once(HostRow {
        name: LOCAL_HOST,
        title: LOCAL_TITLE,
        local: true,
    })
    .chain(state.hosts.iter().map(|host| HostRow {
        name: &host.name,
        title: &host.name,
        local: false,
    }))
    .collect();
    let Ok(body) = serde_json::to_string(&hosts) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

enum Target {
    Local,
    Host(HostEntry),
}

async fn upgrade(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    uri: Uri,
    websocket: WebSocketUpgrade,
) -> Response {
    if !state.accepts_origin(&headers) || !state.holds_token(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let target = match state.target(uri.query()) {
        Ok(target) => target,
        Err(status) => return status.into_response(),
    };
    let websocket = websocket
        .max_message_size(MAX_ENCODED_FRAME_BYTES)
        .max_frame_size(MAX_ENCODED_FRAME_BYTES)
        .max_write_buffer_size(MAX_ENCODED_FRAME_BYTES * 2);
    let host = match target {
        Target::Local => {
            let Ok(daemon) = connect_daemon(&state.socket).await else {
                return StatusCode::SERVICE_UNAVAILABLE.into_response();
            };
            return websocket.on_upgrade(move |websocket| async move {
                let _ = bridge(websocket, daemon).await;
            });
        }
        Target::Host(host) => host,
    };
    websocket.on_upgrade(move |mut websocket| async move {
        match open_host(host.endpoint).await {
            Ok((route, daemon)) => {
                let _ = bridge(websocket, daemon).await;
                let _ = tokio::task::spawn_blocking(move || drop(route)).await;
            }
            Err(error) => {
                let frame = close_frame(&host.name, &error);
                if websocket.send(Message::Close(Some(frame))).await.is_ok() {
                    let _ = tokio::time::timeout(CLOSE_REPLY_TIMEOUT, async {
                        while let Some(Ok(message)) = websocket.recv().await {
                            if matches!(message, Message::Close(_)) {
                                break;
                            }
                        }
                    })
                    .await;
                }
            }
        }
    })
}

async fn connect_daemon(socket: &Path) -> io::Result<Stream> {
    let name = socket.as_os_str().to_fs_name::<GenericFilePath>()?;
    tokio::time::timeout(DAEMON_CONNECT_TIMEOUT, Stream::connect(name))
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
}

async fn open_host(endpoint: Endpoint) -> Result<(DaemonRoute, Stream), DaemonError> {
    let route = tokio::task::spawn_blocking(move || DaemonRoute::open_unattended(&endpoint))
        .await
        .map_err(io::Error::other)??;
    match connect_daemon(route.socket()).await {
        Ok(daemon) => Ok((route, daemon)),
        Err(error) => {
            let _ = tokio::task::spawn_blocking(move || drop(route)).await;
            Err(error.into())
        }
    }
}

fn close_frame(name: &str, error: &DaemonError) -> CloseFrame {
    let found = EndpointError::find(error);
    let code = if found.is_some_and(EndpointError::needs_sign_in) {
        SIGN_IN_CLOSE_CODE
    } else {
        UNREACHABLE_CLOSE_CODE
    };
    let detail = match found {
        Some(
            EndpointError::SshFailed { reason, .. } | EndpointError::ProbeFailure { reason, .. },
        ) => reason.clone(),
        Some(found) => found.ssh_reason().unwrap_or_else(|| found.to_string()),
        None => error.to_string(),
    };
    let mut reason = format!("{name}: {detail}");
    if reason.len() > MAX_CLOSE_REASON_BYTES {
        let mut end = MAX_CLOSE_REASON_BYTES;
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        reason.truncate(end);
    }
    CloseFrame {
        code,
        reason: reason.into(),
    }
}

async fn bridge(websocket: WebSocket, daemon: Stream) -> io::Result<()> {
    let (mut browser_write, mut browser_read) = websocket.split();
    let (mut daemon_read, mut daemon_write) = (&daemon, &daemon);
    let browser_to_daemon = async {
        while let Some(message) = browser_read.next().await {
            match message.map_err(io::Error::other)? {
                Message::Binary(frame) => {
                    validate_frame(&frame)?;
                    daemon_write.write_all(&frame).await?;
                }
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => {}
                Message::Text(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "the zz connection requires binary protocol frames",
                    ));
                }
            }
        }
        Ok(())
    };
    let daemon_to_browser = async {
        loop {
            let frame = read_frame(&mut daemon_read).await?;
            browser_write
                .send(Message::Binary(frame.into()))
                .await
                .map_err(io::Error::other)?;
        }
    };
    tokio::select! {
        result = browser_to_daemon => result,
        result = daemon_to_browser => result,
    }
}

fn frame_length(prefix: [u8; 4]) -> io::Result<usize> {
    let length = u32::from_le_bytes(prefix) as usize;
    if !(4..=MAX_FRAME_BYTES).contains(&length) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid zz protocol frame length",
        ));
    }
    Ok(length)
}

fn validate_frame(frame: &[u8]) -> io::Result<()> {
    let prefix = frame
        .get(..4)
        .and_then(|prefix| prefix.try_into().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated zz protocol frame"))?;
    if frame_length(prefix)? + 4 != frame.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "one WebSocket message must contain exactly one zz protocol frame",
        ));
    }
    Ok(())
}

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix).await?;
    let length = frame_length(prefix)?;
    let mut frame = vec![0; length + 4];
    frame[..4].copy_from_slice(&prefix);
    reader.read_exact(&mut frame[4..]).await?;
    Ok(frame)
}

async fn asset(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    method: Method,
    uri: Uri,
) -> Response {
    if !state.accepts_host(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if method != Method::GET && method != Method::HEAD {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if let Some(query) = uri
        .query()
        .filter(|query| query.split('&').any(|pair| pair.starts_with("token=")))
    {
        return state.open(query);
    }
    if !state.holds_token(&headers) {
        return locked();
    }
    let Some(relative) = asset_path(uri.path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(path) = tokio::fs::canonicalize(state.assets.join(relative)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !path.starts_with(&state.assets) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(file) = File::open(&path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(metadata) = file.metadata().await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !metadata.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(ReaderStream::new(file))
    };
    let mut response = body.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type(&path)),
    );
    if let Ok(length) = HeaderValue::from_str(&metadata.len().to_string()) {
        headers.insert(header::CONTENT_LENGTH, length);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "Cross-Origin-Opener-Policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(
        "Cross-Origin-Embedder-Policy",
        HeaderValue::from_static("require-corp"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'none'"),
    );
    response
}

fn asset_path(path: &str) -> Option<PathBuf> {
    let path = percent_decode_str(path).decode_utf8().ok()?;
    if path.contains(['\\', '\0']) {
        return None;
    }
    let relative = path.strip_prefix('/')?;
    if relative.is_empty() {
        return Some(PathBuf::from("index.html"));
    }
    if relative
        .split('/')
        .any(|component| matches!(component, "." | ".." | ""))
    {
        return None;
    }
    let path = Path::new(relative);
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_frames_are_complete_and_bounded() {
        assert!(validate_frame(&[4, 0, 0, 0, 0, 0, 0, 0]).is_ok());
        assert!(validate_frame(&[4, 0, 0]).is_err());
        assert!(validate_frame(&[4, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(validate_frame(&[4, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(frame_length(3_u32.to_le_bytes()).is_err());
        assert!(frame_length((MAX_FRAME_BYTES as u32 + 1).to_le_bytes()).is_err());
    }

    #[tokio::test]
    async fn stream_frames_keep_boundaries_and_reject_truncation() {
        let first = [4, 0, 0, 0, 1, 2, 3, 4];
        let second = [5, 0, 0, 0, 5, 6, 7, 8, 9];
        let bytes = [first.as_slice(), second.as_slice()].concat();
        let mut reader = bytes.as_slice();
        assert_eq!(read_frame(&mut reader).await.unwrap(), first);
        assert_eq!(read_frame(&mut reader).await.unwrap(), second);
        assert!(read_frame(&mut reader).await.is_err());
        assert!(read_frame(&mut &first[..7]).await.is_err());
    }

    #[test]
    fn asset_paths_reject_encoded_and_platform_traversal() {
        assert_eq!(asset_path("/"), Some(PathBuf::from("index.html")));
        assert_eq!(
            asset_path("/pkg/client.wasm"),
            Some(PathBuf::from("pkg/client.wasm"))
        );
        for path in [
            "/../secret",
            "/%2e%2e/secret",
            "/pkg/%2f../secret",
            "/pkg/./client.js",
            "/pkg\\secret",
            "/%5csecret",
            "/%00secret",
            "//secret",
            "/pkg//secret",
        ] {
            assert!(asset_path(path).is_none(), "accepted {path}");
        }
    }
}
