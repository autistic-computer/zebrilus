use ala::{
    AnyElement, App, IntoElement, ParentElement as _, Styled as _, Transformation, Window, div,
    percentage, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Icon, IconName, Sizable as _, Size, Space, StyledExt as _,
    TextStyle, v_flex,
};

use crate::story::{Section, Story, row, stateless, states};

pub const STORY: Story = Story {
    id: "icons",
    name: "Icons",
    group: "Kit",
    summary: "penna's own glyphs, drawn for the theme radius and smoothing, and the app's logo. An Icon is medium (14) and takes the text color unless a size or color is set.",
    sections: &[
        Section {
            id: "gallery",
            name: "Gallery",
            summary: "Every IconName as Icon::new draws it with no size set, so at medium (14), named as in code.",
            build: |_, cx| stateless(gallery, cx),
        },
        Section {
            id: "sizes",
            name: "Sizes",
            summary: "small 12px, medium 14px, and an explicit Size(px).",
            build: |_, cx| stateless(sizes, cx),
        },
        Section {
            id: "colors-and-rotation",
            name: "Colors and rotation",
            summary: "text_color with the theme roots, and transform with a rotation. The transform is paint-only, so layout does not move.",
            build: |_, cx| stateless(colors_and_rotation, cx),
        },
    ],
};

fn gallery(_: &mut Window, cx: &mut App) -> AnyElement {
    let muted = cx.theme().foreground.muted();
    div()
        .grid()
        .grid_cols(6)
        .gap_x(Space::S12)
        .gap_y(Space::S16)
        .children(IconName::ALL.iter().map(|icon| {
            v_flex()
                .items_center()
                .gap(Space::S6)
                .min_w_0()
                .child(Icon::new(icon.clone()))
                .child(
                    div()
                        .type_style_mono(TextStyle::Caption, cx)
                        .text_color(muted)
                        .child(format!("{icon:?}")),
                )
        }))
        .into_any_element()
}

const SAMPLES: [&str; 5] = [
    IconName::SquareTerminal.path(),
    IconName::GitBranch.path(),
    IconName::Settings.path(),
    IconName::Bell.path(),
    IconName::Search.path(),
];

fn sizes(_: &mut Window, _: &mut App) -> AnyElement {
    [
        ("small", Size::Small),
        ("medium", Size::Medium),
        ("Size(px(32))", Size::Size(px(32.0))),
    ]
    .into_iter()
    .fold(states(), |states, (name, size)| {
        states.state(
            name,
            row().children(
                SAMPLES
                    .into_iter()
                    .chain([crate::catalog().logo])
                    .map(|path| Icon::empty().path(path).with_size(size)),
            ),
        )
    })
    .into_any_element()
}

fn colors_and_rotation(_: &mut Window, cx: &mut App) -> AnyElement {
    let theme = cx.theme();
    let colors = [
        theme.foreground,
        theme.foreground.muted(),
        theme.accent,
        theme.success,
        theme.warning,
        theme.danger,
    ];
    states()
        .columns(2)
        .state(
            "foreground, muted, accent, success, warning, danger",
            row().children(
                colors
                    .into_iter()
                    .map(|color| Icon::new(IconName::CircleCheck).size_6().text_color(color)),
            ),
        )
        .state(
            "0, 90, 180 and 270 degrees",
            row().children((0..4).map(|quarter| {
                Icon::new(IconName::ChevronRight)
                    .size_6()
                    .transform(Transformation::rotate(percentage(quarter as f32 * 0.25)))
            })),
        )
        .into_any_element()
}
