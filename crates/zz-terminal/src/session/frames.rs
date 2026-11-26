use super::*;

pub(super) const ECHO_WINDOW: Duration = Duration::from_millis(50);
const ECHO_PUBLISHES: u8 = 4;

#[derive(Default)]
pub(super) struct EchoWindow {
    opened: Option<Instant>,
    publishes: u8,
}

impl EchoWindow {
    pub(super) fn open(&mut self) {
        self.open_at(Instant::now());
    }

    pub(super) fn open_at(&mut self, opened: Instant) {
        self.opened = Some(opened);
        self.publishes = ECHO_PUBLISHES;
    }

    pub(super) fn due(&self) -> bool {
        self.publishes > 0
            && self
                .opened
                .is_some_and(|opened| opened.elapsed() < ECHO_WINDOW)
    }

    pub(super) fn spend(&mut self) {
        self.publishes = self.publishes.saturating_sub(1);
    }
}

pub(super) const COMPRESS_IDLE: Duration = Duration::from_secs(1);
const COMPRESS_STEP_BUDGET: Duration = Duration::from_millis(2);

#[derive(Default)]
pub(super) struct IdleCompression {
    activity: Option<libghostty_vt::terminal::CompressionActivity>,
    due: Option<Instant>,
    unsupported: bool,
}

impl IdleCompression {
    fn enabled(&self) -> bool {
        !self.unsupported
    }

    pub(super) fn observe(&mut self, terminal: &Terminal<'_, '_>, now: Instant) {
        if !self.enabled() {
            return;
        }
        let Ok(activity) = terminal.compression_activity() else {
            return;
        };
        if self.activity != Some(activity) {
            self.activity = Some(activity);
            self.due = Some(now + COMPRESS_IDLE);
        }
    }

    pub(super) fn rearm(&mut self) {
        if self.enabled() {
            self.due = Some(Instant::now() + COMPRESS_IDLE);
        }
    }

    pub(super) const fn due(&self) -> Option<Instant> {
        self.due
    }

    pub(super) fn run(&mut self, terminal: &mut Terminal<'_, '_>) {
        use libghostty_vt::terminal::{CompressionMode, CompressionResult};

        if self.due.is_none_or(|due| Instant::now() < due) {
            return;
        }
        let started = Instant::now();
        loop {
            match terminal.compress(CompressionMode::Incremental) {
                Ok(CompressionResult::Pending) => {
                    if started.elapsed() >= COMPRESS_STEP_BUDGET {
                        self.due = Some(Instant::now());
                        break;
                    }
                }
                Ok(CompressionResult::Complete) => {
                    self.due = None;
                    break;
                }
                Ok(CompressionResult::Unsupported) => {
                    self.unsupported = true;
                    self.due = None;
                    break;
                }
                Err(error) => {
                    log::debug!("terminal history compression stopped: {error}");
                    self.due = None;
                    break;
                }
            }
        }
        log::trace!(
            target: "zz_terminal::diagnostics::compression",
            "compress step elapsed_us={} pending={}",
            started.elapsed().as_micros(),
            self.due.is_some(),
        );
        self.activity = terminal.compression_activity().ok();
    }
}

pub(super) const UNWATCHED_SETTLE_QUIET: Duration = Duration::from_millis(100);
const UNWATCHED_SETTLE_MAX: Duration = Duration::from_secs(1);
pub(super) const VIEWED_RENDER_RETAIN: Duration = Duration::from_secs(5);
pub(super) const UNWATCHED_NOTIFY_INTERVAL: Duration = Duration::from_millis(100);

pub(super) struct RenderResources<'alloc> {
    state: RenderState<'alloc>,
    rows: RowIterator<'alloc>,
    cells: CellIterator<'alloc>,
}

