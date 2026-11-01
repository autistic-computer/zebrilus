use ala::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Window,
    div, px,
};
use penna::{
    ActiveTheme as _, Chip, Dock, IconName, Inspector, Label, PaintState, Sizable as _, Space,
    TextStyle, Tile, TileVariant, Tone, Well,
    button::Button,
    h_flex,
    settings::{SettingEntry, SettingsStack},
    switch::Switch,
    v_flex,
};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "canvas",
    name: "Canvas",
    group: "Kit",
    summary: "Pieces for a surface things are arranged on: the well, tiles drawn to scale on it, the dock that holds what can still be placed, and the inspector beside it. The app places them; the kit paints them.",
    sections: &[
        Section {
            id: "screens",
            name: "Screen tiles",
            summary: "Tile::new(id).variant(TileVariant::Screen) on a Well, in each state the app gives it.",
            build: |_, cx| stateless(screens, cx),
        },
        Section {
            id: "drop-and-veil",
            name: "Drop zones and veils",
            summary: "TileVariant::Drop marks where a dragged thing will land; TileVariant::Veil is the tint laid over panes while something is dragged. The caller sets the drop zone's corners.",
            build: |_, cx| stateless(drops, cx),
        },
        Section {
            id: "mini",
            name: "Mini tiles",
            summary: "TileVariant::Mini draws no fill of its own, so the caller's .bg and .rounded show; Selected rings the one a window opens to.",
            build: |_, cx| stateless(minis, cx),
        },
        Section {
            id: "dock",
            name: "Dock",
            summary: "Dock::new(title) with a subtitle and an action, over card tiles.",
            build: |_, cx| stateless(dock, cx),
        },
        Section {
            id: "inspector",
            name: "Inspector",
            summary: "Inspector::new(id, title) with a subtitle, a trailing mark, a close button and sections. It docks on the right by default and rules its inner edge.",
            build: |_, cx| stateless(inspector, cx),
        },
    ],
};

fn screen(id: &'static str, label: &'static str, tone: Tone, state: PaintState, cx: &App) -> Tile {
    Tile::new(id)
        .on(Well::ground(cx))
        .variant(TileVariant::Screen)
        .tone(tone)
        .state(state)
        .w(px(150.0))
        .h(px(90.0))
        .flex()
        .items_end()
        .p(Space::S3)
        .child(Label::new(TextStyle::Caption, label))
}

fn screens(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    Well::new()
        .p(Space::S12)
        .child(
            h_flex()
                .flex_wrap()
                .gap(Space::S8)
                .child(screen("rest", "Rest", Tone::Neutral, PaintState::Rest, cx))
                .child(screen(
                    "hover",
                    "Moving",
                    Tone::Neutral,
                    PaintState::Hover,
                    cx,
                ))
                .child(screen(
                    "selected",
                    "Selected",
                    Tone::Neutral,
                    PaintState::Selected,
                    cx,
                ))
                .child(screen(
                    "here",
                    "Pointer is here",
                    Tone::Primary,
                    PaintState::Rest,
                    cx,
                ))
                .child(screen(
                    "refused",
                    "Can't go there",
                    Tone::Danger,
                    PaintState::Hover,
                    cx,
                ))
                .child(screen(
                    "away",
                    "Away",
                    Tone::Neutral,
                    PaintState::Disabled,
                    cx,
                )),
        )
        .into_any_element()
}

fn pane(label: &'static str) -> ala::Div {
    div()
        .size_full()
        .p(Space::S4)
        .child(Label::new(TextStyle::Caption, label))
}

