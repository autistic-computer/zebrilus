use std::sync::{Arc, LazyLock};

use ala::Hsla;

use super::{ThemeColor, ThemeMode, color::parse_hex};

static LIGHT: LazyLock<Arc<ThemeColor>> = LazyLock::new(|| Arc::new(palette(ThemeMode::Light)));
static DARK: LazyLock<Arc<ThemeColor>> = LazyLock::new(|| Arc::new(palette(ThemeMode::Dark)));

impl ThemeColor {
    pub fn light() -> Arc<Self> {
        LIGHT.clone()
    }

    pub fn dark() -> Arc<Self> {
        DARK.clone()
    }

    pub fn for_mode(mode: ThemeMode) -> Arc<Self> {
        if mode.is_dark() {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

#[inline]
fn scrim(alpha: f32) -> Hsla {
    ala::hsla(0., 0., 0., alpha)
}

fn palette(mode: ThemeMode) -> ThemeColor {
    let hex = |light: &str, dark: &str| {
        parse_hex(if mode.is_dark() { dark } else { light }).unwrap_or_default()
    };
    let shade = if mode.is_dark() { 0.2 } else { 0.05 };
    ThemeColor {
        background: hex("#f6f6f3", "#1a1a19"),
        foreground: hex("#1c1c1a", "#e9e7e3"),
        accent: hex("#0a64dd", "#3183ff"),
        success: hex("#007b3e", "#3fbf74"),
        warning: hex("#a75400", "#f0a14a"),
        danger: hex("#cc2737", "#f2555f"),
        scrim: scrim(shade),
        shadow_color: scrim(shade),
        on_accent: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::Colorize as _;

    #[test]
    fn palettes_are_cached() {
        assert!(Arc::ptr_eq(&ThemeColor::light(), &ThemeColor::light()));
    }

    #[test]
    fn borders_sit_between_background_and_foreground() {
        for palette in [ThemeColor::light(), ThemeColor::dark()] {
            let lightness = super::super::color::oklab_lightness;
            let background = lightness(palette.background);
            let foreground = lightness(palette.foreground);
            let border = lightness(palette.border());
            assert!(border > background.min(foreground));
            assert!(border < background.max(foreground));
            assert_eq!(palette.border().a, 1.0);
            let translucent = ThemeColor {
                background: palette.background.opacity(0.5),
                foreground: palette.foreground.opacity(0.5),
                ..*palette
            };
            assert_eq!(translucent.border().a, 1.0);
        }
    }

    #[test]
    fn a_neutral_leans_the_text_less_than_the_background_toward_its_hue() {
        let lightness = super::super::color::oklab_lightness;
        let chroma = |color: Hsla| {
            let (_, a, b) = super::super::color::oklab::rgb_to_oklab(color.into());
            a.hypot(b)
        };
        for palette in [ThemeColor::light(), ThemeColor::dark()] {
            let grey = palette.neutral(145.0, 0.0);
            let tinted = palette.neutral(145.0, 1.0);
            for (before, after) in [
                (palette.background, tinted.background),
                (palette.foreground, tinted.foreground),
            ] {
                assert!((lightness(after) - lightness(before)).abs() < 0.01);
                let rgb = ala::Rgba::from(after);
                assert!(rgb.g > rgb.r && rgb.g > rgb.b, "{rgb:?}");
            }
            let text = chroma(tinted.foreground) - chroma(grey.foreground);
            let background = chroma(tinted.background) - chroma(grey.background);
            assert!(0.0 < text && text < background, "{text} vs {background}");
        }
    }

    #[test]
    fn panels_stay_on_the_background_side_of_the_foreground() {
        for palette in [ThemeColor::light(), ThemeColor::dark()] {
            let distance = |color: Hsla| (color.l - palette.background.l).abs();
            assert!(distance(palette.background.raised(3)) < distance(palette.foreground));
        }
    }
}
