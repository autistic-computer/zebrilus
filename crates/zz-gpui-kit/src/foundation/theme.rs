//! The theme global: one palette, one set of metrics, for the whole process.

use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use zz_gpui::{
    Anchor, App, Corners, Edges, GlassMaterial, Global, Hsla, Pixels, SharedString, Window,
    WindowAppearance, px, size,
};

use crate::{BASE_UI_FONT_SIZE, TITLE_BAR_HEIGHT, highlighter::HighlightTheme};

use super::{
    Colorize as _, Highlight, Kind, PrimaryStyle, ShadowMaterial, Smoothing, StylePatch,
    SurfaceRole, ThemeColor, color,
};

/// The one gap in chrome: how far a control sits from the edge of the bar it
/// lives in, and from its neighbours. A bar pads by this and spaces its
/// children by it.
pub const CHROME_GAP: f32 = 6.0;

pub(super) fn init(cx: &mut App) {
    Theme::change(ThemeMode::Light, None, cx);
    Theme::sync_scrollbar_appearance(cx);
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    #[inline(always)]
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

/// When a scrollbar is visible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScrollbarShow {
    /// Show while scrolling, then fade out after idle.
    #[default]
    Scrolling,
    Hover,
    Always,
}

/// Where notification toasts stack, and how many.
#[derive(Debug, Clone)]
pub struct NotificationSettings {
    pub placement: Anchor,
    /// Insets from the window edges.
    pub margins: Edges<Pixels>,
    /// How many toasts are shown at once before the oldest is dropped.
    pub max_items: usize,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        let offset = px(16.);
        Self {
            placement: Anchor::TopCenter,
            margins: Edges {
                top: TITLE_BAR_HEIGHT + offset,
                right: offset,
                bottom: offset,
                left: offset,
            },
            max_items: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    /// The flat palette. Reachable directly through [`Deref`].
    pub colors: ThemeColor,
    /// Syntax colors for code blocks and the editor gutter.
    pub highlight_theme: Arc<HighlightTheme>,

    pub mode: ThemeMode,
    /// The font family for the application, default is `.SystemUIFont`.
    pub font_family: SharedString,
    /// The base font size for the application, default is 16px.
    pub font_size: Pixels,
    /// The monospace font family. Defaults to `Menlo` on Apple platforms,
    /// `Consolas` on Windows, `DejaVu Sans Mono` elsewhere.
    pub mono_font_family: SharedString,
    /// The monospace font size for the application, default is 13px.
    pub mono_font_size: Pixels,
    pub radius: Pixels,
    /// The corner radius of buttons, fields, tabs and other controls that
    /// round with the theme; `None` follows `radius`.
    pub control_radius: Option<Pixels>,
    pub contrast: f32,
    /// The width of the hairline every edged thing draws: controls, cards
    /// and floating surfaces. Zero draws none.
    pub outline: Pixels,
    /// Whether only the roles the theme turns the outline on for draw it:
    /// every built-in outline is off, and so is every edge drawn without a
    /// role. Dividers are lines, not outlines, and keep theirs.
    pub picked_outlines: bool,
    /// Whether only the roles the theme gives a shadow cast one: every
    /// built-in shadow is off, and so is every shadow drawn without a role.
    pub picked_shadows: bool,
    /// How the corners of every surface drawn by a role curve, unless the
    /// theme sets another for the role.
    pub smoothing: Smoothing,
    /// Whether only the roles the theme sets a smoothing for curve their
    /// corners that way: every other role a theme could smooth draws plain
    /// circular corners.
    pub picked_smoothing: bool,
    /// The tight shadow of things resting on the window.
    pub contact_shadow: ShadowMaterial,
    /// The deep shadow of things floating over it.
    pub float_shadow: ShadowMaterial,
    /// How far selected and hovered list rows float in from the list's
    /// edges, and whether they stand apart. Zero makes them full-width bands
    /// that touch.
    pub row_inset: Pixels,
    /// What the primary tone is drawn in.
    pub primary: PrimaryStyle,
    /// Whether highlights stay flat: a lift draws its wash a step deeper
    /// instead of the outline and shadow that raise it.
    pub flat_highlights: bool,
    /// Whether cards, panes and floating surfaces rise toward white in light
    /// mode, the way GNOME draws them, instead of darkening. Dark mode rises
    /// lighter either way.
    pub bright_surfaces: bool,
    /// How far a tint or lift highlight washes from the resting fill: 1 is
    /// the built-in step, 0 none, 2 twice as far.
    pub highlight_strength: f32,
    /// What a surface whose role takes glass becomes, and the edge pane
    /// frames take. Each surface tints it with its own color; the material's
    /// tint alpha says how much of that color covers the glass.
    pub glass: Option<GlassMaterial>,
    /// The theme's changes to each kind and role, by the name a theme file
    /// gives it.
    pub styles: HashMap<SharedString, StylePatch>,
    /// A role whose every surface draws a marker ring, so a theme editor can
    /// show where it appears.
    pub inspect: Option<SharedString>,
    pub inspect_strength: f32,
    /// How much of its own color a PiP lays over what is under it.
    pub pip_opacity: f32,
    /// How far panes stand in from the window's edges: zero while they sit
    /// flush.
    pub pane_margin: Pixels,
    /// Whether panes draw a [`Self::pane_frame`], which then owns their
    /// fill: set while panes have gaps.
    pub pane_frames: bool,
    /// How far the active pane stands apart from the others: 1 is the
    /// default contrast, 0 draws it like any other pane.
    pub pane_glow_strength: f32,
    pub transparent: Hsla,
    /// When scrollbars are visible, default: [`ScrollbarShow::Scrolling`].
    pub scrollbar_show: ScrollbarShow,
    pub notification: NotificationSettings,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from(&*ThemeColor::light())
    }
}

