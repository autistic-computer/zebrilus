#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

use std::ops::Range;

use ala::SharedString;

use crate::{IconName, Tone};
use comrak::{
    Arena,
    nodes::{AlertType, AstNode, ListType, NodeValue},
};

use super::{
    document::ParsedDocument,
    markdown_ext::{InlineSource, MarkdownParseContext, code_literal, split_info},
    node::{
        self, BlockNode, CodeBlock, ImageNode, InlineNode, LinkMark, NodeContext, Paragraph, Span,
        Table, TableRow, TextMark,
    },
};

type Node<'a> = &'a AstNode<'a>;

pub(crate) fn parse(source: &str, cx: &mut NodeContext) -> Result<ParsedDocument, SharedString> {
    let options = cx.markdown_extensions.parse_options();
    let arena = Arena::new();
    let root = comrak::parse_document(&arena, source, &options);
    let parse_cx = MarkdownParseContext::new(source, cx.offset);
    let blocks = siblings(root.first_child(), &parse_cx, cx, true);
    Ok(ParsedDocument {
        source: source.to_string().into(),
        blocks: blocks.into(),
    })
}

fn span(node: Node<'_>, parse_cx: &MarkdownParseContext<'_>) -> Option<Span> {
    let range = parse_cx.node_range(node)?;
    Some(Span {
        start: parse_cx.offset() + range.start,
        end: parse_cx.offset() + range.end,
    })
}

fn parse_table_row(
    table: &mut Table,
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
) {
    let mut row = TableRow::default();
    for cell in node.children() {
        if matches!(cell.data.borrow().value, NodeValue::TableCell) {
            let mut paragraph = Paragraph::default();
            parse_inlines(&mut paragraph, cell, parse_cx, cx);
            paragraph.span = span(cell, parse_cx);
            row.children.push(node::TableCell {
                children: paragraph,
                ..Default::default()
            });
        }
    }
    row.children
        .resize_with(table.column_aligns.len(), Default::default);
    table.children.push(row);
}

fn push_merged(
    paragraph: &mut Paragraph,
    text: String,
    marks: Vec<(Range<usize>, TextMark)>,
    new_mark: TextMark,
) {
    if text.is_empty() {
        return;
    }

    let mut node = InlineNode::new(text).marks(marks);
    let len = node.text.len();
    if let Some(last) = node.marks.last_mut()
        && last.0.start == 0
        && last.0.end == len
    {
        last.1.merge(new_mark);
    } else {
        node.marks.push((0..len, new_mark));
    }
    paragraph.push(node);
}

fn merge_children_with_mark(
    paragraph: &mut Paragraph,
    node: Node<'_>,
    mark: TextMark,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
) -> String {
    let mut merged_text = String::new();
    let mut merged_marks = Vec::new();

    let mut child_paragraph = Paragraph::default();
    let text = parse_inlines(&mut child_paragraph, node, parse_cx, cx);
    {
        for node in child_paragraph.children {
            let merged_offset = merged_text.len();
            merged_text.push_str(&node.text);

            for (range, child_mark) in node.marks {
                merged_marks.push((
                    range.start + merged_offset..range.end + merged_offset,
                    child_mark,
                ));
            }

            if let Some(mut image) = node.image {
                if let Some(link_mark) = mark.link.clone() {
                    image.link = Some(link_mark);
                }

                push_merged(
                    paragraph,
                    std::mem::take(&mut merged_text),
                    std::mem::take(&mut merged_marks),
                    mark.clone(),
                );
                paragraph.push(InlineNode::image(image));
            }
        }
    }

    push_merged(paragraph, merged_text, merged_marks, mark);
    text
}

fn push_link(paragraph: &mut Paragraph, label: &str, url: &str) {
    if label.is_empty() {
        return;
    }
    paragraph.push(InlineNode::new(label).marks(vec![(
        0..label.len(),
        TextMark {
            link: Some(LinkMark {
                url: url.to_owned().into(),
                ..Default::default()
            }),
            ..Default::default()
        },
    )]));
}

