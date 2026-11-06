use penna::{ActiveTheme as _, blocks::sidebar::controls_leading_inset_in};
use std::rc::Rc;

use ala::{AnyElement, App, Entity, Pixels, Stateful, Window};
use zz_client::{StatusBarModel, StatusBarWindow};
use zz_ui::navigation::{
    WorkspaceStatusWindowState,
    status::{
        MAX_VISIBLE_WINDOWS, StatusPaneEntry, StatusWindowActions, StatusWindowEntry,
        status_window, status_window_overflow, visible_window_range,
    },
};

use crate::mux::{
    client::MuxClient,
    hosts::HostId,
    nav::{
        MuxTreeModel, TreeNode, TreeTarget, activate_nav, kill_target_command,
        select_pane_commands, select_window_command,
    },
};

use zz_ui::shell::{WorkspaceStatusSlots, workspace_status_bar};

pub(crate) fn render_gui_status_bar(
    mux: &Entity<MuxClient>,
    titlebar_controls: Option<(AnyElement, Pixels)>,
    window_controls: Option<AnyElement>,
    _window: &mut Window,
    cx: &mut App,
) -> Stateful<ala::Div> {
    let (snapshot, attached_host, attached, connected) = {
        let mux = mux.read(cx);
        (
            mux.snapshot(),
            mux.attached_host(),
            mux.attached_session(),
            mux.is_connected(),
        )
    };
    let tree_model = MuxTreeModel::from_mux(mux.read(cx));
    let model = StatusBarModel::from_snapshot(&snapshot, attached);
    let active_index = model
        .windows
        .iter()
        .position(|window| window.active)
        .unwrap_or(0);
    let visible_range =
        visible_window_range(model.windows.len(), active_index, MAX_VISIBLE_WINDOWS);
    let visible_windows = model.windows[visible_range]
        .iter()
        .map(|window| render_status_window(window, connected, attached_host, &tree_model, mux, cx))
        .collect::<Vec<_>>();
    let overflow = (model.windows.len() > MAX_VISIBLE_WINDOWS)
        .then(|| render_window_overflow(&model.windows, connected, mux, cx));
    workspace_status_bar(
        crate::config::pane_gaps(cx),
        controls_leading_inset_in(cx.theme().title_bar_over_panes_height(), cx),
        WorkspaceStatusSlots {
            windows: visible_windows.into_iter().chain(overflow).collect(),
            titlebar_controls,
            window_controls,
        },
        cx,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_status_window(
    window: &StatusBarWindow,
    connected: bool,
    attached_host: HostId,
    model: &MuxTreeModel,
    mux: &Entity<MuxClient>,
    cx: &App,
) -> AnyElement {
    let id = window.id;
    let select_mux = mux.clone();
    let close_mux = mux.clone();
    let rename = model
        .rename_activation_for_node(
            TreeNode::Target(attached_host, TreeTarget::Window(id)),
            attached_host,
        )
        .map(|(label, activation)| {
            let mux = mux.clone();
            (
                label.into(),
                Rc::new(move |_: &mut Window, cx: &mut App| {
                    activate_nav(&mux, activation.clone(), cx);
                }) as zz_ui::navigation::status::StatusAction,
            )
        });
    status_window(
        ("gui-status-window", id.0),
        window.label.clone().into(),
        WorkspaceStatusWindowState {
            connected,
            active: window.active,
        },
        window
            .panes
            .iter()
            .map(|pane| {
                let pane_id = pane.id;
                let mux = mux.clone();
                let mut entry = StatusPaneEntry::from_pane(
                    pane,
                    Rc::new(move |_, cx| {
                        for command in select_pane_commands(pane_id) {
                            mux.read(cx).execute(command);
                        }
                    }),
                );
                if let zz_protocol::PaneKindSnapshot::Browser(browser) = &pane.kind {
                    entry.favicon =
                        crate::browser::recent_pages::favicon(&browser.profile, browser.url(), cx);
                    if let Some(title) =
                        crate::browser::recent_pages::title(&browser.profile, browser.url(), cx)
                    {
                        entry.label = title.into();
                    }
                }
                entry
            })
            .collect(),
        StatusWindowActions {
            select: Rc::new(move |_, cx| {
                select_mux.read(cx).execute(select_window_command(id));
            }),
            close: Some(Rc::new(move |_, cx| {
                close_mux
                    .read(cx)
                    .execute(kill_target_command(TreeTarget::Window(id)));
            })),
            rename,
        },
        cx,
    )
}

fn render_window_overflow(
    windows: &[StatusBarWindow],
    connected: bool,
    mux: &Entity<MuxClient>,
    cx: &App,
) -> AnyElement {
    let entries = windows
        .iter()
        .map(|window| {
            let id = window.id;
            let mux = mux.clone();
            StatusWindowEntry {
                label: format!("{} {}", window.index, window.label).into(),
                active: window.active,
                select: Rc::new(move |_, cx| {
                    mux.read(cx).execute(select_window_command(id));
                }),
            }
        })
        .collect();
    status_window_overflow("gui-status-window-overflow", entries, connected, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_windows_stay_centered_on_the_active_window() {
        assert_eq!(visible_window_range(3, 1, 5), 0..3);
        assert_eq!(visible_window_range(9, 0, 5), 0..5);
        assert_eq!(visible_window_range(9, 4, 5), 2..7);
        assert_eq!(visible_window_range(9, 8, 5), 4..9);
    }
}
