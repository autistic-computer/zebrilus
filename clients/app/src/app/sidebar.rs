use super::AppShell;
use crate::connection::Connection;
use ala::{
    AnyElement, App, Context, Entity, FocusHandle, Hsla, ListSizingBehavior, MouseButton,
    ScrollHandle, ScrollStrategy, UniformListScrollHandle, Window, div, prelude::*, px,
    uniform_list,
};
use penna::sidebar::SIDEBAR_CONTENT_INSET;
use penna::{
    ActiveTheme as _, Colorize as _, Ground, Icon, IconName, menu::PopupMenuItem, motion::Instant,
    rems_from_px, scroll::ScrollableElement as _,
};
use std::{collections::BTreeSet, rc::Rc};
use zz_client::{
    ChromeAction,
    new_chat::open_picker_commands,
    sidebar::{
        SidebarBadge, SidebarNode, SidebarRow as TreeRow, navigation_rows, sidebar_activation,
        sidebar_active, sidebar_add_command, sidebar_close_command, sidebar_rename_command,
        sidebar_rows,
    },
    window_summary::{DaemonClock, PaneTag, age_label, summarize_chats},
};
use zz_protocol::{CommandInvocation, MuxSnapshot, PaneId};
use zz_ui::{
    command::PaletteHostId,
    navigation::{
        WORKSPACE_SIDEBAR_FADE_HEIGHT, WORKSPACE_TREE_INDENT_WIDTH,
        WORKSPACE_TREE_MARKER_SLOT_WIDTH, WORKSPACE_TREE_NODE_ICON_SIZE,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatMenu, FlatState, HostConnection, flat_sidebar,
        },
        sidebar::{
            TreeNavigation, TreeNavigationResult, TreeRowMenuItem, tree_action_strip,
            tree_host_indicator, tree_host_marker, tree_navigation, tree_node_marker,
            tree_row_menu,
        },
        tree::{IndentGuideColors, WorkspaceIndentGuides},
        workspace_tree_action_button, workspace_tree_disclosure, workspace_tree_opened,
        workspace_tree_row,
    },
    program::program_mark,
};

pub(super) use zz_client::sidebar::SidebarTarget as Target;

fn target_id(target: Target) -> String {
    match target {
        Target::Host => "host".into(),
        Target::Session(id) => format!("session-{id}"),
        Target::Window(id) => format!("window-{id}"),
        Target::Pane(id) => format!("pane-{id}"),
    }
}

fn badge_color(badge: SidebarBadge, cx: &App) -> Hsla {
    match badge {
        SidebarBadge::NeedsInput => cx.theme().warning,
        SidebarBadge::Failed => cx.theme().danger,
        SidebarBadge::Working => cx.theme().foreground.muted(),
        SidebarBadge::Finished => cx.theme().success,
    }
}

fn row_icon(node: SidebarNode) -> IconName {
    match node {
        SidebarNode::Host => IconName::HardDrive,
        SidebarNode::Session => IconName::Layers,
        SidebarNode::Window => IconName::AppWindow,
        SidebarNode::Pane(PaneTag::Terminal) => IconName::SquareTerminal,
        SidebarNode::Pane(PaneTag::Browser) => IconName::Globe,
        SidebarNode::Pane(PaneTag::Editor) => IconName::File,
        SidebarNode::Pane(PaneTag::Picker) => IconName::Plus,
    }
}

pub(super) fn execute(connection: &Entity<Connection>, command: &CommandInvocation, cx: &mut App) {
    connection.update(cx, |connection, cx| {
        connection.command(
            &command.name,
            command
                .args
                .iter()
                .map(|arg| arg.as_str().to_owned())
                .collect(),
            cx,
        );
    });
}

fn rename_command(
    snapshot: &MuxSnapshot,
    target: Target,
) -> Option<(&'static str, CommandInvocation)> {
    sidebar_rename_command(snapshot, target)
}

fn activate(connection: &Entity<Connection>, target: Target, cx: &mut App) {
    connection.update(cx, |connection, cx| {
        if !connection.connected {
            if target == Target::Host {
                connection.reconnect(cx);
            }
            return;
        }
        let activation = sidebar_activation(
            connection.core.snapshot(),
            connection.core.attached_session(),
            target,
        );
        if let Some(session) = activation.attach {
            connection.attach(session, cx);
        }
        for command in activation.commands {
            connection.invoke(command, cx);
        }
    });
}

