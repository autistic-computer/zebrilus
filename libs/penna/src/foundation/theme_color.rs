use ala::Hsla;

use super::{
    Colorize as _,
    color::{edge_weight, oklab, tint_offset},
};

pub const NEUTRAL_HUE: f32 = 60.0;
pub const NEUTRAL_AMOUNT: f32 = 0.15;
const TEXT_TINT: f32 = 0.35;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ThemeColor {
    pub background: Hsla,
    pub foreground: Hsla,
    pub accent: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub scrim: Hsla,
    pub shadow_color: Hsla,
    pub on_accent: Option<Hsla>,
}

impl ThemeColor {
    pub fn border(&self) -> Hsla {
        self.background
            .mix_oklab(self.foreground, 1.0 - edge_weight())
            .opaque()
    }

    #[must_use]
    pub fn neutral(&self, hue: f32, amount: f32) -> Self {
        Self {
            background: shift(self.background, hue, amount, 1.0),
            foreground: shift(self.foreground, hue, amount, TEXT_TINT),
            ..*self
        }
    }
}

fn shift(color: Hsla, hue: f32, amount: f32, weight: f32) -> Hsla {
    let (from_a, from_b) = tint_offset(NEUTRAL_HUE, NEUTRAL_AMOUNT * weight);
    let (to_a, to_b) = tint_offset(hue, amount * weight);
    if (from_a, from_b) == (to_a, to_b) {
        return color;
    }
    let (lightness, a, b) = oklab::rgb_to_oklab(color.into());
    let shifted: Hsla = oklab::oklab_to_rgb(lightness, a + to_a - from_a, b + to_b - from_b).into();
    shifted.opacity(color.a)
}
