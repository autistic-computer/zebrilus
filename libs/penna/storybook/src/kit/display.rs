use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, IntoElement, Keystroke, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window, div, px,
};
use penna::{
    ActiveTheme as _, Card, CardLook, Colorize as _, Disableable as _, Emphasis, Icon, IconName,
    Label, Sizable as _, Space, StyledExt as _, TextStyle, Tone,
    components::badge::Badge,
    components::kbd::{Kbd, Keycap},
    components::list::ListItem,
    components::separator::Separator,
    components::spinner::Spinner,
    h_flex, v_flex,
};

use super::support::{Probe, hover, when_visible};
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "display",
    name: "Display",
    group: "Kit",
    summary: "Small read-only pieces: key caps, badges, labels, spinners, separators and list rows.",
    sections: &[
        Section {
            id: "kbd",
            name: "Kbd",
            summary: "One Keystroke as a pill. Mac platforms, including a web build in a Mac browser, draw glyphs; others spell modifiers out (Ctrl, Alt, Shift, Win). Keycap takes .emphasis(..) and .tone(..): solid lights it, as a picked answer's key; Keycap::icon holds one glyph in a square cap; .disabled(true) dims it with its row.",
            build: |_, cx| stateless(kbds, cx),
        },
        Section {
            id: "badge",
            name: "Badge",
            summary: "Badge::new(label) with .emphasis(..) and .tone(..). Its height is the line it sits in, its icon 12 at every size. .opaque() fills it with its ground's own fill, so it reads over a bubble or a picture.",
            build: |_, cx| stateless(badges, cx),
        },
        Section {
            id: "label",
            name: "Label",
            summary: "Label::new(style, text) with .muted(), .faint(), .tone(..), .strong() and .mono(), the code face at the style's own size. It wraps when it runs out of room, unless it truncates.",
            build: |_, cx| stateless(labels, cx),
        },
        Section {
            id: "label-ground",
            name: "Label on a card",
            summary: "Muted, faint and tone ink are worked out for the ground under the text: on a card, pass cx.theme().ground_of(look) to .on().",
            build: |_, cx| stateless(label_ground, cx),
        },
        Section {
            id: "spinner",
            name: "Spinner",
            summary: "The Loader glyph on a 800ms turn, at every size and in a few tints. It stops with motion off.",
            build: |_, cx| stateless(spinners, cx),
        },
        Section {
            id: "separator",
            name: "Separator",
            summary: "A one pixel rule that takes no room along its axis. A vertical rule needs a parent with a definite height; .tone(tone) draws it in a tone's ink.",
            build: |_, cx| stateless(separators, cx),
        },
        Section {
            id: "list-item",
            name: "List item",
            summary: "Resting, hovered, selected and disabled rows. The pointer rests on the hovered row.",
            build: |window, cx| ListRows::view(window, cx),
        },
    ],
};

fn key(source: &str) -> Kbd {
    Kbd::new(Keystroke::parse(source).unwrap_or_default())
        .id(SharedString::from(format!("kbd-{source}")))
}

fn kbds(_: &mut Window, _: &mut App) -> AnyElement {
    states()
        .columns(2)
        .state(
            "single keys",
            row()
                .child(key("a"))
                .child(key("escape"))
                .child(key("enter"))
                .child(key("tab"))
                .child(key("space"))
                .child(key("backspace")),
        )
        .state(
            "arrows and function keys",
            row()
                .child(key("up"))
                .child(key("down"))
                .child(key("left"))
                .child(key("right"))
                .child(key("f5"))
                .child(key("pageup")),
        )
        .state(
            "chords",
            row()
                .child(key("cmd-k"))
                .child(key("ctrl-b"))
                .child(key("cmd-shift-p"))
                .child(key("ctrl-alt-shift-cmd-z")),
        )
        .state(
            "lowercase",
            row()
                .child(key("cmd-shift-p").lowercase())
                .child(key("ctrl-a").lowercase()),
        )
        .state(
            "emphasis",
            row().children(Emphasis::ALL.map(|emphasis| {
                Keycap::new(emphasis.name())
                    .emphasis(emphasis)
                    .id(SharedString::from(format!("keycap-{}", emphasis.name())))
            })),
        )
        .state(
            "solid in each tone, a lit key",
            row().children(Tone::ALL.map(|tone| {
                Keycap::new(tone.name())
                    .tone(tone)
                    .emphasis(Emphasis::Solid)
                    .id(SharedString::from(format!("keycap-solid-{}", tone.name())))
            })),
        )
        .state(
            "small: rest, lit, a glyph, disabled and lit disabled",
            row()
                .child(Keycap::new("1").small().id("keycap-small"))
                .child(lit(Keycap::new("2")).id("keycap-small-lit"))
                .child(lit(Keycap::icon(IconName::Check)).id("keycap-small-glyph"))
                .child(
                    Keycap::new("3")
                        .small()
                        .disabled(true)
                        .id("keycap-small-disabled"),
                )
                .child(
                    lit(Keycap::new("4"))
                        .disabled(true)
                        .id("keycap-small-lit-disabled"),
                ),
        )
        .into_any_element()
}

