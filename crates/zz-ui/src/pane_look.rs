use ala::{App, GlassMaterial, Global, Hsla, Pixels, px};
use penna::{ContentLook, Theme, foundation::tuning::read_glass_tuning};

use crate::{Colorize as _, Ground, Highlight};

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
        if active || !self.lights_active_pane(theme) {
            return tint;
        }
        let weight = read_glass_tuning(|tuning| tuning.veil(theme.mode)) * self.glow_strength;
        tint.blend(ala::black().alpha(weight.min(1.0)))
    }

    fn rim(self, theme: &Theme, active: bool) -> f32 {
        if active || !self.lights_active_pane(theme) {
            1.0
        } else {
            (1.0 - read_glass_tuning(|tuning| tuning.rim_dim) * self.glow_strength).max(0.0)
        }
    }

    pub fn frame(self, theme: &Theme, plane: Hsla, active: bool) -> GlassMaterial {
        let content = if plane.a < 1.0 {
            theme.content_plate()
        } else {
            plane
        };
        let tint = self.focus(theme, content, active);
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
        let frame = theme.glass_look.map_or(frame, |look| look.rim.apply(frame));
        frame.specular(frame.specular * self.rim(theme, active))
    }

    pub fn ground(self, theme: &Theme, plane: Hsla, active: bool) -> Ground {
        let content = if plane.a < 1.0 {
            theme.content_ground()
        } else {
            Ground {
                fill: plane,
                ..theme.ground()
            }
        };
        Ground {
            fill: self.focus(theme, content.fill, active),
            ..content
        }
    }

    pub fn edge_color(self, theme: &Theme, active: bool) -> Hsla {
        let divider = theme.divider_color();
        divider.alpha(divider.a * self.rim(theme, active))
    }

    pub fn edge_width(self, theme: &Theme) -> Pixels {
        if self.frames { outline(theme) } else { px(0.0) }
    }
}

pub fn rim_only(frame: GlassMaterial, theme: &Theme) -> GlassMaterial {
    let rim = frame.rim();
    match theme.glass_look.and_then(|look| {
        look.rim
            .get(penna::foundation::tuning::MaterialField::EdgeShadow)
    }) {
        Some(shadow) => rim.edge_shadow(shadow),
        None => rim,
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
        let liquid = penna::foundation::interface_theme::liquid(ThemeMode::Dark);
        for (glass, rim) in [
            (None, bare),
            (
                Some(GlassMaterial::regular().vanished().tint(plane.opaque())),
                bare,
            ),
            (Some(liquid), liquid),
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
    fn the_active_pane_sits_on_the_content_plate_below_the_sidebar() {
        use penna::{Elevation, ThemeColor, to_hex};
        let mode = ThemeMode::Dark;
        let mut theme = Theme::from(&*ThemeColor::for_mode(mode));
        theme.mode = mode;
        crate::interface_theme::builtin_theme("default")
            .expect("the default theme")
            .apply(&mut theme);
        theme.glass = None;
        let look = PaneLook::default();
        let window = theme.ground();
        let sidebar = theme.surface(Elevation::Panel).fill;
        assert_eq!(
            [
                look.frame(&theme, window.fill, true).tint,
                look.frame(&theme, window.fill, false).tint,
                sidebar,
            ]
            .map(to_hex),
            ["#1a1a19", "#151515", "#212120"]
        );

        theme.set_window(0.93, [0.0, 1.0]);
        let window = theme.ground().fill;
        assert_eq!(
            look.frame(&theme, window, true).tint,
            theme.background.alpha(0.57)
        );
        assert_eq!(to_hex(look.ground(&theme, window, true).fill), "#1a1a19f7");
    }

    #[test]
    fn the_composer_rests_on_the_activity_card_color_over_the_lit_pane() {
        use penna::{CardLook, Surface, ThemeColor, components::input::FieldLook, to_hex};
        let mode = ThemeMode::Dark;
        let mut theme = Theme::from(&*ThemeColor::for_mode(mode));
        theme.mode = mode;
        crate::interface_theme::builtin_theme("default")
            .expect("the default theme")
            .apply(&mut theme);
        theme.glass = None;
        let look = PaneLook::default();
        for (active, pane, card) in [(true, "#1a1a19", "#232322"), (false, "#151515", "#1f1f1e")] {
            let lit = look.ground(&theme, theme.background, active);
            let activity = theme
                .ground_of(Surface::from(CardLook::Activity).on(lit))
                .fill;
            let composer = theme
                .ground_of(Surface::from(FieldLook::Composer).on(lit))
                .fill;
            assert_eq!(
                (to_hex(lit.fill), to_hex(activity), to_hex(composer)),
                (pane.to_owned(), card.to_owned(), card.to_owned()),
                "active {active}"
            );
        }
    }

    #[test]
    fn the_active_pane_stands_apart_by_light_not_color() {
        let mut theme = Theme::default();
        theme.glass = Some(penna::foundation::interface_theme::liquid(ThemeMode::Dark));
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
