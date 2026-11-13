use std::sync::Arc;

use ala::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement as _,
    Render, ScrollHandle, Styled as _, Window, div, prelude::*, px, white,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, IconName, StyledExt as _, TextStyle,
    blocks::compact::{
        BENTO_INSET, BentoSpan, PhoneTile, PillLabel, PillPlacement, bento_grid,
        compact_bar_button, compact_floating_bar, compact_floating_group, compact_floating_pill,
        compact_hud, folding_title_bar, pill_glass_over,
    },
    blocks::settings::large_title,
    blocks::touch::{
        ArrowPad, KeyRow, KeyRowKey, PageDot, StackChip, StackClose, StackGroup, StackKey,
        StackKeyTap, StackLook, StackMenu, WhichKeyList, page_dots,
    },
    blocks::which_key::WhichKeyRow,
    v_flex,
};

use crate::story::{Framed as _, Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "compact",
    name: "Phone chrome",
    group: "Kit",
    summary: "The compact pieces for a phone, at phone width: the floating pill and its menu, page dots, the HUD, the keys over the keyboard, the bindings list, the bento grid and the title that folds into a bar.",
    sections: &[
        Section {
            id: "pill",
            name: "Pill",
            summary: "compact_floating_pill in compact_floating_bar: an icon, a title and a line under it, with page dots. It turns amber when the page needs attention, and over a light page pill_glass_over picks its glass.",
            build: |_, cx| stateless(pills, cx),
        },
        Section {
            id: "page-dots",
            name: "Page dots",
            summary: "page_dots: one group per window, one dot per page. The current page is the wide dot; a page asking for attention is amber.",
            build: |_, cx| stateless(dots, cx),
        },
        Section {
            id: "pill-menu",
            name: "Pill menu",
            summary: "The pill's StackMenu, tapped: the page dims, the pill turns into the close and the chips rise above it in groups, nearest first. Chips can be quiet or danger.",
            build: |_, cx| cx.new(Menus::new).into(),
        },
        Section {
            id: "hud",
            name: "HUD",
            summary: "compact_hud: the transient message centred over the page.",
            build: |_, cx| stateless(huds, cx),
        },
        Section {
            id: "keys",
            name: "Keys",
            summary: "StackKey at rest and armed, a stack key with nothing behind it, and the ArrowPad at rest.",
            build: |_, cx| cx.new(Keys::new).into(),
        },
        Section {
            id: "key-row",
            name: "Key row",
            summary: "KeyRow over the soft keyboard: plain keys and the arrow pad in a pill that scrolls sideways when it does not fit.",
            build: |_, cx| cx.new(KeyRows::new).into(),
        },
        Section {
            id: "bindings",
            name: "Bindings list",
            summary: "WhichKeyList: a key table as a touch list, grouped, your own keys in the accent color.",
            build: |_, cx| stateless(bindings, cx),
        },
        Section {
            id: "bento",
            name: "Bento grid",
            summary: "bento_grid packs bento_tile cards of different spans into four columns under a large title, with bento_add_tile last and a settings button grouped on the floating bar.",
            build: |_, cx| stateless(bento, cx),
        },
        Section {
            id: "folding-title",
            name: "Folding title",
            summary: "folding_title_bar: once the page scrolls the large title away, a blurred bar takes it small and centered.",
            build: |_, cx| stateless(folding, cx),
        },
    ],
};

const PHONE_WIDTH: f32 = 390.0;
const GRID: f32 = PHONE_WIDTH - BENTO_INSET * 2.0;
const MENU_HEIGHT: f32 = 560.0;
const NARROW_WIDTH: f32 = 220.0;
const SCROLLED: f32 = 72.0;

const PAGE_LINES: [&str; 6] = [
    "$ make test",
    "running 128 tests",
    "test parser::nested ... ok",
    "test parser::escapes ... ok",
    "test result: ok. 128 passed",
    "$ ",
];

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

fn groups() -> Vec<Vec<PageDot>> {
    vec![
        vec![dot(false, false), dot(true, false)],
        vec![dot(false, false), dot(false, true), dot(false, false)],
        vec![dot(false, false)],
    ]
}

fn placement() -> PillPlacement {
    PillPlacement::new(Some(px(55.0)), px(34.0))
}

fn page_lines(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .px(px(12.0))
        .py(px(8.0))
        .type_style_mono(TextStyle::Small, cx)
        .text_color(theme.foreground)
        .children(PAGE_LINES.map(|line| div().h(TextStyle::Small.line_height()).child(line)))
}

