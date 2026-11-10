use std::rc::Rc;

use ala::{AnyElement, App, Entity, IntoElement, SharedString, Window, div, prelude::*, px};
use penna::{
    Label, Row, RowLook, TextStyle,
    components::input::{Input, InputState},
};
use zz_client::agent_resume::{NOTHING_MATCHES, ResumeRow};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex,
    kbd::Kbd, muted_on, row_state, tooltip::Tooltip,
};

pub const RESUME_WIDTH: f32 = 440.0;
pub const RESUME_FILTER_PLACEHOLDER: &str = "Filter chats and folders";
const AGE_WIDTH: f32 = 28.0;

#[derive(Default)]
struct Line {
    icon: Option<IconName>,
    title: String,
    detail: Option<String>,
    place: Option<String>,
    age: Option<String>,
    tooltip: Option<String>,
}

pub fn resume_filter(input: &Entity<InputState>, cx: &App) -> AnyElement {
    div()
        .flex_none()
        .w_full()
        .pb(px(4.0))
        .child(
            Input::new(input).cleanable(true).prefix(
                Icon::new(IconName::Search)
                    .size(px(14.0))
                    .text_color(cx.theme().foreground.muted()),
            ),
        )
        .into_any_element()
}

pub fn resume_rows(
    pane: u64,
    rows: &[ResumeRow],
    selected: Option<usize>,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    on_hover: impl Fn(usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Vec<AnyElement> {
    let on_pick = Rc::new(on_pick);
    let on_hover = Rc::new(on_hover);
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let pick = on_pick.clone();
            let click = move |window: &mut Window, cx: &mut App| pick(index, window, cx);
            let line = match row {
                ResumeRow::Heading { text, back: false } => return heading(text.clone(), cx),
                ResumeRow::Heading { text, back: true } => {
                    return back(pane, text.clone(), click, cx);
                }
                ResumeRow::Empty => return nothing(cx),
                ResumeRow::Chat {
                    title,
                    place,
                    age,
                    chat,
                } => Line {
                    title: title.clone(),
                    place: place.clone(),
                    age: age.clone(),
                    tooltip: Some(chat.cwd.to_string_lossy().into_owned()),
                    ..Line::default()
                },
                ResumeRow::Project {
                    name,
                    path,
                    age,
                    cwd,
                } => Line {
                    icon: Some(IconName::Folder),
                    title: name.clone(),
                    detail: Some(path.clone()).filter(|path| !path.is_empty()),
                    age: age.clone(),
                    tooltip: Some(cwd.to_string_lossy().into_owned()),
                    ..Line::default()
                },
                ResumeRow::More(more) => {
                    return item(
                        (pane, index),
                        Line {
                            title: more.label().to_owned(),
                            ..Line::default()
                        },
                        selected == Some(index),
                        click,
                        on_hover.clone(),
                        cx,
                    )
                    .text_color(cx.theme().foreground.muted())
                    .into_any_element();
                }
            };
            item(
                (pane, index),
                line,
                selected == Some(index),
                click,
                on_hover.clone(),
                cx,
            )
            .into_any_element()
        })
        .collect()
}

fn heading(text: String, cx: &App) -> AnyElement {
    crate::pane::pane_picker_label(text, cx).into_any_element()
}

fn nothing(cx: &App) -> AnyElement {
    Label::new(TextStyle::Small, NOTHING_MATCHES)
        .flex_none()
        .px(px(12.0))
        .py(px(8.0))
        .text_color(cx.theme().foreground.muted())
        .into_any_element()
}

fn back(
    pane: u64,
    text: String,
    click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let foreground = cx.theme().foreground;
    h_flex()
        .id(("agent-resume-back", pane))
        .flex_none()
        .gap(px(6.0))
        .px(px(12.0))
        .pt(px(8.0))
        .pb(px(2.0))
        .type_style(TextStyle::Caption, cx)
        .text_color(foreground.muted())
        .click_cursor()
        .hover(move |this| this.text_color(foreground))
        .on_click(move |_, window, cx| click(window, cx))
        .child(Icon::new(IconName::ChevronLeft).size(px(12.0)))
        .child(Label::new(TextStyle::Caption, text).strong())
        .child(div().flex_1())
        .child(Kbd::new(ala::Keystroke::parse("escape").expect("static key")).lowercase())
        .into_any_element()
}

fn item(
    id: (u64, usize),
    line: Line,
    selected: bool,
    click: impl Fn(&mut Window, &mut App) + 'static,
    hover: Rc<dyn Fn(usize, &mut Window, &mut App)>,
    cx: &App,
) -> Row {
    let state = row_state(selected, true);
    let ground = cx.theme().ground_at(RowLook::MenuCard, state);
    let height = crate::picker::list_row_height(cx);
    let index = id.1;
    let note = |text: String| Label::new(TextStyle::Caption, text).muted().on(ground);
    let named = line.detail.is_some();
    let row = Row::new(
        ala::ElementId::NamedInteger(format!("agent-resume-{}", id.0).into(), id.1 as u64),
        RowLook::MenuCard,
    )
    .state(state)
    .row_height(height)
    .h_flex()
    .flex_none()
    .h(height)
    .w_full()
    .gap(px(10.0))
    .px(px(12.0))
    .click_cursor()
    .on_click(move |_, window, cx| click(window, cx))
    .on_mouse_move(move |_, window, cx| hover(index, window, cx))
    .children(line.icon.map(|icon| {
        Icon::new(icon)
            .with_size(px(16.0))
            .text_color(muted_on(ground, cx))
    }))
    .child(
        h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(8.0))
            .overflow_hidden()
            .child(
                Label::new(TextStyle::Small, line.title)
                    .truncate()
                    .map(|title| {
                        if named {
                            title.flex_none()
                        } else {
                            title.flex_1().min_w_0()
                        }
                    }),
            )
            .children(
                line.detail
                    .map(|detail| note(detail).truncate().flex_1().min_w_0()),
            ),
    )
    .children(line.place.map(|place| note(place).flex_none()))
    .children(line.age.map(|age| {
        div()
            .flex()
            .flex_none()
            .justify_end()
            .w(px(AGE_WIDTH))
            .child(note(age))
    }));
    match line.tooltip.map(SharedString::from) {
        Some(tooltip) => {
            row.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        }
        None => row,
    }
}
