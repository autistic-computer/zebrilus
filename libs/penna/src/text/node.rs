#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::{
    cell::RefCell,
    collections::HashMap,
    ops::Range,
    sync::{Arc, Mutex},
};

use ala::{
    AnyElement, App, Bounds, DefiniteLength, Div, Element, ElementId, FontStyle, FontWeight,
    GlobalElementId, HighlightStyle, Hsla, InspectorElementId, InteractiveElement as _,
    IntoElement, LayoutId, Length, ObjectFit, Overflow, ParentElement, Pixels, ScrollHandle,
    SharedString, SharedUri, StatefulInteractiveElement, Styled, StyledImage as _, Window, div,
    img, prelude::FluentBuilder as _, px, relative, rems,
};
use markdown::mdast;

use crate::{
    ActiveTheme as _, Edge, PaintState, RuleLook, Sizable as _, StyledExt, Surface, WindowExt as _,
    check::Check,
    h_flex,
    highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter},
    roles,
    tooltip::Tooltip,
    v_flex,
};

use super::{
    TextViewStyle,
    document::NodeRenderOptions,
    global::TextGlobal,
    inline::{Inline, InlineState, point_in_text_selection},
    inline_flow::{InlineFlow, InlineFlowItem},
    links::{LinkClick, SharedLinks, link_highlight, open_link},
    markdown_ext::{MarkdownExtensions, MarkdownNode},
    scroll_area::horizontal_scroll_area,
    text_view::{CodeBlockActionsFn, CodeBlockBodyFn},
    utils::list_item_prefix,
    veil::{StreamingVeil, VeilKey},
    window_selection,
};
use crate::Colorize as _;

const BLOCKQUOTE_BAR_WIDTH: Pixels = px(3.0);

