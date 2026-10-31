use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, Hsla, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, prelude::*, px, white,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, IconName,
    button::{Button, ButtonVariants as _},
    compact::{
        BENTO_INSET, BentoSpan, COMPACT_PILL_HEIGHT, PillLabel, PillMenuAlign, PillMenuItem,
        PillMenuLook, PillPlacement, bento_add_tile, bento_grid, compact_bar_button,
        compact_floating_bar, compact_floating_group, compact_floating_pill, compact_hud,
        compact_pill_menu, folding_title_bar, pill_glass_over,
    },
    settings::large_title,
    touch::{
        ArrowPad, KEY_ROW_HEIGHT, KeyRow, PageDot, PopoverKey, PopoverKeyTap, WhichKeyList,
        page_dots,
    },
    v_flex,
    which_key::WhichKeyRow,
};
use zz_protocol::AgentProvider;
use zz_ui::{
    compact::{
        HostState, ToolKeys, host_tile, host_tile_line, mini_pane, overview_host_empty,
        overview_session_heading, window_layout, window_span, window_tile,
    },
    pane::agent_provider_icon,
};

use crate::stories::agent::lens;
use crate::story::Framed as _;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "phone",
    name: "Phone",
    group: "Phone",
    summary: "The compact pieces of the phone shell, at phone width: the pill, its menu, page dots, the HUD, the key row and the bindings list.",
    sections: &[
        Section {
            id: "bar",
            name: "Pill",
            summary: "The one control at the bottom of a pane, with no header above the page: swipe it sideways to page panes, swipe it up to go back to the host's windows, tap it for the menu. It is set in from a 55pt screen corner so it runs concentric with it, and turns amber when the page needs attention.",
            build: |_, cx| stateless(bars, cx),
        },
        Section {
            id: "page-dots",
            name: "Page dots",
            summary: "One group per window, one dot per pane. The current pane is the wide dot; a pane asking for attention is amber.",
            build: |_, cx| stateless(dots, cx),
        },
        Section {
            id: "pill-menu",
            name: "Pill menu",
            summary: "What tapping the pill, or the prefix key over the keyboard, opens, the way Calendar's + rises into its actions: the page dims, the pill turns into a close button and the actions stack above it. Overview sits nearest the thumb; Kill pane and All bindings sit furthest. A pane running Claude Code or Codex adds Lens view, which reads Terminal view while the lens is open. On a host's page the pill opens what + and the chat did: a new terminal, browser or session on that host (only while it is connected), All hosts and Settings.",
            build: |_, cx| cx.new(Menus::new).into(),
        },
        Section {
            id: "hud",
            name: "HUD",
            summary: "The transient message centred over the page.",
            build: |_, cx| stateless(huds, cx),
        },
        Section {
            id: "key-row",
            name: "Key row",
            summary: "The row above the soft keyboard: the prefix key on its own, then ctrl and alt popover keys, plain keys, the arrow pad and hide in a row that scrolls sideways when it does not fit. A fade and a chevron mark the side with more keys; tapping them scrolls there.",
            build: |_, cx| cx.new(KeyRows::new).into(),
        },
        Section {
            id: "keys",
            name: "Popover keys",
            summary: "The prefix key at rest and armed, as when the prefix is waiting for its next key, a popover key with nothing behind it, and the arrow pad at rest.",
            build: |_, cx| cx.new(Keys::new).into(),
        },
        Section {
            id: "bindings",
            name: "Bindings list",
            summary: "WhichKeyList: the prefix table as a touch list, grouped, your own keys in the accent color.",
            build: |_, cx| stateless(bindings, cx),
        },
        Section {
            id: "overview",
            name: "Overview",
            summary: "Three levels you zoom through, the way Calendar goes from year to month to day. Hosts lists every host as a tile, with settings in the bar. Tapping a host zooms into its page: each session's windows as a bento grid, every window drawn the way it splits its panes, the one it opens to ringed, over the host pill alone. Swiping the pill sideways pages hosts, swiping it up zooms back out to Hosts, and tapping it opens the host's menu. Tapping a window zooms into the pane it was last on. The large title folds into a frosted bar once the page scrolls it away; a host that is not connected shows why and one thing to do.",
            build: |_, cx| stateless(overview, cx),
        },
        Section {
            id: "screen",
            name: "Phone screen",
            summary: "The pieces together: a terminal page under the status bar, the key row and the pill.",
            build: |_, cx| cx.new(Screen::new).into(),
        },
        Section {
            id: "lens",
            name: "Agent lens",
            summary: "A terminal running Claude Code or Codex on the phone opens in the lens, since typing into a TUI with a phone keyboard is slow. Lens view and Terminal view in the pill's menu switch between them; switching to the terminal keeps that pane there until the agent exits. While the lens is open the page drops the key row the way a browser page does, and the composer types into the TUI.",
            build: phone_lens,
        },
    ],
};

