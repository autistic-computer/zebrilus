use ala::{
    AnyElement, App, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, Icon, PaintState, Sizable as _, Size,
    Tone,
    button::{Button, ButtonVariants as _},
    h_flex,
    icon::glyphs,
    v_flex,
};

use crate::story::Framed as _;
use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "icon-set",
    name: "Icon set",
    group: "Foundation",
    summary: "Every penna icon drawn from the theme, adapted from Solar (src/icon/solar.json). Corners take half the theme radius in 24-grid units and the fixed corner smoothing, capped at 0.45 of the shorter side like every corner the kit draws. Radius 0 is flat: sharp corners, square caps, miter joins.",
    sections: &[
        Section {
            id: "catalog",
            name: "Catalog",
            summary: "Every icon by group at 24px: its name, then the Solar icon it comes from, or penna for the ones drawn in Solar's style.",
            build: |_, cx| stateless(catalog, cx),
        },
        Section {
            id: "follows-the-theme",
            name: "Follows the theme",
            summary: "The set at 20px for the current Radius knob.",
            build: |_, cx| stateless(follows_the_theme, cx),
        },
        Section {
            id: "radius-ramp",
            name: "Radius ramp",
            summary: "The same glyphs at theme radius 0, 2, 4, 6, 8, 12, 16 and 25.",
            build: |_, cx| stateless(radius_ramp, cx),
        },
        Section {
            id: "sizes",
            name: "Sizes",
            summary: "12, 14, 16 and 20px at the current radius.",
            build: |_, cx| stateless(sizes, cx),
        },
        Section {
            id: "in-chrome",
            name: "In chrome",
            summary: "A sidebar whose rows and icons share the theme radius.",
            build: |_, cx| stateless(in_chrome, cx),
        },
        Section {
            id: "construction",
            name: "Construction",
            summary: "48px at the current radius, named, to read the geometry.",
            build: |_, cx| stateless(construction, cx),
        },
    ],
};

#[derive(Clone, Copy)]
enum Radius {
    Theme,
    At(f32, f32),
}

impl Radius {
    fn icon(self, name: &str) -> Icon {
        let path = format!("icons/{name}.svg");
        let path = match self {
            Self::Theme => path.into(),
            Self::At(radius, smoothing) => {
                glyphs::path(&path, radius, smoothing).unwrap_or_else(|| path.into())
            }
        };
        Icon::empty().path(path)
    }
}

fn strip(radius: Radius, size: f32) -> impl IntoElement {
    h_flex()
        .flex_wrap()
        .gap(px((size * 0.8).max(12.0)))
        .children(
            glyphs::names().map(move |name| radius.icon(name).with_size(Size::Size(px(size)))),
        )
}

fn follows_the_theme(_: &mut Window, cx: &mut App) -> AnyElement {
    let radius = f32::from(cx.theme().radius);
    states()
        .state(format!("radius {radius}"), strip(Radius::Theme, 20.0))
        .into_any_element()
}

fn radius_ramp(_: &mut Window, _: &mut App) -> AnyElement {
    let smoothing = penna::CORNER_SMOOTHING;
    [0.0, 2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 25.0]
        .into_iter()
        .fold(states(), |states, radius| {
            states.state(
                format!("radius {radius}"),
                strip(Radius::At(radius, smoothing), 24.0),
            )
        })
        .into_any_element()
}

fn sizes(_: &mut Window, _: &mut App) -> AnyElement {
    [12.0, 14.0, 16.0, 20.0]
        .into_iter()
        .fold(states(), |states, size| {
            states.state(format!("{size}px"), strip(Radius::Theme, size))
        })
        .into_any_element()
}

fn catalog(_: &mut Window, cx: &mut App) -> AnyElement {
    let mut groups: Vec<(&str, Vec<glyphs::Glyph>)> = Vec::new();
    for glyph in glyphs::catalog() {
        match groups.iter_mut().find(|(group, _)| *group == glyph.group) {
            Some((_, members)) => members.push(glyph),
            None => groups.push((glyph.group, vec![glyph])),
        }
    }
    groups
        .into_iter()
        .fold(states(), |states, (group, members)| {
            states.state(
                format!("{group} ({})", members.len()),
                h_flex()
                    .flex_wrap()
                    .gap_3()
                    .children(members.iter().map(|glyph| card(*glyph, 24.0, cx))),
            )
        })
        .into_any_element()
}

fn construction(_: &mut Window, cx: &mut App) -> AnyElement {
    h_flex()
        .flex_wrap()
        .gap_3()
        .children(glyphs::catalog().map(|glyph| card(glyph, 48.0, cx)))
        .into_any_element()
}

fn card(glyph: glyphs::Glyph, size: f32, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let label = |text: String| {
        div()
            .w_full()
            .text_center()
            .truncate()
            .text_size(px(11.0))
            .line_height(px(14.0))
            .font_family(theme.mono_font_family.clone())
            .child(text)
    };
    v_flex()
        .w(px(size * 2.0 + 72.0))
        .items_center()
        .gap_1()
        .child(
            h_flex()
                .w_full()
                .h(px(size * 2.0))
                .justify_center()
                .rounded(theme.radius)
                .framed(cx)
                .child(
                    Radius::Theme
                        .icon(glyph.name)
                        .with_size(Size::Size(px(size))),
                ),
        )
        .child(label(glyph.name.to_owned()))
        .child(
            label(glyph.solar.unwrap_or("penna").to_owned()).text_color(theme.foreground.muted()),
        )
}

fn in_chrome(_: &mut Window, cx: &mut App) -> AnyElement {
    sidebar(cx).into_any_element()
}

fn sidebar(cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let button = |name: &'static str| Button::new(name).icon(Radius::Theme.icon(name)).ghost();
    let item = |icon: &'static str, label: &'static str, selected: bool| {
        h_flex()
            .h(px(28.0))
            .px_2()
            .gap_2()
            .rounded(theme.radius)
            .text_size(px(13.0))
            .text_color(theme.foreground.muted())
            .when(selected, |item| {
                item.bg(theme
                    .paint(Tone::Neutral, Emphasis::Ghost)
                    .fill_at(PaintState::Current))
                    .text_color(theme.foreground)
            })
            .child(Radius::Theme.icon(icon).with_size(Size::Size(px(16.0))))
            .child(div().child(label))
    };
    v_flex()
        .w(px(232.0))
        .p_1p5()
        .gap_0p5()
        .rounded(theme.radius * 1.5)
        .framed(cx)
        .bg(theme.surface(Elevation::Panel).fill)
        .child(
            h_flex()
                .justify_between()
                .pb_1()
                .child(button("panel-left"))
                .child(
                    h_flex()
                        .gap_0p5()
                        .child(button("search"))
                        .child(button("plus")),
                ),
        )
        .child(item("terminal", "zsh", true))
        .child(item("globe", "example.com", false))
        .child(item("bot", "claude", false))
        .child(item("folder", "~/dev/app", false))
        .child(item("git-branch", "icon-set", false))
        .child(
            h_flex()
                .justify_between()
                .pt_1()
                .child(
                    h_flex()
                        .gap_0p5()
                        .child(button("settings"))
                        .child(button("bell")),
                )
                .child(
                    h_flex()
                        .gap_0p5()
                        .child(button("chevron-right"))
                        .child(button("xmark")),
                ),
        )
}
