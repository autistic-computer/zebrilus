use ala::{
    App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, ScrollHandle,
    Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, IconName, StyledExt as _, TextStyle,
    blocks::compact::{PillLabel, PillPlacement, compact_floating_bar, compact_floating_pill},
    blocks::touch::{
        ArrowPad, KeyRow, KeyRowKey, StackChip, StackClose, StackGroup, StackKey, StackKeyTap,
        StackLook, StackMenu, StickyModifier,
    },
    v_flex,
};

use crate::story::{Framed as _, Section, Story, states};

pub const STORY: Story = Story {
    id: "stack-menu",
    name: "Stack menu",
    group: "Kit",
    summary: "The phone's one menu: 52 chips rising in a column out of the pill or a key, nearest first, taken by a tap or by a hold, a slide and a release.",
    sections: &[
        Section {
            id: "from-pill",
            name: "From the pill",
            summary: "A tap leaves the stack open and turns the pill into the close. Held, the chip under the finger inverts and grows 4%. Groups sit 8 apart inside and 16 between; a label chip heads the group that makes things; Close is danger, Find disabled, All bindings quiet.",
            build: |_, cx| cx.new(Pills::new).into(),
        },
        Section {
            id: "kinds",
            name: "Rows, values and places",
            summary: "A row of up to three glyph chips with accessible names, a chip that ends in a count, and the address cut in the middle so both ends show. Folded: what does not fit, or what the caller puts there, waits behind More at the far end.",
            build: |_, cx| cx.new(Kinds::new).into(),
        },
        Section {
            id: "ctrl",
            name: "Chords in two columns",
            summary: "Holding ctrl opens its ten chords as a stack over the key, two columns filled a row at a time from the key out, ^C and ^D nearest. The chips share one width so the chords line up; the key stays inverted and is its own close.",
            build: |_, cx| cx.new(|cx| Chords::new(StickyModifier::Control, cx)).into(),
        },
        Section {
            id: "alt",
            name: "Chords in one column",
            summary: "alt's four chords in one column, M-b Word back nearest the key.",
            build: |_, cx| cx.new(|cx| Chords::new(StickyModifier::Alt, cx)).into(),
        },
    ],
};

const PHONE_WIDTH: f32 = 390.0;
const PHONE_HEIGHT: f32 = 680.0;
const KEYS_HEIGHT: f32 = 560.0;

const PAGE_LINES: [&str; 6] = [
    "$ cargo test -p penna stack",
    "   Compiling penna v0.42.0",
    "running 14 tests",
    "test result: ok. 14 passed",
    "$ git push -u origin stack-menu",
    "$ ",
];

fn page_lines(cx: &App) -> impl IntoElement {
    v_flex()
        .px(px(12.0))
        .py(px(8.0))
        .type_style_mono(TextStyle::Small, cx)
        .text_color(cx.theme().foreground)
        .children(PAGE_LINES.map(|line| div().h(TextStyle::Small.line_height()).child(line)))
}

fn placement() -> PillPlacement {
    PillPlacement::new(Some(px(55.0)), px(34.0))
}

fn pane_groups() -> Vec<StackGroup> {
    vec![
        StackGroup::new([
            StackChip::new("new-chat", "New chat", IconName::ChatPlus),
            StackChip::new("new-terminal", "New terminal", IconName::SquareTerminal),
            StackChip::new("new-browser", "New browser", IconName::Globe),
        ])
        .label("New in zz"),
        StackGroup::new([
            StackChip::new("close", "Close", IconName::Xmark).look(StackLook::Danger),
            StackChip::new("rename", "Rename", IconName::Pencil),
            StackChip::new("find", "Find", IconName::Search).disabled(true),
        ]),
        StackGroup::new([StackChip::new(
            "overview",
            "Overview",
            IconName::LayoutDashboard,
        )]),
        StackGroup::new([
            StackChip::new("all-bindings", "All bindings", IconName::Keyboard)
                .look(StackLook::Quiet),
        ]),
    ]
}

fn browser_groups() -> Vec<StackGroup> {
    vec![
        StackGroup::new([
            StackChip::new("tabs", "Tabs", IconName::Copy).value("3"),
            StackChip::new(
                "address",
                "github.com/autistic-computer/zebrilus/pull/new/stack-menu",
                IconName::Globe,
            )
            .place(),
        ]),
        StackGroup::row([
            StackChip::new("back", "Back", IconName::ArrowLeft),
            StackChip::new("forward", "Forward", IconName::ArrowRight),
            StackChip::new("reload", "Reload", IconName::Redo2),
        ]),
    ]
}

fn menu(groups: Vec<StackGroup>, fold: Option<usize>, cx: &mut App) -> Entity<StackMenu> {
    cx.new(|cx| {
        let mut menu = StackMenu::new(StackClose::Disc, cx);
        menu.set_groups(groups, cx);
        menu.set_fold(fold, cx);
        menu.open(cx);
        menu
    })
}

fn phone(id: &'static str, menu: &Entity<StackMenu>, cx: &App) -> impl IntoElement {
    let placement = placement();
    let hidden = menu.read(cx).close_shown();
    let pill = compact_floating_pill(
        id,
        PillLabel {
            icon: IconName::SquareTerminal,
            title: "zsh".into(),
            word: ala::SharedString::default(),
            meta: "zebrilus · stack-menu".into(),
            tone: penna::Tone::Neutral,
        },
        div(),
        &placement,
        cx,
    )
    .opacity(1.0 - hidden)
    .child(menu.read(cx).track_anchor());
    div()
        .w(px(PHONE_WIDTH))
        .h(px(PHONE_HEIGHT))
        .relative()
        .overflow_hidden()
        .bg(cx.theme().background)
        .framed(cx)
        .rounded(cx.theme().radius)
        .child(page_lines(cx))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .child(compact_floating_bar(pill, &placement, cx)),
        )
        .child(menu.clone())
}