fn toggle(view: &mut AppShell, target: Target, cx: &mut Context<AppShell>) {
    view.sidebar_selection = Some(target);
    if !view.collapsed_tree.remove(&target) {
        view.collapsed_tree.insert(target);
    }
    cx.notify();
}

pub(super) fn reconcile(view: &mut AppShell, window: &Window, cx: &App) {
    let core = &view.connection.read(cx).core;
    let snapshot = core.snapshot();
    view.unseen_agents
        .retain(|pane| core.agent_state(*pane).is_some());
    if window.is_window_active()
        && let Some(pane) = view.focused_pane
    {
        view.unseen_agents.remove(&pane);
    }
    let session = snapshot
        .sessions
        .iter()
        .find(|session| Some(session.id) == core.attached_session());
    let mux_window = session.and_then(|session| {
        session
            .windows
            .iter()
            .find(|window| window.id == snapshot.focused_window_for(session))
    });
    let active = sidebar_active(snapshot, core.attached_session());
    let active_changed = view.sidebar_active != Some(active);
    if active_changed {
        view.sidebar_active = Some(active);
        view.collapsed_tree.remove(&Target::Host);
        if let Some(session) = session {
            view.collapsed_tree.remove(&Target::Session(session.id));
        }
        if let Some(window) = mux_window {
            view.collapsed_tree.remove(&Target::Window(window.id));
        }
        if !view.sidebar_focus.is_focused(window) || view.sidebar_pointer_selection {
            view.sidebar_selection = Some(active);
        }
    }
    let rows = sidebar_rows(core, &view.collapsed_tree, &view.unseen_agents);
    if active_changed && let Some(index) = rows.iter().position(|row| row.target == active) {
        view.sidebar_scroll
            .scroll_to_item(index, ScrollStrategy::Nearest);
    }
    if !rows
        .iter()
        .any(|row| Some(row.target) == view.sidebar_selection)
    {
        view.sidebar_selection = rows
            .iter()
            .rfind(|row| row.on_active_path)
            .map(|row| row.target)
            .or(Some(Target::Host));
    }
}

pub(super) fn handle_key(
    view: &mut AppShell,
    action: ChromeAction,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) -> bool {
    reconcile(view, window, cx);
    let core = &view.connection.read(cx).core;
    let rows = sidebar_rows(core, &view.collapsed_tree, &view.unseen_agents);
    let selected = rows
        .iter()
        .position(|row| Some(row.target) == view.sidebar_selection);
    let direction = match action {
        ChromeAction::SidebarSelectUp => Some(TreeNavigation::Up),
        ChromeAction::SidebarSelectDown => Some(TreeNavigation::Down),
        ChromeAction::SidebarSelectFirst => Some(TreeNavigation::First),
        ChromeAction::SidebarSelectLast => Some(TreeNavigation::Last),
        ChromeAction::SidebarSelectLeft => Some(TreeNavigation::Left),
        ChromeAction::SidebarSelectRight => Some(TreeNavigation::Right),
        _ => None,
    };
    if let Some(direction) = direction {
        let navigation_rows = navigation_rows(&rows);
        view.sidebar_pointer_selection = false;
        match tree_navigation(&navigation_rows, selected, direction) {
            TreeNavigationResult::Select(index) => {
                view.sidebar_selection = Some(rows[index].target);
                view.sidebar_scroll
                    .scroll_to_item(index, ScrollStrategy::Nearest);
                cx.notify();
            }
            TreeNavigationResult::Toggle(index) => toggle(view, rows[index].target, cx),
            TreeNavigationResult::None => {}
        }
        return true;
    }
    match action {
        ChromeAction::SidebarCancel => {
            view.release_sidebar_focus(window, cx);
        }
        ChromeAction::SidebarConfirm => {
            if let Some(target) = view.sidebar_selection {
                if target == Target::Host && view.connection.read(cx).connected {
                    toggle(view, target, cx);
                } else {
                    activate(&view.connection, target, cx);
                    view.release_sidebar_focus(window, cx);
                }
            }
        }
        ChromeAction::SidebarRename => {
            if view.connection.read(cx).connected
                && !core.attached_read_only()
                && let Some((_, command)) = view
                    .sidebar_selection
                    .and_then(|target| rename_command(core.snapshot(), target))
            {
                execute(&view.connection, &command, cx);
            }
        }
        ChromeAction::SidebarCommandPalette => view.open_palette(
            Some(crate::command_palette::PaletteMode::Command),
            window,
            cx,
        ),
        _ => return false,
    }
    true
}
pub(super) struct Runtime {
    pub connection: Entity<Connection>,
    pub focus: FocusHandle,
    pub focused: bool,
    pub view: Entity<AppShell>,
    pub selected: Option<Target>,
    pub unseen_agents: BTreeSet<PaneId>,
    pub ground: Ground,
}

