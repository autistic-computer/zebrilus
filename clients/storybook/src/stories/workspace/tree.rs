use zz_ui::sidebar::{SIDEBAR_CONTENT_INSET, SIDEBAR_DEFAULT_WIDTH, sidebar_footer, sidebar_titlebar};
use std::rc::Rc;

use zz_client::program::Program;
use ala::{
    AnyElement, App, Hsla, IntoElement, Keystroke, ListSizingBehavior, ParentElement as _,
    Stateful, Styled as _, UniformListScrollHandle, div, prelude::*, px, uniform_list,
};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Icon, IconName,
    navigation::{WORKSPACE_SIDEBAR_FADE_HEIGHT, WORKSPACE_TREE_INDENT_WIDTH, WORKSPACE_TREE_MARKER_SLOT_WIDTH, WORKSPACE_TREE_NODE_ICON_SIZE, sidebar::{
            TreeRowMenuItem, tree_action_strip, tree_host_indicator, tree_host_marker,
            tree_node_marker, tree_row_menu,
        }, tree::{IndentGuideColors, WorkspaceIndentGuides}, workspace_add_host_button, workspace_layout_button, workspace_settings_button, workspace_sidebar_footer_controls, workspace_sidebar_surface, workspace_sidebar_view_button, workspace_tree_action_button, workspace_tree_disclosure, workspace_tree_row, workspace_tree_shortcut},
    program::program_mark,
    scroll::ScrollableElement as _,
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Badge {
    NeedsInput,
    Failed,
    Working,
    Finished,
}

impl Badge {
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            Self::NeedsInput => cx.theme().warning,
            Self::Failed => cx.theme().danger,
            Self::Working => cx.theme().foreground.muted(),
            Self::Finished => cx.theme().success,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Host,
    Session,
    Window,
    Pane,
}

#[derive(Clone)]
pub struct Node {
    pub key: &'static str,
    pub kind: Kind,
    pub depth: u8,
    pub icon: IconName,
    pub label: &'static str,
    pub on_active_path: bool,
    pub expandable: bool,
    pub expanded: bool,
    pub bell: bool,
    pub badge: Option<Badge>,
    pub program: Option<&'static str>,
    pub shortcut: Option<u8>,
}

impl Node {
    const fn new(key: &'static str, kind: Kind, icon: IconName, label: &'static str) -> Self {
        let depth = match kind {
            Kind::Host => 0,
            Kind::Session => 1,
            Kind::Window => 2,
            Kind::Pane => 3,
        };
        Self {
            key,
            kind,
            depth,
            icon,
            label,
            on_active_path: false,
            expandable: !matches!(kind, Kind::Pane),
            expanded: true,
            bell: false,
            badge: None,
            program: None,
            shortcut: None,
        }
    }

    const fn running(mut self, program: &'static str) -> Self {
        self.program = Some(program);
        self
    }

    pub fn shown_program(&self) -> Option<&'static Program> {
        let shown = match self.kind {
            Kind::Pane => true,
            Kind::Window => !self.expanded,
            Kind::Host | Kind::Session => false,
        };
        self.program.filter(|_| shown).and_then(Program::by_id)
    }

    const fn active_path(mut self) -> Self {
        self.on_active_path = true;
        self
    }

    const fn collapsed(mut self) -> Self {
        self.expanded = false;
        self
    }

    const fn bell(mut self) -> Self {
        self.bell = true;
        self
    }

    const fn badge(mut self, badge: Badge) -> Self {
        self.badge = Some(badge);
        self
    }
}

