use ala::{Hsla, Rgba, hsla};
use std::cell::Cell;

#[inline]
pub fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360., s / 100.0, l / 100.0, 1.0)
}

pub fn parse_hex(value: &str) -> Result<Hsla, String> {
    let digits = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected hexadecimal digits".to_owned());
    }
    let component = |index: usize, width: usize| -> f32 {
        let slice = &digits[index * width..(index + 1) * width];
        let byte = u8::from_str_radix(slice, 16).unwrap_or_default();
        f32::from(if width == 1 { byte * 17 } else { byte }) / 255.0
    };
    let (width, a) = match digits.len() {
        3 => (1, 1.0),
        6 => (2, 1.0),
        8 => (2, component(3, 2)),
        _ => return Err("expected #rgb, #rrggbb or #rrggbbaa".to_owned()),
    };
    Ok(Rgba {
        r: component(0, width),
        g: component(1, width),
        b: component(2, width),
        a,
    }
    .into())
}

#[must_use]
pub fn to_hex(color: Hsla) -> String {
    let rgba: Rgba = color.into();
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, b) = (byte(rgba.r), byte(rgba.g), byte(rgba.b));
    if rgba.a >= 1.0 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{:02x}", byte(rgba.a))
    }
}

#[must_use]
pub fn oklab_lightness(color: Hsla) -> f32 {
    oklab::rgb_to_oklab(color.into()).0
}

const RAISE_STEP: f32 = 0.04;
const EDGE_WEIGHT: f32 = 0.14;
const HOVER: f32 = 0.06;
const ACTIVE: f32 = 0.12;
const REST: f32 = 0.15;
const MUTED: f32 = 0.35;
const FAINT: f32 = 0.55;
const READABLE_ON_LIGHTNESS: f32 = 0.7;
const FILL_ALPHA: f32 = 0.12;
const OUTLINE_ALPHA: f32 = 0.55;
const SUBTLE_ALPHA: f32 = 0.75;
const GLOW_ALPHA: f32 = 0.35;
const WASH_ALPHA: f32 = 0.28;
const FLOATING_ALPHA: f32 = 0.90;
const TINT_CHROMA: f32 = 0.02;
const COLORED_CHROMA: f32 = 0.06;

thread_local! {
    static CONTRAST: Cell<f32> = const { Cell::new(1.0) };
}

pub fn set_contrast(value: f32) {
    CONTRAST.set(if value.is_nan() {
        1.0
    } else {
        value.clamp(1.0, 2.0)
    });
}

pub fn contrast() -> f32 {
    CONTRAST.get()
}

fn scaled(value: f32) -> f32 {
    (value * contrast()).clamp(0.0, 1.0)
}

pub(super) fn rest_weight() -> f32 {
    (REST / contrast()).clamp(0.0, 1.0)
}

pub(super) fn muted_weight() -> f32 {
    (MUTED / contrast()).clamp(0.0, 1.0)
}

pub(super) fn faint_weight() -> f32 {
    (FAINT / contrast()).clamp(0.0, 1.0)
}

pub(super) fn toward(from: Hsla, to: Hsla, amount: f32) -> Hsla {
    if amount == 0.0 {
        return from;
    }
    if amount == 1.0 {
        return to;
    }
    let (from, to) = (Rgba::from(from), Rgba::from(to));
    let a = (from.a + (to.a - from.a) * amount).clamp(0.0, 1.0);
    if a <= 0.0 {
        return Hsla::from(Rgba { a: 0.0, ..from });
    }
    let channel = |from_c: f32, to_c: f32| {
        ((from_c * from.a + (to_c * to.a - from_c * from.a) * amount) / a).clamp(0.0, 1.0)
    };
    Hsla::from(Rgba {
        r: channel(from.r, to.r),
        g: channel(from.g, to.g),
        b: channel(from.b, to.b),
        a,
    })
}

pub(super) fn readable_on(fill: Hsla) -> Hsla {
    let pole = if oklab_lightness(fill.opaque()) > READABLE_ON_LIGHTNESS {
        hsla(0., 0., 0., 1.)
    } else {
        hsla(0., 0., 1., 1.)
    };
    pole.mix_oklab(pole.invert_l(), 0.94)
}