pub(super) fn session_tree(
    runtime: Runtime,
    collapsed: &BTreeSet<Target>,
    scroll: &UniformListScrollHandle,
    fade: Hsla,
    cx: &App,
) -> AnyElement {
    let connection = &runtime.connection;
    let connected = connection.read(cx).connected;
    let rows = sidebar_rows(&connection.read(cx).core, collapsed, &runtime.unseen_agents);
    let active_row = connected
        .then(|| rows.iter().rposition(|row| row.on_active_path))
        .flatten();
    let depths: Rc<[usize]> = rows.iter().map(|row| usize::from(row.depth)).collect();
    let guides = WorkspaceIndentGuides::new(
        depths,
        active_row,
        px(WORKSPACE_TREE_INDENT_WIDTH),
        px(SIDEBAR_CONTENT_INSET + WORKSPACE_TREE_MARKER_SLOT_WIDTH / 2.0),
        px(4.0),
        IndentGuideColors::tree(cx),
    );
    let list = uniform_list(
        "web-session-tree-rows",
        rows.len(),
        move |range, window, cx| {
            range
                .map(|index| {
                    let opened = row_opened(&rows, index, cx);
                    render_row(
                        &rows[index],
                        active_row == Some(index),
                        opened,
                        &runtime,
                        window,
                        cx,
                    )
                })
                .collect::<Vec<_>>()
        },
    )
    .size_full()
    .pr(penna::sidebar::SIDEBAR_SCROLLBAR_GUTTER)
    .with_sizing_behavior(ListSizingBehavior::Auto)
    .track_scroll(scroll)
    .with_decoration(guides);
    div()
        .id("web-session-tree")
        .relative()
        .size_full()
        .min_h_0()
        .child(list)
        .bottom_fade(
            scroll,
            fade,
            zz_ui::navigation::WORKSPACE_SIDEBAR_FADE_HEIGHT,
        )
        .vertical_scrollbar(scroll)
        .into_any_element()
}

const HOST: PaletteHostId = PaletteHostId(0);

#[derive(Default)]
pub(super) struct Flat {
    pub clock: DaemonClock,
    state: FlatState,
    folded_host: bool,
    scroll: ScrollHandle,
}

fn host_connection(connection: &Connection) -> HostConnection {
    match connection.status.as_str() {
        _ if connection.connected => HostConnection::Connected,
        "Connecting…" => HostConnection::Connecting,
        "" | "Not connected" => HostConnection::Disconnected,
        reason => HostConnection::Unreachable(reason.to_owned().into()),
    }
}

fn writable(connection: &Connection) -> bool {
    connection.connected && !connection.core.attached_read_only()
}

