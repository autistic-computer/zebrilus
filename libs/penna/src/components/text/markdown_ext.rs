#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::{
    any::Any,
    collections::HashMap,
    fmt,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use ala::{AnyElement, App, IntoElement, SharedString, Window};
use comrak::{
    Options,
    nodes::{AstNode, NodeValue, Sourcepos},
};

use super::node::Span;

static MARKDOWN_EXTENSIONS_REVISION: AtomicU64 = AtomicU64::new(1);

pub use comrak::nodes as markdown_ast;

pub type MarkdownBlockParserFn = dyn for<'a, 'b> Fn(&'a AstNode<'a>, &MarkdownParseContext<'b>) -> Option<MarkdownNode>
    + Send
    + Sync;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineSource {
    Text,
    Code,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineLink {
    pub range: Range<usize>,
    pub label: String,
    pub url: String,
}

pub type MarkdownInlineLinksFn = dyn Fn(&str, InlineSource) -> Vec<InlineLink> + Send + Sync;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeFence {
    pub lang: Option<String>,
    pub meta: Option<String>,
    pub value: String,
    pub start: usize,
}

pub(crate) fn code_literal(literal: &str) -> String {
    let literal = literal.strip_suffix('\n').unwrap_or(literal);
    literal.strip_suffix('\r').unwrap_or(literal).to_owned()
}

pub(crate) fn split_info(info: &str) -> (Option<&str>, Option<&str>) {
    let info = info.trim();
    match info.split_once(char::is_whitespace) {
        Some((lang, meta)) => (
            Some(lang),
            Some(meta.trim()).filter(|meta| !meta.is_empty()),
        ),
        None => ((!info.is_empty()).then_some(info), None),
    }
}

pub type MarkdownBlockRenderFn =
    dyn Fn(&MarkdownNode, &mut Window, &mut App) -> AnyElement + Send + Sync;

pub trait MarkdownPlugin: Send + Sync + 'static {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str;

    fn parse<'a>(
        &self,
        node: &'a AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode>;

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement;
}

pub struct MarkdownParseContext<'a> {
    source: &'a str,
    offset: usize,
    lines: Vec<usize>,
}

