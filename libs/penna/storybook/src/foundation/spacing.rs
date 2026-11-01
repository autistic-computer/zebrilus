use ala::{AnyElement, App, IntoElement, ParentElement as _, Styled as _, Window, div};
use penna::{
    ActiveTheme as _, Emphasis, Kind, Label, PaintExt as _, PaintState, Space, StyledExt as _,
    TextStyle, Tone, h_flex, v_flex,
};

use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "spacing",
    name: "Spacing",
    group: "Foundation",
    summary: "One spacing scale on a 2px step, from S1 (2) to S16 (32). Space converts to a length, so it goes straight into gap, padding and margin. Measured in rems, it scales with the interface font size.",
    sections: &[
        Section {
            id: "scale",
            name: "Scale",
            summary: "h_flex().gap(Space::S4).p(Space::S6)",
            build: |_, cx| stateless(scale, cx),
        },
        Section {
            id: "in-use",
            name: "In use",
            summary: "Gap between siblings and padding inside a card, at three steps.",
            build: |_, cx| stateless(in_use, cx),
        },
    ],
};

fn scale(_: &mut Window, cx: &mut App) -> AnyElement {
    let bar = cx.theme().paint(Tone::Accent, Emphasis::Solid);
    v_flex()
        .gap(Space::S4)
        .children(Space::ALL.into_iter().map(|space| {
            h_flex()
                .gap(Space::S6)
                .child(
                    div()
                        .w(Space::S16)
                        .child(Label::new(TextStyle::Mono, format!("{space:?}")).muted()),
                )
                .child(div().w(space).h(Space::S6).painted(bar, PaintState::Rest))
                .child(Label::new(TextStyle::Caption, format!("{}px", space.design_px())).faint())
        }))
        .into_any_element()
}

fn in_use(_: &mut Window, cx: &mut App) -> AnyElement {
    let card = cx.theme().ground_of(Kind::Card);
    [Space::S2, Space::S4, Space::S8]
        .into_iter()
        .fold(states(), |states, space| {
            states.state(
                format!("{space:?}"),
                v_flex()
                    .gap(space)
                    .p(space)
                    .surface(Kind::Card, cx)
                    .child(Label::new(TextStyle::Body, "ubuntu"))
                    .child(
                        Label::new(TextStyle::Small, "Connected · 1 screen")
                            .muted()
                            .on(card),
                    ),
            )
        })
        .into_any_element()
}