const PHONE_WIDTH: f32 = 390.0;

fn phone(content: impl IntoElement, cx: &App) -> ala::Div {
    div()
        .w(px(PHONE_WIDTH))
        .bg(cx.theme().background)
        .framed(cx)
        .rounded(cx.theme().radius)
        .overflow_hidden()
        .child(content)
}

fn dot(active: bool, attention: bool) -> PageDot {
    PageDot { active, attention }
}

pub(crate) fn groups() -> Vec<Vec<PageDot>> {
    vec![
        vec![dot(false, false), dot(true, false)],
        vec![dot(false, false), dot(false, true), dot(false, false)],
        vec![dot(false, false)],
    ]
}

const SCROLLED: f32 = 72.0;
const SCROLLED_HEIGHT: f32 = 380.0;

pub(crate) fn placement() -> PillPlacement {
    PillPlacement::new(Some(px(55.0)), px(34.0))
}

fn bar(
    id: &'static str,
    title: &str,
    meta: &str,
    warning: bool,
    page: Option<Hsla>,
    cx: &App,
) -> impl IntoElement {
    let placement = PillPlacement {
        glass: page.and_then(|page| pill_glass_over(page, cx)),
        ..placement()
    };
    let pill = compact_floating_pill(
        id,
        PillLabel {
            icon: if warning {
                IconName::TriangleAlert
            } else {
                IconName::SquareTerminal
            },
            title: title.to_owned().into(),
            meta: meta.to_owned().into(),
            warning,
        },
        page_dots(&groups(), cx),
        &placement,
        cx,
    );
    phone(compact_floating_bar(pill, &placement, cx), cx).when_some(page, Styled::bg)
}

fn bars(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "rest",
            bar("bar-rest", "cargo watch", "zz · editor", false, None, cx),
        )
        .state(
            "attention",
            bar(
                "bar-warning",
                "python train.py",
                "exited with 1 · gpu-box",
                true,
                None,
                cx,
            ),
        )
        .state(
            "long title",
            bar(
                "bar-long",
                "claude · reviewing crates/zz-ui/src/command/palette_view.rs",
                "zz · agents · 2 of 4 panes",
                false,
                None,
                cx,
            ),
        )
        .state(
            "over a white page",
            bar(
                "bar-page",
                "Google",
                "www.google.com",
                false,
                Some(white()),
                cx,
            ),
        )
        .into_any_element()
}

fn dots(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state("one pane", page_dots(&[vec![dot(true, false)]], cx))
        .state(
            "one window, four panes",
            page_dots(
                &[vec![
                    dot(false, false),
                    dot(false, false),
                    dot(true, false),
                    dot(false, false),
                ]],
                cx,
            ),
        )
        .state("three windows", page_dots(&groups(), cx))
        .state(
            "attention in every window",
            page_dots(
                &[
                    vec![dot(true, false), dot(false, true)],
                    vec![dot(false, true)],
                    vec![dot(false, false), dot(false, true)],
                ],
                cx,
            ),
        )
        .into_any_element()
}

