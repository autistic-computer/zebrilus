use zz_daemon_client::{CommandClient, CommandOutcome};
use zz_protocol::agent_mention::{AgentReport, RemoteReport, ReplyNotice};

use super::*;

const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Gate {
    Opening,
    Sending,
    Dropped,
}

pub(super) struct Relay {
    pub(super) shared: Weak<Shared>,
    pub(super) caller: PaneId,
    pub(super) host: String,
    pub(super) pane: PaneId,
    pub(super) payload: String,
    pub(super) gate: Arc<Mutex<Gate>>,
}

pub(super) type Opened = cmdq::Reply<Result<(), String>>;

fn declined(why: &str) -> DaemonError {
    ServerError::NativeCommandParse(format!("agent-send: --relay {why}")).into()
}

impl Shared {
    pub(super) fn relay_agent_send(
        self: &Arc<Self>,
        client: ClientId,
        kind: ClientKind,
        context: &ExecutionContext,
        host: &str,
        parsed: &ParsedAgentSend,
    ) -> Result<Execution, DaemonError> {
        let plain =
            !(parsed.submit || parsed.wait || parsed.notify || parsed.json || parsed.final_only);
        let pane = parsed
            .target
            .as_deref()
            .and_then(|target| target.parse::<PaneId>().ok());
        let caller = self.chat_caller(client, context, None);
        let (true, Some(pane), Some(caller), ClientKind::Command | ClientKind::Control) =
            (plain, pane, caller, kind)
        else {
            return Err(declined("takes a plain send to -t %N from a chat"));
        };
        let payload = {
            use zz_protocol::agent_mention::{Sender, SentMessage};
            let here = self.inner.lock().engine.format_host_short().to_owned();
            SentMessage {
                sender: Sender::Chat {
                    pane: caller,
                    provider: self.agent_provider_of(caller),
                },
                host: Some(here).filter(|here| !here.is_empty()),
                text: parsed.payload()?,
            }
            .to_string()
        };
        let limit = crate::agent::fanout::MAX_SENDS_SINCE_USER;
        if self
            .open_agent_runtime()
            .is_some_and(|runtime| runtime.sends_since_user(caller, pane) >= limit)
        {
            return Err(ServerError::InvalidCommand(format!(
                "{host}:{pane} has had {limit} messages from you since the user last wrote; answer the user now"
            ))
            .into());
        }
        let gate = Arc::new(Mutex::new(Gate::Opening));
        let held = Arc::clone(&gate);
        let notice = ReplyNotice::Relayed {
            host: host.to_owned(),
            pane,
        }
        .to_string();
        let (wait, reply) = agent_waits::reply::<Result<(), String>>(
            self,
            Some(OPEN_TIMEOUT),
            move |shared, outcome| {
                let opened = match outcome {
                    terminal_requests::ReplyOutcome::Ready(opened) => opened,
                    terminal_requests::ReplyOutcome::Closed
                    | terminal_requests::ReplyOutcome::TimedOut => {
                        let mut gate = held.lock();
                        if *gate == Gate::Sending {
                            Ok(())
                        } else {
                            *gate = Gate::Dropped;
                            Err("got no answer from the host in time".to_owned())
                        }
                    }
                };
                match opened {
                    Ok(()) => {
                        shared.record_command_stderr(client, &notice);
                        Ok(Execution {
                            output: pane.to_string().into(),
                            effects: Vec::new(),
                        })
                    }
                    Err(why) => Err(declined(&why)),
                }
            },
        );
        let relay = Relay {
            shared: Arc::downgrade(&self.server_owner()),
            caller,
            host: host.to_owned(),
            pane,
            payload,
            gate,
        };
        let spawned = std::thread::Builder::new()
            .name("zz-agent-relay".to_owned())
            .spawn(move || relay.run(&reply));
        if let Err(error) = spawned {
            log::warn!(target: "zz::agent", "could not start a relay to {host}: {error}");
        }
        self.report_command_queue_park();
        wait.finish(self, Execution::default())
    }
}

impl Relay {
    fn run(self, reply: &Opened) {
        let mut remote = match zz_daemon_client::host_endpoint(&self.host).and_then(|endpoint| {
            CommandClient::connect_endpoint_unattended(&endpoint).map_err(|error| error.to_string())
        }) {
            Ok(remote) => remote,
            Err(why) => {
                reply.try_send(Err(why));
                return;
            }
        };
        self.run_on(|command| remote.execute_streams(command), reply);
    }

