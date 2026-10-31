mod plates;

use ala::{
    AnyElement, App, ElementId, IntoElement, ParentElement as _, Styled as _, Window, div,
    prelude::*, px,
};
use penna::{ActiveTheme as _, Colorize as _, Elevation, h_flex, switch::Switch, v_flex};

use crate::story::{Framed as _, Section, Story};

pub use plates::{CONTENT, CONTROLS, FLOATING, PHONE, WINDOW};

pub const ID: &str = "theme-creator";

pub const PLATES: &[Section] = &[WINDOW, CONTROLS, FLOATING, CONTENT, PHONE];

pub const WIDE: f32 = 1040.0;
pub const MEDIUM: f32 = 680.0;
pub const GAP: f32 = 16.0;
pub const PLATE_PADDING: f32 = 20.0;
pub const PHONE_WIDTH: f32 = 390.0;
pub const SCREEN_RADIUS: f32 = 55.0;
pub const SAFE_BOTTOM: f32 = 34.0;

pub const READING: &str = "Pinned tabs keep a `28px` floor, and the rest share what is left:\n\n\
| Tabs | Pinned | Unpinned |\n\
| ---: | ---: | ---: |\n\
| 10 | 28px | 96px |\n\
| 30 | 28px | 31px |\n\n\
```rust\n\
let floor = px(28.0) * pinned;\n\
```";

pub(crate) const fn story(plates: &'static [Section]) -> Story {
    Story {
        id: ID,
        name: "Theme creator",
        group: "Foundation",
        summary: "Everything a theme touches, region by region. Tune the knobs, then copy the theme as JSON.",
        sections: plates,
    }
}

pub fn tiles(window: &Window, columns: Vec<Vec<AnyElement>>) -> impl IntoElement {
    let width = window.viewport_size().width.as_f32();
    let count = if width >= WIDE {
        3
    } else if width >= MEDIUM {
        2
    } else {
        1
    }
    .min(columns.len());
    let mut stacks: Vec<Vec<AnyElement>> = (0..count).map(|_| Vec::new()).collect();
    let mut next = 0;
    for (group, column) in columns.into_iter().enumerate() {
        for tile in column {
            let index = if count == 3 {
                group % count
            } else {
                next % count
            };
            next += 1;
            stacks[index].push(tile);
        }
    }
    h_flex().w_full().items_start().gap(px(GAP)).children(
        stacks
            .into_iter()
            .map(|stack| v_flex().flex_1().min_w_0().gap(px(GAP)).children(stack)),
    )
}

pub fn card(id: &'static str, cx: &App) -> ala::Stateful<ala::Div> {
    v_flex()
        .id(id)
        .w_full()
        .p(px(16.0))
        .gap_3()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background)
}

pub fn over_backdrop(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    let backdrop = crate::backdrop::backdrop(crate::knobs().backdrop, cx);
    div()
        .id(id)
        .relative()
        .w_full()
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().surface(Elevation::Panel).fill)
        .children(backdrop)
        .child(
            h_flex()
                .relative()
                .w_full()
                .p(px(PLATE_PADDING))
                .justify_center()
                .child(content),
        )
        .into_any_element()
}

pub fn heading(title: &'static str, description: &'static str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .font_weight(ala::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground.muted())
                .child(description),
        )
}

pub fn labeled(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground.muted())
                .child(label),
        )
        .child(control)
}

pub fn toggle(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .gap_3()
        .justify_between()
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.muted())
                        .child(description),
                ),
        )
        .child(
            Switch::new(ElementId::from(id))
                .checked(checked)
                .on_click(on_click),
        )
}

pub fn phone_frame(id: &'static str, cx: &App) -> ala::Stateful<ala::Div> {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .w_full()
        .max_w(px(PHONE_WIDTH))
        .overflow_hidden()
        .rounded(theme.radius)
        .framed(cx)
        .bg(theme.background)
}
