use ala::{AnyElement, App, Div, IntoElement, ParentElement as _, Styled as _, Window, div, px};
use penna::{Space, center, inline, inset, split, stack, tiles};

use crate::story::{Framed as _, Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "layout",
    name: "Layout",
    group: "Foundation",
    summary: "Six primitives place everything. They take a gap or an inset token and draw nothing, so a screen is built from them and the components they hold.",
    sections: &[
        Section {
            id: "stack",
            name: "Stack",
            summary: "stack(Space::GAP_GROUP): children in a column, a gap apart. The gap names the relationship: tight, related, group or section.",
            build: |_, cx| stateless(stacks, cx),
        },
        Section {
            id: "inline",
            name: "Inline",
            summary: "inline(Space::GAP_RELATED): children in a row on one line. .flex_wrap() lets them wrap.",
            build: |_, cx| stateless(inlines, cx),
        },
        Section {
            id: "inset",
            name: "Inset",
            summary: "inset(Space::INSET_CARD): padding around one child, owned by the container.",
            build: |_, cx| stateless(insets, cx),
        },
        Section {
            id: "split",
            name: "Split",
            summary: "split(width, gap, side, main): a fixed side beside a main that takes the rest.",
            build: |_, cx| stateless(splits, cx),
        },
        Section {
            id: "center",
            name: "Center",
            summary: "center(measure): a column no wider than the measure, centered in the space it has.",
            build: |_, cx| stateless(centers, cx),
        },
        Section {
            id: "tiles",
            name: "Tiles",
            summary: "tiles(columns, gap): equal tiles that fill the width, a gap apart.",
            build: |_, cx| stateless(tile_grids, cx),
        },
    ],
};

const FRAME: f32 = 480.0;

fn frame(cx: &App) -> Div {
    div().w(px(FRAME)).framed(cx)
}

fn block(cx: &App) -> Div {
    div().h(Space::S24).framed(cx)
}

fn chip(cx: &App) -> Div {
    block(cx).w(Space::S64)
}

fn stacks(_: &mut Window, cx: &mut App) -> AnyElement {
    Space::GAPS
        .into_iter()
        .fold(states(), |states, (name, gap)| {
            states.state(
                name,
                frame(cx).child(stack(gap).children((0..3).map(|_| block(cx)))),
            )
        })
        .into_any_element()
}

fn inlines(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "related",
            frame(cx).child(inline(Space::GAP_RELATED).children((0..4).map(|_| chip(cx)))),
        )
        .state(
            "wrapping",
            frame(cx).child(
                inline(Space::GAP_RELATED)
                    .flex_wrap()
                    .children((0..9).map(|_| chip(cx))),
            ),
        )
        .into_any_element()
}

fn insets(_: &mut Window, cx: &mut App) -> AnyElement {
    Space::INSETS
        .into_iter()
        .fold(states(), |states, (name, space)| {
            states.state(name, frame(cx).child(inset(space).child(block(cx))))
        })
        .into_any_element()
}

fn splits(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "160 aside",
            frame(cx).child(split(
                px(160.0),
                Space::GAP_GROUP,
                block(cx),
                stack(Space::GAP_RELATED).children((0..3).map(|_| block(cx))),
            )),
        )
        .into_any_element()
}

fn centers(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "320 measure",
            frame(cx).child(
                center(px(320.0))
                    .child(stack(Space::GAP_RELATED).children((0..2).map(|_| block(cx)))),
            ),
        )
        .into_any_element()
}

fn tile_grids(_: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "3 columns",
            frame(cx).child(tiles(3, Space::GAP_RELATED).children((0..6).map(|_| block(cx)))),
        )
        .into_any_element()
}
