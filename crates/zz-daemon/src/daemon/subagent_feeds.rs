use super::*;
use std::collections::HashMap;
use std::time::SystemTime;

use crate::agent::{
    claude::{sessions, subagents},
    codex::children,
    feed::{FeedSourceSpec, FeedSpec, StopTarget, StopTask},
};

const SCAN: Duration = Duration::from_secs(1);
const LINGER: Duration = Duration::from_secs(5);
const RELOOK: Duration = Duration::from_secs(10);
const CODEX_RECHECK: Duration = Duration::from_secs(30);
const CODEX_RETRY: Duration = Duration::from_secs(5);
const WIDTH: u16 = 64;
const HEIGHT: u16 = 18;
pub(super) const FEED_OPTION: &str = "@zz-subagents";

#[derive(Default)]
pub(super) struct SubagentFeeds {
    running: bool,
    kicked: bool,
    sources: BTreeMap<PaneId, Source>,
    floats: BTreeMap<PaneId, Float>,
    dismissed: BTreeMap<PaneId, BTreeSet<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    None,
    Claude,
    Codex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    spec: FeedSourceSpec,
    cwd: PathBuf,
    route: Route,
}

#[derive(Clone, Copy, Debug)]
struct Float {
    pane: PaneId,
    idle_since: Option<Instant>,
}

enum Vendor {
    Claude { session: String, agent: bool },
    CodexAgent { thread: String },
    CodexTerminal,
}

struct Candidate {
    parent: PaneId,
    cwd: PathBuf,
    vendor: Vendor,
}

struct Seen {
    parent: PaneId,
    source: Option<Source>,
    running: Vec<String>,
}

#[derive(Default)]
struct Watcher {
    dirs: HashMap<(PaneId, String), (Option<PathBuf>, Instant)>,
    completions: HashMap<PathBuf, subagents::Completions>,
    rollouts: children::Rollouts,
    codex_panes: HashMap<(PaneId, u32), (bool, Instant)>,
}

fn parse_pane(value: &str) -> Option<PaneId> {
    value.strip_prefix('%')?.parse().ok().map(PaneId)
}

impl Shared {
    pub(super) fn watch_subagents(self: &Arc<Self>) {
        {
            let mut inner = self.inner.lock();
            let feeds = &mut inner.subagent_feeds;
            if feeds.running {
                feeds.kicked = true;
                return;
            }
            feeds.running = true;
        }
        let owner = Arc::downgrade(&self.server_owner());
        if let Err(error) = thread::Builder::new()
            .name("zz-subagents".to_owned())
            .spawn(move || scan_subagents(&owner))
        {
            self.inner.lock().subagent_feeds.running = false;
            log::debug!("could not start the subagent watcher: {error}");
        }
    }

    fn feed_pane(&self, pane: PaneId) -> Option<PaneId> {
        let inner = self.inner.lock();
        let window = inner.engine.state.window_for_pane(pane)?;
        let session = inner.engine.state.windows.get(&window)?.session;
        inner
            .engine
            .format_facts()
            .user_option(
                &pane.to_string(),
                &window.to_string(),
                &session.to_string(),
                FEED_OPTION,
            )
            .and_then(parse_pane)
    }

    pub(super) fn subagent_feed_spec(self: &Arc<Self>, pane: PaneId) -> Option<FeedSpec> {
        let parent = self.feed_pane(pane)?;
        let source = self
            .inner
            .lock()
            .subagent_feeds
            .sources
            .get(&parent)
            .cloned();
        let Some(source) = source else {
            return Some(FeedSpec {
                parent,
                source: FeedSourceSpec::Claude {
                    dir: PathBuf::new(),
                },
                cwd: PathBuf::from("/"),
                stop: None,
            });
        };
        let route = source.route;
        let stop = (route != Route::None).then(|| {
            let owner = Arc::downgrade(&self.server_owner());
            let stop: StopTask = Arc::new(move |target: StopTarget| {
                let Some(runtime) = owner
                    .upgrade()
                    .and_then(|shared| shared.open_agent_runtime())
                else {
                    return;
                };
                let command = match (route, target.turn) {
                    (Route::Codex, Some(turn_id)) => HostCommand::InterruptThread {
                        thread_id: target.task,
                        turn_id,
                    },
                    (Route::Claude, _) => HostCommand::StopTask {
                        task_id: target.task,
                    },
                    _ => return,
                };
                runtime.command(parent, command);
            });
            stop
        });
        Some(FeedSpec {
            parent,
            source: source.spec,
            cwd: source.cwd,
            stop,
        })
    }