impl<'a> MarkdownParseContext<'a> {
    pub(crate) fn new(source: &'a str, offset: usize) -> Self {
        let bytes = source.as_bytes();
        let mut lines = vec![0];
        let mut at = 0;
        while at < bytes.len() {
            match bytes[at] {
                b'\r' if bytes.get(at + 1) == Some(&b'\n') => {
                    at += 2;
                    lines.push(at);
                }
                b'\r' | b'\n' => {
                    at += 1;
                    lines.push(at);
                }
                _ => at += 1,
            }
        }
        Self {
            source,
            offset,
            lines,
        }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    fn line_end(&self, line: usize) -> usize {
        let end = self
            .lines
            .get(line + 1)
            .copied()
            .unwrap_or(self.source.len());
        let text = &self.source.as_bytes()[self.lines[line]..end];
        let trimmed = text
            .iter()
            .rposition(|byte| !matches!(byte, b'\r' | b'\n'))
            .map_or(0, |last| last + 1);
        self.lines[line] + trimmed
    }

    pub fn range(&self, sourcepos: Sourcepos) -> Option<Range<usize>> {
        let start_line = sourcepos.start.line.checked_sub(1)?;
        let end_line = sourcepos.end.line.checked_sub(1)?;
        let start = (self.lines.get(start_line)? + sourcepos.start.column.saturating_sub(1))
            .min(self.line_end(start_line));
        let end = (self.lines.get(end_line)? + sourcepos.end.column).min(self.line_end(end_line));
        let mut start = start.min(self.source.len());
        let mut end = end.max(start).min(self.source.len());
        while !self.source.is_char_boundary(start) {
            start -= 1;
        }
        while !self.source.is_char_boundary(end) {
            end += 1;
        }
        Some(start..end)
    }

    pub fn node_range(&self, node: &AstNode<'_>) -> Option<Range<usize>> {
        self.range(node.data.borrow().sourcepos)
    }

    pub fn node_source(&self, node: &AstNode<'_>) -> Option<&'a str> {
        self.source.get(self.node_range(node)?)
    }

    pub fn code_fence(&self, node: &AstNode<'_>) -> Option<CodeFence> {
        let data = node.data.borrow();
        let NodeValue::CodeBlock(code) = &data.value else {
            return None;
        };
        let (lang, meta) = split_info(&code.info);
        Some(CodeFence {
            lang: lang.map(str::to_owned),
            meta: meta.map(str::to_owned),
            value: code_literal(&code.literal),
            start: self.offset + self.range(data.sourcepos).map_or(0, |range| range.start),
        })
    }

    pub fn sole_text(node: &AstNode<'_>) -> Option<String> {
        if !matches!(node.data.borrow().value, NodeValue::Paragraph) {
            return None;
        }
        let child = node.first_child()?;
        if child.next_sibling().is_some() {
            return None;
        }
        match &child.data.borrow().value {
            NodeValue::Text(text) => Some(text.to_string()),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct MarkdownNode {
    name: SharedString,
    text: SharedString,
    markdown: SharedString,
    data: Arc<dyn Any + Send + Sync>,
    pub(crate) span: Option<Span>,
    pub(crate) absorbs: usize,
}

impl MarkdownNode {
    pub fn new<T>(name: impl Into<SharedString>, data: T) -> Self
    where
        T: Any + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            text: SharedString::default(),
            markdown: SharedString::default(),
            data: Arc::new(data),
            span: None,
            absorbs: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn as_text(&self) -> &str {
        &self.text
    }

    pub fn as_markdown(&self) -> &str {
        &self.markdown
    }

    pub fn text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = text.into();
        self
    }

    pub fn markdown(mut self, markdown: impl Into<SharedString>) -> Self {
        self.markdown = markdown.into();
        self
    }

    pub fn absorbs(mut self, siblings: usize) -> Self {
        self.absorbs = siblings;
        self
    }

    pub fn data<T>(&self) -> Option<&T>
    where
        T: Any + Send + Sync + 'static,
    {
        self.data.downcast_ref()
    }

    pub(crate) fn set_span(&mut self, span: Option<Span>) {
        self.span = span;
    }

    pub(crate) fn to_markdown(&self) -> String {
        if self.markdown.is_empty() {
            self.text.to_string()
        } else {
            self.markdown.to_string()
        }
    }
}

impl fmt::Debug for MarkdownNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkdownNode")
            .field("name", &self.name)
            .field("text", &self.text)
            .field("markdown", &self.markdown)
            .field("span", &self.span)
            .field("absorbs", &self.absorbs)
            .finish_non_exhaustive()
    }
}

impl PartialEq for MarkdownNode {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.text == other.text
            && self.markdown == other.markdown
            && self.span == other.span
            && self.absorbs == other.absorbs
    }
}

pub type MarkdownLinkRewriterFn = dyn Fn(&str) -> Option<String> + Send + Sync;

#[derive(Clone, Default)]
pub struct MarkdownExtensions {
    block_parsers: Vec<Arc<MarkdownBlockParserFn>>,
    block_renderers: HashMap<SharedString, Arc<MarkdownBlockRenderFn>>,
    link_rewriter: Option<Arc<MarkdownLinkRewriterFn>>,
    inline_links: Option<Arc<MarkdownInlineLinksFn>>,
    revision: u64,
}

impl MarkdownExtensions {
    pub fn block_parser<F>(mut self, parser: F) -> Self
    where
        F: for<'a, 'b> Fn(&'a AstNode<'a>, &MarkdownParseContext<'b>) -> Option<MarkdownNode>
            + Send
            + Sync
            + 'static,
    {
        self.push_block_parser(parser);
        self
    }

    pub fn plugin<P>(self, plugin: P) -> Self
    where
        P: MarkdownPlugin,
    {
        if !plugin.is_block() {
            log::warn!(
                "skipping Markdown plugin {:?}: only block plugins are supported",
                plugin.name()
            );
            return self;
        }

        let plugin = Arc::new(plugin);
        let name = SharedString::from(plugin.name().to_string());
        let parser = plugin.clone();
        let renderer = plugin;
        let mut extensions = self.block_parser(move |node, cx| parser.parse(node, cx));
        extensions.push_block_renderer(name, move |node, window, cx| {
            renderer.render(node, window, cx).into_any_element()
        });
        extensions
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn push_block_parser<F>(&mut self, parser: F)
    where
        F: for<'a, 'b> Fn(&'a AstNode<'a>, &MarkdownParseContext<'b>) -> Option<MarkdownNode>
            + Send
            + Sync
            + 'static,
    {
        self.block_parsers.push(Arc::new(parser));
        self.bump_revision();
    }

    pub(crate) fn push_block_renderer<F, E>(&mut self, name: impl Into<SharedString>, renderer: F)
    where
        F: Fn(&MarkdownNode, &mut Window, &mut App) -> E + Send + Sync + 'static,
        E: IntoElement,
    {
        self.block_renderers.insert(
            name.into(),
            Arc::new(move |node, window, cx| renderer(node, window, cx).into_any_element()),
        );
        self.bump_revision();
    }

    pub fn link_rewriter<F>(mut self, rewriter: F) -> Self
    where
        F: Fn(&str) -> Option<String> + Send + Sync + 'static,
    {
        self.link_rewriter = Some(Arc::new(rewriter));
        self.bump_revision();
        self
    }

    pub(crate) fn rewrite_link(&self, url: &str) -> Option<String> {
        self.link_rewriter
            .as_ref()
            .and_then(|rewriter| rewriter(url))
    }

    pub fn inline_links<F>(mut self, links: F) -> Self
    where
        F: Fn(&str, InlineSource) -> Vec<InlineLink> + Send + Sync + 'static,
    {
        self.inline_links = Some(Arc::new(links));
        self.bump_revision();
        self
    }

    pub(crate) fn links_in(&self, text: &str, source: InlineSource) -> Vec<InlineLink> {
        self.inline_links
            .as_ref()
            .map(|links| links(text, source))
            .unwrap_or_default()
    }

    pub(crate) fn parse_options(&self) -> Options<'static> {
        let mut options = Options::default();
        let extension = &mut options.extension;
        extension.strikethrough = true;
        extension.table = true;
        extension.autolink = true;
        extension.tasklist = true;
        extension.footnotes = true;
        extension.alerts = true;
        extension.block_directive = true;
        extension.math_code = true;
        options.parse.leave_footnote_definitions = true;
        options
    }

    pub(crate) fn parse_block<'a>(
        &self,
        node: &'a AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        for parser in &self.block_parsers {
            if let Some(node) = parser(node, cx) {
                return Some(node);
            }
        }
        None
    }

    pub(crate) fn render_block(
        &self,
        node: &MarkdownNode,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        self.block_renderers
            .get(node.name())
            .map(|render| render(node, window, cx))
    }

    fn bump_revision(&mut self) {
        self.revision = MARKDOWN_EXTENSIONS_REVISION.fetch_add(1, Ordering::Relaxed);
    }
}