fn lit(cap: Keycap) -> Keycap {
    cap.small().tone(Tone::Accent).emphasis(Emphasis::Solid)
}

fn badges(_: &mut Window, cx: &mut App) -> AnyElement {
    let bubble = cx.theme().ground_of(CardLook::Bubble);
    states()
        .columns(2)
        .state(
            "emphasis",
            row().children(
                Emphasis::ALL.map(|emphasis| Badge::new(emphasis.name()).emphasis(emphasis)),
            ),
        )
        .state(
            "tones, solid",
            row().children(
                Tone::ALL.map(|tone| Badge::new(tone.name()).tone(tone).emphasis(Emphasis::Solid)),
            ),
        )
        .state(
            "ghost, with icons",
            row().children(
                [
                    (Tone::Danger, IconName::CircleX, "3 failing"),
                    (Tone::Warning, IconName::TriangleAlert, "Waiting"),
                    (Tone::Success, IconName::CircleCheck, "Ready"),
                ]
                .map(|(tone, icon, label)| {
                    Badge::new(label)
                        .icon(icon)
                        .tone(tone)
                        .emphasis(Emphasis::Ghost)
                }),
            ),
        )
        .state(
            "medium and small",
            row()
                .child(
                    Badge::new("3 failing")
                        .icon(IconName::CircleX)
                        .tone(Tone::Danger),
                )
                .child(
                    Badge::new("3 failing")
                        .icon(IconName::CircleX)
                        .tone(Tone::Danger)
                        .small(),
                ),
        )
        .state(
            "soft and opaque on a bubble",
            Card::new("badge-bubble", CardLook::Bubble)
                .px(Space::S12)
                .py(Space::S8)
                .flex()
                .items_center()
                .gap(Space::S8)
                .child(Label::new(TextStyle::Body, "Review"))
                .child(Badge::new("2").tone(Tone::Danger).on(bubble))
                .child(Badge::new("2").tone(Tone::Danger).opaque().on(bubble)),
        )
        .into_any_element()
}

fn labels(_: &mut Window, _: &mut App) -> AnyElement {
    let sentence = "Keyboard and pointer follow the cursor to the next computer";
    states()
        .columns(2)
        .state(
            "full, muted, faint, strong",
            row()
                .child(Label::new(TextStyle::Body, "Ready"))
                .child(Label::new(TextStyle::Body, "ubuntu is to the right").muted())
                .child(Label::new(TextStyle::Body, "Last seen").faint())
                .child(Label::new(TextStyle::Body, "Paired").strong()),
        )
        .state(
            "tones",
            row().children(
                [
                    (Tone::Primary, "Picked"),
                    (Tone::Danger, "Failed"),
                    (Tone::Warning, "Waiting"),
                    (Tone::Success, "Ready"),
                    (Tone::Accent, "Linked"),
                ]
                .map(|(tone, word)| Label::new(TextStyle::Body, word).tone(tone)),
            ),
        )
        .state(
            "code, small and body",
            row()
                .child(Label::new(TextStyle::Small, "git log --oneline").mono())
                .child(Label::new(TextStyle::Body, "just libs pull penna").mono()),
        )
        .state(
            "wrapping and truncated, 180 wide",
            v_flex()
                .w(px(180.0))
                .gap(Space::S8)
                .child(Label::new(TextStyle::Body, sentence))
                .child(Label::new(TextStyle::Body, sentence).truncate()),
        )
        .into_any_element()
}

fn label_ground(_: &mut Window, cx: &mut App) -> AnyElement {
    let code = cx.theme().ground_of(CardLook::Code);
    states()
        .state(
            "full, muted, faint and danger on a code card",
            h_flex().child(
                Card::new("label-card", CardLook::Code)
                    .flex()
                    .items_center()
                    .gap(Space::S12)
                    .child(Label::new(TextStyle::Body, "Ready"))
                    .child(
                        Label::new(TextStyle::Body, "ubuntu is to the right")
                            .muted()
                            .on(code),
                    )
                    .child(Label::new(TextStyle::Body, "Last seen").faint().on(code))
                    .child(
                        Label::new(TextStyle::Body, "Failed")
                            .tone(Tone::Danger)
                            .on(code),
                    ),
            ),
        )
        .into_any_element()
}