impl<'alloc> RenderResources<'alloc> {
    fn new(terminal: &Terminal<'alloc, '_>) -> Result<Self, WorkerError> {
        let mut state = RenderState::new()?;
        state.set_clip(Clip {
            y: 0,
            rows: 1,
            cols: 1,
        })?;
        state.update(terminal)?.set_dirty(Dirty::Full)?;
        Ok(Self {
            state,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
        })
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct StreamState {
    stream: ViewStream,
    epoch: u64,
}

#[derive(PartialEq)]
pub(super) struct SinkEdge {
    title: Arc<str>,
    working_directory: Option<Arc<str>>,
    status: SessionStatus,
}

impl SinkEdge {
    fn of(viewport: &TerminalViewport) -> Self {
        Self {
            title: Arc::clone(&viewport.presentation.title),
            working_directory: viewport.presentation.working_directory.clone(),
            status: viewport.status.clone(),
        }
    }
}

impl<'alloc> Frames<'alloc> {
    pub(super) fn new(appearance: &TerminalAppearance) -> Result<Self, WorkerError> {
        Ok(Self {
            #[cfg(test)]
            snapshot_builds: 0,
            render: None,
            generations: ViewportGenerations::new()?,
            dictionary: ViewportDictionary {
                class_hints: ClassHints::new(appearance),
                ..ViewportDictionary::default()
            },
            streams: HashMap::new(),
            areas: HashMap::new(),
            idle_bound: None,
            preview: false,
            force_fallback: false,
            mode_views: HashSet::new(),
            published: HashSet::new(),
            unbuilt_since: None,
            last_unbuilt: None,
            last_notify: None,
            notify_owed: false,
            synchronized_output_deadline: None,
            retain_render_until: None,
            watched: false,
            last_settle: None,
            sink_edge: None,
        })
    }

    fn defer_synchronized_output(
        &mut self,
        terminal: &mut Terminal<'_, '_>,
        status: &SessionStatus,
    ) -> Result<bool, WorkerError> {
        if !terminal.mode(Mode::SYNC_OUTPUT)? {
            self.synchronized_output_deadline = None;
            return Ok(false);
        }
        let now = Instant::now();
        let deadline = *self
            .synchronized_output_deadline
            .get_or_insert(now + SYNCHRONIZED_OUTPUT_TIMEOUT);
        if matches!(status, SessionStatus::Running) && now < deadline {
            return Ok(true);
        }
        self.synchronized_output_deadline = None;
        terminal.set_mode(Mode::SYNC_OUTPUT, false)?;
        Ok(false)
    }

    pub(super) fn unwatched(&self, active: &ActiveTerminalViews) -> bool {
        !self.needs_cells(active)
            && self.mode_views.is_empty()
            && active.values().all(|view| view.copy_mode.is_none())
    }

    fn streaming(&self, view: TerminalViewId) -> bool {
        self.streams
            .get(&view)
            .is_some_and(|state| state.stream.is_on())
    }

    pub(super) fn epoch(&self, view: TerminalViewId) -> u64 {
        self.streams.get(&view).map_or(0, |state| state.epoch)
    }

    pub(super) fn set_stream(&mut self, view: TerminalViewId, stream: ViewStream) -> bool {
        let restart = stream.is_on() && !self.published.contains(&view);
        let state = self.streams.entry(view).or_default();
        state.stream = stream;
        if restart {
            state.epoch = state.epoch.wrapping_add(1);
        }
        restart
    }

    pub(super) fn shows(&self, view: TerminalViewId, active: &ActiveTerminalViews) -> bool {
        self.streaming(view)
            || self.published.contains(&view)
            || active
                .get(&view)
                .is_some_and(|state| state.copy_mode.is_some())
    }

    pub(super) fn forget_view(&mut self, view: TerminalViewId) {
        self.streams.remove(&view);
        self.areas.remove(&view);
        self.mode_views.remove(&view);
        self.published.remove(&view);
    }

    pub(super) fn frame_bound(&self, active: &ActiveTerminalViews) -> Option<(u16, u16)> {
        if self.preview {
            return None;
        }
        let mut views = active
            .keys()
            .filter(|view| self.streaming(**view))
            .peekable();
        if views.peek().is_none() {
            return self.idle_bound;
        }
        let mut bound = None;
        for view in views {
            let (columns, rows) = *self.areas.get(view)?;
            bound = Some(
                bound.map_or((columns, rows), |(most_columns, most_rows): (u16, u16)| {
                    (most_columns.max(columns), most_rows.max(rows))
                }),
            );
        }
        bound
    }

    pub(super) fn frame_extent(
        &self,
        terminal: &Terminal<'_, '_>,
        active: &ActiveTerminalViews,
    ) -> Result<(u16, u16), WorkerError> {
        Ok(bounded_extent(
            (terminal.cols()?, terminal.rows()?),
            self.frame_bound(active),
        ))
    }

    fn resources(
        &mut self,
        terminal: &Terminal<'alloc, '_>,
    ) -> Result<&mut RenderResources<'alloc>, WorkerError> {
        if self.render.is_none() {
            self.render = Some(RenderResources::new(terminal)?);
        }
        Ok(self
            .render
            .as_mut()
            .expect("render resources were just created"))
    }

    pub(super) fn reset_render(&mut self) {
        self.render = None;
    }

    pub(super) fn mark_full_dirty(
        &mut self,
        terminal: &Terminal<'alloc, '_>,
    ) -> Result<(), WorkerError> {
        if let Some(render) = self.render.as_mut() {
            render.state.update(terminal)?.set_dirty(Dirty::Full)?;
        }
        Ok(())
    }

    fn snapshot(
        &mut self,
        terminal: &Terminal<'alloc, '_>,
        change: SnapshotChange,
        view: Option<&TerminalViewState>,
        status: SessionStatus,
        bound: Option<(u16, u16)>,
    ) -> Result<TerminalViewport, WorkerError> {
        #[cfg(test)]
        {
            self.snapshot_builds += 1;
        }
        if let Some(view) = view
            && let Some(copy_mode) = view.copy_mode.as_ref()
        {
            return Ok(copy_mode_snapshot(
                &mut self.generations,
                change,
                &mut self.dictionary,
                view,
                copy_mode,
                status,
                bound,
            ));
        }
        self.resources(terminal)?;
        let render = self.render.as_mut().expect("render resources exist");
        snapshot(
            terminal,
            &mut render.state,
            &mut render.rows,
            &mut render.cells,
            &mut self.generations,
            change,
            &mut self.dictionary,
            view,
            status,
            bound,
        )
    }

    fn needs_cells(&self, active: &ActiveTerminalViews) -> bool {
        self.preview || active.keys().any(|view| self.streaming(*view))
    }

    pub(super) fn release_unused(&mut self, active: &ActiveTerminalViews) {
        if self.needs_cells(active) {
            self.retain_render_until = None;
            self.watched = true;
            return;
        }
        if std::mem::take(&mut self.watched) && self.render.is_some() {
            self.retain_render_until = Some(Instant::now() + VIEWED_RENDER_RETAIN);
            return;
        }
        if self
            .retain_render_until
            .is_some_and(|until| Instant::now() < until)
        {
            return;
        }
        self.retain_render_until = None;
        self.render = None;
        self.dictionary.release_pools();
    }

    fn rebuild_due(&self) -> Option<Instant> {
        let first = self.unbuilt_since?;
        let last = self.last_unbuilt.unwrap_or(first);
        Some((last + UNWATCHED_SETTLE_QUIET).min(first + UNWATCHED_SETTLE_MAX))
    }

    fn notify_due(&self) -> Option<Instant> {
        self.notify_owed
            .then(|| {
                self.last_notify
                    .map(|last| last + UNWATCHED_NOTIFY_INTERVAL)
            })
            .flatten()
    }

    pub(super) fn settle_due(&self) -> Option<Instant> {
        [
            self.rebuild_due(),
            self.notify_due(),
            self.retain_render_until,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn admit_notify(&mut self, metadata_changed: bool) -> bool {
        let now = Instant::now();
        if metadata_changed
            || self
                .last_notify
                .is_none_or(|last| now >= last + UNWATCHED_NOTIFY_INTERVAL)
        {
            self.last_notify = Some(now);
            self.notify_owed = false;
            true
        } else {
            self.notify_owed = true;
            false
        }
    }
}

pub(super) fn publish_active_views<'alloc: 'callbacks, 'callbacks>(
    terminal: &mut Terminal<'alloc, 'callbacks>,
    publisher: &Publisher,
    frames: &mut Frames<'alloc>,
    change: SnapshotChange,
    active: &mut ActiveTerminalViews,
    word_separators: &WordSeparators,
    status: SessionStatus,
) -> Result<(), WorkerError> {
    publish_views(
        terminal,
        publisher,
        frames,
        change,
        active,
        word_separators,
        status,
        true,
    )
}

fn share_live_frame(left: &TerminalViewState, right: &TerminalViewState) -> bool {
    let live = |view: &TerminalViewState| {
        view.copy_mode.is_none()
            && view.selection.is_none()
            && matches!(view.viewport, ViewportAnchor::FollowBottom)
    };
    live(left)
        && live(right)
        && left.screen == right.screen
        && left.unseen_output == right.unseen_output
        && left.hover_link == right.hover_link
        && left.search.as_ref().map(|search| {
            (
                &search.matches,
                search.current,
                search.pending,
                search.invalid_pattern,
            )
        }) == right.search.as_ref().map(|search| {
            (
                &search.matches,
                search.current,
                search.pending,
                search.invalid_pattern,
            )
        })
}

#[allow(
    clippy::too_many_arguments,
    reason = "the publish entry point threads the actor's frame state and its notification choice"
)]
pub(super) fn publish_views<'alloc: 'callbacks, 'callbacks>(
    terminal: &mut Terminal<'alloc, 'callbacks>,
    publisher: &Publisher,
    frames: &mut Frames<'alloc>,
    change: SnapshotChange,
    active: &mut ActiveTerminalViews,
    word_separators: &WordSeparators,
    status: SessionStatus,
    notify: bool,
) -> Result<(), WorkerError> {
    if frames.defer_synchronized_output(terminal, &status)? {
        return Ok(());
    }
    let force_fallback = std::mem::take(&mut frames.force_fallback);
    let bound = frames.frame_bound(active);
    let size = (terminal.cols()?, terminal.rows()?);
    let mut view_ids = active.keys().copied().collect::<Vec<_>>();
    view_ids.sort_by_key(|view| view.0);
    let mut viewports: Vec<(TerminalViewId, Arc<TerminalViewport>, Option<u64>)> =
        Vec::with_capacity(view_ids.len());
    let mut copy_facts = HashMap::new();
    let mut streamed_any = false;
    for view_id in view_ids {
        let streaming = frames.streaming(view_id);
        let view = active
            .get_mut(&view_id)
            .expect("active view id was collected from the same map");
        let in_mode = view.copy_mode.is_some();
        if let Some(facts) = view
            .copy_mode
            .as_deref()
            .map(|mode| copy_mode_facts(mode, view.search.as_deref(), word_separators))
        {
            copy_facts.insert(view_id, Arc::new(facts));
        }
        let left_mode = !in_mode && frames.mode_views.remove(&view_id);
        if !streaming && !in_mode && !left_mode {
            continue;
        }
        if in_mode {
            frames.mode_views.insert(view_id);
            terminal.set_selection(None)?;
        } else {
            restore_view_state(terminal, view, word_separators)?;
        }
        let view = active
            .get(&view_id)
            .expect("active view id was collected from the same map");
        let shared = viewports.iter().find(|(previous, _, _)| {
            share_live_frame(
                view,
                active.get(previous).expect("published view remains active"),
            )
        });
        let viewport = if let Some((_, viewport, _)) = shared {
            Arc::clone(viewport)
        } else {
            Arc::new(frames.snapshot(terminal, change, Some(view), status.clone(), bound)?)
        };
        streamed_any |= streaming;
        viewports.push((view_id, viewport, streaming.then(|| frames.epoch(view_id))));
    }
    if streamed_any {
        frames.idle_bound = bound;
    }
    let mut sunk = Vec::new();
    let frame_sink = publisher.frame_sink().filter(|_| streamed_any);
    let urgent = frame_sink.is_some_and(|sink| sink.deliver(&viewports, &mut sunk));
    let refreshed = if streamed_any || force_fallback || frames.preview {
        None
    } else {
        publisher.refresh_fallback(terminal, &mut frames.dictionary, &status, bound)?
    };
    let mut notify = notify;
    let fallback = if streamed_any {
        frames.unbuilt_since = None;
        frames.last_unbuilt = None;
        let all_sunk = !frames.preview && viewports.iter().all(|(view, _, _)| sunk.contains(view));
        if !all_sunk {
            frames.sink_edge = None;
        } else if notify {
            let edge = viewports
                .iter()
                .find(|(_, _, epoch)| epoch.is_some())
                .map(|(_, viewport, _)| SinkEdge::of(viewport));
            let changed = frames.sink_edge != edge;
            frames.sink_edge = edge;
            notify = frames.admit_notify(changed || urgent);
        }
        FallbackFrame::FirstStreamed
    } else if let Some((viewport, metadata_changed)) = refreshed {
        if matches!(change, SnapshotChange::Content) {
            let now = Instant::now();
            frames.unbuilt_since.get_or_insert(now);
            frames.last_unbuilt = Some(now);
        }
        notify = notify && frames.admit_notify(metadata_changed);
        FallbackFrame::Metadata(viewport)
    } else {
        if !active.is_empty() {
            terminal.set_selection(None)?;
            terminal.scroll_viewport(ScrollViewport::Bottom);
        }
        frames.unbuilt_since = None;
        frames.last_unbuilt = None;
        FallbackFrame::Built(frames.snapshot(terminal, change, None, status, bound)?)
    };
    frames.published.clear();
    frames
        .published
        .extend(viewports.iter().map(|(view, _, _)| *view));
    let notified = publisher.publish_frame(fallback, viewports, copy_facts, sunk, size, notify);
    if let Some(sink) = frame_sink {
        sink.published(notified);
    }
    frames.release_unused(active);
    Ok(())
}

