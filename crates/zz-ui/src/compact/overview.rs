use zz_gpui::{
    AnyElement, App, Div, ElementId, ParentElement as _, SharedString, Stateful, Styled as _, div,
    prelude::*, px,
};

use zz_gpui_kit::sheet::sheet_detail_row;

use crate::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Layer, PaintState, StyledExt as _, roles,
    touch::press_highlight,
};

pub const OVERVIEW_CARD_WIDTH: f32 = 108.0;
pub const OVERVIEW_CARD_HEIGHT: f32 = 192.0;
pub const OVERVIEW_CARD_GAP: f32 = 10.0;
const CARD_STRIP: f32 = 20.0;
const OVERVIEW_TITLE_WIDTH: f32 = 180.0;

/// `n` of `noun`, as the overview counts panes and windows.
pub fn overview_count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A pane's card in the phone overview, drawn as [`roles::PHONE_CARD`]: a
/// strip with its icon and title over `preview`, on `background`, lit when
/// it is the `current` page and dotted when it asks for `attention`.
pub fn overview_card(
    id: impl Into<ElementId>,
    press: impl Into<ElementId>,
    icon: IconName,
    title: impl Into<SharedString>,
    preview: Option<AnyElement>,
    current: bool,
    attention: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    let frame = theme.layer(
        roles::PHONE_CARD,
        if current {
            PaintState::Selected
        } else {
            PaintState::Rest
        },
    );
    let radius = frame.radius;
    div()
        .id(id)
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(OVERVIEW_CARD_WIDTH))
        .h(px(OVERVIEW_CARD_HEIGHT))
        .overflow_hidden()
        .layer(&Layer {
            glass: None,
            ..frame
        })
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(4.0))
                .h(px(CARD_STRIP))
                .px(px(6.0))
                .text_size(px(11.0))
                .text_color(muted)
                .child(Icon::new(icon).size(px(11.0)))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title.into()),
                ),
        )
        .child(div().flex_1().min_h_0().overflow_hidden().children(preview))
        .when(attention && !current, |card| {
            card.child(
                div()
                    .absolute()
                    .top(px(7.0))
                    .right(px(6.0))
                    .size(px(6.0))
                    .rounded_full()
                    .bg(theme.warning),
            )
        })
        .child(press_highlight(press, None, radius))
}

/// The line over a window's cards: its number, muted, its title, then the
/// folder it works in, muted and cut short with an ellipsis.
pub fn overview_section_heading(index: u32, title: &str, folder: Option<&str>, cx: &App) -> Div {
    let muted = cx.theme().foreground.muted();
    div()
        .flex()
        .items_baseline()
        .gap(px(8.0))
        .px(px(16.0))
        .pt(px(10.0))
        .pb(px(8.0))
        .overflow_hidden()
        .text_size(px(13.0))
        .child(div().flex_none().text_color(muted).child(index.to_string()))
        .child(
            div()
                .flex_none()
                .max_w(px(OVERVIEW_TITLE_WIDTH))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_medium()
                .child(title.to_owned()),
        )
        .children(folder.filter(|folder| !folder.is_empty()).map(|folder| {
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(muted)
                .child(format!("({folder})"))
        }))
}

/// A session's name over its windows on a host's overview page: bold, in
/// the muted ink so the windows under it lead.
pub fn overview_session_heading(title: &str, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .h(px(44.0))
        .px(px(16.0))
        .mt(px(6.0))
        .text_size(px(17.0))
        .font_bold()
        .text_color(cx.theme().foreground.muted())
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(title.to_owned()),
        )
}

/// Where a host stands, as the overview and the hosts drawer show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostState {
    Connected,
    Connecting,
    Offline,
}

/// The dot in front of a host's name: green while connected, amber while
/// connecting, faint otherwise.
pub fn host_state_dot(state: HostState, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex_none()
        .size(px(8.0))
        .rounded_full()
        .bg(match state {
            HostState::Connected => theme.success,
            HostState::Connecting => theme.warning,
            HostState::Offline => theme.foreground.faint(),
        })
}

/// A host's overview page while it has nothing to show: its name, a line
/// saying why, and the one thing to do about it.
pub fn overview_host_empty(
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    action: Option<AnyElement>,
    cx: &App,
) -> Div {
    let muted = cx.theme().foreground.muted();
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.0))
        .px(px(32.0))
        .text_center()
        .child(Icon::new(IconName::Server).size(px(32.0)).text_color(muted))
        .child(
            div()
                .mt(px(6.0))
                .text_size(px(20.0))
                .font_semibold()
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(19.0))
                .text_color(muted)
                .child(detail.into()),
        )
        .children(action.map(|action| div().mt(px(8.0)).child(action)))
}

/// A row of a phone drawer with a line under its name: what the New drawer
/// makes.
pub fn overview_choice_row(
    id: impl Into<ElementId>,
    icon: Option<Icon>,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    sheet_detail_row(id, roles::PHONE_ROW, icon, title, detail.into(), cx)
}

/// A host in the hosts drawer, the way Find My lists a device: a server on a
/// tile, its name, and under it its state dot and how many sessions it has or
/// what it is doing; checked when it is the page on screen. The caller adds
/// a button after it for a host that needs one.
pub fn overview_host_row(
    id: impl Into<ElementId>,
    state: HostState,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    current: bool,
    cx: &App,
) -> Stateful<Div> {
    sheet_detail_row(
        id,
        roles::PHONE_ROW,
        Some(Icon::new(IconName::Server)),
        title,
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(host_state_dot(state, cx))
            .child(detail.into()),
        cx,
    )
    .when(current, |row| {
        row.child(
            Icon::new(IconName::Check)
                .size(px(18.0))
                .text_color(cx.theme().accent),
        )
    })
}
