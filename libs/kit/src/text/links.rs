use std::sync::Arc;

use zz_gpui::{
    AnyView, App, HighlightStyle, Modifiers, MouseButton, SharedString, UnderlineStyle, Window, px,
};

use crate::ActiveTheme as _;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkClick {
    pub url: SharedString,
    pub button: MouseButton,
    pub modifiers: Modifiers,
}

pub trait TextLinks: Send + Sync + 'static {
    fn style(&self, _url: &str, _cx: &App) -> Option<HighlightStyle> {
        None
    }

    fn tooltip(&self, _url: &str, _window: &mut Window, _cx: &mut App) -> Option<AnyView> {
        None
    }

    fn click(&self, _click: &LinkClick, _window: &mut Window, _cx: &mut App) -> bool {
        false
    }
}

pub(crate) type SharedLinks = Arc<dyn TextLinks>;

pub(crate) fn link_highlight(links: Option<&SharedLinks>, url: &str, cx: &App) -> HighlightStyle {
    let plain = HighlightStyle {
        color: Some(cx.theme().foreground),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            ..UnderlineStyle::default()
        }),
        ..HighlightStyle::default()
    };
    links
        .and_then(|links| links.style(url, cx))
        .unwrap_or(plain)
}

pub(crate) fn open_link(
    links: Option<&SharedLinks>,
    click: &LinkClick,
    window: &mut Window,
    cx: &mut App,
) {
    if links.is_some_and(|links| links.click(click, window, cx)) {
        return;
    }
    if is_openable(&click.url) {
        cx.open_url(&click.url);
    }
}

pub(crate) fn is_openable(url: &str) -> bool {
    let scheme = url.split_once(':').map_or("", |(scheme, _)| scheme);
    !(scheme.eq_ignore_ascii_case("data") || scheme.eq_ignore_ascii_case("javascript"))
}
