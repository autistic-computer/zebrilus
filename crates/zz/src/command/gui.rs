use ala::{App, Entity};
use zz_client::gui_actions::{
    GuiAction, GuiOffer, PANE_GAPS, SIDEBAR_LAYOUT, THEME, TITLEBAR_MODE, WINDOW_BLUR,
};
use zz_ui::interface_theme;

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
        crate::profile::profile(cx)
            .has_window_blur
            .then(|| {
                GuiOffer::toggle(
                    WINDOW_BLUR,
                    config::resolved_config(cx).window_background_blur.value,
                )
            })
            .flatten(),
        config::settings::with_theme_picks(cx, zz_ui::command::theme_offer),
        GuiOffer::toggle(SIDEBAR_LAYOUT, sidebar.is_flat()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn theme_sets_font(id: &str, cx: &App) -> bool {
    interface_theme::builtin_theme(id)
        .or_else(|| {
            config::themes(cx)
                .saved
                .iter()
                .find(|(saved, _)| saved.as_ref() == id)
                .map(|(_, theme)| theme)
        })
        .is_some_and(|theme| theme.materials.font.is_some())
}

pub(crate) fn preview(action: &GuiAction, value: &str, cx: &mut App) {
    if action.id == THEME {
        config::preview_interface_theme(value, theme_sets_font(value, cx), cx);
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
        THEME => config::settings::written(
            ConfigKey::InterfaceTheme,
            config::set_interface_theme(value, theme_sets_font(value, cx)),
            cx,
        ),
        _ => {
            if let Some(key) = action.config_key.and_then(ConfigKey::parse) {
                config::settings::written(key, config::set_config_key(key, value), cx);
            }
        }
    }
}
