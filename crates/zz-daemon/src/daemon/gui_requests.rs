use super::*;

impl Shared {
    pub(super) fn load_browser_url(
        self: &Arc<Self>,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_load_browser_url_args(args)?;
        let (pane, before) = {
            let mut inner = self.inner.lock();
            let pane = inner.engine.resolve_pane(
                parsed.target.as_deref(),
                context.window,
                context.pane,
            )?;
            let before = BrowserLoad::read(&inner, pane).map_or(0, |load| load.finished);
            inner.engine.state.update_browser_url(pane, parsed.url)?;
            (pane, before)
        };
        self.publish_snapshot();
        let (wait, reply) =
            agent_waits::reply::<Result<(), DaemonError>>(self, None, move |_, outcome| {
                match outcome {
                    terminal_requests::ReplyOutcome::Ready(Ok(())) => Ok(Execution::default()),
                    terminal_requests::ReplyOutcome::Ready(Err(error)) => Err(error),
                    _ => Err(ServerError::Internal(format!(
                        "load-browser-url: the wait on {pane} was dropped"
                    ))
                    .into()),
                }
            });
        let shared = Arc::downgrade(self);
        let timeout = parsed.timeout.unwrap_or(BROWSER_LOAD_TIMEOUT);
        thread::Builder::new()
            .name("zz-browser-load".to_owned())
            .spawn(move || {
                let started = Instant::now();
                let mut loaded = None;
                loop {
                    let Some(shared) = shared.upgrade() else {
                        return;
                    };
                    let load = BrowserLoad::read(&shared.inner.lock(), pane);
                    drop(shared);
                    match load_outcome(pane, load, before, started.elapsed(), timeout) {
                        Some(Ok(())) => {
                            if loaded.get_or_insert_with(Instant::now).elapsed()
                                >= BROWSER_LOAD_SETTLE
                            {
                                reply.try_send(Ok(()));
                                return;
                            }
                        }
                        Some(error) => {
                            reply.try_send(error);
                            return;
                        }
                        None => loaded = None,
                    }
                    thread::sleep(BROWSER_LOAD_POLL);
                }
            })
            .map_err(|error| DaemonError::Thread(error.to_string()))?;
        self.report_command_queue_park();
        wait.finish(self, Execution::default())
    }

    pub(super) fn capture_browser(
        self: &Arc<Self>,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let parsed = parse_capture_browser_args(args)?;
        let output = parsed.output.ok_or_else(|| {
            ServerError::CommandParse("capture-browser needs an output path (-o)".to_owned())
        })?;
        let path = expand_path(&output);
        if !path.is_absolute() {
            return Err(ServerError::InvalidCommand(format!(
                "capture-browser output path must be absolute: {output}"
            ))
            .into());
        }
        let path = path
            .to_str()
            .ok_or_else(|| {
                ServerError::InvalidCommand(
                    "capture-browser output path is not valid UTF-8".to_owned(),
                )
            })?
            .to_owned();
        let pane = {
            let inner = self.inner.lock();
            let pane = inner.engine.resolve_pane(
                parsed.target.as_deref(),
                context.window,
                context.pane,
            )?;
            if !matches!(
                inner.engine.state.pane(pane).map(|pane| &pane.kind),
                Some(PaneKind::Browser(_))
            ) {
                return Err(
                    ServerError::InvalidTarget(format!("{pane} is not a browser pane")).into(),
                );
            }
            pane
        };
        self.request_from_gui(
            pane,
            |missing| {
                format!(
                    "capture-browser: {missing}, so {pane} is not rendering. Open that session in \
                     the zz app, then retry."
                )
            },
            Some(path.clone()),
            |request_id| EventPayload::BrowserCommand {
                pane,
                command: BrowserCommand::Screenshot { request_id, path },
            },
        )
    }

    fn next_gui_request_id() -> u64 {
        static REQUESTS: AtomicU64 = AtomicU64::new(1);
        REQUESTS.fetch_add(1, Ordering::Relaxed)
    }