pub(crate) fn host_bar(
    id: &'static str,
    title: &str,
    meta: &str,
    page: usize,
    cx: &App,
) -> AnyElement {
    let placement = placement();
    let pill = compact_floating_pill(
        id,
        PillLabel {
            icon: IconName::Server,
            title: title.to_owned().into(),
            meta: meta.to_owned().into(),
            warning: false,
        },
        page_dots(
            &[(0..3).map(|index| dot(index == page, false)).collect()],
            cx,
        ),
        &placement,
        cx,
    );
    compact_floating_bar(pill, &placement, cx).into_any_element()
}

pub(crate) fn hosts_bar(cx: &App) -> AnyElement {
    let placement = placement();
    let settings = compact_bar_button("hosts-settings", IconName::Settings);
    compact_floating_bar(div().flex_1(), &placement, cx)
        .child(compact_floating_group([settings], &placement, cx))
        .into_any_element()
}

const GRID: f32 = PHONE_WIDTH - BENTO_INSET * 2.0;

fn subtitle(text: &str, cx: &App) -> impl IntoElement {
    div()
        .px(px(BENTO_INSET))
        .pb(px(16.0))
        .text_size(px(15.0))
        .line_height(px(20.0))
        .text_color(cx.theme().foreground.muted())
        .child(text.to_owned())
}

pub(crate) fn hosts_page(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let tiles = vec![
        (
            BentoSpan::new(2, 2),
            host_tile(
                "hosts-macbook",
                "hosts-macbook-press",
                HostState::Connected,
                "macbook",
                "2 sessions · 8 panes",
                vec![
                    host_tile_line(theme.warning, "1 waiting", true, cx).into_any_element(),
                    host_tile_line(theme.success, "2 working", false, cx).into_any_element(),
                ],
                cx,
            )
            .into_any_element(),
        ),
        (
            BentoSpan::new(2, 2),
            host_tile(
                "hosts-xps",
                "hosts-xps-press",
                HostState::Connected,
                "xps",
                "1 session · 3 panes",
                vec![host_tile_line(theme.success, "1 working", false, cx).into_any_element()],
                cx,
            )
            .into_any_element(),
        ),
        (
            BentoSpan::new(2, 2),
            host_tile(
                "hosts-alienware",
                "hosts-alienware-press",
                HostState::Offline,
                "alienware",
                "Not connected",
                vec![
                    Button::new("hosts-alienware-connect")
                        .label("Connect")
                        .into_any_element(),
                ],
                cx,
            )
            .into_any_element(),
        ),
        (
            BentoSpan::new(2, 2),
            host_tile(
                "hosts-ubuntu",
                "hosts-ubuntu-press",
                HostState::Connecting,
                "ubuntu",
                "Connecting…",
                vec![
                    Button::new("hosts-ubuntu-cancel")
                        .label("Cancel")
                        .ghost()
                        .into_any_element(),
                ],
                cx,
            )
            .into_any_element(),
        ),
        (
            BentoSpan::new(4, 1),
            bento_add_tile("hosts-add", "Add host", cx).into_any_element(),
        ),
    ];
    v_flex()
        .pb(px(16.0))
        .child(large_title("Hosts", cx))
        .child(subtitle("4 hosts · 2 connected", cx))
        .child(div().px(px(BENTO_INSET)).child(bento_grid(px(GRID), tiles)))
}

fn preview(lines: &str, cx: &App) -> AnyElement {
    div()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(5.0))
        .line_height(px(6.5))
        .whitespace_nowrap()
        .text_color(cx.theme().foreground.muted())
        .children(lines.lines().map(|line| div().child(line.to_owned())))
        .into_any_element()
}

fn mini(
    icon: IconName,
    title: &str,
    lines: &str,
    current: bool,
    attention: bool,
    cx: &App,
) -> AnyElement {
    mini_pane(
        icon,
        (!title.is_empty()).then(|| title.to_owned().into()),
        (!lines.is_empty()).then(|| preview(lines, cx)),
        cx.theme().background,
        current,
        attention,
        cx,
    )
    .into_any_element()
}

