use super::*;

impl Shared {
    pub(super) fn inspect(
        &self,
        client: ClientId,
        context: &ExecutionContext,
        args: &[RawText],
    ) -> Result<Execution, DaemonError> {
        let spec = zz_protocol::catalog_command_spec("inspect").expect("inspect spec");
        let options = zz_protocol::parse_tmux_options(spec, args)?;
        spec.validate_positional_maximum(options.positionals.len())?;
        let mut target = None;
        let mut json = false;
        let mut all = false;
        for option in options.options {
            match option {
                zz_protocol::TmuxOption::Value("-t", value) => target = Some(value),
                zz_protocol::TmuxOption::Flag("--json") => json = true,
                zz_protocol::TmuxOption::Flag("-a") => all = true,
                _ => {}
            }
        }
        if all && target.is_some() {
            return Err(ServerError::NativeCommandParse(
                "inspect: -a cannot be combined with -t".to_owned(),
            )
            .into());
        }
        let inner = self.inner.lock();
        let mut panes = Vec::new();
        if all {
            for session in inner.engine.state.sessions.values() {
                let mut windows = session
                    .windows
                    .iter()
                    .map(|id| &inner.engine.state.windows[id])
                    .collect::<Vec<_>>();
                windows.sort_by_key(|window| window.index);
                for window in windows {
                    panes.extend(
                        window
                            .pane_order()
                            .iter()
                            .map(|pane| (session.id, window.id, *pane)),
                    );
                }
            }
        } else {
            let pane = inner
                .engine
                .resolve_pane(target, context.window, context.pane)?;
            let window = inner
                .engine
                .state
                .window_for_pane(pane)
                .expect("pane window");
            panes.push((inner.engine.state.windows[&window].session, window, pane));
        }
        let facts = borrowed_format_hook_facts_for_client(&inner, client, context);
        let mut hooks = DaemonFormatHooks::command_with_optional_variables(&facts, None);
        let mut rows = Vec::new();
        for (session, window, pane) in panes {
            let status = inner.engine.format_status_context_with_format_client(
                Some(session),
                Some(window),
                Some(pane),
                context.session,
                context.format_client(),
            );
            let session_values = status.scoped_format_values("session", &mut hooks);
            let window_values = status.scoped_format_values("window", &mut hooks);
            let pane_values = status.scoped_format_values("pane", &mut hooks);
            let agent = match inner.engine.state.pane(pane).map(|pane| &pane.kind) {
                Some(PaneKind::Agent(agent)) => Some(agent),
                _ => None,
            };
            let mut values = serde_json::Map::new();
            for &key in INSPECT_FIELDS {
                let value = match key {
                    "permission" | "verbs" | "events" => continue,
                    "window_size" => inner.engine.window_size(window).as_str(),
                    "agent_provider" => agent.map_or("", |agent| agent.provider.command_name()),
                    "agent_model" => agent
                        .and_then(|agent| agent.model.as_deref())
                        .unwrap_or_default(),
                    "agent_session_id" => agent
                        .and_then(|agent| agent.session_id.as_deref())
                        .unwrap_or_default(),
                    _ => {
                        let scope = if key.starts_with("session_") {
                            &session_values
                        } else if key.starts_with("window_") {
                            &window_values
                        } else {
                            &pane_values
                        };
                        scope[key].as_str().unwrap_or_default()
                    }
                };
                values.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
            }
            rows.push((pane, values));
        }
        drop(inner);
        let mut output = Vec::new();
        for (pane, mut values) in rows {
            #[cfg(feature = "agent")]
            let (chat, permission) = {
                let state = self
                    .open_agent_runtime()
                    .and_then(|runtime| runtime.wire_state(pane));
                let chat = state.is_some();
                let pending = state.and_then(|state| state.pending_permission);
                if let Some(pending) = &pending {
                    values.insert(
                        "agent_pending_permission".to_owned(),
                        serde_json::Value::String(pending.request_id.to_string()),
                    );
                }
                let permission = pending.map_or(serde_json::Value::Null, |permission| {
                    agent_permission_json(&permission)
                });
                (chat, permission)
            };
            #[cfg(not(feature = "agent"))]
            let (chat, permission) = (false, serde_json::Value::Null);
            values.insert("permission".to_owned(), permission);
            let kind = values["pane_kind"].as_str().unwrap_or_default().to_owned();
            let mut verbs = INSPECT_VERBS
                .iter()
                .find_map(|(name, verbs)| (*name == kind).then_some(*verbs))
                .unwrap_or_default()
                .to_vec();
            if kind == "terminal" && self.terminal_agent(pane).is_some() {
                verbs.push("flip-agent");
            } else if kind == "terminal"
                && !values["agent_state"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty()
            {
                verbs.push("agent-send");
            }
            if chat {
                verbs.push("agent-respond");
            }
            values.insert("verbs".to_owned(), serde_json::json!(verbs));
            let events = INSPECT_EVENTS
                .iter()
                .copied()
                .filter(|event| *event != "agent-tool-call" || chat)
                .collect::<Vec<_>>();
            values.insert("events".to_owned(), serde_json::json!(events));
            let block = if json {
                serde_json::Value::Object(values).to_string()
            } else {
                INSPECT_FIELDS
                    .iter()
                    .filter_map(|key| {
                        let value = &values[*key];
                        let text = if value.is_object() {
                            value.to_string()
                        } else if let Some(items) = value.as_array() {
                            items
                                .iter()
                                .map(|item| item.as_str().unwrap_or_default())
                                .collect::<Vec<_>>()
                                .join(" ")
                        } else {
                            value.as_str().unwrap_or_default().to_owned()
                        };
                        (!text.is_empty()).then(|| format!("{key}: {text}"))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            output.push(block);
        }
        Ok(Execution {
            output: output.join(if json { "\n" } else { "\n\n" }).into(),
            effects: Vec::new(),
        })
    }
}

pub(super) const INSPECT_FIELDS: &[&str] = &[
    "session_id",
    "session_name",
    "window_id",
    "window_index",
    "window_name",
    "window_width",
    "window_height",
    "window_size",
    "pane_id",
    "pane_index",
    "pane_active",
    "pane_kind",
    "pane_pid",
    "pane_current_command",
    "pane_current_path",
    "pane_title",
    "pane_width",
    "pane_height",
    "pane_dead",
    "pane_dead_status",
    "pane_dead_signal",
    "pane_last_command_status",
    "pane_pb_state",
    "pane_pb_progress",
    "pane_status",
    "pane_status_kind",
    "pane_status_progress",
    "pane_status_app",
    "pane_status_title",
    "pane_status_message",
    "pane_status_raw_title",
    "pane_status_raw_message",
    "pane_status_reported",
    "agent_state",
    "agent_pending_permission",
    "agent_provider",
    "agent_model",
    "agent_session_id",
    "agent_parent",
    "permission",
    "browser_url",
    "verbs",
    "events",
];

pub(super) const INSPECT_VERBS: &[(&str, &[&str])] = &[
    (
        "terminal",
        &[
            "send-keys",
            "send-text",
            "paste-buffer",
            "capture-pane",
            "show-last-output",
            "send-last-output",
            "wait-pane",
            "run-pane",
            "pipe-pane",
            "respawn-pane",
            "resize-pane",
            "kill-pane",
        ],
    ),
    (
        "browser",
        &[
            "set-browser-url",
            "load-browser-url",
            "set-browser-profile",
            "capture-browser",
            "html",
            "resize-pane",
            "kill-pane",
        ],
    ),
    ("editor", &["resize-pane", "kill-pane"]),
    ("view", &["view-pane", "resize-pane", "kill-pane"]),
    (
        "simulator",
        &[
            "set-simulator-device",
            "capture-simulator",
            "tap-simulator",
            "swipe-simulator",
            "type-simulator",
            "press-simulator",
            "resize-pane",
            "kill-pane",
        ],
    ),
    ("picker", &["resize-pane", "kill-pane", "select-pane-kind"]),
    (
        "agent",
        &[
            "agent-send",
            "capture-pane",
            "flip-agent",
            "agent-account",
            "resize-pane",
            "kill-pane",
        ],
    ),
];

const INSPECT_EVENTS: &[&str] = &[
    "pane-died",
    "pane-exited",
    "pane-focus-in",
    "pane-focus-out",
    "pane-mode-changed",
    "pane-set-clipboard",
    "pane-title-changed",
    "alert-activity",
    "alert-bell",
    "alert-silence",
    "@option-changed",
    "agent-state-changed",
    "agent-tool-call",
];
