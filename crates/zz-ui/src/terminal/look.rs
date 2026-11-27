use super::*;

const BLUE_HUE: f32 = 255.0;

const YELLOW_HUE: f32 = 95.0;

const MAGENTA_HUE: f32 = 330.0;

const CYAN_HUE: f32 = 200.0;

const ACCENT_BLUE_SPAN: f32 = 40.0;

const YELLOW_LIFT: f32 = 0.08;

const BRIGHT_STEP: f32 = 0.07;

pub fn theme_terminal_colors(cx: &App) -> [Color; 18] {
    let theme = cx.theme();
    let background = theme.background.opaque();
    let foreground = theme.foreground.opaque();
    let dark = theme.mode.is_dark();
    let (deep, light) = if dark {
        (background, foreground)
    } else {
        (foreground, background)
    };
    let red = to_oklch(theme.danger.opaque());
    let green = to_oklch(theme.success.opaque());
    let lightness = f32::midpoint(red.0, green.0);
    let chroma = f32::midpoint(red.1, green.1);
    let accent = to_oklch(theme.accent.opaque());
    let blue = if hue_distance(accent.2, BLUE_HUE) <= ACCENT_BLUE_SPAN {
        accent
    } else {
        (lightness, chroma, BLUE_HUE)
    };
    let yellow = (
        lightness + if dark { YELLOW_LIFT } else { 0.0 },
        chroma,
        YELLOW_HUE,
    );
    let step = if dark { BRIGHT_STEP } else { -BRIGHT_STEP };
    let mut colors = [terminal_color(foreground); 18];
    colors[1] = terminal_color(background);
    colors[2] = terminal_color(deep.mix_oklab(light, 0.88));
    colors[9] = terminal_color(light.mix_oklab(deep, 0.82));
    colors[10] = terminal_color(deep.mix_oklab(light, 0.5));
    colors[17] = terminal_color(light);
    let hues = [
        red,
        green,
        yellow,
        blue,
        (lightness, chroma, MAGENTA_HUE),
        (lightness, chroma, CYAN_HUE),
    ];
    for (at, (lightness, chroma, hue)) in hues.into_iter().enumerate() {
        colors[3 + at] = terminal_color(oklch(lightness, chroma, hue));
        colors[11 + at] = terminal_color(oklch((lightness + step).clamp(0.0, 1.0), chroma, hue));
    }
    colors
}

const RED_HUE: f32 = 29.0;

const GREEN_HUE: f32 = 142.0;

const ACCENT_SLOTS: [usize; 6] = [4, 12, 5, 13, 6, 14];

const ACCENT_CHROMA: f32 = 0.04;

const STATUS_CHROMA: f32 = 0.06;

const STATUS_HUE_SPAN: f32 = 45.0;

const DANGER_LOOKALIKE_SPAN: f32 = 20.0;

const TEXT_CONTRAST: f32 = 4.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerminalLook {
    pub mode: ThemeMode,
    pub colors: BaseColors,
    pub accent: Option<Hsla>,
}

pub fn wear_terminal_look(theme: &mut InterfaceTheme, look: TerminalLook, accent_picked: bool) {
    theme.colors.set(look.mode, look.colors);
    theme.mode = if look.mode.is_dark() {
        ModeLock::Dark
    } else {
        ModeLock::Light
    };
    if !accent_picked && let Some(accent) = look.accent {
        theme.tint = Tint {
            light: accent,
            dark: accent,
        };
    }
}