pub(crate) fn host_page(cx: &App) -> impl IntoElement {
    let daemon = window_tile(
        "window-daemon",
        "window-daemon-press",
        1,
        "daemon",
        Some("~/dev/zz".into()),
        false,
        window_layout(vec![
            (
                [0.0, 0.0, 0.6, 1.0],
                mini(
                    IconName::SquareTerminal,
                    "cargo test",
                    "running 214 tests\ntest overview::pager ... ok\ntest overview::bento ... ok\ntest result: ok. 214 passed",
                    true,
                    false,
                    cx,
                ),
            ),
            (
                [0.6, 0.0, 0.4, 0.5],
                mini(
                    IconName::File,
                    "overview.rs",
                    "fn host_body(\n  &mut self,",
                    false,
                    false,
                    cx,
                ),
            ),
            (
                [0.6, 0.5, 0.4, 0.5],
                mini(IconName::SquareTerminal, "perf audit", "", false, false, cx),
            ),
        ]),
        cx,
    );
    let site = window_tile(
        "window-site",
        "window-site-press",
        2,
        "site",
        Some("~/dev/zz/site".into()),
        false,
        window_layout(vec![(
            [0.0, 0.0, 1.0, 1.0],
            mini(IconName::Globe, "", "localhost:4321", false, false, cx),
        )]),
        cx,
    );
    let agents = window_tile(
        "window-agents",
        "window-agents-press",
        3,
        "agents",
        None,
        true,
        window_layout(vec![
            (
                [0.0, 0.0, 0.5, 1.0],
                mini(IconName::SquareTerminal, "review", "", true, true, cx),
            ),
            (
                [0.5, 0.0, 0.5, 1.0],
                mini(IconName::SquareTerminal, "fix ci", "", false, false, cx),
            ),
        ]),
        cx,
    );
    let notes = window_tile(
        "window-notes",
        "window-notes-press",
        1,
        "notes",
        None,
        false,
        window_layout(vec![(
            [0.0, 0.0, 1.0, 1.0],
            mini(
                IconName::SquareTerminal,
                "",
                "- [ ] fold title on iPad\n- [x] bento grid",
                false,
                false,
                cx,
            ),
        )]),
        cx,
    );
    v_flex()
        .pb(px(8.0))
        .child(large_title("macbook", cx))
        .child(overview_session_heading("zz", cx))
        .child(div().px(px(BENTO_INSET)).child(bento_grid(
            px(GRID),
            vec![
                (window_span(3), daemon.into_any_element()),
                (window_span(1), site.into_any_element()),
                (window_span(2), agents.into_any_element()),
            ],
        )))
        .child(overview_session_heading("scratch", cx))
        .child(div().px(px(BENTO_INSET)).child(bento_grid(
            px(GRID),
            vec![(window_span(1), notes.into_any_element())],
        )))
}

fn empty_page(cx: &App, action: AnyElement, detail: &str) -> impl IntoElement {
    div().h(px(320.0)).child(overview_host_empty(
        "alienware",
        detail.to_owned(),
        Some(action),
        cx,
    ))
}

fn overview(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "hosts, where you start",
            phone(v_flex().child(hosts_page(cx)).child(hosts_bar(cx)), cx),
        )
        .state(
            "a connected host",
            phone(
                v_flex()
                    .child(host_page(cx))
                    .child(host_bar("overview-bar-macbook", "macbook", "2 sessions", 0, cx)),
                cx,
            ),
        )
        .state(
            "scrolled, the title folded into the bar",
            phone(
                v_flex()
                    .child(
                        div()
                            .relative()
                            .h(px(SCROLLED_HEIGHT))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-SCROLLED))
                                    .left_0()
                                    .right_0()
                                    .child(host_page(cx)),
                            )
                            .child(folding_title_bar("macbook", SCROLLED, px(0.0), cx)),
                    )
                    .child(host_bar("overview-bar-scrolled", "macbook", "2 sessions", 0, cx)),
                cx,
            ),
        )
        .state(
            "a host that is not connected",
            phone(
                v_flex()
                    .child(empty_page(
                        cx,
                        Button::new("overview-connect").label("Connect").into_any_element(),
                        "fabri@alienware is not connected. Your other hosts stay connected while it signs in.",
                    ))
                    .child(host_bar(
                        "overview-bar-alienware",
                        "alienware",
                        "Not connected",
                        2,
                        cx,
                    )),
                cx,
            ),
        )
        .state(
            "signing in",
            phone(
                v_flex()
                    .child(empty_page(
                        cx,
                        Button::new("overview-cancel")
                            .label("Cancel")
                            .ghost()
                            .into_any_element(),
                        "Connecting…",
                    ))
                    .child(host_bar("overview-bar-ubuntu", "ubuntu", "Connecting…", 1, cx)),
                cx,
            ),
        )
        .into_any_element()
}

