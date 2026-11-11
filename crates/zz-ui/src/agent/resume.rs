use std::rc::Rc;

use ala::{
    AnyElement, App, Entity, IntoElement, ScrollHandle, SharedString, Window, div, prelude::*, px,
};
use penna::{
    Label, Row, RowLook, TextStyle,
    components::{
        button::{Button, ButtonLook},
        input::{Input, InputState},
        spinner::Spinner,
    },
};
use zz_client::agent_resume::{LOADING, ResumeRow};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex,
    kbd::Kbd, muted_on, row_state, tooltip::Tooltip,
};

pub const MENU_WIDTH: f32 = 300.0;
pub const RESUME: &str = "Resume";
const MENU_INSET: f32 = 6.0;
const MENU_ROWS: f32 = 8.0;
const ROW_HEIGHT: f32 = 32.0;
const SEARCH_HEIGHT: f32 = 30.0;
const LABEL_HEIGHT: f32 = 24.0;
const BUTTON_HEIGHT: f32 = 30.0;
const ICON_SIZE: f32 = 14.0;
const CHEVRON_SIZE: f32 = 10.0;
const CHECK_SIZE: f32 = 12.0;
const AGE_WIDTH: f32 = 26.0;

pub fn folder_button(id: impl Into<ala::ElementId>, name: &str, cx: &App) -> Button {
    menu_button(id, IconName::Folder, name.to_owned(), cx)
}

pub fn resume_button(id: impl Into<ala::ElementId>, cx: &App) -> Button {
    menu_button(id, IconName::History, RESUME.to_owned(), cx).look(ButtonLook::Quiet)
}

fn menu_button(id: impl Into<ala::ElementId>, icon: IconName, text: String, cx: &App) -> Button {
    let muted = cx.theme().foreground.muted();
    Button::new(id)
        .h(px(BUTTON_HEIGHT))
        .pl(px(9.0))
        .pr(px(10.0))
        .gap(px(8.0))
        .child(Icon::new(icon).with_size(px(ICON_SIZE)).text_color(muted))
        .child(text)
        .child(
            Icon::new(IconName::ChevronDown)
                .with_size(px(CHEVRON_SIZE))
                .text_color(muted),
        )
}

pub fn resume_menu(
    search: &Entity<InputState>,
    scroll: &ScrollHandle,
    items: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    crate::v_flex()
        .w(px(MENU_WIDTH))
        .p(px(MENU_INSET))
        .child(
            Input::new(search)
                .cleanable(true)
                .h(px(SEARCH_HEIGHT))
                .mb(px(4.0))
                .prefix(
                    Icon::new(IconName::Search)
                        .with_size(px(ICON_SIZE))
                        .text_color(cx.theme().foreground.muted()),
                ),
        )
        .child(
            crate::v_flex()
                .id("agent-menu-rows")
                .flex_none()
                .max_h(px(ROW_HEIGHT * MENU_ROWS))
                .overflow_y_scroll()
                .track_scroll(scroll)
                .children(items),
        )
        .into_any_element()
}

#[derive(Default)]
struct Line {
    icon: Option<IconName>,
    title: String,
    path: Option<String>,
    place: Option<String>,
    age: Option<String>,
    current: bool,
    tooltip: Option<String>,
}