/// The corner smoothing every window draws with.
pub const CORNER_SMOOTHING: f32 = 4.0;
/// The share of its shorter side every window holds an ordinary corner radius
/// to, so a large theme radius never turns a control into a pill.
pub const ADAPTIVE_CORNER_FRACTION: f32 = 0.45;
/// How tall the controls in a pane's header are. They sit [`CHROME_GAP`] in
/// from the header's top and sides, which makes the header
/// [`PANE_HEADER_HEIGHT`] tall.
pub const PANE_HEADER_CONTROL_HEIGHT: Pixels = px(22.0);
pub const PANE_HEADER_HEIGHT: Pixels = px(22.0 + 2.0 * CHROME_GAP);
/// A pane header while panes sit flush: taller than [`PANE_HEADER_HEIGHT`],
/// so the window's buttons on its center line keep clear of the window's
/// corner.
const FLUSH_PANE_HEADER_HEIGHT: Pixels = px(40.0);
/// How the selected pane stands apart at full strength, the way Rex draws
/// it: the others under a veil of black, it washed toward white, each by
/// mode, and the others' rims this much dimmer (7% against its 10%).
const PANE_VEIL: [f32; 2] = [0.05, 0.18];
const PANE_WASH: [f32; 2] = [0.3, 0.05];
const PANE_RIM_DIM: f32 = 0.3;

impl Deref for Theme {
    type Target = ThemeColor;

    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.colors
    }
}

impl Global for Theme {}

impl Theme {
    pub fn control_radius(&self) -> Pixels {
        let radius = self.control_radius.unwrap_or(self.radius);
        if radius > px(24.0) {
            zz_gpui::FULL_CORNER_RADIUS
        } else {
            radius
        }
    }

    /// The corner radius of a color swatch: the controls', so a swatch
    /// rounds in step with the controls beside it.
    pub fn swatch_radius(&self) -> Pixels {
        self.control_radius()
    }