pub(super) fn flat_tree(
    view: &AppShell,
    fade: Hsla,
    ground: Ground,
    cx: &mut Context<AppShell>,
) -> AnyElement {
    let connection = view.connection.read(cx);
    let core = &connection.core;
    let snapshot = core.snapshot();
    let now = view.flat.clock.now(super::unix_seconds());
    let mut host = FlatHost::new(
        HOST,
        "zz daemon",
        false,
        host_connection(connection),
        summarize_chats(&snapshot.sessions),
        |changed_at| age_label(changed_at, now),
    );
    host.folded = view.flat.folded_host;
    let current = snapshot
        .sessions
        .iter()
        .find(|session| connection.connected && Some(session.id) == core.attached_session())
        .map(|session| (HOST, snapshot.focused_window_for(session)));
    let state = FlatState {
        current,
        ..view.flat.state
    };
    let shell = cx.entity().downgrade();
    let handler: FlatHandler = Rc::new(move |event, window, cx| {
        let _ = shell.update(cx, |view, cx| flat_event(view, event, window, cx));
    });
    let connection = view.connection.clone();
    let menu: FlatMenu = Rc::new(move |_, id, menu, _, cx| {
        let disabled = !writable(connection.read(cx));
        let rename = rename_command(connection.read(cx).core.snapshot(), Target::Window(id))
            .map(|(_, command)| command);
        let (rename_connection, close_connection) = (connection.clone(), connection.clone());
        menu.item(
            PopupMenuItem::new("Rename window")
                .disabled(disabled || rename.is_none())
                .on_click(move |_, _, cx| {
                    if let Some(command) = &rename {
                        execute(&rename_connection, command, cx);
                    }
                }),
        )
        .item(
            PopupMenuItem::new("Close window")
                .disabled(disabled)
                .on_click(move |_, _, cx| {
                    close_connection.update(cx, |connection, cx| {
                        connection.command("kill-window", vec!["-t".into(), id.to_string()], cx);
                    });
                }),
        )
    });
    let list = flat_sidebar(&[host], state, &handler, &menu, ground, cx);
    let scroll = &view.flat.scroll;
    div()
        .id("web-flat-sidebar")
        .relative()
        .size_full()
        .min_h_0()
        .child(
            div()
                .id("web-flat-sidebar-rows")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .child(list),
        )
        .bottom_fade(scroll, fade, WORKSPACE_SIDEBAR_FADE_HEIGHT)
        .vertical_scrollbar(scroll)
        .into_any_element()
}

fn flat_event(
    view: &mut AppShell,
    event: FlatEvent,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) {
    let connection = view.connection.clone();
    let target = match event {
        FlatEvent::Open(_, id) => Target::Window(id),
        FlatEvent::OpenPane(_, _, pane) => Target::Pane(pane),
        FlatEvent::FoldHost(_) => {
            view.flat.folded_host = !view.flat.folded_host;
            cx.notify();
            return;
        }
        FlatEvent::NewChat(_) => {
            if writable(connection.read(cx)) {
                connection.update(cx, |connection, cx| {
                    let commands = open_picker_commands(
                        connection.core.snapshot(),
                        connection.core.attached_session(),
                    );
                    for command in commands {
                        connection.invoke(command, cx);
                    }
                });
            }
            return;
        }
        FlatEvent::Connect(_) | FlatEvent::Upgrade(_) => {
            connection.update(cx, Connection::reconnect);
            return;
        }
        FlatEvent::Hover(target, hovered) => {
            if view.flat.state.hover(target, hovered) {
                cx.notify();
            }
            return;
        }
    };
    if view.slideover && connection.read(cx).connected {
        view.release_sidebar_focus(window, cx);
    }
    activate(&connection, target, cx);
}

fn disclosure_id(target: Target) -> String {
    format!("web-tree-disclosure-{}", target_id(target))
}

fn row_opened(rows: &[TreeRow], index: usize, cx: &mut App) -> Option<Instant> {
    let mut depth = rows.get(index)?.depth;
    let mut opened = None;
    for row in rows[..index].iter().rev() {
        if row.depth < depth {
            opened = opened.max(workspace_tree_opened(disclosure_id(row.target), cx));
            depth = row.depth;
        }
    }
    opened
}

