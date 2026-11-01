use ala::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use penna::{
    ActiveTheme as _, Bar, BarLook, Card, CardLook, Edge, IconName, Label, Panel, PanelLook,
    RuleLook, Space, StyledExt as _, TextStyle, Tone,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    h_flex,
    separator::Separator,
    v_flex,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "panels",
    name: "Panels",
    group: "Kit",
    summary: "Surfaces that hold other chrome and rest where they are put: floating panels, cards in a transcript, bars across a pane or window, and the rules between things. None of them lights on hover; the app sizes and places them, the kit paints them.",
    sections: &[
        Section {
            id: "panels",
            name: "Panels",
            summary: "Panel::new(id, look) for menus, popovers, raised cards inside a view, pickers, toasts, and an opaque cover. .inset(px) grows a rounded corner by the inset its rows sit at.",
            build: |_, cx| stateless(panels, cx),
        },
        Section {
            id: "cards",
            name: "Cards",
            summary: "Card::new(id, look).on(ground) for message bubbles, code, tables and image tiles. Pieces inside a card pass cx.theme().ground_of(look), like the badge here.",
            build: |_, cx| stateless(cards, cx),
        },
        Section {
            id: "bars",
            name: "Bars",
            summary: "Bar::new(id, look) for a strip across a sheet, a pane header or the title bar. Edge lines come from .rule(RuleLook::Bar, edge, cx) on the bar itself.",
            build: |_, cx| stateless(bars, cx),
        },
        Section {
            id: "rules",
            name: "Rules",
            summary: "Separator::horizontal().look(look) and .rule(look, edge, cx) on any element. Each look is drawn only where the theme draws that rule.",
            build: |_, cx| stateless(rules, cx),
        },
    ],
};

const PANEL_LOOKS: [(PanelLook, &str); 5] = [
    (PanelLook::Popover, "Popover"),
    (PanelLook::Inline, "Inline"),
    (PanelLook::Picker, "Picker"),
    (PanelLook::Toast, "Toast"),
    (PanelLook::Plain, "Plain"),
];

fn panel(id: &'static str, look: PanelLook, name: &'static str) -> Panel {
    Panel::new(id, look)
        .w(px(180.0))
        .p(Space::S6)
        .flex()
        .flex_col()
        .gap(Space::S2)
        .child(Label::new(TextStyle::Body, name).strong())
        .child(Label::new(TextStyle::Small, "What sits in it.").muted())
}

fn panels(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .state(
            "looks",
            row().children(PANEL_LOOKS.map(|(look, name)| panel(name, look, name))),
        )
        .state(
            "inset",
            row()
                .child(panel("picker-plain", PanelLook::Picker, "No inset"))
                .child(
                    Panel::new("picker-inset", PanelLook::Picker)
                        .inset(px(8.0))
                        .w(px(180.0))
                        .p(px(8.0))
                        .child(
                            div()
                                .p(Space::S4)
                                .child(Label::new(TextStyle::Body, "Inset 8px").strong()),
                        ),
                ),
        )
        .into_any_element()
}

fn cards(_: &mut Window, cx: &mut App) -> AnyElement {
    let bubble = cx.theme().ground_of(CardLook::Bubble);
    let code = cx.theme().ground_of(CardLook::Code);
    states()
        .columns(2)
        .state(
            "bubble",
            Card::new("card-bubble", CardLook::Bubble)
                .max_w(px(260.0))
                .px(Space::S6)
                .py(Space::S4)
                .flex()
                .items_center()
                .gap(Space::S4)
                .child(Label::new(
                    TextStyle::Body,
                    "Can you look at the failing test?",
                ))
                .child(Badge::new("2").tone(Tone::Danger).opaque().on(bubble)),
        )
        .state(
            "code",
            Card::new("card-code", CardLook::Code)
                .w(px(260.0))
                .p(Space::S6)
                .flex()
                .flex_col()
                .gap(Space::S2)
                .child(Label::new(TextStyle::Caption, "rust").muted().on(code))
                .child(Label::new(
                    TextStyle::Mono,
                    "let ground = theme.ground_of(look);",
                )),
        )
        .state(
            "figure",
            Card::new("card-figure", CardLook::Figure)
                .w(px(260.0))
                .p(Space::S6)
                .child(Label::new(TextStyle::Small, "Name · Size · Kind")),
        )
        .state(
            "image",
            Card::new("card-image", CardLook::Image)
                .size(px(72.0))
                .flex()
                .items_center()
                .justify_center()
                .child(Label::new(TextStyle::Caption, "image")),
        )
        .into_any_element()
}

fn bars(_: &mut Window, cx: &mut App) -> AnyElement {
    let picker = cx.theme().ground_of(PanelLook::Picker);
    let bar = |id: &'static str, look: BarLook, name: &'static str, cx: &App| {
        Bar::new(id, look)
            .w(px(420.0))
            .h(px(36.0))
            .px(Space::S6)
            .flex()
            .items_center()
            .justify_between()
            .rule(RuleLook::Bar, Edge::Bottom, cx)
            .child(Label::new(TextStyle::Body, name))
            .child(
                Button::new(format!("{id}-action"))
                    .ghost()
                    .icon(IconName::Plus),
            )
    };
    states()
        .state(
            "plain",
            bar("bar-plain", BarLook::Plain, "Sheet header", cx),
        )
        .state("pane", bar("bar-pane", BarLook::Pane, "Pane header", cx))
        .state(
            "window",
            bar("bar-window", BarLook::Window, "Title bar", cx),
        )
        .state(
            "on a picker",
            Panel::new("bar-picker", PanelLook::Picker)
                .w(px(420.0))
                .child(
                    Bar::new("bar-notice", BarLook::Plain)
                        .on(picker)
                        .px(Space::S6)
                        .py(Space::S4)
                        .rule(RuleLook::Bar, Edge::Bottom, cx)
                        .child(Label::new(
                            TextStyle::Caption,
                            "A notice across the top of a picker",
                        )),
                ),
        )
        .into_any_element()
}

const RULE_LOOKS: [(RuleLook, &str); 4] = [
    (RuleLook::Plain, "Plain"),
    (RuleLook::Bar, "Bar"),
    (RuleLook::List, "List"),
    (RuleLook::Tree, "Tree"),
];

fn rules(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "separators",
            v_flex()
                .w(px(240.0))
                .gap(Space::S6)
                .children(RULE_LOOKS.map(|(look, name)| {
                    v_flex()
                        .gap(Space::S2)
                        .child(Label::new(TextStyle::Caption, name).muted())
                        .child(Separator::horizontal().look(look))
                })),
        )
        .state(
            "edges",
            h_flex()
                .gap(Space::S6)
                .children(RULE_LOOKS.map(|(look, name)| {
                    div()
                        .size(px(56.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rule(look, Edge::Top, cx)
                        .rule(look, Edge::Left, cx)
                        .child(Label::new(TextStyle::Caption, name))
                })),
        )
        .into_any_element()
}