    /// The fill of the row a list's cursor is on.
    pub fn selection_background(&self) -> Hsla {
        self.accent.opaque()
    }

    pub fn menu_radius(&self) -> Pixels {
        (self.radius - px(4.0)).max(px(0.0))
    }

    /// The corner radius of a list row `row_height` tall: rounded while rows
    /// float, square when the look's row inset is zero.
    pub fn row_radius(&self, row_height: Pixels) -> Pixels {
        if self.inset_rows() {
            self.menu_radius().min(row_height * 0.40)
        } else {
            px(0.0)
        }
    }

    /// Whether list rows float as rounded highlights, inset from the list's
    /// edges and apart from each other. Square corners make them full-width
    /// bands that touch.
    pub fn inset_rows(&self) -> bool {
        self.row_inset > px(0.0)
    }

    /// Whether edged things draw their hairline at all.
    pub fn outlined(&self) -> bool {
        self.outline > px(0.0)
    }

    /// Whether an edge drawn without a role takes the hairline: the theme
    /// has one and does not pick where it goes.
    pub fn outlines_edges(&self) -> bool {
        self.outlined() && !self.picked_outlines
    }

    /// The theme's glass, when it draws anything a plain fill would not.
    pub fn glass_effect(&self) -> Option<GlassMaterial> {
        self.glass.filter(|material| !material.is_opaque_fill())
    }

    /// Whether `role` casts a shadow in this theme.
    pub fn casts_shadow(&self, role: SurfaceRole) -> bool {
        !self.shadows_of(self.style(role).shadow).is_empty()
    }

    /// The solid color of `fill` over the window color. Panes never paint
    /// it: the window shows through them. A PiP lays it over what is under
    /// it at [`Self::pip_opacity`].
    pub fn pane_color(&self, fill: Hsla) -> Hsla {
        self.background.opaque().blend(fill)
    }

    /// Whether the active pane stands apart from the others: the look's
    /// strength is on and the pane role's selected highlight is not none.
    pub fn lights_active_pane(&self) -> bool {
        self.pane_glow_strength > 0.0 && self.style(Kind::Pane.role()).selected != Highlight::None
    }