pub fn workspace_tree() -> Vec<Node> {
    vec![
        Node::new("host", Kind::Host, IconName::HardDrive, "zz daemon")
            .active_path()
            .bell()
            .badge(Badge::NeedsInput),
        Node::new("main", Kind::Session, IconName::Layers, "main")
            .active_path()
            .bell()
            .badge(Badge::NeedsInput),
        Node::new("editor", Kind::Window, IconName::AppWindow, "editor")
            .active_path()
            .badge(Badge::NeedsInput)
            .running("neovim"),
        Node::new("nvim", Kind::Pane, IconName::SquareTerminal, "nvim")
            .active_path()
            .running("neovim"),
        Node::new("review", Kind::Pane, IconName::Claude, "Review sidebar")
            .badge(Badge::NeedsInput)
            .running("claude"),
        Node::new("docs", Kind::Pane, IconName::Globe, "zed.dev"),
        Node::new("shell", Kind::Pane, IconName::SquareTerminal, "zsh"),
        Node::new("agents", Kind::Window, IconName::AppWindow, "agents")
            .badge(Badge::Failed)
            .running("codex"),
        Node::new("rework", Kind::Pane, IconName::Openai, "Rework status bar")
            .badge(Badge::Working)
            .running("codex"),
        Node::new(
            "flaky",
            Kind::Pane,
            IconName::Claude,
            "Fix flaky daemon test",
        )
        .badge(Badge::Failed)
        .running("claude"),
        Node::new("notes", Kind::Pane, IconName::Claude, "Write release notes")
            .badge(Badge::Finished)
            .running("claude"),
        Node::new("build", Kind::Window, IconName::AppWindow, "build")
            .collapsed()
            .running("rust"),
        Node::new("logs", Kind::Window, IconName::AppWindow, "logs")
            .collapsed()
            .bell()
            .running("docker"),
        Node::new("scratch", Kind::Session, IconName::Layers, "scratch").collapsed(),
    ]
}

pub fn numbered(mut nodes: Vec<Node>) -> Vec<Node> {
    for (node, key) in nodes
        .iter_mut()
        .filter(|node| node.kind == Kind::Window)
        .zip(1..=9)
    {
        node.shortcut = Some(key);
    }
    nodes
}

pub fn host_only() -> Vec<Node> {
    vec![Node::new("host", Kind::Host, IconName::HardDrive, "zz daemon").collapsed()]
}

#[derive(Clone, Copy)]
pub struct TreeState {
    pub connected: bool,
    pub connecting: bool,
    pub focused: bool,
    pub selected: Option<&'static str>,
}

impl TreeState {
    pub const CONNECTED: Self = Self {
        connected: true,
        connecting: false,
        focused: false,
        selected: None,
    };
}

pub fn render_node(node: &Node, active: bool, state: TreeState, cx: &App) -> AnyElement {
    let key = node.key;
    let group = format!("tree-group-{key}");
    let is_host = node.kind == Kind::Host;
    let disabled = !state.connected;
    let color = if is_host || state.connected && node.on_active_path {
        cx.theme().foreground
    } else {
        cx.theme().foreground.muted()
    };
    let mut actions = Vec::new();
    match node.kind {
        Kind::Host => actions.push(
            workspace_tree_action_button(
                format!("tree-add-{key}"),
                IconName::LayersPlus,
                "New session",
                disabled,
                cx,
            )
            .into_any_element(),
        ),
        Kind::Session => actions.push(
            workspace_tree_action_button(
                format!("tree-add-{key}"),
                IconName::WindowPlus,
                "New window",
                disabled,
                cx,
            )
            .into_any_element(),
        ),
        Kind::Window | Kind::Pane => {}
    }
    if !is_host {
        let label = match node.kind {
            Kind::Session => "Close session",
            Kind::Window => "Close window",
            _ => "Close pane",
        };
        actions.push(
            workspace_tree_action_button(
                format!("tree-close-{key}"),
                IconName::Xmark,
                label,
                disabled,
                cx,
            )
            .into_any_element(),
        );
    }
    if is_host && !state.connected {
        actions.insert(
            0,
            tree_host_indicator(
                "tree-host-indicator",
                state.connecting,
                Some(if state.connecting {
                    "Connecting…".into()
                } else {
                    "Connection lost: devbox closed the ssh session".into()
                }),
                cx,
            ),
        );
    }
    let actions = tree_action_strip(format!("tree-actions-{key}"), actions, cx);
    let badge = node.badge.map(|badge| badge.color(cx));
    let marker = if is_host {
        tree_host_marker(node.bell, badge, cx)
    } else {
        let icon = node
            .shown_program()
            .and_then(|program| {
                program_mark(program, px(WORKSPACE_TREE_NODE_ICON_SIZE), color, None)
            })
            .unwrap_or_else(|| {
                Icon::new(node.icon.clone())
                    .medium()
                    .text_color(color)
                    .into_any_element()
            });
        tree_node_marker(icon, node.bell, badge, cx)
    };
    let marker = if node.expandable {
        workspace_tree_disclosure(
            format!("tree-disclosure-{key}"),
            marker,
            node.expanded,
            group.clone().into(),
            cx,
        )
        .into_any_element()
    } else {
        marker
    };
    let row = workspace_tree_shortcut(
        workspace_tree_row(
            format!("tree-row-{key}"),
            node.depth,
            active,
            state.focused && state.selected == Some(key),
            state.focused,
            state.connected,
            node.expandable || state.connected && !is_host,
            !is_host,
            group.into(),
            marker,
            div().min_w_0().text_color(color).child(node.label),
            actions,
            cx.theme().ground(),
            cx,
        ),
        node.shortcut
            .and_then(|key| Keystroke::parse(&format!("secondary-{key}")).ok()),
        cx.theme().ground(),
    );
    let rename = match node.kind {
        Kind::Host => None,
        Kind::Session => Some("Rename session…"),
        Kind::Window | Kind::Pane => Some("Rename window…"),
    };
    match rename {
        Some(label) if state.connected => {
            tree_row_menu(row, vec![TreeRowMenuItem::new(label, |_, _| {})])
        }
        _ => row.into_any_element(),
    }
}

