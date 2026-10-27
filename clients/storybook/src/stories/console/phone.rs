use std::sync::Arc;

use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, prelude::*, px,
};
use zz_protocol::AgentProvider;
use zz_ui::{
    ActiveTheme as _, Colorize as _, Elevation, IconName,
    button::{Button, ButtonVariants as _},
    compact::{
        ArrowPad, BENTO_INSET, BentoSpan, HostState, KeyRow, PageDot, PillLabel, PillPlacement,
        PopoverKey, PopoverKeyItem, PopoverKeyTap, ToolKeys, WhichKeyList, bento_add_tile,
        bento_grid, compact_bar_button, compact_composer_pill, compact_floating_bar,
        compact_floating_group, compact_floating_pill, compact_group_dot, compact_hud,
        compact_pane_header, folding_title_bar, host_tile, host_tile_line, large_title, mini_pane,
        overview_choice_row, overview_host_empty, overview_session_heading, page_dots,
        window_layout, window_tile,
    },
    pane::{PaneLens, agent_provider_icon, pane_header_icon_button, pane_lens_button},
    v_flex,
    which_key::WhichKeyRow,
};

use crate::stories::agent::lens;
use crate::story::Framed as _;
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "phone",
    name: "Phone",
    group: "Phone",
    summary: "The compact pieces of the phone shell, at phone width: the bottom bar and its pill, page dots, the pane header, the HUD, the key row and the bindings list.",
    sections: &[
        Section {
            id: "bar",
            name: "Bottom bar",
            summary: "The bar at the bottom of a pane: back to the host's windows, the pill that pages panes, and the driver chat with a dot while the driver works (accent) or waits on you (amber). The pill is set in from a 55pt screen corner so it runs concentric with it, and turns amber when the page needs attention.",
            build: |_, cx| stateless(bars, cx),
        },
        Section {
            id: "page-dots",
            name: "Page dots",
            summary: "One group per window, one dot per pane. The current pane is the wide dot; a pane asking for attention is amber.",
            build: |_, cx| stateless(dots, cx),
        },
        Section {
            id: "pane-header",
            name: "Pane header",
            summary: "The header over a terminal page, with and without actions.",
            build: |_, cx| stateless(headers, cx),
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
            summary: "The row above the soft keyboard: prefix, ctrl and alt popover keys, plain keys, the arrow pad and hide. It scrolls sideways when it does not fit.",
            build: |_, cx| cx.new(KeyRows::new).into(),
        },
        Section {
            id: "keys",
            name: "Popover keys",
            summary: "A popover key at rest and armed, as when the prefix is waiting for its next key, and the arrow pad at rest.",
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
            summary: "Three levels you zoom through, the way Calendar goes from year to month to day. Hosts lists every host as a tile, under the driver's last word, with the message field and settings in the bar. Tapping a host zooms into its page: each session's windows as a bento grid, every window drawn the way it splits its panes, the one it opens to ringed; back, the host pill that pages hosts, + and the chat sit in the bar. Tapping a window zooms into the pane it was last on. The large title folds into a frosted bar once the page scrolls it away; a host that is not connected shows why and one thing to do.",
            build: |_, cx| stateless(overview, cx),
        },
        Section {
            id: "drawers",
            name: "New drawer",
            summary: "The rows of the drawer + opens on a host's page: what it makes.",
            build: |_, cx| stateless(drawers, cx),
        },
        Section {
            id: "screen",
            name: "Phone screen",
            summary: "The pieces together: a terminal page under its header, the key row and the bar.",
            build: |_, cx| cx.new(Screen::new).into(),
        },
        Section {
            id: "lens",
            name: "Agent lens",
            summary: "A terminal running Claude Code or Codex on the phone opens in the lens, since typing into a TUI with a phone keyboard is slow. The toggle sits before close in the pane header; closing the lens keeps that pane on its terminal until the agent exits. While the lens is open the page drops the key row the way a browser page does, and the composer types into the TUI.",
            build: phone_lens,
        },
    ],
};

const PHONE_WIDTH: f32 = 390.0;

