use std::cell::RefCell;

use ala::{GlassMaterial, px};

use super::{color, glass::GlassLook, interface_theme};
use crate::ThemeMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialField {
    Blur,
    Bezel,
    Refraction,
    Dispersion,
    TintAlpha,
    TintLightness,
    Saturation,
    Brightness,
    Contrast,
    Specular,
    GlintWidth,
    GlintSpread,
    LightAngle,
    LightDirectionality,
    InactiveFlattening,
    LightAlpha,
    Fresnel,
    EdgeShadow,
    EdgeWidth,
    EdgeAlpha,
    EdgeSpread,
    Noise,
    Glow,
    GlowRadius,
    GlowX,
    GlowY,
    Merge,
    Opacity,
}

impl MaterialField {
    pub const ALL: [Self; 28] = [
        Self::Blur,
        Self::Bezel,
        Self::Refraction,
        Self::Dispersion,
        Self::TintAlpha,
        Self::TintLightness,
        Self::Saturation,
        Self::Brightness,
        Self::Contrast,
        Self::Specular,
        Self::GlintWidth,
        Self::GlintSpread,
        Self::LightAngle,
        Self::LightDirectionality,
        Self::InactiveFlattening,
        Self::LightAlpha,
        Self::Fresnel,
        Self::EdgeShadow,
        Self::EdgeWidth,
        Self::EdgeAlpha,
        Self::EdgeSpread,
        Self::Noise,
        Self::Glow,
        Self::GlowRadius,
        Self::GlowX,
        Self::GlowY,
        Self::Merge,
        Self::Opacity,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Blur => "blur",
            Self::Bezel => "bezel",
            Self::Refraction => "refraction",
            Self::Dispersion => "dispersion",
            Self::TintAlpha => "tint_alpha",
            Self::TintLightness => "tint_lightness",
            Self::Saturation => "saturation",
            Self::Brightness => "brightness",
            Self::Contrast => "contrast",
            Self::Specular => "specular",
            Self::GlintWidth => "glint_width",
            Self::GlintSpread => "glint_spread",
            Self::LightAngle => "light_angle",
            Self::LightDirectionality => "light_directionality",
            Self::InactiveFlattening => "inactive_flattening",
            Self::LightAlpha => "light_alpha",
            Self::Fresnel => "fresnel",
            Self::EdgeShadow => "edge_shadow",
            Self::EdgeWidth => "edge_width",
            Self::EdgeAlpha => "edge_alpha",
            Self::EdgeSpread => "edge_spread",
            Self::Noise => "noise",
            Self::Glow => "glow",
            Self::GlowRadius => "glow_radius",
            Self::GlowX => "glow_x",
            Self::GlowY => "glow_y",
            Self::Merge => "merge",
            Self::Opacity => "opacity",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }

    pub fn get(self, material: &GlassMaterial) -> f32 {
        match self {
            Self::Blur => material.blur.as_f32(),
            Self::Bezel => material.bezel.as_f32(),
            Self::Refraction => material.refraction.as_f32(),
            Self::Dispersion => material.dispersion,
            Self::TintAlpha => material.tint.a,
            Self::TintLightness => material.tint.l,
            Self::Saturation => material.saturation,
            Self::Brightness => material.brightness,
            Self::Contrast => material.contrast,
            Self::Specular => material.specular,
            Self::GlintWidth => material.glint_width.as_f32(),
            Self::GlintSpread => material.glint_spread,
            Self::LightAngle => material.light_angle,
            Self::LightDirectionality => material.light_directionality,
            Self::InactiveFlattening => material.inactive_flattening,
            Self::LightAlpha => material.light_color.a,
            Self::Fresnel => material.fresnel,
            Self::EdgeShadow => material.edge_shadow,
            Self::EdgeWidth => material.edge_width.as_f32(),
            Self::EdgeAlpha => material.edge_color.a,
            Self::EdgeSpread => material.edge_spread,
            Self::Noise => material.noise,
            Self::Glow => material.glow,
            Self::GlowRadius => material.glow_radius.as_f32(),
            Self::GlowX => material.glow_center.x,
            Self::GlowY => material.glow_center.y,
            Self::Merge => material.merge.as_f32(),
            Self::Opacity => material.opacity,
        }
    }

