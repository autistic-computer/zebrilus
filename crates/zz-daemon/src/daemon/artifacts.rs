use super::*;
use std::time::SystemTime;
use std::{
    collections::{HashMap, HashSet},
    io,
    sync::{OnceLock, mpsc},
};

use zz_protocol::{MAX_ARTIFACT_BYTES, MAX_ARTIFACT_CHUNK_BYTES, MAX_ARTIFACT_PATH_BYTES};

pub(super) const ARTIFACT_PROFILE: &str = "artifacts";
const INDEX: &str = "index.html";
const SCAN: Duration = Duration::from_millis(250);
const REPLY_GAP: Duration = Duration::from_secs(1);
const SEND_QUEUE_BYTES: usize = 4 * 1024 * 1024;
const SEND_QUEUE_MESSAGES: usize = 64;
const SEND_WAIT: Duration = Duration::from_secs(30);
const SEND_POLL: Duration = Duration::from_millis(5);

#[derive(Default)]
pub(super) struct Artifacts {
    by_token: HashMap<String, Artifact>,
    by_root: HashMap<PathBuf, String>,
}

#[derive(Clone, Debug)]
pub(super) enum ArtifactRoot {
    File(PathBuf),
    Dir(PathBuf),
    Inline(Arc<[u8]>),
}

#[derive(Debug)]
pub(super) struct Artifact {
    pub(super) root: ArtifactRoot,
    pub(super) opener: Option<PaneId>,
    pub(super) reply: bool,
    pub(super) served: HashMap<PathBuf, Option<Stamp>>,
    replied: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    pub(super) fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Target {
    File(PathBuf),
    Bytes(Arc<[u8]>),
}

impl Artifacts {
    pub(super) fn register(
        &mut self,
        root: ArtifactRoot,
        opener: Option<PaneId>,
        reply: bool,
    ) -> io::Result<String> {
        let key = match &root {
            ArtifactRoot::File(path) | ArtifactRoot::Dir(path) => Some(path.clone()),
            ArtifactRoot::Inline(_) => None,
        };
        if let Some(token) = key.as_ref().and_then(|key| self.by_root.get(key))
            && let Some(artifact) = self.by_token.get_mut(token)
        {
            artifact.opener = opener;
            artifact.reply = reply;
            return Ok(token.clone());
        }
        let token = mint_token()?;
        if let Some(key) = key {
            self.by_root.insert(key, token.clone());
        }
        self.by_token.insert(
            token.clone(),
            Artifact {
                root,
                opener,
                reply,
                served: HashMap::new(),
                replied: None,
            },
        );
        Ok(token)
    }

    pub(super) fn get(&self, token: &str) -> Option<&Artifact> {
        self.by_token.get(token)
    }

    pub(super) fn get_mut(&mut self, token: &str) -> Option<&mut Artifact> {
        self.by_token.get_mut(token)
    }

    pub(super) fn take_reply(&mut self, token: &str, now: Instant) -> Option<(PaneId, String)> {
        let artifact = self.by_token.get_mut(token)?;
        if !artifact.reply
            || artifact
                .replied
                .is_some_and(|replied| now.duration_since(replied) < REPLY_GAP)
        {
            return None;
        }
        let opener = artifact.opener?;
        artifact.replied = Some(now);
        let name = match &artifact.root {
            ArtifactRoot::File(path) | ArtifactRoot::Dir(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ArtifactRoot::Inline(_) => "the page".to_owned(),
        };
        Some((opener, name))
    }

    pub(super) fn watched(&self) -> Vec<(String, Vec<(PathBuf, Option<Stamp>)>)> {
        self.by_token
            .iter()
            .filter(|(_, artifact)| !artifact.served.is_empty())
            .map(|(token, artifact)| {
                (
                    token.clone(),
                    artifact
                        .served
                        .iter()
                        .map(|(path, stamp)| (path.clone(), *stamp))
                        .collect(),
                )
            })
            .collect()
    }
}

fn mint_token() -> io::Result<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes
        .iter()
        .fold(String::with_capacity(32), |mut token, byte| {
            let _ = write!(token, "{byte:02x}");
            token
        }))
}