pub(super) fn edge_weight() -> f32 {
    scaled(EDGE_WEIGHT)
}

pub(super) fn is_colored(color: Hsla) -> bool {
    let (_, a, b) = oklab::rgb_to_oklab(color.into());
    a.hypot(b) > COLORED_CHROMA
}

#[must_use]
pub fn oklch(lightness: f32, chroma: f32, hue: f32) -> Hsla {
    let (sin, cos) = hue.to_radians().sin_cos();
    oklab::oklab_to_rgb(lightness, chroma * cos, chroma * sin).into()
}

#[must_use]
pub fn to_oklch(color: Hsla) -> (f32, f32, f32) {
    let (lightness, a, b) = oklab::rgb_to_oklab(color.into());
    (
        lightness,
        a.hypot(b),
        b.atan2(a).to_degrees().rem_euclid(360.0),
    )
}

pub(super) fn tint(color: Hsla, hue: f32, intensity: f32) -> Hsla {
    let (lightness, ..) = oklab::rgb_to_oklab(color.into());
    let tinted = oklch(lightness, TINT_CHROMA, hue);
    color.mix_oklab(tinted.opacity(color.a), 1.0 - intensity.clamp(0.0, 1.0))
}

pub(super) fn raised_bright(color: Hsla, level: u8) -> Hsla {
    if color.l < 0.5 {
        return color.raised(level);
    }
    let lightness = oklab_lightness(color);
    let room = 1.0 - lightness;
    if room <= f32::EPSILON {
        return color;
    }
    let amount = ((lightness - oklab_lightness(color.raised(level))) / room).clamp(0.0, 1.0);
    Hsla {
        a: color.a,
        ..color.mix_oklab(hsla(0., 0., 1., 1.), 1.0 - amount)
    }
}

pub trait Colorize: Sized {
    fn raised(&self, level: u8) -> Self;

    fn washed(&self, level: u8) -> Self;

    fn hover(&self) -> Self;

    fn active(&self) -> Self;

    fn rest(&self) -> Self;

    fn muted(&self) -> Self;

    fn faint(&self) -> Self;

    fn on(&self) -> Self;

    fn opaque(&self) -> Self;

    fn fill(&self) -> Self;

    fn outline(&self) -> Self;

    fn subtle(&self) -> Self;

    fn glow(&self) -> Self;

    fn wash(&self) -> Self;

    fn floating(&self) -> Self;

    fn opacity(&self, opacity: f32) -> Self;

    fn divide(&self, divisor: f32) -> Self;

    fn invert(&self) -> Self;

    fn invert_l(&self) -> Self;

    fn lighten(&self, amount: f32) -> Self;

    fn darken(&self, amount: f32) -> Self;

    fn apply(&self, base_color: Self) -> Self;

    fn mix(&self, other: Self, factor: f32) -> Self;

    fn mix_oklab(&self, other: Self, factor: f32) -> Self;

    fn hue(&self, hue: f32) -> Self;

    fn saturation(&self, saturation: f32) -> Self;

    fn lightness(&self, lightness: f32) -> Self;
}

pub(super) mod oklab {
    use ala::Rgba;

    #[inline]
    fn to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    #[inline]
    fn from_linear(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    #[allow(non_snake_case)]
    pub fn rgb_to_oklab(rgb: Rgba) -> (f32, f32, f32) {
        let lr = to_linear(rgb.r);
        let lg = to_linear(rgb.g);
        let lb = to_linear(rgb.b);

        let l = 0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb;
        let m = 0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb;
        let s = 0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb;

        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();

        let L = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
        let a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
        let b = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

        (L, a, b)
    }

    #[allow(non_snake_case)]
    pub fn oklab_to_rgb(L: f32, a: f32, b: f32) -> Rgba {
        let l_ = L + 0.3963377774 * a + 0.2158037573 * b;
        let m_ = L - 0.1055613458 * a - 0.0638541728 * b;
        let s_ = L - 0.0894841775 * a - 1.2914855480 * b;

        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;

        let lr = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
        let lg = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
        let lb = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;

        Rgba {
            r: from_linear(lr).clamp(0.0, 1.0),
            g: from_linear(lg).clamp(0.0, 1.0),
            b: from_linear(lb).clamp(0.0, 1.0),
            a: 1.0,
        }
    }
}

#[inline]
fn contrast_pole(color: Hsla) -> Hsla {
    if color.l < 0.5 {
        hsla(0., 0., 1., 1.)
    } else {
        hsla(0., 0., 0., 1.)
    }
}

#[inline]
fn toward_contrast(color: Hsla, amount: f32) -> Hsla {
    let mixed = color.mix_oklab(contrast_pole(color), 1.0 - amount.clamp(0.0, 1.0));
    Hsla {
        a: color.a,
        ..mixed
    }
}

impl Colorize for Hsla {
    fn raised(&self, level: u8) -> Self {
        toward_contrast(*self, scaled(RAISE_STEP) * f32::from(level))
    }

