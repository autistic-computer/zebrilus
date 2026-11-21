use std::{
    env,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, ExitCode},
    thread,
    time::Duration,
};

use agent_client_protocol::{
    schema::ProtocolVersion,
    v1::schema::{
        AgentCapabilities, Content, ContentBlock, ContentChunk, Implementation, InitializeResponse,
        LoadSessionResponse, NewSessionResponse, PermissionOption, PermissionOptionId,
        PermissionOptionKind, PromptResponse, RequestPermissionRequest, SessionConfigOption,
        SessionConfigOptionCategory, SessionConfigSelectOption, SessionId, SessionNotification,
        SessionUpdate, SetSessionConfigOptionResponse, SetSessionModeResponse, StopReason,
        TextContent, ToolCall, ToolCallContent, ToolCallId, ToolCallStatus, ToolCallUpdate,
        ToolCallUpdateFields, ToolKind,
    },
};
use serde_json::{Value, json};
use zz_daemon_client::unmasked::SpawnUnmasked as _;

struct State {
    provider: String,
    model: String,
    effort: String,
    mode: String,
    cwd: PathBuf,
    session_counter: u64,
    tool_counter: u64,
    request_counter: u64,
}

impl State {
    fn new(provider: String) -> Self {
        let model = match provider.as_str() {
            "codex" => "gpt-5.4",
            "agy" => "gemini-3-pro",
            "grok" => "grok-4",
            _ => "claude-sonnet-4-6",
        }
        .to_owned();
        Self {
            provider,
            model,
            effort: "medium".to_owned(),
            mode: "acceptEdits".to_owned(),
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from("/home/user/demo")),
            session_counter: 0,
            tool_counter: 0,
            request_counter: 100,
        }
    }

    fn config_options(&self) -> Vec<SessionConfigOption> {
        let models = match self.provider.as_str() {
            "codex" => vec![
                SessionConfigSelectOption::new("gpt-5.4", "GPT-5.4"),
                SessionConfigSelectOption::new("gpt-5.4-mini", "GPT-5.4 Mini"),
            ],
            "agy" => vec![
                SessionConfigSelectOption::new("gemini-3-pro", "Gemini 3 Pro"),
                SessionConfigSelectOption::new("gemini-3-flash", "Gemini 3 Flash"),
            ],
            _ => vec![
                SessionConfigSelectOption::new("claude-sonnet-4-6", "Claude Sonnet 4.6"),
                SessionConfigSelectOption::new("claude-opus-4-7", "Claude Opus 4.7"),
            ],
        };
        let efforts = vec![
            SessionConfigSelectOption::new("low", "Low"),
            SessionConfigSelectOption::new("medium", "Medium"),
            SessionConfigSelectOption::new("high", "High"),
        ];
        let approval_modes = vec![
            SessionConfigSelectOption::new("default", "Ask"),
            SessionConfigSelectOption::new("acceptEdits", "Accept edits"),
            SessionConfigSelectOption::new("plan", "Plan"),
        ];
        vec![
            SessionConfigOption::select("model", "Model", self.model.clone(), models)
                .category(SessionConfigOptionCategory::Model),
            SessionConfigOption::select("effort", "Effort", self.effort.clone(), efforts)
                .category(SessionConfigOptionCategory::ThoughtLevel),
            SessionConfigOption::select("mode", "Mode", self.mode.clone(), approval_modes)
                .category(SessionConfigOptionCategory::Mode),
        ]
    }
}

fn send_json(out: &mut impl Write, value: &Value) {
    if let Ok(line) = serde_json::to_string(value) {
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}

fn send_result(out: &mut impl Write, id: &Value, result: &Value) {
    send_json(
        out,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
    );
}

fn send_notification(out: &mut impl Write, method: &str, params: &Value) {
    send_json(
        out,
        &json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }),
    );
}

fn send_update(out: &mut impl Write, session_id: &str, update: SessionUpdate) {
    let notif = SessionNotification::new(SessionId::new(session_id.to_owned()), update);
    if let Ok(params) = serde_json::to_value(notif) {
        send_notification(out, "session/update", &params);
    }
}

fn stream_thought(out: &mut impl Write, session_id: &str, text: &str) {
    send_update(
        out,
        session_id,
        SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
            text,
        )))),
    );
}

fn stream_text(out: &mut impl Write, session_id: &str, text: &str) {
    for chunk in text.split_inclusive('\n') {
        send_update(
            out,
            session_id,
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(chunk),
            ))),
        );
        thread::sleep(Duration::from_millis(12));
    }
}