pub(super) fn artifact_url(token: &str, root: &ArtifactRoot) -> String {
    let name = match root {
        ArtifactRoot::File(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        ArtifactRoot::Dir(_) | ArtifactRoot::Inline(_) => String::new(),
    };
    format!("zz://{token}/{}", encode_path(&name))
}

fn encode_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

pub(super) fn token_of_url(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("zz://")?;
    Some(rest.split(['/', '?', '#']).next().unwrap_or(rest))
}

pub(super) fn resolve(root: &ArtifactRoot, path: &str) -> Result<Target, String> {
    let path = path.trim_start_matches('/');
    match root {
        ArtifactRoot::Inline(bytes) => {
            if path.is_empty() || path == INDEX {
                Ok(Target::Bytes(Arc::clone(bytes)))
            } else {
                Err(format!("{path} is not part of this page"))
            }
        }
        ArtifactRoot::File(file) => {
            let name = file.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if path.is_empty() || path == name {
                Ok(Target::File(file.clone()))
            } else {
                Err(format!(
                    "{name} was opened on its own, so {path} is not served; run zz html on its \
                     directory to serve the files beside it"
                ))
            }
        }
        ArtifactRoot::Dir(dir) => {
            let path = if path.is_empty() || path.ends_with('/') {
                format!("{path}{INDEX}")
            } else {
                path.to_owned()
            };
            let mut joined = dir.clone();
            for part in path.split('/') {
                if part.is_empty() || part.starts_with('.') || part.contains('\\') {
                    return Err(format!("{path} is not served"));
                }
                joined.push(part);
            }
            let real = fs::canonicalize(&joined).map_err(|_| format!("{path} was not found"))?;
            let base = fs::canonicalize(dir).map_err(|_| format!("{} is gone", dir.display()))?;
            let hidden = real.strip_prefix(&base).map_or(true, |inside| {
                inside
                    .components()
                    .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
            });
            if hidden || !real.is_file() {
                return Err(format!("{path} is not served"));
            }
            Ok(Target::File(real))
        }
    }
}

pub(super) fn read_target(target: &Target) -> Result<(Arc<[u8]>, Option<Stamp>), String> {
    match target {
        Target::Bytes(bytes) => Ok((Arc::clone(bytes), None)),
        Target::File(path) => {
            let (bytes, stamp) = read_regular_file(path, u64::from(MAX_ARTIFACT_BYTES))?;
            Ok((bytes.into(), Some(stamp)))
        }
    }
}

fn read_regular_file(path: &Path, limit: u64) -> Result<(Vec<u8>, Stamp), String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK);
    let file = options
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if metadata.len() > limit {
        return Err(format!(
            "{} is {} bytes; a page file may be at most {limit}",
            path.display(),
            metadata.len()
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{} grew past the page file limit", path.display()));
    }
    let stamp = Stamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    };
    Ok((bytes, stamp))
}

fn wire_reason(mut reason: String) -> String {
    if reason.len() > MAX_ARTIFACT_PATH_BYTES {
        let mut end = MAX_ARTIFACT_PATH_BYTES - 3;
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        reason.truncate(end);
        reason.push_str("...");
    }
    reason
}

pub(super) fn mime_for(path: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        None | Some("html" | "htm") => "text/html",
        Some("css") => "text/css",
        Some("js" | "mjs") => "text/javascript",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("wasm") => "application/wasm",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("txt" | "md" | "log") => "text/plain",
        Some("csv") => "text/csv",
        Some("xml") => "application/xml",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some(_) => "application/octet-stream",
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ParsedHtml {
    target: Option<String>,
    float: bool,
    reply: bool,
    path: String,
    payload: Option<String>,
}

fn parse_html_args(args: &[RawText]) -> Result<ParsedHtml, ServerError> {
    let mut parsed = ParsedHtml::default();
    let mut positionals = Vec::new();
    let mut options_done = false;
    let mut boundary_seen = false;
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if !options_done {
            if argument == "--" {
                options_done = true;
                boundary_seen = true;
                index += 1;
                continue;
            }
            if let Some(value) = option_value("html", args, index, &["-t"])? {
                parsed.target = Some(value.value);
                index += value.consumed;
                continue;
            }
            if argument == "--float" {
                parsed.float = true;
                index += 1;
                continue;
            }
            if argument == "--reply" {
                parsed.reply = true;
                index += 1;
                continue;
            }
            if argument.as_bytes().first() == Some(&b'-') && argument != "-" {
                return Err(ServerError::CommandParse(format!(
                    "unexpected html argument: {argument}"
                )));
            }
            options_done = true;
        }
        if argument == "--" && !boundary_seen && positionals == ["-"] {
            boundary_seen = true;
            index += 1;
            continue;
        }
        positionals.push(argument.as_str().to_owned());
        index += 1;
    }
    match positionals.as_slice() {
        [path] => parsed.path.clone_from(path),
        [dash, payload] if dash == "-" => {
            parsed.path.clone_from(dash);
            parsed.payload = Some(payload.clone());
        }
        [] => {
            return Err(ServerError::CommandParse(
                "html needs a file or directory, or - to read the page from stdin".to_owned(),
            ));
        }
        _ => {
            return Err(ServerError::CommandParse(
                "html takes one file or directory".to_owned(),
            ));
        }
    }
    Ok(parsed)
}