fn pill(
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
            word: ala::SharedString::default(),
            meta: meta.to_owned().into(),
            tone: if warning {
                penna::Tone::Warning
            } else {
                penna::Tone::Neutral
            },
        },
        page_dots(&groups(), cx),
        &placement,
        cx,
    );
    phone(compact_floating_bar(pill, &placement, cx), cx).when_some(page, Styled::bg)
}

fn pills(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state(
            "rest",
            pill("pill-rest", "make test", "work · build", false, None, cx),
        )
        .state(
            "attention",
            pill(
                "pill-warning",
                "python train.py",
                "exited with 1 · lab",
                true,
                None,
                cx,
            ),
        )
        .state(
            "long title",
            pill(
                "pill-long",
                "review · reading src/settings/navigation.rs and its tests",
                "work · 2 of 4 pages",
                false,
                None,
                cx,
            ),
        )
        .state(
            "over a white page",
            pill(
                "pill-page",
                "Example",
                "www.example.com",
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
        .state("one page", page_dots(&[vec![dot(true, false)]], cx))
        .state(
            "one window, four pages",
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

fn menu(groups: Vec<StackGroup>, cx: &mut App) -> Entity<StackMenu> {
    cx.new(|cx| {
        let mut menu = StackMenu::new(StackClose::Disc, cx);
        menu.set_groups(groups, cx);
        menu.open(cx);
        menu
    })
}

struct Menus {
    page: Entity<StackMenu>,
    home: Entity<StackMenu>,
}

impl Menus {
    fn new(cx: &mut Context<Self>) -> Self {
        let page = menu(
            vec![
                StackGroup::new([
                    StackChip::new("overview", "Overview", IconName::LayoutDashboard),
                    StackChip::new("new-page", "New page", IconName::Plus),
                    StackChip::new("new-window", "New window", IconName::AppWindow),
                    StackChip::new("rename", "Rename", IconName::Pencil),
                ]),
                StackGroup::new([
                    StackChip::new("close-page", "Close page", IconName::Xmark)
                        .look(StackLook::Danger),
                    StackChip::new("all-keys", "All keys", IconName::Keyboard)
                        .look(StackLook::Quiet),
                ]),
            ],
            cx,
        );
        let home = menu(
            vec![
                StackGroup::new([
                    StackChip::new("search", "Search", IconName::Search),
                    StackChip::new("new-file", "New file", IconName::File),
                    StackChip::new("new-folder", "New folder", IconName::Folder),
                ]),
                StackGroup::new([StackChip::new("settings", "Settings", IconName::Settings)
                    .look(StackLook::Quiet)]),
            ],
            cx,
        );
        cx.observe(&page, |_, _, cx| cx.notify()).detach();
        cx.observe(&home, |_, _, cx| cx.notify()).detach();
        Self { page, home }
    }

    fn phone(id: &'static str, menu: &Entity<StackMenu>, cx: &App) -> impl IntoElement {
        let placement = placement();
        let pill = compact_floating_pill(
            id,
            PillLabel {
                icon: IconName::SquareTerminal,
                title: "make test".into(),
                word: ala::SharedString::default(),
                meta: "work · build".into(),
                tone: penna::Tone::Neutral,
            },
            page_dots(&groups(), cx),
            &placement,
            cx,
        )
        .opacity(1.0 - menu.read(cx).close_shown())
        .child(menu.read(cx).track_anchor());
        phone(
            div()
                .relative()
                .h(px(MENU_HEIGHT))
                .child(page_lines(cx))
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
}

impl Render for Menus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        states()
            .columns(2)
            .state("on a page", Self::phone("menu-page", &self.page, cx))
            .state("on the home page", Self::phone("menu-home", &self.home, cx))
    }
}

fn hud(text: &'static str, cx: &App) -> impl IntoElement {
    phone(
        div()
            .relative()
            .h(px(140.0))
            .p(px(12.0))
            .type_style_mono(TextStyle::Small, cx)
            .text_color(cx.theme().foreground.muted())
            .child("~/notes > ls")
            .child(compact_hud(text, cx)),
        cx,
    )
}

fn huds(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    states()
        .columns(2)
        .state("short", hud("Copied", cx))
        .state("longer", hud("Page 3 of 5 · zoomed", cx))
        .into_any_element()
}

struct Keys {
    rest: Entity<StackKey>,
    armed: Entity<StackKey>,
    plain: Entity<StackKey>,
    arrows: Entity<ArrowPad>,
}

impl Keys {
    fn new(cx: &mut Context<Self>) -> Self {
        let stacked = |cx: &mut Context<Self>, armed: bool| {
            let key = cx.new(|cx| {
                StackKey::new("ctrl", StackKeyTap::Open, cx).groups(
                    vec![StackGroup::new([StackChip::chord(
                        "ctrl-c",
                        "^C",
                        "Interrupt",
                    )])],
                    cx,
                )
            });
            key.update(cx, |key, cx| key.set_armed(armed, cx));
            key
        };
        Self {
            rest: stacked(cx, false),
            armed: stacked(cx, true),
            plain: cx.new(|cx| StackKey::new("menu", StackKeyTap::Open, cx)),
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
                "stack key, rest",
                row().child(slot("key-rest", self.rest.clone().into_any_element())),
            )
            .state(
                "stack key, armed",
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

struct KeyRows {
    rows: [(Entity<ArrowPad>, ScrollHandle); 3],
}

impl KeyRows {
    fn new(cx: &mut Context<Self>) -> Self {
        let rows = std::array::from_fn(|_| (cx.new(ArrowPad::new), ScrollHandle::new()));
        rows[2].1.set_offset(ala::point(px(-1000.0), px(0.0)));
        Self { rows }
    }

    fn row(&self, index: usize) -> KeyRow {
        let (arrows, scroll) = &self.rows[index];
        KeyRow::new(vec![
            KeyRowKey::tab(),
            KeyRowKey::named("esc", "escape").icon(IconName::KeyEscape),
            KeyRowKey::text("/"),
            KeyRowKey::text("-"),
            KeyRowKey::text("~"),
            KeyRowKey::Arrows(arrows.clone()),
            KeyRowKey::Hide,
        ])
        .track_scroll(scroll)
        .on_key(|_, _, _| {})
        .on_hide(|_, _| {})
    }
}

impl Render for KeyRows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        states()
            .columns(2)
            .state(
                "fits",
                phone(div().id("key-row-wide").child(self.row(0)), cx),
            )
            .state(
                "narrow: more keys ahead",
                phone(div().id("key-row-narrow").child(self.row(1)), cx).w(px(NARROW_WIDTH)),
            )
            .state(
                "narrow, scrolled to the end",
                phone(div().id("key-row-end").child(self.row(2)), cx).w(px(NARROW_WIDTH)),
            )
    }
}

fn bindings(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let rows: Arc<[WhichKeyRow]> = super::which_key::prefix_rows()
        .into_iter()
        .filter(|row| {
            row.group
                .as_ref()
                .is_some_and(|group| matches!(group.as_ref(), "Windows" | "Yours"))
        })
        .collect();
    phone(
        v_flex()
            .id("bindings-list")
            .h(px(420.0))
            .overflow_y_scroll()
            .child(WhichKeyList::new(rows)),
        cx,
    )
    .into_any_element()
}

const TILES: [(&str, &str, u8, u8); 4] = [
    ("Inbox", "12 unread", 2, 2),
    ("Drafts", "3 notes", 2, 1),
    ("Archive", "Last week", 2, 1),
    ("Shared", "With 4 people", 4, 1),
];

fn bento(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let mut tiles = TILES
        .iter()
        .enumerate()
        .map(|(index, (title, detail, columns, rows))| {
            (
                BentoSpan::new(usize::from(*columns), usize::from(*rows)),
                PhoneTile::new(("bento-tile", index), *title)
                    .foot(*detail)
                    .into_any_element(),
            )
        })
        .collect::<Vec<_>>();
    tiles.push((
        BentoSpan::new(4, 1),
        PhoneTile::add("bento-add", "Add folder").into_any_element(),
    ));
    let placement = placement();
    let bar = compact_floating_bar(div().flex_1(), &placement, cx).child(compact_floating_group(
        [compact_bar_button("bento-settings", IconName::Settings)],
        &placement,
        cx,
    ));
    phone(
        v_flex()
            .child(
                v_flex()
                    .pb(px(16.0))
                    .child(large_title("Library", cx))
                    .child(div().px(px(BENTO_INSET)).child(bento_grid(px(GRID), tiles))),
            )
            .child(bar),
        cx,
    )
    .into_any_element()
}

fn folding(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let page = |scrolled: f32| {
        phone(
            div()
                .relative()
                .h(px(200.0))
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .top(px(-scrolled))
                        .left_0()
                        .right_0()
                        .child(large_title("Library", cx))
                        .child(page_lines(cx))
                        .child(page_lines(cx)),
                )
                .child(folding_title_bar("Library", scrolled, px(0.0), cx)),
            cx,
        )
    };
    states()
        .columns(2)
        .state("at the top", page(0.0))
        .state("scrolled, the title folded into the bar", page(SCROLLED))
        .into_any_element()
}
