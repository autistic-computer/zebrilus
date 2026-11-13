use ala::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    Styled as _, Subscription, Window, div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _, Size, Space,
    blocks::compact::{
        BENTO_INSET, BentoSpan, COMPACT_TITLE_BAR_HEIGHT, MiniPane, PhoneTile, PillLabel,
        PillPlacement, ZoomCard, bento_grid, compact_bar_button, compact_floating_bar,
        compact_floating_group, compact_floating_pill, window_span, zoom_card_ground,
    },
    blocks::settings::large_title,
    blocks::sheet::BottomSheet,
    blocks::touch::{
        KeyRow, PageDot, PhoneEmpty, StackClose, StackGroup, StackMenu, StatusMark, TouchHeader,
        TouchRow, action_bar, page_dots,
    },
    components::button::{Button, ButtonVariants as _},
    components::field::Field,
    components::input::{Input, InputState},
    v_flex,
};
use zz_ui::compact::{
    Confirm, FoundMachine, HostState, MachineMenu, PaneMenu, Rename, ToolKeys, found_machines,
    hold_sheet, host_mark, machine_menu, pane_menu,
};

use crate::story::Framed as _;
use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "phone",
    name: "Phone",
    group: "Phone",
    summary: "The phone shell's own pieces, at phone width: the pill menu's zz entries, the key row, the overview and the screen they make together. The pill, page dots, HUD, keys and bindings list are penna's, in its Phone chrome story.",
    sections: &[
        Section {
            id: "pill-menu",
            name: "Pill menu",
            summary: "What tapping the pill opens, as a stack menu. Inside a machine: Settings apart at the top, All machines, New session, then New in and the session in view over New browser, New terminal and New chat, New chat nearest the thumb; a machine with no sessions says In a new session and leaves New session out. Inside a pane: All bindings in a terminal, Overview, then Rename and Close for the pane, Close in danger ink, then the same New group for the pane's session.",
            build: |_, cx| cx.new(Menus::new).into(),
        },
        Section {
            id: "prefix-menu",
            name: "Prefix menu",
            summary: "The pane's stack from the prefix key over the keyboard: it rises from the key, its chips lined up along the start, and the groups that do not fit fold behind More.",
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
            name: "Machine",
            summary: "Inside a machine: the close control in the top row, the machine's name as the large title, then each session's name over a bento grid of its windows, each drawn the way it splits its panes. The pill names the machine over the session in view, where its menu makes new things. The last of several sessions leaves room to rise under the top row. A machine with no sessions is the title and the New tile, which opens the pill's menu. A machine that is not connected is the empty state with Connect as the main action, or Connecting with Cancel.",
            build: |_, cx| stateless(overview, cx),
        },
        Section {
            id: "sheets",
            name: "Sheets",
            summary: "Holding a window tile opens its sheet: the title over what the tile leaves out, then Rename and Close window in danger ink. Rename opens a sheet with the name in a 56 field and Save, off until the name changes. Close asks first: the question, what goes, Cancel and Close. The close button, the scrim, Cancel or a pick dismiss a sheet; a press on its empty frame brings it back.",
            build: |window, cx| cx.new(|cx| Sheets::new(window, cx)).into(),
        },
        Section {
            id: "screen",
            name: "Phone screen",
            summary: "The pieces together: a terminal page under the status bar, the key row and the pill.",
            build: |_, cx| cx.new(Screen::new).into(),
        },
        Section {
            id: "chat",
            name: "Chat",
            summary: "A chat pane on the phone: no header, the transcript at touch sizes (your message a bubble, the reply plain text, both at 17; steps 44 tall), then one composer row 16 from the sides: a 44 plus, the field, a 44 action. The action is send, stop while it works, or queue once you type while it works. With the keyboard up the composer takes the pill's place 8 above it. A pasted image is an 88 thumbnail with a 24 remove badge on a 44 target. Something else brings the composer back with the question over the field.",
            build: super::phone_chat::chat_states,
        },
        Section {
            id: "ask",
            name: "Ask card",
            summary: "When the agent asks, a card takes the composer's place: what it would do over the command, cut short until a tap, then the answers as touch rows that answer on tap. A question shows its count when more wait, its answers' details, and Something else last in muted ink; a question that takes several answers checks rows and sends them from a last row in accent ink. The pill says Needs approval or Has a question in warning ink.",
            build: super::phone_chat::ask_states,
        },
        Section {
            id: "plus",
            name: "Plus sheet",
            summary: "The plus opens a sheet titled with the agent: Mention, then Model, Effort, Permissions and Account with their values, and one muted line for the context and the account's limits. Each row opens its choices in the same sheet under a back control, the current one checked; Mention lists what the @ menu lists and goes inside sessions, hosts and folders.",
            build: super::phone_chat::plus_states,
        },
        Section {
            id: "hold",
            name: "Hold a message",
            summary: "No buttons under messages: holding one opens a sheet titled with its first words, with Copy and Fork from here.",
            build: super::phone_chat::hold_state,
        },
    ],
};

