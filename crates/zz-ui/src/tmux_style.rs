use std::ops::Range;

use ala::{
    App, FontStyle, FontWeight, HighlightStyle, Hsla, Rgba, SharedString, StrikethroughStyle,
    StyledText, UnderlineStyle, px,
};
pub use zz_client::tmux_style::split_tmux_alignment;
use zz_client::tmux_style::style_field;
use zz_protocol::{
    StyledSegment, TmuxAttributeState, TmuxColour, TmuxStyle, indexed_colour_rgb, parse_tmux_colour,
};

use crate::{ActiveTheme as _, Colorize as _, Elevation};

#[must_use]
pub fn tmux_style_colour(style: &str, key: &str, fallback: Hsla, cx: &App) -> Hsla {
    style_field(style, key)
        .and_then(parse_tmux_colour)
        .and_then(|colour| resolve_tmux_colour(colour, cx))
        .unwrap_or(fallback)
}

#[must_use]
pub fn tmux_literal_colour(style: &str, key: &str, _cx: &App) -> Option<Hsla> {
    zz_client::tmux_style::literal_style_colour(style, key).map(packed_tmux_colour)
}

#[derive(Clone, Debug, PartialEq)]
pub struct TmuxStyledText {
    text: SharedString,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
}

impl TmuxStyledText {
    #[must_use]
    pub fn into_styled_text(self) -> StyledText {
        StyledText::new(self.text).with_highlights(self.highlights)
    }
}

#[must_use]
pub fn tmux_styled_segments_text(
    segments: &[StyledSegment],
    base_foreground: Hsla,
    base_background: Hsla,
    cx: &App,
) -> TmuxStyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for segment in segments {
        let start = text.len();
        text.push_str(&segment.text);
        let highlight = tmux_highlight_style(&segment.style, base_foreground, base_background, cx);
        if highlight != HighlightStyle::default() {
            highlights.push((start..text.len(), highlight));
        }
    }
    TmuxStyledText {
        text: text.into(),
        highlights,
    }
}

fn tmux_highlight_style(
    style: &TmuxStyle,
    base_foreground: Hsla,
    base_background: Hsla,
    cx: &App,
) -> HighlightStyle {
    let mut color = style.fg.and_then(|colour| resolve_tmux_colour(colour, cx));
    let mut background_color = style.bg.and_then(|colour| resolve_tmux_colour(colour, cx));
    if style.attributes.reverse == TmuxAttributeState::On {
        let resolved_foreground = color.unwrap_or(base_foreground);
        let resolved_background = background_color.unwrap_or(base_background);
        color = Some(resolved_background);
        background_color = Some(resolved_foreground);
    }

    let underline = [
        style.attributes.underscore,
        style.attributes.double_underscore,
        style.attributes.curly_underscore,
        style.attributes.dotted_underscore,
        style.attributes.dashed_underscore,
    ]
    .into_iter()
    .any(|state| state == TmuxAttributeState::On)
    .then(|| UnderlineStyle {
        thickness: px(1.0),
        color: style.us.and_then(|colour| resolve_tmux_colour(colour, cx)),
        wavy: false,
    });

    HighlightStyle {
        color,
        font_weight: match style.attributes.bold {
            TmuxAttributeState::On => Some(FontWeight::BOLD),
            TmuxAttributeState::Off => Some(FontWeight::NORMAL),
            TmuxAttributeState::Unset => None,
        },
        font_style: match style.attributes.italics {
            TmuxAttributeState::On => Some(FontStyle::Italic),
            TmuxAttributeState::Off => Some(FontStyle::Normal),
            TmuxAttributeState::Unset => None,
        },
        background_color,
        underline,
        strikethrough: (style.attributes.strikethrough == TmuxAttributeState::On).then(|| {
            StrikethroughStyle {
                thickness: px(1.0),
                color: None,
            }
        }),
        fade_out: style.dim_percentage.map_or_else(
            || (style.attributes.dim == TmuxAttributeState::On).then_some(0.5),
            |percentage| Some(f32::from(percentage) / 100.0),
        ),
    }
}

pub fn resolve_tmux_colour(colour: TmuxColour, cx: &App) -> Option<Hsla> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => {
            Some(packed_tmux_colour(indexed_colour_rgb(index)))
        }
        TmuxColour::Rgb(colour) => Some(packed_tmux_colour(colour)),
        TmuxColour::Default | TmuxColour::Terminal => None,
        TmuxColour::Theme(index) => Some(match index {
            0 => cx.theme().background,
            1 | 7..=9 => cx.theme().foreground,
            2 => cx.theme().border(),
            3 => cx.theme().surface(Elevation::Panel).fill.opaque(),
            4 => cx.theme().success,
            5 => cx.theme().warning,
            6 => cx.theme().danger,
            _ => return None,
        }),
    }
}

fn packed_tmux_colour(colour: u32) -> Hsla {
    let channel = |shift: u32| {
        f32::from(u8::try_from((colour >> shift) & 0xff_u32).unwrap_or_default()) / 255.0
    };
    Rgba {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: 1.0,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use ala::TestAppContext;

    use super::*;

    #[ala::test]
    fn the_last_colour_field_wins_and_theme_slots_are_not_literal(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            let appended = "bold,bg=colour17,fg=default,bg=colour52";
            let colour52 = packed_tmux_colour(indexed_colour_rgb(52));
            assert_eq!(
                tmux_style_colour(appended, "bg", cx.theme().background, cx),
                colour52
            );
            assert_eq!(tmux_literal_colour(appended, "bg", cx), Some(colour52));
            assert_eq!(tmux_literal_colour(appended, "fg", cx), None);
            assert_eq!(
                tmux_literal_colour("bg=themedarkgrey bg=themeyellow", "bg", cx),
                None
            );
            assert_eq!(
                tmux_literal_colour("bg=themeyellow,bg=#102030", "bg", cx),
                Some(packed_tmux_colour(0x0010_2030))
            );
        });
    }

    #[test]
    fn indicator_labels_split_into_alignment_buckets() {
        let [left, centre, right] =
            split_tmux_alignment("L#[align=centre]C#[align=right]#[fg=red]80x24");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "L");
        assert_eq!(centre.len(), 1);
        assert_eq!(centre[0].text, "C");
        assert_eq!(right.len(), 1);
        assert_eq!(right[0].text, "80x24");
        assert_eq!(
            right[0].style.fg,
            Some(TmuxColour::Basic(1)),
            "styled segments keep their parsed colours"
        );
        let [left, centre, right] = split_tmux_alignment("#[align=right]80x24");
        assert!(left.is_empty() && centre.is_empty());
        assert_eq!(right[0].text, "80x24");
    }
}