#[must_use]
pub fn html_reads_stdin(args: &[RawText]) -> bool {
    parse_html_args(args).is_ok_and(|parsed| parsed.path == "-" && parsed.payload.is_none())
}

fn html_root(parsed: &ParsedHtml, path: &Path) -> Result<ArtifactRoot, ServerError> {
    if let Some(payload) = &parsed.payload {
        return Ok(ArtifactRoot::Inline(Arc::from(payload.as_bytes())));
    }
    let path = fs::canonicalize(path)
        .map_err(|_| ServerError::InvalidCommand(format!("{} was not found", parsed.path)))?;
    if path.is_dir() {
        Ok(ArtifactRoot::Dir(path))
    } else if path.is_file() {
        Ok(ArtifactRoot::File(path))
    } else {
        Err(ServerError::InvalidCommand(format!(
            "{} is not a regular file or directory; write the page to a file, or pipe it to \
             zz html -",
            parsed.path
        )))
    }
}

fn shown_tokens(state: &zz_mux::MuxState) -> HashMap<String, Vec<PaneId>> {
    let mut shown: HashMap<String, Vec<PaneId>> = HashMap::new();
    for window in state.windows.values() {
        for (pane, entry) in &window.panes {
            if let PaneKind::Browser(descriptor) = &entry.kind
                && let Some(token) = token_of_url(descriptor.url())
            {
                shown.entry(token.to_owned()).or_default().push(*pane);
            }
        }
    }
    shown
}