const PHONE_WIDTH: f32 = 390.0;

pub(super) fn phone(content: impl IntoElement, cx: &App) -> ala::Div {
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
            word: ala::SharedString::default(),
            meta: meta.to_owned().into(),
            tone: penna::Tone::Neutral,
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

fn top_row(id: &'static str) -> impl IntoElement {
    div()
        .flex()
        .justify_end()
        .items_center()
        .h(px(COMPACT_TITLE_BAR_HEIGHT))
        .px(Space::INSET_PANE)
        .child(compact_bar_button(id, IconName::Xmark).tooltip("Close"))
}

fn session(
    name: &'static str,
    first: bool,
    tiles: Vec<(BentoSpan, AnyElement)>,
) -> impl IntoElement {
    div()
        .px(px(BENTO_INSET))
        .when(!first, |section| section.mt(Space::GAP_GROUP))
        .child(TouchHeader::new(name).over_tiles().holdable())
        .child(bento_grid(px(GRID), tiles))
}

fn zz_windows() -> Vec<(BentoSpan, AnyElement)> {
    let pill = PhoneTile::new("window-pill", "Make the pill menu offer New chat")
        .mark(StatusMark::Warning)
        .state("Needs approval")
        .pane([0.0, 0.0, 0.5, 1.0], MiniPane::chat("mini-1"))
        .pane(
            [0.5, 0.0, 0.5, 0.5],
            term(
                "mini-2",
                "running 14 tests\ntest pill::menu_order ... ok\ntest pill::new_chat ... ok",
            ),
        )
        .pane(
            [0.5, 0.5, 0.5, 0.5],
            MiniPane::browser("mini-3", "127.0.0.1:8090"),
        );
    let stack = PhoneTile::new("window-stack", "Fix pane stack hit testing")
        .mark(StatusMark::Spinner)
        .pane([0.0, 0.0, 1.0, 1.0], MiniPane::chat("mini-4"));
    let watch = PhoneTile::new("window-watch", "cargo watch").pane(
        [0.0, 0.0, 1.0, 1.0],
        term(
            "mini-5",
            "~/dev/zebrilus main\n> cargo watch -x check\n    Checking zz-mux v0.42.0\n    Finished `dev` profile in 2.14s",
        ),
    );
    vec![
        (window_span(3), pill.into_any_element()),
        (window_span(1), stack.into_any_element()),
        (window_span(1), watch.into_any_element()),
    ]
}

fn dotfiles_windows() -> Vec<(BentoSpan, AnyElement)> {
    let zsh = PhoneTile::new("window-zsh", "zsh").pane(
        [0.0, 0.0, 1.0, 1.0],
        term(
            "mini-6",
            "~/.files main\n> git status -sb\n## main...origin/main\n M .bash/macos.bash",
        ),
    );
    vec![(window_span(1), zsh.into_any_element())]
}

fn nidus_windows() -> Vec<(BentoSpan, AnyElement)> {
    let warp = PhoneTile::new("window-warp", "Pointer warp on a second screen")
        .mark(StatusMark::Spinner)
        .pane([0.0, 0.0, 1.0, 0.5], MiniPane::chat("mini-7"))
        .pane(
            [0.0, 0.5, 1.0, 0.5],
            term("mini-8", "warp: DP-1 -> HDMI-1\nwarp: pointer 2581,412"),
        );
    let build = PhoneTile::new("window-build", "cargo build")
        .mark(StatusMark::Danger)
        .state("Exit 101")
        .pane(
            [0.0, 0.0, 1.0, 1.0],
            term(
                "mini-9",
                "~/dev/nidus warp\n> cargo build --release\nerror[E0425]: cannot find value `edge`\nerror: could not compile `nidus`",
            ),
        );
    vec![
        (window_span(2), warp.into_any_element()),
        (window_span(1), build.into_any_element()),
    ]
}

pub(crate) fn host_page(cx: &App) -> impl IntoElement {
    v_flex()
        .pb(px(8.0))
        .child(top_row("machine-close"))
        .child(large_title("macbook", cx))
        .child(session("zz", true, zz_windows()))
        .child(session("dotfiles", false, dotfiles_windows()))
        .child(session("nidus", false, nidus_windows()))
}

fn machine_screen(
    id: &'static str,
    page: impl IntoElement,
    pill: AnyElement,
    cx: &App,
) -> ala::Div {
    phone(
        v_flex()
            .h(px(SCREEN_HEIGHT))
            .child(
                div()
                    .id(id)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(page),
            )
            .child(pill),
        cx,
    )
}

fn unreached(id: &'static str, empty: PhoneEmpty, action: Button, cx: &App) -> ala::Div {
    phone(
        v_flex()
            .h(px(SCREEN_HEIGHT))
            .child(top_row(id))
            .child(large_title("alienware", cx))
            .child(div().mt(px(96.0)).child(empty))
            .child(div().flex_1())
            .child(action_bar(px(34.0)).child(action.with_size(Size::TouchLarge).w_full())),
        cx,
    )
}

fn overview(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let one = v_flex()
        .child(top_row("machine-close-xps"))
        .child(large_title("xps", cx))
        .child(session("work", true, zz_windows()));
    let empty = v_flex()
        .child(top_row("machine-close-empty"))
        .child(large_title("xps", cx))
        .child(
            div()
                .px(px(BENTO_INSET))
                .pt(Space::GAP_GROUP)
                .child(bento_grid(
                    px(GRID),
                    vec![(
                        BentoSpan::new(2, 2),
                        PhoneTile::add("machine-new", "New").into_any_element(),
                    )],
                )),
        );
    states()
        .columns(2)
        .state(
            "several sessions: the pill names zz, the session in view",
            machine_screen(
                "machine-several",
                host_page(cx),
                host_bar("overview-bar-macbook", "macbook", "zz", 0, cx),
                cx,
            ),
        )
        .state(
            "one session",
            machine_screen(
                "machine-one",
                one,
                host_bar("overview-bar-xps", "xps", "work", 1, cx),
                cx,
            ),
        )
        .state(
            "no sessions: the title and the New tile",
            machine_screen(
                "machine-empty",
                empty,
                host_bar("overview-bar-empty", "xps", "No sessions", 1, cx),
                cx,
            ),
        )
        .state(
            "not connected: Connect is the main action",
            unreached(
                "machine-close-offline",
                PhoneEmpty::new(IconName::Server, "Not connected")
                    .line("ssh: connect to host alienware port 22: Connection refused"),
                Button::new("machine-connect").primary().label("Connect"),
                cx,
            ),
        )
        .state(
            "connecting: Cancel is the one action",
            unreached(
                "machine-close-connecting",
                PhoneEmpty::spinner("Connecting").line("Waiting for ssh on fabrico@alienware"),
                Button::new("machine-cancel").label("Cancel"),
                cx,
            ),
        )
        .into_any_element()
}

const MENU_HEIGHT: f32 = 700.0;

fn machine_groups() -> Vec<StackGroup> {
    machine_menu(MachineMenu::Session("zz"))
}

fn terminal_groups() -> Vec<StackGroup> {
    pane_menu(PaneMenu {
        session: "zz",
        terminal: true,
    })
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
    machine: Entity<StackMenu>,
    empty: Entity<StackMenu>,
    pane: Entity<StackMenu>,
}

fn open_menu(groups: Vec<StackGroup>, cx: &mut Context<Menus>) -> Entity<StackMenu> {
    let menu = cx.new(|cx| {
        let mut menu = StackMenu::new(StackClose::Disc, cx);
        menu.set_groups(groups, cx);
        menu.open(cx);
        menu
    });
    cx.observe(&menu, |_, _, cx| cx.notify()).detach();
    menu
}

impl Menus {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            machine: open_menu(machine_groups(), cx),
            empty: open_menu(machine_menu(MachineMenu::Empty), cx),
            pane: open_menu(terminal_groups(), cx),
        }
    }
}