const MENU_HEIGHT: f32 = 700.0;
const MENU_TOP: f32 = 12.0;
const PREFIX_AT: (f32, f32) = (12.0, 8.0);
const PREFIX_SIZE: f32 = 40.0;

fn menu_items(lens: Option<bool>) -> Vec<Vec<PillMenuItem>> {
    let mut panes = vec![
        PillMenuItem::new("rename-pane", "Rename pane", IconName::Pencil),
        PillMenuItem::new("new-window", "New window", IconName::AppWindow),
        PillMenuItem::new("new-pane", "New pane", IconName::Plus),
        PillMenuItem::new("last-pane", "Last pane", IconName::History),
        PillMenuItem::new("overview", "Overview", IconName::LayoutDashboard),
    ];
    if let Some(open) = lens {
        let (label, icon) = if open {
            ("Terminal view", IconName::SquareTerminal)
        } else {
            ("Lens view", IconName::Eye)
        };
        panes.push(PillMenuItem::new("lens", label, icon).look(PillMenuLook::Primary));
    }
    vec![
        vec![
            PillMenuItem::new("all-bindings", "All bindings", IconName::Keyboard)
                .look(PillMenuLook::Quiet),
            PillMenuItem::new("kill-pane", "Kill pane", IconName::Xmark).look(PillMenuLook::Danger),
        ],
        panes,
    ]
}

fn host_menu_items() -> Vec<Vec<PillMenuItem>> {
    vec![
        vec![
            PillMenuItem::new("settings", "Settings", IconName::Settings).look(PillMenuLook::Quiet),
        ],
        vec![
            PillMenuItem::new("new-session", "New session", IconName::Layers),
            PillMenuItem::new("new-browser", "New browser", IconName::Globe),
            PillMenuItem::new("new-terminal", "New terminal", IconName::SquareTerminal),
            PillMenuItem::new("all-hosts", "All hosts", IconName::Server),
        ],
    ]
}

fn screen_lines(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .px(px(12.0))
        .py(px(8.0))
        .font_family(theme.mono_font_family.clone())
        .text_size(px(12.0))
        .line_height(px(17.0))
        .text_color(theme.foreground)
        .children(
            SCREEN_LINES
                .iter()
                .map(|line| div().h(px(17.0)).child(*line)),
        )
}

struct Menus {
    keys: ToolKeys,
}

impl Menus {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            keys: tool_keys(cx),
        }
    }

    fn from_pill(items: Vec<Vec<PillMenuItem>>, cx: &App) -> impl IntoElement {
        let placement = placement();
        let top = px(MENU_HEIGHT) - placement.zone() + placement.pill_top();
        let close = ala::Bounds::new(
            ala::point(placement.inset, top),
            ala::size(
                px(PHONE_WIDTH) - placement.inset * 2.0,
                px(COMPACT_PILL_HEIGHT),
            ),
        );
        phone(
            div()
                .relative()
                .h(px(MENU_HEIGHT))
                .child(screen_lines(cx))
                .child(compact_pill_menu(
                    items,
                    close,
                    px(MENU_TOP),
                    PillMenuAlign::Center,
                    1.0,
                    |_, _, _| {},
                    |_, _| {},
                    cx,
                )),
            cx,
        )
    }
}