fn push_prose(paragraph: &mut Paragraph, text: &str, cx: &NodeContext) -> String {
    let mut links = cx.markdown_extensions.links_in(text, InlineSource::Text);
    if links.is_empty() {
        paragraph.push_str(text);
        return text.to_owned();
    }
    links.sort_by_key(|link| link.range.start);
    let mut shown = String::with_capacity(text.len());
    let mut last = 0;
    for link in links {
        let Range { start, end } = link.range;
        if start < last
            || end > text.len()
            || start >= end
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
        {
            continue;
        }
        if start > last {
            paragraph.push_str(&text[last..start]);
            shown.push_str(&text[last..start]);
        }
        push_link(paragraph, &link.label, &link.url);
        shown.push_str(&link.label);
        last = end;
    }
    if last < text.len() {
        paragraph.push_str(&text[last..]);
        shown.push_str(&text[last..]);
    }
    shown
}

fn parse_inlines(
    paragraph: &mut Paragraph,
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
) -> String {
    let mut text = String::new();
    let mut run = String::new();
    for child in node.children() {
        let joined = match &child.data.borrow().value {
            NodeValue::Text(value) => {
                run.push_str(value);
                true
            }
            NodeValue::SoftBreak => {
                run.push(' ');
                true
            }
            _ => false,
        };
        if joined {
            continue;
        }
        if !run.is_empty() {
            text.push_str(&push_prose(paragraph, &reflow(&run), cx));
            run.clear();
        }
        text.push_str(&parse_paragraph(paragraph, child, parse_cx, cx));
    }
    if !run.is_empty() {
        text.push_str(&push_prose(paragraph, &reflow(&run), cx));
    }
    text
}

fn reflow(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\n', '\r'], " ")
}

fn plain_text(node: Node<'_>, out: &mut String) {
    match &node.data.borrow().value {
        NodeValue::Text(text) => out.push_str(text),
        NodeValue::Code(code) => out.push_str(&code.literal),
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
        _ => {}
    }
    for child in node.children() {
        plain_text(child, out);
    }
}

fn parse_paragraph(
    paragraph: &mut Paragraph,
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
) -> String {
    if let Some(span) = span(node, parse_cx) {
        paragraph.set_span(span);
    }

    let mut text = String::new();
    let value = node.data.borrow().value.clone();

    match value {
        NodeValue::Paragraph => {
            text = parse_inlines(paragraph, node, parse_cx, cx);
        }
        NodeValue::Text(value) => {
            text = push_prose(paragraph, &reflow(&value), cx);
        }
        NodeValue::SoftBreak => {
            text.push(' ');
            paragraph.push_str(" ");
        }
        NodeValue::LineBreak => {
            text.push('\n');
            paragraph.push(InlineNode::new("\n"));
        }
        NodeValue::Emph => {
            text = merge_children_with_mark(
                paragraph,
                node,
                TextMark::default().italic(),
                parse_cx,
                cx,
            );
        }
        NodeValue::Strong => {
            text =
                merge_children_with_mark(paragraph, node, TextMark::default().bold(), parse_cx, cx);
        }
        NodeValue::Strikethrough => {
            text = merge_children_with_mark(
                paragraph,
                node,
                TextMark::default().strikethrough(),
                parse_cx,
                cx,
            );
        }
        NodeValue::Code(code) => {
            let literal = code.literal;
            let whole = 0..literal.len();
            match cx
                .markdown_extensions
                .links_in(&literal, InlineSource::Code)
                .into_iter()
                .find(|link| link.range == whole)
            {
                Some(link) => {
                    push_link(paragraph, &link.label, &link.url);
                    text = link.label;
                }
                None => {
                    paragraph.push(
                        InlineNode::new(&literal)
                            .marks(vec![(0..literal.len(), TextMark::default().code())]),
                    );
                    text = literal;
                }
            }
        }
        NodeValue::Link(link) => {
            let url = cx
                .markdown_extensions
                .rewrite_link(&link.url)
                .map_or_else(|| link.url.clone().into(), SharedString::from);
            let link_mark = Some(LinkMark {
                url,
                title: (!link.title.is_empty()).then(|| link.title.clone().into()),
                ..Default::default()
            });

            text = merge_children_with_mark(
                paragraph,
                node,
                TextMark {
                    link: link_mark,
                    ..Default::default()
                },
                parse_cx,
                cx,
            );
        }
        NodeValue::Image(link) => {
            let mut alt = String::new();
            for child in node.children() {
                plain_text(child, &mut alt);
            }
            paragraph.push_image(ImageNode {
                url: link.url.clone().into(),
                title: (!link.title.is_empty()).then(|| link.title.clone().into()),
                alt: Some(alt.into()),
                ..Default::default()
            });
        }
        NodeValue::Math(math) => {
            text = math.literal;
            paragraph.push(
                InlineNode::new(&text).marks(vec![(0..text.len(), TextMark::default().code())]),
            );
        }
        NodeValue::HtmlInline(html) if is_line_break_tag(&html) => {
            text.push('\n');
            paragraph.push(InlineNode::new("\n"));
        }
        NodeValue::HtmlInline(html) => {
            text = html;
            paragraph.push(InlineNode::new(&text));
        }
        NodeValue::FootnoteReference(foot) => {
            paragraph.push(InlineNode::new(superscript(foot.ix)));
        }
        _ => {
            if cfg!(debug_assertions) {
                log::warn!("unsupported inline node: {:?}", node.data.borrow().value);
            }
        }
    }

    text
}