impl Shared {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn html(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &mut ExecutionContext,
        args: &[RawText],
        mux_source: MuxOptionSource,
        invoking_client_terminal: ClientTerminal,
        queue_execution: Option<&CommandQueueExecution>,
        format_facts_unread: Option<bool>,
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_html_args(args)?;
        let source = context.replay_client().unwrap_or(client);
        let (opener, path) = {
            let inner = self.inner.lock();
            (
                inner.client(source).and_then(|client| client.origin),
                buffer_file_path(&inner, Some(source), &parsed.path),
            )
        };
        if parsed.reply && opener.is_none() {
            return Err(ServerError::InvalidCommand(
                "html --reply needs a pane to reply to; run it inside zz".to_owned(),
            )
            .into());
        }
        let root = html_root(&parsed, &path)?;
        let (url, reuse, anchor) = {
            let mut inner = self.inner.lock();
            let target = parsed
                .target
                .as_deref()
                .map(|target| {
                    inner
                        .engine
                        .resolve_pane(Some(target), context.window, context.pane)
                })
                .transpose()?;
            let reuse = target.filter(|pane| {
                matches!(
                    inner.engine.state.pane(*pane).map(|pane| &pane.kind),
                    Some(PaneKind::Browser(descriptor)) if descriptor.profile == ARTIFACT_PROFILE
                )
            });
            let token = inner
                .artifacts
                .register(root.clone(), opener, parsed.reply)
                .map_err(|error| ServerError::InvalidCommand(error.to_string()))?;
            (
                artifact_url(&token, &root),
                reuse,
                target.or(opener).or(context.pane),
            )
        };
        let mut arguments = Vec::new();
        let verb = if let Some(pane) = reuse {
            arguments.extend(["-t".to_owned(), pane.to_string()]);
            "set-browser-url"
        } else {
            arguments.push("-d".to_owned());
            if let Some(anchor) = anchor {
                arguments.extend(["-t".to_owned(), anchor.to_string()]);
            }
            arguments.extend([
                "--kind".to_owned(),
                "browser".to_owned(),
                "--profile".to_owned(),
                ARTIFACT_PROFILE.to_owned(),
            ]);
            if parsed.float {
                "new-pane"
            } else {
                arguments.push("-h".to_owned());
                "split-window"
            }
        };
        arguments.push(url.clone());
        let mut nested = context.clone();
        let execution = self.execute_with_mux_source_inner(
            client,
            kind,
            &mut nested,
            &CommandInvocation::new(verb, arguments),
            mux_source,
            invoking_client_terminal,
            queue_execution,
            format_facts_unread,
        )?;
        let pane = execution.effects.iter().find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(pane.to_string()),
            _ => None,
        });
        self.publish_snapshot();
        let output = match pane {
            Some(pane) => format!("{url} {pane}"),
            None => url,
        };
        Ok(Execution {
            output: output.into(),
            effects: execution.effects,
        })
    }

    pub(super) fn fetch_artifact(
        self: &Arc<Self>,
        client: ClientId,
        request_id: u64,
        token: &str,
        path: &str,
    ) {
        let job = FetchJob {
            owner: Arc::downgrade(self),
            client,
            request_id,
            token: token.to_owned(),
            path: path.to_owned(),
        };
        if let Err(mpsc::SendError(job)) = fetch_worker().send(job) {
            job.serve();
        }
    }

    fn serve_artifact(
        self: &Arc<Self>,
        client: ClientId,
        request_id: u64,
        token: &str,
        path: &str,
    ) {
        let (subscriber, found) = {
            let inner = self.inner.lock();
            let Some(subscriber) = inner
                .client(client)
                .and_then(|client| client.subscriber.as_ref())
                .cloned()
            else {
                return;
            };
            let found = inner
                .artifacts
                .get(token)
                .map(|artifact| (artifact.root.clone(), artifact.reply))
                .ok_or_else(|| "this page is gone; run zz html again".to_owned());
            (subscriber, found)
        };
        let read = found.and_then(|(root, reply)| {
            let target = resolve(&root, path)?;
            read_target(&target).map(|(bytes, stamp)| (target, reply, bytes, stamp))
        });
        let (target, reply, bytes, stamp) = match read {
            Ok(read) => read,
            Err(reason) => {
                let _ = subscriber.enqueue_reliable(&ProtocolMessage::ArtifactUnavailable {
                    request_id,
                    reason: wire_reason(reason),
                });
                return;
            }
        };
        let mime = match &target {
            Target::File(file) => mime_for(&file.to_string_lossy()),
            Target::Bytes(_) => mime_for(INDEX),
        };
        if let (Target::File(file), Some(stamp)) = (target, stamp) {
            if let Some(artifact) = self.inner.lock().artifacts.get_mut(token) {
                artifact.served.insert(file, Some(stamp));
            }
            self.watch_artifacts();
        }
        let total_bytes = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let _ = subscriber.enqueue_reliable(&ProtocolMessage::ArtifactBegin {
            request_id,
            mime: mime.to_owned(),
            total_bytes,
            reply,
        });
        for chunk in bytes.chunks(MAX_ARTIFACT_CHUNK_BYTES) {
            if !wait_for_room(&subscriber) {
                let _ = subscriber.enqueue_reliable(&ProtocolMessage::ArtifactUnavailable {
                    request_id,
                    reason: "zz fell too far behind sending this file".to_owned(),
                });
                return;
            }
            let _ = subscriber.enqueue_reliable(&ProtocolMessage::ArtifactChunk {
                request_id,
                bytes: chunk.to_vec(),
            });
        }
    }

    pub(super) fn reply_from_artifact(self: &Arc<Self>, client: ClientId, token: &str, text: &str) {
        let delivery = {
            let mut inner = self.inner.lock();
            if inner.client_flags.contains(client) {
                return;
            }
            let Some((opener, name)) = inner.artifacts.take_reply(token, Instant::now()) else {
                return;
            };
            ExecutionContext::for_pane(&inner.engine.state, opener).map(|context| {
                (
                    context,
                    CommandInvocation::new(
                        "agent-send",
                        [
                            "--submit".to_owned(),
                            "-t".to_owned(),
                            opener.to_string(),
                            "--".to_owned(),
                            format!("From {name}:\n\n{text}"),
                        ],
                    ),
                )
            })
        };
        let Some((mut context, command)) = delivery else {
            return;
        };
        context.no_hooks = true;
        if let Err(error) = self.execute(client, ClientKind::Interactive, &mut context, &command) {
            log::debug!("could not deliver an artifact reply: {error}");
        }
    }

    fn watch_artifacts(self: &Arc<Self>) {
        {
            let mut inner = self.inner.lock();
            if inner.artifacts_watching {
                return;
            }
            inner.artifacts_watching = true;
        }
        let owner = Arc::downgrade(&self.server_owner());
        if let Err(error) = thread::Builder::new()
            .name("zz-artifacts".to_owned())
            .spawn(move || scan_artifacts(&owner))
        {
            self.inner.lock().artifacts_watching = false;
            log::debug!("could not start the artifact watcher: {error}");
        }
    }

    fn reload_artifact(&self, token: &str, files: Vec<(PathBuf, Option<Stamp>)>) {
        let panes = {
            let mut inner = self.inner.lock();
            if let Some(artifact) = inner.artifacts.get_mut(token) {
                for (path, stamp) in files {
                    artifact.served.insert(path, stamp);
                }
            }
            shown_tokens(&inner.engine.state)
                .remove(token)
                .unwrap_or_default()
        };
        for pane in panes {
            self.publish_for_pane(
                pane,
                &EventPayload::BrowserCommand {
                    pane,
                    command: BrowserCommand::Reload,
                },
            );
        }
    }
}

