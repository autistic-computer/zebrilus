use super::*;

impl MuxClient {
    pub(super) fn request_history_backfill(&mut self, pane: PaneId) {
        self.request_history(pane, None, Instant::now());
    }

    fn defer_history_backfill(
        &mut self,
        pane: PaneId,
        history_mutations: u64,
        cx: &mut Context<Self>,
    ) {
        let host = self.attached_host;
        if !self
            .attached_connection_mut()
            .history
            .defer(pane, history_mutations)
        {
            return;
        }

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(HISTORY_BACKFILL_QUIET).await;
                let rearm = this.update(cx, |client, _cx| {
                    if client.attached_host != host {
                        return false;
                    }
                    let mutations = client
                        .viewports
                        .get(&pane)
                        .map(|retained| retained.read().history_mutations);
                    let Some(connection) = client.connections.get_mut(&host) else {
                        return false;
                    };
                    match connection.history.resume_deferred(pane, mutations) {
                        DeferredBackfill::Request => {
                            client.request_history_backfill(pane);
                            false
                        }
                        DeferredBackfill::Wait => true,
                        DeferredBackfill::Done => false,
                    }
                });
                if !matches!(rearm, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn request_history_prefetch(&mut self, pane: PaneId, target_offset: u32) {
        self.request_history(pane, Some(target_offset), Instant::now());
    }

    pub(super) fn request_history(
        &mut self,
        pane: PaneId,
        prefetch_target: Option<u32>,
        now: Instant,
    ) {
        let trickle_budget = self.core.history_trickle_budget();
        let retained = self.viewports.get(&pane).cloned();
        let retained = retained.as_ref().map(|retained| retained.read());
        let connection = self.attached_connection_mut();
        let Some((start, count)) = connection.history.request(
            pane,
            prefetch_target,
            trickle_budget,
            retained.as_deref(),
            now,
        ) else {
            return;
        };
        drop(retained);
        if let Some(client) = &connection.client {
            if let Err(error) = client.request_history(pane, start, count) {
                connection.history.forget(pane);
                log::warn!("failed to request terminal history for {pane}: {error}");
            }
            return;
        }
        #[cfg(test)]
        if let Some(client) = &connection.fake_client {
            client.request_history(pane, start, count);
            return;
        }
        connection.history.forget(pane);
    }

    pub(super) fn backfill_retained_history(&mut self) {
        let panes = self.viewports.keys().copied().collect::<Vec<_>>();
        for pane in panes {
            self.request_history_backfill(pane);
        }
    }

    pub(super) fn apply_history_chunk_event(
        &mut self,
        pane: PaneId,
        start: u32,
        total: u32,
        offset: u32,
        columns: u16,
        rows: Vec<Vec<PackedCell>>,
        dictionary: TerminalDictionary,
        cx: &mut Context<Self>,
    ) {
        let retained = self.viewports.get(&pane).cloned();
        let mutations = retained
            .as_ref()
            .map(|retained| retained.read().history_mutations);
        let (apply, follow_up) = self
            .attached_connection_mut()
            .history
            .chunk_arrived(pane, mutations);
        if apply && let Some(retained) = retained {
            apply_history_chunk(
                &mut retained.write(),
                start,
                total,
                offset,
                columns,
                rows,
                dictionary,
                &mut self.next_row_revision,
            );
        }
        match follow_up {
            HistoryFollowUp::Prefetch(target) => self.request_history_prefetch(pane, target),
            HistoryFollowUp::Defer(mutations) => self.defer_history_backfill(pane, mutations, cx),
            HistoryFollowUp::Backfill => self.request_history_backfill(pane),
        }
    }
}
