use super::AppShell;
use crate::{connection::Connection, roster::HostRequest};
use ala::{
    AnyElement, App, Context, Entity, FocusHandle, Hsla, ListSizingBehavior, MouseButton,
    ScrollHandle, ScrollStrategy, UniformListScrollHandle, Window, div, prelude::*, px,
    uniform_list,
};
use penna::{
    ActiveTheme as _, Colorize as _, Ground, Icon, IconName,
    components::menu::PopupMenuItem,
    components::scroll::ScrollableElement as _,
    components::tree::{IndentGuideColors, IndentGuides},
    foundation::motion::Instant,
    rems_from_px,
};
use std::{
    collections::{BTreeSet, HashMap},
    rc::Rc,
    sync::Arc,
};
use zz_client::{
    ChromeAction,
    new_chat::{ChatViewPlace, leave_picker_command},
    sidebar::{
        SidebarBadge, SidebarNode, SidebarRow as TreeRow, navigation_rows, sidebar_activation,
        sidebar_active, sidebar_add_command, sidebar_close_command, sidebar_rename_command,
        sidebar_rows,
    },
    window_summary::{
        ChatCurrent, ChatHints, ChatMachine, ChatParent, PaneTag, number_chat_rows,
        summarize_chat_machine,
    },
};
use zz_protocol::{CommandInvocation, HostPlatform, MuxSnapshot, WindowId};
use zz_ui::{
    command::PaletteHostId,
    navigation::{
        WORKSPACE_SIDEBAR_FADE_HEIGHT, WORKSPACE_TREE_INDENT_WIDTH, WORKSPACE_TREE_NODE_ICON_SIZE,
        WorkspaceTreeInk,
        flat::{
            FlatEvent, FlatHandler, FlatHost, FlatMenu, FlatState, HostConnection, flat_sidebar,
        },
        sidebar::{
            TreeNavigation, TreeNavigationResult, TreeRowMenuItem, tree_action_strip,
            tree_host_indicator, tree_host_marker, tree_navigation, tree_node_marker,
            tree_row_menu,
        },
        workspace_tree_action_button, workspace_tree_disclosure, workspace_tree_opened,
        workspace_tree_row,
    },
    program::program_mark,
};

pub(super) use zz_client::sidebar::SidebarTarget as Target;

fn target_id((host, target): Node) -> String {
    let host = host.0;
    match target {
        Target::Host => format!("{host}-host"),
        Target::Session(id) => format!("{host}-session-{id}"),
        Target::Window(id) => format!("{host}-window-{id}"),
        Target::Pane(id) => format!("{host}-pane-{id}"),
    }
}

fn add_command(target: Target, attached: bool) -> Option<(&'static str, CommandInvocation)> {
    match target {
        Target::Host if !attached => {
            Some(("New session", CommandInvocation::new("new-session", ["-d"])))
        }
        _ => sidebar_add_command(target),
    }
}

fn badge_color(badge: SidebarBadge, cx: &App) -> Hsla {
    match badge {
        SidebarBadge::NeedsInput => cx.theme().accent,
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
        SidebarNode::Pane(PaneTag::View) => IconName::Eye,
        SidebarNode::Pane(PaneTag::Other) => IconName::Info,
        SidebarNode::Pane(PaneTag::Picker) => IconName::Plus,
        SidebarNode::Pane(PaneTag::Agent(provider)) => zz_ui::pane::agent_provider_icon(provider),
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

pub(super) type Node = (PaletteHostId, Target);

pub(super) struct Row {
    host: PaletteHostId,
    connection: Option<Entity<Connection>>,
    attached: bool,
    row: TreeRow,
}

impl Row {
    const fn node(&self) -> Node {
        (self.host, self.row.target)
    }

    fn connected(&self, cx: &App) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|connection| connection.read(cx).connected)
    }
}

fn toggle(view: &mut AppShell, node: Node, cx: &mut Context<AppShell>) {
    view.sidebar_selection = Some(node);
    if !view.collapsed_tree.remove(&node) {
        view.collapsed_tree.insert(node);
    }
    cx.notify();
}