fn superscript(number: u32) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    number
        .to_string()
        .chars()
        .filter_map(|digit| digit.to_digit(10))
        .map(|digit| DIGITS[digit as usize])
        .collect()
}

fn footnote_number(node: Node<'_>, name: &str) -> Option<u32> {
    node.ancestors()
        .last()?
        .descendants()
        .find_map(|found| match &found.data.borrow().value {
            NodeValue::FootnoteReference(reference) if reference.name == name => Some(reference.ix),
            _ => None,
        })
}

fn is_line_break_tag(html: &str) -> bool {
    matches!(
        html.trim().to_ascii_lowercase().as_str(),
        "<br>" | "<br/>" | "<br />"
    )
}

fn children(
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
    tight: bool,
) -> Vec<BlockNode> {
    siblings(node.first_child(), parse_cx, cx, tight)
}

fn siblings<'a>(
    first: Option<Node<'a>>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
    tight: bool,
) -> Vec<BlockNode> {
    let mut blocks = Vec::new();
    let mut next = first;
    while let Some(node) = next {
        let (block, last) = block_node(node, parse_cx, cx, tight);
        blocks.push(block);
        next = last.next_sibling();
    }
    blocks
}

fn absorbed<'a>(node: Node<'a>, count: usize) -> Node<'a> {
    (0..count)
        .try_fold(node, |node, _| node.next_sibling())
        .unwrap_or(node)
}

fn inline_block(
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
) -> Paragraph {
    let mut paragraph = Paragraph::default();
    parse_inlines(&mut paragraph, node, parse_cx, cx);
    paragraph.span = span(node, parse_cx);
    paragraph
}

fn code_block(info: &str, literal: String, span: Option<Span>) -> CodeBlock {
    let (lang, meta) = split_info(info);
    CodeBlock::new(
        code_literal(&literal).into(),
        lang.map(|lang| lang.to_owned().into()),
        span,
    )
    .with_meta(meta.map(|meta| meta.to_owned().into()))
}

#[allow(clippy::too_many_arguments)]
fn callout(
    name: &str,
    title: Option<&str>,
    open: String,
    directive: bool,
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
    span: Option<Span>,
) -> BlockNode {
    let (tone, icon, label) = match name.to_ascii_lowercase().as_str() {
        "note" | "info" => (Tone::Accent, IconName::Info, "Note"),
        "tip" | "hint" | "success" => (Tone::Success, IconName::Sparkle, "Tip"),
        "important" => (Tone::Primary, IconName::Bell, "Important"),
        "warning" | "warn" => (Tone::Warning, IconName::TriangleAlert, "Warning"),
        "caution" | "danger" | "error" => (Tone::Danger, IconName::CircleX, "Caution"),
        _ => (Tone::Neutral, IconName::Info, ""),
    };
    let title = match title.filter(|title| !title.trim().is_empty()) {
        Some(title) => title.trim().to_owned(),
        None if label.is_empty() => {
            let mut chars = name.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        None => label.to_owned(),
    };
    BlockNode::Callout {
        tone,
        icon,
        title: title.into(),
        open: open.into(),
        directive,
        children: children(node, parse_cx, cx, true),
        span,
    }
}

fn block_node<'a>(
    node: Node<'a>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
    tight: bool,
) -> (BlockNode, Node<'a>) {
    if let Some(mut custom) = cx.markdown_extensions.parse_block(node, parse_cx) {
        let last = absorbed(node, custom.absorbs);
        custom.set_span(
            span(node, parse_cx)
                .zip(span(last, parse_cx))
                .map(|(first, last)| Span {
                    start: first.start,
                    end: last.end,
                }),
        );
        let block = BlockNode::Custom {
            node: custom,
            state: node::BlockState::default(),
        };
        return (block, last);
    }
    (ast_to_node(node, parse_cx, cx, tight), node)
}

