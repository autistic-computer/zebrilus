use zz_terminal::{Color, Glyph, PackedStyle};

#[must_use]
pub fn resolved_foreground(style: PackedStyle, minimum_contrast: f32, decorative: bool) -> Color {
    let foreground = faint_foreground(style, style.foreground());
    if decorative || minimum_contrast <= 1.0 {
        return foreground;
    }
    ensure_minimum_contrast(foreground, style.background(), minimum_contrast)
}

#[must_use]
pub fn faint_foreground(style: PackedStyle, foreground: Color) -> Color {
    if !style.faint() {
        return foreground;
    }
    let background = style.background();
    let channel = |foreground: u8, background: u8| {
        (f32::from(foreground) * 0.58 + f32::from(background) * 0.42).round() as u8
    };
    Color::rgb(
        channel(foreground.r, background.r),
        channel(foreground.g, background.g),
        channel(foreground.b, background.b),
    )
}

#[must_use]
pub fn ensure_minimum_contrast(foreground: Color, background: Color, minimum: f32) -> Color {
    if contrast_ratio(foreground, background) >= minimum {
        return foreground;
    }
    let black = Color::rgb(0, 0, 0);
    let white = Color::rgb(u8::MAX, u8::MAX, u8::MAX);
    let lighten = contrast_ratio(white, background) > contrast_ratio(black, background);
    let extreme = if lighten { white } else { black };
    if contrast_ratio(extreme, background) < minimum {
        return extreme;
    }

    let (lightness, green_red, blue_yellow) = oklab(foreground);
    let mut near = lightness;
    let mut far = if lighten { 1.0 } else { 0.0 };
    for _ in 0..16 {
        let midpoint = f32::midpoint(near, far);
        if contrast_ratio(from_oklab(midpoint, green_red, blue_yellow), background) >= minimum {
            far = midpoint;
        } else {
            near = midpoint;
        }
    }
    from_oklab(far, green_red, blue_yellow)
}

#[must_use]
pub fn contrast_ratio(left: Color, right: Color) -> f32 {
    let left = relative_luminance(left);
    let right = relative_luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn relative_luminance(color: Color) -> f32 {
    let [red, green, blue] = linear_rgb(color);
    red * 0.2126 + green * 0.7152 + blue * 0.0722
}

fn linear_rgb(color: Color) -> [f32; 3] {
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(color.r), channel(color.g), channel(color.b)]
}

#[must_use]
pub fn oklab(color: Color) -> (f32, f32, f32) {
    let [red, green, blue] = linear_rgb(color);
    let long = (0.412_221_46 * red + 0.536_332_55 * green + 0.051_445_995 * blue).cbrt();
    let medium = (0.211_903_5 * red + 0.680_699_5 * green + 0.107_396_96 * blue).cbrt();
    let short = (0.088_302_46 * red + 0.281_718_85 * green + 0.629_978_7 * blue).cbrt();
    (
        0.210_454_26 * long + 0.793_617_8 * medium - 0.004_072_047 * short,
        1.977_998_5 * long - 2.428_592_2 * medium + 0.450_593_7 * short,
        0.025_904_037 * long + 0.782_771_77 * medium - 0.808_675_77 * short,
    )
}

fn oklab_to_linear_rgb(lightness: f32, green_red: f32, blue_yellow: f32) -> [f32; 3] {
    let long = (lightness + 0.396_337_78 * green_red + 0.215_803_76 * blue_yellow).powi(3);
    let medium = (lightness - 0.105_561_346 * green_red - 0.063_854_17 * blue_yellow).powi(3);
    let short = (lightness - 0.089_484_18 * green_red - 1.291_485_5 * blue_yellow).powi(3);
    [
        4.076_741_7 * long - 3.307_711_6 * medium + 0.230_969_94 * short,
        -1.268_438 * long + 2.609_757_4 * medium - 0.341_319_38 * short,
        -0.004_196_086_3 * long - 0.703_418_6 * medium + 1.707_614_7 * short,
    ]
}

fn from_oklab(lightness: f32, green_red: f32, blue_yellow: f32) -> Color {
    let in_gamut = |rgb: [f32; 3]| rgb.iter().all(|channel| (-1e-4..=1.0001).contains(channel));
    let mut rgb = oklab_to_linear_rgb(lightness, green_red, blue_yellow);
    if !in_gamut(rgb) {
        let mut kept = 0.0_f32;
        let mut dropped = 1.0_f32;
        for _ in 0..12 {
            let chroma = f32::midpoint(kept, dropped);
            if in_gamut(oklab_to_linear_rgb(
                lightness,
                green_red * chroma,
                blue_yellow * chroma,
            )) {
                kept = chroma;
            } else {
                dropped = chroma;
            }
        }
        rgb = oklab_to_linear_rgb(lightness, green_red * kept, blue_yellow * kept);
    }
    let channel = |value: f32| {
        let value = value.clamp(0.0, 1.0);
        let encoded = if value <= 0.003_130_8 {
            value * 12.92
        } else {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        };
        (encoded * 255.0).round() as u8
    };
    Color::rgb(channel(rgb[0]), channel(rgb[1]), channel(rgb[2]))
}

#[must_use]
pub fn decorative_glyph(glyph: Glyph<'_>) -> bool {
    let character = match glyph {
        Glyph::Scalar(character) => Some(character),
        Glyph::Grapheme(text) => text.chars().next(),
        Glyph::Empty => None,
    };
    character.is_some_and(is_decorative_character)
}

#[must_use]
pub fn is_decorative_character(character: char) -> bool {
    matches!(
        character as u32,
        0x2500..=0x257f
            | 0x2580..=0x259f
            | 0x25a0..=0x25ff
            | 0xe0b0..=0xe0ca
            | 0xe0cc..=0xe0d7
            | 0x1fb00..=0x1fbff
    )
}
