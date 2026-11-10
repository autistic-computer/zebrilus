use ala::{
    AnyElement, App, ImageSource, ObjectFit, ParentElement as _, Styled as _, Window, div, img,
    prelude::*, px,
};
use penna::{Label, TextStyle, blocks::chat::transcript_text};
use zz_client::program::Program;
use zz_protocol::{
    ViewContent,
    chat::{ChatView, ItemId},
};

use super::{
    FigureImage, agent_text_view, assistant_markdown_extensions,
    chat::{ChatClicks, ChatHooks, ChatOpen, chat_agent_body},
    figure_image,
};
use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName,
    pane::{PaneKindButton, pip_picture},
    text::{MarkdownExtensions, TextView},
    v_flex,
};

pub const VIEW_SOURCE: &str = "view-source";

#[must_use]
pub fn view_buttons(content: &ViewContent, source: bool) -> Vec<PaneKindButton> {
    match content {
        ViewContent::Diagram { .. } if source => vec![PaneKindButton {
            key: VIEW_SOURCE,
            icon: IconName::ListTree,
            label: "Show the diagram",
        }],
        ViewContent::Diagram { .. } => vec![PaneKindButton {
            key: VIEW_SOURCE,
            icon: IconName::Braces,
            label: "Show the source",
        }],
        _ => Vec::new(),
    }
}

pub struct ViewChat<'a> {
    pub view: Option<&'a ChatView>,
    pub open: &'a ChatOpen,
    pub now: u64,
    pub phase: f32,
    pub click: Option<ChatClicks>,
    pub vendor: Option<&'static Program>,
    pub owner: u64,
}

pub fn view_chat_body(content: &ViewContent, chat: &ViewChat<'_>, cx: &App) -> AnyElement {
    match content {
        ViewContent::Image { item, index } => chat
            .open
            .images
            .get(&ItemId::new(item.clone()))
            .and_then(|images| images.get(usize::try_from(*index).ok()?))
            .map_or_else(
                || view_missing("This image is no longer in the chat", cx),
                |image| view_image(std::sync::Arc::clone(image)),
            ),
        ViewContent::Subagent { item } => {
            let hooks = ChatHooks {
                click: chat.click.clone(),
                vendor: chat.vendor,
                owner: Some(chat.owner),
                ..ChatHooks::default()
            };
            chat.view
                .and_then(|view| {
                    chat_agent_body(
                        view,
                        &ItemId::new(item.clone()),
                        chat.now,
                        chat.open,
                        chat.phase,
                        &hooks,
                        cx,
                    )
                })
                .map_or_else(
                    || view_missing("This subagent's chat is not open in this window", cx),
                    |(_, body)| {
                        div()
                            .w_full()
                            .px_3()
                            .pt_2()
                            .pb_3()
                            .text_color(cx.theme().foreground)
                            .child(body)
                            .into_any_element()
                    },
                )
        }
        _ => view_missing("Nothing to show", cx),
    }
}

pub fn view_body(
    content: &ViewContent,
    scope: u64,
    show_source: bool,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    match content {
        ViewContent::Diagram { language, source } if !show_source => {
            diagram_body(language, source, scope, window, cx)
        }
        ViewContent::Diagram { language, source } => {
            code_body(Some(language.as_str()), source, scope, cx)
        }
        ViewContent::Code { language, text } => code_body(language.as_deref(), text, scope, cx),
        ViewContent::Markdown { text } => transcript_text(div().w_full().px_4().py_3(), cx)
            .child(
                agent_text_view(
                    TextView::markdown(("view-markdown", scope), text.clone()),
                    scope,
                    None,
                    cx,
                )
                .selectable(true)
                .markdown_extensions(assistant_markdown_extensions()),
            )
            .into_any_element(),
        ViewContent::Image { .. } => view_missing("This image is no longer in the chat", cx),
        ViewContent::Subagent { .. } => {
            view_missing("This subagent's chat is not open in this window", cx)
        }
    }
}

pub fn view_missing(text: &'static str, cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(Label::new(TextStyle::Caption, text).text_color(cx.theme().foreground.muted()))
        .into_any_element()
}

pub fn view_image(image: std::sync::Arc<ala::Image>) -> AnyElement {
    div()
        .size_full()
        .p_4()
        .flex()
        .items_center()
        .justify_center()
        .child(pip_picture(move || {
            img(ImageSource::Image(std::sync::Arc::clone(&image)))
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::ScaleDown)
        }))
        .into_any_element()
}

fn diagram_body(
    language: &str,
    source: &str,
    scope: u64,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let key = usize::try_from(scope).unwrap_or_default();
    let muted = cx.theme().foreground.muted();
    let picture = match figure_image(language, source, key, window, cx) {
        FigureImage::Ready(image) => pip_picture(move || {
            img(ImageSource::Render(std::sync::Arc::clone(&image)))
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::Contain)
        })
        .into_any_element(),
        FigureImage::Pending => Icon::new(IconName::Loader)
            .size(px(18.0))
            .text_color(muted)
            .into_any_element(),
        FigureImage::Failed(error) => v_flex()
            .gap_1()
            .items_center()
            .child(
                Icon::new(IconName::TriangleAlert)
                    .size(px(18.0))
                    .text_color(cx.theme().warning),
            )
            .child(Label::new(TextStyle::Caption, error).text_color(muted))
            .into_any_element(),
    };
    div()
        .size_full()
        .p_4()
        .flex()
        .items_center()
        .justify_center()
        .child(picture)
        .into_any_element()
}

fn code_body(language: Option<&str>, text: &str, scope: u64, cx: &App) -> AnyElement {
    let ticks = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        .max(2)
        + 1;
    let fence = "`".repeat(ticks);
    let source = format!("{fence}{}\n{text}\n{fence}", language.unwrap_or_default());
    transcript_text(div().w_full().px_3().py_3(), cx)
        .child(
            agent_text_view(
                TextView::markdown(("view-code", scope), source),
                scope,
                None,
                cx,
            )
            .selectable(true)
            .markdown_extensions(MarkdownExtensions::default()),
        )
        .into_any_element()
}