    fn washed(&self, level: u8) -> Self {
        contrast_pole(*self).opacity(scaled(RAISE_STEP) * f32::from(level))
    }

    fn hover(&self) -> Self {
        toward_contrast(*self, scaled(HOVER))
    }

    fn active(&self) -> Self {
        toward_contrast(*self, scaled(ACTIVE))
    }

    fn rest(&self) -> Self {
        toward_contrast(*self, rest_weight())
    }

    fn muted(&self) -> Self {
        toward_contrast(*self, muted_weight())
    }

    fn faint(&self) -> Self {
        toward_contrast(*self, faint_weight())
    }

    fn on(&self) -> Self {
        toward_contrast(contrast_pole(*self), 0.06)
    }

    fn opaque(&self) -> Self {
        Self { a: 1.0, ..*self }
    }

    fn fill(&self) -> Self {
        self.opacity(scaled(FILL_ALPHA))
    }

    fn outline(&self) -> Self {
        self.opacity(OUTLINE_ALPHA)
    }

    fn subtle(&self) -> Self {
        self.opacity(SUBTLE_ALPHA)
    }

    fn glow(&self) -> Self {
        self.opacity(scaled(GLOW_ALPHA))
    }

    fn wash(&self) -> Self {
        self.opacity(scaled(WASH_ALPHA))
    }

    fn floating(&self) -> Self {
        self.opacity(FLOATING_ALPHA)
    }

    fn opacity(&self, factor: f32) -> Self {
        Self {
            a: self.a * factor.clamp(0.0, 1.0),
            ..*self
        }
    }

    fn divide(&self, divisor: f32) -> Self {
        Self {
            a: divisor,
            ..*self
        }
    }

    fn invert(&self) -> Self {
        Self {
            h: 1.0 - self.h,
            s: 1.0 - self.s,
            l: 1.0 - self.l,
            a: self.a,
        }
    }

    fn invert_l(&self) -> Self {
        Self {
            l: 1.0 - self.l,
            ..*self
        }
    }

    fn lighten(&self, factor: f32) -> Self {
        let l = self.l * (1.0 + factor.clamp(0.0, 1.0));

        Hsla { l, ..*self }
    }

    fn darken(&self, factor: f32) -> Self {
        let l = self.l * (1.0 - factor.clamp(0.0, 1.0));

        Self { l, ..*self }
    }

    fn apply(&self, new_color: Self) -> Self {
        Hsla {
            h: new_color.h,
            s: new_color.s,
            l: self.l,
            a: self.a,
        }
    }

    fn mix(&self, other: Self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        let inv = 1.0 - factor;

        #[inline]
        fn lerp_hue(a: f32, b: f32, t: f32) -> f32 {
            let diff = (b - a + 180.0).rem_euclid(360.) - 180.;
            (a + diff * t).rem_euclid(360.0)
        }

        Hsla {
            h: lerp_hue(self.h * 360., other.h * 360., factor) / 360.,
            s: self.s * factor + other.s * inv,
            l: self.l * factor + other.l * inv,
            a: self.a * factor + other.a * inv,
        }
    }