fn drops(_: &mut Window, cx: &mut App) -> AnyElement {
    let radius = cx.theme().radius;
    let area = |id: &'static str| div().id(id).relative().w(px(200.0)).h(px(120.0)).framed(cx);
    states()
        .columns(2)
        .state(
            "drop, selected",
            area("drop-area").child(pane("Right half")).child(
                Tile::new("drop")
                    .variant(TileVariant::Drop)
                    .state(PaintState::Selected)
                    .absolute()
                    .top(Space::S2)
                    .bottom(Space::S2)
                    .right(Space::S2)
                    .w(px(96.0))
                    .rounded(radius),
            ),
        )
        .state(
            "veil",
            area("veil-area").child(pane("Drag to move")).child(
                Tile::new("veil")
                    .variant(TileVariant::Veil)
                    .absolute()
                    .inset_0(),
            ),
        )
        .into_any_element()
}

fn mini(id: &'static str, state: PaintState, cx: &App) -> Tile {
    Tile::new(id)
        .variant(TileVariant::Mini)
        .state(state)
        .size_full()
        .p(Space::S3)
        .bg(cx.theme().background)
        .rounded(px(6.0))
        .child(Label::new(TextStyle::Micro, "zsh"))
}

fn minis(_: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    let window = |id: &'static str, state: PaintState| {
        h_flex()
            .w(px(140.0))
            .h(px(90.0))
            .gap(Space::S2)
            .child(mini(id, state, cx))
            .child(mini("other", PaintState::Rest, cx))
    };
    states()
        .columns(2)
        .state("rest", window("mini-rest", PaintState::Rest))
        .state("selected", window("mini-selected", PaintState::Selected))
        .into_any_element()
}

fn card(id: &'static str, name: &'static str, detail: &'static str, state: PaintState) -> Tile {
    Tile::new(id)
        .state(state)
        .w(px(132.0))
        .h(px(64.0))
        .px(Space::S5)
        .py(Space::S4)
        .flex()
        .flex_col()
        .justify_end()
        .child(Label::new(TextStyle::Body, name).strong().truncate())
        .child(Label::new(TextStyle::Caption, detail).truncate())
}

fn dock(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .state(
            "with computers nearby",
            Dock::new("Nearby")
                .subtitle("Drag one onto the desk, or click it to pick a side.")
                .header_height(px(48.0))
                .action(
                    Button::new("by-address")
                        .small()
                        .icon(IconName::Plus)
                        .label("By address…"),
                )
                .child(
                    h_flex()
                        .px(Space::S8)
                        .pb(Space::S8)
                        .gap(Space::S4)
                        .child(card("xps", "xps", "Linux · 1 screen", PaintState::Rest))
                        .child(card("picked", "thinkpad", "Picked", PaintState::Selected))
                        .child(card("carried", "mini", "Being dragged", PaintState::Hover))
                        .child(card(
                            "old",
                            "gaming-pc",
                            "Needs Nidus 0.8",
                            PaintState::Disabled,
                        )),
                ),
        )
        .state(
            "empty",
            Dock::new("Nearby")
                .subtitle("Computers running Nidus on this network show up here.")
                .header_height(px(48.0)),
        )
        .into_any_element()
}

fn inspector(_: &mut Window, _: &mut App) -> AnyElement {
    div()
        .h(px(420.0))
        .flex()
        .justify_end()
        .child(
            Inspector::new("inspector", "ubuntu")
                .subtitle("Connected · 2 screens")
                .trailing(
                    Chip::new("inspector-ready", "Ready")
                        .dot(Tone::Success)
                        .small(),
                )
                .on_close(|_, _, _| {})
                .child(
                    SettingsStack::titled("What ubuntu can do here")
                        .child(
                            SettingEntry::new("Control this computer")
                                .control(Switch::new("control").checked(true)),
                        )
                        .child(
                            SettingEntry::new("Share the clipboard")
                                .control(Switch::new("clipboard").checked(true)),
                        ),
                )
                .child(
                    v_flex()
                        .gap(Space::S2)
                        .child(Label::new(TextStyle::Small, "Screens").strong())
                        .child(Label::new(TextStyle::Small, "DP-1 24″ · HDMI-1 24″")),
                ),
        )
        .into_any_element()
}
