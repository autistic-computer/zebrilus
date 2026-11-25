use std::{
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_client_protocol::AcpAgent;
use async_channel::Sender;
use parking_lot::Mutex;
use serde_json::{Value, json};
use zz_protocol::{AgentAutoApprove, AgentProvider, QuotaWindows, chat::ChatEvent};

use crate::{
    acp_chat::Transcript,
    chat_bridge::{is_zz_note, journal_marks},
    host::RuntimeChannels,
    runtime::{AgentSpawnConfig, RuntimeCommand, run_acp_agent, validate_payload},
    stream::AgentStreamPayload,
};

const EFFORT: &str = "effort";
const MODEL: &str = "model";
const MODE: &str = "mode";
const FULL_ACCESS: &str = "full-access";
const SIGNED_OUT: &str = "Authentication required";

#[derive(Clone, Copy)]
pub struct Dialect {
    pub effort_option: &'static str,
    pub signed_out: &'static str,
    pub stored_title: fn(&str) -> Option<String>,
    pub sign_in: Option<&'static str>,
    pub model_name: Option<fn(&str) -> Option<String>>,
    pub unguarded: Option<Unguarded>,
    pub quota: Option<Quota>,
    pub rules: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub struct Quota {
    pub method: &'static str,
    pub read: fn(&Value) -> Option<QuotaWindows>,
}

#[derive(Clone, Copy)]
pub struct Unguarded {
    pub over: &'static str,
    pub always: fn() -> bool,
}

#[derive(Clone, Copy)]
struct Guard {
    over: &'static str,
    on: bool,
    always: bool,
    saved: Option<AgentAutoApprove>,
}

impl Guard {
    fn choose(&mut self, asked: String, tier: &Mutex<AgentAutoApprove>) -> String {
        if self.always {
            return if asked == FULL_ACCESS {
                self.over.to_owned()
            } else {
                asked
            };
        }
        if asked == FULL_ACCESS {
            if !self.on {
                self.saved = Some(std::mem::replace(&mut *tier.lock(), AgentAutoApprove::All));
            }
            self.on = true;
            return self.over.to_owned();
        }
        if let Some(saved) = self.saved.take() {
            *tier.lock() = saved;
        }
        self.on = false;
        asked
    }

    fn shown(&self, mode: String) -> String {
        if self.on && mode == self.over {
            FULL_ACCESS.to_owned()
        } else {
            mode
        }
    }
}

#[derive(Default)]
struct SignIn {
    done: bool,
    held: Option<RuntimeCommand>,
}

pub async fn run(
    agent: AcpAgent,
    dialect: Dialect,
    config: AgentSpawnConfig,
    provider: AgentProvider,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let session_meta = dialect
        .rules
        .zip(config.workspace.pane.as_deref())
        .map(|(key, pane)| {
            let session = config.workspace.session.as_deref().unwrap_or_default();
            let rules = zz_daemon_client::agent_context::workspace_context(pane, session, true);
            serde_json::Map::from_iter([(key.to_owned(), Value::String(rules))])
        });
    run_through(dialect, provider, channels, move |inner| {
        run_acp_agent(
            agent,
            config,
            provider,
            inner.auto_approve,
            inner.permission_ids,
            inner.journal,
            inner.commands,
            inner.controls,
            inner.events,
            session_meta,
        )
    })
    .await
}

pub async fn run_through<F>(
    dialect: Dialect,
    provider: AgentProvider,
    channels: RuntimeChannels,
    runtime: impl FnOnce(RuntimeChannels) -> F,
) -> Result<(), String>
where
    F: Future<Output = Result<(), String>>,
{
    let (command_tx, command_rx) = async_channel::bounded(1);
    let (control_tx, control_rx) = async_channel::bounded(1);
    let (event_tx, event_rx) = async_channel::bounded(1);
    let transcript = Mutex::new(Transcript::marking());
    let journal = channels.journal.clone();
    let record = |(session, marks): (Option<String>, Vec<Value>)| {
        journal_marks(journal.as_deref(), provider, session.as_deref(), &marks);
    };
    let sign_in = Mutex::new(SignIn::default());
    let held_commands = command_tx.downgrade();
    let models = Mutex::new(Vec::<String>::new());
    let tier = channels.auto_approve;
    let guard = dialect.unguarded.map(|unguarded| {
        let always = (unguarded.always)();
        Mutex::new(Guard {
            over: unguarded.over,
            on: always,
            always,
            saved: None,
        })
    });
    let commands = channels.commands;
    let controls = channels.controls;
    let events = channels.events;
    let outbound = async {
        while let Ok(command) = commands.recv().await {
            let command = match (dialect.sign_in, to_agent(command, dialect.effort_option)) {
                (Some(method), open @ RuntimeCommand::Open { .. }) if !sign_in.lock().done => {
                    sign_in.lock().held = Some(open);
                    RuntimeCommand::Authenticate {
                        method_id: method.to_owned(),
                    }
                }
                (_, RuntimeCommand::SetConfigOption { option_id, value })
                    if dialect.model_name.is_some() && option_id == MODEL =>
                {
                    let value = bracket_model(&models.lock(), value);
                    RuntimeCommand::SetConfigOption { option_id, value }
                }
                (_, RuntimeCommand::SetConfigOption { option_id, value }) if option_id == MODE => {
                    let value = match &guard {
                        Some(guard) => guard.lock().choose(value, &tier),
                        None => value,
                    };
                    RuntimeCommand::SetConfigOption { option_id, value }
                }
                (_, RuntimeCommand::SetMode { mode_id }) => RuntimeCommand::SetMode {
                    mode_id: match &guard {
                        Some(guard) => guard.lock().choose(mode_id, &tier),
                        None => mode_id,
                    },
                },
                (_, command) => command,
            };
            if command_tx.send(command).await.is_err() {
                break;
            }
        }
        drop(command_tx);
        std::future::pending::<Result<(), String>>().await
    };
    let answers = async {
        while let Ok(control) = controls.recv().await {
            let (chat, ended) = {
                let mut transcript = transcript.lock();
                (transcript.control(&control), transcript.journal_notes())
            };
            record(ended);
            if send_chat(&events, chat).await.is_err() || control_tx.send(control).await.is_err() {
                break;
            }
        }
        drop(control_tx);
        std::future::pending::<Result<(), String>>().await
    };
    let (metered_tx, metered_rx) = async_channel::bounded::<()>(1);
    let quotas = async {
        while let (Some(quota), Ok(())) = (dialect.quota, metered_rx.recv().await) {
            let (reply, answer) = async_channel::bounded(1);
            let Some(commands) = held_commands.upgrade() else {
                break;
            };
            let asked = commands
                .send(RuntimeCommand::Ask {
                    method: quota.method.to_owned(),
                    reply,
                })
                .await;
            drop(commands);
            if asked.is_err() {
                break;
            }
            if let Ok(Ok(answer)) = answer.recv().await
                && let Some(quota) = (quota.read)(&answer)
                && events
                    .send(AgentStreamPayload::QuotaChanged { quota })
                    .await
                    .is_err()
            {
                break;
            }
        }
        std::future::pending::<Result<(), String>>().await
    };
    let inbound = async {
        let mut titled = false;
        while let Ok(payload) = event_rx.recv().await {
            if matches!(payload, AgentStreamPayload::Authenticated) {
                let held = {
                    let mut sign_in = sign_in.lock();
                    sign_in.done = true;
                    sign_in.held.take()
                };
                if let Some(open) = held {
                    let Some(commands) = held_commands.upgrade() else {
                        break;
                    };
                    if commands.send(open).await.is_err() {
                        break;
                    }
                    continue;
                }
            }
            if dialect.model_name.is_some()
                && let Some(offered) = offered_models(&payload)
            {
                *models.lock() = offered;
            }
            if matches!(
                payload,
                AgentStreamPayload::SessionReady { .. }
                    | AgentStreamPayload::SessionSwitched { .. }
                    | AgentStreamPayload::PromptFinished { .. }
            ) {
                let _ = metered_tx.try_send(());
            }
            let (chat, ended) = {
                let mut transcript = transcript.lock();
                (transcript.payload(&payload), transcript.journal_notes())
            };
            record(ended);
            let note =
                matches!(&payload, AgentStreamPayload::Update { update } if is_zz_note(update));
            let title = settled_title(&payload, &mut titled, dialect.stored_title);
            let payload = to_zz(payload, dialect, guard.as_ref().map(|guard| *guard.lock()));
            titled |= matches!(payload, AgentStreamPayload::TitleChanged { title: Some(_) });
            if !note && events.send(payload).await.is_err() {
                break;
            }
            if let Some(title) = title
                && events.send(title).await.is_err()
            {
                break;
            }
            if send_chat(&events, chat).await.is_err() {
                break;
            }
        }
    };
    let runtime = futures_lite::future::or(
        futures_lite::future::or(
            runtime(RuntimeChannels {
                auto_approve: Arc::clone(&tier),
                permission_ids: channels.permission_ids,
                journal: channels.journal,
                ended: channels.ended,
                commands: command_rx,
                controls: control_rx,
                events: event_tx,
            }),
            outbound,
        ),
        futures_lite::future::or(answers, quotas),
    );
    let result = futures_lite::future::zip(runtime, inbound).await.0;
    if let Err(message) = &result {
        let (chat, ended) = {
            let mut transcript = transcript.lock();
            (transcript.exited(message), transcript.journal_notes())
        };
        record(ended);
        let _ = send_chat(&events, chat).await;
    }
    result
}

async fn send_chat(
    events: &Sender<AgentStreamPayload>,
    chat: Vec<ChatEvent>,
) -> Result<(), async_channel::SendError<AgentStreamPayload>> {
    for event in chat {
        let payload = AgentStreamPayload::Chat { event };
        if validate_payload(&payload).is_ok() {
            events.send(payload).await?;
        }
    }
    Ok(())
}

fn settled_title(
    payload: &AgentStreamPayload,
    titled: &mut bool,
    stored_title: fn(&str) -> Option<String>,
) -> Option<AgentStreamPayload> {
    let title = match payload {
        AgentStreamPayload::SessionReset { restoring } => {
            *titled = false;
            (!restoring).then_some(None)
        }
        AgentStreamPayload::SessionReady { session_id, .. } if !*titled => {
            stored_title(session_id).map(Some)
        }
        AgentStreamPayload::SessionSwitched { session_id, .. } if !*titled => {
            Some(stored_title(session_id))
        }
        _ => None,
    };
    title.map(|title| AgentStreamPayload::TitleChanged { title })
}

pub fn installed(path: Option<&str>, program: &str) -> Vec<PathBuf> {
    crate::environment::find_executables(path, program)
}

fn to_agent(command: RuntimeCommand, effort_option: &str) -> RuntimeCommand {
    match command {
        RuntimeCommand::SetConfigOption { option_id, value } if option_id == EFFORT => {
            RuntimeCommand::SetConfigOption {
                option_id: effort_option.to_owned(),
                value,
            }
        }
        command => command,
    }
}

fn to_zz(
    payload: AgentStreamPayload,
    dialect: Dialect,
    guard: Option<Guard>,
) -> AgentStreamPayload {
    let effort_option = dialect.effort_option;
    match payload {
        AgentStreamPayload::PaneFailed { message } if message.starts_with(SIGNED_OUT) => {
            AgentStreamPayload::PaneFailed {
                message: dialect.signed_out.to_owned(),
            }
        }
        AgentStreamPayload::AuthenticationFailed { .. } if dialect.sign_in.is_some() => {
            AgentStreamPayload::AuthenticationFailed {
                message: dialect.signed_out.to_owned(),
            }
        }
        AgentStreamPayload::SessionReady {
            session_id,
            modes,
            mut config_options,
        } => {
            if let Some(options) = &mut config_options {
                shape(options, dialect, guard);
            }
            AgentStreamPayload::SessionReady {
                session_id,
                modes,
                config_options,
            }
        }
        AgentStreamPayload::SessionSwitched {
            session_id,
            cwd,
            modes,
            mut config_options,
            replay,
        } => {
            if let Some(options) = &mut config_options {
                shape(options, dialect, guard);
            }
            AgentStreamPayload::SessionSwitched {
                session_id,
                cwd,
                modes,
                config_options,
                replay,
            }
        }
        AgentStreamPayload::ConfigOptionsChanged {
            option_id,
            value,
            mut config_options,
        } => {
            shape(&mut config_options, dialect, guard);
            let value = match guard {
                Some(guard) if option_id == MODE => guard.shown(value),
                _ => value,
            };
            AgentStreamPayload::ConfigOptionsChanged {
                option_id: zz_option(option_id, effort_option),
                value,
                config_options,
            }
        }
        AgentStreamPayload::SessionsListed {
            client,
            mut sessions,
            next_cursor,
            cwd_filter,
            replace,
        } => {
            if let Some(folder) = &cwd_filter {
                let wanted = resolved(folder);
                sessions.retain_mut(|session| {
                    let here = session.cwd == *folder || resolved(&session.cwd) == wanted;
                    if here {
                        session.cwd.clone_from(folder);
                    }
                    here
                });
            }
            AgentStreamPayload::SessionsListed {
                client,
                sessions,
                next_cursor,
                cwd_filter,
                replace,
            }
        }
        AgentStreamPayload::ModeChanged { mode_id } => AgentStreamPayload::ModeChanged {
            mode_id: match guard {
                Some(guard) => guard.shown(mode_id),
                None => mode_id,
            },
        },
        AgentStreamPayload::SettingFailed { option_id, message } => {
            AgentStreamPayload::SettingFailed {
                option_id: zz_option(option_id, effort_option),
                message,
            }
        }
        AgentStreamPayload::Update { mut update } => match update["sessionUpdate"].as_str() {
            Some("session_info_update") if update.get("title").is_some() => {
                AgentStreamPayload::TitleChanged {
                    title: update["title"]
                        .as_str()
                        .map(str::trim)
                        .filter(|title| !title.is_empty())
                        .map(str::to_owned),
                }
            }
            Some("config_option_update") => {
                shape(&mut update["configOptions"], dialect, guard);
                AgentStreamPayload::Update { update }
            }
            _ => AgentStreamPayload::Update { update },
        },
        payload => payload,
    }
}

fn offered_models(payload: &AgentStreamPayload) -> Option<Vec<String>> {
    let options = match payload {
        AgentStreamPayload::SessionReady { config_options, .. }
        | AgentStreamPayload::SessionSwitched { config_options, .. } => config_options.as_ref()?,
        AgentStreamPayload::ConfigOptionsChanged { config_options, .. } => config_options,
        AgentStreamPayload::Update { update }
            if update["sessionUpdate"] == "config_option_update" =>
        {
            &update["configOptions"]
        }
        _ => return None,
    };
    let model = options
        .as_array()?
        .iter()
        .find(|option| option["id"] == MODEL)?;
    Some(
        model["options"]
            .as_array()?
            .iter()
            .filter_map(|choice| choice["value"].as_str())
            .map(str::to_owned)
            .collect(),
    )
}

fn bracket_model(offered: &[String], asked: String) -> String {
    if asked.contains('[') {
        return asked;
    }
    offered
        .iter()
        .find(|value| {
            value
                .strip_prefix(asked.as_str())
                .is_some_and(|rest| rest.starts_with('['))
        })
        .cloned()
        .unwrap_or(asked)
}

fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

fn zz_option(option_id: String, effort_option: &str) -> String {
    if option_id == effort_option {
        EFFORT.to_owned()
    } else {
        option_id
    }
}

fn shape(options: &mut Value, dialect: Dialect, guard: Option<Guard>) {
    for option in options.as_array_mut().into_iter().flatten() {
        if option["id"] == dialect.effort_option {
            option["id"] = Value::from(EFFORT);
        }
        if option["id"] == MODEL
            && let Some(name) = dialect.model_name
        {
            for choice in option["options"].as_array_mut().into_iter().flatten() {
                if let Some(name) = choice["value"].as_str().and_then(name) {
                    choice["name"] = Value::from(name);
                }
            }
        }
        if option["id"] == MODE
            && let Some(guard) = guard
        {
            unguard(option, guard);
        }
    }
}

fn unguard(mode: &mut Value, guard: Guard) {
    if let Some(shown) = mode["currentValue"].as_str() {
        mode["currentValue"] = Value::from(guard.shown(shown.to_owned()));
    }
    let Some(choices) = mode["options"].as_array_mut() else {
        return;
    };
    let Some(at) = choices
        .iter()
        .position(|choice| choice["value"] == guard.over)
    else {
        return;
    };
    let unguarded = json!({
        "value": FULL_ACCESS,
        "name": "Run everything",
        "description": "Run anything without asking",
    });
    if guard.always {
        choices[at] = unguarded;
    } else {
        choices.insert(at + 1, unguarded);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const AGY: Dialect = Dialect {
        effort_option: "thought_level",
        signed_out: "signed out of agy",
        stored_title: |_| None,
        sign_in: None,
        model_name: None,
        unguarded: None,
        quota: None,
        rules: None,
    };
    const GROK: Dialect = Dialect {
        effort_option: "reasoning_effort",
        signed_out: "signed out of grok",
        stored_title: |_| None,
        sign_in: None,
        model_name: None,
        unguarded: None,
        quota: None,
        rules: None,
    };

    #[test]
    fn the_agents_effort_option_is_the_chat_effort_option_and_session_info_is_the_title() {
        let options = || {
            json!([
                {"id": "model", "category": "model", "currentValue": "gemini-3.8-flash"},
                {"id": "thought_level", "category": "thought_level", "currentValue": "low"},
            ])
        };
        let AgentStreamPayload::ConfigOptionsChanged {
            option_id,
            config_options,
            ..
        } = to_zz(
            AgentStreamPayload::ConfigOptionsChanged {
                option_id: "thought_level".to_owned(),
                value: "low".to_owned(),
                config_options: options(),
            },
            AGY,
            None,
        )
        else {
            panic!("the payload kind changed");
        };
        assert_eq!(option_id, "effort");
        assert_eq!(
            config_options,
            json!([
                {"id": "model", "category": "model", "currentValue": "gemini-3.8-flash"},
                {"id": "effort", "category": "thought_level", "currentValue": "low"},
            ])
        );
        assert!(matches!(
            to_agent(
                RuntimeCommand::SetConfigOption {
                    option_id: "effort".to_owned(),
                    value: "high".to_owned(),
                },
                "reasoning_effort",
            ),
            RuntimeCommand::SetConfigOption { option_id, value }
                if option_id == "reasoning_effort" && value == "high"
        ));
        assert_eq!(
            to_zz(
                AgentStreamPayload::Update {
                    update: json!({
                        "sessionUpdate": "session_info_update",
                        "title": "Create And Read Probe File",
                    }),
                },
                GROK,
                None,
            ),
            AgentStreamPayload::TitleChanged {
                title: Some("Create And Read Probe File".to_owned()),
            }
        );
        assert_eq!(
            to_zz(
                AgentStreamPayload::PaneFailed {
                    message: "Authentication required: \"no auth method id provided\"".to_owned(),
                },
                GROK,
                None,
            ),
            AgentStreamPayload::PaneFailed {
                message: "signed out of grok".to_owned(),
            }
        );
        let offered = offered_models(&AgentStreamPayload::ConfigOptionsChanged {
            option_id: "model".to_owned(),
            value: String::new(),
            config_options: json!([{
                "id": "model",
                "options": [
                    {"value": "default[]"},
                    {"value": "composer-2[fast=true]"},
                    {"value": "composer-2.5[fast=true]"},
                ],
            }]),
        })
        .unwrap();
        assert_eq!(
            bracket_model(&offered, "composer-2".to_owned()),
            "composer-2[fast=true]"
        );
        assert_eq!(
            bracket_model(&offered, "composer-2.5[fast=false]".to_owned()),
            "composer-2.5[fast=false]"
        );
        assert_eq!(bracket_model(&offered, "gpt-9".to_owned()), "gpt-9");
        let mut asks = Guard {
            over: "agent",
            on: false,
            always: false,
            saved: None,
        };
        let tier = Mutex::new(AgentAutoApprove::Reads);
        assert_eq!(asks.choose("full-access".to_owned(), &tier), "agent");
        assert_eq!(*tier.lock(), AgentAutoApprove::All);
        let mut modes = json!([{
            "id": "mode",
            "currentValue": "agent",
            "options": [{"value": "agent"}, {"value": "plan"}],
        }]);
        shape(&mut modes, GROK, Some(asks));
        assert_eq!(modes[0]["currentValue"], "full-access");
        assert_eq!(
            modes[0]["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|choice| choice["value"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["agent", "full-access", "plan"]
        );
        assert_eq!(asks.choose("plan".to_owned(), &tier), "plan");
        assert_eq!(*tier.lock(), AgentAutoApprove::Reads);
        let never_asks = Guard {
            over: "agent",
            on: true,
            always: true,
            saved: None,
        };
        let mut modes = json!([{
            "id": "mode",
            "currentValue": "agent",
            "options": [{"value": "agent"}, {"value": "plan"}],
        }]);
        shape(&mut modes, GROK, Some(never_asks));
        assert_eq!(modes[0]["currentValue"], "full-access");
        assert_eq!(modes[0]["options"][0]["value"], "full-access");
        assert_eq!(modes[0]["options"][1]["value"], "plan");
    }

    #[test]
    fn a_new_chat_or_a_switch_shows_that_sessions_title_or_none() {
        let stored = |session: &str| (session == "named").then(|| "Named".to_owned());
        let ready = |session: &str| AgentStreamPayload::SessionReady {
            session_id: session.to_owned(),
            modes: None,
            config_options: None,
        };
        let switched = |session: &str| AgentStreamPayload::SessionSwitched {
            session_id: session.to_owned(),
            cwd: PathBuf::from("/workspace"),
            modes: None,
            config_options: None,
            replay: Vec::new(),
        };
        let reset = AgentStreamPayload::SessionReset { restoring: true };
        let shown = |title: Option<&str>| {
            Some(AgentStreamPayload::TitleChanged {
                title: title.map(str::to_owned),
            })
        };
        let mut named = false;
        let mut sent = Vec::new();
        for payload in [
            &reset,
            &ready("named"),
            &reset,
            &switched("fresh"),
            &reset,
            &switched("named"),
        ] {
            sent.push(settled_title(payload, &mut named, stored));
        }
        sent.push(settled_title(&ready("fresh"), &mut named, stored));
        named = true;
        sent.push(settled_title(&switched("fresh"), &mut named, stored));
        assert_eq!(
            sent,
            [
                None,
                shown(Some("Named")),
                None,
                shown(None),
                None,
                shown(Some("Named")),
                None,
                None,
            ]
        );
    }
}