pub fn guides(depths: Rc<[usize]>, active_row: Option<usize>, cx: &App) -> WorkspaceIndentGuides {
    WorkspaceIndentGuides::new(
        depths,
        active_row,
        px(WORKSPACE_TREE_INDENT_WIDTH),
        px(SIDEBAR_CONTENT_INSET + WORKSPACE_TREE_MARKER_SLOT_WIDTH / 2.0),
        px(4.0),
        IndentGuideColors::tree(cx),
    )
}

pub fn tree_list(
    nodes: Vec<Node>,
    state: TreeState,
    scroll: &UniformListScrollHandle,
    cx: &App,
) -> AnyElement {
    let active_row = state
        .connected
        .then(|| nodes.iter().rposition(|node| node.on_active_path))
        .flatten();
    let depths: Rc<[usize]> = nodes.iter().map(|node| usize::from(node.depth)).collect();
    let count = nodes.len();
    let list = uniform_list("tree-rows", count, move |range, _, cx| {
        range
            .map(|index| render_node(&nodes[index], active_row == Some(index), state, cx))
            .collect::<Vec<_>>()
    })
    .size_full()
    .pr(zz_ui::sidebar::SIDEBAR_SCROLLBAR_GUTTER)
    .with_sizing_behavior(ListSizingBehavior::Auto)
    .track_scroll(scroll)
    .with_decoration(guides(depths, active_row, cx));
    div()
        .id("tree")
        .relative()
        .size_full()
        .min_h_0()
        .child(list)
        .bottom_fade(scroll, cx.theme().background, WORKSPACE_SIDEBAR_FADE_HEIGHT)
        .vertical_scrollbar(scroll)
        .into_any_element()
}

pub fn chrome_controls() -> zz_ui::button::Button {
    workspace_layout_button("layout")
}

pub fn sidebar(
    navigation: impl IntoElement,
    settings_selected: bool,
    cx: &App,
) -> Stateful<ala::Div> {
    sidebar_in_view(navigation, settings_selected, false, cx)
}

pub fn flat_view_sidebar(navigation: impl IntoElement, cx: &App) -> Stateful<ala::Div> {
    sidebar_in_view(navigation, false, true, cx)
}

fn sidebar_in_view(
    navigation: impl IntoElement,
    settings_selected: bool,
    flat: bool,
    cx: &App,
) -> Stateful<ala::Div> {
    workspace_sidebar_surface(
        "sidebar",
        SIDEBAR_DEFAULT_WIDTH,
        sidebar_titlebar("sidebar-titlebar", chrome_controls(), cx),
        navigation,
        cx,
    )
    .when(!settings_selected, |surface| {
        surface.child(sidebar_footer(
            "sidebar-footer",
            workspace_settings_button("settings"),
            Some(
                workspace_sidebar_footer_controls(
                    workspace_add_host_button("add-host"),
                    workspace_sidebar_view_button("sidebar-view", flat),
                )
                .into_any_element(),
            ),
            cx,
        ))
    })
}