pub(super) fn settle_unwatched<'alloc: 'callbacks, 'callbacks>(
    terminal: &mut Terminal<'alloc, 'callbacks>,
    publisher: &Publisher,
    frames: &mut Frames<'alloc>,
    active: &mut ActiveTerminalViews,
    word_separators: &WordSeparators,
    status: SessionStatus,
) -> Result<(), WorkerError> {
    let now = Instant::now();
    if frames.notify_due().is_some_and(|due| now >= due) {
        frames.notify_owed = false;
        frames.last_notify = Some(now);
        publisher.notify_latest();
    }
    if frames.rebuild_due().is_none_or(|due| now < due) {
        if frames.retain_render_until.is_some_and(|until| now >= until) {
            frames.release_unused(active);
        }
        return Ok(());
    }
    let horizon = UNWATCHED_SETTLE_MAX + UNWATCHED_SETTLE_QUIET;
    let recurring = frames
        .last_unbuilt
        .is_some_and(|last| now < last + UNWATCHED_SETTLE_QUIET)
        || frames.last_settle.is_some_and(|last| now < last + horizon);
    frames.last_settle = Some(now);
    frames.retain_render_until = frames
        .retain_render_until
        .max(recurring.then(|| now + horizon));
    frames.force_fallback = true;
    publish_views(
        terminal,
        publisher,
        frames,
        SnapshotChange::Content,
        active,
        word_separators,
        status,
        false,
    )?;
    frames.force_fallback = false;
    Ok(())
}