fn ask_permission(
    reader: &mut BufReader<io::StdinLock<'_>>,
    out: &mut impl Write,
    state: &mut State,
    session_id: &str,
    tool_id: &str,
    title: &str,
    kind: ToolKind,
) -> bool {
    state.request_counter += 1;
    let req_id = state.request_counter;
    let tool = ToolCallUpdate::new(
        ToolCallId::new(tool_id.to_owned()),
        ToolCallUpdateFields::new()
            .status(ToolCallStatus::Pending)
            .title(title.to_owned())
            .kind(kind),
    );
    let req = RequestPermissionRequest::new(
        SessionId::new(session_id.to_owned()),
        tool,
        vec![
            PermissionOption::new(
                PermissionOptionId::new("allow"),
                "Allow",
                PermissionOptionKind::AllowOnce,
            ),
            PermissionOption::new(
                PermissionOptionId::new("deny"),
                "Deny",
                PermissionOptionKind::RejectOnce,
            ),
        ],
    );
    if let Ok(params) = serde_json::to_value(req) {
        send_json(
            out,
            &json!({
                "jsonrpc": "2.0",
                "id": req_id,
                "method": "session/request_permission",
                "params": params,
            }),
        );
    }
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if let Ok(msg) = serde_json::from_str::<Value>(&line)
            && msg.get("id").and_then(Value::as_u64) == Some(req_id)
        {
            let outcome = &msg["result"]["outcome"];
            let option_id = outcome["optionId"].as_str().unwrap_or("");
            return option_id == "allow"
                || (outcome["outcome"].as_str() == Some("selected") && option_id != "deny");
        }
        line.clear();
    }
    false
}

fn run_shell_tool(
    reader: &mut BufReader<io::StdinLock<'_>>,
    out: &mut impl Write,
    state: &mut State,
    session_id: &str,
    title: &str,
    kind: ToolKind,
    cmd: &str,
    needs_approval: bool,
) -> String {
    state.tool_counter += 1;
    let id = format!("tool-{}", state.tool_counter);
    let call = ToolCall::new(ToolCallId::new(id.clone()), title.to_owned())
        .kind(kind)
        .status(ToolCallStatus::InProgress)
        .raw_input(json!({ "command": cmd }));
    send_update(out, session_id, SessionUpdate::ToolCall(call));

    if needs_approval && state.mode == "default" {
        let allowed = ask_permission(reader, out, state, session_id, &id, title, kind);
        if !allowed {
            let update = ToolCallUpdate::new(
                ToolCallId::new(id),
                ToolCallUpdateFields::new()
                    .status(ToolCallStatus::Failed)
                    .content(vec![ToolCallContent::Content(Content::new(
                        ContentBlock::Text(TextContent::new("Permission denied by user.")),
                    ))]),
            );
            send_update(out, session_id, SessionUpdate::ToolCallUpdate(update));
            return "Permission denied by user.".to_owned();
        }
    }

    let output = Command::new("/bin/bash")
        .arg("-lc")
        .arg(cmd)
        .current_dir(&state.cwd)
        .output_unmasked();

    let (ok, text) = match output {
        Ok(out) => {
            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.is_empty() {
                if !combined.is_empty() && !combined.ends_with('\n') {
                    combined.push('\n');
                }
                combined.push_str(&err);
            }
            if combined.trim().is_empty() {
                combined = format!("(exit {})", out.status.code().unwrap_or(0));
            }
            (out.status.success(), combined)
        }
        Err(error) => (false, format!("failed to execute: {error}")),
    };

    let trimmed: String = text.chars().take(4000).collect();
    let update = ToolCallUpdate::new(
        ToolCallId::new(id),
        ToolCallUpdateFields::new()
            .status(if ok {
                ToolCallStatus::Completed
            } else {
                ToolCallStatus::Failed
            })
            .content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new(trimmed.clone())),
            ))]),
    );
    send_update(out, session_id, SessionUpdate::ToolCallUpdate(update));
    trimmed
}

