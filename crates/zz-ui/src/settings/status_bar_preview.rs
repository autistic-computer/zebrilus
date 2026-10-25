use std::rc::Rc;

pub use zz_client::StatusBarSettings;
use zz_gpui::{App, IntoElement, div, prelude::*, px};

use crate::{
    ActiveTheme as _, Colorize as _, IconName, StyledExt as _,
    navigation::{
        WorkspaceStatusWindowState,
        status::{StatusPaneEntry, StatusWindowActions, status_window},
        status_dot, workspace_status_item,
    },
    shell::{WorkspaceStatusSlots, workspace_status_bar},
};

use super::{SettingsSection, settings_group_header, settings_page_title, settings_scroll_column};

pub fn status_bar_page(
    settings: StatusBarSettings,
    gaps: bool,
    controls: impl IntoElement,
    cx: &App,
) -> zz_gpui::Div {
    div()
        .flex()
        .flex_col()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(
            settings_scroll_column("settings-status-bar")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child(settings_page_title(SettingsSection::StatusBar, cx))
                        .child(settings_group_header("Preview".into(), cx))
                        .child(status_bar_preview(settings, gaps, cx)),
                )
                .child(controls),
        )
}

fn status_bar_preview(settings: StatusBarSettings, gaps: bool, cx: &App) -> zz_gpui::Div {
    let windows = ["workspace", "logs"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            status_window(
                ("settings-preview-window", index),
                name.into(),
                WorkspaceStatusWindowState {
                    connected: true,
                    active: index == 0,
                    bell: settings.badges && index == 1,
                    activity: settings.badges && index == 1,
                },
                if index == 0 {
                    [
                        (IconName::SquareTerminal, "cargo test"),
                        (IconName::Globe, "GPUI docs"),
                        (
                            crate::pane::agent_provider_icon(zz_protocol::AgentProvider::Codex),
                            "Rework status bar",
                        ),
                        (IconName::Claude, "Review pane titles"),
                        (IconName::File, "status_bar.rs"),
                        (IconName::SquareTerminal, "dev server"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (icon, label))| StatusPaneEntry {
                        label: label.into(),
                        detail: "Sample pane".into(),
                        icon,
                        favicon: None,
                        active: index == 1,
                        select: Rc::new(|_, _| {}),
                    })
                    .collect()
                } else {
                    vec![StatusPaneEntry {
                        label: "daemon logs".into(),
                        detail: "Terminal".into(),
                        icon: IconName::SquareTerminal,
                        favicon: None,
                        active: true,
                        select: Rc::new(|_, _| {}),
                    }]
                },
                StatusWindowActions {
                    select: Rc::new(|_, _| {}),
                    close: None,
                    rename: None,
                },
                cx,
            )
        })
        .collect();
    let mut right = Vec::new();
    if settings.show_host {
        right.push(
            workspace_status_item(
                "settings-preview-host",
                Some(IconName::Globe),
                "devbox".into(),
                cx,
            )
            .into_any_element(),
        );
    }
    if settings.show_update {
        right.push(
            workspace_status_item("settings-preview-update", None, "v0.9.0".into(), cx)
                .flex_none()
                .px(px(6.0))
                .surface(crate::roles::STATUS_CHIP, cx)
                .text_color(cx.theme().foreground.muted())
                .child(status_dot(cx.theme().success))
                .into_any_element(),
        );
    }
    super::settings_preview(cx.theme().background.opaque(), cx)
        .relative()
        .w_full()
        .flex_none()
        .child(
            workspace_status_bar(
                gaps,
                px(0.0),
                WorkspaceStatusSlots {
                    windows,
                    right,
                    ..Default::default()
                },
                cx,
            )
            .id("settings-status-bar-preview"),
        )
}
