use std::sync::Arc;

use ala::{
    AnyElement, App, Div, ImageSource, IntoElement, ObjectFit, ParentElement as _, SharedString,
    Styled as _, Window, div, img, prelude::*, px,
};
use penna::{Card, CardLook, Label, TextStyle};
use zz_client::program::{CodeLanguage, code_language};
use zz_protocol::{CommandInvocation, PaneId, ViewContent};

use super::{FIGURE_MAX_SOURCE_BYTES, FigureImage, figure_image};
use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Icon, IconName, StyledExt as _, Tone,
    pane::{PipPart, PipSource, pip_part},
    text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast},
};

pub(crate) const PREVIEW_NODE_NAME: &str = "zz-preview";
pub const CODE_CARD_MIN_LINES: usize = 6;
const THUMB_WIDTH: f32 = 208.0;
const THUMB_HEIGHT: f32 = 120.0;
const TILE: f32 = 28.0;
const PREVIEW_GROUP: &str = "agent-preview";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewRequest {
    pub title: String,
    pub content: ViewContent,
}

pub type ViewOpener = Arc<dyn Fn(ViewRequest, &mut Window, &mut App) + Send + Sync>;

impl ViewRequest {
    #[must_use]
    pub fn command(&self, origin: PaneId, rows: Option<u16>) -> CommandInvocation {
        let mut args = vec![
            "-t".to_owned(),
            origin.to_string(),
            "-T".to_owned(),
            self.title.clone(),
        ];
        if let Some(rows) = rows {
            args.push("-y".to_owned());
            args.push(rows.to_string());
        }
        let mut flag = |name: &str, value: String| {
            args.push(name.to_owned());
            args.push(value);
        };
        let text = match &self.content {
            ViewContent::Diagram { language, source } => {
                flag("-k", "diagram".to_owned());
                flag("-l", language.clone());
                Some(source.clone())
            }
            ViewContent::Code { language, text } => {
                flag("-k", "code".to_owned());
                if let Some(language) = language {
                    flag("-l", language.clone());
                }
                Some(text.clone())
            }
            ViewContent::Markdown { text } => {
                flag("-k", "markdown".to_owned());
                Some(text.clone())
            }
            ViewContent::Image { item, index } => {
                flag("-k", "image".to_owned());
                flag("-i", item.clone());
                flag("-n", index.to_string());
                None
            }
            ViewContent::Subagent { item } => {
                flag("-k", "subagent".to_owned());
                flag("-i", item.clone());
                None
            }
        };
        if let Some(text) = text {
            args.push("--".to_owned());
            args.push(text);
        }
        CommandInvocation::new("view-pane", args)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Preview {
    Diagram {
        language: String,
        source: String,
        kind: &'static str,
        offset: usize,
    },
    Code {
        language: Option<String>,
        title: Option<String>,
        text: String,
        lines: usize,
        offset: usize,
    },
    Markdown {
        title: Option<String>,
        text: String,
        lines: usize,
        offset: usize,
    },
}

impl Preview {
    pub(crate) fn request(&self) -> ViewRequest {
        match self {
            Self::Diagram {
                language,
                source,
                kind,
                ..
            } => ViewRequest {
                title: (*kind).to_owned(),
                content: ViewContent::Diagram {
                    language: language.clone(),
                    source: source.clone(),
                },
            },
            Self::Code {
                language,
                title,
                text,
                ..
            } => ViewRequest {
                title: title
                    .clone()
                    .or_else(|| language_name(language.as_deref()).map(str::to_owned))
                    .unwrap_or_else(|| "Code".to_owned()),
                content: ViewContent::Code {
                    language: language.clone(),
                    text: text.clone(),
                },
            },
            Self::Markdown { title, text, .. } => ViewRequest {
                title: title.clone().unwrap_or_else(|| "Markdown".to_owned()),
                content: ViewContent::Markdown { text: text.clone() },
            },
        }
    }

    fn offset(&self) -> usize {
        match self {
            Self::Diagram { offset, .. }
            | Self::Code { offset, .. }
            | Self::Markdown { offset, .. } => *offset,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreviewRun(pub(crate) Vec<Preview>);

pub(crate) struct PreviewPlugin {
    pub(crate) open: ViewOpener,
}

impl MarkdownPlugin for PreviewPlugin {
    fn name(&self) -> &str {
        PREVIEW_NODE_NAME
    }

    fn parse<'a>(
        &self,
        node: &'a markdown_ast::AstNode<'a>,
        cx: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let mut run = vec![preview(node, cx)?];
        let mut last = node;
        while let Some(next) = last.next_sibling() {
            let Some(found) = preview(next, cx) else {
                break;
            };
            run.push(found);
            last = next;
        }
        let start = cx.node_range(node)?.start;
        let end = cx.node_range(last)?.end;
        let text = run
            .iter()
            .map(|preview| match preview {
                Preview::Diagram { source, .. } => source.as_str(),
                Preview::Code { text, .. } | Preview::Markdown { text, .. } => text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let absorbs = run.len() - 1;
        Some(
            MarkdownNode::new(PREVIEW_NODE_NAME, PreviewRun(run))
                .text(text)
                .markdown(cx.source().get(start..end).unwrap_or_default())
                .absorbs(absorbs),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let run = node.data::<PreviewRun>().expect("preview run node data");
        div()
            .w_full()
            .flex()
            .flex_wrap()
            .gap_2()
            .items_start()
            .children(
                run.0
                    .iter()
                    .map(|preview| preview_card(preview, &self.open, window, cx)),
            )
    }
}

pub(crate) fn preview(
    node: &markdown_ast::AstNode<'_>,
    cx: &MarkdownParseContext<'_>,
) -> Option<Preview> {
    let fence = cx.code_fence(node)?;
    if fence.value.len() > zz_protocol::MAX_VIEW_TEXT_BYTES {
        return None;
    }
    let (language, path) = match fence.lang.as_deref().map(str::trim) {
        Some(lang) if !lang.is_empty() => match lang.split_once(':') {
            Some((lang, path)) if !path.is_empty() => {
                (Some(lang.to_owned()), Some(path.to_owned()))
            }
            _ => (Some(lang.to_owned()), None),
        },
        _ => (None, None),
    };
    let offset = fence.start;
    if let Some(kind) = language
        .as_deref()
        .and_then(|language| diagram_kind(language, &fence.value))
    {
        return Some(Preview::Diagram {
            language: language.unwrap_or_default().to_ascii_lowercase(),
            source: fence.value,
            kind,
            offset,
        });
    }
    let lines = fence.value.lines().count();
    if lines < CODE_CARD_MIN_LINES {
        return None;
    }
    let meta = fence.meta.as_deref().unwrap_or_default();
    let is = |name: &str| {
        language
            .as_deref()
            .is_some_and(|language| language.eq_ignore_ascii_case(name))
    };
    if is("html")
        && meta
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("preview"))
    {
        return None;
    }
    let title = path.or_else(|| meta_path(meta));
    if is("markdown") || is("md") {
        return Some(Preview::Markdown {
            title: title.or_else(|| first_heading(&fence.value)),
            text: fence.value,
            lines,
            offset,
        });
    }
    Some(Preview::Code {
        language,
        title,
        text: fence.value,
        lines,
        offset,
    })
}

fn diagram_kind(language: &str, source: &str) -> Option<&'static str> {
    if source.len() > FIGURE_MAX_SOURCE_BYTES {
        return None;
    }
    if language.eq_ignore_ascii_case("svg") {
        return Some("SVG");
    }
    if !language.eq_ignore_ascii_case("mermaid") || !pele::supports(source) {
        return None;
    }
    let found = pele::detect_type(source).ok().flatten()?;
    Some(match found.diagram_type {
        "flowchart" => "Flowchart",
        "sequence" => "Sequence diagram",
        "pie" => "Pie chart",
        "quadrantChart" => "Quadrant chart",
        "class" => "Class diagram",
        "state" => "State diagram",
        "er" => "ER diagram",
        "gantt" => "Gantt chart",
        _ => "Diagram",
    })
}

fn meta_path(meta: &str) -> Option<String> {
    meta.split_whitespace().find_map(|word| {
        let word = word
            .strip_prefix("title=")
            .or_else(|| word.strip_prefix("file="))
            .or_else(|| word.strip_prefix("filename="))
            .unwrap_or(word)
            .trim_matches(['"', '\'']);
        (!word.is_empty()
            && !word.contains('=')
            && !word.starts_with(['{', '#'])
            && (word.contains('/') || word.contains('.')))
        .then(|| word.to_owned())
    })
}

fn first_heading(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let heading = line.trim_start().strip_prefix('#')?;
        let heading = heading.trim_start_matches('#');
        heading
            .starts_with(' ')
            .then(|| heading.trim().to_owned())
            .filter(|heading| !heading.is_empty())
    })
}

fn language_name(language: Option<&str>) -> Option<&'static str> {
    language.and_then(code_language).map(|found| found.name)
}

pub(crate) fn code_name(language: Option<&str>) -> SharedString {
    match (language_name(language), language) {
        (Some(name), _) => SharedString::from(name),
        (None, Some(language)) => SharedString::from(language.to_owned()),
        (None, None) => SharedString::from("Code"),
    }
}

fn preview_card(
    preview: &Preview,
    open: &ViewOpener,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let request = preview.request();
    let open = Arc::clone(open);
    let source = PipSource::new(
        ("agent-preview-source", preview.offset()),
        cx.theme().radius_of(CardLook::Activity),
    );
    let source = match preview {
        Preview::Code { .. } => source.carries_content(),
        Preview::Diagram { .. } | Preview::Markdown { .. } => source,
    };
    let launch = source.clone();
    let card = Card::new(("agent-preview", preview.offset()), CardLook::Activity)
        .group(PREVIEW_GROUP)
        .relative()
        .p_0()
        .flex_none()
        .overflow_hidden()
        .cursor_pointer()
        .on_click(move |_, window, cx| {
            launch.launch(window, cx);
            open(request.clone(), window, cx);
        });
    let card = match preview {
        Preview::Diagram {
            language,
            source,
            kind,
            offset,
        } => diagram_card(card, language, source, kind, *offset, window, cx),
        Preview::Code {
            language,
            title,
            lines,
            ..
        } => {
            let found = language.as_deref().and_then(code_language);
            let name = code_name(language.as_deref());
            let (heading, detail) = match title {
                Some(title) => (
                    SharedString::from(title.clone()),
                    format!("{name} · {}", line_count(*lines)),
                ),
                None => (name, line_count(*lines)),
            };
            row_card(
                card,
                language_tile(found, IconName::Braces, cx),
                heading,
                detail.into(),
                cx,
            )
        }
        Preview::Markdown { title, lines, .. } => {
            let found = code_language("markdown");
            let heading = title
                .clone()
                .map_or_else(|| SharedString::from("Markdown"), SharedString::from);
            row_card(
                card,
                language_tile(found, IconName::Braces, cx),
                heading,
                format!("Markdown · {}", line_count(*lines)).into(),
                cx,
            )
        }
    };
    source.draw(card).into_any_element()
}

fn line_count(lines: usize) -> String {
    if lines == 1 {
        "1 line".to_owned()
    } else {
        format!("{lines} lines")
    }
}

fn row_card(card: Card, tile: Div, heading: SharedString, detail: SharedString, cx: &App) -> Card {
    card.h_flex()
        .min_w(px(200.0))
        .max_w(px(360.0))
        .gap_2p5()
        .py_2()
        .pl_2()
        .pr_3()
        .items_center()
        .child(pip_part(PipPart::Icon, tile))
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(pip_part(
                    PipPart::Title,
                    Label::new(TextStyle::Body, heading)
                        .truncate()
                        .text_color(cx.theme().foreground),
                ))
                .child(
                    Label::new(TextStyle::Caption, detail)
                        .truncate()
                        .text_color(cx.theme().foreground.muted()),
                ),
        )
}

pub fn language_tile(found: Option<CodeLanguage>, fallback: IconName, cx: &App) -> Div {
    let size = px(TILE);
    match found.filter(|_| crate::program::program_colors(cx)) {
        Some(found) => crate::program::program_tile(found.program, size, 0.0, cx),
        None => div()
            .flex_none()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .bg(cx.theme().paint(Tone::Neutral, Emphasis::Soft).fill)
            .child(
                found
                    .and_then(|found| {
                        crate::program::program_mark(
                            found.program,
                            px(15.0),
                            cx.theme().foreground,
                            None,
                        )
                    })
                    .unwrap_or_else(|| Icon::new(fallback).size(px(15.0)).into_any_element()),
            ),
    }
}

fn diagram_card(
    card: Card,
    language: &str,
    source: &str,
    kind: &'static str,
    offset: usize,
    window: &mut Window,
    cx: &mut App,
) -> Card {
    let muted = cx.theme().foreground.muted();
    let picture = match figure_image(language, source, offset, window, cx) {
        FigureImage::Ready(image) => pip_part(
            PipPart::Picture,
            img(ImageSource::Render(image))
                .size_full()
                .object_fit(ObjectFit::Contain),
        )
        .into_any_element(),
        FigureImage::Pending => Icon::new(IconName::Loader)
            .size(px(16.0))
            .text_color(muted)
            .into_any_element(),
        FigureImage::Failed(_) => Icon::new(IconName::TriangleAlert)
            .size(px(16.0))
            .text_color(cx.theme().warning)
            .into_any_element(),
    };
    let hover_only = !crate::touch::CoarsePointer::get(cx);
    card.v_flex()
        .w(px(THUMB_WIDTH))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .w_full()
                .h(px(THUMB_HEIGHT))
                .p_2()
                .child(picture),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_1()
                .px_2()
                .pb_1p5()
                .child(
                    Label::new(TextStyle::Caption, kind)
                        .truncate()
                        .text_color(muted),
                )
                .child(
                    div()
                        .flex_none()
                        .when(hover_only, |mark| {
                            mark.invisible()
                                .group_hover(PREVIEW_GROUP, ala::Styled::visible)
                        })
                        .child(
                            Icon::new(IconName::WindowMaximize)
                                .size(px(12.0))
                                .text_color(muted),
                        ),
                ),
        )
}
