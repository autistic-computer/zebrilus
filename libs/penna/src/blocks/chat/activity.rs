use ala::{
    App, Div, ElementId, Hsla, Pixels, SharedString, Stateful, Styled, div, prelude::*, px,
    relative,
};

use crate::{
    ActiveTheme as _, Colorize as _, Emphasis, Ground, Icon, IconName, Sizable as _,
    StyledExt as _, TextStyle, Tone, components::badge::Badge, components::icon::cap_center_drop,
    components::tooltip::Tooltip, h_flex,
};

pub const ACTIVITY_ROW_HEIGHT: f32 = 28.0;
pub const ACTIVITY_ROW_FONT_SIZE: f32 = 13.0;
pub const ACTIVITY_ROW_LINE_HEIGHT: f32 = 16.0;
pub const ACTIVITY_DISCLOSURE_SIZE: f32 = 12.0;
const COUNT_BADGE: f32 = 15.0;
const COUNT_RING: f32 = 1.5;
const COUNT_BADGE_LEFT: f32 = 7.5;
const COUNT_BADGE_OVERHANG: f32 = COUNT_BADGE_LEFT + COUNT_BADGE - ACTIVITY_ROW_FONT_SIZE;
const COUNT_GAP: f32 = COUNT_BADGE_OVERHANG + 6.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityCount {
    pub icon: IconName,
    pub count: usize,
    pub tooltip: SharedString,
    pub failed: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ActivityMark {
    pub icon: Option<IconName>,
    pub spin: Option<f32>,
    pub turn: Option<f32>,
    pub shimmer: Option<u64>,
    pub tint: Option<Hsla>,
    pub label_color: Option<Hsla>,
    pub detail: Option<SharedString>,
    pub counts: Vec<ActivityCount>,
    pub tag: Option<(SharedString, Tone)>,
    pub spread: bool,
}

impl ActivityMark {
    #[must_use]
    pub const fn bare() -> Self {
        Self {
            icon: None,
            spin: None,
            turn: None,
            shimmer: None,
            tint: None,
            label_color: None,
            detail: None,
            counts: Vec::new(),
            tag: None,
            spread: false,
        }
    }

    #[must_use]
    pub const fn plain(icon: IconName) -> Self {
        let mut mark = Self::bare();
        mark.icon = Some(icon);
        mark
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityEnd {
    None,
    Disclosure(bool),
    Open,
}

#[must_use]
pub const fn disclosure_icon(expanded: bool) -> IconName {
    if expanded {
        IconName::ChevronUp
    } else {
        IconName::ChevronRight
    }
}

#[must_use]
pub fn activity_color(cx: &App) -> Hsla {
    cx.theme().foreground.muted()
}

#[must_use]
pub fn single_line(text: SharedString) -> SharedString {
    if !text.contains(['\n', '\r']) {
        return text;
    }
    text.split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
        .into()
}

pub fn transcript_text<E: Styled>(element: E, cx: &App) -> E {
    element.type_style(TextStyle::ReadBody, cx)
}

#[must_use]
pub fn transcript_ground(cx: &App) -> Ground {
    let theme = cx.theme();
    Ground {
        fill: theme.background,
        ..theme.ground()
    }
}

#[must_use]
pub fn activity_glyph(icon: IconName, size: f32, cx: &App) -> Div {
    div()
        .flex_none()
        .relative()
        .top(cap_center_drop(px(ACTIVITY_ROW_FONT_SIZE), px(size), cx))
        .child(Icon::new(icon).size(px(size)))
}

pub fn activity_row(
    id: impl Into<ElementId>,
    mark: ActivityMark,
    label: SharedString,
    end: ActivityEnd,
    cx: &App,
) -> Stateful<Div> {
    let drop = cap_center_drop(px(ACTIVITY_ROW_FONT_SIZE), px(ACTIVITY_ROW_FONT_SIZE), cx);
    let glyph = match (mark.spin, mark.icon) {
        (Some(phase), _) => Some(super::spinner(phase)),
        (None, Some(icon)) => Some(match mark.turn {
            Some(phase) => {
                Icon::new(icon).transform(ala::Transformation::rotate(ala::percentage(phase)))
            }
            None => Icon::new(icon),
        }),
        (None, None) => None,
    }
    .map(|glyph| {
        let glyph = glyph.size(px(ACTIVITY_ROW_FONT_SIZE));
        match mark.tint {
            Some(tint) => glyph.text_color(tint),
            None => glyph,
        }
    });
    let spread = mark.spread;
    let live = mark.shimmer.is_some();
    let end_icon = match end {
        ActivityEnd::None => None,
        ActivityEnd::Disclosure(expanded) => Some(disclosure_icon(expanded)),
        ActivityEnd::Open => Some(IconName::ExternalLink),
    };
    let clickable = end != ActivityEnd::None;
    h_flex()
        .id(id)
        .h(px(ACTIVITY_ROW_HEIGHT))
        .flex_none()
        .gap_2()
        .overflow_hidden()
        .font_family(cx.theme().font_family.clone())
        .type_style(TextStyle::Body, cx)
        .line_height(px(ACTIVITY_ROW_LINE_HEIGHT))
        .text_color(activity_color(cx))
        .when(clickable, |row| {
            let foreground = cx.theme().foreground;
            row.click_cursor()
                .hover(move |row| row.text_color(foreground))
        })
        .when_some(glyph, |this, glyph| {
            this.child(
                div()
                    .debug_selector(|| "chat-activity-glyph".to_owned())
                    .flex_none()
                    .relative()
                    .top(drop)
                    .child(glyph),
            )
        })
        .child(
            div()
                .debug_selector(|| "chat-activity-label".to_owned())
                .min_w_0()
                .flex_shrink_0()
                .when(spread, |label| label.max_w(relative(0.6)))
                .max_w_full()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when_some(mark.label_color, Styled::text_color)
                .map(|this| match mark.shimmer {
                    Some(now) => {
                        this.flex_shrink(1.0)
                            .child(super::shimmer(&single_line(label), now, cx))
                    }
                    None => this.child(single_line(label)),
                }),
        )
        .when_some(mark.detail, |this, detail| {
            this.child(
                div()
                    .debug_selector(|| "chat-activity-detail".to_owned())
                    .when(live, Styled::flex_none)
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_color(cx.theme().foreground.faint())
                    .child(detail),
            )
        })
        .when(spread, |this| this.child(div().flex_1()))
        .when(!mark.counts.is_empty(), |this| {
            this.child(
                h_flex()
                    .debug_selector(|| "chat-activity-counts".to_owned())
                    .flex_none()
                    .gap(px(COUNT_GAP))
                    .pl_1()
                    .pr(px(COUNT_BADGE_OVERHANG))
                    .children(
                        mark.counts
                            .into_iter()
                            .enumerate()
                            .map(|(slot, count)| count_glyph(slot, count, drop, cx)),
                    ),
            )
        })
        .when_some(mark.tag, |this, (tag, tone)| {
            this.child(
                div()
                    .debug_selector(|| "chat-activity-tag".to_owned())
                    .flex_none()
                    .child(Badge::new(tag).tone(tone)),
            )
        })
        .when_some(end_icon, |this, icon| {
            this.child(
                activity_glyph(icon, ACTIVITY_DISCLOSURE_SIZE, cx)
                    .debug_selector(|| "chat-activity-chevron".to_owned()),
            )
        })
}

#[allow(
    clippy::disallowed_methods,
    reason = "the ring is a cutout in the color the badge sits on that parts it from the icon it overlaps, not an edge"
)]
fn count_glyph(slot: usize, count: ActivityCount, drop: Pixels, cx: &App) -> Stateful<Div> {
    let ground = transcript_ground(cx);
    let (tone, emphasis) = if count.failed {
        (Tone::Danger, Emphasis::Solid)
    } else {
        (Tone::Neutral, Emphasis::Soft)
    };
    let tooltip = count.tooltip.clone();
    div()
        .id(("chat-activity-count", slot))
        .debug_selector(|| "chat-activity-count".to_owned())
        .flex_none()
        .relative()
        .top(drop)
        .when(count.failed, |this| this.text_color(cx.theme().danger))
        .child(Icon::new(count.icon).size(px(ACTIVITY_ROW_FONT_SIZE)))
        .child(
            div()
                .debug_selector(|| "chat-activity-count-badge".to_owned())
                .absolute()
                .top(px(-6.5))
                .left(px(COUNT_BADGE_LEFT))
                .child(
                    Badge::new(count.count.to_string())
                        .tone(tone)
                        .emphasis(emphasis)
                        .opaque()
                        .on(ground)
                        .small()
                        .min_w(px(COUNT_BADGE))
                        .h(px(COUNT_BADGE))
                        .px(px(3.0))
                        .justify_center()
                        .border(px(COUNT_RING))
                        .border_color(ground.fill)
                        .line_height(px(COUNT_BADGE - 2.0 * COUNT_RING)),
                ),
        )
        .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
}

#[cfg(test)]
mod tests {
    use super::single_line;

    #[test]
    fn single_line_collapses_embedded_breaks() {
        assert_eq!(
            single_line("sed -n '110,280p' worker.ts\nsed -n '90,230p' api.rs".into()),
            "sed -n '110,280p' worker.ts · sed -n '90,230p' api.rs"
        );
        assert_eq!(
            single_line("first\r\n\n   second   \n".into()),
            "first · second"
        );
        assert_eq!(single_line("no breaks at all".into()), "no breaks at all");
    }
}
