//! Appearance settings every client shares: the light or dark mode, the app
//! icon, the built-in interface themes by id, and hex color parsing.

/// The interface themes compiled into every client, by the id
/// `interface-theme` names. Each one bundles a look with its light and dark
/// colors.
pub const BUILTIN_THEMES: [&str; 3] = ["default", "macos", "yaru"];

/// The theme a fresh install uses.
pub const DEFAULT_THEME: &str = "default";

/// How fast interface motion runs, as a share of its designed speed:
/// `animation-speed` takes 0.25 (four times slower) to 2 (twice as fast).
pub const DEFAULT_ANIMATION_SPEED: f32 = 1.0;
pub const MIN_ANIMATION_SPEED: f32 = 0.25;
pub const MAX_ANIMATION_SPEED: f32 = 2.0;

/// The roundest corner radius `widget-corner-radius` takes.
pub const MAX_CORNER_RADIUS: f32 = 25.0;

/// The named accents `chrome-accent` takes besides a `#rrggbb` color, each
/// tuned for light and for dark.
pub const ACCENTS: [&str; 8] = [
    "blue", "purple", "pink", "red", "orange", "yellow", "green", "graphite",
];

/// The hue `chrome-tint` leans toward until `chrome-tint-hue` picks one, in
/// Oklch degrees: a cool blue.
pub const DEFAULT_TINT_HUE: f32 = 260.0;

/// What `chrome-accent` sets: the theme's own accent, one of [`ACCENTS`], or
/// a color for both modes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum AccentSetting {
    #[default]
    Theme,
    Named(&'static str),
    Color([f32; 4]),
}

impl AccentSetting {
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value == "theme" {
            return Some(Self::Theme);
        }
        match ACCENTS.iter().find(|name| **name == value) {
            Some(name) => Some(Self::Named(name)),
            None => parse_hex(value).ok().map(Self::Color),
        }
    }

    /// How `zz/config` spells it.
    pub fn spelled(self) -> String {
        match self {
            Self::Theme => "theme".to_owned(),
            Self::Named(name) => name.to_owned(),
            Self::Color([r, g, b, _]) => {
                let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
                format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
            }
        }
    }
}

pub fn is_builtin_theme(id: &str) -> bool {
    BUILTIN_THEMES.contains(&id.trim())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemeModeSetting {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeModeSetting {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.as_str() == value.trim())
    }

    pub const fn pinned(self) -> Option<bool> {
        match self {
            Self::System => None,
            Self::Light => Some(false),
            Self::Dark => Some(true),
        }
    }
}

/// What `app-icon` selects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppIconSetting {
    /// Follow the OS appearance.
    #[default]
    Automatic,
    Light,
    Dark,
}

impl AppIconSetting {
    pub const ALL: [Self; 3] = [Self::Automatic, Self::Light, Self::Dark];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|setting| setting.as_str() == value.trim())
    }
}

pub fn parse_hex(value: &str) -> Result<[f32; 4], String> {
    let digits = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected hexadecimal digits".to_owned());
    }
    // `#rgb` is the CSS shorthand: each digit doubles into a byte.
    let component = |index: usize, width: usize| -> f32 {
        let slice = &digits[index * width..(index + 1) * width];
        let byte = u8::from_str_radix(slice, 16).unwrap_or_default();
        f32::from(if width == 1 { byte * 17 } else { byte }) / 255.0
    };
    let (width, alpha) = match digits.len() {
        3 => (1, 1.0),
        6 => (2, 1.0),
        8 => (2, component(3, 2)),
        _ => return Err("expected #rgb, #rrggbb or #rrggbbaa".to_owned()),
    };
    Ok([
        component(0, width),
        component(1, width),
        component(2, width),
        alpha,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_parse_names_and_colors_and_spell_them_back() {
        assert_eq!(AccentSetting::parse(" theme "), Some(AccentSetting::Theme));
        assert_eq!(
            AccentSetting::parse("green"),
            Some(AccentSetting::Named("green"))
        );
        let color = AccentSetting::parse("#FF8800").unwrap();
        assert_eq!(color.spelled(), "#ff8800");
        assert_eq!(AccentSetting::parse("chartreuse"), None);
    }
}