    #[allow(non_snake_case)]
    fn mix_oklab(&self, other: Self, factor: f32) -> Self {
        let factor = factor.clamp(0.0, 1.0);
        let inv = 1.0 - factor;

        let result_alpha = self.a * factor + other.a * inv;

        if result_alpha == 0.0 {
            return Self {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 0.0,
            };
        }

        let rgb1 = self.to_rgb();
        let rgb2 = other.to_rgb();

        let (l1, a1, b1) = oklab::rgb_to_oklab(rgb1);
        let (l2, a2, b2) = oklab::rgb_to_oklab(rgb2);

        let alpha1 = self.a;
        let alpha2 = other.a;

        let l1_pm = l1 * alpha1;
        let a1_pm = a1 * alpha1;
        let b1_pm = b1 * alpha1;

        let l2_pm = l2 * alpha2;
        let a2_pm = a2 * alpha2;
        let b2_pm = b2 * alpha2;

        let L_pm = l1_pm * factor + l2_pm * inv;
        let a_pm = a1_pm * factor + a2_pm * inv;
        let b_pm = b1_pm * factor + b2_pm * inv;

        let L = L_pm / result_alpha;
        let a = a_pm / result_alpha;
        let b = b_pm / result_alpha;

        let mut rgb = oklab::oklab_to_rgb(L, a, b);
        rgb.a = result_alpha;

        rgb.into()
    }

    fn hue(&self, hue: f32) -> Self {
        let mut color = *self;
        color.h = hue.clamp(0., 1.);
        color
    }

    fn saturation(&self, saturation: f32) -> Self {
        let mut color = *self;
        color.s = saturation.clamp(0., 1.);
        color
    }

