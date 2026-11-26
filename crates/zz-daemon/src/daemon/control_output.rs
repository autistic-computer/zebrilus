use super::*;

impl Shared {
    #[cfg(test)]
    pub(super) fn publish_control_output_for_pane(&self, pane: PaneId, bytes: &Arc<[u8]>) {
        let mailboxes = {
            let inner = self.inner.lock();
            let Some(window) = inner.engine.state.window_for_pane(pane) else {
                return;
            };
            inner
                .engine
                .state
                .sessions
                .iter()
                .filter(|(_, session)| session.windows.contains(&window))
                .flat_map(|(session, _)| {
                    inner.attachment.attached.get(session).into_iter().flatten()
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|client| {
                    inner
                        .client(*client)
                        .filter(|c| c.kind == Some(ClientKind::Control))
                        .and_then(|c| c.subscriber.clone())
                })
                .collect::<Vec<_>>()
        };
        for mailbox in mailboxes {
            let _ = mailbox
                .ensure_control_feed(&self.control_wake)
                .push(pane, bytes, None);
        }
        self.pump_control_output_at(Instant::now());
    }

    pub(super) fn turn_control_output(self: &Arc<Self>, due: bool) {
        if !self.control_wake.take() && !due {
            return;
        }
        let (pending, progressed) = self.pump_control_output_at(Instant::now());
        if !pending {
            return;
        }
        self.control_wake.again(progressed);
        #[cfg(windows)]
        {
            let weak = Arc::downgrade(self);
            let delay = if progressed {
                Duration::ZERO
            } else {
                COPY_PIPE_POLL_INTERVAL
            };
            let _ = self.spawn_delay(delay, move || {
                if let Some(shared) = weak.upgrade() {
                    shared.turn_control_output(true);
                }
            });
        }
    }

    #[cfg(unix)]
    pub(super) fn control_output_deadline(&self) -> Option<Instant> {
        let inner = self.inner.lock();
        inner
            .clients
            .values()
            .filter_map(|client| {
                let output = client.control_output.as_ref()?;
                let oldest = client.subscriber.as_ref()?.control_feed()?.oldest()?;
                Some(
                    oldest
                        + output
                            .pause_after_ms
                            .map_or(CONTROL_MAXIMUM_AGE, Duration::from_millis),
                )
            })
            .min()
    }

    pub(super) fn pump_control_output_at(&self, now: Instant) -> (bool, bool) {
        let clients = {
            let inner = self.inner.lock();
            inner
                .clients
                .iter()
                .filter_map(|(id, client)| {
                    let subscriber = client.subscriber.clone()?;
                    subscriber.control_feed()?;
                    let output = client.control_output.as_ref();
                    Some((
                        *id,
                        subscriber,
                        output.and_then(|output| output.pause_after_ms),
                    ))
                })
                .collect::<Vec<_>>()
        };
        let active = clients
            .iter()
            .filter_map(|(_, subscriber, _)| subscriber.control_feed())
            .flat_map(shard_sink::ControlFeed::pending_panes)
            .collect::<BTreeSet<_>>();
        for pane in active {
            self.note_control_output_activity(pane);
        }
        let mut pending = false;
        let mut progressed = false;
        for (client, subscriber, pause_after_ms) in clients {
            let Some(feed) = subscriber.control_feed() else {
                continue;
            };
            let outcome = feed.pump(&subscriber, pause_after_ms, now);
            pending |= outcome.pending;
            progressed |= outcome.progressed;
            if !outcome.paused.is_empty() {
                let mut inner = self.inner.lock();
                if let Some(output) = inner
                    .client_mut(client)
                    .and_then(|c| c.control_output.as_mut())
                {
                    for pane in outcome.paused {
                        output.panes.entry(pane).or_default().mode = ControlPaneOutputMode::Paused;
                    }
                }
            }
            if outcome.kill {
                subscriber.close_too_far_behind();
            }
        }
        (pending, progressed)
    }
}