fn prompt_text(params: &Value) -> String {
    params["prompt"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| block["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn handle_prompt(
    reader: &mut BufReader<io::StdinLock<'_>>,
    out: &mut impl Write,
    state: &mut State,
    session_id: &str,
    prompt: &str,
) {
    let lower = prompt.to_ascii_lowercase();

    if let Some(cmd) = prompt
        .strip_prefix('!')
        .or_else(|| prompt.strip_prefix("run "))
        .or_else(|| prompt.strip_prefix("Run "))
    {
        let cmd = cmd.trim();
        stream_thought(
            out,
            session_id,
            &format!("Running `{cmd}` inside the live unkernel Linux VM."),
        );
        let result = run_shell_tool(
            reader,
            out,
            state,
            session_id,
            &format!("bash: {cmd}"),
            ToolKind::Execute,
            cmd,
            true,
        );
        stream_text(
            out,
            session_id,
            &format!(
                "Ran `{cmd}` in the live Linux 7.3.4 Wasm guest:\n\n```text\n{}\n```\n",
                result.trim()
            ),
        );
        return;
    }

    if lower.contains("diagram") || lower.contains("mermaid") || lower.contains("arch") {
        stream_thought(
            out,
            session_id,
            "Inspecting live kernel and rendering the zz + unkernel architecture diagram.",
        );
        let uname = run_shell_tool(
            reader,
            out,
            state,
            session_id,
            "uname -srm && zz list-panes -a",
            ToolKind::Read,
            "uname -srm && zz list-panes -a",
            false,
        );
        let reply = format!(
            "Everything in this tab is running locally in WebAssembly—no remote server involved:\n\n\
```mermaid\n\
flowchart LR\n\
    UI[\"zz Web Client (ala + WebGPU)\"] <-->|\"zz_protocol frames\"| Bridge[\"JS Stream Bridge\"]\n\
    Bridge <-->|\"kwvsock / zz proxy\"| Daemon[\"zz-daemon (aarch64 musl)\"]\n\
    subgraph VM[\"unkernel (Linux 7.3.4 Wasm)\"]\n\
        Daemon --> VT[\"libghostty-vt + real PTYs\"]\n\
        VT --> Shell[\"bash / git / jq / htop\"]\n\
        Daemon <-->|\"ACP JSON-RPC\"| Agent[\"Showcase Agent\"]\n\
    end\n\
```\n\n\
Live guest snapshot:\n```text\n{}\n```\n\
Click the diagram to pop it into PiP, or ask me to `split a pane`, `check git status`, or `run <command>`.\n",
            uname.trim()
        );
        stream_text(out, session_id, &reply);
        return;
    }

    if lower.contains("split") || lower.contains("new pane") || lower.contains("new window") {
        let is_window = lower.contains("window");
        let cmd = if is_window {
            "zz new-window -n agent-scratch -c /home/user/demo && zz list-windows"
        } else {
            "zz split-window -v -c /home/user/demo && zz list-panes -a"
        };
        stream_thought(
            out,
            session_id,
            "Invoking the real `zz` CLI inside the unkernel guest to mutate the live multiplexer layout.",
        );
        let result = run_shell_tool(
            reader,
            out,
            state,
            session_id,
            cmd,
            ToolKind::Execute,
            cmd,
            true,
        );
        stream_text(
            out,
            session_id,
            &format!(
                "Done—the real `zz-daemon` inside unkernel updated your live layout:\n\n```text\n{}\n```\n",
                result.trim()
            ),
        );
        return;
    }

    if lower.contains("git") || lower.contains("commit") || lower.contains("diff") {
        stream_thought(
            out,
            session_id,
            "Checking the real git repository inside /home/user/demo.",
        );
        let result = run_shell_tool(
            reader,
            out,
            state,
            session_id,
            "git status -sb && git log --oneline -n 5",
            ToolKind::Read,
            "git status -sb && git log --oneline -n 5",
            false,
        );
        stream_text(
            out,
            session_id,
            &format!(
                "Live repository state in `{}`:\n\n```text\n{}\n```\n",
                state.cwd.display(),
                result.trim()
            ),
        );
        return;
    }

    if lower.contains("browser") {
        stream_thought(
            out,
            session_id,
            "Opening a showcase browser pane via `zz split-window --kind browser`.",
        );
        let result = run_shell_tool(
            reader,
            out,
            state,
            session_id,
            "zz split-window -h --kind browser http://localhost:3000",
            ToolKind::Execute,
            "zz split-window -h --kind browser http://localhost:3000",
            true,
        );
        stream_text(
            out,
            session_id,
            &format!(
                "Opened a live browser pane in your workspace (`{}`).\n",
                result.trim()
            ),
        );
        return;
    }

    stream_thought(
        out,
        session_id,
        "Inspecting the live Alpine Linux guest environment and workspace.",
    );
    let info = run_shell_tool(
        reader,
        out,
        state,
        session_id,
        "uname -srm && ls -la",
        ToolKind::Read,
        "uname -srm && ls -la",
        false,
    );
    let reply = format!(
        "I'm a local showcase agent running over **real ACP JSON-RPC** inside the `unkernel` Linux 7.3.4 Wasm VM (model: `{}`), talking to the real `zz-daemon`.\n\n\
Here's what I just read live from `{}`:\n\n\
```text\n{}\n```\n\n\
Try asking me to:\n\
- **draw an architecture diagram** (renders live Mermaid via `pele-rs`)\n\
- **split a pane** or **open a browser pane** (drives the real `zz` CLI)\n\
- **check git status** (runs real `git` in `~/demo`)\n\
- **`run <any shell command>`** (executes live in `bash`—switch mode to **Ask** to test interactive permission cards!)\n",
        state.model,
        state.cwd.display(),
        info.trim()
    );
    stream_text(out, session_id, &reply);
}

pub fn run(provider: String) -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut out = stdout.lock();
    let mut state = State::new(provider);

    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(trimmed) else {
            line.clear();
            continue;
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned();
        let params = msg.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => {
                if let Some(id) = id {
                    let version = params
                        .get("protocolVersion")
                        .and_then(|v| serde_json::from_value::<ProtocolVersion>(v.clone()).ok())
                        .unwrap_or(ProtocolVersion::V1);
                    let resp = InitializeResponse::new(version)
                        .agent_capabilities(AgentCapabilities::new().load_session(true))
                        .agent_info(Implementation::new("zz-showcase-agent", "0.29.0"));
                    if let Ok(val) = serde_json::to_value(resp) {
                        send_result(&mut out, &id, &val);
                    }
                }
            }
            "session/new" => {
                if let Some(id) = id {
                    if let Some(cwd) = params.get("cwd").and_then(Value::as_str)
                        && !cwd.is_empty()
                    {
                        state.cwd = PathBuf::from(cwd);
                    }
                    state.session_counter += 1;
                    let session_id = format!("zz-showcase-{}", state.session_counter);
                    let resp = NewSessionResponse::new(SessionId::new(session_id))
                        .config_options(state.config_options());
                    if let Ok(val) = serde_json::to_value(resp) {
                        send_result(&mut out, &id, &val);
                    }
                }
            }
            "session/load" => {
                if let Some(id) = id {
                    let resp = LoadSessionResponse::new().config_options(state.config_options());
                    if let Ok(val) = serde_json::to_value(resp) {
                        send_result(&mut out, &id, &val);
                    }
                }
            }
            "session/set_config_option" => {
                if let Some(id) = id {
                    let config_id = params["configId"].as_str().unwrap_or("");
                    let value = params["value"].as_str().unwrap_or("");
                    match config_id {
                        "model" if !value.is_empty() => value.clone_into(&mut state.model),
                        "effort" | "reasoning_effort" | "thought_level" if !value.is_empty() => {
                            value.clone_into(&mut state.effort);
                        }
                        "mode" if !value.is_empty() => value.clone_into(&mut state.mode),
                        _ => {}
                    }
                    let resp = SetSessionConfigOptionResponse::new(state.config_options());
                    if let Ok(val) = serde_json::to_value(resp) {
                        send_result(&mut out, &id, &val);
                    }
                }
            }
            "session/set_mode" => {
                if let Some(id) = id {
                    if let Some(mode_id) = params["modeId"].as_str()
                        && !mode_id.is_empty()
                    {
                        mode_id.clone_into(&mut state.mode);
                    }
                    let resp = SetSessionModeResponse::new();
                    if let Ok(val) = serde_json::to_value(resp) {
                        send_result(&mut out, &id, &val);
                    }
                }
            }
            "session/prompt" => {
                if let Some(id) = id {
                    let session_id = params["sessionId"]
                        .as_str()
                        .unwrap_or("zz-showcase-1")
                        .to_owned();
                    let text = prompt_text(&params);
                    handle_prompt(&mut reader, &mut out, &mut state, &session_id, &text);
                    let resp = PromptResponse::new(StopReason::EndTurn);
                    if let Ok(val) = serde_json::to_value(resp) {
                        send_result(&mut out, &id, &val);
                    }
                }
            }
            "session/cancel" => {}
            _ => {
                if let Some(id) = id {
                    send_result(&mut out, &id, &json!({}));
                }
            }
        }
        line.clear();
    }
    ExitCode::SUCCESS
}