    fn lightness(&self, lightness: f32) -> Self {
        let mut color = *self;
        color.l = lightness.clamp(0., 1.);
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn contrast_scales_surfaces_edges_and_muted_text_in_both_modes() {
        for palette in [crate::ThemeColor::light(), crate::ThemeColor::dark()] {
            set_contrast(1.0);
            let distance = |first, second| (oklab_lightness(first) - oklab_lightness(second)).abs();
            let raised = distance(palette.background.raised(1), palette.background);
            let hovered = distance(palette.background.hover(), palette.background);
            let active = distance(palette.background.active(), palette.background);
            let muted = distance(palette.foreground.muted(), palette.foreground);
            let edge = distance(palette.border(), palette.background);
            for value in [1.5, 2.0] {
                set_contrast(value);
                assert!(distance(palette.background.raised(1), palette.background) > raised);
                assert!(distance(palette.background.hover(), palette.background) > hovered);
                assert!(distance(palette.background.active(), palette.background) > active);
                assert!(distance(palette.foreground.muted(), palette.foreground) < muted);
                assert!(distance(palette.border(), palette.background) > edge);
                assert_close(palette.background.washed(2).a, 0.08 * value);
                assert_close(palette.foreground.fill().a, 0.12 * value);
                assert_close(palette.foreground.glow().a, 0.35 * value);
                assert_close(palette.foreground.wash().a, 0.28 * value);
                assert_close(palette.border().a, 1.0);
            }
        }
        set_contrast(1.0);
    }

    #[test]
    fn contrast_leaves_fixed_derivations_and_explicit_color_math_unchanged() {
        let color = hsl(210.0, 50.0, 40.0).opacity(0.6);
        let other = hsl(30.0, 60.0, 70.0);
        let samples = || {
            [
                color.on(),
                color.outline(),
                color.subtle(),
                color.floating(),
                color.opacity(0.1),
                color.divide(0.4),
                color.mix(other, 0.3),
                color.mix_oklab(other, 0.3),
                color.lighten(0.2),
                color.darken(0.2),
                color.hue(0.3),
                color.saturation(0.3),
                color.lightness(0.3),
            ]
        };
        set_contrast(1.0);
        let baseline = samples();
        for value in [1.5, 2.0] {
            set_contrast(value);
            assert_eq!(samples(), baseline);
            assert_close(color.washed(u8::MAX).a, 1.0);
        }
        set_contrast(1.0);
    }

    #[test]
    fn theme_contrast_clamps_and_keeps_the_stored_value_in_sync() {
        let mut theme = crate::Theme::default();
        for (value, expected) in [(-1.0, 1.0), (0.5, 1.0), (4.0, 2.0), (f32::NAN, 1.0)] {
            theme.set_contrast(value);
            assert_close(theme.contrast(), expected);
            assert_close(contrast(), expected);
        }
        theme.set_contrast(1.0);
    }

    #[test]
    fn hex_round_trips_through_every_accepted_shape() {
        assert_eq!(to_hex(parse_hex("#1a1b26").unwrap()), "#1a1b26");
        assert_eq!(to_hex(parse_hex("1A1B26").unwrap()), "#1a1b26");
        assert_eq!(to_hex(parse_hex("#f00").unwrap()), "#ff0000");
        assert_eq!(to_hex(parse_hex("#ff000080").unwrap()), "#ff000080");
    }

    #[test]
    fn hex_rejects_what_a_user_can_mistype() {
        for value in ["", "#", "#12", "#12345", "#gggggg", "#1234567", "rebecca"] {
            assert!(parse_hex(value).is_err(), "{value:?} should be rejected");
        }
    }

    #[test]
    fn hsl_maps_css_units_onto_ala_units() {
        let color = hsl(180., 50., 25.);
        assert_close(color.h, 0.5);
        assert_close(color.s, 0.5);
        assert_close(color.l, 0.25);
        assert_close(color.a, 1.0);
    }

    #[test]
    fn opacity_is_relative_and_clamped() {
        let half = hsl(0., 0., 0.).opacity(0.5);
        assert_close(half.a, 0.5);
        assert_close(half.opacity(0.5).a, 0.25);
        assert_close(half.opacity(4.0).a, 0.5);
    }

    #[test]
    fn lighten_and_darken_scale_lightness() {
        let base = hsl(0., 0., 50.);
        assert_close(base.lighten(0.5).l, 0.75);
        assert_close(base.darken(0.5).l, 0.25);
    }

    #[test]
    fn mix_oklab_with_transparent_keeps_hue_and_scales_alpha() {
        let red = hsl(0., 100., 50.);
        let mixed = red.mix_oklab(ala::transparent_black(), 0.2);
        assert_close(mixed.a, 0.2);
        assert_close(mixed.h, red.h);
    }

    #[test]
    fn mix_oklab_of_two_transparents_is_transparent() {
        let mixed = ala::transparent_black().mix_oklab(ala::transparent_black(), 0.5);
        assert_close(mixed.a, 0.0);
    }

    #[test]
    fn raising_moves_away_from_the_color_itself_in_either_mode() {
        let light_plane = hsl(0., 0., 100.);
        let dark_plane = hsl(0., 0., 3.9);

        assert!(
            light_plane.raised(1).l < light_plane.l,
            "light plane darkens"
        );
        assert!(dark_plane.raised(1).l > dark_plane.l, "dark plane lightens");
    }

    #[test]
    fn elevation_levels_are_monotonic() {
        let plane = hsl(0., 0., 3.9);
        assert!(plane.raised(1).l < plane.raised(2).l);
        assert!(plane.raised(2).l < plane.raised(3).l);
    }

    #[test]
    fn washed_is_the_pole_at_step_alpha() {
        let dark = hsl(0., 0., 3.9);
        let wash = dark.washed(2);
        assert_close(wash.l, 1.0);
        assert_close(wash.a, 2.0 * RAISE_STEP);
        assert_close(hsl(0., 0., 98.).washed(2).l, 0.0);
    }

    #[test]
    fn raising_preserves_alpha() {
        let translucent = hsl(0., 0., 3.9).opacity(0.6);
        assert_close(translucent.raised(2).a, 0.6);
    }

    #[test]
    fn hover_sits_between_rest_and_active() {
        let plane = hsl(0., 0., 100.);
        assert!(plane.hover().l < plane.l);
        assert!(plane.active().l < plane.hover().l);
    }

    #[test]
    fn muted_text_stays_legible_in_both_modes() {
        for (background, foreground) in [
            (hsl(0., 0., 100.), hsl(0., 0., 3.9)),
            (hsl(0., 0., 3.9), hsl(0., 0., 98.)),
        ] {
            let muted = foreground.muted();
            let span = foreground.l - background.l;
            let travelled = (muted.l - background.l) / span;

            assert!(
                (0.5..1.0).contains(&travelled),
                "muted at {travelled} of the way from background to foreground"
            );
        }
    }

    #[test]
    fn on_returns_a_legible_counterpart() {
        assert!(hsl(0., 0., 100.).on().l < 0.2, "dark text on a light color");
        assert!(hsl(0., 0., 3.9).on().l > 0.8, "light text on a dark color");
    }
}