fn menu_screen(
    id: &'static str,
    label: PillLabel,
    menu: &Entity<StackMenu>,
    page: AnyElement,
    cx: &App,
) -> ala::Div {
    let placement = placement();
    let pill = compact_floating_pill(id, label, page_dots(&groups(), cx), &placement, cx)
        .opacity(1.0 - menu.read(cx).close_shown())
        .child(menu.read(cx).track_anchor());
    phone(
        div()
            .relative()
            .h(px(MENU_HEIGHT))
            .overflow_hidden()
            .child(page)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(compact_floating_bar(pill, &placement, cx)),
            )
            .child(menu.clone()),
        cx,
    )
}

impl Render for Menus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let machine = |meta: &str| PillLabel {
            icon: IconName::Server,
            title: "macbook".into(),
            word: ala::SharedString::default(),
            meta: meta.to_owned().into(),
            tone: penna::Tone::Neutral,
        };
        states()
            .columns(2)
            .state(
                "inside a machine: New in zz",
                menu_screen(
                    "menu-machine-pill",
                    machine("zz"),
                    &self.machine,
                    host_page(cx).into_any_element(),
                    cx,
                ),
            )
            .state(
                "a machine with no sessions: In a new session",
                menu_screen(
                    "menu-empty-pill",
                    machine("No sessions"),
                    &self.empty,
                    v_flex()
                        .child(top_row("menu-empty-close"))
                        .child(large_title("macbook", cx))
                        .into_any_element(),
                    cx,
                ),
            )
            .state(
                "inside a terminal: All bindings, Overview, this pane, New in zz",
                menu_screen(
                    "menu-pane-pill",
                    PillLabel {
                        icon: IconName::SquareTerminal,
                        title: "zsh".into(),
                        word: ala::SharedString::default(),
                        meta: "zebrilus · main".into(),
                        tone: penna::Tone::Neutral,
                    },
                    &self.pane,
                    screen_lines(cx).into_any_element(),
                    cx,
                ),
            )
    }
}