    pub(super) fn request_from_gui(
        self: &Arc<Self>,
        pane: PaneId,
        unavailable: impl FnOnce(&str) -> String,
        screenshot: Option<String>,
        payload: impl FnOnce(u64) -> EventPayload,
    ) -> Result<Execution, DaemonError> {
        let request_id = Self::next_gui_request_id();
        let (client, subscriber) = {
            let inner = self.inner.lock();
            let window = inner
                .engine
                .state
                .window_for_pane(pane)
                .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
            let session = inner.engine.state.windows[&window].session;
            gui_client(&inner, session).ok_or_else(|| {
                let name = inner
                    .engine
                    .state
                    .sessions
                    .get(&session)
                    .map_or_else(|| session.to_string(), |session| session.name.clone());
                ServerError::InvalidCommand(unavailable(&format!(
                    "no zz window shows session {name} on {}",
                    inner.engine.format_host_short()
                )))
            })?
        };
        let asked = self
            .inner
            .lock()
            .client(client)
            .and_then(|client| client.name.clone())
            .map_or_else(
                || "the zz window".to_owned(),
                |name| format!("the zz window on {name}"),
            );
        let (wait, reply) =
            agent_waits::reply::<Result<String, String>>(self, None, move |shared, outcome| {
                shared
                    .inner
                    .lock()
                    .transfers
                    .pending_gui_requests
                    .remove(&request_id);
                match outcome {
                    terminal_requests::ReplyOutcome::Ready(Ok(output)) => Ok(Execution {
                        output: output.into(),
                        effects: Vec::new(),
                    }),
                    terminal_requests::ReplyOutcome::Ready(Err(message)) => {
                        Err(ServerError::InvalidCommand(message).into())
                    }
                    _ => Err(ServerError::Internal(format!(
                        "{asked} did not answer within {} seconds",
                        GUI_REQUEST_TIMEOUT.as_secs()
                    ))
                    .into()),
                }
            });
        self.inner.lock().transfers.pending_gui_requests.insert(
            request_id,
            PendingGuiRequest {
                client,
                reply,
                screenshot: screenshot.map(|path| PendingScreenshot {
                    path,
                    png: Vec::new(),
                }),
            },
        );
        self.expire_stalled_gui_request(request_id, 0);
        if !subscriber.enqueue_reliable(&Self::event(payload(request_id))) {
            self.inner
                .lock()
                .transfers
                .pending_gui_requests
                .remove(&request_id);
            wait.state
                .resolve(Err(ServerError::PaneNotAttached(pane).into()));
            return wait.finish(self, Execution::default());
        }
        self.report_command_queue_park();
        wait.finish(self, Execution::default())
    }

    fn expire_stalled_gui_request(self: &Arc<Self>, request_id: u64, received: usize) {
        self.terminal_requests
            .schedule(Instant::now() + GUI_REQUEST_TIMEOUT, move |shared| {
                let stalled = {
                    let mut inner = shared.inner.lock();
                    let Some(pending) = inner.transfers.pending_gui_requests.get(&request_id)
                    else {
                        return;
                    };
                    let now = pending
                        .screenshot
                        .as_ref()
                        .map_or(0, |screenshot| screenshot.png.len());
                    if now > received {
                        drop(inner);
                        shared.expire_stalled_gui_request(request_id, now);
                        return;
                    }
                    inner.transfers.pending_gui_requests.remove(&request_id)
                };
                drop(stalled);
            });
    }

    fn take_gui_request(&self, client: ClientId, request_id: u64) -> Option<PendingGuiRequest> {
        let mut inner = self.inner.lock();
        owned_gui_request(&mut inner, client, request_id)?;
        inner.transfers.pending_gui_requests.remove(&request_id)
    }

    pub(super) fn complete_gui_request(&self, client: ClientId, response: GuiResponse) {
        let Some(pending) = self.take_gui_request(client, response.request_id()) else {
            return;
        };
        pending.reply.try_send(match response {
            GuiResponse::Success { output, .. } => Ok(output),
            GuiResponse::Error { message, .. } => Err(message),
        });
    }

    pub(super) fn receive_browser_screenshot(
        &self,
        client: ClientId,
        request_id: u64,
        last: bool,
        png: &[u8],
    ) {
        let pending = {
            let mut inner = self.inner.lock();
            let Some(pending) = owned_gui_request(&mut inner, client, request_id) else {
                return;
            };
            if let Some(screenshot) = &mut pending.screenshot {
                screenshot.png.extend_from_slice(png);
                if !last && screenshot.png.len() <= zz_protocol::MAX_BROWSER_SCREENSHOT_BYTES {
                    return;
                }
            }
            inner.transfers.pending_gui_requests.remove(&request_id)
        };
        let Some(pending) = pending else {
            return;
        };
        let Some(PendingScreenshot { path, png }) = pending.screenshot else {
            pending.reply.try_send(Err(
                "the zz window sent a screenshot for another request".to_owned()
            ));
            return;
        };
        let written = std::thread::Builder::new()
            .name("zz-browser-screenshot".to_owned())
            .spawn(move || {
                let written = write_browser_screenshot(Path::new(&path), &png).map(|()| path);
                pending.reply.try_send(written);
            });
        if let Err(error) = written {
            log::warn!("could not write a browser screenshot: {error}");
        }
    }

    pub(super) fn fail_gui_requests_for(&self, client: ClientId) {
        let pending = {
            let mut inner = self.inner.lock();
            let ids = inner
                .transfers
                .pending_gui_requests
                .iter()
                .filter_map(|(id, request)| (request.client == client).then_some(*id))
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| inner.transfers.pending_gui_requests.remove(&id))
                .collect::<Vec<_>>()
        };
        for request in pending {
            request
                .reply
                .try_send(Err("the zz window disconnected".to_owned()));
        }
    }
}