    fn subagent_candidates(
        self: &Arc<Self>,
        codex_panes: &mut HashMap<(PaneId, u32), (bool, Instant)>,
    ) -> Vec<Candidate> {
        let (agents, claude_terminals, codex_terminals) = {
            let inner = self.inner.lock();
            let feeds = inner
                .subagent_feeds
                .floats
                .values()
                .map(|float| float.pane)
                .collect::<BTreeSet<_>>();
            let mut agents = Vec::new();
            let mut claude_terminals = Vec::new();
            let mut codex_terminals = Vec::new();
            for window in inner.engine.state.windows.values() {
                for (pane, state) in &window.panes {
                    match &state.kind {
                        PaneKind::Agent(descriptor) if !feeds.contains(pane) => {
                            agents.push((*pane, descriptor.provider, descriptor.cwd.clone()));
                        }
                        PaneKind::Terminal => {
                            let Some(runtime) = inner.engine.pane_runtime_facts(*pane) else {
                                continue;
                            };
                            #[cfg(unix)]
                            if inner.claude_peer_states.contains_key(pane) {
                                claude_terminals.push((*pane, runtime.pid));
                                continue;
                            }
                            if let Some(pid) = runtime.pid
                                && matches!(runtime.current_command.as_str(), "codex" | "node")
                            {
                                let cwd = if runtime.current_path.is_empty() {
                                    &runtime.start_path
                                } else {
                                    &runtime.current_path
                                };
                                codex_terminals.push((*pane, pid, PathBuf::from(cwd)));
                            }
                        }
                        _ => {}
                    }
                }
            }
            (agents, claude_terminals, codex_terminals)
        };
        let mut candidates = Vec::new();
        if let Some(runtime) = self.open_agent_runtime() {
            for (pane, provider, cwd) in agents {
                if self.feed_pane(pane).is_some() {
                    continue;
                }
                let (Some(session), Some(cwd)) = (
                    runtime.wire_state(pane).and_then(|state| state.session_id),
                    cwd,
                ) else {
                    continue;
                };
                let vendor = match provider {
                    AgentProvider::ClaudeCode => Vendor::Claude {
                        session,
                        agent: true,
                    },
                    AgentProvider::Codex => Vendor::CodexAgent { thread: session },
                };
                candidates.push(Candidate {
                    parent: pane,
                    cwd,
                    vendor,
                });
            }
        }
        #[cfg(unix)]
        if !claude_terminals.is_empty()
            && let Ok(records) = crate::agent::claude_peers::read_records()
        {
            for (pane, pid) in claude_terminals {
                if let Some(record) =
                    crate::agent::claude_peers::record_for_pane(&records, &pane.to_string(), pid)
                {
                    candidates.push(Candidate {
                        parent: pane,
                        cwd: PathBuf::from(&record.cwd),
                        vendor: Vendor::Claude {
                            session: record.session_id.clone(),
                            agent: false,
                        },
                    });
                }
            }
        }
        #[cfg(not(unix))]
        let _ = claude_terminals;
        let now = Instant::now();
        codex_panes.retain(|(pane, pid), (runs, at)| {
            now.saturating_duration_since(*at) < if *runs { CODEX_RECHECK } else { CODEX_RETRY }
                && codex_terminals
                    .iter()
                    .any(|(candidate, candidate_pid, _)| candidate == pane && candidate_pid == pid)
        });
        for (pane, pid, cwd) in codex_terminals {
            let (runs, _) = *codex_panes.entry((pane, pid)).or_insert_with(|| {
                #[cfg(unix)]
                let runs = crate::agent::codex_queue::pane_runs_codex(pid);
                #[cfg(not(unix))]
                let runs = false;
                (runs, now)
            });
            if runs {
                candidates.push(Candidate {
                    parent: pane,
                    cwd,
                    vendor: Vendor::CodexTerminal,
                });
            }
        }
        candidates
    }