struct Sheets {
    open: [bool; 3],
    name: Entity<InputState>,
    _theme: Subscription,
}

impl Sheets {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            open: [true; 3],
            name: cx.new(|cx| InputState::new(window, cx).default_value("cargo watch")),
            _theme: cx.observe_global::<penna::Theme>(|this: &mut Self, cx| {
                this.open = [true; 3];
                cx.notify();
            }),
        }
    }

    fn dismissed(at: usize, cx: &mut Context<Self>) -> impl Fn(&mut Window, &mut App) + 'static {
        let view = cx.entity().downgrade();
        move |_, cx| {
            let _ = view.update(cx, |this, cx| {
                this.open[at] = false;
                cx.notify();
            });
        }
    }

    fn frame(
        &self,
        id: &'static str,
        at: usize,
        sheet: BottomSheet,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.open[at];
        phone(
            div()
                .id(id)
                .relative()
                .h(px(SCREEN_HEIGHT))
                .overflow_hidden()
                .child(host_page(cx))
                .when(open, |frame| frame.child(sheet))
                .when(!open, |frame| {
                    frame.on_click(cx.listener(move |this, _, _, cx| {
                        this.open[at] = true;
                        cx.notify();
                    }))
                }),
            cx,
        )
    }
}

impl Render for Sheets {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hold = hold_sheet(
            "story-hold-sheet",
            "Fix pane stack hit testing",
            Some("Testing 62% · zebrilus · pane-stack · now".into()),
            [
                TouchRow::new("story-hold-rename", "Rename").icon(IconName::Pencil),
                TouchRow::new("story-hold-close", "Close window")
                    .icon(IconName::Xmark)
                    .danger(),
            ],
            px(34.0),
            Self::dismissed(0, cx),
            cx,
        );
        let rename = Rename {
            id: "story-rename-sheet".into(),
            title: "Rename window".into(),
            current: "cargo watch".into(),
        }
        .sheet(
            &self.name,
            false,
            px(34.0),
            |_, _, _| {},
            Self::dismissed(1, cx),
            cx,
        );
        let confirm = Confirm {
            id: "story-confirm-sheet".into(),
            question: "Close this pane?".into(),
            detail: Some("zsh stops. Its scrollback goes.".into()),
            verb: "Close".into(),
        }
        .sheet(px(34.0), |_, _| {}, Self::dismissed(2, cx), cx);
        states()
            .columns(2)
            .state("hold a window tile", self.frame("sheet-hold", 0, hold, cx))
            .state(
                "rename: Save stays off until the name changes",
                self.frame("sheet-rename", 1, rename, cx),
            )
            .state(
                "close asks first",
                self.frame("sheet-confirm", 2, confirm, cx),
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
            menu.set_groups(terminal_groups(), cx);
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
                word: ala::SharedString::default(),
                meta: "zz · editor".into(),
                tone: penna::Tone::Neutral,
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