fn phone(content: impl IntoElement, cx: &App) -> impl IntoElement {
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

fn bar(id: &'static str, title: &str, meta: &str, warning: bool, cx: &App) -> impl IntoElement {
    let placement = placement();
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
    let back = compact_bar_button(format!("{id}-back"), IconName::ChevronLeft);
    let chat = compact_bar_button(format!("{id}-chat"), IconName::Chat);
    phone(
        compact_floating_bar(
            compact_floating_group([back], &placement, cx),
            &placement,
            cx,
        )
        .child(pill)
        .child(
            compact_floating_group([chat], &placement, cx)
                .relative()
                .child(compact_group_dot(cx.theme().accent)),
        ),
        cx,
    )
}

fn bars(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "rest",
            bar("bar-rest", "cargo watch", "zz · editor", false, cx),
        )
        .state(
            "attention",
            bar(
                "bar-warning",
                "python train.py",
                "exited with 1 · gpu-box",
                true,
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

fn host_bar(id: &'static str, title: &str, meta: &str, page: usize, cx: &App) -> AnyElement {
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
    let back = compact_bar_button(format!("{id}-back"), IconName::ChevronLeft);
    let new = compact_bar_button(format!("{id}-new"), IconName::Plus);
    let chat = compact_bar_button(format!("{id}-chat"), IconName::Chat);
    compact_floating_bar(
        compact_floating_group([back], &placement, cx),
        &placement,
        cx,
    )
    .child(pill)
    .child(
        compact_floating_group([new, chat], &placement, cx)
            .relative()
            .child(compact_group_dot(cx.theme().warning)),
    )
    .into_any_element()
}

fn hosts_bar(cx: &App) -> AnyElement {
    let placement = placement();
    let leading = div()
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .child(zz_ui::Icon::new(IconName::Chat).size(px(20.0)));
    let composer = compact_composer_pill(
        "hosts-composer",
        leading,
        "Message the driver",
        &placement,
        cx,
    );
    let settings = compact_bar_button("hosts-settings", IconName::Settings);
    compact_floating_bar(composer, &placement, cx)
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

fn hosts_page(cx: &App) -> impl IntoElement {
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

fn host_page(cx: &App) -> impl IntoElement {
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
                    "running 214 tests\ntest driver_chat::steps ... ok\ntest overview::bento ... ok\ntest result: ok. 214 passed",
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
                "- [ ] fold title on iPad\n- [x] driver drawer",
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
                (BentoSpan::for_window(3), daemon.into_any_element()),
                (BentoSpan::for_window(1), site.into_any_element()),
                (BentoSpan::for_window(2), agents.into_any_element()),
            ],
        )))
        .child(overview_session_heading("scratch", cx))
        .child(div().px(px(BENTO_INSET)).child(bento_grid(
            px(GRID),
            vec![(BentoSpan::for_window(1), notes.into_any_element())],
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

fn drawers(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let muted = cx.theme().foreground.muted();
    let new = v_flex()
        .p(px(8.0))
        .child(
            div()
                .px(px(12.0))
                .pb(px(8.0))
                .text_size(px(13.0))
                .text_color(muted)
                .child("Opens as a new window in zz on macbook."),
        )
        .child(overview_choice_row(
            "drawer-terminal",
            Some(zz_ui::Icon::new(IconName::SquareTerminal)),
            "Terminal",
            "A shell in the current folder",
            cx,
        ))
        .child(overview_choice_row(
            "drawer-browser",
            Some(zz_ui::Icon::new(IconName::Globe)),
            "Browser",
            "A web page on macbook, localhost included",
            cx,
        ))
        .child(overview_choice_row(
            "drawer-session",
            Some(zz_ui::Icon::new(IconName::Layers)),
            "New session",
            "Its own set of windows on macbook",
            cx,
        ));
    states()
        .columns(2)
        .state("new", phone(new, cx))
        .into_any_element()
}

pub(crate) fn close(id: &'static str, cx: &App) -> AnyElement {
    pane_header_icon_button(id, IconName::Xmark, true, cx).into_any_element()
}

fn headers(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "close action",
            phone(
                compact_pane_header(
                    IconName::SquareTerminal,
                    "cargo watch",
                    [close("header-close", cx)],
                    cx,
                ),
                cx,
            ),
        )
        .state(
            "no actions",
            phone(
                compact_pane_header(IconName::SquareTerminal, "~/dev/zz", [], cx),
                cx,
            ),
        )
        .state(
            "two actions, long title",
            phone(
                compact_pane_header(
                    IconName::SquareTerminal,
                    "ssh gpu-box · tail -f /var/log/train/run-2026-10-09.log",
                    [
                        pane_header_icon_button("header-copy", IconName::Copy, true, cx)
                            .into_any_element(),
                        close("header-close-long", cx),
                    ],
                    cx,
                ),
                cx,
            ),
        )
        .state(
            "running Claude Code: the lens toggle before close",
            phone(
                div().id("header-lens-claude").child(compact_pane_header(
                    IconName::SquareTerminal,
                    "Pinned tabs keep their icon width",
                    [
                        lens_toggle(AgentProvider::ClaudeCode, false, cx),
                        close("header-close-claude", cx),
                    ],
                    cx,
                )),
                cx,
            ),
        )
        .state(
            "running Codex with the lens open: the toggle lit",
            phone(
                div().id("header-lens-codex").child(compact_pane_header(
                    IconName::SquareTerminal,
                    "codex",
                    [
                        lens_toggle(AgentProvider::Codex, true, cx),
                        close("header-close-codex", cx),
                    ],
                    cx,
                )),
                cx,
            ),
        )
        .state(
            "disabled action",
            phone(
                compact_pane_header(
                    IconName::SquareTerminal,
                    "read-only attach",
                    [
                        pane_header_icon_button("header-disabled", IconName::Xmark, false, cx)
                            .into_any_element(),
                    ],
                    cx,
                ),
                cx,
            ),
        )
        .into_any_element()
}

fn lens_toggle(provider: AgentProvider, open: bool, cx: &App) -> AnyElement {
    pane_lens_button(PaneLens { provider, open }, |_, _| {}, cx)
        .hit_slop(10.0, 10.0)
        .into_any_element()
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
            "Codex after its lens was closed: the toggle opens it again",
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

fn prefix_items() -> (Vec<PopoverKeyItem>, PopoverKeyItem) {
    (
        vec![
            PopoverKeyItem::new("new-pane", "New pane").icon(IconName::Plus),
            PopoverKeyItem::new("new-window", "New window").icon(IconName::AppWindow),
            PopoverKeyItem::new("rename-pane", "Rename pane").icon(IconName::Pencil),
            PopoverKeyItem::new("last-pane", "Last pane").icon(IconName::History),
            PopoverKeyItem::new("kill-pane", "Kill pane")
                .icon(IconName::Xmark)
                .danger(),
        ],
        PopoverKeyItem::new("all-bindings", "All bindings").icon(IconName::Keyboard),
    )
}

pub(crate) fn tool_keys(cx: &mut App) -> ToolKeys {
    let (items, footer) = prefix_items();
    ToolKeys::new(items, Some(footer), cx)
}

pub(crate) fn key_row(keys: &ToolKeys) -> KeyRow {
    KeyRow::new(keys.row())
        .track_scroll(&keys.scroll)
        .on_key(|_, _, _| {})
        .on_hide(|_, _| {})
}

struct KeyRows {
    rest: ToolKeys,
    armed: ToolKeys,
}

impl KeyRows {
    fn new(cx: &mut Context<Self>) -> Self {
        let rest = tool_keys(cx);
        let armed = tool_keys(cx);
        armed.prefix.update(cx, |key, cx| key.set_armed(true, cx));
        Self { rest, armed }
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
            let key = cx.new(|cx| {
                let (items, footer) = prefix_items();
                PopoverKey::new("prefix", PopoverKeyTap::Open, cx)
                    .items(items)
                    .footer(footer)
            });
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
    super::which_key::prefix_rows()
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
        let theme = cx.theme();
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
                .child(compact_pane_header(
                    IconName::SquareTerminal,
                    "cargo test",
                    [close("screen-close", cx)],
                    cx,
                ))
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
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
                        ),
                )
                .child(div().id("screen-key-row").child(key_row(&self.keys)))
                .child(compact_floating_bar(pill, &placement, cx)),
            cx,
        )
    }
}