    fn apply_subagent_scan(self: &Arc<Self>, seen: Vec<Seen>) {
        let now = Instant::now();
        let mut opens = Vec::new();
        let mut closes = self.stray_subagent_floats();
        {
            let mut inner = self.inner.lock();
            let gone = inner
                .subagent_feeds
                .floats
                .iter()
                .filter(|(_, float)| inner.engine.state.pane(float.pane).is_none())
                .map(|(parent, _)| *parent)
                .collect::<Vec<_>>();
            let feeds = &mut inner.subagent_feeds;
            for parent in gone {
                feeds.floats.remove(&parent);
                let running = seen
                    .iter()
                    .find(|seen| seen.parent == parent)
                    .map(|seen| seen.running.iter().cloned().collect())
                    .unwrap_or_default();
                feeds.dismissed.insert(parent, running);
            }
            let parents = seen.iter().map(|seen| seen.parent).collect::<BTreeSet<_>>();
            feeds.floats.retain(|parent, float| {
                let keep = parents.contains(parent);
                if !keep {
                    closes.push(float.pane);
                }
                keep
            });
            feeds.sources.retain(|parent, _| parents.contains(parent));
            feeds.dismissed.retain(|parent, _| parents.contains(parent));
            for seen in seen {
                if let Some(source) = seen.source {
                    feeds.sources.insert(seen.parent, source);
                }
                if seen.running.is_empty() {
                    feeds.dismissed.remove(&seen.parent);
                    if let Some(float) = feeds.floats.get_mut(&seen.parent) {
                        let since = *float.idle_since.get_or_insert(now);
                        if now.saturating_duration_since(since) >= LINGER {
                            closes.push(float.pane);
                            feeds.floats.remove(&seen.parent);
                        }
                    }
                    continue;
                }
                if let Some(float) = feeds.floats.get_mut(&seen.parent) {
                    float.idle_since = None;
                    continue;
                }
                if feeds
                    .dismissed
                    .get(&seen.parent)
                    .is_some_and(|dismissed| seen.running.iter().all(|id| dismissed.contains(id)))
                {
                    continue;
                }
                feeds.dismissed.remove(&seen.parent);
                opens.push(seen.parent);
            }
        }
        for parent in opens {
            self.open_subagent_float(parent);
        }
        for pane in closes {
            self.close_subagent_float(pane);
        }
    }

    fn stray_subagent_floats(&self) -> Vec<PaneId> {
        let (agents, tracked) = {
            let inner = self.inner.lock();
            let agents = inner
                .engine
                .state
                .windows
                .values()
                .flat_map(|window| &window.panes)
                .filter(|(_, state)| matches!(state.kind, PaneKind::Agent(_)))
                .map(|(pane, _)| *pane)
                .collect::<Vec<_>>();
            let tracked = inner
                .subagent_feeds
                .floats
                .values()
                .map(|float| float.pane)
                .collect::<BTreeSet<_>>();
            (agents, tracked)
        };
        agents
            .into_iter()
            .filter(|pane| !tracked.contains(pane) && self.feed_pane(*pane).is_some())
            .collect()
    }