const GUI_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn write_browser_screenshot(path: &Path, png: &[u8]) -> Result<(), String> {
    if png.len() > zz_protocol::MAX_BROWSER_SCREENSHOT_BYTES {
        return Err(format!(
            "the zz window sent a screenshot larger than {} MiB",
            zz_protocol::MAX_BROWSER_SCREENSHOT_BYTES / (1024 * 1024)
        ));
    }
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("the zz window sent a screenshot that is not a PNG".to_owned());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.is_dir()
    {
        return Err(format!("{} is not a directory", parent.display()));
    }
    std::fs::write(path, png)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn owned_gui_request(
    inner: &mut ServerState,
    client: ClientId,
    request_id: u64,
) -> Option<&mut PendingGuiRequest> {
    let pending = inner.transfers.pending_gui_requests.get_mut(&request_id)?;
    if pending.client != client {
        log::warn!(target: "zz_daemon::diagnostics::connection", "discarding GUI response for request={request_id} from client={client}; owner is {}", pending.client);
        return None;
    }
    Some(pending)
}

const BROWSER_LOAD_TIMEOUT: Duration = Duration::from_secs(30);

const BROWSER_LOAD_GRACE: Duration = Duration::from_millis(1500);

const BROWSER_LOAD_POLL: Duration = Duration::from_millis(50);

const BROWSER_LOAD_SETTLE: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BrowserLoad {
    pub(super) reported: bool,
    pub(super) loading: bool,
    pub(super) finished: u64,
}

impl BrowserLoad {
    pub(super) fn read(inner: &ServerState, pane: PaneId) -> Option<Self> {
        let browser = matches!(
            inner.engine.state.pane(pane).map(|pane| &pane.kind),
            Some(PaneKind::Browser(_))
        );
        let loading = inner.engine.pane_user_option(pane, "@browser_loading");
        browser.then(|| Self {
            reported: loading.is_some(),
            loading: loading == Some("1"),
            finished: inner
                .engine
                .pane_user_option(pane, "@browser_loaded")
                .and_then(|count| count.parse().ok())
                .unwrap_or(0),
        })
    }
}

pub(super) fn load_outcome(
    pane: PaneId,
    load: Option<BrowserLoad>,
    before: u64,
    waited: Duration,
    timeout: Duration,
) -> Option<Result<(), DaemonError>> {
    let Some(load) = load else {
        return Some(Err(ServerError::InvalidCommand(format!(
            "{pane} is no longer a browser pane"
        ))
        .into()));
    };
    if !load.loading && (load.finished != before || waited >= BROWSER_LOAD_GRACE) {
        return Some(if load.reported {
            Ok(())
        } else {
            Err(ServerError::InvalidCommand(format!(
                "{pane} went to the page, but no window reported its load: open its session in \
                 a zz app that is up to date"
            ))
            .into())
        });
    }
    (!timeout.is_zero() && waited >= timeout).then(|| {
        Err(DaemonError::CommandExit {
            output: format!(
                "load-browser-url: {pane} was still loading after {} seconds\n",
                timeout.as_secs_f32()
            )
            .into(),
            exit_code: 124,
        })
    })
}

#[derive(Debug)]
pub(super) struct ParsedLoadBrowserUrl {
    pub(super) target: Option<String>,
    pub(super) timeout: Option<Duration>,
    pub(super) url: String,
}

pub(super) fn parse_load_browser_url_args(
    args: &[RawText],
) -> Result<ParsedLoadBrowserUrl, ServerError> {
    let usage = || {
        ServerError::CommandParse("usage: load-browser-url [-t %N] [--timeout SECS] URL".to_owned())
    };
    let (mut target, mut timeout, mut url) = (None, None, None);
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if let Some(value) = option_value("load-browser-url", args, index, &["-t"])? {
            target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("load-browser-url", args, index, &["--timeout"])? {
            timeout = Some(parse_pane_timeout("load-browser-url", &value.value)?);
            index += value.consumed;
            continue;
        }
        let rest = if argument == "--" {
            &args[index + 1..]
        } else {
            &args[index..]
        };
        let [word] = rest else {
            return Err(usage());
        };
        url = Some(word.to_string());
        break;
    }
    Ok(ParsedLoadBrowserUrl {
        target,
        timeout,
        url: url.ok_or_else(usage)?,
    })
}

#[derive(Debug, Default)]
pub(super) struct ParsedCaptureBrowser {
    pub(super) target: Option<String>,
    pub(super) output: Option<String>,
}

pub(super) fn parse_capture_browser_args(
    args: &[RawText],
) -> Result<ParsedCaptureBrowser, ServerError> {
    let mut parsed = ParsedCaptureBrowser::default();
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        if argument == "--" {
            if index.saturating_add(1) != args.len() {
                return Err(ServerError::CommandParse(
                    "capture-browser does not accept positional arguments".to_owned(),
                ));
            }
            break;
        }
        if let Some(value) = option_value("capture-browser", args, index, &["-t"])? {
            parsed.target = Some(value.value);
            index += value.consumed;
            continue;
        }
        if let Some(value) = option_value("capture-browser", args, index, &["-o"])? {
            parsed.output = Some(value.value);
            index += value.consumed;
            continue;
        }
        return Err(ServerError::CommandParse(format!(
            "unexpected capture-browser argument: {argument}"
        )));
    }
    Ok(parsed)
}
