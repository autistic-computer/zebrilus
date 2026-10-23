#![allow(
    clippy::disallowed_types,
    reason = "a program tile lights and shades the brand color it carries, not a surface of the look"
)]

use std::borrow::Cow;

use zz_client::program::{Look, Program};
use zz_gpui::{
    AnyElement, App, AssetSource, Background, BoxShadow, Global, Hsla, IntoElement as _, Length,
    ParentElement, Pixels, Result, Rgba, SharedString, Styled, div, img, linear_color_stop,
    linear_gradient, point, px,
};

use crate::{
    ActiveTheme as _, Colorize as _, Icon, PaintState, Surface,
    navigation::pane_stack::stack_radius, roles, to_oklch,
};

pub const PROGRAM_GLYPH_SCALE: f32 = 0.625;

pub struct ProgramColors(pub bool);

impl Global for ProgramColors {}

#[must_use]
pub fn program_colors(cx: &App) -> bool {
    cx.try_global::<ProgramColors>()
        .is_none_or(|colors| colors.0)
}

#[derive(rust_embed::RustEmbed)]
#[folder = "assets"]
#[include = "programs/*.svgz"]
struct Programs;

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match Programs::get(path) {
            Some(asset) => Ok(Some(asset.data)),
            None => zz_gpui_kit::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = zz_gpui_kit::Assets.list(path)?;
        assets.extend(
            Programs::iter()
                .filter(|asset| asset.starts_with(path))
                .map(Into::into),
        );
        Ok(assets)
    }
}

fn color(hex: u32) -> Hsla {
    Rgba {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

#[must_use]
pub fn program_background(program: &Program) -> Hsla {
    match program.look {
        Look::Mark { background, .. } | Look::Art { background } => color(background),
    }
}

#[derive(Clone, Copy)]
pub struct Lit {
    pub size: Pixels,
    pub dark: bool,
    pub strength: f32,
}

fn shadow(y: Pixels, blur: Pixels, spread: Pixels, color: Hsla, inset: bool) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(Pixels::ZERO, y),
        blur_radius: blur,
        spread_radius: spread,
        inset,
    }
}

impl Lit {
    fn unit(&self, value: f32) -> Pixels {
        px(value * f32::from(self.size) / 28.0)
    }

    #[must_use]
    pub fn background(fill: Hsla) -> Background {
        linear_gradient(
            180.0,
            linear_color_stop(fill.mix_oklab(Hsla::white(), 0.84), 0.0),
            linear_color_stop(fill.mix_oklab(Hsla::black(), 0.9), 1.0),
        )
    }

    #[must_use]
    pub fn shadows(&self) -> Vec<BoxShadow> {
        let zero = Pixels::ZERO;
        let (contact, edge, ring, width) = if self.dark {
            (0.4, 0.5, Hsla::white().alpha(0.12 * self.strength), 0.75)
        } else {
            (0.2, 0.18, Hsla::white().alpha(0.8 * self.strength), 0.5)
        };
        vec![
            shadow(
                self.unit(0.5),
                self.unit(1.5),
                zero,
                Hsla::black().alpha(contact * self.strength),
                false,
            ),
            shadow(
                zero,
                zero,
                self.unit(width),
                Hsla::black().alpha(edge * self.strength),
                false,
            ),
            shadow(
                self.unit(1.0),
                zero,
                zero,
                Hsla::white().alpha(0.3 * self.strength),
                true,
            ),
            shadow(zero, zero, self.unit(width), ring, true),
        ]
    }
}

#[derive(Clone, Copy)]
pub struct Face {
    pub program: &'static Program,
    pub size: Pixels,
    pub radius: Pixels,
    pub smoothing: f32,
    pub dark: bool,
    pub ground: Hsla,
    pub dimmed: f32,
}

impl Face {
    fn dim(&self, color: Hsla) -> Hsla {
        if self.dimmed > 0.0 {
            color.mix_oklab(self.ground, self.dimmed)
        } else {
            color
        }
    }

    fn lit(&self) -> Lit {
        Lit {
            size: self.size,
            dark: self.dark,
            strength: if self.dimmed > 0.0 { self.dimmed } else { 1.0 },
        }
    }

    fn glyph(&self) -> AnyElement {
        let program = self.program;
        let size = self.size * PROGRAM_GLYPH_SCALE;
        let path = program_glyph_path(program);
        match program.look {
            Look::Mark { foreground, .. } => Icon::empty()
                .path(path)
                .flex_none()
                .size(size)
                .text_color(self.dim(color(foreground)))
                .into_any_element(),
            Look::Art { .. } => img(path)
                .flex_none()
                .size(size)
                .opacity(1.0 - self.dimmed)
                .into_any_element(),
        }
    }

    #[must_use]
    pub fn background(&self) -> Background {
        Lit::background(self.dim(program_background(self.program)))
    }

    pub fn paint<E: Styled + ParentElement>(self, tile: E) -> E {
        tile.bg(self.background())
            .shadow(self.lit().shadows())
            .child(self.glyph())
    }

    pub fn tile(self) -> zz_gpui::Div {
        let tile = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(self.size)
            .rounded(self.radius)
            .corner_smoothing(self.smoothing);
        self.paint(tile)
    }
}

#[must_use]
pub fn program_glyph_path(program: &Program) -> SharedString {
    match program.icon {
        Some(icon) => format!("icons/{icon}.svg"),
        None => format!("programs/{}.svgz", program.id),
    }
    .into()
}

pub fn program_tile(
    program: &'static Program,
    size: Pixels,
    around: Pixels,
    dimmed: f32,
    cx: &App,
) -> zz_gpui::Div {
    let theme = cx.theme();
    Face {
        program,
        size,
        radius: stack_radius(size, around, cx),
        smoothing: theme
            .layer(Surface::new(roles::PANE_STACK), PaintState::Rest)
            .smoothing,
        dark: theme.is_dark(),
        ground: theme.ground().fill,
        dimmed,
    }
    .tile()
}

const BRAND_INK_SEPARATION: f32 = 0.28;

fn brand_ink(background: Hsla, foreground: Hsla, ground: Hsla) -> Option<Hsla> {
    let lightness = to_oklch(ground).0;
    let mut inks = [background, foreground];
    inks.sort_by(|a, b| to_oklch(*b).1.total_cmp(&to_oklch(*a).1));
    inks.into_iter()
        .find(|ink| (to_oklch(*ink).0 - lightness).abs() >= BRAND_INK_SEPARATION)
}

pub fn program_mark(
    program: &'static Program,
    size: impl Clone + Into<Length>,
    ink: Hsla,
    brand_on: Option<Hsla>,
) -> Option<AnyElement> {
    let path = program_glyph_path(program);
    match program.look {
        Look::Mark {
            background,
            foreground,
        } => {
            let ink = brand_on
                .and_then(|ground| brand_ink(color(background), color(foreground), ground))
                .unwrap_or(ink);
            Some(
                Icon::empty()
                    .path(path)
                    .flex_none()
                    .size(size)
                    .text_color(ink)
                    .into_any_element(),
            )
        }
        Look::Art { .. } => brand_on.map(|_| img(path).flex_none().size(size).into_any_element()),
    }
}