    fn open_subagent_float(self: &Arc<Self>, parent: PaneId) {
        let (mut context, command) = {
            let inner = self.inner.lock();
            let Some(source) = inner.subagent_feeds.sources.get(&parent).cloned() else {
                return;
            };
            let provider = match source.spec {
                FeedSourceSpec::Claude { .. } => "claude-code",
                FeedSourceSpec::Codex { .. } => "codex",
            };
            let (Some((xoff, yoff)), Some((sx, sy))) = (
                inner.engine.pane_origin(parent),
                inner.engine.pane_geometry(parent),
            ) else {
                return;
            };
            let width = WIDTH.min(sx);
            let height = HEIGHT.min(sy);
            let x = (xoff + i32::from(sx) - i32::from(width)).max(0);
            let y = yoff.max(0);
            let Some(mut context) = ExecutionContext::for_pane(&inner.engine.state, parent) else {
                return;
            };
            context.set_spawn_pane_options(vec![(FEED_OPTION.to_owned(), parent.to_string())]);
            let command = CommandInvocation::new(
                "new-pane",
                [
                    "-d".to_owned(),
                    "-t".to_owned(),
                    parent.to_string(),
                    "--kind".to_owned(),
                    "agent".to_owned(),
                    "--provider".to_owned(),
                    provider.to_owned(),
                    "-c".to_owned(),
                    source.cwd.display().to_string(),
                    "-T".to_owned(),
                    "Subagents".to_owned(),
                    "-x".to_owned(),
                    width.to_string(),
                    "-y".to_owned(),
                    height.to_string(),
                    "-X".to_owned(),
                    x.to_string(),
                    "-Y".to_owned(),
                    y.to_string(),
                ],
            );
            (context, command)
        };
        context.no_hooks = true;
        let created = match self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &command,
        ) {
            Ok(execution) => execution.effects.iter().find_map(|effect| match effect {
                MuxEffect::PaneCreated { pane, .. } => Some(*pane),
                _ => None,
            }),
            Err(error) => {
                log::debug!(target: "zz::agent", "could not float {parent}'s subagents: {error}");
                None
            }
        };
        if let Some(pane) = created {
            self.inner.lock().subagent_feeds.floats.insert(
                parent,
                Float {
                    pane,
                    idle_since: None,
                },
            );
        }
    }

    fn close_subagent_float(self: &Arc<Self>, pane: PaneId) {
        if self.inner.lock().engine.state.pane(pane).is_none() {
            return;
        }
        let target = pane.to_string();
        let command = CommandInvocation::new("kill-pane", ["-t", target.as_str()]);
        let mut context = ExecutionContext::default();
        context.no_hooks = true;
        if let Err(error) = self.execute(
            ClientId(u64::MAX),
            ClientKind::Command,
            &mut context,
            &command,
        ) {
            log::debug!(target: "zz::agent", "could not close the subagent float {pane}: {error}");
        }
    }
}

impl Watcher {
    fn claude(
        &mut self,
        parent: PaneId,
        session: &str,
        cwd: PathBuf,
        agent: bool,
        now: Instant,
        clock: SystemTime,
    ) -> Option<Seen> {
        let config = sessions::config_home()?;
        let key = (parent, session.to_owned());
        let stale = self
            .dirs
            .get(&key)
            .is_none_or(|(dir, at)| dir.is_none() && now.saturating_duration_since(*at) >= RELOOK);
        if stale {
            let dir = sessions::session_file(&config, &cwd, session)
                .map(|file| subagents::subagents_dir(&file));
            self.dirs.insert(key.clone(), (dir, now));
        }
        let dir = self.dirs.get(&key)?.0.clone()?;
        let completions = self
            .completions
            .entry(dir.clone())
            .or_insert_with(|| subagents::Completions::new(&dir));
        completions.refresh();
        Some(Seen {
            parent,
            running: subagents::running_subagents(&dir, clock, completions),
            source: Some(Source {
                spec: FeedSourceSpec::Claude { dir },
                cwd,
                route: if agent { Route::Claude } else { Route::None },
            }),
        })
    }