fn ast_to_node(
    node: Node<'_>,
    parse_cx: &MarkdownParseContext<'_>,
    cx: &mut NodeContext,
    tight: bool,
) -> BlockNode {
    let span = span(node, parse_cx);

    let value = node.data.borrow().value.clone();
    match value {
        NodeValue::Paragraph => BlockNode::Paragraph(inline_block(node, parse_cx, cx)),
        NodeValue::Alert(alert) => {
            let name = match alert.alert_type {
                AlertType::Note => "note",
                AlertType::Tip => "tip",
                AlertType::Important => "important",
                AlertType::Warning => "warning",
                AlertType::Caution => "caution",
            };
            let open = match &alert.title {
                Some(title) => format!("[!{}] {title}", name.to_ascii_uppercase()),
                None => format!("[!{}]", name.to_ascii_uppercase()),
            };
            callout(
                name,
                alert.title.as_deref(),
                open,
                false,
                node,
                parse_cx,
                cx,
                span,
            )
        }
        NodeValue::BlockDirective(directive) => {
            let (name, title) = split_info(&directive.info);
            callout(
                name.unwrap_or_default(),
                title,
                format!(":::{}", directive.info.trim()),
                true,
                node,
                parse_cx,
                cx,
                span,
            )
        }
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => BlockNode::Blockquote {
            children: children(node, parse_cx, cx, tight),
            span,
        },
        NodeValue::List(list) => BlockNode::List {
            ordered: list.list_type == ListType::Ordered,
            children: children(node, parse_cx, cx, list.tight),
            span,
        },
        NodeValue::Item(_) => BlockNode::ListItem {
            children: children(node, parse_cx, cx, tight),
            spread: !tight,
            checked: None,
            span,
        },
        NodeValue::TaskItem(task) => BlockNode::ListItem {
            children: children(node, parse_cx, cx, tight),
            spread: !tight,
            checked: Some(task.symbol.is_some()),
            span,
        },
        NodeValue::CodeBlock(code) => {
            BlockNode::CodeBlock(code_block(&code.info, code.literal, span))
        }
        NodeValue::Heading(heading) => BlockNode::Heading {
            level: heading.level,
            children: inline_block(node, parse_cx, cx),
            span,
        },
        NodeValue::HtmlBlock(html) => {
            let mut paragraph =
                Paragraph::new(html.literal.trim_end_matches(['\n', '\r']).to_owned());
            paragraph.span = span;
            BlockNode::Paragraph(paragraph)
        }
        NodeValue::FrontMatter(front) => {
            BlockNode::CodeBlock(CodeBlock::new(front.into(), Some("yml".into()), span))
        }
        NodeValue::ThematicBreak => BlockNode::HorizontalRule { span },
        NodeValue::Table(table) => {
            let mut parsed = Table::default();
            parsed.column_aligns = table
                .alignments
                .iter()
                .map(|align| (*align).into())
                .collect();
            for row in node.children() {
                if matches!(row.data.borrow().value, NodeValue::TableRow(_)) {
                    parse_table_row(&mut parsed, row, parse_cx, cx);
                }
            }
            parsed.span = span;
            BlockNode::Table(parsed)
        }
        NodeValue::FootnoteDefinition(def) => {
            let mut paragraph = Paragraph::default();
            for child in node.children() {
                parse_paragraph(&mut paragraph, child, parse_cx, cx);
            }
            paragraph.span = span;
            BlockNode::Footnote {
                number: footnote_number(node, &def.name),
                name: def.name.into(),
                children: paragraph,
                span,
            }
        }
        value => {
            if cfg!(debug_assertions) {
                log::warn!("unsupported node: {value:?}");
            }
            BlockNode::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ala::ParentElement;

    use crate::components::text::{MarkdownExtensions, MarkdownNode, MarkdownPlugin};

    #[test]
    fn hard_break_renders_newline() {
        for source in [
            "Owner: Jane  \nPersona: assistant",
            "Owner: Jane\\\nPersona: assistant",
        ] {
            let document = parse(source, &mut NodeContext::default()).unwrap();
            let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
                panic!("expected paragraph");
            };
            let texts = paragraph
                .children
                .iter()
                .map(|child| child.text.as_ref())
                .collect::<Vec<_>>();
            assert_eq!(
                texts,
                ["Owner: Jane", "\n", "Persona: assistant"],
                "source: {source:?}"
            );
        }
    }

    #[test]
    fn table_cells_carry_their_own_spans() {
        let source = "Intro\n\n| a | b |\n|---|---|\n| c | d |\n";
        let document = parse(source, &mut NodeContext::default()).unwrap();
        let BlockNode::Table(table) = &document.blocks[1] else {
            panic!("expected table");
        };
        let cells = table
            .children
            .iter()
            .flat_map(|row| &row.children)
            .map(|cell| {
                let span = cell.children.span.expect("cell span");
                &source[span.start..span.end]
            })
            .collect::<Vec<_>>();
        assert_eq!(cells, [" a ", " b ", " c ", " d "]);
    }

    #[test]
    fn footnotes_number_by_first_reference() {
        let document = parse(
            "One[^b] two[^a].\n\n[^a]: First.\n\n[^b]: Second.\n",
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
            panic!("expected paragraph");
        };
        let texts = paragraph
            .children
            .iter()
            .map(|child| child.text.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["One", "¹", " two", "²", "."]);
        let notes = document.blocks[1..]
            .iter()
            .map(|block| match block {
                BlockNode::Footnote {
                    name,
                    number,
                    children,
                    ..
                } => (name.to_string(), *number, children.text()),
                other => panic!("expected footnote, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            notes,
            [
                ("a".to_owned(), Some(2), "First.".to_owned()),
                ("b".to_owned(), Some(1), "Second.".to_owned()),
            ]
        );
    }

    #[test]
    fn inline_br_tags_render_as_line_breaks() {
        let document = parse(
            "one<br>two<br/>three<BR />four <b>bold</b>",
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
            panic!("expected paragraph");
        };
        let texts = paragraph
            .children
            .iter()
            .map(|child| child.text.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                "one", "\n", "two", "\n", "three", "\n", "four ", "<b>", "bold", "</b>"
            ]
        );
    }

    #[test]
    fn soft_break_reflows_to_space() {
        for ending in ["\n", "\r\n", "\r"] {
            let source = format!("this sentence{ending}continues as a soft wrap");
            let document = parse(&source, &mut NodeContext::default()).unwrap();
            let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
                panic!("expected paragraph");
            };
            assert_eq!(paragraph.children.len(), 1, "source: {source:?}");
            assert_eq!(
                paragraph.children[0].text.as_ref(),
                "this sentence continues as a soft wrap",
                "source: {source:?}"
            );
        }
    }

    #[test]
    fn soft_break_reflows_while_hard_break_and_marks_survive() {
        let document = parse("**a\r\nb**  \nc", &mut NodeContext::default()).unwrap();
        let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
            panic!("expected paragraph");
        };
        let texts = paragraph
            .children
            .iter()
            .map(|child| child.text.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["a b", "\n", "c"]);
        assert_eq!(paragraph.children[0].marks.len(), 1);
        assert_eq!(paragraph.children[0].marks[0].0, 0..3);
        assert!(paragraph.children[0].marks[0].1.bold);
    }

    #[test]
    fn test_nested_emphasis_merges_text_marks() {
        let mut cx = NodeContext::default();
        let document = parse("This has **_bold and italic_** text.", &mut cx).unwrap();

        let BlockNode::Paragraph(paragraph) = &document.blocks[0] else {
            panic!("expected paragraph");
        };

        let bold_italic = paragraph
            .children
            .iter()
            .find(|child| child.text.as_ref() == "bold and italic")
            .expect("expected emphasized text");

        assert!(
            bold_italic
                .marks
                .iter()
                .any(|(_, mark)| mark.bold && mark.italic),
            "nested emphasis should produce a bold and italic mark"
        );
    }

    #[test]
    fn table_rows_use_header_column_count() {
        let document = parse(
            "| A | B | C |\n| --- | --- | --- |\n| one |\n| two | three | four | ignored |\n",
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Table(table) = &document.blocks[0] else {
            panic!("expected table");
        };
        assert_eq!(table.column_aligns.len(), 3);
        assert!(table.children.iter().all(|row| row.children.len() == 3));
        assert_eq!(table.children[1].children[0].children.text(), "one");
        assert!(table.children[1].children[1].children.text().is_empty());
        assert_eq!(table.children[2].children[2].children.text(), "four");
    }

    #[test]
    fn a_pipe_inside_inline_code_does_not_add_a_column() {
        let document = parse(
            "| Command | Effect |\n| --- | --- |\n| `a | b` | pipes |\n",
            &mut NodeContext::default(),
        )
        .unwrap();
        let BlockNode::Table(table) = &document.blocks[0] else {
            panic!("expected table");
        };
        assert!(table.children.iter().all(|row| row.children.len() == 2));
    }

    #[test]
    fn raw_html_blocks_keep_distinct_source_spans() {
        let mut cx = NodeContext::default();
        let document = parse("<div>one</div>\n\n<div>two</div>", &mut cx).unwrap();
        let spans = document
            .blocks
            .iter()
            .map(|block| block.span().expect("html span").start)
            .collect::<Vec<_>>();

        assert_eq!(spans, vec![0, 16]);
    }

    #[test]
    fn alerts_and_directives_become_callouts() {
        let source = "> [!WARNING]\n> Mind the gap\n\n:::note Heads up\n- one\n- two\n:::\n";
        let document = parse(source, &mut NodeContext::default()).unwrap();
        let callouts = document
            .blocks
            .iter()
            .map(|block| match block {
                BlockNode::Callout {
                    tone,
                    title,
                    directive,
                    children,
                    span,
                    ..
                } => (
                    *tone,
                    title.to_string(),
                    *directive,
                    children.len(),
                    span.map(|span| span.start),
                ),
                other => panic!("expected a callout, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            callouts,
            [
                (Tone::Warning, "Warning".to_owned(), false, 1, Some(0)),
                (Tone::Accent, "Heads up".to_owned(), true, 1, Some(29)),
            ]
        );
        assert_eq!(
            document.blocks[1].to_markdown(),
            ":::note Heads up\n- one\n- two\n:::"
        );
    }

    #[test]
    fn inline_links_mark_prose_and_whole_code_spans_but_not_fences() {
        let extensions = MarkdownExtensions::default().inline_links(|text, source| {
            let token = "@pane";
            let range = match source {
                InlineSource::Text => text.find(token).map(|at| at..at + token.len()),
                InlineSource::Code => (text == token).then(|| 0..text.len()),
            };
            range
                .map(|range| super::super::InlineLink {
                    range,
                    label: "Pane 1".to_owned(),
                    url: "ref:pane".to_owned(),
                })
                .into_iter()
                .collect()
        });
        let mut cx = NodeContext {
            markdown_extensions: extensions.into(),
            ..NodeContext::default()
        };
        let document = parse(
            "é\r\n\r\nsee @pane, then\r\n`@pane`\n\n```\n@pane\n```\n",
            &mut cx,
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &document.blocks[1] else {
            panic!("expected paragraph");
        };
        assert_eq!(paragraph.span.map(|span| span.start), Some(6));
        let pieces = paragraph
            .children
            .iter()
            .map(|child| {
                let url = child
                    .marks
                    .iter()
                    .find_map(|(_, mark)| mark.link.as_ref().map(|link| link.url.to_string()));
                (child.text.to_string(), url)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            pieces,
            [
                ("see ".to_owned(), None),
                ("Pane 1".to_owned(), Some("ref:pane".to_owned())),
                (", then ".to_owned(), None),
                ("Pane 1".to_owned(), Some("ref:pane".to_owned())),
            ]
        );
        let BlockNode::CodeBlock(code) = &document.blocks[2] else {
            panic!("expected code block");
        };
        assert_eq!(code.code().as_ref(), "@pane");
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Ticker {
        symbol: String,
    }

    fn parse_ticker_block(
        node: &AstNode<'_>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let text = MarkdownParseContext::sole_text(node)?;
        let symbol = text.strip_prefix('$')?.to_string();
        let node_text = format!("${symbol}");

        Some(
            MarkdownNode::new("ticker", Ticker { symbol })
                .text(node_text)
                .markdown(cx.node_source(node).unwrap_or_default()),
        )
    }

    #[test]
    fn custom_block_parser_converts_ticker_syntax_to_custom_node() {
        let extensions = MarkdownExtensions::default().block_parser(parse_ticker_block);

        let mut cx = NodeContext {
            markdown_extensions: extensions.into(),
            ..NodeContext::default()
        };
        let document = parse("$TSLA.US", &mut cx).unwrap();

        let BlockNode::Custom { node, .. } = &document.blocks[0] else {
            panic!("expected custom markdown node");
        };
        assert_eq!(node.name(), "ticker");
        assert_eq!(node.as_text(), "$TSLA.US");
        assert_eq!(node.as_markdown(), "$TSLA.US");
        assert_eq!(
            node.data::<Ticker>(),
            Some(&Ticker {
                symbol: "TSLA.US".to_string()
            })
        );
        assert_eq!(document.text(), "$TSLA.US\n");
        assert_eq!(document.to_markdown(), "$TSLA.US");
    }

    struct TickerPlugin {
        name: &'static str,
    }

    impl TickerPlugin {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    impl MarkdownPlugin for TickerPlugin {
        fn name(&self) -> &str {
            self.name
        }

        fn parse<'a>(
            &self,
            node: &'a AstNode<'a>,
            cx: &MarkdownParseContext<'_>,
        ) -> Option<MarkdownNode> {
            parse_ticker_block(node, cx)
        }

        fn render(
            &self,
            node: &MarkdownNode,
            _window: &mut ala::Window,
            _cx: &mut ala::App,
        ) -> impl ala::IntoElement {
            ala::div().child(node.as_text().to_string())
        }
    }

    #[test]
    fn custom_block_plugin_registers_parser_and_renderer() {
        let extensions = MarkdownExtensions::default().plugin(TickerPlugin::new("ticker"));

        let mut cx = NodeContext {
            markdown_extensions: extensions.into(),
            ..NodeContext::default()
        };
        let document = parse("$TSLA.US", &mut cx).unwrap();

        let BlockNode::Custom { node, .. } = &document.blocks[0] else {
            panic!("expected custom markdown node");
        };
        assert_eq!(node.name(), "ticker");
        assert_eq!(
            node.data::<Ticker>(),
            Some(&Ticker {
                symbol: "TSLA.US".to_string()
            })
        );
    }

    #[test]
    fn a_block_that_absorbs_its_neighbors_spans_them_and_hides_them() {
        let extensions = MarkdownExtensions::default().block_parser(|node, cx| {
            cx.code_fence(node)?;
            let mut run = 0;
            let mut next = node.next_sibling();
            while let Some(sibling) = next.filter(|sibling| cx.code_fence(sibling).is_some()) {
                run += 1;
                next = sibling.next_sibling();
            }
            Some(MarkdownNode::new("run", run).absorbs(run))
        });
        let mut cx = NodeContext {
            markdown_extensions: extensions.into(),
            ..NodeContext::default()
        };
        let source = "```a\n1\n```\n```b\n2\n```\n\n~~~c\n3\n~~~\n\nafter";
        let document = parse(source, &mut cx).unwrap();

        assert_eq!(document.blocks.len(), 2);
        let BlockNode::Custom { node, .. } = &document.blocks[0] else {
            panic!("expected the run");
        };
        assert_eq!(node.data::<usize>(), Some(&2));
        assert_eq!(node.span, Some(Span { start: 0, end: 33 }));
        assert!(matches!(document.blocks[1], BlockNode::Paragraph(_)));
    }

    struct InlineTickerPlugin(TickerPlugin);

    impl MarkdownPlugin for InlineTickerPlugin {
        fn is_block(&self) -> bool {
            false
        }

        fn name(&self) -> &str {
            self.0.name()
        }

        fn parse<'a>(
            &self,
            node: &'a AstNode<'a>,
            cx: &MarkdownParseContext<'_>,
        ) -> Option<MarkdownNode> {
            self.0.parse(node, cx)
        }

        fn render(
            &self,
            node: &MarkdownNode,
            window: &mut ala::Window,
            cx: &mut ala::App,
        ) -> impl ala::IntoElement {
            self.0.render(node, window, cx)
        }
    }

    #[test]
    fn inline_plugin_is_skipped_instead_of_panicking() {
        let extensions =
            MarkdownExtensions::default().plugin(InlineTickerPlugin(TickerPlugin::new("ticker")));

        let mut cx = NodeContext {
            markdown_extensions: extensions.into(),
            ..NodeContext::default()
        };
        let document = parse("$TSLA.US", &mut cx).unwrap();

        assert!(matches!(&document.blocks[0], BlockNode::Paragraph(_)));
    }
}