pub(super) fn tree_rows(view: &AppShell, cx: &App) -> Vec<Row> {
    let none = BTreeSet::new();
    let mut rows = Vec::new();
    for page in view.host_pages(cx) {
        let host = page.id;
        let attached = page.connection.as_ref() == Some(&view.connection);
        let Some(connection) = page.connection else {
            rows.push(Row {
                host,
                connection: None,
                attached,
                row: TreeRow {
                    target: Target::Host,
                    depth: 0,
                    node: SidebarNode::Host,
                    label: page.title,
                    index: None,
                    on_active_path: false,
                    expanded: false,
                    expandable: false,
                    bell: false,
                    badge: None,
                    program: None,
                },
            });
            continue;
        };
        let collapsed = view
            .collapsed_tree
            .iter()
            .filter(|(owner, _)| *owner == host)
            .map(|(_, target)| *target)
            .collect();
        let unseen = if attached { &view.unseen_agents } else { &none };
        let tree = sidebar_rows(&connection.read(cx).core, &collapsed, unseen);
        rows.extend(tree.into_iter().map(|mut row| {
            if row.target == Target::Host {
                row.label.clone_from(&page.title);
            }
            row.on_active_path &= attached;
            Row {
                host,
                connection: Some(connection.clone()),
                attached,
                row,
            }
        }));
    }
    rows
}

pub(super) fn reconcile(view: &mut AppShell, window: &Window, cx: &App) {
    let here = view.host_id_of(view.connection.read(cx));
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
    let active = (here, sidebar_active(snapshot, core.attached_session()));
    let active_changed = view.sidebar_active != Some(active);
    if active_changed {
        view.sidebar_active = Some(active);
        view.collapsed_tree.remove(&(here, Target::Host));
        if let Some(session) = session {
            view.collapsed_tree
                .remove(&(here, Target::Session(session.id)));
        }
        if let Some(window) = mux_window {
            view.collapsed_tree
                .remove(&(here, Target::Window(window.id)));
        }
        if !view.sidebar_focus.is_focused(window) || view.sidebar_pointer_selection {
            view.sidebar_selection = Some(active);
        }
    }
    let rows = tree_rows(view, cx);
    if active_changed && let Some(index) = rows.iter().position(|row| row.node() == active) {
        view.sidebar_scroll
            .scroll_to_item(index, ScrollStrategy::Nearest);
    }
    if !rows
        .iter()
        .any(|row| Some(row.node()) == view.sidebar_selection)
    {
        view.sidebar_selection = rows
            .iter()
            .rfind(|row| row.row.on_active_path)
            .map(Row::node)
            .or(Some((here, Target::Host)));
    }
}