    fn codex(&mut self, candidates: Vec<Candidate>, clock: SystemTime) -> Vec<Seen> {
        let Some(sessions) = children::sessions_dir() else {
            return Vec::new();
        };
        let trees = children::running_children(&sessions, clock, &mut self.rollouts);
        let claimed = candidates
            .iter()
            .filter_map(|candidate| match &candidate.vendor {
                Vendor::CodexAgent { thread } => Some(thread.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let terminal_cwds = candidates
            .iter()
            .filter(|candidate| matches!(candidate.vendor, Vendor::CodexTerminal))
            .map(|candidate| candidate.cwd.clone())
            .collect::<Vec<_>>();
        candidates
            .into_iter()
            .map(|candidate| {
                if let Vendor::CodexAgent { thread } = candidate.vendor {
                    return Seen {
                        parent: candidate.parent,
                        running: trees
                            .get(&thread)
                            .map(|tree| tree.children.clone())
                            .unwrap_or_default(),
                        source: Some(Source {
                            spec: FeedSourceSpec::Codex {
                                sessions: sessions.clone(),
                                root: thread,
                            },
                            cwd: candidate.cwd,
                            route: Route::Codex,
                        }),
                    };
                }
                let unique = terminal_cwds
                    .iter()
                    .filter(|cwd| **cwd == candidate.cwd)
                    .count()
                    == 1;
                let mut roots = trees.iter().filter(|(root, tree)| {
                    unique && !claimed.contains(*root) && tree.cwd.as_ref() == Some(&candidate.cwd)
                });
                match (roots.next(), roots.next()) {
                    (Some((root, tree)), None) => Seen {
                        parent: candidate.parent,
                        running: tree.children.clone(),
                        source: Some(Source {
                            spec: FeedSourceSpec::Codex {
                                sessions: sessions.clone(),
                                root: root.clone(),
                            },
                            cwd: candidate.cwd,
                            route: Route::None,
                        }),
                    },
                    _ => Seen {
                        parent: candidate.parent,
                        running: Vec::new(),
                        source: None,
                    },
                }
            })
            .collect()
    }
}

fn scan_subagents(owner: &Weak<Shared>) {
    let mut watcher = Watcher::default();
    loop {
        thread::sleep(SCAN);
        let Some(shared) = owner.upgrade() else {
            return;
        };
        let candidates = shared.subagent_candidates(&mut watcher.codex_panes);
        {
            let mut inner = shared.inner.lock();
            let feeds = &mut inner.subagent_feeds;
            if candidates.is_empty()
                && feeds.floats.is_empty()
                && watcher.codex_panes.is_empty()
                && !feeds.kicked
            {
                feeds.running = false;
                return;
            }
            feeds.kicked = false;
        }
        let now = Instant::now();
        let clock = SystemTime::now();
        let parents = candidates
            .iter()
            .map(|candidate| candidate.parent)
            .collect::<BTreeSet<_>>();
        watcher
            .dirs
            .retain(|(parent, _), _| parents.contains(parent));
        let dirs = &watcher.dirs;
        watcher
            .completions
            .retain(|dir, _| dirs.values().any(|(known, _)| known.as_ref() == Some(dir)));
        let (codex, claude): (Vec<_>, Vec<_>) = candidates
            .into_iter()
            .partition(|candidate| !matches!(candidate.vendor, Vendor::Claude { .. }));
        let mut seen = claude
            .into_iter()
            .filter_map(|candidate| match candidate.vendor {
                Vendor::Claude { session, agent } => {
                    watcher.claude(candidate.parent, &session, candidate.cwd, agent, now, clock)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !codex.is_empty() {
            seen.extend(watcher.codex(codex, clock));
        }
        let target = Arc::downgrade(&shared);
        let _ = shared.timer_tx.send(timers::TimerInput::Callback {
            deadline: None,
            callback: Box::new(move || {
                if let Some(shared) = target.upgrade() {
                    shared.apply_subagent_scan(seen);
                }
            }),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::parse_pane;
    use zz_protocol::PaneId;

    #[test]
    fn a_feed_names_its_parent_by_pane_id() {
        assert_eq!(parse_pane("%12"), Some(PaneId(12)));
        assert_eq!(parse_pane("12"), None);
        assert_eq!(parse_pane("%x"), None);
    }
}
