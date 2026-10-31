use ala::{App, GlassMaterial, Global, Hsla, Pixels, px};
use penna::Theme;

use crate::{Colorize as _, Highlight, roles};

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
        self.glow_strength > 0.0 && theme.style(roles::PANE).selected != Highlight::None
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
        let glass = theme.glass_effect();
        let tint = match glass {
            Some(material) if theme.bright_surfaces && !theme.mode.is_dark() => theme
                .raise(plane.opaque(), 1)
                .alpha(if plane.a < 1.0 { material.tint.a } else { 1.0 }),
            _ if plane.a < 1.0 => plane.alpha(0.0),
            _ => plane,
        };
        let tint = self.focus(theme, tint, active);
        let frame = glass
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
    if theme.style(roles::PANE).outline {
        theme.outline
    } else {
        px(0.0)
    }
}

#[cfg(test)]
mod tests {
    use penna::{ThemeColor, ThemeMode};

    use super::*;

    const UNLIT: PaneLook = PaneLook {
        frames: false,
        glow_strength: 0.0,
    };

    #[test]
    fn a_pane_frame_lights_an_opaque_edge_with_or_without_the_looks_glass() {
        let mut theme = Theme::default();
        let plane = theme.background;
        let tint = plane;
        let edge = GlassMaterial::edge(tint).glint_width(theme.outline);
        let frosted = GlassMaterial::frosted();
        for (glass, rim) in [
            (None, edge),
            (
                Some(GlassMaterial::regular().vanished().tint(plane.opaque())),
                edge,
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

        theme.outline = px(0.0);
        assert_eq!(UNLIT.frame(&theme, plane, false).specular, frosted.specular);
        theme.glass = None;
        assert_eq!(UNLIT.frame(&theme, plane, false).specular, 0.0);
        assert_eq!(UNLIT.edge_width(&theme), px(0.0));
        theme.outline = px(2.0);
        assert_eq!(UNLIT.edge_width(&theme), px(0.0));
        let framed = PaneLook {
            frames: true,
            ..UNLIT
        };
        assert_eq!(framed.edge_width(&theme), px(2.0));
    }

    #[test]
    fn a_pane_frame_over_a_translucent_plane_adds_nothing_to_it() {
        let theme = Theme::default();
        let plane = theme.background.alpha(0.93);
        assert_eq!(UNLIT.frame(&theme, plane, false).tint.a, 0.0);
        assert_eq!(UNLIT.frame(&theme, plane.opaque(), false).tint.a, 1.0);
    }

    #[test]
    fn bright_glass_panes_rise_in_light_mode_only() {
        let mut light = *ThemeColor::light();
        light.background = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.97,
            a: 1.0,
        };
        for (mode, colors) in [
            (ThemeMode::Light, &light),
            (ThemeMode::Dark, &*ThemeColor::dark()),
        ] {
            let mut theme = Theme::from(colors);
            theme.mode = mode;
            let glass = GlassMaterial::regular();
            theme.glass = Some(glass);
            for alpha in [0.7, 1.0] {
                let plane = theme.background.alpha(alpha);
                let plain = UNLIT.frame(&theme, plane, false);
                theme.bright_surfaces = true;
                let bright = UNLIT.frame(&theme, plane, false);
                if theme.mode.is_dark() {
                    assert_eq!(bright, plain);
                } else {
                    assert!(bright.tint.l > plane.l);
                    assert_eq!(bright.tint.a, if alpha < 1.0 { glass.tint.a } else { 1.0 });
                    assert_eq!(bright.fallback, Some(bright.tint));
                }
                theme.bright_surfaces = false;
            }
        }
    }

    #[test]
    fn the_active_pane_stands_apart_by_light_not_color() {
        let mut theme = Theme::default();
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
