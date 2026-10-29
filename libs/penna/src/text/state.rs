#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use async_channel::{Receiver, Sender, unbounded};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
};

use ala::{
    App, AppContext as _, Bounds, Context, FocusHandle, IntoElement, ListState, ParentElement as _,
    Pixels, Point, Render, SharedString, Styled as _, Task, Window, canvas,
    prelude::FluentBuilder as _, px,
};

use crate::v_flex;

use super::{
    SelectAll,
    auto_scroll::AutoScroll,
    document::ParsedDocument,
    links::SharedLinks,
    markdown_ext::MarkdownExtensions,
    markdown_parse,
    node::{self, NodeContext},
    style::TextViewStyle,
    text_view::{CodeBlockActionsFn, CodeBlockBodyFn},
    veil::{RowVeil, StreamingVeil},
};

const MAX_COALESCED_UPDATES_PER_PARSE: usize = 64;

#[cfg(test)]
pub(super) mod parse_counters {
    use std::sync::atomic::AtomicUsize;

    pub(crate) static BACKGROUND: AtomicUsize = AtomicUsize::new(0);
    pub(crate) static COMMITTED: AtomicUsize = AtomicUsize::new(0);

    pub(super) fn bump(counter: &AtomicUsize) {
        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct TextViewState {
    pub(super) focus_handle: FocusHandle,
    pub(super) entity_id: ala::EntityId,
    pub(super) list_state: ListState,

    bounds: Bounds<Pixels>,

    pub(super) selectable: bool,
    pub(super) scrollable: bool,
    pub(super) text_view_style: TextViewStyle,
    pub(super) code_block_actions: Option<std::sync::Arc<CodeBlockActionsFn>>,
    pub(super) code_block_body: Option<std::sync::Arc<CodeBlockBodyFn>>,
    pub(super) links: Option<SharedLinks>,
    pub(super) markdown_extensions: Arc<MarkdownExtensions>,
    pub(super) streaming: bool,

    pub(super) is_selecting: bool,
    preserve_inline_selection: bool,
    multi_click_selection: Option<TextViewMultiClickSelection>,
    selected_text_override: Option<String>,
    select_all: bool,
    pub(super) auto_scroll: AutoScroll,

    pub(super) parsed_content: ParsedContent,
    text: String,
    revision: usize,
    committed_revision: usize,
    full_update_revision: usize,
    deferred_full_parse: bool,
    parsed_error: Option<SharedString>,
    tx: Sender<UpdateOptions>,
    streaming_veil: Arc<Mutex<RowVeil>>,
    seed_streaming_veil: bool,
    streaming_veil_baseline: usize,
    _parse_task: Task<()>,
    _receive_task: Task<()>,
}

impl TextViewState {
    pub fn markdown(text: &str, cx: &mut Context<Self>) -> Self {
        Self::new(text, true, cx)
    }

    pub fn markdown_deferred(text: &str, cx: &mut Context<Self>) -> Self {
        Self::new(text, false, cx)
    }

    fn new(text: &str, parse_initial: bool, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let entity_id = cx.entity_id();

        let (tx, rx) = unbounded::<UpdateOptions>();
        let (tx_result, rx_result) = unbounded::<ParsedUpdate>();
        let _receive_task = cx.spawn({
            async move |weak_self, cx| {
                while let Ok(mut parsed_update) = rx_result.recv().await {
                    while let Ok(newer) = rx_result.try_recv() {
                        parsed_update = parsed_update.superseded_by(newer);
                    }
                    _ = weak_self.update(cx, |state, cx| {
                        state.commit_parsed_update(parsed_update, cx);
                    });
                }
            }
        });

        let _parse_task = cx.background_spawn(parse_loop(rx, tx_result));

        let mut this = Self {
            focus_handle,
            entity_id,
            bounds: Bounds::default(),
            multi_click_selection: None,
            selected_text_override: None,
            select_all: false,
            selectable: false,
            scrollable: false,
            list_state: ListState::new(0, ala::ListAlignment::Top, px(1000.)).measure_all(),
            text_view_style: TextViewStyle::default(),
            code_block_actions: None,
            code_block_body: None,
            links: None,
            markdown_extensions: Arc::default(),
            streaming: false,
            is_selecting: false,
            preserve_inline_selection: false,
            auto_scroll: AutoScroll::default(),
            parsed_content: Default::default(),
            parsed_error: None,
            text: text.to_string(),
            revision: 0,
            committed_revision: 0,
            full_update_revision: 0,
            deferred_full_parse: !parse_initial,
            tx,
            streaming_veil: Arc::new(Mutex::new(RowVeil::default())),
            seed_streaming_veil: false,
            streaming_veil_baseline: 0,
            _parse_task,
            _receive_task,
        };
        this.increment_update(&text, false, parse_initial, cx);
        this
    }

    pub fn source(&self) -> SharedString {
        self.parsed_content.document.source.clone()
    }

    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.text.as_str() == text {
            return;
        }

        self.replace_markdown(text, false, cx);
    }

    pub fn replace_markdown(&mut self, text: &str, defer_full_parse: bool, cx: &mut Context<Self>) {
        self.text.clear();
        self.text.push_str(text);
        self.deferred_full_parse = defer_full_parse;
        self.parsed_error = None;
        self.increment_update(text, false, !defer_full_parse, cx);
    }

    pub fn synchronize_markdown(
        &mut self,
        text: &str,
        defer_full_parse: bool,
        cx: &mut Context<Self>,
    ) {
        if self.text.as_str() == text {
            return;
        }
        let len = self.text.len();
        if len < text.len()
            && text.is_char_boundary(len)
            && self.text.as_bytes() == &text.as_bytes()[..len]
        {
            self.text.push_str(&text[len..]);
            self.increment_update(&text[len..], true, false, cx);
            return;
        }
        self.replace_markdown(text, defer_full_parse, cx);
    }

    pub fn push_str(&mut self, new_text: &str, cx: &mut Context<Self>) {
        if new_text.is_empty() {
            return;
        }
        self.text.push_str(new_text);
        self.increment_update(new_text, true, false, cx);
    }

    pub(crate) fn set_streaming(&mut self, streaming: bool, cx: &mut Context<Self>) {
        if self.streaming == streaming {
            return;
        }
        self.streaming = streaming;
        self.seed_streaming_veil = streaming;
        self.streaming_veil_baseline = if streaming { self.text.len() } else { 0 };
        if let Ok(mut veil) = self.streaming_veil.lock() {
            *veil = RowVeil::default();
        }
        cx.notify();
    }

    pub(crate) fn set_markdown_extensions(
        &mut self,
        markdown_extensions: Arc<MarkdownExtensions>,
        cx: &mut Context<Self>,
    ) {
        if self.markdown_extensions.revision() == markdown_extensions.revision() {
            return;
        }

        self.markdown_extensions = markdown_extensions;
        let text = self.text.clone();
        self.increment_update(&text, false, !self.deferred_full_parse, cx);
    }

    pub fn selected_text(&self) -> String {
        if self.select_all {
            return self.parsed_content.document.text();
        }

        if let Some(text) = &self.selected_text_override {
            return text.clone();
        }

        self.parsed_content.document.selected_text()
    }

    fn increment_update(
        &mut self,
        text: &str,
        append: bool,
        parse_synchronously: bool,
        cx: &mut Context<Self>,
    ) {
        self.revision += 1;
        if !append {
            self.full_update_revision = self.revision;
        }
        let update_options = UpdateOptions {
            revision: self.revision,
            append,
            pending_text: text.to_string(),
            markdown_extensions: self.markdown_extensions.clone(),
        };

        if parse_synchronously {
            #[cfg(test)]
            parse_counters::bump(&parse_counters::COMMITTED);
            self.committed_revision = self.revision;
            match parse_content(ParsedContent::default(), &update_options) {
                Ok(content) => {
                    self.parsed_content = content;
                    self.parsed_error = None;
                    if !self.is_selecting {
                        self.reset_selection();
                    }
                }
                Err(err) => {
                    self.parsed_error = Some(err);
                }
            }
            cx.notify();
        }

        _ = self.tx.try_send(update_options);
    }

    fn commits(&self, parsed_update: &ParsedUpdate) -> bool {
        parsed_update.revision >= self.full_update_revision
            && parsed_update.revision > self.committed_revision
    }

    fn commit_parsed_update(&mut self, parsed_update: ParsedUpdate, cx: &mut Context<Self>) {
        if !self.commits(&parsed_update) {
            return;
        }

        #[cfg(test)]
        parse_counters::bump(&parse_counters::COMMITTED);
        self.committed_revision = parsed_update.revision;
        match parsed_update.result {
            Ok(content) => {
                if parsed_update.append && self.preserve_inline_selection {
                    for (block, previous) in content
                        .document
                        .blocks
                        .iter()
                        .zip(self.parsed_content.document.blocks.iter())
                    {
                        block.preserve_selection_from(previous);
                    }
                }
                self.parsed_content = content;
                self.parsed_error = None;
            }
            Err(err) => {
                self.parsed_error = Some(err);
            }
        }
        self.deferred_full_parse = false;
        if !parsed_update.append && !self.is_selecting {
            self.reset_selection();
        }
        cx.notify();
    }

    pub(super) fn update_bounds(&mut self, bounds: Bounds<Pixels>, has_selection: bool) {
        if self.bounds.size.width != bounds.size.width && has_selection && !self.is_selecting {
            self.preserve_inline_selection = true;
        }
        self.bounds = bounds;
    }

    pub(crate) fn preserves_inline_selection(&self) -> bool {
        self.preserve_inline_selection
    }

    pub(super) fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    pub(super) fn has_view_selection(&self) -> bool {
        self.select_all
            || self.preserve_inline_selection
            || self.multi_click_selection.is_some()
            || self.selected_text_override.is_some()
    }

    pub(super) fn stop_auto_scroll(&mut self) {
        self.auto_scroll.stop();
    }

    fn reset_selection(&mut self) {
        self.multi_click_selection = None;
        self.selected_text_override = None;
        self.select_all = false;
        self.is_selecting = false;
        self.preserve_inline_selection = false;
        self.auto_scroll.stop();
        self.parsed_content.document.clear_selection();
    }

    pub fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.reset_selection();
        cx.notify();
    }