struct Pills {
    tapped: Entity<StackMenu>,
    held: Entity<StackMenu>,
}

impl Pills {
    fn new(cx: &mut Context<Self>) -> Self {
        let tapped = menu(pane_groups(), None, cx);
        let held = cx.new(|cx| {
            let mut menu = StackMenu::new(StackClose::Disc, cx);
            menu.set_groups(pane_groups(), cx);
            menu.lift("new-terminal", cx);
            menu
        });
        cx.observe(&tapped, |_, _, cx| cx.notify()).detach();
        cx.observe(&held, |_, _, cx| cx.notify()).detach();
        Self { tapped, held }
    }
}

impl Render for Pills {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        states()
            .columns(2)
            .state(
                "tapped: it stays, the pill is the close",
                phone("pill-tapped", &self.tapped, cx),
            )
            .state(
                "held: New terminal under the finger",
                phone("pill-held", &self.held, cx),
            )
    }
}

struct Kinds {
    browser: Entity<StackMenu>,
    folded: Entity<StackMenu>,
}

impl Kinds {
    fn new(cx: &mut Context<Self>) -> Self {
        let browser = menu(browser_groups(), None, cx);
        let folded = menu(pane_groups(), Some(2), cx);
        cx.observe(&browser, |_, _, cx| cx.notify()).detach();
        cx.observe(&folded, |_, _, cx| cx.notify()).detach();
        Self { browser, folded }
    }
}

impl Render for Kinds {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        states()
            .columns(2)
            .state(
                "a glyph row, a count and the address",
                phone("pill-browser", &self.browser, cx),
            )
            .state("folded behind More", phone("pill-folded", &self.folded, cx))
    }
}

struct Chords {
    key: Entity<StackKey>,
    other: Entity<StackKey>,
    arrows: Entity<ArrowPad>,
    scroll: ScrollHandle,
    modifier: StickyModifier,
}

fn chord_chips(modifier: StickyModifier) -> Vec<StackChip> {
    let chords: &[(&str, &str, &str)] = match modifier {
        StickyModifier::Control => &[
            ("ctrl-c", "^C", "Interrupt"),
            ("ctrl-d", "^D", "End input"),
            ("ctrl-z", "^Z", "Suspend"),
            ("ctrl-l", "^L", "Clear"),
            ("ctrl-r", "^R", "History"),
            ("ctrl-u", "^U", "Kill line"),
            ("ctrl-w", "^W", "Kill word"),
            ("ctrl-k", "^K", "Kill to end"),
            ("ctrl-a", "^A", "Line start"),
            ("ctrl-e", "^E", "Line end"),
        ],
        StickyModifier::Alt => &[
            ("alt-b", "M-b", "Word back"),
            ("alt-f", "M-f", "Word ahead"),
            ("alt-d", "M-d", "Kill word"),
            ("alt-.", "M-.", "Last arg"),
        ],
    };
    chords
        .iter()
        .map(|(id, chord, label)| StackChip::chord(*id, *chord, *label))
        .collect()
}

impl Chords {
    fn new(modifier: StickyModifier, cx: &mut Context<Self>) -> Self {
        let scroll = ScrollHandle::new();
        let latch = |modifier: StickyModifier, cx: &mut Context<Self>| {
            let label = match modifier {
                StickyModifier::Control => "ctrl",
                StickyModifier::Alt => "alt",
            };
            cx.new(|cx| {
                StackKey::new(label, StackKeyTap::Latch(modifier), cx)
                    .icon(modifier.icon())
                    .scrolls(scroll.clone())
                    .groups(vec![StackGroup::new(chord_chips(modifier))], cx)
            })
        };
        let other = match modifier {
            StickyModifier::Control => StickyModifier::Alt,
            StickyModifier::Alt => StickyModifier::Control,
        };
        let key = latch(modifier, cx);
        let other = latch(other, cx);
        key.update(cx, |key, cx| {
            key.menu().update(cx, StackMenu::open);
        });
        Self {
            key,
            other,
            arrows: cx.new(ArrowPad::new),
            scroll,
            modifier,
        }
    }

    fn row(&self) -> KeyRow {
        let (control, alt) = match self.modifier {
            StickyModifier::Control => (&self.key, &self.other),
            StickyModifier::Alt => (&self.other, &self.key),
        };
        KeyRow::new(vec![
            KeyRowKey::Stack(control.clone()),
            KeyRowKey::Stack(alt.clone()),
            KeyRowKey::tab(),
            KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
            KeyRowKey::text("/"),
            KeyRowKey::Arrows(self.arrows.clone()),
            KeyRowKey::Hide,
        ])
        .track_scroll(&self.scroll)
        .on_key(|_, _, _| {})
        .on_hide(|_, _| {})
    }
}

impl Render for Chords {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(PHONE_WIDTH))
            .h(px(KEYS_HEIGHT))
            .relative()
            .overflow_hidden()
            .bg(cx.theme().background)
            .framed(cx)
            .rounded(cx.theme().radius)
            .child(page_lines(cx))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(8.0))
                    .child(self.row()),
            )
    }
}