impl Render for Menus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (x, y) = PREFIX_AT;
        let row_top = MENU_HEIGHT - KEY_ROW_HEIGHT;
        let prefix = ala::Bounds::new(
            ala::point(px(x), px(row_top + y)),
            ala::size(px(PREFIX_SIZE), px(PREFIX_SIZE)),
        );
        states()
            .columns(2)
            .state("tapping the pill", Self::from_pill(menu_items(None), cx))
            .state(
                "on a Claude Code pane: Lens view",
                Self::from_pill(menu_items(Some(false)), cx),
            )
            .state("on a host's page", Self::from_pill(host_menu_items(), cx))
            .state(
                "tapping the prefix key over the keyboard",
                phone(
                    div()
                        .id("menu-prefix")
                        .relative()
                        .h(px(MENU_HEIGHT))
                        .child(screen_lines(cx))
                        .child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .child(key_row(&self.keys)),
                        )
                        .child(compact_pill_menu(
                            menu_items(None),
                            prefix,
                            px(MENU_TOP),
                            PillMenuAlign::Start,
                            1.0,
                            |_, _, _| {},
                            |_, _| {},
                            cx,
                        )),
                    cx,
                ),
            )
    }
}

struct PhoneLens(Vec<(&'static str, &'static str, AgentProvider, AnyView)>);

fn phone_lens(window: &mut Window, cx: &mut App) -> AnyView {
    let views = vec![
        (
            "Claude Code with the lens open: a turn running, the plan and a background task above the composer",
            "Pinned tabs keep their icon width",
            AgentProvider::ClaudeCode,
            lens::phone_claude(window, cx),
        ),
        (
            "Claude Code at its permission prompt: Allow types 1, Deny presses Esc",
            "Open the pinned tabs pull request",
            AgentProvider::ClaudeCode,
            lens::phone_claude_asks(window, cx),
        ),
        (
            "Codex after switching to its terminal: Lens view in the pill's menu opens the lens again",
            "codex",
            AgentProvider::Codex,
            lens::phone_codex(window, cx),
        ),
    ];
    cx.new(|_| PhoneLens(views)).into()
}

impl Render for PhoneLens {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let placement = placement();
        self.0.iter().enumerate().fold(
            states(),
            |states, (index, (label, title, provider, view))| {
                let pill = compact_floating_pill(
                    ("lens-pill", index),
                    PillLabel {
                        icon: agent_provider_icon(*provider),
                        title: (*title).into(),
                        meta: "zz · agents".into(),
                        warning: false,
                    },
                    page_dots(&groups(), cx),
                    &placement,
                    cx,
                );
                states.state(
                    *label,
                    phone(
                        v_flex()
                            .h(px(700.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .child(view.clone()),
                            )
                            .child(compact_floating_bar(pill, &placement, cx)),
                        cx,
                    ),
                )
            },
        )
    }
}

fn hud(text: &'static str, cx: &App) -> impl IntoElement {
    phone(
        div()
            .relative()
            .h(px(140.0))
            .p(px(12.0))
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(12.0))
            .text_color(cx.theme().foreground.muted())
            .child("~/dev/zz main* > cargo test")
            .child(compact_hud(text, cx)),
        cx,
    )
}

fn huds(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state("short", hud("Copied", cx))
        .state("longer", hud("Pane 3 of 5 · zoomed", cx))
        .into_any_element()
}

pub(crate) fn tool_keys(cx: &mut App) -> ToolKeys {
    ToolKeys::new(cx)
}

pub(crate) fn key_row(keys: &ToolKeys) -> KeyRow {
    KeyRow::new(keys.row())
        .lead(keys.lead())
        .track_scroll(&keys.scroll)
        .on_key(|_, _, _| {})
        .on_hide(|_, _| {})
}

struct KeyRows {
    rest: ToolKeys,
    armed: ToolKeys,
    narrow: ToolKeys,
    end: ToolKeys,
}

const NARROW_WIDTH: f32 = 320.0;

impl KeyRows {
    fn new(cx: &mut Context<Self>) -> Self {
        let rest = tool_keys(cx);
        let armed = tool_keys(cx);
        armed.prefix.update(cx, |key, cx| key.set_armed(true, cx));
        let end = tool_keys(cx);
        end.scroll.set_offset(ala::point(px(-1000.0), px(0.0)));
        Self {
            rest,
            armed,
            narrow: tool_keys(cx),
            end,
        }
    }
}

