use zz_gpui::Hsla;

use super::{
    Colorize as _,
    color::{edge_weight, tint},
};

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
    pub fn tinted(&self, hue: f32, intensity: f32) -> Self {
        Self {
            background: tint(self.background, hue, intensity),
            foreground: tint(self.foreground, hue, intensity * TEXT_TINT),
            ..*self
        }
    }
}
