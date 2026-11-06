use ala::{App, GlassMaterial, Global, Hsla, Pixels, px};
use penna::{ContentLook, Theme};

use crate::{Colorize as _, Highlight};

const VEIL: [f32; 2] = [0.05, 0.18];
const WASH: [f32; 2] = [0.3, 0.05];
const RIM_DIM: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneLook {
    pub frames: bool,
    pub glow_strength: f32,
}

impl Default for PaneLook {
    fn default() -> Self {
        Self {
            frames: false,
            glow_strength: 1.0,
        }
    }
}

impl Global for PaneLook {}

impl PaneLook {
    pub fn global(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }

    fn lights_active_pane(self, theme: &Theme) -> bool {
        self.glow_strength > 0.0 && theme.style_of(ContentLook::Pane).selected != Highlight::None
    }

    fn focus(self, theme: &Theme, tint: Hsla, active: bool) -> Hsla {
        if !self.lights_active_pane(theme) {
            return tint;
        }
        let (pole, weights) = if active {
            (ala::white(), WASH)
        } else {
            (ala::black(), VEIL)
        };
        let weight = weights[usize::from(theme.mode.is_dark())] * self.glow_strength;
        tint.blend(pole.alpha(weight.min(1.0)))
    }

    fn rim(self, theme: &Theme, active: bool) -> f32 {
        if active || !self.lights_active_pane(theme) {
            1.0
        } else {
            (1.0 - RIM_DIM * self.glow_strength).max(0.0)
        }
    }

    pub fn frame(self, theme: &Theme, plane: Hsla, active: bool) -> GlassMaterial {
        let tint = if plane.a < 1.0 {
            plane.alpha(0.0)
        } else {
            plane
        };
        let tint = self.focus(theme, tint, active);
        let frame = theme
            .glass_effect()
            .unwrap_or_else(|| {
                let width = outline(theme);
                let edge = GlassMaterial::edge(tint).glint_width(width);
                if width > px(0.0) {
                    edge
                } else {
                    edge.specular(0.0)
                }
            })
            .blur(px(0.0))
            .refraction(px(0.0))
            .dispersion(0.0)
            .tint(tint)
            .fallback(tint);
        frame.specular(frame.specular * self.rim(theme, active))
    }

    pub fn edge_color(self, theme: &Theme, active: bool) -> Hsla {
        let divider = theme.divider_color();
        divider.alpha(divider.a * self.rim(theme, active))
    }

    pub fn edge_width(self, theme: &Theme) -> Pixels {
        if self.frames { outline(theme) } else { px(0.0) }
    }
}

pub fn pane_color(theme: &Theme, fill: Hsla) -> Hsla {
    theme.background.opaque().blend(fill)
}

fn outline(theme: &Theme) -> Pixels {
    if theme.style_of(ContentLook::Pane).outline {
        theme.outline
    } else {
        px(0.0)
    }
}

#[cfg(test)]
mod tests {
    use penna::ThemeMode;

    use super::*;

    const UNLIT: PaneLook = PaneLook {
        frames: false,
        glow_strength: 0.0,
    };

    #[test]
    fn a_pane_frame_takes_the_glass_rim_and_no_rim_without_glass() {
        let mut theme = Theme::default();
        let plane = theme.background;
        let tint = plane;
        let bare = GlassMaterial::edge(tint).glint_width(px(0.0)).specular(0.0);
        let frosted = GlassMaterial::frosted();
        for (glass, rim) in [
            (None, bare),
            (
                Some(GlassMaterial::regular().vanished().tint(plane.opaque())),
                bare,
            ),
            (Some(frosted), frosted),
        ] {
            theme.glass = glass;
            let frame = UNLIT.frame(&theme, plane, false);
            assert_eq!(frame.tint, tint);
            assert_eq!(frame.fallback, Some(tint));
            assert!(!frame.reads_backdrop());
            assert!(!frame.is_opaque_fill());
            assert_eq!(
                (frame.blur, frame.refraction, frame.dispersion),
                (px(0.0), px(0.0), 0.0)
            );
            assert_eq!(
                (
                    frame.bezel,
                    frame.glint_width,
                    frame.specular,
                    frame.fresnel
                ),
                (rim.bezel, rim.glint_width, rim.specular, rim.fresnel)
            );
            assert_eq!(
                (frame.noise, frame.light_color),
                (rim.noise, rim.light_color)
            );
        }

        let framed = PaneLook {
            frames: true,
            ..UNLIT
        };
        assert_eq!(framed.edge_width(&theme), px(0.0));
    }

    #[test]
    fn a_pane_frame_paints_an_opaque_plane_and_leaves_a_see_through_one_clear() {
        let theme = Theme::default();
        let plane = theme.background.alpha(0.93);
        assert_eq!(UNLIT.frame(&theme, plane, false).tint, plane.alpha(0.0));
        assert_eq!(
            UNLIT.frame(&theme, plane.opaque(), false).tint,
            plane.opaque()
        );
    }

    #[test]
    fn the_active_pane_stands_apart_by_light_not_color() {
        let mut theme = Theme::default();
        theme.glass = Some(GlassMaterial::frosted());
        let look = PaneLook::default();
        let plane = theme.background;
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            theme.mode = mode;
            let active = look.frame(&theme, plane, true);
            let inactive = look.frame(&theme, plane, false);
            assert!(
                crate::oklab_lightness(inactive.tint) < crate::oklab_lightness(active.tint),
                "{mode:?}"
            );
            assert_eq!(active.light_color, inactive.light_color);
            assert!(inactive.specular < active.specular);
            assert!(look.edge_color(&theme, false).a < look.edge_color(&theme, true).a);
        }
        assert_eq!(
            UNLIT.frame(&theme, plane, true),
            UNLIT.frame(&theme, plane, false)
        );
        assert_eq!(UNLIT.edge_color(&theme, false), theme.divider_color());
    }
}
