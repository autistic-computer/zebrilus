use ala::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, Window};
use penna::{
    ActiveTheme as _, Ground, Kind, Label, Space, StyledExt as _, Surface, TextStyle, Tone, h_flex,
    v_flex,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "type",
    name: "Type",
    group: "Foundation",
    summary: "Nine text styles. Each one sets size, line height and weight, and Mono also the code font. They are measured in rems, so the interface font size scales all of them at once.",
    sections: &[
        Section {
            id: "scale",
            name: "Scale",
            summary: "Label::new(style, text) for a line of text. strong raises the weight of the small styles; the headings are already heavy.",
            build: |_, cx| stateless(scale, cx),
        },
        Section {
            id: "ink",
            name: "Ink",
            summary: "Text inherits its ink. muted and faint mix the ink toward the ground it sits on, so a label on a card passes .on(ground) to stay readable there.",
            build: |_, cx| stateless(ink, cx),
        },
        Section {
            id: "inherited",
            name: "Inherited",
            summary: ".type_style(style, cx) on a container sets the style its text children inherit.",
            build: |_, cx| stateless(inherited, cx),
        },
        Section {
            id: "code",
            name: "Code",
            summary: "Label::mono() and .type_style_mono(style, cx) set any style in the code font at its own size. Mono is Body in the code font.",
            build: |_, cx| stateless(code, cx),
        },
    ],
};

fn scale(_: &mut Window, _: &mut App) -> AnyElement {
    TextStyle::ALL
        .into_iter()
        .fold(states(), |states, style| {
            states.state(
                format!(
                    "{} · {}/{}",
                    style.name(),
                    style.design_px(),
                    style.line_height_px()
                ),
                h_flex()
                    .gap(Space::S8)
                    .child(Label::new(style, "Drag one onto the desk"))
                    .child(Label::new(style, "Strong").strong()),
            )
        })
        .into_any_element()
}

fn ink(_: &mut Window, cx: &mut App) -> AnyElement {
    let card = cx.theme().ground_of(Surface::new(Kind::Card.role()));
    states()
        .state("on the window", lines(None))
        .state(
            "on a card, with .on(ground)",
            v_flex()
                .p(Space::S8)
                .surface(Surface::new(Kind::Card.role()), cx)
                .child(lines(Some(card))),
        )
        .state(
            "tones",
            h_flex().gap(Space::S8).children(
                Tone::ALL
                    .into_iter()
                    .map(|tone| Label::new(TextStyle::Body, tone.name()).tone(tone)),
            ),
        )
        .into_any_element()
}

fn lines(ground: Option<Ground>) -> AnyElement {
    let on = |label: Label| match ground {
        Some(ground) => label.on(ground),
        None => label,
    };
    v_flex()
        .gap(Space::S2)
        .child(on(Label::new(TextStyle::Body, "Ready")))
        .child(on(
            Label::new(TextStyle::Small, "ubuntu is to the right").muted()
        ))
        .child(on(
            Label::new(TextStyle::Caption, "Last seen a minute ago").faint()
        ))
        .into_any_element()
}

fn inherited(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "caption",
            v_flex()
                .type_style(TextStyle::Caption, cx)
                .child("Every line here is a plain string")
                .child("and takes the container's style"),
        )
        .state(
            "mono",
            v_flex()
                .type_style(TextStyle::Mono, cx)
                .child("cargo test -p nid-app")
                .child("just libs pull penna"),
        )
        .into_any_element()
}

fn code(_: &mut Window, cx: &mut App) -> AnyElement {
    [TextStyle::Micro, TextStyle::Caption, TextStyle::Small]
        .into_iter()
        .fold(states(), |states, style| {
            states.state(
                format!("{} · mono", style.name()),
                h_flex()
                    .gap(Space::S8)
                    .child(Label::new(style, "git log --oneline").mono())
                    .child(
                        v_flex()
                            .type_style_mono(style, cx)
                            .child("cargo test -p nid-app"),
                    ),
            )
        })
        .into_any_element()
}