    /// The glass a gapped pane draws as its frame: the look's glass as the
    /// theme defines it, or a plain lit edge the outline's width when the
    /// look has none, with no frost or lensing. No color marks the active
    /// pane: it is washed lighter and the others are veiled darker, their
    /// rims dimmer, all scaled by
    /// [`Self::pane_glow_strength`].
    pub fn pane_frame(&self, plane: Hsla, active: bool) -> GlassMaterial {
        let glass = self.glass_effect();
        let tint = match glass {
            Some(material) if self.bright_surfaces && !self.mode.is_dark() => self
                .raise(plane.opaque(), 1)
                .alpha(if plane.a < 1.0 { material.tint.a } else { 1.0 }),
            _ if plane.a < 1.0 => plane.alpha(0.0),
            _ => plane,
        };
        let tint = self.pane_focus(tint, active);
        let frame = glass
            .unwrap_or_else(|| {
                let width = self.pane_outline();
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
        frame.specular(frame.specular * self.pane_rim(active))
    }

    /// `tint` washed lighter for the active pane, veiled darker for the
    /// others.
    fn pane_focus(&self, tint: Hsla, active: bool) -> Hsla {
        if !self.lights_active_pane() {
            return tint;
        }
        let (pole, weights) = if active {
            (zz_gpui::white(), PANE_WASH)
        } else {
            (zz_gpui::black(), PANE_VEIL)
        };
        let weight = weights[usize::from(self.mode.is_dark())] * self.pane_glow_strength;
        tint.blend(pole.alpha(weight.min(1.0)))
    }

    /// How strongly a pane's rim shows: in full on the active pane, dimmer
    /// on the others.
    fn pane_rim(&self, active: bool) -> f32 {
        if active || !self.lights_active_pane() {
            1.0
        } else {
            (1.0 - PANE_RIM_DIM * self.pane_glow_strength).max(0.0)
        }
    }

    /// The hairline a gapped pane draws where its frame has no glass: the
    /// divider color, dimmer on panes that are not the active one.
    pub fn pane_edge_color(&self, active: bool) -> Hsla {
        let divider = self.divider_color();
        divider.alpha(divider.a * self.pane_rim(active))
    }

    /// The width of the glint along a pane frame's edge: the theme's
    /// outline while panes have frames and take it, otherwise none.
    pub fn pane_edge_width(&self) -> Pixels {
        if self.pane_frames {
            self.pane_outline()
        } else {
            px(0.0)
        }
    }

    fn pane_outline(&self) -> Pixels {
        if self.style(Kind::Pane.role()).outline {
            self.outline
        } else {
            px(0.0)
        }
    }

    /// The corner radius of a gapped pane: concentric with the controls in
    /// its header, which sit [`CHROME_GAP`] in from its top and sides and
    /// round the way a window rounds a control their height.
    pub fn pane_radius(&self) -> Pixels {
        if self.radius <= px(0.0) {
            return px(0.0);
        }
        let control = size(PANE_HEADER_CONTROL_HEIGHT, PANE_HEADER_CONTROL_HEIGHT);
        Corners::all(self.control_radius())
            .resolve_radii_for_quad_size(control, ADAPTIVE_CORNER_FRACTION)
            .top_left
            + px(CHROME_GAP)
    }

    /// The height of a pane header: [`PANE_HEADER_HEIGHT`] with a pane
    /// margin, [`FLUSH_PANE_HEADER_HEIGHT`] while panes sit flush.
    pub fn pane_header_height(&self) -> Pixels {
        if self.pane_margin > px(0.0) {
            PANE_HEADER_HEIGHT
        } else {
            FLUSH_PANE_HEADER_HEIGHT
        }
    }

    /// The height of the strip along the window's top edge: a pane header
    /// with the pane margin above and below it, so the window's buttons sit
    /// on the pane headers' center line.
    pub fn title_bar_height(&self) -> Pixels {
        self.pane_header_height() + self.pane_margin * 2.0
    }

    /// The color of a divider rule: translucent foreground, so it lightens
    /// glass in a dark theme the way it does a solid fill.
    pub fn divider_color(&self) -> Hsla {
        self.foreground.opacity(color::edge_weight())
    }

    /// The radius of content that sits `inset` inside a surface, so the two
    /// stay concentric. Square corners stay square.
    pub fn inner_radius(&self, inset: Pixels) -> Pixels {
        (self.radius - inset).max(px(0.0))
    }

    pub fn set_contrast(&mut self, value: f32) {
        color::set_contrast(value);
        self.contrast = color::contrast();
    }

    #[inline(always)]
    pub fn global(cx: &App) -> &Theme {
        cx.global::<Theme>()
    }

    #[inline(always)]
    pub fn global_mut(cx: &mut App) -> &mut Theme {
        cx.global_mut::<Theme>()
    }

    #[inline(always)]
    pub fn is_dark(&self) -> bool {
        self.mode.is_dark()
    }

    pub fn sync_system_appearance(window: Option<&mut Window>, cx: &mut App) {
        // Prefer the window's own appearance: the app-level query errors on
        // Linux (gpui-component#104).
        let appearance = window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance());

        Self::change(appearance, window, cx);
    }

    pub fn sync_scrollbar_appearance(cx: &mut App) {
        Theme::global_mut(cx).scrollbar_show = if cx.should_auto_hide_scrollbars() {
            ScrollbarShow::Scrolling
        } else {
            ScrollbarShow::Hover
        };
    }

    /// Switch to `mode`, resetting the palette to that mode's built-in colors.
    /// Installs the global on first call. Fonts, radii and the notification
    /// placement are left alone.
    pub fn change(mode: impl Into<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
        let mode = mode.into();
        if !cx.has_global::<Theme>() {
            cx.set_global(Theme::default());
        }

        let colors = *ThemeColor::for_mode(mode);
        let theme = cx.global_mut::<Theme>();
        theme.mode = mode;
        theme.colors = colors;
        theme.highlight_theme = if mode.is_dark() {
            HighlightTheme::default_dark()
        } else {
            HighlightTheme::default_light()
        };

        if let Some(window) = window {
            window.refresh();
        }
    }

    /// The code editor background, falling back to one elevation step when the
    /// highlight theme does not set one.
    #[inline]
    pub fn editor_background(&self) -> Hsla {
        self.highlight_theme
            .style
            .editor_background
            .unwrap_or_else(|| self.background.raised(1))
    }
}

impl From<&ThemeColor> for Theme {
    fn from(colors: &ThemeColor) -> Self {
        Theme {
            mode: ThemeMode::default(),
            transparent: Hsla::transparent_black(),
            font_family: ".SystemUIFont".into(),
            font_size: px(BASE_UI_FONT_SIZE),
            mono_font_family: if cfg!(any(target_os = "macos", target_os = "ios")) {
                "Menlo".into()
            } else if cfg!(target_os = "windows") {
                "Consolas".into()
            } else {
                "DejaVu Sans Mono".into()
            },
            mono_font_size: px(13.),
            radius: px(6.),
            control_radius: None,
            contrast: 1.0,
            outline: px(0.5),
            picked_outlines: false,
            picked_shadows: false,
            smoothing: Smoothing::THEME,
            picked_smoothing: false,
            contact_shadow: ShadowMaterial::CONTACT,
            float_shadow: ShadowMaterial::FLOAT,
            row_inset: px(4.0),
            primary: PrimaryStyle::Foreground,
            flat_highlights: false,
            bright_surfaces: false,
            highlight_strength: 1.0,
            glass: None,
            styles: HashMap::default(),
            inspect: None,
            inspect_strength: 1.0,
            pip_opacity: 0.8,
            pane_margin: px(0.0),
            pane_frames: false,
            pane_glow_strength: 1.0,
            scrollbar_show: ScrollbarShow::default(),
            notification: NotificationSettings::default(),
            colors: *colors,
            highlight_theme: HighlightTheme::default_light(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl ThemeMode {
    #[inline(always)]
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }

    /// The lowercase name: `light`, `dark`.
    pub fn name(&self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

impl From<WindowAppearance> for ThemeMode {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_radius_rounds_controls_after_24_without_changing_surfaces() {
        for radius in [0.0, 6.0, 24.0, 24.5, 25.0] {
            let theme = Theme {
                radius: px(radius),
                ..Theme::default()
            };
            for (width, height) in [(24.0, 24.0), (120.0, 30.0), (360.0, 40.0)] {
                let size = zz_gpui::size(px(width), px(height));
                let control = zz_gpui::Corners::all(theme.control_radius())
                    .resolve_radii_for_quad_size(size, ADAPTIVE_CORNER_FRACTION);
                let surface = zz_gpui::Corners::all(theme.radius)
                    .resolve_radii_for_quad_size(size, ADAPTIVE_CORNER_FRACTION);
                if radius > 24.0 {
                    assert_eq!(control, zz_gpui::Corners::all(px(height / 2.0)));
                    assert!(surface.top_left < control.top_left);
                } else {
                    assert_eq!(control, surface);
                }
                assert_eq!(theme.radius, px(radius));
            }
        }
    }

    #[test]
    fn a_pane_frame_lights_an_opaque_edge_with_or_without_the_looks_glass() {
        let mut theme = Theme {
            pane_glow_strength: 0.0,
            ..Theme::default()
        };
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
            let frame = theme.pane_frame(plane, false);
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
        assert_eq!(theme.pane_frame(plane, false).specular, frosted.specular);
        theme.glass = None;
        assert_eq!(theme.pane_frame(plane, false).specular, 0.0);
        assert_eq!(theme.pane_edge_width(), px(0.0));
        theme.outline = px(2.0);
        assert_eq!(theme.pane_edge_width(), px(0.0));
        theme.pane_frames = true;
        assert_eq!(theme.pane_edge_width(), px(2.0));
    }

    #[test]
    fn a_pane_frame_over_a_translucent_plane_adds_nothing_to_it() {
        let theme = Theme {
            pane_glow_strength: 0.0,
            ..Theme::default()
        };
        let plane = theme.background.alpha(0.93);
        assert_eq!(theme.pane_frame(plane, false).tint.a, 0.0);
        assert_eq!(theme.pane_frame(plane.opaque(), false).tint.a, 1.0);
    }

    #[test]
    fn bright_glass_panes_rise_in_light_mode_only() {
        let mut light = (*ThemeColor::light()).clone();
        light.background = zz_gpui::hsla(0., 0., 0.97, 1.);
        for (mode, colors) in [
            (ThemeMode::Light, &light),
            (ThemeMode::Dark, &*ThemeColor::dark()),
        ] {
            let mut theme = Theme::from(colors);
            theme.mode = mode;
            let glass = GlassMaterial::regular();
            theme.glass = Some(glass);
            theme.pane_glow_strength = 0.0;
            for alpha in [0.7, 1.0] {
                let plane = theme.background.alpha(alpha);
                let plain = theme.pane_frame(plane, false);
                theme.bright_surfaces = true;
                let bright = theme.pane_frame(plane, false);
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
    fn a_pane_rounds_concentric_with_its_header_controls() {
        let control = size(px(96.0), PANE_HEADER_CONTROL_HEIGHT);
        for radius in [4.0, 10.0, 24.0, 25.0] {
            let theme = Theme {
                radius: px(radius),
                ..Theme::default()
            };
            let rounded = Corners::all(theme.control_radius())
                .resolve_radii_for_quad_size(control, ADAPTIVE_CORNER_FRACTION);
            let gap = theme.pane_radius() - rounded.top_left;
            assert!((gap - px(CHROME_GAP)).abs() < px(0.001));
        }
        let capsules = Theme {
            radius: px(25.0),
            ..Theme::default()
        };
        assert_eq!(capsules.pane_radius(), PANE_HEADER_HEIGHT / 2.0);
        let square = Theme {
            radius: px(0.0),
            ..Theme::default()
        };
        assert_eq!(square.pane_radius(), px(0.0));
    }

    #[test]
    fn the_title_bar_shares_the_pane_headers_center_line() {
        let mut theme = Theme::default();
        assert_eq!(theme.title_bar_height(), FLUSH_PANE_HEADER_HEIGHT);
        theme.pane_margin = px(8.0);
        assert_eq!(
            theme.title_bar_height() / 2.0,
            theme.pane_margin + PANE_HEADER_HEIGHT / 2.0
        );
    }

    #[test]
    fn the_active_pane_stands_apart_by_light_not_color() {
        let mut theme = Theme::default();
        let plane = theme.background;
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            theme.mode = mode;
            let active = theme.pane_frame(plane, true);
            let inactive = theme.pane_frame(plane, false);
            assert!(
                crate::oklab_lightness(inactive.tint) < crate::oklab_lightness(active.tint),
                "{mode:?}"
            );
            assert_eq!(active.light_color, inactive.light_color);
            assert!(inactive.specular < active.specular);
            assert!(theme.pane_edge_color(false).a < theme.pane_edge_color(true).a);
        }
        theme.pane_glow_strength = 0.0;
        assert_eq!(
            theme.pane_frame(plane, true),
            theme.pane_frame(plane, false)
        );
        assert_eq!(theme.pane_edge_color(false), theme.divider_color());
    }
}
