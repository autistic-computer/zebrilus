use std::sync::OnceLock;

use ala::Rgba;

use super::tuning::{MaterialField, MaterialTuning, read_glass_tuning};
use crate::ThemeMode;

pub(super) fn side(mode: ThemeMode) -> usize {
    usize::from(mode.is_dark())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassLook {
    pub chrome_opacity: f32,
    pub window_opacity: f32,
    pub blur: f32,
    pub saturation: f32,
    pub contrast: f32,
    pub brightness: f32,
    pub cap: f32,
    pub wallpaper_tint: f32,
    pub content_plate: f32,
    pub floating: MaterialTuning,
    pub rim: MaterialTuning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookField {
    ChromeOpacity,
    WindowOpacity,
    Blur,
    Saturation,
    Contrast,
    Brightness,
    Cap,
    WallpaperTint,
    ContentPlate,
    Floating(MaterialField),
    Rim(MaterialField),
}

impl LookField {
    pub const SCALARS: [Self; 9] = [
        Self::ChromeOpacity,
        Self::WindowOpacity,
        Self::Blur,
        Self::Saturation,
        Self::Contrast,
        Self::Brightness,
        Self::Cap,
        Self::WallpaperTint,
        Self::ContentPlate,
    ];

    pub fn every() -> impl Iterator<Item = Self> {
        Self::SCALARS
            .into_iter()
            .chain(MaterialField::ALL.map(Self::Floating))
            .chain(MaterialField::ALL.map(Self::Rim))
    }

    pub fn key(self) -> String {
        match self {
            Self::ChromeOpacity => "chrome_opacity".to_owned(),
            Self::WindowOpacity => "window_opacity".to_owned(),
            Self::Blur => "blur".to_owned(),
            Self::Saturation => "saturation".to_owned(),
            Self::Contrast => "contrast".to_owned(),
            Self::Brightness => "brightness".to_owned(),
            Self::Cap => "cap".to_owned(),
            Self::WallpaperTint => "wallpaper_tint".to_owned(),
            Self::ContentPlate => "content_plate".to_owned(),
            Self::Floating(field) => format!("floating.{}", field.name()),
            Self::Rim(field) => format!("rim.{}", field.name()),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::every().find(|field| field.key() == key)
    }

    pub fn get(self, look: &GlassLook) -> Option<f32> {
        Some(match self {
            Self::ChromeOpacity => look.chrome_opacity,
            Self::WindowOpacity => look.window_opacity,
            Self::Blur => look.blur,
            Self::Saturation => look.saturation,
            Self::Contrast => look.contrast,
            Self::Brightness => look.brightness,
            Self::Cap => look.cap,
            Self::WallpaperTint => look.wallpaper_tint,
            Self::ContentPlate => look.content_plate,
            Self::Floating(field) => return look.floating.get(field),
            Self::Rim(field) => return look.rim.get(field),
        })
    }

    pub fn set(self, look: &mut GlassLook, value: Option<f32>) {
        let scalar = match self {
            Self::Floating(field) => return look.floating.set(field, value),
            Self::Rim(field) => return look.rim.set(field, value),
            Self::ChromeOpacity => &mut look.chrome_opacity,
            Self::WindowOpacity => &mut look.window_opacity,
            Self::Blur => &mut look.blur,
            Self::Saturation => &mut look.saturation,
            Self::Contrast => &mut look.contrast,
            Self::Brightness => &mut look.brightness,
            Self::Cap => &mut look.cap,
            Self::WallpaperTint => &mut look.wallpaper_tint,
            Self::ContentPlate => &mut look.content_plate,
        };
        if let Some(value) = value {
            *scalar = value;
        }
    }
}

const BLANK: GlassLook = GlassLook {
    chrome_opacity: 1.0,
    window_opacity: 1.0,
    blur: 0.0,
    saturation: 1.0,
    contrast: 1.0,
    brightness: 1.0,
    cap: 1.0,
    wallpaper_tint: 0.0,
    content_plate: 1.0,
    floating: MaterialTuning::NONE,
    rim: MaterialTuning::NONE,
};

const SHARED: [&str; 2] = [
    "\
floating.blur = 30
floating.refraction = 0
floating.dispersion = 0
floating.saturation = 1.3
floating.bezel = 0.5
floating.specular = 0.3
floating.glint_width = 0.5
floating.glint_spread = 1
floating.light_directionality = 0
floating.fresnel = 0
floating.edge_shadow = 0.4
floating.edge_width = 0.5
floating.edge_spread = 1
floating.glow = 0
rim.bezel = 0.5
rim.specular = 0.3
rim.glint_width = 0.5
rim.glint_spread = 1
rim.light_directionality = 0
rim.fresnel = 0
rim.edge_shadow = 0.4
rim.edge_width = 0.5
rim.edge_spread = 1
",
    "\
floating.blur = 30
floating.refraction = 0
floating.dispersion = 0
floating.saturation = 1.5
floating.bezel = 0.5
floating.specular = 0.17
floating.glint_width = 0.5
floating.glint_spread = 1
floating.light_directionality = 0
floating.fresnel = 0
floating.edge_shadow = 0.6
floating.edge_width = 0.5
floating.edge_spread = 1
floating.glow = 0
rim.bezel = 0.5
rim.specular = 0.17
rim.glint_width = 0.5
rim.glint_spread = 1
rim.light_directionality = 0
rim.fresnel = 0
rim.edge_shadow = 0.6
rim.edge_width = 0.5
rim.edge_spread = 1
",
];

const LOOK: [&str; 2] = [
    "\
chrome_opacity = 0.82
window_opacity = 0.97
blur = 40
saturation = 1.1
contrast = 0.33
brightness = 1.47
cap = 1
wallpaper_tint = 0
content_plate = 0.72
floating.tint_alpha = 0.88
",
    "\
chrome_opacity = 0.65
window_opacity = 0.97
blur = 40
saturation = 1.6
contrast = 0.75
brightness = 0.55
cap = 0.55
wallpaper_tint = 0
content_plate = 0.57
floating.tint_alpha = 0.75
floating.edge_shadow = 0
floating.noise = 0.01
floating.glow = 0.02
floating.glow_radius = 300
floating.merge = 60
rim.bezel = 30
rim.light_alpha = 1
rim.edge_shadow = 0
rim.edge_width = 0
rim.edge_spread = 0
rim.opacity = 1
",
];

fn shipped_looks() -> &'static [GlassLook; 2] {
    static LOOKED: OnceLock<[GlassLook; 2]> = OnceLock::new();
    LOOKED.get_or_init(|| {
        [ThemeMode::Light, ThemeMode::Dark].map(|mode| {
            let read = |look: GlassLook, text: &str| {
                look.read(text).unwrap_or_else(|line| {
                    panic!("the {mode:?} glass look has a bad line {line:?}")
                })
            };
            read(read(BLANK, SHARED[side(mode)]), LOOK[side(mode)])
        })
    })
}

fn shared_material(mode: ThemeMode) -> GlassLook {
    BLANK
        .read(SHARED[side(mode)])
        .expect("the shared glass lines parse")
}

impl GlassLook {
    pub fn of(mode: ThemeMode) -> Self {
        read_glass_tuning(|tuning| tuning.looks[side(mode)])
    }

    pub fn shipped(mode: ThemeMode) -> Self {
        shipped_looks()[side(mode)]
    }

    pub fn read(mut self, text: &str) -> Result<Self, String> {
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let parsed = line.split_once('=').and_then(|(key, value)| {
                let field = LookField::parse(key.trim())?;
                let value = value.trim().parse::<f32>().ok()?;
                value.is_finite().then_some((field, value))
            });
            let Some((field, value)) = parsed else {
                return Err(line.to_owned());
            };
            field.set(&mut self, Some(value));
        }
        Ok(self)
    }

    pub fn lines(&self, mode: ThemeMode) -> String {
        let shared = shared_material(mode);
        LookField::every()
            .filter_map(|field| {
                let value = field.get(self)?;
                let scalar = !matches!(field, LookField::Floating(_) | LookField::Rim(_));
                (scalar || field.get(&shared) != Some(value))
                    .then(|| format!("{} = {value}\n", field.key()))
            })
            .collect()
    }

    pub fn curve(&self) -> Option<(f32, f32)> {
        (self.contrast != 1.0 || self.brightness != 1.0).then_some((
            self.brightness * self.contrast,
            self.brightness * 0.5 * (1.0 - self.contrast),
        ))
    }

    pub fn backdrop(&self, color: Rgba) -> Rgba {
        let luma = |color: Rgba| 0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b;
        let each = |color: Rgba, change: &dyn Fn(f32) -> f32| Rgba {
            r: change(color.r).clamp(0.0, 1.0),
            g: change(color.g).clamp(0.0, 1.0),
            b: change(color.b).clamp(0.0, 1.0),
            a: color.a,
        };
        let gray = luma(color);
        let saturated = each(color, &|channel| gray + self.saturation * (channel - gray));
        let curved = match self.curve() {
            Some((scale, offset)) => each(saturated, &|channel| channel * scale + offset),
            None => saturated,
        };
        if self.cap >= 1.0 {
            return curved;
        }
        let squeeze = 1.0 / (1.0 + luma(curved) * (1.0 / self.cap.max(f32::EPSILON) - 1.0));
        each(curved, &|channel| channel * squeeze)
    }

    pub fn backdrop_range(&self) -> [f32; 2] {
        let gray = |value: f32| Rgba {
            r: value,
            g: value,
            b: value,
            a: 1.0,
        };
        [self.backdrop(gray(0.0)).r, self.backdrop(gray(1.0)).r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Emphasis, PaintState, Surface, Theme, ThemeColor, Tone,
        foundation::{
            color::contrast_ratio,
            interface_theme::{Glass, InterfaceTheme, default_theme},
            roles,
        },
    };
    use ala::Hsla;

    #[test]
    fn each_mode_holds_its_look() {
        let mut floating = [MaterialTuning::NONE; 2];
        let mut rim = [MaterialTuning::NONE; 2];
        for (side, specular, shadow, saturation) in [(0, 0.3, 0.4, 1.3), (1, 0.17, 0.6, 1.5)] {
            for (field, value) in [
                (MaterialField::Bezel, 0.5),
                (MaterialField::Specular, specular),
                (MaterialField::GlintWidth, 0.5),
                (MaterialField::GlintSpread, 1.0),
                (MaterialField::LightDirectionality, 0.0),
                (MaterialField::Fresnel, 0.0),
                (MaterialField::EdgeShadow, shadow),
                (MaterialField::EdgeWidth, 0.5),
                (MaterialField::EdgeSpread, 1.0),
            ] {
                floating[side].set(field, Some(value));
                rim[side].set(field, Some(value));
            }
            for (field, value) in [
                (MaterialField::Blur, 30.0),
                (MaterialField::Refraction, 0.0),
                (MaterialField::Dispersion, 0.0),
                (MaterialField::Saturation, saturation),
                (MaterialField::Glow, 0.0),
            ] {
                floating[side].set(field, Some(value));
            }
        }
        floating[0].set(MaterialField::TintAlpha, Some(0.88));
        for (field, value) in [
            (MaterialField::TintAlpha, 0.75),
            (MaterialField::EdgeShadow, 0.0),
            (MaterialField::Noise, 0.01),
            (MaterialField::Glow, 0.02),
            (MaterialField::GlowRadius, 300.0),
            (MaterialField::Merge, 60.0),
        ] {
            floating[1].set(field, Some(value));
        }
        for (field, value) in [
            (MaterialField::Bezel, 30.0),
            (MaterialField::LightAlpha, 1.0),
            (MaterialField::EdgeShadow, 0.0),
            (MaterialField::EdgeWidth, 0.0),
            (MaterialField::EdgeSpread, 0.0),
            (MaterialField::Opacity, 1.0),
        ] {
            rim[1].set(field, Some(value));
        }
        let (light, dark) = (ThemeMode::Light, ThemeMode::Dark);
        assert_eq!(
            [GlassLook::shipped(light), GlassLook::shipped(dark)],
            [
                GlassLook {
                    chrome_opacity: 0.82,
                    window_opacity: 0.97,
                    blur: 40.0,
                    saturation: 1.1,
                    contrast: 0.33,
                    brightness: 1.47,
                    cap: 1.0,
                    wallpaper_tint: 0.0,
                    content_plate: 0.72,
                    floating: floating[0],
                    rim: rim[0],
                },
                GlassLook {
                    chrome_opacity: 0.65,
                    window_opacity: 0.97,
                    blur: 40.0,
                    saturation: 1.6,
                    contrast: 0.75,
                    brightness: 0.55,
                    cap: 0.55,
                    wallpaper_tint: 0.0,
                    content_plate: 0.57,
                    floating: floating[1],
                    rim: rim[1],
                },
            ]
        );
        for mode in [light, dark] {
            let shipped = GlassLook::shipped(mode);
            assert_eq!(
                shared_material(mode).read(&shipped.lines(mode)),
                Ok(shipped)
            );
        }
    }

    fn lightness(color: Hsla) -> f32 {
        let y = contrast_ratio(color, crate::parse_hex("#000000").unwrap()) * 0.05 - 0.05;
        if y > 216.0 / 24389.0 {
            116.0 * y.cbrt() - 16.0
        } else {
            y * 24389.0 / 27.0
        }
    }

    fn round(value: f32) -> f32 {
        (value * 100.0).round() / 100.0
    }

    #[test]
    fn each_look_keeps_text_legible_over_black_and_white() {
        let mut drawn = Vec::new();
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let theme = InterfaceTheme {
                glass: Some(Glass::Liquid),
                ..default_theme().clone()
            };
            let mut kit = Theme::from(&*ThemeColor::for_mode(mode));
            kit.mode = mode;
            theme.apply(&mut kit);
            let look = kit.glass_look.expect("liquid glass has a look");
            assert_eq!(look, GlassLook::of(mode));
            kit.set_window(look.chrome_opacity, look.backdrop_range());
            let window = kit.ground();
            let sidebar = kit.layer(Surface::new(roles::SIDEBAR).on(window), PaintState::Rest);
            let muted = kit
                .paint_on(sidebar.ground, Tone::Neutral, Emphasis::Ghost)
                .muted_at(PaintState::Rest);
            let content = kit.content_ground().fill;
            let body = kit.foreground;
            let mut row = Vec::new();
            for behind in look.backdrop_range() {
                let behind = Hsla::from(Rgba {
                    r: behind,
                    g: behind,
                    b: behind,
                    a: 1.0,
                });
                let chrome = behind.blend(window.fill).blend(sidebar.fill);
                let content = behind.blend(content);
                row.extend([
                    round(lightness(chrome)),
                    round(contrast_ratio(body, chrome)),
                    round(contrast_ratio(chrome.blend(muted), chrome)),
                    round(lightness(content)),
                    round(contrast_ratio(body, content)),
                ]);
            }
            drawn.push((mode, row));
        }
        for (mode, row) in &drawn {
            for at in [0, 5] {
                assert!(row[at + 1] >= 4.5, "{mode:?} body on chrome {row:?}");
                assert!(row[at + 2] >= 3.0, "{mode:?} muted on chrome {row:?}");
                assert!(row[at + 4] >= 4.5, "{mode:?} body on content {row:?}");
            }
            if mode.is_dark() {
                assert!(row[5] <= 30.0, "{mode:?} chrome over white {row:?}");
                assert!(row[8] <= 22.0, "{mode:?} content over white {row:?}");
            } else {
                assert!(row[0] >= 80.0, "{mode:?} chrome over black {row:?}");
                assert!(row[3] >= 90.0, "{mode:?} content over black {row:?}");
            }
        }
        assert_eq!(
            drawn,
            [
                (
                    ThemeMode::Light,
                    vec![
                        86.05, 11.88, 6.51, 94.7, 14.94, 93.59, 14.52, 7.37, 96.87, 15.79
                    ]
                ),
                (
                    ThemeMode::Dark,
                    vec![12.23, 13.2, 8.23, 8.52, 14.3, 23.5, 9.5, 6.3, 13.85, 12.68]
                ),
            ]
        );
    }
}
