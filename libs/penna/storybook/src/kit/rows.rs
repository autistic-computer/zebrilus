use ala::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};
use penna::{
    ActiveTheme as _, BarLook, Disableable as _, IconName, Label, PaintState, Row, RowLook, Space,
    Surface, Tab, TabLook, TextStyle, Tone, h_flex, icon::Icon, list::ListItem, v_flex,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "tabs-and-rows",
    name: "Tabs and rows",
    group: "Kit",
    summary: "Things you point at that show which one is open, current or picked. Each takes a state from the app and lights on hover and press while it rests; .interactive(false) paints the state alone.",
    sections: &[
        Section {
            id: "tabs",
            name: "Tabs",
            summary: "Tab::new(id, look).state(state) for window pills, browser tabs, the tray chips over a composer and a pane's title. Point at the resting ones to see them light.",
            build: |_, cx| stateless(tabs, cx),
        },
        Section {
            id: "rows",
            name: "Rows",
            summary: "Row::new(id, look).state(state).row_height(height) for menu, picker, sidebar and navigation rows. row_height sizes the corners only; the app sets the height.",
            build: |_, cx| stateless(rows, cx),
        },
        Section {
            id: "fills",
            name: "Row fills",
            summary: "A Row with .hover_group(group) is a fill set inside a taller row that lights when the whole row is pointed at. On a touch screen it draws the press highlight.",
            build: |_, cx| stateless(fills, cx),
        },
        Section {
            id: "list-items",
            name: "List items",
            summary: "ListItem::new(id).look(look) picks the row look, .row_height(height) the corners.",
            build: |_, cx| stateless(list_items, cx),
        },
    ],
};

const STATES: [(PaintState, &str); 5] = [
    (PaintState::Rest, "Rest"),
    (PaintState::Hover, "Hover"),
    (PaintState::Selected, "Selected"),
    (PaintState::Current, "Current"),
    (PaintState::Disabled, "Disabled"),
];

const TAB_LOOKS: [(TabLook, &str); 4] = [
    (TabLook::Window, "window"),
    (TabLook::Page, "page"),
    (TabLook::Tray, "tray"),
    (TabLook::Title, "title"),
];

fn tab(look: TabLook, name: &str, state: PaintState, label: &'static str, cx: &App) -> Tab {
    let ground = cx.theme().ground_at(look, state);
    Tab::new(SharedString::from(format!("tab-{name}-{label}")), look)
        .state(state)
        .h(px(28.0))
        .px(Space::S5)
        .flex()
        .items_center()
        .gap(Space::S3)
        .child(Icon::new(IconName::Terminal).size(px(14.0)))
        .child(Label::new(TextStyle::Small, label))
        .child(Label::new(TextStyle::Caption, "zz").muted().on(ground))
}

fn tabs(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let pane = cx.theme().ground_of(BarLook::Pane);
    TAB_LOOKS
        .iter()
        .fold(states(), |states, (look, name)| {
            states.state(
                *name,
                row().children(
                    STATES
                        .iter()
                        .map(|(state, label)| tab(*look, name, *state, label, cx)),
                ),
            )
        })
        .state(
            "not interactive",
            row()
                .child(
                    tab(TabLook::Window, "still", PaintState::Rest, "Rest", cx).interactive(false),
                )
                .child(
                    tab(
                        TabLook::Window,
                        "still",
                        PaintState::Selected,
                        "Selected",
                        cx,
                    )
                    .interactive(false),
                ),
        )
        .state(
            "title on a pane header",
            h_flex().h(px(36.0)).px(Space::S4).child(
                Tab::new("tab-title-on-pane", TabLook::Title)
                    .on(pane)
                    .h(px(24.0))
                    .px(Space::S4)
                    .flex()
                    .items_center()
                    .child(
                        Label::new(TextStyle::Small, "~/dev/zebrilus")
                            .muted()
                            .on(pane),
                    ),
            ),
        )
        .into_any_element()
}

const ROW_LOOKS: [(RowLook, &str); 6] = [
    (RowLook::Menu, "menu"),
    (RowLook::MenuCard, "menu card"),
    (RowLook::MenuTint, "menu tint"),
    (RowLook::Sidebar, "sidebar"),
    (RowLook::Nav, "nav"),
    (RowLook::Cursor, "cursor"),
];

