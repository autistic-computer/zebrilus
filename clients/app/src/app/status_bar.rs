use std::rc::Rc;

use ala::{AnyElement, App, Context, Entity, IntoElement, px};
use penna::{ActiveTheme as _, TabLook};
use zz_client::{StatusBarModel, StatusBarPane, navigation::move_window_commands};
#[cfg(any(target_os = "ios", target_os = "android"))]
use zz_protocol::PaneKindSnapshot;
use zz_ui::{
    navigation::{
        WorkspaceStatusWindowState,
        status::{
            MAX_VISIBLE_WINDOWS, StatusAction, StatusPaneEntry, StatusWindowActions,
            StatusWindowEntry, status_window, status_window_overflow, visible_window_range,
        },
        tab::{TabMove, TabReorder, TabStripId},
    },
    shell::{WorkspaceStatusSlots, workspace_status_bar},
};

use super::{AppShell, sidebar};
use crate::connection::Connection;

pub(super) fn render(
    view: &AppShell,
    window: &ala::Window,
    cx: &mut Context<AppShell>,
) -> AnyElement {
    let connection = view.connection.read(cx);
    let core = &connection.core;
    let connected = connection.connected;
    let writable = connected && !core.attached_read_only();
    let host = connection.endpoint().unwrap_or_default().to_owned();
    let model = StatusBarModel::from_snapshot(core.snapshot(), core.attached_session());
    let active = model
        .windows
        .iter()
        .position(|window| window.active)
        .unwrap_or(0);
    let visible = visible_window_range(model.windows.len(), active, MAX_VISIBLE_WINDOWS);
    let alone = model.windows.len() == 1;
    let strip = TabStripId::new(cx.entity_id(), TabLook::Window);
    let on_move = writable.then(|| {
        let windows = model.windows.clone();
        let connection = view.connection.clone();
        Rc::new(move |id, to, _: &mut ala::Window, cx: &mut App| {
            for command in move_window_commands(&windows, zz_protocol::WindowId(id), to) {
                sidebar::execute(&connection, &command, cx);
            }
        }) as TabMove
    });
    let mut windows = model.windows[visible.clone()]
        .iter()
        .zip(visible)
        .map(|(window, index)| {
            let id = window.id;
            let rename = writable.then(|| {
                let (label, command) = zz_client::navigation::rename_prompt_command(
                    zz_client::navigation::RenameTarget::Window(id),
                    &window.name,
                );
                let connection = view.connection.clone();
                (
                    label.into(),
                    Rc::new(move |_: &mut ala::Window, cx: &mut App| {
                        sidebar::execute(&connection, &command, cx);
                    }) as StatusAction,
                )
            });
            status_window(
                ("web-status-window", id.0),
                window.label.clone().into(),
                WorkspaceStatusWindowState {
                    connected,
                    active: window.active,
                    alone,
                },
                window
                    .panes
                    .iter()
                    .map(|pane| {
                        let connection = view.connection.clone();
                        let target = pane.id.to_string();
                        let mut entry = StatusPaneEntry::from_pane(
                            pane,
                            Rc::new(move |_, cx| {
                                connection.update(cx, |connection, cx| {
                                    connection.command(
                                        "select-window",
                                        vec!["-t".into(), target.clone()],
                                        cx,
                                    );
                                    connection.command(
                                        "select-pane",
                                        vec!["-Z".into(), "-t".into(), target.clone()],
                                        cx,
                                    );
                                });
                            }),
                        );
                        if let Some(title) = browser_title(&host, pane, cx) {
                            entry.label = title.into();
                        }
                        entry
                    })
                    .collect(),
                StatusWindowActions {
                    select: command_action(&view.connection, "select-window", id.to_string()),
                    close: writable
                        .then(|| command_action(&view.connection, "kill-window", id.to_string())),
                    rename,
                    reorder: on_move.as_ref().map(|on_move| TabReorder {
                        strip,
                        id: id.0,
                        index,
                        on_move: Rc::clone(on_move),
                    }),
                },
                cx,
            )
        })
        .collect::<Vec<_>>();
    if model.windows.len() > MAX_VISIBLE_WINDOWS {
        let entries = model
            .windows
            .iter()
            .map(|window| StatusWindowEntry {
                label: format!("{} {}", window.index, window.label).into(),
                active: window.active,
                select: command_action(&view.connection, "select-window", window.id.to_string()),
            })
            .collect();
        windows.push(status_window_overflow(
            "web-status-window-overflow",
            entries,
            connected,
            cx,
        ));
    }
    workspace_status_bar(
        view.preferences.gaps,
        px(8.0),
        WorkspaceStatusSlots {
            windows,
            titlebar_controls: Some((
                view.controls(cx.theme().ground(), cx),
                AppShell::controls_width(window),
            )),
            window_controls: None,
        },
        cx,
    )
    .into_any_element()
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn browser_title(host: &str, pane: &StatusBarPane, cx: &mut App) -> Option<String> {
    let PaneKindSnapshot::Browser(browser) = &pane.kind else {
        return None;
    };
    super::browser_pane::page_title(host, &browser.profile, browser.url(), cx)
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn browser_title(_: &str, _: &StatusBarPane, _: &mut App) -> Option<String> {
    None
}

fn command_action(
    connection: &Entity<Connection>,
    command: &'static str,
    target: String,
) -> StatusAction {
    let connection = connection.clone();
    Rc::new(move |_, cx| {
        connection.update(cx, |connection, cx| {
            connection.command(command, vec!["-t".into(), target.clone()], cx);
        });
    })
}