pub(super) fn handle_key(
    view: &mut AppShell,
    action: ChromeAction,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) -> bool {
    reconcile(view, window, cx);
    let rows = tree_rows(view, cx);
    let selected = rows
        .iter()
        .position(|row| Some(row.node()) == view.sidebar_selection);
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
        let tree = rows.iter().map(|row| row.row.clone()).collect::<Vec<_>>();
        view.sidebar_pointer_selection = false;
        match tree_navigation(&navigation_rows(&tree), selected, direction) {
            TreeNavigationResult::Select(index) => {
                view.sidebar_selection = Some(rows[index].node());
                view.sidebar_scroll
                    .scroll_to_item(index, ScrollStrategy::Nearest);
                cx.notify();
            }
            TreeNavigationResult::Toggle(index) => toggle(view, rows[index].node(), cx),
            TreeNavigationResult::None => {}
        }
        return true;
    }
    match action {
        ChromeAction::SidebarCancel => {
            view.release_sidebar_focus(window, cx);
        }
        ChromeAction::SidebarConfirm => {
            if let Some(row) = selected.map(|index| &rows[index]) {
                let (host, target) = row.node();
                if target == Target::Host && row.attached && row.connected(cx) {
                    toggle(view, row.node(), cx);
                } else {
                    view.open_on_host(host, target, window, cx);
                    view.release_sidebar_focus(window, cx);
                }
            }
        }
        ChromeAction::SidebarRename => {
            if let Some((host, target)) = view.sidebar_selection {
                view.rename_on(host, target, window, cx);
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
    pub focus: FocusHandle,
    pub focused: bool,
    pub view: Entity<AppShell>,
    pub selected: Option<Node>,
    pub ground: Ground,
}

pub(super) fn session_tree(
    runtime: Runtime,
    rows: Vec<Row>,
    scroll: &UniformListScrollHandle,
    fade: Hsla,
    cx: &App,
) -> AnyElement {
    let active_row = rows
        .iter()
        .rposition(|row| row.attached && row.row.on_active_path)
        .filter(|index| rows[*index].connected(cx));
    let depths: Rc<[usize]> = rows.iter().map(|row| usize::from(row.row.depth)).collect();
    let guides = IndentGuides::new(
        depths,
        active_row,
        px(WORKSPACE_TREE_INDENT_WIDTH),
        zz_ui::navigation::workspace_tree_guide_origin(cx),
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
    .pr(penna::blocks::sidebar::SIDEBAR_SCROLLBAR_GUTTER)
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

impl AppShell {
    pub(super) fn rename_on(
        &mut self,
        host: PaletteHostId,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.host_page(host, cx) else {
            return;
        };
        let Some(connection) = page
            .connection
            .clone()
            .filter(|connection| writable(connection.read(cx)))
        else {
            return;
        };
        let snapshot = Arc::clone(connection.read(cx).core.snapshot());
        let Some((_, command)) = rename_command(&snapshot, target) else {
            return;
        };
        if connection != self.connection {
            let Some(session) = sidebar_activation(&snapshot, None, target).attach else {
                return;
            };
            open_on_other_host(self, &page, Target::Session(session), window, cx);
            if connection != self.connection {
                return;
            }
        }
        execute(&connection, &command, cx);
    }
}

impl AppShell {
    pub(super) fn host_id_of(&self, connection: &Connection) -> PaletteHostId {
        self.host_ids.id(connection.endpoint())
    }
}

#[derive(Default)]
struct HostRows {
    folded: bool,
    hints: ChatHints,
    open: BTreeSet<WindowId>,
    pile: bool,
}

#[derive(Default)]
pub(super) struct Flat {
    state: FlatState,
    scroll: ScrollHandle,
    pub place: ChatViewPlace,
    hosts: HashMap<PaletteHostId, HostRows>,
}

impl Flat {
    pub(super) fn forget_daemon(&mut self, host: PaletteHostId) {
        self.place = ChatViewPlace::default();
        let rows = self.hosts.entry(host).or_default();
        rows.hints = ChatHints::default();
        rows.open.clear();
        rows.pile = false;
    }

    pub(super) fn leave_host(&mut self) {
        self.state = FlatState::default();
        self.place = ChatViewPlace::default();
    }
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
    let pages = view.host_pages(cx);
    let flat = &view.flat;
    let none = HostRows::default();
    let mut machines = pages
        .iter()
        .map(|page| {
            let rows = flat.hosts.get(&page.id).unwrap_or(&none);
            page.connection
                .as_ref()
                .map_or_else(ChatMachine::default, |connection| {
                    summarize_chat_machine(
                        &connection.read(cx).core.snapshot().sessions,
                        &rows.hints,
                        |parent| match parent {
                            ChatParent::Window(window) => rows.open.contains(&window),
                            ChatParent::Pile => rows.pile,
                        },
                    )
                })
        })
        .collect::<Vec<_>>();
    number_chat_rows(&mut machines);
    let mut current = None;
    let mut hosts = Vec::with_capacity(pages.len());
    for (page, machine) in pages.iter().zip(machines) {
        let id = page.id;
        let rows = flat.hosts.get(&id).unwrap_or(&none);
        let Some(connection) = &page.connection else {
            let mut host = FlatHost::new(
                id,
                page.title.clone(),
                true,
                HostConnection::Disconnected,
                machine,
                super::unix_seconds(),
            );
            host.folded = rows.folded;
            hosts.push(host);
            continue;
        };
        if *connection == view.connection {
            current = current_chat(connection.read(cx), &machine).map(|at| (id, at));
        }
        let connection = connection.read(cx);
        let mut host = FlatHost::new(
            id,
            page.title.clone(),
            page.endpoint.is_some(),
            host_connection(connection),
            machine,
            connection.clock.now(super::unix_seconds()),
        );
        host.folded = rows.folded;
        host.platform = connection.core.host_platform();
        hosts.push(host);
    }
    let state = FlatState {
        current,
        ..view.flat.state
    };
    let shell = cx.entity().downgrade();
    let handler: FlatHandler = Rc::new(move |event, window, cx| {
        let _ = shell.update(cx, |view, cx| flat_event(view, event, window, cx));
    });
    let shell = cx.entity().downgrade();
    let menu: FlatMenu = Rc::new(move |host, id, menu, _, cx| {
        let Some(connection) = shell
            .upgrade()
            .and_then(|shell| shell.read(cx).host_page(host, cx)?.connection)
        else {
            return menu;
        };
        let disabled = !writable(connection.read(cx));
        let renamable =
            rename_command(connection.read(cx).core.snapshot(), Target::Window(id)).is_some();
        let close_connection = connection;
        let view = shell.clone();
        menu.item(
            PopupMenuItem::new("Rename window")
                .disabled(disabled || !renamable)
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.rename_on(host, Target::Window(id), window, cx);
                    });
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
    let list = flat_sidebar(&hosts, state, &handler, &menu, ground, cx);
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

fn current_chat(connection: &Connection, machine: &ChatMachine) -> Option<ChatCurrent> {
    let core = &connection.core;
    let snapshot = core.snapshot();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| connection.connected && Some(session.id) == core.attached_session())?;
    let focused = snapshot.focused_window_for(session);
    let window = snapshot
        .sessions
        .iter()
        .flat_map(|session| &session.windows)
        .find(|window| window.id == focused)?;
    machine.current(focused, window.active_pane)
}

pub(super) fn new_chat(view: &mut AppShell, cx: &mut Context<AppShell>) {
    let connection = view.connection.read(cx);
    if writable(connection) {
        view.flat.place.new_chat(
            connection.core.snapshot(),
            connection
                .core
                .attached_session()
                .or_else(|| connection.remembered_session()),
        );
        cx.notify();
    }
}

impl AppShell {
    pub(super) fn new_chat_on(
        &mut self,
        host: PaletteHostId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(page) = self.host_page(host, cx)
            && let (Some(endpoint), Some(connection)) = (page.endpoint, page.connection)
            && connection != self.connection
        {
            let session = connection.read(cx).remembered_session();
            self.switch_host(&endpoint, session, None, window, cx);
        }
        new_chat(self, cx);
    }
}

pub(super) fn follow_chat_view(view: &mut AppShell, cx: &mut Context<AppShell>) {
    {
        let core = &view.connection.read(cx).core;
        let snapshot = core.snapshot();
        let focused = snapshot
            .sessions
            .iter()
            .find(|session| Some(session.id) == core.attached_session())
            .map(|session| snapshot.focused_window_for(session));
        let windows = || {
            snapshot
                .sessions
                .iter()
                .flat_map(|session| &session.windows)
        };
        let live = windows().map(|window| window.id).collect::<BTreeSet<_>>();
        let host = view.host_id_of(view.connection.read(cx));
        let rows = view.flat.hosts.entry(host).or_default();
        rows.hints.retain(|window| live.contains(&window));
        rows.open.retain(|window| live.contains(window));
        if let Some(window) = windows().find(|window| Some(window.id) == focused) {
            rows.hints.observe(window);
        }
    }
    if !view.preferences.flat_sidebar {
        return;
    }
    let connection = view.connection.clone();
    let landing = view.flat.place.landing();
    let open = {
        let connection = connection.read(cx);
        view.flat
            .place
            .follow(
                connection.core.snapshot(),
                connection.core.attached_session(),
            )
            .filter(|_| writable(connection))
    };
    if landing != view.flat.place.landing() {
        cx.notify();
    }
    if let Some(window) = open {
        activate(&connection, Target::Window(window), cx);
    }
}

fn flat_event(
    view: &mut AppShell,
    event: FlatEvent,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) {
    let host = match event {
        FlatEvent::Hover(target, hovered) => {
            if view.flat.state.hover(target, hovered) {
                cx.notify();
            }
            return;
        }
        FlatEvent::Open(host, _)
        | FlatEvent::OpenPane(host, ..)
        | FlatEvent::ToggleWindow(host, _)
        | FlatEvent::TogglePile(host)
        | FlatEvent::FoldHost(host)
        | FlatEvent::Connect(host)
        | FlatEvent::Upgrade(host) => host,
    };
    let Some(machine) = view.host_page(host, cx) else {
        return;
    };
    let rows = view.flat.hosts.entry(host).or_default();
    let target = match event {
        FlatEvent::Open(_, id) => Target::Window(id),
        FlatEvent::OpenPane(_, _, pane) => Target::Pane(pane),
        FlatEvent::FoldHost(_) => {
            rows.folded = !rows.folded;
            cx.notify();
            return;
        }
        FlatEvent::ToggleWindow(_, window) => {
            if !rows.open.remove(&window) {
                rows.open.insert(window);
            }
            cx.notify();
            return;
        }
        FlatEvent::TogglePile(_) => {
            rows.pile = !rows.pile;
            cx.notify();
            return;
        }
        FlatEvent::Connect(_) | FlatEvent::Upgrade(_) => {
            connect(view, machine, window, cx);
            return;
        }
        FlatEvent::Hover(..) => return,
    };
    let Some(connection) = machine.connection.clone() else {
        return;
    };
    if view.slideover && connection.read(cx).connected {
        view.release_sidebar_focus(window, cx);
    }
    if connection != view.connection {
        open_on_other_host(view, &machine, target, window, cx);
        return;
    }
    view.flat.place.close_landing();
    leave_abandoned_picker(&connection, target, cx);
    activate(&connection, target, cx);
}

fn connect(
    view: &mut AppShell,
    page: super::compact::HostPage,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) {
    if let Some(endpoint) = page.endpoint {
        view.connect_host(endpoint, window, cx);
    }
}

impl AppShell {
    pub(super) fn sync_roster(&mut self, cx: &mut Context<Self>) {
        let hosts = self.host_pages(cx);
        self.roster.update(cx, |roster, cx| roster.sync(hosts, cx));
    }

    pub(super) fn host_request(
        &mut self,
        request: HostRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match request {
            HostRequest::Open(host, target) => self.open_on_host(host, target, window, cx),
            HostRequest::Connect(host) => {
                if let Some(page) = self.host_page(host, cx) {
                    connect(self, page, window, cx);
                }
            }
        }
    }

    pub(super) fn open_on_host(
        &mut self,
        host: PaletteHostId,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.host_page(host, cx) else {
            return;
        };
        let Some(connection) = page.connection.clone() else {
            connect(self, page, window, cx);
            return;
        };
        if connection == self.connection {
            activate(&connection, target, cx);
        } else {
            open_on_other_host(self, &page, target, window, cx);
        }
    }
}

fn open_on_other_host(
    view: &mut AppShell,
    host: &super::compact::HostPage,
    target: Target,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) {
    let (Some(endpoint), Some(connection)) = (&host.endpoint, &host.connection) else {
        return;
    };
    let connection = connection.read(cx);
    if !connection.connected {
        if target == Target::Host {
            view.connect_host(endpoint.clone(), window, cx);
        }
        return;
    }
    let snapshot = connection.core.snapshot();
    let place = match target {
        Target::Host => Some((None, None)),
        Target::Session(id) => snapshot
            .sessions
            .iter()
            .any(|session| session.id == id)
            .then_some((Some(id), None)),
        Target::Window(_) | Target::Pane(_) => snapshot.sessions.iter().find_map(|session| {
            session.windows.iter().find_map(|mux_window| match target {
                Target::Window(id) if mux_window.id == id => {
                    Some((Some(session.id), Some(mux_window.active_pane)))
                }
                Target::Pane(pane) if mux_window.panes.contains_key(&pane) => {
                    Some((Some(session.id), Some(pane)))
                }
                _ => None,
            })
        }),
    };
    if let Some((session, pane)) = place {
        view.switch_host(endpoint, session, pane, window, cx);
    }
}

fn leave_abandoned_picker(connection: &Entity<Connection>, target: Target, cx: &mut App) {
    connection.update(cx, |connection, cx| {
        if !writable(connection) {
            return;
        }
        let command = leave_picker_command(
            connection.core.snapshot(),
            connection.core.attached_session(),
            target,
        );
        if let Some(command) = command {
            connection.invoke(command, cx);
        }
    });
}

pub(super) fn set_chat_view(view: &mut AppShell, on: bool, cx: &mut Context<AppShell>) {
    if view.preferences.flat_sidebar == on {
        return;
    }
    view.preferences.flat_sidebar = on;
    view.preferences.save();
    let connection = view.connection.clone();
    let open = {
        let connection = connection.read(cx);
        let (snapshot, attached) = (
            connection.core.snapshot(),
            connection.core.attached_session(),
        );
        if on {
            view.flat
                .place
                .enter(snapshot, attached)
                .filter(|_| writable(connection))
        } else {
            view.flat.place.leave(snapshot, attached);
            None
        }
    };
    if let Some(window) = open {
        activate(&connection, Target::Window(window), cx);
    }
}

fn disclosure_id(node: Node) -> String {
    format!("web-tree-disclosure-{}", target_id(node))
}

fn row_opened(rows: &[Row], index: usize, cx: &mut App) -> Option<Instant> {
    let mut depth = rows.get(index)?.row.depth;
    let mut opened = None;
    for row in rows[..index].iter().rev() {
        if row.row.depth < depth {
            opened = opened.max(workspace_tree_opened(disclosure_id(row.node()), cx));
            depth = row.row.depth;
        }
    }
    opened
}

fn render_row(
    entry: &Row,
    active: bool,
    opened: Option<Instant>,
    runtime: &Runtime,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    let connected = entry.connected(cx);
    let disabled = !entry
        .connection
        .as_ref()
        .is_some_and(|connection| writable(connection.read(cx)));
    let node = entry.node();
    let (host, target) = node;
    let attached = entry.attached;
    let id = target_id(node);
    let group = format!("web-tree-group-{id}");
    let is_host = matches!(target, Target::Host);
    let selected = runtime.focused && runtime.selected == Some(node);
    let ink = WorkspaceTreeInk::new(active, selected, runtime.focused, runtime.ground, cx);
    let color = ink.node(is_host || connected && entry.row.on_active_path);
    let mut actions = Vec::new();
    if let Some((label, command)) = add_command(target, attached) {
        let icon = if is_host {
            IconName::LayersPlus
        } else {
            IconName::WindowPlus
        };
        let connection = entry.connection.clone();
        actions.push(
            workspace_tree_action_button(
                format!("web-tree-add-{id}"),
                icon,
                label,
                disabled,
                ink.ground,
                cx,
            )
            .on_click(move |_, _, cx| {
                if let Some(connection) = &connection {
                    connection.update(cx, |connection, cx| {
                        connection.invoke(command.clone(), cx);
                    });
                }
                cx.stop_propagation();
            })
            .into_any_element(),
        );
    }

    if let Some((label, command)) = sidebar_close_command(target) {
        let connection = entry.connection.clone();
        actions.push(
            workspace_tree_action_button(
                format!("web-tree-close-{id}"),
                IconName::Xmark,
                label,
                disabled,
                ink.ground,
                cx,
            )
            .on_click(move |_, _, cx| {
                if let Some(connection) = &connection {
                    connection.update(cx, |connection, cx| {
                        connection.invoke(command.clone(), cx);
                    });
                }
                cx.stop_propagation();
            })
            .into_any_element(),
        );
    }
    if is_host && !connected {
        let status = entry
            .connection
            .as_ref()
            .map(|connection| connection.read(cx).status.clone());
        actions.insert(
            0,
            tree_host_indicator(
                format!("web-tree-host-indicator-{id}"),
                status.as_deref() == Some("Connecting…"),
                status.map(Into::into),
                cx,
            ),
        );
    }
    let actions = tree_action_strip(format!("web-tree-actions-{id}"), actions, cx);
    let badge_color = entry.row.badge.map(|badge| badge_color(badge, cx));
    let marker = if is_host {
        tree_host_marker(
            entry
                .connection
                .as_ref()
                .map_or(HostPlatform::Unknown, |connection| {
                    connection.read(cx).core.host_platform()
                }),
            color,
            entry.row.bell,
            badge_color,
            ink.ground,
            cx,
        )
    } else {
        tree_node_marker(
            entry
                .row
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
                    Icon::new(row_icon(entry.row.node))
                        .medium()
                        .text_color(color)
                        .into_any_element()
                }),
            entry.row.bell,
            badge_color,
            ink.ground,
            cx,
        )
    };
    let marker = if entry.row.expandable {
        let view = runtime.view.clone();
        workspace_tree_disclosure(
            disclosure_id(node),
            marker,
            entry.row.expanded,
            group.clone().into(),
            ink,
        )
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            view.update(cx, |view, cx| {
                view.sidebar_pointer_selection = true;
                toggle(view, node, cx);
            });
            cx.stop_propagation();
        })
        .into_any_element()
    } else {
        marker
    };
    let connection = entry.connection.clone();
    let focus = runtime.focus.clone();
    let view = runtime.view.clone();
    let row = workspace_tree_row(
        format!("web-tree-row-{id}"),
        entry.row.depth,
        active,
        selected,
        runtime.focused,
        connected,
        is_host || entry.row.expandable || connected,
        !is_host,
        group.into(),
        marker,
        div()
            .flex()
            .min_w_0()
            .items_baseline()
            .gap_2()
            .text_color(color)
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .child(entry.row.label.clone()),
            )
            .children(
                entry
                    .row
                    .index
                    .clone()
                    .map(|index| div().flex_none().text_color(ink.faint).child(index)),
            ),
        actions,
        runtime.ground,
        cx,
    )
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        focus.focus(window, cx);
    })
    .on_click(move |_, window, cx| {
        let live = connection
            .as_ref()
            .is_some_and(|connection| connection.read(cx).connected);
        view.update(cx, |view, cx| {
            view.sidebar_selection = Some(node);
            view.sidebar_pointer_selection = true;
            if is_host && attached && live {
                toggle(view, node, cx);
            } else {
                if view.slideover && !is_host && live {
                    view.release_sidebar_focus(window, cx);
                }
                view.open_on_host(host, target, window, cx);
            }
            cx.notify();
        });
        cx.stop_propagation();
    });
    let row = penna::foundation::motion::enter_since(row, opened, window, cx);
    let rename = entry
        .connection
        .as_ref()
        .filter(|_| !disabled)
        .and_then(|connection| rename_command(connection.read(cx).core.snapshot(), target));
    if let Some((label, _)) = rename {
        let view = runtime.view.clone();
        tree_row_menu(
            row,
            vec![TreeRowMenuItem::new(label, move |window, cx| {
                view.update(cx, |view, cx| view.rename_on(host, target, window, cx));
            })],
        )
    } else {
        row.into_any_element()
    }
}
