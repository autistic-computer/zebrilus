//! Appearance settings every client shares: the light or dark mode, the app
//! icon, the built-in interface themes by id, and hex color parsing.

/// The interface themes compiled into every client, by the id
/// `interface-theme` names. Each one bundles a look with its light and dark
/// colors.
pub const BUILTIN_THEMES: [&str; 3] = ["default", "macos", "linear"];

/// The theme a fresh install uses.
pub const DEFAULT_THEME: &str = "default";

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
