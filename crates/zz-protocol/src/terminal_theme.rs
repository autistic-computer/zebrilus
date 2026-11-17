use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use zz_terminal::{
    AppearanceColor, Color, LocalColors, TerminalAppearance, TerminalColorScheme, TerminalPalette,
};

pub const MAX_TERMINAL_THEMES: usize = 4096;
pub const MAX_TERMINAL_THEME_NAME_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalTheme {
    #[serde(deserialize_with = "deserialize_theme_name")]
    pub name: String,
    pub scheme: TerminalColorScheme,
    pub foreground: Color,
    pub background: Color,
    pub cursor: Color,
    pub selection_foreground: Color,
    pub selection_background: AppearanceColor,
    pub palette: [Color; 16],
}

impl TerminalTheme {
    #[must_use]
    pub fn new(name: String, appearance: &TerminalAppearance) -> Self {
        let mut palette = [Color::default(); 16];
        palette.copy_from_slice(&appearance.palette.as_array()[..16]);
        Self {
            name,
            scheme: appearance.background_scheme(),
            foreground: appearance.foreground,
            background: appearance.background,
            cursor: appearance.cursor_color,
            selection_foreground: appearance.selection_foreground,
            selection_background: appearance.selection_background,
            palette,
        }
    }

    pub fn recolor(&self, appearance: &mut TerminalAppearance) {
        let mut local = [self.foreground; 18];
        local[1] = self.background;
        local[2..].copy_from_slice(&self.palette);
        appearance.local_colors = Some(LocalColors::new(
            appearance.foreground,
            appearance.background,
            &appearance.palette,
            local,
        ));
        appearance.foreground = self.foreground;
        appearance.background = self.background;
        appearance.cursor_color = self.cursor;
        appearance.selection_foreground = self.selection_foreground;
        appearance.selection_background = self.selection_background;
        let mut palette = appearance.palette.into_array();
        palette[..16].copy_from_slice(&self.palette);
        appearance.palette = TerminalPalette::new(palette);
    }
}

fn deserialize_theme_name<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let name = String::deserialize(deserializer)?;
    if name.len() > MAX_TERMINAL_THEME_NAME_BYTES {
        return Err(D::Error::invalid_length(
            name.len(),
            &"a terminal theme name within the wire byte limit",
        ));
    }
    Ok(name)
}

pub(crate) fn deserialize_terminal_themes<'de, D>(
    deserializer: D,
) -> Result<Vec<TerminalTheme>, D::Error>
where
    D: Deserializer<'de>,
{
    let themes = Vec::<TerminalTheme>::deserialize(deserializer)?;
    if themes.len() > MAX_TERMINAL_THEMES {
        return Err(D::Error::invalid_length(
            themes.len(),
            &"a terminal theme list within the wire entry limit",
        ));
    }
    Ok(themes)
}

#[cfg(test)]
mod tests {
    use zz_terminal::{ColourClass, PackedStyle, UnderlineStyle};

    use super::*;
    use crate::{ProtocolMessage, decode_protocol_frame, encode_protocol_message};

    #[test]
    fn a_theme_recolors_host_defaults_and_base_palette() {
        let host = TerminalAppearance::default();
        let theme = TerminalTheme::new(
            "Paper".to_owned(),
            &TerminalAppearance::default_for(TerminalColorScheme::Light),
        );
        assert_eq!(theme.scheme, TerminalColorScheme::Light);
        let mut shown = host.clone();
        theme.recolor(&mut shown);
        assert_eq!(
            (shown.foreground, shown.background, shown.cursor_color),
            (theme.foreground, theme.background, theme.cursor)
        );
        assert_eq!(shown.palette.as_array()[..16], theme.palette);
        assert_eq!(
            shown.palette.as_array()[16..],
            host.palette.as_array()[16..]
        );
        let local = shown.local_colors.expect("theme sets local colors");
        let style = |foreground, background| {
            PackedStyle::new(foreground, background, None, 0, UnderlineStyle::None)
        };
        let red = local.recolor(
            style(host.palette[1], host.background)
                .with_classes(ColourClass::Palette(1), ColourClass::Default),
        );
        assert_eq!(
            (red.foreground(), red.background()),
            (theme.palette[1], theme.background)
        );
        let picked = style(Color::rgb(255, 0, 128), host.background)
            .with_classes(ColourClass::Rgb, ColourClass::Resolved);
        assert_eq!(local.recolor(picked).foreground(), Color::rgb(255, 0, 128));
        assert_eq!(local.recolor(picked).background(), theme.background);
    }

    #[test]
    fn theme_lists_round_trip_and_long_names_are_refused() {
        let theme = TerminalTheme::new("Paper".to_owned(), &TerminalAppearance::default());
        let message = ProtocolMessage::TerminalThemesResponse {
            request_id: 7,
            themes: vec![theme.clone()],
        };
        let frame = encode_protocol_message(&message).expect("encode");
        assert_eq!(decode_protocol_frame(&frame).expect("decode"), message);
        let long = ProtocolMessage::TerminalThemesResponse {
            request_id: 7,
            themes: vec![TerminalTheme {
                name: "x".repeat(MAX_TERMINAL_THEME_NAME_BYTES + 1),
                ..theme
            }],
        };
        let frame = encode_protocol_message(&long).expect("encode");
        assert!(decode_protocol_frame(&frame).is_err());
    }
}