struct FetchJob {
    owner: Weak<Shared>,
    client: ClientId,
    request_id: u64,
    token: String,
    path: String,
}

impl FetchJob {
    fn serve(self) {
        if let Some(shared) = self.owner.upgrade() {
            shared.serve_artifact(self.client, self.request_id, &self.token, &self.path);
        }
    }
}

fn fetch_worker() -> &'static mpsc::Sender<FetchJob> {
    static WORKER: OnceLock<mpsc::Sender<FetchJob>> = OnceLock::new();
    WORKER.get_or_init(|| {
        let (sender, jobs) = mpsc::channel::<FetchJob>();
        if let Err(error) = thread::Builder::new()
            .name("zz-artifact-fetch".to_owned())
            .spawn(move || {
                for job in jobs {
                    job.serve();
                }
            })
        {
            log::debug!("could not start the artifact fetch thread: {error}");
        }
        sender
    })
}

fn wait_for_room(subscriber: &OutboundMailbox) -> bool {
    let started = Instant::now();
    loop {
        match subscriber.queued_reliable() {
            None => return false,
            Some((bytes, messages))
                if bytes <= SEND_QUEUE_BYTES && messages <= SEND_QUEUE_MESSAGES =>
            {
                return true;
            }
            Some(_) if started.elapsed() >= SEND_WAIT => return false,
            Some(_) => thread::sleep(SEND_POLL),
        }
    }
}

