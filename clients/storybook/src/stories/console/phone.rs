use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    Render, Styled as _, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, Size,
    blocks::compact::{
        BENTO_INSET, BentoSpan, MiniPane, PhoneTile, PillLabel, PillPlacement, ZoomCard,
        bento_grid, compact_bar_button, compact_floating_bar, compact_floating_group,
        compact_floating_pill, window_span, zoom_card_ground,
    },
    blocks::settings::large_title,
    blocks::touch::{
        KeyRow, PageDot, PhoneEmpty, StackChip, StackClose, StackGroup, StackLook, StackMenu,
        StatusMark, page_dots,
    },
    components::button::{Button, ButtonVariants as _},
    components::field::Field,
    components::input::{Input, InputState},
    v_flex,
};
use zz_protocol::AgentProvider;
use zz_ui::{
    compact::{
        FoundMachine, HostState, ToolKeys, found_machines, host_mark, overview_host_empty,
        overview_session_heading,
    },
    pane::agent_provider_icon,
};

use crate::stories::agent::pane as agent_pane;
use crate::story::Framed as _;
use crate::story::{Section, Story, stateless, states};
use zz_ui::pane::FlipTo;

pub const STORY: Story = Story {
    id: "phone",
    name: "Phone",
    group: "Phone",
    summary: "The phone shell's own pieces, at phone width: the pill menu's zz entries, the key row, the overview and the screen they make together. The pill, page dots, HUD, keys and bindings list are penna's, in its Phone chrome story.",
    sections: &[
        Section {
            id: "pill-menu",
            name: "Pill menu",
            summary: "What tapping the pill opens, as a stack menu. Switch to chat or Overview sits nearest the thumb; Kill pane and All bindings sit furthest. A pane running Claude Code or Codex adds Switch to chat, which reads Switch to the CLI on an agent pane.",
            build: |_, cx| cx.new(Menus::new).into(),
        },
        Section {
            id: "prefix-menu",
            name: "Prefix menu",
            summary: "The same stack from the prefix key over the keyboard: it rises from the key, its chips lined up along the start.",
            build: |_, cx| cx.new(PrefixMenu::new).into(),
        },
        Section {
            id: "key-row",
            name: "Key row",
            summary: "The row above the soft keyboard: the prefix key on its own, then ctrl and alt popover keys, plain keys, the arrow pad and hide in a row that scrolls sideways when it does not fit. A fade and a chevron mark the side with more keys; tapping them scrolls there.",
            build: |_, cx| cx.new(KeyRows::new).into(),
        },
        Section {
            id: "machines",
            name: "Machines",
            summary: "Where the app opens: the large title, machine tiles two to a row in words and a mark, and the add tile last, with the gear disc at the bottom. Empty, it is the title and the add tile alone. The add tile zooms into the add card, found machines first; a connecting tile zooms into the connecting card, Cancel its one action.",
            build: |window, cx| cx.new(|cx| Machines::new(window, cx)).into(),
        },
        Section {
            id: "overview",
            name: "Overview",
            summary: "Inside a machine: each session's windows as a bento grid, every window drawn the way it splits its panes, over the host pill. Swiping the pill sideways pages machines, swiping it up zooms back out to Machines, and tapping it opens the machine's menu. Tapping a window zooms into the pane it was last on. A machine that is not connected shows why and one thing to do.",
            build: |_, cx| stateless(overview, cx),
        },
        Section {
            id: "screen",
            name: "Phone screen",
            summary: "The pieces together: a terminal page under the status bar, the key row and the pill.",
            build: |_, cx| cx.new(Screen::new).into(),
        },
        Section {
            id: "agent",
            name: "Agent pane",
            summary: "An agent pane on the phone is a chat, which beats typing into a TUI with a phone keyboard. Switch to the CLI and Switch to chat in the pill's menu flip the pane on the same session; a busy pane asks first in its corner. A chat page drops the key row the way a browser page does.",
            build: phone_agents,
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

pub(crate) fn placement() -> PillPlacement {
    PillPlacement::new(Some(px(55.0)), px(34.0))
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

fn machine(
    id: &'static str,
    name: &'static str,
    state: HostState,
    waiting: usize,
    foot: &[&'static str],
) -> (BentoSpan, AnyElement) {
    let (mark, word) = host_mark(state, waiting);
    let tile = PhoneTile::new(id, name).mark(mark).state(word);
    let tile = foot.iter().fold(tile, |tile, line| tile.foot(*line));
    (BentoSpan::new(2, 2), tile.into_any_element())
}

fn add_tile() -> (BentoSpan, AnyElement) {
    (
        BentoSpan::new(2, 2),
        PhoneTile::add("hosts-add", "Add machine").into_any_element(),
    )
}

pub(crate) fn machines_page(tiles: Vec<(BentoSpan, AnyElement)>, cx: &App) -> impl IntoElement {
    v_flex()
        .h(px(SCREEN_HEIGHT))
        .child(
            v_flex().flex_1().child(large_title("Machines", cx)).child(
                div()
                    .px(px(BENTO_INSET))
                    .pt(px(16.0))
                    .child(bento_grid(px(GRID), tiles)),
            ),
        )
        .child(hosts_bar(cx))
}

fn every_machine() -> Vec<(BentoSpan, AnyElement)> {
    vec![
        machine(
            "hosts-macbook",
            "macbook",
            HostState::Connected,
            1,
            &["3 sessions", "2 working"],
        ),
        machine("hosts-xps", "xps", HostState::Connected, 0, &["2 sessions"]),
        machine(
            "hosts-ubuntu",
            "ubuntu",
            HostState::Connecting,
            0,
            &["fabrico@ubuntu"],
        ),
        machine("hosts-thinkpad", "thinkpad", HostState::Offline, 0, &[]),
        machine(
            "hosts-alienware",
            "alienware",
            HostState::Failed,
            0,
            &["Connection refused"],
        ),
        add_tile(),
    ]
}

const SCREEN_HEIGHT: f32 = 760.0;

struct Machines {
    found: Entity<InputState>,
    alone: Entity<InputState>,
}

impl Machines {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            found: cx.new(|cx| InputState::new(window, cx).placeholder("user@host")),
            alone: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("user@host")
                    .default_value("fabrico@xps")
            }),
        }
    }
}

fn card(card: ZoomCard, cx: &App) -> impl IntoElement {
    phone(
        div()
            .h(px(SCREEN_HEIGHT))
            .child(card.top_inset(px(16.0)).bottom_inset(px(34.0))),
        cx,
    )
}

fn address(
    input: &Entity<InputState>,
    id: &'static str,
    label: &'static str,
    ground: penna::Ground,
) -> Field {
    Field::new().with_size(Size::TouchLarge).label(label).child(
        Input::new(input)
            .id(id)
            .with_size(Size::TouchLarge)
            .on(ground),
    )
}

fn connect(id: &'static str, ground: penna::Ground, empty: bool) -> Button {
    Button::new(id)
        .primary()
        .label("Connect")
        .with_size(Size::TouchLarge)
        .w_full()
        .on(ground)
        .disabled(empty)
}

fn connecting_card(id: &'static str, name: &'static str, line: &'static str, cx: &App) -> ZoomCard {
    let ground = zoom_card_ground(cx);
    ZoomCard::new(id, name)
        .child(
            div()
                .mt(px(96.0))
                .child(PhoneEmpty::spinner("Connecting").line(line).on(ground)),
        )
        .action(
            Button::new((id, 1usize))
                .label("Cancel")
                .with_size(Size::TouchLarge)
                .w_full()
                .on(ground),
        )
}

impl Render for Machines {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ground = zoom_card_ground(cx);
        let found = [
            FoundMachine {
                name: "macbook".into(),
                system: "macOS".into(),
                runs_zz: true,
            },
            FoundMachine {
                name: "alienware".into(),
                system: "linux".into(),
                runs_zz: true,
            },
            FoundMachine {
                name: "homelab".into(),
                system: "linux".into(),
                runs_zz: false,
            },
        ];
        let tailnet = ZoomCard::new("add-found", "Add machine")
            .children(found_machines(&found, |_, _, _| {}))
            .child(address(
                &self.found,
                "add-found-address",
                "Or an address",
                ground,
            ))
            .action(connect("add-found-connect", ground, true));
        let alone = ZoomCard::new("add-alone", "Add machine")
            .child(address(&self.alone, "add-alone-address", "Address", ground))
            .action(connect("add-alone-connect", ground, false));
        states()
            .columns(2)
            .state(
                "empty: the title and the add tile",
                phone(machines_page(vec![add_tile()], cx), cx),
            )
            .state(
                "connected, waiting, connecting, not connected, failed",
                phone(machines_page(every_machine(), cx), cx),
            )
            .state("add card, found on your tailnet", card(tailnet, cx))
            .state("add card, no tailnet, typed", card(alone, cx))
            .state(
                "connecting card, over ssh",
                card(
                    connecting_card(
                        "connecting-ssh",
                        "xps",
                        "Waiting for ssh on fabrico@xps",
                        cx,
                    ),
                    cx,
                ),
            )
            .state(
                "connecting card, on your tailnet",
                card(
                    connecting_card(
                        "connecting-tailnet",
                        "studio",
                        "Waiting for studio on your tailnet",
                        cx,
                    ),
                    cx,
                ),
            )
    }
}

