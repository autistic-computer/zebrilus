use super::AppShell;
use crate::connection::Connection;
use std::{collections::BTreeSet, rc::Rc};
use zz_client::{
    AgentAttentionStatus, ChromeAction,
    navigation::{RenameTarget, ordered_panes, pane_label, rename_prompt_command, session_label},
    program::Program,
    window_summary::{DaemonClock, age_label, pane_program, summarize_host},
};
use zz_gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Hsla, ListSizingBehavior, MouseButton,
    ScrollHandle, ScrollStrategy, UniformListScrollHandle, Window, div, prelude::*, px,
    uniform_list,
};
use zz_protocol::{CommandInvocation, MuxSnapshot, PaneId, SessionId, WindowId};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Ground, Icon, IconName,
    command::PaletteHostId,
    menu::PopupMenuItem,
    motion::Instant,
    navigation::{
        WORKSPACE_SIDEBAR_FADE_HEIGHT, WORKSPACE_TREE_CONTENT_INSET, WORKSPACE_TREE_INDENT_WIDTH,
        WORKSPACE_TREE_MARKER_SLOT_WIDTH, WORKSPACE_TREE_NODE_ICON_SIZE,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatMenu, FlatState, HostConnection, flat_sidebar,
        },
        sidebar::{
            TreeNavigation, TreeNavigationResult, TreeNavigationRow, TreeRowMenuItem,
            tree_action_strip, tree_host_indicator, tree_host_marker, tree_navigation,
            tree_node_marker, tree_row_menu,
        },
        tree::{IndentGuideColors, WorkspaceIndentGuides},
        workspace_tree_action_button, workspace_tree_disclosure, workspace_tree_opened,
        workspace_tree_row,
    },
    program::program_mark,
    rems_from_px,
    scroll::ScrollableElement as _,
};

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Target {
    Host,
    Session(SessionId),
    Window(WindowId),
    Pane(PaneId),
}
impl Target {
    fn id(self) -> String {
        match self {
            Self::Host => "host".into(),
            Self::Session(id) => format!("session-{id}"),
            Self::Window(id) => format!("window-{id}"),
            Self::Pane(id) => format!("pane-{id}"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum Badge {
    NeedsInput,
    Failed,
    Working,
    Finished,
}

impl Badge {
    fn color(self, cx: &App) -> Hsla {
        match self {
            Self::NeedsInput => cx.theme().warning,
            Self::Failed => cx.theme().danger,
            Self::Working => cx.theme().foreground.muted(),
            Self::Finished => cx.theme().success,
        }
    }
}

fn pane_badge(
    pane: PaneId,
    core: &zz_client::ClientCore,
    unseen: &BTreeSet<PaneId>,
) -> Option<Badge> {
    match zz_client::agent_attention_status(core.agent_state(pane)?) {
        AgentAttentionStatus::NeedsInput => Some(Badge::NeedsInput),
        AgentAttentionStatus::Failed => Some(Badge::Failed),
        AgentAttentionStatus::Working => Some(Badge::Working),
        AgentAttentionStatus::Idle => unseen.contains(&pane).then_some(Badge::Finished),
    }
}

fn rollup(
    panes: impl Iterator<Item = PaneId>,
    core: &zz_client::ClientCore,
    unseen: &BTreeSet<PaneId>,
) -> Option<Badge> {
    panes
        .filter_map(|pane| pane_badge(pane, core, unseen))
        .min()
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
    match target {
        Target::Host => None,
        Target::Session(id) => snapshot
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| rename_prompt_command(RenameTarget::Session(id), &session.name)),
        Target::Window(id) => snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.id == id)
            .map(|window| rename_prompt_command(RenameTarget::Window(id), &window.name)),
        Target::Pane(id) => snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .find(|window| window.panes.contains_key(&id))
            .map(|window| rename_prompt_command(RenameTarget::Window(window.id), &window.name)),
    }
}

fn activate(connection: &Entity<Connection>, target: Target, cx: &mut App) {
    connection.update(cx, |connection, cx| {
        if !connection.connected {
            if target == Target::Host {
                connection.reconnect(cx);
            }
            return;
        }
        let owner = connection
            .core
            .snapshot()
            .sessions
            .iter()
            .find(|session| match target {
                Target::Host => false,
                Target::Session(id) => session.id == id,
                Target::Window(id) => session.windows.iter().any(|window| window.id == id),
                Target::Pane(id) => session
                    .windows
                    .iter()
                    .any(|window| window.panes.contains_key(&id)),
            })
            .map(|session| session.id);
        if let Some(owner) = owner
            && (matches!(target, Target::Session(_))
                || connection.core.attached_session() != Some(owner))
        {
            connection.attach(owner, cx);
        }
        match target {
            Target::Host | Target::Session(_) => {}
            Target::Window(id) => {
                connection.command("select-window", vec!["-t".into(), id.to_string()], cx);
            }
            Target::Pane(id) => {
                connection.command("select-window", vec!["-t".into(), id.to_string()], cx);
                connection.command(
                    "select-pane",
                    vec!["-Z".into(), "-t".into(), id.to_string()],
                    cx,
                );
            }
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
    let active = mux_window
        .map(|window| Target::Pane(window.active_pane))
        .or_else(|| session.map(|session| Target::Session(session.id)))
        .unwrap_or(Target::Host);
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
    let rows = tree_rows(
        snapshot,
        core.attached_session(),
        &view.collapsed_tree,
        core,
        &view.unseen_agents,
    );
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
    let rows = tree_rows(
        core.snapshot(),
        core.attached_session(),
        &view.collapsed_tree,
        core,
        &view.unseen_agents,
    );
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
        let navigation_rows = rows
            .iter()
            .enumerate()
            .map(|(index, row)| TreeNavigationRow {
                depth: row.depth,
                parent: rows[..index]
                    .iter()
                    .rposition(|parent| parent.depth < row.depth),
                expandable: row.expandable,
                expanded: row.expanded,
            })
            .collect::<Vec<_>>();
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
struct TreeRow {
    target: Target,
    depth: u8,
    icon: IconName,
    label: String,
    index: Option<String>,
    on_active_path: bool,
    expanded: bool,
    expandable: bool,
    bell: bool,
    badge: Option<Badge>,
    program: Option<&'static Program>,
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
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
    runtime: Runtime,
    collapsed: &BTreeSet<Target>,
    scroll: &UniformListScrollHandle,
    fade: Hsla,
    cx: &App,
) -> AnyElement {
    let connection = &runtime.connection;
    let connected = connection.read(cx).connected;
    let rows = tree_rows(
        snapshot,
        attached,
        collapsed,
        &connection.read(cx).core,
        &runtime.unseen_agents,
    );
    let active_row = connected
        .then(|| rows.iter().rposition(|row| row.on_active_path))
        .flatten();
    let depths: Rc<[usize]> = rows.iter().map(|row| usize::from(row.depth)).collect();
    let guides = WorkspaceIndentGuides::new(
        depths,
        active_row,
        px(WORKSPACE_TREE_INDENT_WIDTH),
        px(WORKSPACE_TREE_CONTENT_INSET + WORKSPACE_TREE_MARKER_SLOT_WIDTH / 2.0),
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
    .pr(zz_ui::navigation::WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER)
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
    folded_sessions: BTreeSet<SessionId>,
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
        summarize_host(snapshot),
        |changed_at| age_label(changed_at, now),
    );
    host.folded = view.flat.folded_host;
    for session in &mut host.sessions {
        session.folded = view.flat.folded_sessions.contains(&session.id);
    }
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
        FlatEvent::FoldSession(_, session) => {
            if !view.flat.folded_sessions.remove(&session) {
                view.flat.folded_sessions.insert(session);
            }
            cx.notify();
            return;
        }
        FlatEvent::NewSession(_) => {
            if writable(connection.read(cx)) {
                connection.update(cx, |connection, cx| {
                    connection.command("new-session", Vec::new(), cx);
                });
            }
            return;
        }
        FlatEvent::NewWindow(_, session) => {
            if writable(connection.read(cx)) {
                connection.update(cx, |connection, cx| {
                    connection.command("new-window", vec!["-t".into(), session.to_string()], cx);
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

fn tree_rows(
    snapshot: &MuxSnapshot,
    attached: Option<SessionId>,
    collapsed: &BTreeSet<Target>,
    core: &zz_client::ClientCore,
    unseen_agents: &BTreeSet<PaneId>,
) -> Vec<TreeRow> {
    let expanded = !collapsed.contains(&Target::Host);
    let mut rows = vec![TreeRow {
        target: Target::Host,
        depth: 0,
        icon: IconName::HardDrive,
        label: "zz daemon".into(),
        index: None,
        on_active_path: attached.is_some(),
        expanded,
        expandable: true,
        bell: snapshot
            .sessions
            .iter()
            .flat_map(|session| &session.windows)
            .flat_map(|window| window.panes.values())
            .any(|pane| pane.bell),
        badge: rollup(
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
                .flat_map(|window| window.panes.keys().copied()),
            core,
            unseen_agents,
        ),
        program: None,
    }];
    if !expanded {
        return rows;
    }
    for session in &snapshot.sessions {
        let active_session = attached == Some(session.id);
        let target = Target::Session(session.id);
        let expanded = !collapsed.contains(&target);
        let label = session_label(session);
        rows.push(TreeRow {
            target,
            depth: 1,
            icon: IconName::Layers,
            label: label.text,
            index: label.index,
            on_active_path: active_session,
            expanded,
            expandable: !session.windows.is_empty(),
            bell: session
                .windows
                .iter()
                .flat_map(|window| window.panes.values())
                .any(|pane| pane.bell),
            badge: rollup(
                session
                    .windows
                    .iter()
                    .flat_map(|window| window.panes.keys().copied()),
                core,
                unseen_agents,
            ),
            program: None,
        });
        if !expanded {
            continue;
        }
        for window in &session.windows {
            let active_window = active_session && snapshot.focused_window_for(session) == window.id;
            let target = Target::Window(window.id);
            let expanded = !collapsed.contains(&target);
            rows.push(TreeRow {
                target,
                depth: 2,
                icon: IconName::AppWindow,
                label: window.name.clone(),
                index: None,
                on_active_path: active_window,
                expanded,
                expandable: !window.panes.is_empty(),
                bell: window.panes.values().any(|pane| pane.bell),
                badge: rollup(window.panes.keys().copied(), core, unseen_agents),
                program: if expanded {
                    None
                } else {
                    window.panes.get(&window.active_pane).and_then(pane_program)
                },
            });
            if !expanded {
                continue;
            }
            rows.extend(ordered_panes(window).into_iter().map(|pane| TreeRow {
                target: Target::Pane(pane.id),
                depth: 3,
                icon: super::pane_icon(&pane.kind),
                label: pane_label(pane),
                index: None,
                on_active_path: active_window && window.active_pane == pane.id,
                expanded: false,
                expandable: false,
                bell: pane.bell,
                badge: pane_badge(pane.id, core, unseen_agents),
                program: pane_program(pane),
            }));
        }
    }
    rows
}

fn disclosure_id(target: Target) -> String {
    format!("web-tree-disclosure-{}", target.id())
}

/// When the row at `index` came into view by an ancestor opening, so it can
/// drop in: the latest instant any of its ancestors opened.
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
    let id = target.id();
    let group = format!("web-tree-group-{id}");
    let is_host = matches!(target, Target::Host);
    let color = if is_host || connected && entry.on_active_path {
        cx.theme().foreground
    } else {
        cx.theme().foreground.muted()
    };
    let mut actions = Vec::new();
    let add = match target {
        Target::Host => Some(("New session", "new-session", Vec::new())),
        Target::Session(session) => Some((
            "New window",
            "new-window",
            vec!["-t".into(), session.to_string()],
        )),
        _ => None,
    };
    if let Some((label, command, args)) = add {
        let connection = connection.clone();
        actions.push(
            workspace_tree_action_button(
                format!("web-tree-add-{id}"),
                IconName::Plus,
                label,
                disabled,
                cx,
            )
            .on_click(move |_, _, cx| {
                connection.update(cx, |connection, cx| {
                    connection.command(command, args.clone(), cx);
                });
                cx.stop_propagation();
            })
            .into_any_element(),
        );
    }

    let close = match target {
        Target::Host => None,
        Target::Session(id) => Some(("Close session", "kill-session", id.to_string())),
        Target::Window(id) => Some(("Close window", "kill-window", id.to_string())),
        Target::Pane(id) => Some(("Close pane", "kill-pane", id.to_string())),
    };
    if let Some((label, command, target_id)) = close {
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
                    connection.command(command, vec!["-t".into(), target_id.clone()], cx);
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
    let badge_color = entry.badge.map(|badge| badge.color(cx));
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
                    Icon::new(entry.icon.clone())
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
    let row = zz_ui::motion::enter_since(row, opened, window, cx);
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