thread_local! {
    static CODE_BLOCK_HIGHLIGHTERS: RefCell<HashMap<SharedString, SyntaxHighlighter>> =
        RefCell::new(HashMap::new());
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BlockNode {
    Root {
        children: Vec<BlockNode>,
        span: Option<Span>,
    },
    Paragraph(Paragraph),
    Heading {
        level: u8,
        children: Paragraph,
        span: Option<Span>,
    },
    Blockquote {
        children: Vec<BlockNode>,
        span: Option<Span>,
    },
    List {
        children: Vec<BlockNode>,
        ordered: bool,
        span: Option<Span>,
    },
    ListItem {
        children: Vec<BlockNode>,
        spread: bool,
        checked: Option<bool>,
        span: Option<Span>,
    },
    CodeBlock(CodeBlock),
    Custom {
        node: MarkdownNode,
        state: BlockState,
    },
    Table(Table),
    Break {
        html: bool,
        span: Option<Span>,
    },
    HorizontalRule {
        span: Option<Span>,
    },
    Definition {
        identifier: SharedString,
        url: SharedString,
        title: Option<SharedString>,
        span: Option<Span>,
    },
    Unknown,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BlockState {
    selected: Arc<Mutex<bool>>,
}

impl BlockState {
    fn is_selected(&self) -> bool {
        self.selected.lock().is_ok_and(|selected| *selected)
    }

    fn set_selected(&self, value: bool) {
        if let Ok(mut selected) = self.selected.lock() {
            *selected = value;
        }
    }
}

impl PartialEq for BlockState {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

struct CustomBlockElement {
    content: AnyElement,
    state: BlockState,
}

impl IntoElement for CustomBlockElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for CustomBlockElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.content.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.content.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = TextGlobal::current_view(cx).cloned();
        let selectable = view
            .as_ref()
            .is_some_and(|view| view.read(cx).is_selectable());
        let selected = selectable
            && view.as_ref().is_some_and(|view| {
                let state = view.read(cx);
                if state.is_all_selected() {
                    return true;
                }
                if state.preserves_inline_selection() {
                    return self.state.is_selected();
                }
                state
                    .selection_points(window, cx)
                    .is_some_and(|(start, end)| custom_block_is_selected(bounds, start, end))
            });
        self.state.set_selected(selected);
        if let Some(view) = view.as_ref().filter(|_| selectable) {
            let visible = bounds.intersect(&window.content_mask().bounds);
            if visible.size.width > Pixels::ZERO && visible.size.height > Pixels::ZERO {
                window_selection::register_selectable_text_inline(view, vec![visible], window, cx);
            }
        }
        self.content.paint(window, cx);
        if selected {
            window.paint_quad(ala::fill(bounds, cx.theme().foreground.wash()));
        }
    }
}

fn custom_block_is_selected(
    bounds: Bounds<Pixels>,
    start: ala::Point<Pixels>,
    end: ala::Point<Pixels>,
) -> bool {
    let (start_inside, end_inside) = (bounds.contains(&start), bounds.contains(&end));
    start != end
        && !(start_inside && end_inside)
        && (start_inside
            || end_inside
            || point_in_text_selection(
                bounds.origin,
                bounds.size.width,
                start,
                end,
                bounds.size.height,
            ))
}

#[derive(Clone, Copy)]
enum BlockTextKind {
    All,
    Selected,
}

impl BlockNode {
    pub(super) fn is_list_item(&self) -> bool {
        matches!(self, Self::ListItem { .. })
    }

    pub(super) fn compact(self) -> BlockNode {
        match self {
            Self::Root { mut children, .. } if children.len() == 1 => children.remove(0).compact(),
            _ => self,
        }
    }

    pub(crate) fn span(&self) -> Option<Span> {
        match self {
            BlockNode::Root { span, .. } => *span,
            BlockNode::Paragraph(paragraph) => paragraph.span,
            BlockNode::Heading { span, .. } => *span,
            BlockNode::Blockquote { span, .. } => *span,
            BlockNode::List { span, .. } => *span,
            BlockNode::ListItem { span, .. } => *span,
            BlockNode::CodeBlock(code_block) => code_block.span,
            BlockNode::Custom { node, .. } => node.span,
            BlockNode::Table(table) => table.span,
            BlockNode::Break { span, .. } => *span,
            BlockNode::HorizontalRule { span, .. } => *span,
            BlockNode::Definition { span, .. } => *span,
            BlockNode::Unknown { .. } => None,
        }
    }

    pub(super) fn text(&self) -> String {
        self.text_by_kind(BlockTextKind::All)
    }

    pub(super) fn selected_text(&self) -> String {
        self.text_by_kind(BlockTextKind::Selected)
    }

    fn text_by_kind(&self, kind: BlockTextKind) -> String {
        let mut text = String::new();
        match self {
            BlockNode::Root { children, .. } => {
                let block_text = Self::children_text(children, kind);
                if !block_text.is_empty() {
                    text.push_str(&block_text);
                    text.push('\n');
                }
            }
            BlockNode::Paragraph(paragraph) => {
                let block_text = match kind {
                    BlockTextKind::All => paragraph.text(),
                    BlockTextKind::Selected => paragraph.selected_text(),
                };
                if !block_text.is_empty() {
                    text.push_str(&block_text);
                    text.push('\n');
                }
            }
            BlockNode::Heading { children, .. } => {
                let block_text = match kind {
                    BlockTextKind::All => children.text(),
                    BlockTextKind::Selected => children.selected_text(),
                };
                if !block_text.is_empty() {
                    text.push_str(&block_text);
                    text.push('\n');
                }
            }
            BlockNode::List { children, .. } | BlockNode::ListItem { children, .. } => {
                text.push_str(&Self::children_text(children, kind));
            }
            BlockNode::Blockquote { children, .. } => {
                let block_text = Self::children_text(children, kind);

                if !block_text.is_empty() {
                    text.push_str(&block_text);
                    text.push('\n');
                }
            }
            BlockNode::Table(table) => {
                let mut block_text = String::new();
                for row in table.children.iter() {
                    let mut row_texts = vec![];
                    for cell in row.children.iter() {
                        row_texts.push(match kind {
                            BlockTextKind::All => cell.children.text(),
                            BlockTextKind::Selected => cell.children.selected_text(),
                        });
                    }
                    if !row_texts.is_empty() {
                        block_text.push_str(&row_texts.join(" "));
                        block_text.push('\n');
                    }
                }

                if !block_text.is_empty() {
                    text.push_str(&block_text);
                    text.push('\n');
                }
            }
            BlockNode::CodeBlock(code_block) => {
                let block_text = match kind {
                    BlockTextKind::All => code_block.text(),
                    BlockTextKind::Selected => code_block.selected_text(),
                };
                if !block_text.is_empty() {
                    text.push_str(&block_text);
                    text.push('\n');
                }
            }
            BlockNode::Custom { node, state } => {
                if matches!(kind, BlockTextKind::All) || state.is_selected() {
                    let content = node.as_text();
                    if !content.is_empty() {
                        text.push_str(content);
                        text.push('\n');
                    }
                }
            }
            BlockNode::Definition { .. }
            | BlockNode::Break { .. }
            | BlockNode::HorizontalRule { .. }
            | BlockNode::Unknown { .. } => {}
        }

        text
    }

    fn children_text(children: &[BlockNode], kind: BlockTextKind) -> String {
        let mut text = String::new();
        for child in children.iter() {
            text.push_str(&child.text_by_kind(kind));
        }

        text
    }

    pub(super) fn clear_selection(&self) {
        match self {
            BlockNode::Root { children, .. }
            | BlockNode::Blockquote { children, .. }
            | BlockNode::List { children, .. }
            | BlockNode::ListItem { children, .. } => {
                for child in children.iter() {
                    child.clear_selection();
                }
            }
            BlockNode::Paragraph(paragraph) => paragraph.clear_selection(),
            BlockNode::Heading { children, .. } => children.clear_selection(),
            BlockNode::Table(table) => {
                for row in table.children.iter() {
                    for cell in row.children.iter() {
                        cell.children.clear_selection();
                    }
                }
            }
            BlockNode::CodeBlock(code_block) => code_block.clear_selection(),
            BlockNode::Custom { state, .. } => state.set_selected(false),
            BlockNode::Definition { .. }
            | BlockNode::Break { .. }
            | BlockNode::HorizontalRule { .. }
            | BlockNode::Unknown { .. } => {}
        }
    }

    pub(super) fn preserve_selection_from(&self, previous: &Self) {
        if self.span().map(|span| span.start) != previous.span().map(|span| span.start) {
            return;
        }
        match (self, previous) {
            (
                Self::Root { children, .. }
                | Self::Blockquote { children, .. }
                | Self::List { children, .. }
                | Self::ListItem { children, .. },
                Self::Root { children: old, .. }
                | Self::Blockquote { children: old, .. }
                | Self::List { children: old, .. }
                | Self::ListItem { children: old, .. },
            ) => {
                for (child, old) in children.iter().zip(old) {
                    child.preserve_selection_from(old);
                }
            }
            (
                Self::Paragraph(paragraph)
                | Self::Heading {
                    children: paragraph,
                    ..
                },
                Self::Paragraph(old) | Self::Heading { children: old, .. },
            ) => paragraph.preserve_selection_from(old),
            (Self::Table(table), Self::Table(old)) => {
                for (row, old) in table.children.iter().zip(&old.children) {
                    for (cell, old) in row.children.iter().zip(&old.children) {
                        cell.children.preserve_selection_from(&old.children);
                    }
                }
            }
            (Self::CodeBlock(block), Self::CodeBlock(old)) => {
                preserve_inline_selection(&block.state, &old.state, &block.code());
            }
            (
                Self::Custom { node, state },
                Self::Custom {
                    node: old,
                    state: old_state,
                },
            ) if node.name() == old.name() && node.as_text().starts_with(old.as_text()) => {
                state.set_selected(old_state.is_selected());
            }
            _ => {}
        }
    }
}

fn preserve_inline_selection(
    state: &Arc<Mutex<InlineState>>,
    previous: &Arc<Mutex<InlineState>>,
    text: &str,
) {
    if Arc::ptr_eq(state, previous) {
        return;
    }
    let selection = previous.lock().ok().and_then(|previous| {
        previous
            .selection
            .filter(|selection| text.get(..selection.end) == previous.text.get(..selection.end))
    });
    if let Ok(mut state) = state.lock() {
        state.selection = selection;
        state.set_text(text.to_owned().into());
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct LinkMark {
    pub url: SharedString,
    pub identifier: Option<SharedString>,
    pub title: Option<SharedString>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct TextMark {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub code: bool,
    pub highlight: Option<Hsla>,
    pub link: Option<LinkMark>,
}

impl TextMark {
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub fn code(mut self) -> Self {
        self.code = true;
        self
    }

    pub fn highlight(mut self, color: Hsla) -> Self {
        self.highlight = Some(color);
        self
    }

    pub fn link(mut self, link: impl Into<LinkMark>) -> Self {
        self.link = Some(link.into());
        self
    }

    pub fn merge(&mut self, other: TextMark) {
        self.bold |= other.bold;
        self.italic |= other.italic;
        self.strikethrough |= other.strikethrough;
        self.underline |= other.underline;
        self.code |= other.code;
        if other.highlight.is_some() {
            self.highlight = other.highlight;
        }
        if let Some(link) = other.link {
            self.link = Some(link);
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl From<Span> for ElementId {
    fn from(value: Span) -> Self {
        ElementId::Name(format!("md-{}:{}", value.start, value.end).into())
    }
}

#[derive(Debug, Default, Clone)]
pub struct ImageNode {
    pub url: SharedUri,
    pub link: Option<LinkMark>,
    pub title: Option<SharedString>,
    pub alt: Option<SharedString>,
    pub width: Option<DefiniteLength>,
    pub height: Option<DefiniteLength>,
}

impl ImageNode {
    pub fn title(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| self.alt.clone().unwrap_or_default())
            .to_string()
    }
}

impl PartialEq for ImageNode {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
            && self.link == other.link
            && self.title == other.title
            && self.alt == other.alt
            && self.width == other.width
            && self.height == other.height
    }
}

#[derive(Default, Clone, Debug)]
pub(crate) struct InlineNode {
    pub(crate) text: SharedString,
    pub(crate) image: Option<ImageNode>,
    pub(crate) marks: Vec<(Range<usize>, TextMark)>,

    state: Arc<Mutex<InlineState>>,
}

impl PartialEq for InlineNode {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.image == other.image && self.marks == other.marks
    }
}

impl InlineNode {
    pub(crate) fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            image: None,
            marks: vec![],
            state: Arc::new(Mutex::new(InlineState::default())),
        }
    }

    pub(crate) fn image(image: ImageNode) -> Self {
        let mut this = Self::new("");
        this.image = Some(image);
        this
    }

    pub(crate) fn marks(mut self, marks: Vec<(Range<usize>, TextMark)>) -> Self {
        self.marks = marks;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Paragraph {
    pub(super) span: Option<Span>,
    pub(super) children: Vec<InlineNode>,
    pub(super) link_refs: HashMap<SharedString, SharedString>,

    pub(crate) state: Arc<Mutex<InlineState>>,
}

impl PartialEq for Paragraph {
    fn eq(&self, other: &Self) -> bool {
        self.span == other.span
            && self.children == other.children
            && self.link_refs == other.link_refs
    }
}

impl Paragraph {
    fn preserve_selection_from(&self, previous: &Self) {
        let mut text = String::new();
        for (index, child) in self.children.iter().enumerate() {
            text.push_str(&child.text);
            if child.image.is_some() {
                if let Some(old) = previous.children.get(index) {
                    preserve_inline_selection(&child.state, &old.state, &text);
                }
                text.clear();
            }
        }
        preserve_inline_selection(&self.state, &previous.state, &text);
    }

    pub(crate) fn new(text: String) -> Self {
        Self {
            span: None,
            children: vec![InlineNode::new(&text)],
            link_refs: HashMap::new(),
            state: Arc::new(Mutex::new(InlineState::default())),
        }
    }

    pub(super) fn selected_text(&self) -> String {
        let mut text = String::new();

        for c in self.children.iter() {
            let Ok(state) = c.state.lock() else {
                continue;
            };
            if let Some(selection) = &state.selection {
                text.push_str(&state.text[selection.start..selection.end]);
            }
        }

        if let Ok(state) = self.state.lock()
            && let Some(selection) = &state.selection
        {
            text.push_str(&state.text[selection.start..selection.end]);
        }

        text
    }

    pub(super) fn text(&self) -> String {
        let mut text = String::new();
        for node in self.children.iter() {
            text.push_str(&node.text);
        }
        text
    }

    pub(super) fn clear_selection(&self) {
        for c in self.children.iter() {
            if let Ok(mut state) = c.state.lock() {
                state.selection = None;
            }
        }

        if let Ok(mut state) = self.state.lock() {
            state.selection = None;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Table {
    pub(crate) children: Vec<TableRow>,
    pub(crate) column_aligns: Vec<ColumnumnAlign>,
    pub(crate) span: Option<Span>,
}

impl Table {
    pub(crate) fn column_align(&self, index: usize) -> ColumnumnAlign {
        self.column_aligns.get(index).copied().unwrap_or_default()
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub(crate) enum ColumnumnAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl From<mdast::AlignKind> for ColumnumnAlign {
    fn from(value: mdast::AlignKind) -> Self {
        match value {
            mdast::AlignKind::None => ColumnumnAlign::Left,
            mdast::AlignKind::Left => ColumnumnAlign::Left,
            mdast::AlignKind::Center => ColumnumnAlign::Center,
            mdast::AlignKind::Right => ColumnumnAlign::Right,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TableRow {
    pub children: Vec<TableCell>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TableCell {
    pub children: Paragraph,
    pub width: Option<DefiniteLength>,
}

impl Paragraph {
    pub(crate) fn take(&mut self) -> Paragraph {
        std::mem::replace(
            self,
            Paragraph {
                span: None,
                children: vec![],
                link_refs: Default::default(),
                state: Arc::new(Mutex::new(InlineState::default())),
            },
        )
    }

    pub(crate) fn is_image(&self) -> bool {
        false
    }

    pub(crate) fn set_span(&mut self, span: Span) {
        self.span = Some(span);
    }

    pub(crate) fn push_str(&mut self, text: &str) {
        self.children.push(
            InlineNode::new(text.to_string()).marks(vec![(0..text.len(), TextMark::default())]),
        );
    }

    pub(crate) fn push(&mut self, text: InlineNode) {
        self.children.push(text);
    }

    pub(crate) fn push_image(&mut self, image: ImageNode) {
        self.children.push(InlineNode::image(image));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.children.is_empty()
            || self
                .children
                .iter()
                .all(|node| node.text.is_empty() && node.image.is_none())
    }

    pub(crate) fn text_len(&self) -> usize {
        self.children
            .iter()
            .map(|node| node.text.len())
            .sum::<usize>()
    }

    pub(crate) fn merge(&mut self, other: Self) {
        self.children.extend(other.children);
    }
}

#[derive(Debug, Clone)]
struct CachedCodeBlockStyles {
    highlight_theme: Arc<HighlightTheme>,
    styles: Vec<(Range<usize>, HighlightStyle)>,
}

pub const CODE_BLOCK_GROUP: &str = "penna-code-block";

#[derive(Debug, Clone)]
pub struct CodeBlock {
    lang: Option<SharedString>,
    meta: Option<SharedString>,
    styles: Arc<Mutex<Option<CachedCodeBlockStyles>>>,
    state: Arc<Mutex<InlineState>>,
    pub span: Option<Span>,
}

impl PartialEq for CodeBlock {
    fn eq(&self, other: &Self) -> bool {
        self.lang == other.lang
            && self.meta == other.meta
            && self.code() == other.code()
            && self.span == other.span
    }
}

impl CodeBlock {
    pub fn lang(&self) -> Option<SharedString> {
        self.lang.clone()
    }

    pub fn meta(&self) -> Option<SharedString> {
        self.meta.clone()
    }

    pub(crate) fn with_meta(mut self, meta: Option<SharedString>) -> Self {
        self.meta = meta;
        self
    }

    pub fn code(&self) -> SharedString {
        self.state
            .lock()
            .map(|state| state.text.clone())
            .unwrap_or_default()
    }

    pub(crate) fn new(
        code: SharedString,
        lang: Option<SharedString>,
        span: Option<impl Into<Span>>,
    ) -> Self {
        let state = Arc::new(Mutex::new(InlineState::default()));
        if let Ok(mut state) = state.lock() {
            state.set_text(code);
        }

        Self {
            lang,
            meta: None,
            styles: Arc::new(Mutex::new(None)),
            state,
            span: span.map(|s| s.into()),
        }
    }

    pub(crate) fn styles(
        &self,
        highlight_theme: &Arc<HighlightTheme>,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let Some(lang) = &self.lang else {
            return Vec::new();
        };

        let Ok(mut styles) = self.styles.lock() else {
            return Vec::new();
        };

        if let Some(cached) = styles.as_mut() {
            if Arc::ptr_eq(&cached.highlight_theme, highlight_theme) {
                return cached.styles.clone();
            }

            if cached.highlight_theme.as_ref() == highlight_theme.as_ref() {
                cached.highlight_theme = highlight_theme.clone();
                return cached.styles.clone();
            }
        }

        let code = self.code();
        let computed_styles = CODE_BLOCK_HIGHLIGHTERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            let highlighter = cache
                .entry(lang.clone())
                .or_insert_with(|| SyntaxHighlighter::new(lang));

            if let Some(config) = LanguageRegistry::singleton().language(lang)
                && highlighter.language() != &config.name
            {
                *highlighter = SyntaxHighlighter::new(lang);
            }

            highlighter.update(None, &ropey::Rope::from(code.as_str()), None);
            highlighter.styles(&(0..code.len()), highlight_theme)
        });
        *styles = Some(CachedCodeBlockStyles {
            highlight_theme: highlight_theme.clone(),
            styles: computed_styles.clone(),
        });
        computed_styles
    }

    pub(super) fn selected_text(&self) -> String {
        let mut text = String::new();
        if let Ok(state) = self.state.lock()
            && let Some(selection) = &state.selection
        {
            text.push_str(&state.text[selection.start..selection.end]);
        }
        text
    }

    pub(super) fn text(&self) -> String {
        self.state
            .lock()
            .map(|state| state.text.to_string())
            .unwrap_or_default()
    }

    pub(super) fn clear_selection(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.selection = None;
        }
    }

    fn render(
        &self,
        options: &NodeRenderOptions,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let style = &node_cx.style;
        let actions = node_cx.code_block_actions.clone();
        let body = node_cx
            .code_block_body
            .as_ref()
            .and_then(|body| body(self, window, cx));
        let code = cx
            .theme()
            .layer(Surface::new(roles::CARD_CODE), PaintState::Rest);

        div()
            .w_full()
            .min_w_0()
            .when(!options.is_last, |this| this.pb(style.paragraph_gap))
            .child(
                div()
                    .id(("codeblock", options.ix))
                    .group(CODE_BLOCK_GROUP)
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_3()
                    .pt(if actions.is_some() {
                        px(44.0)
                    } else {
                        px(12.0)
                    })
                    .layer(&code)
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(cx.theme().mono_font_size)
                    .relative()
                    .refine_style(&style.code_block)
                    .map(|this| match body {
                        Some(body) => this.child(body),
                        None => this.child(
                            Inline::new(
                                "code",
                                self.state.clone(),
                                vec![],
                                self.styles(&cx.theme().highlight_theme),
                            )
                            .streaming_veil(
                                node_cx.streaming_veil.clone(),
                                VeilKey::new(self.span.map_or(options.ix, |span| span.start), 0),
                            ),
                        ),
                    })
                    .when_some(actions, |this, actions| {
                        this.child(
                            div()
                                .id("actions")
                                .absolute()
                                .top_2()
                                .left_2()
                                .right_2()
                                .flex()
                                .items_center()
                                .justify_end()
                                .child(actions(&self, window, cx)),
                        )
                    }),
            )
            .into_any_element()
    }
}

#[derive(Default, Clone)]
pub(crate) struct NodeContext {
    pub(crate) offset: usize,
    pub(crate) link_refs: HashMap<SharedString, LinkMark>,
    pub(crate) style: TextViewStyle,
    pub(crate) code_block_actions: Option<Arc<CodeBlockActionsFn>>,
    pub(crate) code_block_body: Option<Arc<CodeBlockBodyFn>>,
    pub(crate) links: Option<SharedLinks>,
    pub(crate) markdown_extensions: Arc<MarkdownExtensions>,
    pub(crate) streaming_veil: Option<StreamingVeil>,
}

impl NodeContext {
    pub(super) fn add_ref(&mut self, identifier: SharedString, link: LinkMark) {
        self.link_refs.insert(identifier, link);
    }
}

impl PartialEq for NodeContext {
    fn eq(&self, other: &Self) -> bool {
        self.link_refs == other.link_refs && self.style == other.style
    }
}

impl Paragraph {
    fn render(&self, node_cx: &NodeContext, _window: &mut Window, cx: &mut App) -> AnyElement {
        let span = self.span;
        let children = &self.children;

        if self.should_render_inline_flow() {
            return InlineFlow::new(
                span.unwrap_or_default(),
                self.inline_flow_items(node_cx, cx),
            )
            .links(node_cx.links.clone())
            .into_any_element();
        }

        let mut child_nodes: Vec<AnyElement> = vec![];

        let mut text = String::new();
        let mut highlights: Vec<(Range<usize>, HighlightStyle)> = vec![];
        let mut links: Vec<(Range<usize>, LinkMark)> = vec![];
        let mut code_ranges: Vec<Range<usize>> = vec![];
        let mut offset = 0;

        let mut ix = 0;
        for inline_node in children {
            let text_len = inline_node.text.len();
            text.push_str(&inline_node.text);

            if let Some(image) = &inline_node.image {
                if text.len() > 0 {
                    if let Ok(mut state) = inline_node.state.lock() {
                        state.set_text(text.clone().into());
                    }
                    let element = child_nodes.len();
                    child_nodes.push(
                        Inline::new(
                            ix,
                            inline_node.state.clone(),
                            links.clone(),
                            highlights.clone(),
                        )
                        .code_ranges(code_ranges.clone())
                        .links(node_cx.links.clone())
                        .streaming_veil(
                            node_cx.streaming_veil.clone(),
                            VeilKey::new(span.unwrap_or_default().start, element),
                        )
                        .into_any_element(),
                    );
                }
                child_nodes.push(
                    img(image.url.clone())
                        .id(ix)
                        .object_fit(ObjectFit::Contain)
                        .max_w(relative(1.))
                        .when_some(image.width, |this, width| this.w(width))
                        .when_some(image.link.clone(), |this, link| {
                            let title = image.title();
                            let links = node_cx.links.clone();
                            this.cursor_pointer()
                                .tooltip(move |window, cx| {
                                    Tooltip::new(title.clone()).build(window, cx)
                                })
                                .on_click(move |event, window, cx| {
                                    window.end_text_selection(cx);
                                    cx.stop_propagation();
                                    let click = LinkClick {
                                        url: link.url.clone(),
                                        button: ala::MouseButton::Left,
                                        modifiers: event.modifiers(),
                                    };
                                    open_link(links.as_ref(), &click, window, cx);
                                })
                        })
                        .into_any_element(),
                );

                text.clear();
                links.clear();
                highlights.clear();
                code_ranges.clear();
                offset = 0;
            } else {
                let mut node_highlights = vec![];
                for (range, style) in &inline_node.marks {
                    let inner_range = (offset + range.start)..(offset + range.end);

                    let mut highlight = HighlightStyle::default();
                    if style.bold {
                        highlight.font_weight = Some(FontWeight::BOLD);
                    }
                    if style.italic {
                        highlight.font_style = Some(FontStyle::Italic);
                    }
                    if style.strikethrough {
                        highlight.strikethrough = Some(ala::StrikethroughStyle {
                            thickness: ala::px(1.),
                            ..Default::default()
                        });
                    }
                    if style.underline {
                        highlight.underline = Some(ala::UnderlineStyle {
                            thickness: ala::px(1.),
                            ..Default::default()
                        });
                    }
                    if style.code {
                        code_ranges.push(inner_range.clone());
                    }
                    if let Some(color) = style.highlight {
                        highlight.background_color = Some(color);
                    }

                    if let Some(mut link_mark) = style.link.clone() {
                        if let Some(identifier) = link_mark.identifier.as_ref() {
                            if let Some(mark) = node_cx.link_refs.get(identifier) {
                                link_mark = mark.clone();
                            }
                        }
                        highlight = highlight.highlight(link_highlight(
                            node_cx.links.as_ref(),
                            &link_mark.url,
                            cx,
                        ));

                        links.push((inner_range.clone(), link_mark));
                    }

                    node_highlights.push((inner_range, highlight));
                }

                highlights = ala::combine_highlights(highlights, node_highlights).collect();
                offset += text_len;
            }
            ix += 1;
        }

        if text.len() > 0 {
            if let Ok(mut state) = self.state.lock() {
                state.set_text(text.into());
            }
            let element = child_nodes.len();
            child_nodes.push(
                Inline::new(ix, self.state.clone(), links, highlights)
                    .code_ranges(code_ranges)
                    .links(node_cx.links.clone())
                    .streaming_veil(
                        node_cx.streaming_veil.clone(),
                        VeilKey::new(span.unwrap_or_default().start, element),
                    )
                    .into_any_element(),
            );
        }

        div()
            .id(span.unwrap_or_default())
            .children(child_nodes)
            .into_any_element()
    }

    fn should_render_inline_flow(&self) -> bool {
        let has_image = self.children.iter().any(|child| child.image.is_some());
        let has_text = self.children.iter().any(|child| !child.text.is_empty());
        has_image && has_text
    }

    fn inline_flow_items(&self, node_cx: &NodeContext, cx: &mut App) -> Vec<InlineFlowItem> {
        let mut items = Vec::new();
        let mut text = String::new();
        let mut highlights: Vec<(Range<usize>, HighlightStyle)> = vec![];
        let mut links: Vec<(Range<usize>, LinkMark)> = vec![];
        let mut code_ranges: Vec<Range<usize>> = vec![];
        let mut offset = 0;

        for inline_node in &self.children {
            let text_len = inline_node.text.len();
            text.push_str(&inline_node.text);

            if let Some(image) = &inline_node.image {
                if !text.is_empty() {
                    if let Ok(mut state) = inline_node.state.lock() {
                        state.set_text(text.clone().into());
                    }
                    items.push(InlineFlowItem::Text {
                        state: inline_node.state.clone(),
                        text: text.clone().into(),
                        links: links.clone(),
                        highlights: highlights.clone(),
                        code_ranges: code_ranges.clone(),
                    });
                }

                items.push(InlineFlowItem::Image {
                    url: image.url.clone(),
                    link: image.link.clone(),
                    title: image.title(),
                    width: image.width,
                    height: image.height,
                });

                text.clear();
                links.clear();
                highlights.clear();
                code_ranges.clear();
                offset = 0;
            } else {
                let mut node_highlights = vec![];
                for (range, style) in &inline_node.marks {
                    let inner_range = (offset + range.start)..(offset + range.end);

                    let mut highlight = HighlightStyle::default();
                    if style.bold {
                        highlight.font_weight = Some(FontWeight::BOLD);
                    }
                    if style.italic {
                        highlight.font_style = Some(FontStyle::Italic);
                    }
                    if style.strikethrough {
                        highlight.strikethrough = Some(ala::StrikethroughStyle {
                            thickness: ala::px(1.),
                            ..Default::default()
                        });
                    }
                    if style.underline {
                        highlight.underline = Some(ala::UnderlineStyle {
                            thickness: ala::px(1.),
                            ..Default::default()
                        });
                    }
                    if style.code {
                        code_ranges.push(inner_range.clone());
                    }
                    if let Some(color) = style.highlight {
                        highlight.background_color = Some(color);
                    }

                    if let Some(mut link_mark) = style.link.clone() {
                        if let Some(identifier) = link_mark.identifier.as_ref()
                            && let Some(mark) = node_cx.link_refs.get(identifier)
                        {
                            link_mark = mark.clone();
                        }
                        highlight = highlight.highlight(link_highlight(
                            node_cx.links.as_ref(),
                            &link_mark.url,
                            cx,
                        ));

                        links.push((inner_range.clone(), link_mark));
                    }

                    node_highlights.push((inner_range, highlight));
                }

                highlights = ala::combine_highlights(highlights, node_highlights).collect();
                offset += text_len;
            }
        }

        if !text.is_empty() {
            if let Ok(mut state) = self.state.lock() {
                state.set_text(text.clone().into());
            }
            items.push(InlineFlowItem::Text {
                state: self.state.clone(),
                text: text.into(),
                links,
                highlights,
                code_ranges,
            });
        }

        items
    }
}

impl Paragraph {
    fn to_markdown(&self) -> String {
        let mut text = self
            .children
            .iter()
            .map(|text_node| {
                let mut text = text_node.text.to_string();
                for (range, style) in &text_node.marks {
                    if style.bold {
                        text = format!("**{}**", &text_node.text[range.clone()]);
                    }
                    if style.italic {
                        text = format!("*{}*", &text_node.text[range.clone()]);
                    }
                    if style.strikethrough {
                        text = format!("~~{}~~", &text_node.text[range.clone()]);
                    }
                    if style.code {
                        text = format!("`{}`", &text_node.text[range.clone()]);
                    }
                    if style.highlight.is_some() {
                        text = format!("=={}==", &text_node.text[range.clone()]);
                    }
                    if let Some(link) = &style.link {
                        text = format!("[{}]({})", &text_node.text[range.clone()], link.url);
                    }
                }

                if let Some(image) = &text_node.image {
                    let alt = image.alt.clone().unwrap_or_default();
                    let title = image
                        .title
                        .clone()
                        .map_or(String::new(), |t| format!(" \"{}\"", t));
                    text.push_str(&format!("![{}]({}{})", alt, image.url, title))
                }

                text
            })
            .collect::<Vec<_>>()
            .join("");

        text.push_str("\n\n");
        text
    }
}

impl BlockNode {
    #[allow(dead_code)]
    pub(crate) fn to_markdown(&self) -> String {
        match self {
            BlockNode::Root { children, .. } => children
                .iter()
                .map(|child| child.to_markdown())
                .collect::<Vec<_>>()
                .join("\n\n"),
            BlockNode::Paragraph(paragraph) => paragraph.to_markdown(),
            BlockNode::Heading {
                level, children, ..
            } => {
                let hashes = "#".repeat(*level as usize);
                format!("{} {}", hashes, children.to_markdown())
            }
            BlockNode::Blockquote { children, .. } => {
                let content = children
                    .iter()
                    .map(|child| child.to_markdown())
                    .collect::<Vec<_>>()
                    .join("\n\n");

                content
                    .lines()
                    .map(|line| format!("> {}", line))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            BlockNode::List {
                children, ordered, ..
            } => children
                .iter()
                .enumerate()
                .map(|(i, child)| {
                    let prefix = if *ordered {
                        format!("{}. ", i + 1)
                    } else {
                        "- ".to_string()
                    };
                    format!("{}{}", prefix, child.to_markdown())
                })
                .collect::<Vec<_>>()
                .join("\n"),
            BlockNode::ListItem {
                children, checked, ..
            } => {
                let checkbox = if let Some(checked) = checked {
                    if *checked { "[x] " } else { "[ ] " }
                } else {
                    ""
                };
                format!(
                    "{}{}",
                    checkbox,
                    children
                        .iter()
                        .map(|child| child.to_markdown())
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            BlockNode::CodeBlock(code_block) => {
                format!(
                    "```{}\n{}\n```",
                    code_block.lang.clone().unwrap_or_default(),
                    code_block.code()
                )
            }
            BlockNode::Table(table) => {
                let header = table
                    .children
                    .first()
                    .map(|row| {
                        row.children
                            .iter()
                            .map(|cell| cell.children.to_markdown())
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .unwrap_or_default();
                let alignments = table
                    .column_aligns
                    .iter()
                    .map(|align| {
                        match align {
                            ColumnumnAlign::Left => ":--",
                            ColumnumnAlign::Center => ":-:",
                            ColumnumnAlign::Right => "--:",
                        }
                        .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                let rows = table
                    .children
                    .iter()
                    .skip(1)
                    .map(|row| {
                        row.children
                            .iter()
                            .map(|cell| cell.children.to_markdown())
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{}\n{}\n{}", header, alignments, rows)
            }
            BlockNode::Break { html, .. } => {
                if *html {
                    "<br>".to_string()
                } else {
                    "\n".to_string()
                }
            }
            BlockNode::HorizontalRule { .. } => "---".to_string(),
            BlockNode::Custom { node, .. } => node.to_markdown(),
            BlockNode::Definition {
                identifier,
                url,
                title,
                ..
            } => {
                if let Some(title) = title {
                    format!("[{}]: {} \"{}\"", identifier, url, title)
                } else {
                    format!("[{}]: {}", identifier, url)
                }
            }
            BlockNode::Unknown { .. } => "".to_string(),
        }
        .trim()
        .to_string()
    }
}

impl BlockNode {
    fn render_list_item_row(
        content: AnyElement,
        ix: usize,
        options: NodeRenderOptions,
        checked: Option<bool>,
        window: &Window,
        cx: &mut App,
    ) -> Div {
        let line_height = window.line_height();
        let font_size = window.text_style().font_size.to_pixels(window.rem_size());
        let drop = crate::icon::cap_center_drop(font_size, font_size, cx);
        h_flex()
            .w_full()
            .flex_1()
            .min_w_0()
            .relative()
            .items_start()
            .content_start()
            .when(!options.todo && checked.is_none(), |this| {
                this.child(list_item_prefix(ix, options.ordered, options.depth))
            })
            .when_some(checked, |this, checked| {
                this.child(
                    div()
                        .flex_none()
                        .h(line_height)
                        .mr_1p5()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .relative()
                                .top(drop)
                                .flex()
                                .child(Check::checkbox(checked).with_size(font_size)),
                        ),
                )
            })
            .child(div().flex_1().min_w_0().overflow_hidden().child(content))
    }

    fn render_list_item(
        item: &BlockNode,
        ix: usize,
        options: NodeRenderOptions,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        match item {
            BlockNode::ListItem {
                children,
                spread,
                checked,
                ..
            } => v_flex()
                .id(("li", options.ix))
                .w_full()
                .min_w_0()
                .when(*spread, |this| this.child(div()))
                .children({
                    let mut items: Vec<Div> = Vec::with_capacity(children.len());

                    for (child_ix, child) in children.iter().enumerate() {
                        match child {
                            BlockNode::Paragraph { .. } => {
                                let last_not_list = child_ix > 0
                                    && !matches!(children[child_ix - 1], BlockNode::List { .. });

                                let text = child.render_block(
                                    NodeRenderOptions {
                                        depth: options.depth + 1,
                                        todo: checked.is_some(),
                                        is_last: true,
                                        ..options
                                    },
                                    node_cx,
                                    window,
                                    cx,
                                );

                                if last_not_list {
                                    if let Some(preceding_row) = items.pop() {
                                        items.push(
                                            v_flex().child(preceding_row).child(
                                                div()
                                                    .w_full()
                                                    .pl(rems(1.))
                                                    .overflow_hidden()
                                                    .child(text),
                                            ),
                                        );
                                        continue;
                                    }
                                }

                                items.push(Self::render_list_item_row(
                                    text, ix, options, *checked, window, cx,
                                ));
                            }
                            BlockNode::List { .. } => {
                                items.push(div().ml(rems(1.)).child(child.render_block(
                                    NodeRenderOptions {
                                        depth: options.depth + 1,
                                        todo: checked.is_some(),
                                        is_last: true,
                                        ..options
                                    },
                                    node_cx,
                                    window,
                                    cx,
                                )));
                            }
                            BlockNode::Root { .. }
                            | BlockNode::Heading { .. }
                            | BlockNode::Blockquote { .. }
                            | BlockNode::CodeBlock(_)
                            | BlockNode::Custom { .. }
                            | BlockNode::Table(_)
                            | BlockNode::HorizontalRule { .. } => {
                                let block = child.render_block(
                                    NodeRenderOptions {
                                        depth: options.depth + 1,
                                        todo: checked.is_some(),
                                        is_last: true,
                                        ..options
                                    },
                                    node_cx,
                                    window,
                                    cx,
                                );

                                if child_ix == 0 {
                                    items.push(Self::render_list_item_row(
                                        block, ix, options, *checked, window, cx,
                                    ));
                                } else {
                                    items.push(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .pl(rems(1.))
                                            .overflow_hidden()
                                            .child(block),
                                    );
                                }
                            }
                            BlockNode::ListItem { .. }
                            | BlockNode::Break { .. }
                            | BlockNode::Definition { .. }
                            | BlockNode::Unknown => {}
                        }
                    }
                    items
                })
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn render_table(
        item: &BlockNode,
        options: &NodeRenderOptions,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let table = match item {
            BlockNode::Table(table) => table,
            _ => return div().into_any_element(),
        };

        let col_count = table
            .children
            .iter()
            .map(|row| row.children.len())
            .max()
            .unwrap_or(0);
        let col_w = measure_table_columns(table, col_count, window);

        if matches!(node_cx.style.table.overflow.x, Some(Overflow::Scroll)) {
            Self::render_scroll_table(table, &col_w, options, node_cx, window, cx)
        } else {
            Self::render_wrap_table(table, &col_w, options, node_cx, window, cx)
        }
    }

    fn render_scroll_table(
        table: &Table,
        col_w: &[f32],
        options: &NodeRenderOptions,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let col_w = col_w
            .iter()
            .map(|width| width.min(CELL_MAX_PX))
            .collect::<Vec<_>>();
        let total_w: f32 = col_w.iter().sum();

        let style = &node_cx.style;
        let table_scroll_key = if let Some(span) = table.span {
            SharedString::from(format!(
                "{}-table-scroll-{}:{}",
                window.current_view(),
                span.start,
                span.end
            ))
        } else {
            SharedString::from(format!(
                "{}-table-scroll-{}",
                window.current_view(),
                options.ix
            ))
        };
        let scroll_handle = window
            .use_keyed_state(table_scroll_key, cx, |_, _| ScrollHandle::default())
            .read(cx)
            .clone();
        let row_count = table.children.len();
        let mut rows = Vec::with_capacity(row_count);
        for (row_ix, row) in table.children.iter().enumerate() {
            let mut cells = Vec::with_capacity(row.children.len());
            for (ix, cell) in row.children.iter().enumerate() {
                let align = table.column_align(ix);
                let is_last_col = ix == row.children.len() - 1;
                let width = col_w.get(ix).copied().unwrap_or(CELL_MIN_PX);
                cells.push(
                    div()
                        .id(("cell", ix))
                        .flex_basis(px(width))
                        .flex_grow(width)
                        .flex_shrink_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .when(align == ColumnumnAlign::Center, |this| this.text_center())
                        .when(align == ColumnumnAlign::Right, |this| this.text_right())
                        .px_2()
                        .py_1()
                        .when(!is_last_col, |this| {
                            this.rule(RuleLook::Plain, Edge::Right, cx)
                        })
                        .refine_style(&style.table_cell)
                        .child(cell.children.render(node_cx, window, cx)),
                );
            }
            rows.push(
                div()
                    .id("row")
                    .w_full()
                    .when(row_ix < row_count - 1, |this| {
                        this.rule(RuleLook::Plain, Edge::Bottom, cx)
                    })
                    .flex()
                    .flex_row()
                    .children(cells),
            );
        }

        div()
            .pb(rems(1.))
            .w_full()
            .child(horizontal_scroll_area(
                ("table", options.ix),
                &scroll_handle,
                &style.table,
                figure(div().min_w_full().w(px(total_w)), cx).children(rows),
            ))
            .into_any_element()
    }

    fn render_wrap_table(
        table: &Table,
        col_w: &[f32],
        options: &NodeRenderOptions,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let style = &node_cx.style;
        let row_count = table.children.len();
        let mut rows = Vec::with_capacity(row_count);
        for (row_ix, row) in table.children.iter().enumerate() {
            let mut cells = Vec::with_capacity(row.children.len());
            for (ix, cell) in row.children.iter().enumerate() {
                let align = table.column_align(ix);
                let is_last_col = ix == row.children.len() - 1;
                let width = col_w
                    .get(ix)
                    .copied()
                    .unwrap_or(WRAP_CELL_MAX_PX)
                    .min(WRAP_CELL_MAX_PX);

                cells.push(
                    div()
                        .id(("cell", ix))
                        .overflow_hidden()
                        .when(align == ColumnumnAlign::Center, |this| this.text_center())
                        .when(align == ColumnumnAlign::Right, |this| this.text_right())
                        .min_w_16()
                        .w(Length::Definite(relative(width)))
                        .px_2()
                        .py_1()
                        .when(!is_last_col, |this| {
                            this.rule(RuleLook::Plain, Edge::Right, cx)
                        })
                        .refine_style(&style.table_cell)
                        .child(cell.children.render(node_cx, window, cx)),
                );
            }

            rows.push(
                div()
                    .id("row")
                    .w_full()
                    .when(row_ix < row_count - 1, |this| {
                        this.rule(RuleLook::Plain, Edge::Bottom, cx)
                    })
                    .flex()
                    .flex_row()
                    .children(cells),
            );
        }

        div()
            .pb(rems(1.))
            .w_full()
            .child(
                figure(div().id(("table", options.ix)).w_full(), cx)
                    .overflow_hidden()
                    .children(rows)
                    .refine_style(&style.table),
            )
            .into_any_element()
    }

    pub(crate) fn render_block(
        &self,
        options: NodeRenderOptions,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let ix = options.ix;
        let mb = if options.in_list || options.is_last {
            rems(0.)
        } else {
            node_cx.style.paragraph_gap
        };

        match self {
            BlockNode::Root { children, .. } => div()
                .id(("div", ix))
                .children(children.into_iter().enumerate().map(move |(ix, node)| {
                    node.render_block(NodeRenderOptions { ix, ..options }, node_cx, window, cx)
                }))
                .into_any_element(),
            BlockNode::Paragraph(paragraph) => div()
                .id(("p", ix))
                .pb(mb)
                .child(paragraph.render(node_cx, window, cx))
                .into_any_element(),
            BlockNode::Heading {
                level, children, ..
            } => {
                let (text_size, font_weight) = match level {
                    1 => (rems(2.), FontWeight::BOLD),
                    2 => (rems(1.5), FontWeight::SEMIBOLD),
                    3 => (rems(1.25), FontWeight::SEMIBOLD),
                    4 => (rems(1.125), FontWeight::SEMIBOLD),
                    5 => (rems(1.), FontWeight::SEMIBOLD),
                    6 => (rems(1.), FontWeight::MEDIUM),
                    _ => (rems(1.), FontWeight::NORMAL),
                };

                let heading_base_font_size =
                    crate::rems_from_px(node_cx.style.heading_base_font_size.as_f32())
                        .to_pixels(window.rem_size());
                let mut text_size = text_size.to_pixels(heading_base_font_size);
                if let Some(f) = node_cx.style.heading_font_size.as_ref() {
                    text_size = (f)(*level, heading_base_font_size);
                }

                div()
                    .id(SharedString::from(format!("h{}-{}", level, ix)))
                    .pb(rems(0.3))
                    .whitespace_normal()
                    .text_size(text_size)
                    .font_weight(font_weight)
                    .child(children.render(node_cx, window, cx))
                    .into_any_element()
            }
            BlockNode::Blockquote { children, .. } => div()
                .w_full()
                .pb(mb)
                .child(
                    div()
                        .id(("blockquote", ix))
                        .w_full()
                        .text_color(cx.theme().foreground.muted())
                        .border_l(BLOCKQUOTE_BAR_WIDTH)
                        .border_color(cx.theme().divider_color())
                        .px_4()
                        .children({
                            let children_len = children.len();
                            children.into_iter().enumerate().map(move |(index, c)| {
                                let is_last = index == children_len - 1;
                                c.render_block(options.is_last(is_last), node_cx, window, cx)
                            })
                        }),
                )
                .into_any_element(),
            BlockNode::List {
                children, ordered, ..
            } => v_flex()
                .id((if *ordered { "ol" } else { "ul" }, ix))
                .w_full()
                .min_w_0()
                .pb(mb)
                .children({
                    let mut items = Vec::with_capacity(children.len());
                    let mut item_index = 0;
                    for (ix, item) in children.into_iter().enumerate() {
                        let is_item = item.is_list_item();

                        items.push(Self::render_list_item(
                            item,
                            item_index,
                            NodeRenderOptions {
                                ix,
                                ordered: *ordered,
                                ..options
                            },
                            node_cx,
                            window,
                            cx,
                        ));

                        if is_item {
                            item_index += 1;
                        }
                    }
                    items
                })
                .into_any_element(),
            BlockNode::CodeBlock(code_block) => code_block.render(&options, node_cx, window, cx),
            BlockNode::Custom { node, state } => {
                let inner = match node_cx.markdown_extensions.render_block(node, window, cx) {
                    Some(rendered) => rendered,
                    None => div().child(node.as_text().to_string()).into_any_element(),
                };

                div()
                    .pb(mb)
                    .child(CustomBlockElement {
                        content: inner,
                        state: state.clone(),
                    })
                    .into_any_element()
            }
            BlockNode::Table { .. } => {
                Self::render_table(self, &options, node_cx, window, cx).into_any_element()
            }
            BlockNode::HorizontalRule { .. } => div()
                .pb(mb)
                .child(
                    div()
                        .id("horizontal-rule")
                        .w_full()
                        .rule(RuleLook::Plain, Edge::Top, cx),
                )
                .into_any_element(),
            BlockNode::Break { .. } => div().id("break").into_any_element(),
            BlockNode::Unknown { .. } | BlockNode::Definition { .. } => div().into_any_element(),
            _ => {
                if cfg!(debug_assertions) {
                    log::warn!("unknown implementation: {:?}", self);
                }

                div().into_any_element()
            }
        }
    }
}

const CELL_PAD_PX: f32 = 16.0;
const CELL_MIN_PX: f32 = 48.0;
const CELL_MAX_PX: f32 = 480.0;
const WRAP_CELL_MAX_PX: f32 = 1200.0;

fn measure_table_columns(table: &Table, col_count: usize, window: &mut Window) -> Vec<f32> {
    let text_style = window.text_style();
    let font_size = text_style.font_size.to_pixels(window.rem_size());
    let mut col_w = vec![CELL_MIN_PX; col_count];
    for row in table.children.iter() {
        for (ix, cell) in row.children.iter().enumerate() {
            let Some(slot) = col_w.get_mut(ix) else {
                continue;
            };
            let mut w = 0.0_f32;
            for line in cell.children.text().split('\n') {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let run = text_style.to_run(line.len());
                let line_w = window
                    .text_system()
                    .layout_line(line, font_size, &[run], None)
                    .width;
                w = w.max(f32::from(line_w));
            }
            *slot = slot.max(w + CELL_PAD_PX);
        }
    }
    col_w
}

fn figure<E: Styled>(element: E, cx: &App) -> E {
    element.surface(Surface::new(roles::CARD_FIGURE), cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ala::test]
    fn inline_code_beside_an_image_keeps_its_rounded_code_fill(cx: &mut ala::TestAppContext) {
        cx.update(crate::init);
        let document = super::super::markdown_parse::parse(
            "run `ls` ![icon](icon.png) now",
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
            panic!("expected paragraph");
        };
        assert!(paragraph.should_render_inline_flow());
        let items = cx.update(|cx| paragraph.inline_flow_items(&NodeContext::default(), cx));
        let InlineFlowItem::Text {
            text,
            highlights,
            code_ranges,
            ..
        } = &items[0]
        else {
            panic!("expected text before the image");
        };
        assert_eq!(code_ranges.len(), 1);
        assert_eq!(&text[code_ranges[0].clone()], "ls");
        assert!(
            highlights
                .iter()
                .all(|(_, style)| style.background_color.is_none())
        );
    }

    #[ala::test]
    fn table_columns_follow_measured_width_not_utf8_bytes(cx: &mut ala::TestAppContext) {
        let document = super::super::markdown_parse::parse(
            "| 中文中文 | abcdefghijkl |\n| --- | --- |\n| 中 | a |\n",
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Table(table) = &document.blocks[0] else {
            panic!("expected table");
        };
        let cx = cx.add_empty_window();
        let widths = cx.update(|window, _| measure_table_columns(table, 2, window));
        let content = widths.iter().map(|w| w - CELL_PAD_PX).collect::<Vec<_>>();
        assert!(
            (content[0] * 3.0 - content[1]).abs() < 0.5,
            "four CJK glyphs should take a third of twelve ASCII glyphs, got {widths:?}"
        );
    }

    #[test]
    fn custom_blocks_are_atomic_in_both_drag_directions() {
        use ala::{point, size};
        let bounds = Bounds::new(point(px(10.), px(20.)), size(px(100.), px(40.)));
        let inside = bounds.center();
        let above = point(px(0.), px(0.));
        let below = point(px(0.), px(80.));
        for (start, end) in [(above, inside), (inside, below), (above, below)] {
            assert!(custom_block_is_selected(bounds, start, end));
            assert!(custom_block_is_selected(bounds, end, start));
        }
        assert!(!custom_block_is_selected(bounds, inside, inside));
        for (start, end) in [
            (inside, inside + point(px(1.), px(0.))),
            (bounds.origin, bounds.bottom_right() - point(px(1.), px(1.))),
        ] {
            assert!(!custom_block_is_selected(bounds, start, end));
            assert!(!custom_block_is_selected(bounds, end, start));
        }
        assert!(!custom_block_is_selected(
            bounds,
            point(px(150.), px(30.)),
            point(px(160.), px(40.))
        ));
    }

    #[test]
    fn a_selected_custom_block_copies_its_text_and_clears() {
        let state = BlockState::default();
        let block = BlockNode::Custom {
            node: MarkdownNode::new("ticker", ()).text("$TSLA.US"),
            state: state.clone(),
        };
        assert_eq!(block.selected_text(), "");
        state.set_selected(true);
        assert_eq!(block.selected_text(), "$TSLA.US\n");
        assert_eq!(
            block,
            BlockNode::Custom {
                node: MarkdownNode::new("ticker", ()).text("$TSLA.US"),
                state: BlockState::default(),
            }
        );
        block.clear_selection();
        assert_eq!(block.selected_text(), "");
    }

    #[ala::test]
    fn a_drag_across_a_custom_block_copies_it(cx: &mut ala::TestAppContext) {
        use super::super::{MarkdownExtensions, TextView, TextViewState};
        use ala::{
            AppContext as _, Context, Entity, Modifiers, MouseButton, ParentElement as _, Render,
            Styled as _, point,
        };

        struct Host {
            state: Entity<TextViewState>,
            extensions: MarkdownExtensions,
        }

        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(300.)).child(
                    TextView::new(&self.state)
                        .selectable(true)
                        .markdown_extensions(self.extensions.clone()),
                )
            }
        }

        let extensions = MarkdownExtensions::default().block_parser(|node, cx| {
            let mdast::Node::Paragraph(paragraph) = node else {
                return None;
            };
            let [mdast::Node::Text(text)] = paragraph.children.as_slice() else {
                return None;
            };
            text.value.starts_with('$').then(|| {
                MarkdownNode::new("ticker", ())
                    .text(text.value.clone())
                    .markdown(cx.node_source(node).unwrap_or_default())
            })
        });
        cx.update(crate::init);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let host = cx.new(|cx| Host {
                state: cx.new(|cx| TextViewState::markdown("before\n\n$TSLA.US\n\nafter", cx)),
                extensions,
            });
            crate::Root::new(host, window, cx)
        });
        let state = root.read_with(cx, |root, cx| {
            root.view()
                .clone()
                .downcast::<Host>()
                .unwrap()
                .read(cx)
                .state
                .clone()
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let bounds = state.read_with(cx, |state, _| state.bounds());

        let end = point(px(290.), bounds.bottom() - px(2.));
        cx.simulate_mouse_down(
            point(px(1.), px(4.)),
            MouseButton::Left,
            Modifiers::default(),
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let selected = state.read_with(cx, |state, _| state.selected_text());
        assert!(
            selected.contains("before") && selected.contains("$TSLA.US"),
            "the custom block was left out of the drag: {selected:?}"
        );
    }

    #[test]
    fn code_block_equality_includes_code_content() {
        let first = CodeBlock::new("let value = 1;".into(), Some("rust".into()), None::<Span>);
        let second = CodeBlock::new("let value = 2;".into(), Some("rust".into()), None::<Span>);

        assert_ne!(first, second);
    }

    #[cfg(feature = "tree-sitter")]
    #[test]
    fn code_block_styles_follow_a_theme_change() {
        let light = Arc::new(HighlightTheme::default_light().clone());
        let dark = Arc::new(HighlightTheme::default_dark().clone());

        let block = CodeBlock::new(
            "fn main() { let answer = 42; }".into(),
            Some("rust".into()),
            None::<Span>,
        );

        let light_styles = block.styles(&light);
        assert!(light_styles.iter().any(|(_, style)| style.color.is_some()));

        let dark_styles = block.styles(&dark);
        assert_ne!(
            light_styles, dark_styles,
            "code block kept the palette it was first rendered with"
        );

        let dark_again = Arc::new(HighlightTheme::default_dark().clone());
        assert_eq!(block.styles(&dark_again), dark_styles);
        let cached = block.styles.lock().unwrap();
        assert!(Arc::ptr_eq(
            &cached.as_ref().unwrap().highlight_theme,
            &dark_again
        ));
    }
}
