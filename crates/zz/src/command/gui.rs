use ala::{App, Entity};
use zz_client::gui_actions::{
    ACCENT, GLASS, GuiAction, GuiOffer, PANE_GAPS, SIDEBAR_LAYOUT, TITLEBAR_MODE,
};

use crate::{
    config::{self, ConfigKey},
    workspace::sidebar::{ChromeMode, WorkspaceSidebar},
};

pub(crate) fn offers(sidebar: &Entity<WorkspaceSidebar>, cx: &App) -> Vec<GuiOffer> {
    let sidebar = sidebar.read(cx);
    let titlebar = match sidebar.mode() {
        ChromeMode::Sidebar => "sidebar",
        ChromeMode::Titlebar => "titlebar",
    };
    [
        GuiOffer::fixed(TITLEBAR_MODE, titlebar),
        GuiOffer::toggle(PANE_GAPS, config::pane_gaps(cx)),
        GuiOffer::fixed(
            GLASS,
            if config::appearance(cx).glass() == penna::foundation::interface_theme::Glass::None {
                "none"
            } else {
                "liquid"
            },
        ),
        GuiOffer::fixed(ACCENT, &config::appearance(cx).accent()),
        GuiOffer::toggle(SIDEBAR_LAYOUT, sidebar.is_flat()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(crate) fn preview(action: &GuiAction, value: &str, cx: &mut App) {
    if action.id == ACCENT {
        config::preview_accent(value, cx);
    }
}

pub(crate) fn revert(cx: &mut App) {
    config::reload(cx);
}

pub(crate) fn apply(
    action: &GuiAction,
    value: &str,
    sidebar: &Entity<WorkspaceSidebar>,
    cx: &mut App,
) {
    match action.id {
        TITLEBAR_MODE => sidebar.update(cx, |sidebar, cx| {
            let titlebar = sidebar.mode() == ChromeMode::Titlebar;
            if titlebar != (value == "titlebar") {
                sidebar.toggle_mode(cx);
            }
        }),
        SIDEBAR_LAYOUT => sidebar.update(cx, |sidebar, cx| sidebar.set_flat(value == "true", cx)),
        _ => {
            if let Some(key) = action.config_key.and_then(ConfigKey::parse) {
                config::settings::written(key, config::set_config_key(key, value), cx);
            }
        }
    }
}