#[allow(
    clippy::disallowed_methods,
    reason = "the sample lines stand in for terminal cells, which keep the terminal's own size"
)]
fn terminal_text(element: ala::Div, size: f32, cx: &App) -> ala::Div {
    element
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(size))
}

fn term(id: &'static str, lines: &str) -> MiniPane {
    MiniPane::terminal(
        id,
        v_flex().children(lines.lines().map(|line| div().child(line.to_owned()))),
    )
}

pub(crate) fn host_page(cx: &App) -> impl IntoElement {
    let daemon = PhoneTile::new("window-daemon", "cargo test")
        .mark(StatusMark::Spinner)
        .pane(
            [0.0, 0.0, 0.6, 1.0],
            term(
                "mini-1",
                "running 214 tests\ntest overview::pager ... ok\ntest overview::bento ... ok\ntest result: ok. 214 passed",
            ),
        )
        .pane([0.6, 0.0, 0.4, 0.5], term("mini-2", "fn host_body(\n  &mut self,"))
        .pane([0.6, 0.5, 0.4, 0.5], MiniPane::blank("mini-3"));
    let site = PhoneTile::new("window-site", "site").pane(
        [0.0, 0.0, 1.0, 1.0],
        MiniPane::browser("mini-4", "localhost:4321"),
    );
    let agents = PhoneTile::new("window-agents", "Review the tailnet plan")
        .mark(StatusMark::Warning)
        .state("Needs approval")
        .pane([0.0, 0.0, 0.5, 1.0], MiniPane::chat("mini-5"))
        .pane([0.5, 0.0, 0.5, 1.0], MiniPane::chat("mini-6"));
    let notes = PhoneTile::new("window-notes", "notes").pane(
        [0.0, 0.0, 1.0, 1.0],
        term("mini-7", "- [ ] fold title on iPad\n- [x] bento grid"),
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
            "a connected host",
            phone(
                v_flex()
                    .child(host_page(cx))
                    .child(host_bar("overview-bar-macbook", "macbook", "2 sessions", 0, cx)),
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

fn menu_groups(flip: Option<FlipTo>) -> Vec<StackGroup> {
    let flip = flip.map(|to| StackChip::new("flip", to.label(), zz_ui::pane::flip_icon(to)));
    vec![
        StackGroup::new(flip.into_iter().chain([
            StackChip::new("overview", "Overview", IconName::LayoutDashboard),
            StackChip::new("last-pane", "Last pane", IconName::History),
            StackChip::new("new-pane", "New pane", IconName::Plus),
            StackChip::new("new-window", "New window", IconName::AppWindow),
            StackChip::new("rename-pane", "Rename pane", IconName::Pencil),
        ])),
        StackGroup::new([
            StackChip::new("kill-pane", "Kill pane", IconName::Xmark).look(StackLook::Danger),
            StackChip::new("all-bindings", "All bindings", IconName::Keyboard)
                .look(StackLook::Quiet),
        ]),
    ]
}

fn screen_lines(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .px(px(12.0))
        .py(px(8.0))
        .map(|lines| terminal_text(lines, 12.0, cx))
        .line_height(px(17.0))
        .text_color(theme.foreground)
        .children(
            SCREEN_LINES
                .iter()
                .map(|line| div().h(px(17.0)).child(*line)),
        )
}

struct Menus {
    menu: Entity<StackMenu>,
}

impl Menus {
    fn new(cx: &mut Context<Self>) -> Self {
        let menu = cx.new(|cx| {
            let mut menu = StackMenu::new(StackClose::Disc, cx);
            menu.set_groups(menu_groups(Some(FlipTo::Chat)), cx);
            menu.open(cx);
            menu
        });
        cx.observe(&menu, |_, _, cx| cx.notify()).detach();
        Self { menu }
    }
}

impl Render for Menus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let placement = placement();
        let pill = compact_floating_pill(
            "menu-pill",
            PillLabel {
                icon: IconName::SquareTerminal,
                title: "claude".into(),
                meta: "zebrilus · main".into(),
                warning: false,
            },
            page_dots(&groups(), cx),
            &placement,
            cx,
        )
        .opacity(1.0 - self.menu.read(cx).close_shown())
        .child(self.menu.read(cx).track_anchor());
        states().columns(1).state(
            "on a terminal running Claude Code: Switch to chat",
            phone(
                div()
                    .relative()
                    .h(px(MENU_HEIGHT))
                    .child(screen_lines(cx))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(compact_floating_bar(pill, &placement, cx)),
                    )
                    .child(self.menu.clone()),
                cx,
            ),
        )
    }
}