impl Render for KeyRows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        states()
            .columns(2)
            .state(
                "rest",
                phone(div().id("key-row-rest").child(key_row(&self.rest)), cx),
            )
            .state(
                "prefix armed",
                phone(div().id("key-row-armed").child(key_row(&self.armed)), cx),
            )
            .state(
                "narrow: more keys ahead",
                phone(div().id("key-row-narrow").child(key_row(&self.narrow)), cx)
                    .w(px(NARROW_WIDTH)),
            )
            .state(
                "narrow, scrolled to the end",
                phone(div().id("key-row-end").child(key_row(&self.end)), cx).w(px(NARROW_WIDTH)),
            )
    }
}

struct Keys {
    rest: Entity<PopoverKey>,
    armed: Entity<PopoverKey>,
    plain: Entity<PopoverKey>,
    arrows: Entity<ArrowPad>,
}

impl Keys {
    fn new(cx: &mut Context<Self>) -> Self {
        let popover = |cx: &mut Context<Self>, armed: bool| {
            let key = cx.new(|cx| PopoverKey::new("prefix", PopoverKeyTap::Press, cx));
            key.update(cx, |key, cx| key.set_armed(armed, cx));
            key
        };
        Self {
            rest: popover(cx, false),
            armed: popover(cx, true),
            plain: cx.new(|cx| PopoverKey::new("menu", PopoverKeyTap::Open, cx)),
            arrows: cx.new(ArrowPad::new),
        }
    }
}

impl Render for Keys {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let slot = |id: &'static str, child: AnyElement| {
            div()
                .id(id)
                .p(px(8.0))
                .rounded(cx.theme().radius)
                .bg(cx.theme().surface(Elevation::Panel).fill)
                .child(child)
        };
        states()
            .columns(4)
            .state(
                "prefix, rest",
                row().child(slot("key-rest", self.rest.clone().into_any_element())),
            )
            .state(
                "prefix, armed",
                row().child(slot("key-armed", self.armed.clone().into_any_element())),
            )
            .state(
                "no items",
                row().child(slot("key-plain", self.plain.clone().into_any_element())),
            )
            .state(
                "arrow pad",
                row().child(slot("key-arrows", self.arrows.clone().into_any_element())),
            )
    }
}

pub(crate) fn binding_rows() -> Arc<[WhichKeyRow]> {
    penna_storybook::kit::which_key::prefix_rows()
        .into_iter()
        .filter(|row| {
            row.group
                .as_ref()
                .is_some_and(|group| matches!(group.as_ref(), "Windows" | "Panes" | "Yours"))
        })
        .collect()
}

fn bindings(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    phone(
        v_flex()
            .id("bindings-list")
            .h(px(560.0))
            .overflow_y_scroll()
            .child(WhichKeyList::new(binding_rows())),
        cx,
    )
    .into_any_element()
}

struct Screen {
    keys: ToolKeys,
}

impl Screen {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            keys: tool_keys(cx),
        }
    }
}

const SCREEN_LINES: &[&str] = &[
    "~/dev/zz main* > cargo test -p zz-ui",
    "   Compiling zz-ui v0.15.0",
    "    Finished `test` profile in 41.87s",
    "     Running unittests src/lib.rs",
    "",
    "running 214 tests",
    "test result: ok. 214 passed; 0 failed",
    "",
    "~/dev/zz main* > ",
];

impl Render for Screen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let placement = placement();
        let pill = compact_floating_pill(
            "screen-pill",
            PillLabel {
                icon: IconName::SquareTerminal,
                title: "cargo test".into(),
                meta: "zz · editor".into(),
                warning: false,
            },
            page_dots(&groups(), cx),
            &placement,
            cx,
        );
        phone(
            v_flex()
                .h(px(640.0))
                .child(div().flex_1().min_h_0().child(screen_lines(cx)))
                .child(div().id("screen-key-row").child(key_row(&self.keys)))
                .child(compact_floating_bar(pill, &placement, cx)),
            cx,
        )
    }
}