    pub(super) fn scroll_offset(&self) -> Point<Pixels> {
        if self.scrollable {
            self.list_state.scroll_px_offset_for_scrollbar()
        } else {
            Point::default()
        }
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        self.multi_click_selection = None;
        self.selected_text_override = None;
        self.select_all = true;
        self.is_selecting = false;
        self.auto_scroll.stop();
        cx.notify();
    }

    pub(crate) fn set_multi_click_selection(
        &mut self,
        pos: Point<Pixels>,
        kind: TextViewMultiClickKind,
        selected_text: String,
    ) {
        let scroll_offset = self.scroll_offset();
        let pos = pos - self.bounds.origin - scroll_offset;
        self.multi_click_selection = Some(TextViewMultiClickSelection { pos, kind });
        self.selected_text_override = Some(selected_text);
        self.select_all = false;
        self.preserve_inline_selection = false;
        self.is_selecting = false;
        self.auto_scroll.stop();
    }

    pub(super) fn set_auto_scroll(&mut self, delta: Option<Pixels>, cx: &mut Context<Self>) {
        self.auto_scroll.set(delta, cx, |delta, state, cx| {
            state.list_state.scroll_by(delta);
            cx.notify();
        });
    }

    pub(crate) fn selection_points(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(Point<Pixels>, Point<Pixels>)> {
        if !self.selectable {
            return None;
        }
        let root = window.root::<crate::Root>().flatten()?;
        let selection = &root.read(cx).text_selection;
        if let Some(view_id) = selection.single_view() {
            if view_id != self.entity_id {
                return None;
            }
        }
        selection.resolved_points(cx)
    }

    pub(crate) fn has_selection(&self, window: &Window, cx: &App) -> bool {
        self.has_view_selection() || self.selection_points(window, cx).is_some()
    }

    pub(super) fn on_action_select_all(
        &mut self,
        _: &SelectAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selectable {
            cx.propagate();
            return;
        }

        self.select_all(cx);
    }

    pub(crate) fn is_selectable(&self) -> bool {
        self.selectable
    }

    pub(crate) fn is_all_selected(&self) -> bool {
        self.select_all
    }

    pub(crate) fn multi_click_selection(&self) -> Option<TextViewMultiClickSelection> {
        let scroll_offset = self.scroll_offset();
        self.multi_click_selection.map(|selection| {
            let pos = selection.pos + scroll_offset + self.bounds.origin;
            TextViewMultiClickSelection { pos, ..selection }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextViewMultiClickSelection {
    pub(crate) pos: Point<Pixels>,
    pub(crate) kind: TextViewMultiClickKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextViewMultiClickKind {
    Word,
    Paragraph,
}

impl Render for TextViewState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = cx.entity();
        let document = self.parsed_content.document.clone();
        let mut node_cx = self.parsed_content.node_cx.clone();

        node_cx.code_block_actions = self.code_block_actions.clone();
        node_cx.code_block_body = self.code_block_body.clone();
        node_cx.links = self.links.clone();
        node_cx.markdown_extensions = self.markdown_extensions.clone();
        node_cx.style = self.text_view_style.clone();
        if self.streaming {
            let seed = self.seed_streaming_veil || cx.reduce_motion();
            node_cx.streaming_veil = Some(StreamingVeil::new(
                self.streaming_veil.clone(),
                self.entity_id,
                seed,
                self.streaming_veil_baseline,
            ));
            if !document.blocks.is_empty() {
                self.seed_streaming_veil = false;
            }
        }

        v_flex()
            .size_full()
            .map(|this| match &mut self.parsed_error {
                None => this.child(document.render_root(
                    if self.scrollable {
                        Some(self.list_state.clone())
                    } else {
                        None
                    },
                    &node_cx,
                    window,
                    cx,
                )),
                Some(err) => this.child(
                    v_flex()
                        .gap_1()
                        .child("Failed to parse content")
                        .child(err.to_string()),
                ),
            })
            .child(
                canvas(
                    move |bounds, window, cx| {
                        let resized = state.read(cx).bounds().size.width != bounds.size.width;
                        let has_selection = resized && state.read(cx).has_selection(window, cx);
                        state.update(cx, |state, _| {
                            state.update_bounds(bounds, has_selection);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}

#[derive(Clone, PartialEq, Default)]
pub(crate) struct ParsedContent {
    pub(crate) document: ParsedDocument,
    pub(crate) node_cx: node::NodeContext,
}

async fn parse_loop(rx: Receiver<UpdateOptions>, tx_result: Sender<ParsedUpdate>) {
    let mut content = ParsedContent::default();

    while let Ok(mut options) = rx.recv().await {
        let hit_coalesce_budget = merge_pending_options(&mut options, &rx);

        #[cfg(test)]
        parse_counters::bump(&parse_counters::BACKGROUND);
        let res = parse_content(content.clone(), &options);
        if let Ok(parsed) = &res {
            content = parsed.clone();
        }
        _ = tx_result.try_send(ParsedUpdate {
            revision: options.revision,
            append: options.append,
            result: res,
        });

        if hit_coalesce_budget {
            YieldNow(false).await;
        }
    }
}

struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[derive(Clone)]
struct UpdateOptions {
    revision: usize,
    pending_text: String,
    append: bool,
    markdown_extensions: Arc<MarkdownExtensions>,
}

impl UpdateOptions {
    fn merge(&mut self, next: UpdateOptions) {
        if next.append {
            self.pending_text.push_str(&next.pending_text);
            self.revision = next.revision;
        } else {
            *self = next;
        }
    }
}

struct ParsedUpdate {
    revision: usize,
    append: bool,
    result: Result<ParsedContent, SharedString>,
}

impl ParsedUpdate {
    fn superseded_by(self, newer: ParsedUpdate) -> ParsedUpdate {
        ParsedUpdate {
            append: self.append && newer.append,
            ..newer
        }
    }
}

fn merge_pending_options(options: &mut UpdateOptions, rx: &Receiver<UpdateOptions>) -> bool {
    let mut update_count = 1;

    while update_count < MAX_COALESCED_UPDATES_PER_PARSE {
        match rx.try_recv() {
            Ok(next_options) => {
                options.merge(next_options);
                update_count += 1;
            }
            Err(_) => return false,
        }
    }

    true
}

fn parse_content(
    mut content: ParsedContent,
    options: &UpdateOptions,
) -> Result<ParsedContent, SharedString> {
    let mut node_cx = NodeContext {
        markdown_extensions: options.markdown_extensions.clone(),
        ..NodeContext::default()
    };

    let mut source = String::new();
    if options.append
        && let Some(last_block) = Arc::make_mut(&mut content.document.blocks).pop()
        && let Some(span) = last_block.span()
    {
        node_cx.offset = span.start;
        let last_source = &content.document.source[span.start..];
        source.push_str(last_source);
        source.push_str(&options.pending_text);
    } else {
        source = options.pending_text.to_string();
    }

    let new_document = markdown_parse::parse(&source, &mut node_cx)?;

    if options.append {
        content.document.source =
            format!("{}{}", content.document.source, options.pending_text).into();
        Arc::make_mut(&mut content.document.blocks)
            .extend(Arc::unwrap_or_clone(new_document.blocks));
    } else {
        content.document = new_document;
    }

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::MarkdownNode;
    use ala::TestAppContext;

    #[test]
    fn appending_markdown_preserves_previous_document_snapshots() {
        let options = UpdateOptions {
            revision: 1,
            pending_text: "# Heading\n\nFirst paragraph.\n\nLast".to_owned(),
            append: false,
            markdown_extensions: Arc::default(),
        };
        let before = parse_content(ParsedContent::default(), &options).unwrap();
        let snapshot = before.clone();
        let after = parse_content(
            before,
            &UpdateOptions {
                revision: 2,
                pending_text: " paragraph.\n\n- Next".to_owned(),
                append: true,
                markdown_extensions: Arc::default(),
            },
        )
        .unwrap();
        assert_eq!(snapshot.document.source.as_str(), options.pending_text);
        assert_eq!(snapshot.document.blocks.len(), 3);
        assert_eq!(snapshot.document.blocks[2].text(), "Last\n");
        assert_eq!(after.document.blocks.len(), 4);
        assert_eq!(after.document.blocks[2].text(), "Last paragraph.\n");
        assert_eq!(after.document.blocks[3].text(), "Next\n");
    }

    #[test]
    #[ignore = "manual document snapshot benchmark"]
    fn benchmark_markdown_document_snapshots() {
        use std::{hint::black_box, time::Instant};

        for paragraphs in [8, 64, 512] {
            let source = (0..paragraphs)
                .map(|i| format!("Paragraph {i}: **bold** and [link](https://example.test).\n\n"))
                .collect::<String>();
            let document = markdown_parse::parse(&source, &mut NodeContext::default()).unwrap();
            for round in 0..6 {
                for shared in if round % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                } {
                    let start = Instant::now();
                    for _ in 0..2_000 {
                        if shared {
                            black_box(black_box(&document.blocks).clone());
                        } else {
                            black_box(black_box(document.blocks.as_ref()).clone());
                        }
                    }
                    eprintln!(
                        "markdown_snapshot paragraphs={paragraphs} shared={shared} round={round} ns={}",
                        start.elapsed().as_nanos()
                    );
                }
            }
        }
    }

    #[ala::test]
    fn set_text_then_push_str_appends_to_replaced_content(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("old", cx)));
        cx.run_until_parked();

        state.update(cx, |state, cx| {
            state.set_text("", cx);
            state.push_str("new", cx);
            state.push_str(" text", cx);
        });
        cx.run_until_parked();

        state.read_with(cx, |state, _| {
            assert_eq!(state.text.as_str(), "new text");
            assert_eq!(state.source().as_str(), "new text");
        });

        state.update(cx, |state, cx| {
            state.set_text("", cx);
        });
        cx.run_until_parked();

        state.read_with(cx, |state, _| {
            assert_eq!(state.text.as_str(), "");
            assert_eq!(state.source().as_str(), "");
        });
    }

    #[ala::test]
    fn deferred_markdown_parses_after_construction(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state =
            cx.update(|cx| cx.new(|cx| TextViewState::markdown_deferred("# delayed\n\nbody", cx)));
        state.update(cx, |state, cx| {
            state.set_markdown_extensions(
                Arc::new(MarkdownExtensions::default().link_rewriter(|_| None)),
                cx,
            );
        });

        state.read_with(cx, |state, _| {
            assert_eq!(state.source().as_str(), "");
        });

        cx.run_until_parked();

        state.read_with(cx, |state, _| {
            assert_eq!(state.source().as_str(), "# delayed\n\nbody");
        });
    }

    #[ala::test]
    fn streamed_appends_keep_the_created_prefix(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("`", cx)));
        cx.run_until_parked();

        state.update(cx, |state, cx| {
            state.push_str("/Users/x/AGENTS.md", cx);
            state.push_str("`", cx);
        });
        cx.run_until_parked();

        state.read_with(cx, |state, _| {
            assert_eq!(state.text.as_str(), "`/Users/x/AGENTS.md`");
            assert_eq!(state.source().as_str(), "`/Users/x/AGENTS.md`");
        });
    }

    #[test]
    fn update_options_merge_keeps_latest_full_text() {
        let mut options = UpdateOptions {
            revision: 1,
            pending_text: "old".to_string(),
            append: true,
            markdown_extensions: Arc::default(),
        };

        options.merge(UpdateOptions {
            revision: 2,
            pending_text: "new".to_string(),
            append: false,
            markdown_extensions: Arc::default(),
        });
        options.merge(UpdateOptions {
            revision: 3,
            pending_text: " text".to_string(),
            append: true,
            markdown_extensions: Arc::default(),
        });

        assert_eq!(options.revision, 3);
        assert_eq!(options.pending_text, "new text");
        assert!(!options.append);
    }

    #[test]
    fn update_future_yields_before_coalescing_all_queued_updates() {
        let (tx, rx) = unbounded::<UpdateOptions>();
        let (tx_result, rx_result) = unbounded::<ParsedUpdate>();
        let total_updates = 128;

        for revision in 1..=total_updates {
            tx.try_send(UpdateOptions {
                revision,
                pending_text: format!("{revision}\n"),
                append: revision != 1,
                markdown_extensions: Arc::default(),
            })
            .unwrap();
        }

        let mut future = Box::pin(parse_loop(rx, tx_result));
        let waker = std::task::Waker::noop().clone();
        let mut task_cx = TaskContext::from_waker(&waker);

        assert!(matches!(
            std::future::Future::poll(future.as_mut(), &mut task_cx),
            Poll::Pending
        ));
        let parsed_update = rx_result.try_recv().expect("parse result");

        assert!(
            parsed_update.revision < total_updates,
            "single poll coalesced every queued update through revision {}",
            parsed_update.revision
        );

        assert!(matches!(
            std::future::Future::poll(future.as_mut(), &mut task_cx),
            Poll::Pending
        ));
        let parsed_update = rx_result.try_recv().expect("next parse result");
        assert_eq!(parsed_update.revision, total_updates);
    }

    fn push_and_parse(
        state: &ala::Entity<TextViewState>,
        chunk: &str,
        cx: &mut TestAppContext,
    ) -> ParsedUpdate {
        let (revision, baseline) = state.update(cx, |state, cx| {
            state.push_str(chunk, cx);
            (state.revision, state.parsed_content.clone())
        });
        let options = UpdateOptions {
            revision,
            pending_text: chunk.to_string(),
            append: true,
            markdown_extensions: Arc::default(),
        };
        ParsedUpdate {
            revision,
            append: true,
            result: parse_content(baseline, &options),
        }
    }

    #[ala::test]
    fn stream_commits_a_parse_that_a_newer_chunk_overtook(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("# Answer\n\n", cx)));
        cx.run_until_parked();

        let parsed = push_and_parse(&state, "Streaming", cx);
        state.update(cx, |state, cx| {
            state.push_str(" tokens", cx);
            state.commit_parsed_update(parsed, cx);
            assert_eq!(state.source().as_str(), "# Answer\n\nStreaming");
        });

        cx.run_until_parked();
        state.read_with(cx, |state, _| {
            assert_eq!(state.source().as_str(), "# Answer\n\nStreaming tokens");
        });
    }

    #[ala::test]
    fn a_parse_from_before_a_replacement_is_discarded(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("old", cx)));
        cx.run_until_parked();

        let parsed = push_and_parse(&state, " text", cx);
        state.update(cx, |state, cx| {
            state.replace_markdown("new", true, cx);
            state.commit_parsed_update(parsed, cx);
            assert_eq!(state.source().as_str(), "old");
        });

        cx.run_until_parked();
        state.read_with(cx, |state, _| {
            assert_eq!(state.source().as_str(), "new");
        });
    }

    #[ala::test]
    fn parses_that_arrive_together_commit_once(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("# Answer\n\n", cx)));
        cx.run_until_parked();
        let commits = std::rc::Rc::new(std::cell::Cell::new(0));
        let _subscription = cx.update(|cx| {
            let commits = commits.clone();
            cx.observe(&state, move |_, _| commits.set(commits.get() + 1))
        });