fn list_row(look: RowLook, name: &str, state: PaintState, label: &'static str) -> Row {
    let height = px(32.0);
    Row::new(SharedString::from(format!("row-{name}-{label}")), look)
        .state(state)
        .row_height(height)
        .h(height)
        .w(px(200.0))
        .px(Space::S6)
        .flex()
        .items_center()
        .gap(Space::S4)
        .child(Icon::new(IconName::Folder).size(px(14.0)))
        .child(Label::new(TextStyle::Small, label))
}

fn rows(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let danger = cx.theme().ground_at(
        Surface::from(RowLook::Menu).tone(Tone::Danger),
        PaintState::Selected,
    );
    ROW_LOOKS
        .iter()
        .fold(states().columns(2), |states, (look, name)| {
            states.state(
                *name,
                v_flex().gap(Space::S1).children(
                    STATES
                        .iter()
                        .filter(|(state, _)| *state != PaintState::Current)
                        .map(|(state, label)| list_row(*look, name, *state, label)),
                ),
            )
        })
        .state(
            "toned",
            v_flex()
                .gap(Space::S1)
                .child(
                    list_row(RowLook::Menu, "danger", PaintState::Rest, "Rest").tone(Tone::Danger),
                )
                .child(
                    list_row(RowLook::Menu, "danger", PaintState::Selected, "Selected")
                        .tone(Tone::Danger)
                        .child(Label::new(TextStyle::Caption, "Delete").muted().on(danger)),
                ),
        )
        .state(
            "not interactive",
            v_flex().gap(Space::S1).child(
                list_row(RowLook::Menu, "still", PaintState::Rest, "Rest").interactive(false),
            ),
        )
        .into_any_element()
}

fn fill_row(id: &'static str, look: RowLook, lit: PaintState, label: &'static str) -> AnyElement {
    let group = SharedString::from(format!("{id}-group"));
    div()
        .id(id)
        .group(group.clone())
        .relative()
        .w(px(240.0))
        .h(px(48.0))
        .px(Space::S8)
        .flex()
        .items_center()
        .child(
            Row::new(format!("{id}-fill"), look)
                .state(lit)
                .hover_group(group)
                .absolute()
                .top(px(2.0))
                .bottom(px(2.0))
                .left(Space::S2)
                .right(Space::S2),
        )
        .child(Label::new(TextStyle::Body, label).relative())
        .into_any_element()
}

fn fills(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "sidebar",
            v_flex()
                .child(fill_row(
                    "fill-sidebar-rest",
                    RowLook::Sidebar,
                    PaintState::Rest,
                    "Rest",
                ))
                .child(fill_row(
                    "fill-sidebar-current",
                    RowLook::Sidebar,
                    PaintState::Current,
                    "Current",
                )),
        )
        .state(
            "nav and cursor",
            v_flex()
                .child(fill_row(
                    "fill-nav-rest",
                    RowLook::Nav,
                    PaintState::Rest,
                    "Rest",
                ))
                .child(fill_row(
                    "fill-cursor",
                    RowLook::Cursor,
                    PaintState::Selected,
                    "Cursor",
                )),
        )
        .into_any_element()
}

const ITEM_LOOKS: [(RowLook, &str); 3] = [
    (RowLook::Menu, "menu"),
    (RowLook::Sidebar, "sidebar"),
    (RowLook::MenuTint, "menu tint"),
];

fn list_items(_: &mut Window, _: &mut App) -> AnyElement {
    ITEM_LOOKS
        .iter()
        .enumerate()
        .fold(states().columns(3), |states, (index, (look, name))| {
            states.state(
                *name,
                v_flex()
                    .w(px(200.0))
                    .child(
                        ListItem::new(("item-rest", index))
                            .look(*look)
                            .child("Resting"),
                    )
                    .child(
                        ListItem::new(("item-selected", index))
                            .look(*look)
                            .selected(true)
                            .child("Selected"),
                    )
                    .child(
                        ListItem::new(("item-disabled", index))
                            .look(*look)
                            .disabled(true)
                            .child("Disabled"),
                    )
                    .child(
                        ListItem::new(("item-tall", index))
                            .look(*look)
                            .row_height(px(40.0))
                            .h(px(40.0))
                            .selected(true)
                            .child("Selected, 40px"),
                    ),
            )
        })
        .into_any_element()
}