    pub(super) fn run_on(
        self,
        mut remote: impl FnMut(CommandInvocation) -> Result<CommandOutcome, DaemonError>,
        reply: &Opened,
    ) {
        let pane = self.pane.to_string();
        let facts = remote(CommandInvocation::new(
            "display-message",
            [
                "-p",
                "-t",
                pane.as_str(),
                "#{pane_kind} #{pane_current_command}",
            ],
        ));
        let provider = match facts {
            Ok(facts) if facts.exit_code == 0 => {
                let facts = facts.stdout.to_string();
                let Some(program) = facts.trim().strip_prefix("agent ") else {
                    reply.try_send(Err("found no chat in that pane".to_owned()));
                    return;
                };
                AgentProvider::ALL
                    .into_iter()
                    .find(|provider| provider.command_name() == program)
                    .unwrap_or_default()
            }
            Ok(facts) => {
                reply.try_send(Err(facts.stderr));
                return;
            }
            Err(error) => {
                reply.try_send(Err(error.to_string()));
                return;
            }
        };
        {
            let mut gate = self.gate.lock();
            if *gate == Gate::Dropped {
                return;
            }
            *gate = Gate::Sending;
        }
        if let Some(runtime) = self
            .shared
            .upgrade()
            .and_then(|shared| shared.open_agent_runtime())
        {
            runtime.count_send(self.caller, self.pane);
        }
        reply.try_send(Ok(()));
        let ran = remote(CommandInvocation::new(
            "agent-send",
            [
                "-t",
                pane.as_str(),
                "--wait",
                "--timeout",
                "0",
                "--json",
                "--",
                self.payload.as_str(),
            ],
        ));
        let report = relayed_report(&self.host, self.pane, provider, ran);
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        shared.deliver_to_chat(
            self.caller,
            ChatDelivery {
                from: self.pane,
                request: None,
                text: RemoteReport {
                    host: self.host,
                    report,
                }
                .to_string(),
            },
        );
    }
}

fn server_error(error: &DaemonError) -> Option<&ServerError> {
    match error {
        DaemonError::Server(error) => Some(error),
        DaemonError::CommandFailed { error, .. } => server_error(error),
        _ => None,
    }
}

fn relayed_report(
    host: &str,
    pane: PaneId,
    provider: AgentProvider,
    ran: Result<CommandOutcome, DaemonError>,
) -> AgentReport {
    let failed = |message: String| AgentReport::Failed {
        pane,
        provider,
        message,
    };
    let stopped = |reason: &str, text: String| AgentReport::Stopped {
        pane,
        provider,
        reason: reason.to_owned(),
        text,
    };
    let outcome = match ran {
        Ok(outcome) => outcome,
        Err(error) => {
            return match server_error(&error) {
                Some(ServerError::PaneExited(_)) => AgentReport::Closed { pane, provider },
                Some(error) => {
                    let message = error.tmux_message();
                    let turn = message
                        .strip_prefix(&format!("{pane}: turn "))
                        .unwrap_or(&message);
                    match turn.strip_prefix("failed: ") {
                        Some(why) => failed(why.to_owned()),
                        None if turn == "cancelled" => stopped("cancelled", String::new()),
                        None => failed(message),
                    }
                }
                None => failed(format!(
                    "lost the connection to {host} ({error}); read its last reply with: zz --host {host} capture-pane -p -t {pane}"
                )),
            };
        }
    };
    let Ok(reply) = serde_json::from_str::<serde_json::Value>(outcome.stdout.as_ref()) else {
        let said = outcome.stderr.trim();
        return failed(if said.is_empty() {
            format!("agent-send exited {} on {host}", outcome.exit_code)
        } else {
            said.to_owned()
        });
    };
    let field = |key: &str| {
        reply
            .get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
    };
    let text = match field("final_text") {
        last if last.trim().is_empty() => reply_tail(field("transcript")),
        last => reply_tail(last),
    };
    if outcome.exit_code == 0 {
        AgentReport::Finished {
            pane,
            provider,
            text,
        }
    } else {
        stopped(field("stop_reason"), text)
    }
}

#[cfg(test)]
#[path = "agent_relay_tests.rs"]
mod tests;
