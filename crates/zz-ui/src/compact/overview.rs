use ala::{AnyElement, App, Div, ParentElement as _, SharedString, Styled as _, div, px};

use crate::{ActiveTheme as _, Colorize as _, Icon, IconName, StyledExt as _};

/// `n` of `noun`, as the overview counts panes and windows.
pub fn overview_count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
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