pub fn resume_rows(
    pane: u64,
    rows: &[ResumeRow],
    selected: Option<usize>,
    searching: bool,
    on_pick: impl Fn(usize, &mut Window, &mut App) + 'static,
    on_hover: impl Fn(usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Vec<AnyElement> {
    let on_pick = Rc::new(on_pick);
    let on_hover: Rc<dyn Fn(usize, &mut Window, &mut App)> = Rc::new(on_hover);
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            let line = match row {
                ResumeRow::Label(text) => return label(text.clone(), cx),
                ResumeRow::Note(text) => return note(text, cx),
                ResumeRow::Loading => return loading(pane, cx),
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
                ResumeRow::Folder {
                    cwd,
                    name,
                    path,
                    age,
                    current,
                } => Line {
                    icon: Some(IconName::Folder),
                    title: name.clone(),
                    path: Some(path.clone()).filter(|path| !path.is_empty()),
                    age: age.clone(),
                    current: *current,
                    tooltip: Some(cwd.to_string_lossy().into_owned()),
                    ..Line::default()
                },
            };
            let pick = on_pick.clone();
            let hover = on_hover.clone();
            let picked = selected == Some(index);
            item(
                (pane, index),
                line,
                picked,
                picked && searching,
                move |window, cx| pick(index, window, cx),
                move |window, cx| hover(index, window, cx),
                cx,
            )
            .into_any_element()
        })
        .collect()
}

fn label(text: String, cx: &App) -> AnyElement {
    Label::new(TextStyle::Caption, text)
        .strong()
        .flex_none()
        .h(px(LABEL_HEIGHT))
        .px(px(10.0))
        .pt(px(4.0))
        .text_color(cx.theme().foreground.muted())
        .into_any_element()
}

fn note(text: &'static str, cx: &App) -> AnyElement {
    Label::new(TextStyle::Small, text)
        .flex_none()
        .px(px(10.0))
        .py(px(7.0))
        .text_color(cx.theme().foreground.muted())
        .into_any_element()
}

fn loading(pane: u64, cx: &App) -> AnyElement {
    let muted = cx.theme().foreground.muted();
    h_flex()
        .flex_none()
        .h(px(ROW_HEIGHT))
        .w_full()
        .gap(px(8.0))
        .px(px(10.0))
        .child(
            Spinner::new()
                .id(("agent-menu-loading", pane))
                .with_size(px(ICON_SIZE))
                .color(muted),
        )
        .child(Label::new(TextStyle::Small, LOADING).text_color(muted))
        .into_any_element()
}

fn item(
    id: (u64, usize),
    line: Line,
    selected: bool,
    enter: bool,
    click: impl Fn(&mut Window, &mut App) + 'static,
    hover: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> Row {
    let state = row_state(selected, true);
    let ground = cx.theme().ground_at(RowLook::Menu, state);
    let note = |text: String| Label::new(TextStyle::Caption, text).muted().on(ground);
    let row = Row::new(
        ala::ElementId::NamedInteger(format!("agent-menu-{}", id.0).into(), id.1 as u64),
        RowLook::Menu,
    )
    .state(state)
    .row_height(px(ROW_HEIGHT))
    .h_flex()
    .flex_none()
    .h(px(ROW_HEIGHT))
    .w_full()
    .gap(px(8.0))
    .px(px(10.0))
    .click_cursor()
    .on_click(move |_, window, cx| click(window, cx))
    .on_mouse_move(move |_, window, cx| hover(window, cx))
    .children(line.icon.map(|icon| {
        Icon::new(icon)
            .with_size(px(ICON_SIZE))
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
                        if line.path.is_some() {
                            title.flex_none()
                        } else {
                            title.flex_1().min_w_0()
                        }
                    }),
            )
            .children(
                line.path
                    .map(|path| note(path).truncate().flex_1().min_w_0()),
            ),
    )
    .children(line.place.map(|place| note(place).flex_none()))
    .map(|row| {
        if line.current {
            row.child(Icon::new(IconName::Check).with_size(px(CHECK_SIZE)))
        } else {
            row.children(line.age.map(|age| {
                div()
                    .flex()
                    .flex_none()
                    .justify_end()
                    .w(px(AGE_WIDTH))
                    .child(note(age))
            }))
        }
    })
    .children(
        enter.then(|| Kbd::new(ala::Keystroke::parse("enter").expect("static key")).lowercase()),
    );
    match line.tooltip.map(SharedString::from) {
        Some(tooltip) => {
            row.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
        }
        None => row,
    }
}