        let dispatcher = cx.executor().dispatcher().clone();
        let dispatcher = dispatcher.as_test().unwrap();
        for chunk in ["one", " two", " three"] {
            state.update(cx, |state, cx| state.push_str(chunk, cx));
            while dispatcher.tick(true) {}
        }
        cx.run_until_parked();

        assert_eq!(commits.get(), 1);
        state.read_with(cx, |state, _| {
            assert_eq!(state.source().as_str(), "# Answer\n\none two three");
        });
    }

    #[test]
    fn a_superseded_replacement_still_counts_as_one() {
        let update = |revision, append| ParsedUpdate {
            revision,
            append,
            result: Ok(ParsedContent::default()),
        };
        let merged = update(1, false).superseded_by(update(2, true));
        assert_eq!(merged.revision, 2);
        assert!(!merged.append);
        assert!(update(1, true).superseded_by(update(2, true)).append);
    }

    #[ala::test]
    fn a_synchronous_parse_is_not_committed_again(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("one", cx)));
        cx.run_until_parked();

        state.update(cx, |state, cx| {
            state.set_text("**two**", cx);
            state.select_all(cx);
        });
        cx.run_until_parked();

        state.read_with(cx, |state, _| {
            assert!(
                state.is_all_selected(),
                "the background parser's copy of a synchronous parse was committed again"
            );
            assert_eq!(state.selected_text().trim(), "two");
        });
    }

    #[ala::test]
    fn select_all_returns_rendered_text(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("**quick** value", cx)));
        cx.run_until_parked();

        state.update(cx, |state, cx| {
            state.select_all(cx);
        });

        state.read_with(cx, |state, _| {
            assert!(state.has_view_selection());
            assert_eq!(state.selected_text().trim(), "quick value");
        });

        state.update(cx, |state, cx| {
            state.clear_selection(cx);
        });

        state.read_with(cx, |state, _| {
            assert!(!state.has_view_selection());
            assert_eq!(state.selected_text(), "");
        });
    }

    #[ala::test]
    fn set_markdown_extensions_reparses_existing_text(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let state = cx.update(|cx| cx.new(|cx| TextViewState::markdown("$TSLA.US", cx)));
        cx.run_until_parked();

        let extensions = MarkdownExtensions::default().block_parser(|node, cx| {
            let markdown::mdast::Node::Paragraph(paragraph) = node else {
                return None;
            };
            let [markdown::mdast::Node::Text(text)] = paragraph.children.as_slice() else {
                return None;
            };
            let symbol = text.value.strip_prefix('$')?.to_string();
            let node_text = format!("${symbol}");

            Some(
                MarkdownNode::new("ticker", symbol)
                    .text(node_text)
                    .markdown(cx.node_source(node).unwrap_or_default()),
            )
        });

        state.update(cx, |state, cx| {
            state.set_markdown_extensions(Arc::new(extensions), cx);
        });
        cx.run_until_parked();

        state.read_with(cx, |state, _| {
            let node::BlockNode::Custom { node, .. } = &state.parsed_content.document.blocks[0]
            else {
                panic!("expected custom markdown node");
            };
            assert_eq!(node.name(), "ticker");
            assert_eq!(node.data::<String>().map(String::as_str), Some("TSLA.US"));
        });
    }
}