    pub fn set(self, material: &mut GlassMaterial, value: f32) {
        match self {
            Self::Blur => material.blur = px(value),
            Self::Bezel => material.bezel = px(value),
            Self::Refraction => material.refraction = px(value),
            Self::Dispersion => material.dispersion = value,
            Self::TintAlpha => material.tint.a = value,
            Self::TintLightness => material.tint.l = value,
            Self::Saturation => material.saturation = value,
            Self::Brightness => material.brightness = value,
            Self::Contrast => material.contrast = value,
            Self::Specular => material.specular = value,
            Self::GlintWidth => material.glint_width = px(value),
            Self::GlintSpread => material.glint_spread = value,
            Self::LightAngle => material.light_angle = value,
            Self::LightDirectionality => material.light_directionality = value,
            Self::InactiveFlattening => material.inactive_flattening = value,
            Self::LightAlpha => material.light_color.a = value,
            Self::Fresnel => material.fresnel = value,
            Self::EdgeShadow => material.edge_shadow = value,
            Self::EdgeWidth => material.edge_width = px(value),
            Self::EdgeAlpha => material.edge_color.a = value,
            Self::EdgeSpread => material.edge_spread = value,
            Self::Noise => material.noise = value,
            Self::Glow => material.glow = value,
            Self::GlowRadius => material.glow_radius = px(value),
            Self::GlowX => material.glow_center.x = value,
            Self::GlowY => material.glow_center.y = value,
            Self::Merge => material.merge = px(value),
            Self::Opacity => material.opacity = value,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialTuning([Option<f32>; MaterialField::ALL.len()]);

impl MaterialTuning {
    pub const NONE: Self = Self([None; MaterialField::ALL.len()]);

    pub fn get(&self, field: MaterialField) -> Option<f32> {
        self.0[field.index()]
    }

    pub fn set(&mut self, field: MaterialField, value: Option<f32>) {
        self.0[field.index()] = value;
    }

    pub fn apply(&self, mut material: GlassMaterial) -> GlassMaterial {
        for field in MaterialField::ALL {
            if let Some(value) = self.get(field) {
                field.set(&mut material, value);
            }
        }
        material
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlassTuning {
    pub raise_step: f32,
    pub edge_weight: f32,
    pub divider_weight: f32,
    pub veil: [f32; 2],
    pub rim_dim: f32,
    pub text_floor: f32,
    pub looks: [GlassLook; 2],
}

impl GlassTuning {
    pub fn shipped() -> Self {
        Self {
            raise_step: color::RAISE_STEP,
            edge_weight: color::EDGE_WEIGHT,
            divider_weight: color::DIVIDER_WEIGHT,
            veil: [0.05, 0.18],
            rim_dim: 0.3,
            text_floor: interface_theme::TEXT_ON_GLASS,
            looks: [ThemeMode::Light, ThemeMode::Dark].map(GlassLook::shipped),
        }
    }

    pub fn look_mut(&mut self, mode: ThemeMode) -> &mut GlassLook {
        &mut self.looks[usize::from(mode.is_dark())]
    }

    pub fn veil(&self, mode: ThemeMode) -> f32 {
        self.veil[usize::from(mode.is_dark())]
    }
}

impl Default for GlassTuning {
    fn default() -> Self {
        Self::shipped()
    }
}

thread_local! {
    static TUNING: RefCell<GlassTuning> = RefCell::new(GlassTuning::shipped());
}

pub fn glass_tuning() -> GlassTuning {
    TUNING.with_borrow(|tuning| *tuning)
}

pub fn read_glass_tuning<R>(read: impl FnOnce(&GlassTuning) -> R) -> R {
    TUNING.with_borrow(read)
}

pub fn set_glass_tuning(tuning: GlassTuning) {
    TUNING.set(tuning);
    color::forget_memo();
}