fn spinners(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    states()
        .columns(2)
        .state(
            "sizes",
            row()
                .child(Spinner::new().id("spinner-small").small())
                .child(Spinner::new().id("spinner-medium"))
                .child(Spinner::new().id("spinner-32").with_size(px(32.0))),
        )
        .state(
            "colors",
            row()
                .child(
                    Spinner::new()
                        .id("spinner-muted")
                        .color(theme.foreground.muted()),
                )
                .child(Spinner::new().id("spinner-accent").color(theme.accent))
                .child(Spinner::new().id("spinner-success").color(theme.success))
                .child(Spinner::new().id("spinner-warning").color(theme.warning))
                .child(Spinner::new().id("spinner-danger").color(theme.danger)),
        )
        .state(
            "inline with text",
            h_flex()
                .gap(Space::S8)
                .type_style(TextStyle::Lead, cx)
                .child(
                    Spinner::new()
                        .id("spinner-inline")
                        .color(theme.foreground.muted()),
                )
                .child("Connecting to devbox"),
        )
        .into_any_element()
}

fn separators(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let muted = theme.foreground.muted();
    states()
        .columns(3)
        .state(
            "horizontal",
            v_flex()
                .gap(Space::S8)
                .type_style(TextStyle::Lead, cx)
                .child("Sessions")
                .child(Separator::horizontal().id("separator-horizontal"))
                .child(div().text_color(muted).child("work, scratch, logs")),
        )
        .state(
            "vertical, 20px parent",
            h_flex()
                .h(px(20.0))
                .gap(Space::S12)
                .type_style(TextStyle::Lead, cx)
                .child("macbook")
                .child(Separator::vertical().id("separator-vertical-1"))
                .child("work")
                .child(Separator::vertical().id("separator-vertical-2"))
                .child(div().text_color(muted).child("3 panes")),
        )
        .state(
            "danger tone",
            v_flex()
                .gap(Space::S8)
                .type_style(TextStyle::Lead, cx)
                .child("Danger zone")
                .child(
                    Separator::horizontal()
                        .id("separator-danger")
                        .tone(Tone::Danger),
                )
                .child(div().text_color(muted).child("Kill every session")),
        )
        .into_any_element()
}

struct ListRows {
    hovered: Probe,
    _bounds: Subscription,
}

impl ListRows {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let hovered = Probe::default();
        let probe = hovered.clone();
        when_visible(&hovered, window, move |window, cx| {
            hover(&probe, window, cx);
        });
        cx.new(|cx| Self {
            hovered,
            _bounds: cx.observe_window_bounds(window, |this: &mut Self, window, cx| {
                hover(&this.hovered, window, cx);
            }),
        })
        .into()
    }
}

fn list_row(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    detail: &'static str,
    cx: &App,
) -> ListItem {
    let muted = cx.theme().foreground.muted();
    ListItem::new(id).child(
        h_flex()
            .w_full()
            .gap(Space::S8)
            .child(Icon::new(icon).medium().text_color(muted))
            .child(div().flex_1().child(label))
            .child(
                div()
                    .type_style(TextStyle::Small, cx)
                    .text_color(muted)
                    .child(detail),
            ),
    )
}

impl Render for ListRows {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = v_flex()
            .w(px(360.0))
            .gap(Space::S2)
            .child(list_row(
                "row-rest",
                IconName::SquareTerminal,
                "work",
                "3 panes",
                cx,
            ))
            .child(
                div()
                    .relative()
                    .child(list_row(
                        "row-hover",
                        IconName::SquareTerminal,
                        "scratch",
                        "1 pane",
                        cx,
                    ))
                    .child(self.hovered.measure()),
            )
            .child(
                list_row(
                    "row-selected",
                    IconName::SquareTerminal,
                    "logs",
                    "2 panes",
                    cx,
                )
                .selected(true),
            )
            .child(
                list_row(
                    "row-disabled",
                    IconName::SquareTerminal,
                    "detached",
                    "gone",
                    cx,
                )
                .disabled(true),
            )
            .child(
                list_row(
                    "row-disabled-selected",
                    IconName::SquareTerminal,
                    "stale",
                    "gone",
                    cx,
                )
                .selected(true)
                .disabled(true),
            )
            .child(
                list_row(
                    "row-focused",
                    IconName::SquareTerminal,
                    "build",
                    "1 pane",
                    cx,
                )
                .focused(true),
            )
            .child(
                list_row(
                    "row-selected-focused",
                    IconName::SquareTerminal,
                    "deploy",
                    "4 panes",
                    cx,
                )
                .selected(true)
                .focused(true),
            );
        states().state(
            "resting, hovered, selected, disabled, disabled and selected, focused, selected and focused",
            list,
        )
    }
}
