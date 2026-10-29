#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::{
    any::Any,
    collections::HashMap,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use ala::{AnyElement, App, IntoElement, SharedString, Window};
use markdown::{ParseOptions, mdast};

use super::node::Span;

static MARKDOWN_EXTENSIONS_REVISION: AtomicU64 = AtomicU64::new(1);

pub use markdown::mdast as markdown_ast;

pub type MarkdownBlockParserFn =
    dyn for<'a> Fn(&mdast::Node, &MarkdownParseContext<'a>) -> Option<MarkdownNode> + Send + Sync;

pub type MarkdownBlockRenderFn =
    dyn Fn(&MarkdownNode, &mut Window, &mut App) -> AnyElement + Send + Sync;

pub trait MarkdownPlugin: Send + Sync + 'static {
    fn is_block(&self) -> bool {
        false
    }

    fn name(&self) -> &str;

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode>;

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement;
}

pub struct MarkdownParseContext<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> MarkdownParseContext<'a> {
    pub(crate) fn new(source: &'a str, offset: usize) -> Self {
        Self { source, offset }
    }

    pub fn source(&self) -> &'a str {
        self.source
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn node_source(&self, node: &mdast::Node) -> Option<&'a str> {
        let position = node.position()?;
        self.source.get(position.start.offset..position.end.offset)
    }
}

#[derive(Clone)]
pub struct MarkdownNode {
    name: SharedString,
    text: SharedString,
    markdown: SharedString,
    data: Arc<dyn Any + Send + Sync>,
    pub(crate) span: Option<Span>,
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
            .finish_non_exhaustive()
    }
}

impl PartialEq for MarkdownNode {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.text == other.text
            && self.markdown == other.markdown
            && self.span == other.span
    }
}

pub type MarkdownLinkRewriterFn = dyn Fn(&str) -> Option<String> + Send + Sync;

#[derive(Clone, Default)]
pub struct MarkdownExtensions {
    block_parsers: Vec<Arc<MarkdownBlockParserFn>>,
    block_renderers: HashMap<SharedString, Arc<MarkdownBlockRenderFn>>,
    link_rewriter: Option<Arc<MarkdownLinkRewriterFn>>,
    revision: u64,
}

impl MarkdownExtensions {
    pub fn block_parser<F>(mut self, parser: F) -> Self
    where
        F: for<'a> Fn(&mdast::Node, &MarkdownParseContext<'a>) -> Option<MarkdownNode>
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
        let plugin = Arc::new(plugin);
        let name = SharedString::from(plugin.name().to_string());
        let parser = plugin.clone();
        let renderer = plugin;

        if parser.is_block() {
            let mut extensions = self.block_parser(move |node, cx| parser.parse(node, cx));
            extensions.push_block_renderer(name, move |node, window, cx| {
                renderer.render(node, window, cx).into_any_element()
            });
            extensions
        } else {
            panic!("inline Markdown plugins are not supported by TextView yet")
        }
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn push_block_parser<F>(&mut self, parser: F)
    where
        F: for<'a> Fn(&mdast::Node, &MarkdownParseContext<'a>) -> Option<MarkdownNode>
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

    pub(crate) fn parse_options(&self) -> ParseOptions {
        ParseOptions::gfm()
    }

    pub(crate) fn parse_block(
        &self,
        node: &mdast::Node,
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
