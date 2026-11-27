use super::*;

impl MuxClient {
    #[cfg(feature = "agent-pane")]
    pub(super) fn apply_agent_updates(
        &mut self,
        pane: PaneId,
        first_seq: u64,
        items: Vec<Vec<u8>>,
    ) {
        if !self.watched_agents.contains(&pane) {
            return;
        }
        let cursor = self.agent_cursors.entry(pane).or_default();
        let (accepted, gapped) = accept_agent_items(pane, cursor, first_seq, items);
        if !accepted.is_empty() {
            self.agent_events.items.push((pane, accepted));
        }
        if gapped {
            if !self.agent_replays_pending.contains(&pane) {
                self.request_agent_replay_from_cursor(pane);
            }
        } else {
            self.agent_replays_pending.remove(&pane);
        }
    }

    #[cfg(not(feature = "agent-pane"))]
    pub(super) fn apply_agent_updates(
        &mut self,
        _pane: PaneId,
        _first_seq: u64,
        _items: Vec<Vec<u8>>,
    ) {
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn request_agent_replay_from_cursor(&mut self, pane: PaneId) -> bool {
        let from_seq = self.agent_cursors.get(&pane).copied().unwrap_or_default();
        let sent = self.send_agent_request(pane, AgentRequest::Replay { from_seq });
        if sent {
            self.agent_replays_pending.insert(pane);
        }
        sent
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn watch_agent(&mut self, pane: PaneId, from_seq: u64) {
        self.watched_agents.insert(pane);
        if !self.agent_cursors.contains_key(&pane)
            && self.send_agent_request(pane, AgentRequest::Replay { from_seq })
        {
            self.agent_cursors.insert(pane, from_seq);
            self.agent_replays_pending.insert(pane);
        }
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn unwatch_agent(&mut self, pane: PaneId) {
        self.watched_agents.remove(&pane);
        self.agent_cursors.remove(&pane);
        self.agent_replays_pending.remove(&pane);
        self.agent_events
            .items
            .retain(|(target, _)| *target != pane);
        self.agent_events
            .states
            .retain(|(target, _)| *target != pane);
        self.agent_events
            .sessions
            .retain(|(target, _)| *target != pane);
    }

    #[cfg(feature = "agent-pane")]
    pub(super) fn clear_agent_streams(&mut self) {
        self.agent_cursors.clear();
        self.agent_replays_pending.clear();
        self.agent_events = AgentEvents::default();
    }

    #[cfg(not(feature = "agent-pane"))]
    pub(super) fn clear_agent_streams(&mut self) {}

    #[cfg(feature = "agent-pane")]
    pub(crate) fn take_agent_events_for(&mut self, panes: &BTreeSet<PaneId>) -> AgentEvents {
        let pending = std::mem::take(&mut self.agent_events);
        let mut ready = AgentEvents::default();
        for (pane, items) in pending.items {
            if panes.contains(&pane) {
                ready.items.push((pane, items));
            } else {
                self.agent_events.items.push((pane, items));
            }
        }
        for (pane, state) in pending.states {
            if panes.contains(&pane) {
                ready.states.push((pane, state));
            } else {
                self.agent_events.states.push((pane, state));
            }
        }
        ready.sessions = pending
            .sessions
            .into_iter()
            .filter(|(pane, _)| panes.contains(pane))
            .collect();
        ready
    }

    #[cfg(feature = "agent-pane")]
    #[must_use]
    pub(crate) fn has_agent_events(&self) -> bool {
        !self.agent_events.is_empty()
    }

    #[cfg(feature = "agent-pane")]
    pub(crate) fn send_agent_request(&self, pane: PaneId, request: AgentRequest) -> bool {
        #[cfg(test)]
        if let Some(sink) = &self.agent_sink {
            sink.borrow_mut().push((pane, request));
            return true;
        }
        let Some(client) = self
            .connections
            .get(&self.attached_host)
            .and_then(|connection| connection.client.as_ref())
        else {
            log::trace!(target: "zz::diagnostics::mux", "agent request skipped: no client");
            return false;
        };
        let sent = match request {
            AgentRequest::Prompt { text, images } => client.agent_prompt(pane, text, images),
            AgentRequest::Cancel => client.agent_cancel(pane),
            AgentRequest::Unqueue => client.agent_unqueue(pane),
            AgentRequest::RespondPermission {
                request_id,
                option_id,
            } => client.agent_respond_permission(pane, request_id, option_id),
            AgentRequest::Replay { from_seq } => client.agent_replay(pane, from_seq),
            AgentRequest::SetConfigOption { option_id, value } => {
                client.send(&zz_protocol::ProtocolMessage::AgentSetConfigOption {
                    pane,
                    option_id,
                    value,
                })
            }
            AgentRequest::AcknowledgePromptRestore { reclaim_id } => {
                client.agent_acknowledge_prompt_restore(pane, reclaim_id)
            }
            AgentRequest::AnswerQuestion {
                request_id,
                answers,
            } => client.send(&zz_protocol::ProtocolMessage::AgentAnswerQuestion {
                pane,
                request_id,
                answers,
            }),
            AgentRequest::SessionOp(op) => client.agent_session_op(pane, op),
        };
        if let Err(error) = sent {
            log::warn!("failed to send an agent request for {pane}: {error}");
            return false;
        }
        true
    }
}
