use ala::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Window,
    div,
};
use penna::{
    ActiveTheme as _, Colorize as _, Icon, IconName, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "title-bar",
    name: "Title bar",
    group: "Kit",
    summary: "The window title bar. Children sit in the drag region; native window controls are drawn only on Windows and client-decorated Linux, so the web build shows none.",
    sections: &[Section {
        id: "bar",
        name: "Title bar",
        summary: "A bare bar, and one carrying a title, a path and trailing buttons. The left padding is 12px off macOS and clears the traffic lights on it.",
        build: |_, cx| stateless(bars, cx),
    }],
};

fn bars(_: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().foreground.muted();
    states()
        .state("empty", div().id("bar-empty").child(TitleBar::new()))
        .state(
            "title and actions",
            div().id("bar-full").child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .pr_2()
                        .justify_between()
                        .child(
                            h_flex()
                                .gap_2()
                                .text_sm()
                                .child(Icon::empty().path(crate::catalog().logo).medium())
                                .child("work")
                                .child(div().text_color(muted).child("~/dev/app")),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Button::new("bar-split")
                                        .icon(IconName::PanelRight)
                                        .ghost()
                                        .tooltip("Split right"),
                                )
                                .child(
                                    Button::new("bar-settings")
                                        .icon(IconName::Settings)
                                        .ghost()
                                        .tooltip("Settings"),
                                ),
                        ),
                ),
            ),
        )
        .into_any_element()
}