fn scan_artifacts(owner: &Weak<Shared>) {
    let mut pending: HashMap<PathBuf, Option<Stamp>> = HashMap::new();
    loop {
        thread::sleep(SCAN);
        let Some(shared) = owner.upgrade() else {
            return;
        };
        let watched = {
            let mut inner = shared.inner.lock();
            let shown = shown_tokens(&inner.engine.state);
            let watched = inner
                .artifacts
                .watched()
                .into_iter()
                .filter(|(token, _)| shown.contains_key(token))
                .collect::<Vec<_>>();
            if watched.is_empty() {
                inner.artifacts_watching = false;
                return;
            }
            watched
        };
        let mut live = HashSet::new();
        for (token, files) in watched {
            let mut changed = Vec::new();
            for (path, served) in files {
                live.insert(path.clone());
                let now = Stamp::of(&path);
                if now == served {
                    pending.remove(&path);
                } else if pending.get(&path) == Some(&now) {
                    pending.remove(&path);
                    changed.push((path, now));
                } else {
                    pending.insert(path, now);
                }
            }
            if changed.is_empty() {
                continue;
            }
            let target = Arc::downgrade(&shared);
            let _ = shared.timer_tx.send(timers::TimerInput::Callback {
                deadline: None,
                callback: Box::new(move || {
                    if let Some(shared) = target.upgrade() {
                        shared.reload_artifact(&token, changed);
                    }
                }),
            });
        }
        pending.retain(|path, _| live.contains(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("index.html"), "<p>hi</p>").unwrap();
        fs::create_dir(dir.path().join("lib")).unwrap();
        fs::write(dir.path().join("lib/chart.js"), "1").unwrap();
        fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        fs::create_dir(dir.path().join(".ssh")).unwrap();
        fs::write(dir.path().join(".ssh/id"), "key").unwrap();
        dir
    }

    fn args(words: &[&str]) -> Vec<RawText> {
        words.iter().map(|word| RawText::from(*word)).collect()
    }

    #[test]
    fn html_arguments_take_one_path_or_stdin() {
        let parsed = parse_html_args(&args(&["--reply", "-t", "%4", "report.html"])).unwrap();
        assert_eq!(parsed.target.as_deref(), Some("%4"));
        assert!(parsed.reply);
        assert_eq!(parsed.path, "report.html");

        assert!(html_reads_stdin(&args(&["-"])));
        assert!(html_reads_stdin(&args(&["--float", "-"])));
        assert!(!html_reads_stdin(&args(&["page.html"])));
        let piped = parse_html_args(&args(&["-", "--", "<p>hi</p>"])).unwrap();
        assert_eq!(piped.payload.as_deref(), Some("<p>hi</p>"));

        assert!(parse_html_args(&args(&[])).is_err());
        assert!(parse_html_args(&args(&["a.html", "b.html"])).is_err());
        assert!(parse_html_args(&args(&["--float", "-t", "%1", "a.html"])).is_ok());
        assert!(parse_html_args(&args(&["--nope", "a.html"])).is_err());
        assert_eq!(
            parse_html_args(&args(&["--", "-x.html"])).unwrap().path,
            "-x.html"
        );
    }

    #[test]
    fn a_directory_serves_its_files_and_nothing_hidden_or_outside() {
        let dir = page_dir();
        let root = ArtifactRoot::Dir(dir.path().to_path_buf());
        let real = fs::canonicalize(dir.path()).unwrap();

        assert_eq!(
            resolve(&root, "/"),
            Ok(Target::File(real.join("index.html")))
        );
        assert_eq!(
            resolve(&root, "lib/chart.js"),
            Ok(Target::File(real.join("lib/chart.js")))
        );
        for refused in [
            ".env",
            ".ssh/id",
            "lib/../.env",
            "../x",
            "lib//chart.js",
            "lib\\x",
        ] {
            assert!(
                resolve(&root, refused).is_err(),
                "{refused} must be refused"
            );
        }
        assert!(resolve(&root, "lib").is_err(), "a directory is not a file");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_root_is_refused() {
        let dir = page_dir();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), dir.path().join("leak")).unwrap();
        let root = ArtifactRoot::Dir(dir.path().to_path_buf());

        assert!(resolve(&root, "leak").is_err());
    }

    #[test]
    fn a_file_serves_only_itself() {
        let dir = page_dir();
        let file = dir.path().join("index.html");
        let root = ArtifactRoot::File(file.clone());

        assert_eq!(resolve(&root, ""), Ok(Target::File(file.clone())));
        assert_eq!(resolve(&root, "/index.html"), Ok(Target::File(file)));
        assert!(resolve(&root, "lib/chart.js").is_err());
    }

    #[test]
    fn opening_a_path_again_keeps_its_token() {
        let dir = page_dir();
        let mut artifacts = Artifacts::default();
        let file = ArtifactRoot::File(dir.path().join("index.html"));

        let first = artifacts
            .register(file.clone(), Some(PaneId(1)), false)
            .unwrap();
        let again = artifacts.register(file, Some(PaneId(2)), true).unwrap();
        let inline = artifacts
            .register(
                ArtifactRoot::Inline(Arc::from(&b"<p>x</p>"[..])),
                None,
                false,
            )
            .unwrap();

        assert_eq!(first, again);
        assert_eq!(first.len(), 32);
        assert_ne!(first, inline);
        let artifact = artifacts.get(&first).unwrap();
        assert_eq!(artifact.opener, Some(PaneId(2)));
        assert!(artifact.reply);
    }

    #[test]
    fn replies_need_the_flag_an_opener_and_a_second_between_them() {
        let dir = page_dir();
        let mut artifacts = Artifacts::default();
        let page = ArtifactRoot::Dir(dir.path().to_path_buf());
        let quiet = artifacts
            .register(
                ArtifactRoot::File(dir.path().join("index.html")),
                Some(PaneId(1)),
                false,
            )
            .unwrap();
        let orphan = artifacts
            .register(ArtifactRoot::Inline(Arc::from(&b"x"[..])), None, true)
            .unwrap();
        let open = artifacts.register(page, Some(PaneId(7)), true).unwrap();
        let start = Instant::now();

        assert_eq!(artifacts.take_reply(&quiet, start), None);
        assert_eq!(artifacts.take_reply(&orphan, start), None);
        assert_eq!(artifacts.take_reply("unknown", start), None);
        let (opener, _) = artifacts.take_reply(&open, start).unwrap();
        assert_eq!(opener, PaneId(7));
        assert_eq!(artifacts.take_reply(&open, start + REPLY_GAP / 2), None);
        assert!(artifacts.take_reply(&open, start + REPLY_GAP).is_some());
    }

    #[cfg(feature = "agent")]
    #[test]
    fn read_only_artifact_replies_do_not_submit_or_consume_the_reply_gap() {
        let shared = Arc::new(Shared::new(1));
        let token = {
            let mut inner = shared.inner.lock();
            let (_, _, pane) = inner.engine.state.create_session("artifact-reply").unwrap();
            inner
                .artifacts
                .register(
                    ArtifactRoot::Inline(Arc::from(&b"hi"[..])),
                    Some(pane),
                    true,
                )
                .unwrap()
        };
        let mailbox = OutboundMailbox::new();
        let (client, _) = shared.register_subscribed(ClientKind::Interactive, None, None, mailbox);
        let mut context = ExecutionContext::default();
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("attach-session", ["-r", "-t", "artifact-reply"]),
            )
            .unwrap();
        shared.reply_from_artifact(client, &token, "blocked");
        assert!(
            shared
                .inner
                .lock()
                .artifacts
                .get(&token)
                .unwrap()
                .replied
                .is_none()
        );
        shared
            .execute(
                client,
                ClientKind::Interactive,
                &mut context,
                &CommandInvocation::new("switch-client", ["-r"]),
            )
            .unwrap();
        shared.reply_from_artifact(client, &token, "allowed");
        assert!(
            shared
                .inner
                .lock()
                .artifacts
                .get(&token)
                .unwrap()
                .replied
                .is_some()
        );
    }

    #[test]
    fn artifact_reload_watches_a_deleted_file_until_it_returns() {
        let dir = page_dir();
        let path = fs::canonicalize(dir.path().join(INDEX)).unwrap();
        let shared = Arc::new(Shared::new(1));
        let (session, pane, token) = {
            let mut inner = shared.inner.lock();
            let root = ArtifactRoot::File(path.clone());
            let token = inner.artifacts.register(root.clone(), None, false).unwrap();
            inner
                .artifacts
                .get_mut(&token)
                .unwrap()
                .served
                .insert(path.clone(), Stamp::of(&path));
            let (session, _, pane) = inner.engine.state.create_session("artifact-watch").unwrap();
            inner.engine.state.pane_mut(pane).unwrap().kind =
                PaneKind::Browser(zz_protocol::BrowserDescriptor::single(
                    artifact_url(&token, &root),
                    ARTIFACT_PROFILE.to_owned(),
                ));
            inner.artifacts_watching = true;
            (session, pane, token)
        };
        let mailbox = OutboundMailbox::new();
        let (client, _) =
            shared.register_subscribed(ClientKind::Interactive, None, None, Arc::clone(&mailbox));
        shared.attach(client, session).unwrap();
        let inputs = shared.timer_rx.lock().take().unwrap();
        let owner = Arc::downgrade(&shared);
        let watcher = thread::spawn(move || scan_artifacts(&owner));
        let run_callback = |timeout| {
            let deadline = Instant::now() + timeout;
            while let Ok(input) = inputs.recv_deadline(deadline) {
                if let timers::TimerInput::Callback { callback, .. } = input {
                    callback();
                    return true;
                }
            }
            false
        };
        let reloads = || {
            super::super::tests::take_reliable_messages(&mailbox)
                .into_iter()
                .filter(|message| {
                    matches!(message, ProtocolMessage::Event(Event {
                        payload: EventPayload::BrowserCommand {
                            pane: target,
                            command: BrowserCommand::Reload,
                        },
                        ..
                    }) if *target == pane)
                })
                .count()
        };

        fs::remove_file(&path).unwrap();
        assert!(
            run_callback(Duration::from_secs(5)),
            "deletion was not detected"
        );
        assert_eq!(reloads(), 1);
        assert_eq!(
            shared.inner.lock().artifacts.get(&token).unwrap().served[&path],
            None
        );
        assert!(
            !run_callback(SCAN * 3),
            "missing file caused repeated reloads"
        );
        assert_eq!(shared.inner.lock().artifacts.watched().len(), 1);

        fs::write(&path, "<p>rebuilt</p>").unwrap();
        assert!(
            run_callback(Duration::from_secs(5)),
            "recreation was not detected"
        );
        assert_eq!(reloads(), 1);
        assert_eq!(
            shared.inner.lock().artifacts.get(&token).unwrap().served[&path],
            Stamp::of(&path)
        );
        assert!(
            !run_callback(SCAN * 3),
            "unchanged file caused repeated reloads"
        );
        shared
            .inner
            .lock()
            .engine
            .state
            .pane_mut(pane)
            .unwrap()
            .kind = PaneKind::Terminal;
        watcher.join().unwrap();
        assert!(!shared.inner.lock().artifacts_watching);
    }

    #[test]
    fn urls_name_the_file_and_tokens_come_back_out() {
        let root = ArtifactRoot::File(PathBuf::from("/tmp/my report.html"));
        let url = artifact_url("abc", &root);

        assert_eq!(url, "zz://abc/my%20report.html");
        assert_eq!(token_of_url(&url), Some("abc"));
        assert_eq!(token_of_url("zz://abc"), Some("abc"));
        assert_eq!(token_of_url("https://abc/"), None);
    }

    #[test]
    fn mime_types_follow_the_extension() {
        assert_eq!(mime_for(""), "text/html");
        assert_eq!(mime_for("a/b.JS"), "text/javascript");
        assert_eq!(mime_for("x.svg"), "image/svg+xml");
        assert_eq!(mime_for("x.bin"), "application/octet-stream");
    }

    #[cfg(unix)]
    #[test]
    fn special_files_are_refused_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe.html");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );

        let started = Instant::now();
        assert!(read_target(&Target::File(fifo.clone())).is_err());
        assert!(read_target(&Target::File(PathBuf::from("/dev/zero"))).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));

        let parsed = parse_html_args(&args(&["pipe.html"])).unwrap();
        assert!(html_root(&parsed, &fifo).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_root_to_a_hidden_file_is_refused() {
        let dir = page_dir();
        std::os::unix::fs::symlink(dir.path().join(".env"), dir.path().join("env")).unwrap();
        std::os::unix::fs::symlink(dir.path().join(".ssh"), dir.path().join("keys")).unwrap();
        let root = ArtifactRoot::Dir(dir.path().to_path_buf());

        assert!(resolve(&root, "env").is_err());
        assert!(resolve(&root, "keys/id").is_err());
    }

    #[test]
    fn a_directory_path_serves_its_index() {
        let dir = page_dir();
        fs::write(dir.path().join("lib/index.html"), "<p>lib</p>").unwrap();
        let root = ArtifactRoot::Dir(dir.path().to_path_buf());
        let real = fs::canonicalize(dir.path()).unwrap();

        assert_eq!(
            resolve(&root, "lib/"),
            Ok(Target::File(real.join("lib/index.html")))
        );
    }

    #[test]
    fn unavailable_reasons_fit_the_wire() {
        let long = "é".repeat(MAX_ARTIFACT_PATH_BYTES);
        let reason = wire_reason(format!("{long} is not served"));
        assert!(reason.len() <= MAX_ARTIFACT_PATH_BYTES);
        assert!(reason.ends_with("..."));
        assert_eq!(wire_reason("short".to_owned()), "short");
    }

    #[test]
    fn an_oversized_file_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.html");
        let file = fs::File::create(&path).unwrap();
        file.set_len(u64::from(MAX_ARTIFACT_BYTES) + 1).unwrap();

        assert!(read_target(&Target::File(path)).is_err());
    }
}
