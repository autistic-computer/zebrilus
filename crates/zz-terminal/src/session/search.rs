use super::*;

impl SearchMatch {
    pub(super) fn span(self, row: u32, columns: u16) -> Option<(u16, u16)> {
        if row < self.row || row > self.end_row {
            return None;
        }
        let start = if row == self.row { self.start } else { 0 };
        let end = if row == self.end_row {
            self.end
        } else {
            columns
        };
        Some((start.min(columns), end.min(columns)))
    }

    pub(super) fn contains(self, point: PointCoordinate, columns: u16) -> bool {
        self.span(point.y, columns)
            .is_some_and(|(start, end)| (start..end).contains(&point.x))
    }
}

pub(super) fn store_search_state(slot: &mut SearchSlot, state: SearchState) {
    if let Some(current) = slot.as_mut() {
        **current = state;
    } else {
        *slot = Some(Box::new(state));
    }
}

pub(super) const MAX_SEARCH_QUERY_BYTES: usize = 4096;
const MAX_SEARCH_MATCHES: usize = 1_000_000;

#[derive(Clone, Copy)]
pub(super) enum SearchSelectionPolicy {
    Last,
    From {
        point: PointCoordinate,
        direction: SearchDirection,
    },
    Preserve {
        index: Option<usize>,
        found: Option<SearchMatch>,
    },
}

static SEARCH_SCHEDULER: LazyLock<Sender<Arc<SearchMailbox>>> = LazyLock::new(|| {
    let (ready, mailboxes) = crossbeam_channel::unbounded();
    thread::Builder::new()
        .name("zz-terminal-search".into())
        .spawn(move || search_worker(&mailboxes))
        .expect("could not start the terminal search thread");
    ready
});

impl SearchWorker {
    pub(super) fn spawn(wake: ActorWake) -> (Self, Receiver<SearchResults>) {
        let (jobs, job_rx) = crossbeam_channel::bounded::<SearchJobs>(1);
        let discard_jobs = job_rx.clone();
        let (result_tx, results) = crossbeam_channel::bounded::<SearchResults>(1);
        let discard_results = results.clone();
        (
            Self {
                jobs,
                discard_jobs,
                latest_requests: HashMap::new(),
                next_request: 0,
                match_scratch: Vec::new(),
                mailbox: Arc::new(SearchMailbox {
                    queued: AtomicBool::new(false),
                    jobs: job_rx,
                    results: result_tx,
                    discard_results,
                    wake,
                }),
            },
            results,
        )
    }

    pub(super) fn next_request(&mut self, view_id: TerminalViewId) -> (u64, Arc<AtomicU64>) {
        self.next_request = self.next_request.wrapping_add(1).max(1);
        let latest = self
            .latest_requests
            .entry(view_id)
            .or_insert_with(|| Arc::new(AtomicU64::new(0)));
        latest.store(self.next_request, Ordering::Release);
        (self.next_request, Arc::clone(latest))
    }

    pub(super) fn cancel(&mut self, view_id: TerminalViewId) -> u64 {
        let (request_id, _) = self.next_request(view_id);
        if let Ok(mut pending) = self.discard_jobs.try_recv() {
            if let Some(mut discarded) = pending.by_view.remove(&view_id) {
                keep_larger_match_scratch(&mut self.match_scratch, &mut discarded.match_scratch);
            }
            if !pending.by_view.is_empty() && self.jobs.try_send(pending).is_ok() {
                self.schedule();
            }
        }
        request_id
    }

    pub(super) fn forget(&mut self, view_id: TerminalViewId) {
        self.cancel(view_id);
        self.latest_requests.remove(&view_id);
    }

    pub(super) fn is_current(&self, view_id: TerminalViewId, request_id: u64) -> bool {
        self.latest_requests
            .get(&view_id)
            .is_some_and(|latest| latest.load(Ordering::Acquire) == request_id)
    }