struct PrefixMenu {
    keys: ToolKeys,
}

impl PrefixMenu {
    fn new(cx: &mut Context<Self>) -> Self {
        let keys = tool_keys(cx);
        let prefix = keys.prefix.read(cx).menu().clone();
        prefix.update(cx, |menu, cx| {
            menu.set_groups(menu_groups(None), cx);
            menu.open(cx);
        });
        Self { keys }
    }
}

impl Render for PrefixMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                ),
            cx,
        )
    }
}

struct PhoneAgents(Vec<(&'static str, &'static str, AgentProvider, AnyView)>);

fn phone_agents(window: &mut Window, cx: &mut App) -> AnyView {
    let views = vec![
        (
            "a Claude Code chat: a turn running, the plan and a background task above the composer",
            "Pinned tabs keep their icon width",
            AgentProvider::ClaudeCode,
            agent_pane::phone_claude(window, cx),
        ),
        (
            "Claude Code at its permission prompt",
            "Open the pinned tabs pull request",
            AgentProvider::ClaudeCode,
            agent_pane::phone_claude_asks(window, cx),
        ),
        (
            "Switch to the CLI during a turn: the choice sits in the page's corner",
            "Pinned tabs keep their icon width",
            AgentProvider::ClaudeCode,
            agent_pane::phone_claude_busy(window, cx),
        ),
        (
            "Codex in a terminal: Switch to chat in the pill's menu turns it into a chat",
            "codex",
            AgentProvider::Codex,
            agent_pane::phone_codex(window, cx),
        ),
    ];
    cx.new(|_| PhoneAgents(views)).into()
}

impl Render for PhoneAgents {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let placement = placement();
        self.0.iter().enumerate().fold(
            states(),
            |states, (index, (label, title, provider, view))| {
                let pill = compact_floating_pill(
                    ("agent-pill", index),
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