fn render_row(
    entry: &TreeRow,
    active: bool,
    opened: Option<Instant>,
    runtime: &Runtime,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let connection = &runtime.connection;
    let connected = connection.read(cx).connected;
    let disabled = !connected || connection.read(cx).core.attached_read_only();
    let target = entry.target;
    let id = target_id(target);
    let group = format!("web-tree-group-{id}");
    let is_host = matches!(target, Target::Host);
    let color = if is_host || connected && entry.on_active_path {
        cx.theme().foreground
    } else {
        cx.theme().foreground.muted()
    };
    let mut actions = Vec::new();
    if let Some((label, command)) = sidebar_add_command(target) {
        let icon = if is_host {
            IconName::LayersPlus
        } else {
            IconName::WindowPlus
        };
        let connection = connection.clone();
        actions.push(
            workspace_tree_action_button(format!("web-tree-add-{id}"), icon, label, disabled, cx)
                .on_click(move |_, _, cx| {
                    connection.update(cx, |connection, cx| {
                        connection.invoke(command.clone(), cx);
                    });
                    cx.stop_propagation();
                })
                .into_any_element(),
        );
    }

    if let Some((label, command)) = sidebar_close_command(target) {
        let connection = connection.clone();
        actions.push(
            workspace_tree_action_button(
                format!("web-tree-close-{id}"),
                IconName::Xmark,
                label,
                disabled,
                cx,
            )
            .on_click(move |_, _, cx| {
                connection.update(cx, |connection, cx| {
                    connection.invoke(command.clone(), cx);
                });
                cx.stop_propagation();
            })
            .into_any_element(),
        );
    }
    if is_host && !connected {
        actions.insert(
            0,
            tree_host_indicator(
                "web-tree-host-indicator",
                connection.read(cx).status == "Connecting…",
                Some(connection.read(cx).status.clone().into()),
                cx,
            ),
        );
    }
    let actions = tree_action_strip(format!("web-tree-actions-{id}"), actions, cx);
    let badge_color = entry.badge.map(|badge| badge_color(badge, cx));
    let marker = if is_host {
        tree_host_marker(entry.bell, badge_color, cx)
    } else {
        tree_node_marker(
            entry
                .program
                .and_then(|program| {
                    program_mark(
                        program,
                        rems_from_px(WORKSPACE_TREE_NODE_ICON_SIZE),
                        color,
                        None,
                    )
                })
                .unwrap_or_else(|| {
                    Icon::new(row_icon(entry.node))
                        .medium()
                        .text_color(color)
                        .into_any_element()
                }),
            entry.bell,
            badge_color,
            cx,
        )
    };
    let marker = if entry.expandable {
        let view = runtime.view.clone();
        workspace_tree_disclosure(
            disclosure_id(target),
            marker,
            entry.expanded,
            group.clone().into(),
            cx,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            view.update(cx, |view, cx| {
                view.sidebar_pointer_selection = true;
                toggle(view, target, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
    } else {
        marker
    };
    let connection = connection.clone();
    let focus = runtime.focus.clone();
    let view = runtime.view.clone();
    let row = workspace_tree_row(
        format!("web-tree-row-{id}"),
        entry.depth,
        active,
        runtime.focused && runtime.selected == Some(target),
        runtime.focused,
        connected,
        entry.expandable || connected && !is_host,
        !is_host,
        group.into(),
        marker,
        div()
            .flex()
            .min_w_0()
            .items_baseline()
            .gap_2()
            .text_color(color)
            .child(div().min_w_0().overflow_hidden().child(entry.label.clone()))
            .children(entry.index.clone().map(|index| {
                div()
                    .flex_none()
                    .text_color(cx.theme().foreground.faint())
                    .child(index)
            })),
        actions,
        runtime.ground,
        cx,
    )
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        focus.focus(window, cx);
    })
    .on_click(move |_, window, cx| {
        view.update(cx, |view, cx| {
            view.sidebar_selection = Some(target);
            view.sidebar_pointer_selection = true;
            if target == Target::Host && connection.read(cx).connected {
                toggle(view, target, cx);
            } else if view.slideover && target != Target::Host && connection.read(cx).connected {
                view.release_sidebar_focus(window, cx);
            }
            cx.notify();
        });
        activate(&connection, target, cx);
        cx.stop_propagation();
    });
    let row = penna::motion::enter_since(row, opened, window, cx);
    if !disabled
        && let Some((label, command)) =
            rename_command(runtime.connection.read(cx).core.snapshot(), target)
    {
        let connection = runtime.connection.clone();
        tree_row_menu(
            row,
            vec![TreeRowMenuItem::new(label, move |_, cx| {
                execute(&connection, &command, cx);
            })],
        )
    } else {
        row.into_any_element()
    }
}