    fn recycle_matches(&mut self, matches: &mut Vec<SearchMatch>) {
        keep_larger_match_scratch(&mut self.match_scratch, matches);
    }

    fn schedule(&self) {
        if !self.mailbox.queued.swap(true, Ordering::AcqRel) {
            let _ = SEARCH_SCHEDULER.send(Arc::clone(&self.mailbox));
        }
    }

    pub(super) fn submit(&mut self, job: SearchJob) {
        let mut pending = SearchJobs::default();
        pending.by_view.insert(job.view_id, job);
        loop {
            match self.jobs.try_send(pending) {
                Ok(()) => {
                    self.schedule();
                    return;
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => return,
                Err(crossbeam_channel::TrySendError::Full(returned)) => {
                    pending = returned;
                    if let Ok(older) = self.discard_jobs.try_recv() {
                        merge_older_search_jobs(&mut pending, older);
                    }
                }
            }
        }
    }
}

fn merge_older_search_jobs(newer: &mut SearchJobs, older: SearchJobs) {
    for (view_id, mut job) in older.by_view {
        if let Some(replacement) = newer.by_view.get_mut(&view_id) {
            keep_larger_match_scratch(&mut replacement.match_scratch, &mut job.match_scratch);
        } else {
            newer.by_view.insert(view_id, job);
        }
    }
}

impl Drop for SearchWorker {
    fn drop(&mut self) {
        for latest in self.latest_requests.values() {
            latest.store(0, Ordering::Release);
        }
    }
}

fn search_worker(mailboxes: &Receiver<Arc<SearchMailbox>>) {
    let mut match_scratch = Vec::new();
    while let Ok(mailbox) = mailboxes.recv() {
        mailbox.queued.store(false, Ordering::Release);
        let Ok(pending) = mailbox.jobs.try_recv() else {
            continue;
        };
        let mut pending = pending.by_view.into_values().collect::<Vec<_>>();
        pending.sort_by_key(|job| job.view_id.0);
        for mut job in pending {
            keep_larger_match_scratch(&mut match_scratch, &mut job.match_scratch);
            if job.latest_request.load(Ordering::Acquire) != job.request_id {
                continue;
            }
            let Some(mut state) =
                job.snapshot
                    .search_reusing(&job.query, job.request_id, &mut match_scratch, || {
                        job.latest_request.load(Ordering::Acquire) != job.request_id
                    })
            else {
                continue;
            };
            select_search_result(&mut state, job.selection);
            if job.latest_request.load(Ordering::Acquire) != job.request_id {
                keep_larger_match_scratch(&mut match_scratch, &mut state.matches);
                continue;
            }
            let mut completed = SearchResults::default();
            completed.by_view.insert(
                job.view_id,
                SearchResult {
                    request_id: job.request_id,
                    view_id: job.view_id,
                    screen: job.screen,
                    state,
                },
            );
            if send_latest_search_results(
                &mailbox.results,
                &mailbox.discard_results,
                completed,
                &mut match_scratch,
            ) {
                mailbox.wake.notify();
            }
        }
    }
}

pub(super) fn send_latest_search_results(
    results: &Sender<SearchResults>,
    discard_results: &Receiver<SearchResults>,
    mut completed: SearchResults,
    match_scratch: &mut Vec<SearchMatch>,
) -> bool {
    loop {
        match results.try_send(completed) {
            Ok(()) => return true,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => return false,
            Err(crossbeam_channel::TrySendError::Full(returned)) => {
                completed = returned;
                if let Ok(older) = discard_results.try_recv() {
                    for (view_id, mut result) in older.by_view {
                        match completed.by_view.entry(view_id) {
                            Entry::Occupied(_) => {
                                keep_larger_match_scratch(match_scratch, &mut result.state.matches);
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(result);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn keep_larger_match_scratch(target: &mut Vec<SearchMatch>, candidate: &mut Vec<SearchMatch>) {
    target.clear();
    candidate.clear();
    if candidate.capacity() > target.capacity() {
        std::mem::swap(target, candidate);
    }
}

pub(super) fn select_search_result(state: &mut SearchState, selection: SearchSelectionPolicy) {
    if state.matches.is_empty() {
        return;
    }
    state.current = match selection {
        SearchSelectionPolicy::Last => Some(state.matches.len() - 1),
        SearchSelectionPolicy::From { point, direction } => match direction {
            SearchDirection::Forward => state
                .matches
                .iter()
                .position(|found| (found.row, found.start) > (point.y, point.x))
                .or(Some(0)),
            SearchDirection::Backward => state
                .matches
                .iter()
                .rposition(|found| (found.row, found.start) < (point.y, point.x))
                .or(Some(state.matches.len() - 1)),
        },
        SearchSelectionPolicy::Preserve { index, found } => found
            .and_then(|found| {
                state
                    .matches
                    .iter()
                    .position(|candidate| *candidate == found)
            })
            .or_else(|| index.map(|index| index.min(state.matches.len() - 1)))
            .or_else(|| Some(state.matches.len() - 1)),
    };
}

pub(super) fn search_selection_policy(
    copy_mode: Option<&CopyModeState>,
    origin: &mut Option<PointCoordinate>,
    direction: SearchDirection,
    reset_origin: bool,
) -> SearchSelectionPolicy {
    if reset_origin {
        *origin = None;
    }
    if origin.is_none()
        && let Some(mode) = copy_mode
    {
        *origin = Some(mode.cursor);
    }
    let Some(origin) = origin.as_ref() else {
        return SearchSelectionPolicy::Last;
    };
    SearchSelectionPolicy::From {
        point: *origin,
        direction,
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the target bundles exclusive view-state borrows for one queued request"
)]
pub(super) fn queue_search(
    terminal: &Terminal<'_, '_>,
    target: SearchTarget<'_>,
    worker: &mut SearchWorker,
    query: SearchQuery,
    selection: SearchSelectionPolicy,
) -> Result<(), WorkerError> {
    let (request_id, latest_request) = worker.next_request(target.view_id);
    let searchable = !query.text.is_empty() && query.text.len() <= MAX_SEARCH_QUERY_BYTES;
    let invalid_pattern = query.text.len() > MAX_SEARCH_QUERY_BYTES;
    if searchable && target.snapshot.is_none() {
        *target.snapshot = Some(Arc::new(HistorySearchSnapshot::capture(terminal)?));
    }
    let mut match_scratch = target
        .search
        .as_mut()
        .map_or_else(Vec::new, |state| std::mem::take(&mut state.matches));
    store_search_state(
        target.search,
        SearchState {
            query: query.clone(),
            matches: if searchable {
                Vec::new()
            } else {
                std::mem::take(&mut match_scratch)
            },
            request_id,
            pending: searchable,
            invalid_pattern,
            ..SearchState::default()
        },
    );
    if !searchable {
        return Ok(());
    }
    worker.submit(SearchJob {
        request_id,
        view_id: target.view_id,
        screen: target.screen,
        query,
        snapshot: Arc::clone(target.snapshot.as_ref().expect("captured above")),
        selection,
        match_scratch,
        latest_request,
    });
    Ok(())
}

pub(super) fn refresh_view_search(
    terminal: &Terminal<'_, '_>,
    view_id: TerminalViewId,
    view: &mut TerminalViewState,
    worker: &mut SearchWorker,
) -> Result<bool, WorkerError> {
    let Some(previous) = view.search.as_ref() else {
        worker.cancel(view_id);
        return Ok(false);
    };
    let query = previous.query.clone();
    let selection = SearchSelectionPolicy::Preserve {
        index: previous.current,
        found: previous
            .current
            .and_then(|index| previous.matches.get(index))
            .copied(),
    };
    let screen = view.screen;
    let current = view.active_mut();
    if current.search_snapshot.is_none()
        && let Some(mode) = current.copy_mode.as_ref()
    {
        current.search_snapshot = Some(Arc::clone(&mode.revision.search));
    }
    queue_search(
        terminal,
        SearchTarget {
            view_id,
            screen,
            search: &mut current.search,
            snapshot: &mut current.search_snapshot,
        },
        worker,
        query,
        selection,
    )?;
    Ok(true)
}

pub(super) fn apply_search_result(
    terminal: &mut Terminal<'_, '_>,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    worker: &mut SearchWorker,
    mut result: SearchResult,
) -> Result<bool, WorkerError> {
    if !worker.is_current(result.view_id, result.request_id) {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    }
    let is_active = active.contains_key(&result.view_id);
    let view = if let Some(view) = active.get_mut(&result.view_id) {
        view
    } else if let Some(view) = inactive.get_mut(&result.view_id) {
        view
    } else {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    };
    let current_screen = view.screen;
    let screen = view.screen_mut(result.screen);
    let Some(pending) = screen.search.as_mut() else {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    };
    if pending.request_id != result.request_id || pending.query != result.state.query {
        worker.recycle_matches(&mut result.state.matches);
        return Ok(false);
    }
    worker.recycle_matches(&mut pending.matches);
    store_search_state(&mut screen.search, result.state);
    if !is_active || current_screen != result.screen {
        return Ok(false);
    }
    {
        let current = view.active_mut();
        sync_copy_cursor_to_search(&mut current.copy_mode, current.search.as_deref());
        reveal_search_for_view(
            terminal,
            current.copy_mode.as_deref(),
            current.search.as_deref(),
        )?;
    }
    sync_viewport_anchor(terminal, view)?;
    Ok(true)
}

pub(super) fn apply_search_results(
    terminal: &mut Terminal<'_, '_>,
    active: &mut ActiveTerminalViews,
    inactive: &mut InactiveTerminalViews,
    worker: &mut SearchWorker,
    results: SearchResults,
) -> Result<bool, WorkerError> {
    let mut results = results.by_view.into_values().collect::<Vec<_>>();
    results.sort_by_key(|result| result.view_id.0);
    let mut changed = false;
    for result in results {
        changed |= apply_search_result(terminal, active, inactive, worker, result)?;
    }
    Ok(changed)
}

pub(super) fn complete_view_search(
    terminal: &mut Terminal<'_, '_>,
    view: &mut TerminalViewState,
) -> Result<(), WorkerError> {
    let Some(previous) = view.search.as_ref() else {
        return Ok(());
    };
    let query = previous.query.clone();
    let request_id = previous.request_id;
    let selection = SearchSelectionPolicy::Preserve {
        index: previous.current,
        found: previous
            .current
            .and_then(|index| previous.matches.get(index))
            .copied(),
    };
    let snapshot = view.copy_mode.as_ref().map_or_else(
        || HistorySearchSnapshot::capture(terminal).map(Arc::new),
        |mode| Ok(Arc::clone(&mode.revision.search)),
    )?;
    let mut state = snapshot
        .search(&query, request_id, || false)
        .expect("inline terminal search is never cancelled");
    select_search_result(&mut state, selection);
    view.search_snapshot = Some(snapshot);
    store_search_state(&mut view.search, state);
    {
        let current = view.active_mut();
        sync_copy_cursor_to_search(&mut current.copy_mode, current.search.as_deref());
        reveal_search_for_view(
            terminal,
            current.copy_mode.as_deref(),
            current.search.as_deref(),
        )?;
    }
    sync_viewport_anchor(terminal, view)
}

struct SearchSnapshotCompression<'a>(&'a HistorySearchSnapshot);

impl Drop for SearchSnapshotCompression<'_> {
    fn drop(&mut self) {
        let _ = self.0.terminal.lock().compress(CompressionMode::Full);
    }
}

impl HistorySearchSnapshot {
    pub(super) fn capture(terminal: &Terminal<'_, '_>) -> Result<Self, WorkerError> {
        Ok(Self {
            columns: terminal.cols()?,
            total_rows: u32::try_from(terminal.total_rows()?)
                .map_err(|_| WorkerError::ViewportMetadataTooLarge)?,
            terminal: Arc::new(Mutex::new(terminal.clone_screen()?)),
            #[cfg(test)]
            search_gate: Mutex::new(()),
        })
    }

    pub(super) fn search(
        &self,
        query: &SearchQuery,
        request_id: u64,
        cancelled: impl Fn() -> bool,
    ) -> Option<SearchState> {
        let mut match_scratch = Vec::new();
        self.search_reusing(query, request_id, &mut match_scratch, cancelled)
    }

    pub(super) fn search_reusing(
        &self,
        query: &SearchQuery,
        request_id: u64,
        match_scratch: &mut Vec<SearchMatch>,
        cancelled: impl Fn() -> bool,
    ) -> Option<SearchState> {
        #[cfg(test)]
        let _search_gate = self.search_gate.lock();
        match_scratch.clear();
        if cancelled() {
            return None;
        }
        if query.text.is_empty() || query.text.len() > MAX_SEARCH_QUERY_BYTES {
            return Some(SearchState {
                query: query.clone(),
                matches: std::mem::take(match_scratch),
                request_id,
                invalid_pattern: query.text.len() > MAX_SEARCH_QUERY_BYTES,
                ..SearchState::default()
            });
        }
        let pattern = match query.mode {
            SearchMode::Literal => regex::escape(&query.text),
            SearchMode::Regex => query.text.clone(),
        };
        let case_sensitive = match query.case {
            SearchCase::Smart => query.text.chars().any(char::is_uppercase),
            SearchCase::Sensitive => true,
            SearchCase::Insensitive => false,
        };
        let Ok(expression) = RegexBuilder::new(&pattern)
            .case_insensitive(!case_sensitive)
            .size_limit(1024 * 1024)
            .dfa_size_limit(1024 * 1024)
            .build()
        else {
            return Some(SearchState {
                query: query.clone(),
                matches: std::mem::take(match_scratch),
                request_id,
                invalid_pattern: true,
                ..SearchState::default()
            });
        };
        let _compression = SearchSnapshotCompression(self);
        let mut text = String::new();
        let mut offsets = Vec::new();
        let mut graphemes = Vec::new();
        let mut wrapped_rows = Vec::new();
        for row in 0..self.total_rows {
            if cancelled() {
                return None;
            }
            {
                let mut terminal = self.terminal.lock();
                if row > 0 && row % 512 == 0 {
                    terminal.compress(CompressionMode::Full).ok()?;
                }
                let text_start = u32::try_from(text.len()).ok()?;
                let offset_start = offsets.len();
                let wrapped = append_history_row(
                    &*terminal,
                    row,
                    self.columns,
                    &mut text,
                    &mut offsets,
                    &mut graphemes,
                )
                .ok()?;
                for offset in &mut offsets[offset_start..] {
                    offset.start = offset.start.checked_add(text_start)?;
                    offset.end = offset.end.checked_add(text_start)?;
                }
                wrapped_rows.push((row, offsets.len()));
                if wrapped && row.saturating_add(1) < self.total_rows {
                    continue;
                }
            }
            for found in expression.find_iter(&text) {
                let Some((start, end)) =
                    search_match_span(&offsets, found.start(), found.end(), self.columns)
                else {
                    continue;
                };
                let (start_row, end_row) = {
                    let first = offsets.partition_point(|cell| {
                        usize::try_from(cell.end).is_ok_and(|end| end <= found.start())
                    });
                    let after_last = offsets.partition_point(|cell| {
                        usize::try_from(cell.start).is_ok_and(|start| start < found.end())
                    });
                    let start = wrapped_rows.partition_point(|(_, end)| *end <= first);
                    let end = wrapped_rows.partition_point(|(_, end)| *end < after_last);
                    (wrapped_rows.get(start)?.0, wrapped_rows.get(end)?.0)
                };
                match_scratch.push(SearchMatch {
                    row: start_row,
                    end_row,
                    start,
                    end,
                });
                if match_scratch.len() >= MAX_SEARCH_MATCHES {
                    return Some(SearchState {
                        query: query.clone(),
                        matches: std::mem::take(match_scratch),
                        request_id,
                        ..SearchState::default()
                    });
                }
            }
            text.clear();
            offsets.clear();
            wrapped_rows.clear();
        }
        Some(SearchState {
            query: query.clone(),
            matches: std::mem::take(match_scratch),
            request_id,
            ..SearchState::default()
        })
    }
}

pub(super) fn search_match_span(
    offsets: &[SearchCellOffset],
    byte_start: usize,
    byte_end: usize,
    columns: u16,
) -> Option<(u16, u16)> {
    if byte_start >= byte_end {
        return None;
    }
    let first = offsets
        .partition_point(|cell| usize::try_from(cell.end).is_ok_and(|end| end <= byte_start));
    let after_last = offsets
        .partition_point(|cell| usize::try_from(cell.start).is_ok_and(|start| start < byte_end));
    if first >= after_last {
        return None;
    }
    let first = offsets.get(first)?;
    let last = offsets.get(after_last - 1)?;
    Some((
        first.column,
        last.column.saturating_add(last.width).min(columns),
    ))
}

#[cfg(test)]
pub(super) fn search_history(
    terminal: &Terminal<'_, '_>,
    query: &str,
) -> Result<SearchState, WorkerError> {
    Ok(HistorySearchSnapshot::capture(terminal)?
        .search(&SearchQuery::literal(query), 1, || false)
        .expect("synchronous fixture is never cancelled"))
}

pub(super) fn append_history_row(
    terminal: &impl libghostty_vt::terminal::GridRead,
    row: u32,
    columns: u16,
    text: &mut String,
    offsets: &mut Vec<SearchCellOffset>,
    grapheme_scratch: &mut Vec<char>,
) -> Result<bool, WorkerError> {
    let row_text_start = text.len();
    let mut stack = ['\0'; 8];
    let native_row = terminal.grid_row(Point::Screen(PointCoordinate { x: 0, y: row }))?;
    let wrapped = native_row.row()?.is_wrapped()?;
    for column in 0..columns {
        let grid_ref = native_row.cell(column).expect("history column");
        let wide = grid_ref.cell()?.wide()?;
        if matches!(wide, CellWide::SpacerTail | CellWide::SpacerHead) {
            continue;
        }
        let start = u32::try_from(text.len().saturating_sub(row_text_start))
            .map_err(|_| WorkerError::SearchSnapshotTooLarge)?;
        match grid_ref.graphemes(&mut stack) {
            Ok(0) => {}
            Ok(1) => text.push(stack[0]),
            Ok(count) => text.extend(stack[..count].iter()),
            Err(libghostty_vt::Error::OutOfSpace { required }) => {
                if required > MAX_SEARCH_SNAPSHOT_BYTES / std::mem::size_of::<char>() {
                    return Err(WorkerError::SearchSnapshotTooLarge);
                }
                if grapheme_scratch.len() < required {
                    grapheme_scratch.resize(required, '\0');
                }
                let count = grid_ref.graphemes(grapheme_scratch)?;
                text.extend(grapheme_scratch[..count].iter());
            }
            Err(error) => return Err(error.into()),
        }
        let end = u32::try_from(text.len().saturating_sub(row_text_start))
            .map_err(|_| WorkerError::SearchSnapshotTooLarge)?;
        if end > start {
            offsets.push(SearchCellOffset {
                start,
                end,
                column,
                width: if matches!(wide, CellWide::Wide) { 2 } else { 1 },
            });
        }
    }
    Ok(wrapped)
}

pub(super) fn remember_copy_mode_search(
    mode: Option<&mut CopyModeState>,
    pane_search: &mut Option<CopyModeSearch>,
    query: &SearchQuery,
) {
    if query.text.is_empty() {
        return;
    }
    if let Some(mode) = mode {
        let search = CopyModeSearch {
            text: query.text.clone(),
            direction: query.direction,
            regex: query.mode == SearchMode::Regex,
            incremental: false,
        };
        mode.search = Some(search.clone());
        *pane_search = Some(search);
    }
}

pub(super) fn copy_mode_search_again(
    mode: Option<&CopyModeState>,
    reverse: bool,
) -> Option<CopyModeSearch> {
    let stored = mode?.search.as_ref()?;
    let direction = match (stored.direction, reverse) {
        (SearchDirection::Forward, false) | (SearchDirection::Backward, true) => {
            SearchDirection::Forward
        }
        (SearchDirection::Backward, false) | (SearchDirection::Forward, true) => {
            SearchDirection::Backward
        }
    };
    Some(CopyModeSearch {
        direction,
        incremental: false,
        ..stored.clone()
    })
}

fn pick_copy_search_match(
    matches: &[SearchMatch],
    cursor: PointCoordinate,
    direction: SearchDirection,
    inclusive: bool,
    wrap: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let found = match direction {
        SearchDirection::Forward => matches.iter().position(|found| {
            if inclusive {
                (found.row, found.start) >= (cursor.y, cursor.x)
            } else {
                (found.row, found.start) > (cursor.y, cursor.x)
            }
        }),
        SearchDirection::Backward => matches
            .iter()
            .rposition(|found| (found.row, found.start) < (cursor.y, cursor.x)),
    };
    if found.is_some() {
        return found;
    }
    if !wrap {
        return None;
    }
    match direction {
        SearchDirection::Forward => Some(0),
        SearchDirection::Backward => Some(matches.len() - 1),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "one call site threads the view's search state"
)]
pub(super) fn run_copy_mode_search(
    view_id: TerminalViewId,
    copy_mode: &mut CopyModeSlot,
    search: &mut SearchSlot,
    search_snapshot: &mut Option<Arc<HistorySearchSnapshot>>,
    worker: &mut SearchWorker,
    spec: &CopyModeSearch,
    count: u32,
    mode_keys_vi: bool,
    wrap: bool,
    pane_search: &mut Option<CopyModeSearch>,
) -> bool {
    let Some(mode) = copy_mode.as_deref_mut() else {
        return false;
    };
    if spec.incremental {
        match mode.incremental_origin {
            None => {
                mode.incremental_origin = Some(CopyModeIncrementalOrigin {
                    row: mode.cursor.y,
                    viewport_offset: mode.viewport_offset,
                });
            }
            Some(origin) => {
                if mode
                    .search
                    .as_ref()
                    .is_some_and(|previous| previous.text != spec.text)
                {
                    mode.viewport_offset = origin.viewport_offset;
                    mode.cursor = PointCoordinate {
                        x: copy_cursor_limit(&mode.revision, origin.row, mode_keys_vi, false),
                        y: origin.row,
                    };
                }
            }
        }
        if spec.text.is_empty() {
            mode.search_marks = false;
            mode.search_count = None;
            *search = None;
            return true;
        }
    } else if spec.text.is_empty() {
        return false;
    }

    let regex = spec.regex && spec.text.contains(|c| "^$*+()?[].\\".contains(c));
    let visible_only = !mode.search_all
        && pane_search
            .as_ref()
            .is_some_and(|stored| stored.text == spec.text && stored.regex == regex);
    mode.search_all = false;
    if !visible_only && mode.search_marks {
        mode.search_marks = false;
        mode.search_count = None;
    }
    mode.search = Some(spec.clone());
    *pane_search = Some(CopyModeSearch {
        regex,
        incremental: false,
        ..spec.clone()
    });
    let query = SearchQuery {
        text: spec.text.clone(),
        mode: if regex {
            SearchMode::Regex
        } else {
            SearchMode::Literal
        },
        case: SearchCase::Smart,
        direction: spec.direction,
    };
    if search_snapshot.is_none() {
        *search_snapshot = Some(Arc::clone(&mode.revision.search));
    }
    let snapshot = Arc::clone(search_snapshot.as_ref().expect("captured above"));
    let (request_id, _) = worker.next_request(view_id);
    let mut scratch = Vec::new();
    let Some(mut state) = snapshot.search_reusing(&query, request_id, &mut scratch, || false)
    else {
        return false;
    };
    let inclusive = !mode_keys_vi && spec.direction == SearchDirection::Forward;
    let mut placed = None;
    let mut cursor = mode.cursor;
    for _ in 0..count.max(1) {
        let Some(index) =
            pick_copy_search_match(&state.matches, cursor, spec.direction, inclusive, wrap)
        else {
            break;
        };
        let Some(found) = state.matches.get(index).copied() else {
            break;
        };
        cursor = PointCoordinate {
            x: if inclusive { found.end } else { found.start },
            y: if inclusive { found.end_row } else { found.row },
        };
        placed = Some(index);
    }
    let Some(index) = placed else {
        state.current = None;
        store_search_state(search, state);
        return true;
    };
    state.current = Some(index);
    let total = u32::try_from(state.matches.len()).unwrap_or(u32::MAX);
    store_search_state(search, state);
    mode.search_marks = true;
    if !visible_only {
        mode.search_count = Some((total, false));
    }
    place_copy_cursor(mode, cursor, mode_keys_vi);
    scroll_copy_view_to_cursor(mode);
    if mode.selecting {
        update_copy_selection(mode, None);
    }
    true
}

pub(super) fn step_search(search: &mut SearchSlot, direction: isize, wrap: bool) {
    let Some(search) = search.as_mut() else {
        return;
    };
    if search.matches.is_empty() {
        search.current = None;
        return;
    }
    let len = isize::try_from(search.matches.len()).unwrap_or(isize::MAX);
    let current = isize::try_from(search.current.unwrap_or(0)).unwrap_or(0);
    let next = if wrap {
        current.saturating_add(direction).rem_euclid(len)
    } else {
        current.saturating_add(direction).clamp(0, len - 1)
    };
    search.current = usize::try_from(next).ok();
}

pub(super) fn sync_copy_cursor_to_search(
    copy_mode: &mut CopyModeSlot,
    search: Option<&SearchState>,
) {
    let Some(mode) = copy_mode.as_mut() else {
        return;
    };
    let Some(found) = search.and_then(|search| {
        search
            .current
            .and_then(|current| search.matches.get(current))
    }) else {
        return;
    };
    mode.cursor = mode.revision.clamp_point(PointCoordinate {
        x: found.start,
        y: found.row,
    });
    reveal_copy_cursor(mode);
    if mode.selecting {
        update_copy_selection(mode, None);
    }
}

pub(super) fn reveal_search_for_view(
    terminal: &mut Terminal<'_, '_>,
    copy_mode: Option<&CopyModeState>,
    search: Option<&SearchState>,
) -> Result<(), WorkerError> {
    if copy_mode.is_none()
        && let Some(search) = search
    {
        reveal_search_match(terminal, search)?;
    }
    Ok(())
}

fn reveal_search_match(
    terminal: &mut Terminal<'_, '_>,
    search: &SearchState,
) -> Result<(), WorkerError> {
    let Some(found) = search
        .current
        .and_then(|current| search.matches.get(current))
    else {
        return Ok(());
    };
    let half_page = u32::from(terminal.rows()?) / 2;
    let top = found.row.saturating_sub(half_page);
    terminal.scroll_viewport(ScrollViewport::Top);
    terminal.scroll_viewport(ScrollViewport::Delta(saturating_isize(i64::from(top))));
    Ok(())
}
