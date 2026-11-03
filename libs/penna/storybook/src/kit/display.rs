use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, IntoElement, Keystroke, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Window, div, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, Sizable as _,
    StyledExt as _, TextStyle, components::kbd::Kbd, components::list::ListItem,
    components::separator::Separator, components::spinner::Spinner, h_flex, v_flex,
};

use super::support::{Probe, hover, when_visible};
use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "display",
    name: "Display",
    group: "Kit",
    summary: "Small read-only pieces: key caps, spinners, separators and list rows. Badges live in the Design system story.",
    sections: &[
        Section {
            id: "kbd",
            name: "Kbd",
            summary: "One Keystroke as a pill. Mac platforms, including a web build in a Mac browser, draw glyphs; others spell modifiers out (Ctrl, Alt, Shift, Win).",
            build: |_, cx| stateless(kbds, cx),
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
            summary: "A one pixel rule that takes no room along its axis. A vertical rule needs a parent with a definite height.",
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
                .gap_2()
                .text_sm()
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
                .gap_2()
                .text_sm()
                .child("Sessions")
                .child(Separator::horizontal().id("separator-horizontal"))
                .child(div().text_color(muted).child("work, scratch, logs")),
        )
        .state(
            "vertical, 20px parent",
            h_flex()
                .h(px(20.0))
                .gap_3()
                .text_sm()
                .child("macbook")
                .child(Separator::vertical().id("separator-vertical-1"))
                .child("work")
                .child(Separator::vertical().id("separator-vertical-2"))
                .child(div().text_color(muted).child("3 panes")),
        )
        .state(
            "custom color",
            v_flex()
                .gap_2()
                .text_sm()
                .child("Danger zone")
                .child(
                    Separator::horizontal()
                        .id("separator-danger")
                        .color(theme.danger),
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
            .gap_2()
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
            .gap_0p5()
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
            );
        states().state(
            "resting, hovered, selected, disabled, disabled and selected",
            list,
        )
    }
}