pub fn terminal_look(appearance: &TerminalAppearance) -> Option<TerminalLook> {
    let stock = TerminalAppearance::default_for(appearance.color_scheme);
    if (
        appearance.background,
        appearance.foreground,
        appearance.palette,
    ) == (stock.background, stock.foreground, stock.palette)
    {
        return None;
    }
    let background = appearance.background;
    let foreground = appearance.foreground;
    let slot = |index: usize| appearance.palette[index];
    let danger_hue = oklch_of(slot(1)).2;
    let accent = std::iter::once(appearance.cursor_color)
        .filter(|cursor| *cursor != foreground && *cursor != stock.cursor_color)
        .chain(ACCENT_SLOTS.map(slot))
        .find(|candidate| {
            let (_, chroma, hue) = oklch_of(*candidate);
            chroma >= ACCENT_CHROMA && hue_distance(hue, danger_hue) >= DANGER_LOOKALIKE_SPAN
        });
    let status = |index: usize, hue: f32| {
        let reads_as = |candidate: Color| {
            let (_, chroma, its_hue) = oklch_of(candidate);
            chroma >= STATUS_CHROMA && hue_distance(its_hue, hue) <= STATUS_HUE_SPAN
        };
        [slot(index), slot(index + 8)]
            .into_iter()
            .filter(|candidate| reads_as(*candidate))
            .find(|candidate| contrast_ratio(*candidate, background) >= TEXT_CONTRAST)
            .or_else(|| Some(slot(index)).filter(|candidate| reads_as(*candidate)))
            .map(|candidate| color(candidate, 1.0))
    };
    let mode = match appearance.background_scheme() {
        zz_terminal::TerminalColorScheme::Dark => ThemeMode::Dark,
        zz_terminal::TerminalColorScheme::Light => ThemeMode::Light,
    };
    Some(TerminalLook {
        mode,
        colors: BaseColors {
            bg: Some(color(background, 1.0)),
            fg: Some(color(foreground, 1.0)),
            success: status(2, GREEN_HUE),
            warning: status(3, YELLOW_HUE),
            danger: status(1, RED_HUE),
        },
        accent: accent.map(|accent| color(accent, 1.0)),
    })
}

fn oklch_of(value: Color) -> (f32, f32, f32) {
    let (lightness, a, b) = oklab(value);
    (
        lightness,
        a.hypot(b),
        b.atan2(a).to_degrees().rem_euclid(360.0),
    )
}

fn hue_distance(from: f32, to: f32) -> f32 {
    let turn = (from - to).rem_euclid(360.0);
    turn.min(360.0 - turn)
}

pub(super) fn terminal_color(color: Hsla) -> Color {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::rgb(channel(rgba.r), channel(rgba.g), channel(rgba.b))
}

pub fn terminal_background(background: Color, opacity: f32) -> Hsla {
    let alpha = (opacity * 255.0).round().clamp(0.0, 255.0) as u8;
    color(background, f32::from(alpha) / 255.0)
}

pub(super) fn resolved_foreground(
    style: PackedStyle,
    appearance: &TerminalAppearance,
    decorative: bool,
) -> Color {
    let foreground = faint_foreground(style, style.foreground());
    if decorative || appearance.minimum_contrast <= 1.0 {
        return foreground;
    }
    contrasted_foreground(foreground, style.background(), appearance.minimum_contrast)
}

const CONTRAST_CACHE_LIMIT: usize = 4096;

thread_local! {
    static CONTRASTED: RefCell<ala::collections::FxHashMap<(u32, u32, u32), Color>> =
        RefCell::default();
}

pub(super) fn contrasted_foreground(foreground: Color, background: Color, minimum: f32) -> Color {
    let key = (foreground.packed(), background.packed(), minimum.to_bits());
    CONTRASTED.with(|cache| {
        if let Some(color) = cache.borrow().get(&key) {
            return *color;
        }
        let color = ensure_minimum_contrast(foreground, background, minimum);
        let mut cache = cache.borrow_mut();
        if cache.len() >= CONTRAST_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, color);
        color
    })
}

pub(super) fn color(value: Color, alpha: f32) -> Hsla {
    Rgba {
        r: f32::from(value.r) / 255.0,
        g: f32::from(value.g) / 255.0,
        b: f32::from(value.b) / 255.0,
        a: alpha,
    }
    .into()
}

pub(super) fn appearance_color(value: AppearanceColor) -> Hsla {
    Rgba {
        r: f32::from(value.r) / 255.0,
        g: f32::from(value.g) / 255.0,
        b: f32::from(value.b) / 255.0,
        a: f32::from(value.a) / 255.0,
    }
    .into()
}
