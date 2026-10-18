use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use serde_json::Value;
use zz_protocol::{AgentTaskWire, PaneId};

use crate::agent::{
    claude::{subagents::ClaudeSource, translate::chunk},
    codex::children::CodexSource,
    host::RuntimeChannels,
    runtime::RuntimeCommand,
    stream::{AgentPromptOutcome, AgentSessionCapabilities, AgentStreamPayload},
};

const POLL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Running,
    Done,
    Stopped,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChildPoll {
    pub(crate) id: String,
    pub(crate) heading: String,
    pub(crate) fresh: Vec<Value>,
    pub(crate) phase: Phase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StopTarget {
    pub(crate) task: String,
    pub(crate) turn: Option<String>,
}

pub(crate) trait FeedSource: Send {
    fn poll(&mut self, now: SystemTime) -> Vec<ChildPoll>;

    fn stop_target(&self, id: &str) -> Option<StopTarget> {
        Some(StopTarget {
            task: id.to_owned(),
            turn: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FeedEvent {
    Replace(Vec<Value>),
    Append(Vec<Value>),
    Tasks(Vec<AgentTaskWire>),
    Busy(bool),
}

struct Turn {
    id: String,
    heading: String,
    updates: Vec<Value>,
    phase: Phase,
}

pub(crate) struct Feed {
    source: Box<dyn FeedSource>,
    turns: Vec<Turn>,
    shown: Vec<usize>,
    follow: Option<usize>,
    tasks: Vec<AgentTaskWire>,
    busy: bool,
}

impl Feed {
    pub(crate) fn new(source: Box<dyn FeedSource>) -> Self {
        Self {
            source,
            turns: Vec::new(),
            shown: Vec::new(),
            follow: None,
            tasks: Vec::new(),
            busy: false,
        }
    }

    pub(crate) fn running(&self) -> bool {
        self.turns.iter().any(|turn| turn.phase == Phase::Running)
    }

    pub(crate) fn timeline(&self) -> Vec<Value> {
        self.shown
            .iter()
            .chain(self.follow.iter())
            .flat_map(|index| self.turns[*index].updates.iter().cloned())
            .collect()
    }

    pub(crate) fn stop_target(&self, id: &str) -> Option<StopTarget> {
        self.source.stop_target(id)
    }

    pub(crate) fn poll(&mut self, now: SystemTime) -> Vec<FeedEvent> {
        let followed = self.follow;
        let mut append = Vec::new();
        let mut replace = false;
        let mut finished = Vec::new();
        for child in self.source.poll(now) {
            let index = if let Some(index) = self.turns.iter().position(|turn| turn.id == child.id)
            {
                index
            } else {
                self.turns.push(Turn {
                    updates: vec![chunk(
                        "user_message_chunk",
                        &format!("subagent-{}", child.id),
                        &child.heading,
                    )],
                    id: child.id.clone(),
                    heading: child.heading.clone(),
                    phase: Phase::Running,
                });
                self.turns.len() - 1
            };
            let turn = &mut self.turns[index];
            let before = turn.phase;
            turn.phase = child.phase;
            turn.heading = child.heading;
            turn.updates.extend(child.fresh.iter().cloned());
            if followed == Some(index) {
                append.extend(child.fresh);
            }
            if before == Phase::Running && child.phase != Phase::Running {
                finished.push(index);
            } else if before != Phase::Running
                && child.phase == Phase::Running
                && let Some(at) = self.shown.iter().position(|shown| *shown == index)
            {
                self.shown.remove(at);
                replace = true;
            }
        }
        if let Some(index) = followed.filter(|index| finished.contains(index)) {
            self.shown.push(index);
            self.follow = None;
        }
        for index in finished
            .into_iter()
            .filter(|index| Some(*index) != followed)
        {
            if self.shown.contains(&index) {
                continue;
            }
            self.shown.push(index);
            if self.follow.is_some() {
                replace = true;
            } else {
                append.extend(self.turns[index].updates.iter().cloned());
            }
        }
        if self.follow.is_none()
            && let Some(index) = (0..self.turns.len()).find(|index| {
                self.turns[*index].phase == Phase::Running && !self.shown.contains(index)
            })
        {
            self.follow = Some(index);
            append.extend(self.turns[index].updates.iter().cloned());
        }
        let mut events = Vec::new();
        if replace {
            events.push(FeedEvent::Replace(self.timeline()));
        } else if !append.is_empty() {
            events.push(FeedEvent::Append(append));
        }
        let tasks = self
            .turns
            .iter()
            .filter(|turn| turn.phase == Phase::Running)
            .map(|turn| AgentTaskWire {
                id: turn.id.clone(),
                kind: "agent".to_owned(),
                description: turn.heading.clone(),
                tool_call_id: None,
            })
            .collect::<Vec<_>>();
        if tasks != self.tasks {
            self.tasks.clone_from(&tasks);
            events.push(FeedEvent::Tasks(tasks));
        }
        let busy = self.running();
        if busy != self.busy || replace && busy {
            self.busy = busy;
            events.push(FeedEvent::Busy(busy));
        }
        events
    }
}

pub(crate) type StopTask = Arc<dyn Fn(StopTarget) + Send + Sync>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FeedSourceSpec {
    Claude { dir: PathBuf },
    Codex { sessions: PathBuf, root: String },
}

#[derive(Clone)]
pub(crate) struct FeedSpec {
    pub(crate) parent: PaneId,
    pub(crate) source: FeedSourceSpec,
    pub(crate) cwd: PathBuf,
    pub(crate) stop: Option<StopTask>,
}

impl std::fmt::Debug for FeedSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FeedSpec")
            .field("parent", &self.parent)
            .field("source", &self.source)
            .field("cwd", &self.cwd)
            .field("stop", &self.stop.is_some())
            .finish()
    }
}

impl FeedSpec {
    fn feed(&self, since: SystemTime) -> (Feed, &'static str, &'static str) {
        match &self.source {
            FeedSourceSpec::Claude { dir } => (
                Feed::new(Box::new(ClaudeSource::new(
                    dir.clone(),
                    self.cwd.clone(),
                    since,
                ))),
                "Claude Code subagents",
                "claude-code",
            ),
            FeedSourceSpec::Codex { sessions, root } => (
                Feed::new(Box::new(CodexSource::new(
                    sessions.clone(),
                    root.clone(),
                    self.cwd.clone(),
                    since,
                ))),
                "Codex subagents",
                "codex",
            ),
        }
    }
}

enum Input {
    Command(Result<RuntimeCommand, async_channel::RecvError>),
    Tick,
}

pub(crate) async fn run_subagent_feed(
    spec: FeedSpec,
    channels: RuntimeChannels,
) -> Result<(), String> {
    let events = channels.events;
    let commands = channels.commands;
    let session_id = format!("subagents-{}", spec.parent);
    let (mut feed, agent_name, agent_key) = spec.feed(SystemTime::now());
    let mut open = false;
    loop {
        let input =
            futures_lite::future::or(async { Input::Command(commands.recv().await) }, async {
                smol::Timer::after(POLL).await;
                Input::Tick
            })
            .await;
        let mut payloads = Vec::new();
        match input {
            Input::Command(Ok(RuntimeCommand::Shutdown) | Err(_)) => return Ok(()),
            Input::Command(Ok(RuntimeCommand::Open { .. })) => {
                open = true;
                payloads.push(AgentStreamPayload::Ready {
                    agent_name: agent_name.to_owned(),
                    agent_key: agent_key.to_owned(),
                    auth_methods: Vec::new(),
                    capabilities: AgentSessionCapabilities {
                        watch: true,
                        ..AgentSessionCapabilities::default()
                    },
                });
                payloads.push(AgentStreamPayload::SessionReset { restoring: false });
                payloads.push(AgentStreamPayload::SessionReady {
                    session_id: session_id.clone(),
                    modes: None,
                    config_options: None,
                });
                payloads.extend(feed_payloads(&mut feed, &spec, &session_id));
            }
            Input::Command(Ok(RuntimeCommand::StopTask { task_id })) => {
                if let (Some(stop), Some(target)) = (&spec.stop, feed.stop_target(&task_id)) {
                    stop(target);
                }
            }
            Input::Command(Ok(RuntimeCommand::Prompt { turn_id, .. })) => {
                payloads.push(AgentStreamPayload::PromptFinished {
                    turn_id,
                    outcome: AgentPromptOutcome::Failed {
                        message: "This pane only watches its subagents.".to_owned(),
                    },
                });
            }
            Input::Tick if open => {
                payloads.extend(feed_payloads(&mut feed, &spec, &session_id));
            }
            Input::Command(Ok(_)) | Input::Tick => {}
        }
        for payload in payloads {
            events
                .send(payload)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
}

fn feed_payloads(feed: &mut Feed, spec: &FeedSpec, session_id: &str) -> Vec<AgentStreamPayload> {
    feed.poll(SystemTime::now())
        .into_iter()
        .flat_map(|event| match event {
            FeedEvent::Replace(replay) => vec![AgentStreamPayload::SessionSwitched {
                session_id: session_id.to_owned(),
                cwd: spec.cwd.clone(),
                modes: None,
                config_options: None,
                replay,
            }],
            FeedEvent::Append(updates) => updates
                .into_iter()
                .map(|update| AgentStreamPayload::Update { update })
                .collect(),
            FeedEvent::Tasks(tasks) if spec.stop.is_some() => {
                vec![AgentStreamPayload::TasksChanged { tasks }]
            }
            FeedEvent::Tasks(_) => Vec::new(),
            FeedEvent::Busy(busy) => vec![AgentStreamPayload::Activity { busy }],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script(Vec<Vec<ChildPoll>>);

    impl FeedSource for Script {
        fn poll(&mut self, _: SystemTime) -> Vec<ChildPoll> {
            if self.0.is_empty() {
                Vec::new()
            } else {
                self.0.remove(0)
            }
        }

        fn stop_target(&self, id: &str) -> Option<StopTarget> {
            Some(StopTarget {
                task: id.to_owned(),
                turn: Some("t1".to_owned()),
            })
        }
    }

    fn child(id: &str, text: &str, phase: Phase) -> ChildPoll {
        ChildPoll {
            id: id.to_owned(),
            heading: format!("worker · {id}"),
            fresh: if text.is_empty() {
                Vec::new()
            } else {
                vec![chunk("agent_message_chunk", id, text)]
            },
            phase,
        }
    }

    #[test]
    fn the_feed_answers_open_turns_prompts_away_and_hands_stop_to_its_parent() {
        let (commands, command_rx) = async_channel::unbounded();
        let (_controls, control_rx) = async_channel::unbounded();
        let (events, event_rx) = async_channel::unbounded();
        let stopped = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let seen = Arc::clone(&stopped);
        let spec = FeedSpec {
            parent: PaneId(3),
            source: FeedSourceSpec::Claude {
                dir: PathBuf::from("/nowhere/subagents"),
            },
            cwd: PathBuf::from("/w"),
            stop: Some(Arc::new(move |target: StopTarget| seen.lock().push(target))),
        };
        let channels = RuntimeChannels {
            auto_approve: Arc::new(parking_lot::Mutex::new(zz_protocol::AgentAutoApprove::Off)),
            permission_ids: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            journal: None,
            commands: command_rx,
            controls: control_rx,
            events,
        };
        for command in [
            RuntimeCommand::Open {
                cwd: "/w".into(),
                resume_session: None,
            },
            RuntimeCommand::StopTask {
                task_id: "a1".to_owned(),
            },
            RuntimeCommand::Prompt {
                turn_id: 7,
                prompt: crate::agent::stream::AgentPrompt::default(),
            },
            RuntimeCommand::Shutdown,
        ] {
            commands.try_send(command).unwrap();
        }
        futures_lite::future::block_on(run_subagent_feed(spec, channels)).unwrap();
        assert_eq!(
            *stopped.lock(),
            [StopTarget {
                task: "a1".to_owned(),
                turn: None
            }]
        );
        let payloads = std::iter::from_fn(|| event_rx.try_recv().ok()).collect::<Vec<_>>();
        assert!(matches!(
            &payloads[0],
            AgentStreamPayload::Ready { capabilities, .. } if capabilities.watch
        ));
        assert!(payloads.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::SessionReady { session_id, .. } if session_id == "subagents-%3"
        )));
        assert!(payloads.iter().any(|payload| matches!(
            payload,
            AgentStreamPayload::PromptFinished {
                turn_id: 7,
                outcome: AgentPromptOutcome::Failed { .. }
            }
        )));
    }

    #[test]
    fn a_feed_with_nothing_to_stop_through_shows_no_task_tray() {
        let tasks = |stop: Option<StopTask>| {
            let spec = FeedSpec {
                parent: PaneId(3),
                source: FeedSourceSpec::Claude {
                    dir: PathBuf::new(),
                },
                cwd: PathBuf::from("/w"),
                stop,
            };
            let mut feed = Feed::new(Box::new(Script(vec![vec![child("a", "", Phase::Running)]])));
            feed_payloads(&mut feed, &spec, "subagents-%3")
                .iter()
                .any(|payload| matches!(payload, AgentStreamPayload::TasksChanged { .. }))
        };
        assert!(tasks(Some(Arc::new(|_| {}))));
        assert!(!tasks(None));
    }

    #[test]
    fn a_source_drives_whole_turns_and_names_its_stop_target() {
        let mut feed = Feed::new(Box::new(Script(vec![
            vec![
                child("a", "", Phase::Running),
                child("b", "", Phase::Running),
            ],
            vec![child("b", "b done", Phase::Done)],
            vec![child("a", "a done", Phase::Done)],
        ])));
        let first = feed.poll(SystemTime::now());
        assert!(matches!(&first[0], FeedEvent::Append(updates) if updates.len() == 1));
        let second = feed.poll(SystemTime::now());
        let FeedEvent::Replace(timeline) = &second[0] else {
            panic!("{second:?}");
        };
        assert_eq!(timeline.len(), 3, "b's whole turn, then a's heading");
        assert_eq!(
            feed.poll(SystemTime::now())[0],
            FeedEvent::Append(vec![chunk("agent_message_chunk", "a", "a done")])
        );
        assert_eq!(
            feed.stop_target("a"),
            Some(StopTarget {
                task: "a".to_owned(),
                turn: Some("t1".to_owned())
            })
        );
    }
}
