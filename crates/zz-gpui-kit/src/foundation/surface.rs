//! Surfaces and their roles.
//!
//! Every painted piece of UI names a [`SurfaceRole`]: a [`Kind`] of widget and, for
//! the pieces a theme wants to tell apart, the place it sits in. A role's
//! [`Style`] says which treatments it takes (a fill, an outline, a shadow, a
//! shape, and what pointing at it or selecting it adds), and the theme's
//! materials say what each treatment looks like: its radius, outline width,
//! shadows and glass. A theme changes a kind or a role with a [`StylePatch`];
//! a role takes the kind's built-in style, then the theme's patch for the
//! kind, then its own built-in patch, then the theme's patch for the role.

use serde::{Deserialize, Serialize};
use zz_gpui::{
    App, BoxShadow, FULL_CORNER_RADIUS, GlassMaterial, Hsla, InteractiveElement, Pixels,
    StatefulInteractiveElement, Styled, point, px,
};

use super::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, Ground, PaintState, Theme, Tone,
};

/// What a surface is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    /// Nothing: what is behind shows through.
    None,
    /// A translucent wash over what is behind.
    Tint,
    /// An opaque fill a step off what it sits on.
    Solid,
    /// The theme's glass, tinted with the solid fill. On glass it paints a
    /// tint instead, and where the theme has no glass it paints solid.
    Glass,
}

/// Which of the theme's shadows a surface casts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shadow {
    None,
    /// The tight shadow of something resting on the window.
    Contact,
    /// The contact shadow under a deep, soft one, for what floats.
    Float,
}

/// The corners of a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Square,
    /// The theme's radius. Rows take it less the list's inset, so they sit
    /// concentric inside the menu or panel that holds them, and go square
    /// when the theme insets nothing.
    Round,
    /// Ends rounded into half circles.
    Pill,
}

/// What a surface adds while it is pointed at or pressed, or while it is
/// the selected or current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Highlight {
    None,
    /// A wash a step deeper than the fill.
    Tint,
    /// The wash with the outline and a contact shadow, as if the surface
    /// rose off what it sits on.
    Lift,
    /// The accent, with the text that reads on it.
    Accent,
    /// A ring in the accent just outside the edge.
    Ring,
    /// A bar in the accent along the leading edge.
    Bar,
}

/// The treatments a surface takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    pub fill: Fill,
    /// Whether it draws the theme's outline.
    pub outline: bool,
    pub shadow: Shadow,
    pub shape: Shape,
    pub hover: Highlight,
    pub selected: Highlight,
}

/// Changes to a [`Style`]: what a theme file writes for a kind or a role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StylePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<Highlight>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<Highlight>,
}

impl StylePatch {
    pub const EMPTY: Self = Self {
        fill: None,
        outline: None,
        shadow: None,
        shape: None,
        hover: None,
        selected: None,
    };

    pub const fn fill(self, fill: Fill) -> Self {
        Self {
            fill: Some(fill),
            ..self
        }
    }

    pub const fn outline(self, outline: bool) -> Self {
        Self {
            outline: Some(outline),
            ..self
        }
    }

    pub const fn shadow(self, shadow: Shadow) -> Self {
        Self {
            shadow: Some(shadow),
            ..self
        }
    }

    pub const fn shape(self, shape: Shape) -> Self {
        Self {
            shape: Some(shape),
            ..self
        }
    }

    pub const fn hover(self, hover: Highlight) -> Self {
        Self {
            hover: Some(hover),
            ..self
        }
    }

    pub const fn selected(self, selected: Highlight) -> Self {
        Self {
            selected: Some(selected),
            ..self
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::EMPTY
    }

    pub fn apply(&self, style: &mut Style) {
        if let Some(fill) = self.fill {
            style.fill = fill;
        }
        if let Some(outline) = self.outline {
            style.outline = outline;
        }
        if let Some(shadow) = self.shadow {
            style.shadow = shadow;
        }
        if let Some(shape) = self.shape {
            style.shape = shape;
        }
        if let Some(hover) = self.hover {
            style.hover = hover;
        }
        if let Some(selected) = self.selected {
            style.selected = selected;
        }
    }

    /// The patch that turns `from` into `to`, naming only what differs.
    pub fn between(from: Style, to: Style) -> Self {
        let differs = |same: bool| !same;
        Self {
            fill: differs(from.fill == to.fill).then_some(to.fill),
            outline: differs(from.outline == to.outline).then_some(to.outline),
            shadow: differs(from.shadow == to.shadow).then_some(to.shadow),
            shape: differs(from.shape == to.shape).then_some(to.shape),
            hover: differs(from.hover == to.hover).then_some(to.hover),
            selected: differs(from.selected == to.selected).then_some(to.selected),
        }
    }
}

/// The kinds of widget every role is one of. Each carries a built-in style;
/// the Default theme changes none of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Bar,
    Panel,
    Pane,
    Card,
    Tile,
    Row,
    Tab,
    Chip,
    Button,
    IconButton,
    Field,
    Badge,
    Keycap,
    Popover,
    Dialog,
    Sheet,
    Toast,
    Tooltip,
    Rule,
}

impl Kind {
    pub const ALL: [Self; 19] = [
        Self::Bar,
        Self::Panel,
        Self::Pane,
        Self::Card,
        Self::Tile,
        Self::Row,
        Self::Tab,
        Self::Chip,
        Self::Button,
        Self::IconButton,
        Self::Field,
        Self::Badge,
        Self::Keycap,
        Self::Popover,
        Self::Dialog,
        Self::Sheet,
        Self::Toast,
        Self::Tooltip,
        Self::Rule,
    ];

    /// What a theme file calls it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Panel => "panel",
            Self::Pane => "pane",
            Self::Card => "card",
            Self::Tile => "tile",
            Self::Row => "row",
            Self::Tab => "tab",
            Self::Chip => "chip",
            Self::Button => "button",
            Self::IconButton => "icon-button",
            Self::Field => "field",
            Self::Badge => "badge",
            Self::Keycap => "keycap",
            Self::Popover => "popover",
            Self::Dialog => "dialog",
            Self::Sheet => "sheet",
            Self::Toast => "toast",
            Self::Tooltip => "tooltip",
            Self::Rule => "rule",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Bar => "Bars",
            Self::Panel => "Panels",
            Self::Pane => "Panes",
            Self::Card => "Cards",
            Self::Tile => "Tiles",
            Self::Row => "Rows",
            Self::Tab => "Tabs",
            Self::Chip => "Chips",
            Self::Button => "Buttons",
            Self::IconButton => "Icon buttons",
            Self::Field => "Fields",
            Self::Badge => "Badges",
            Self::Keycap => "Keycaps",
            Self::Popover => "Popovers and menus",
            Self::Dialog => "Dialogs",
            Self::Sheet => "Sheets",
            Self::Toast => "Toasts",
            Self::Tooltip => "Tooltips",
            Self::Rule => "Dividers",
        }
    }

    const fn about(self) -> &'static str {
        match self {
            Self::Bar => "Strips across a window or pane: the title bar, pane headers, footers",
            Self::Panel => "Columns set into the window, like the sidebar",
            Self::Pane => "Terminal, browser, agent and editor panes",
            Self::Card => "Raised boxes in the flow: message bubbles, setting groups, code blocks",
            Self::Tile => "Pickable thumbnails and image tiles",
            Self::Row => "Rows in lists and menus",
            Self::Tab => "Tabs and window pills",
            Self::Chip => "Small status chips over content",
            Self::Button => "Buttons with a label",
            Self::IconButton => "Buttons that are only an icon",
            Self::Field => "Text fields, selects and anything you type into",
            Self::Badge => "Small labels and counts",
            Self::Keycap => "Key hints",
            Self::Popover => "Menus, dropdowns and popovers",
            Self::Dialog => "Dialogs and modal pickers",
            Self::Sheet => "Sheets that rise from the bottom",
            Self::Toast => "Notifications and banners",
            Self::Tooltip => "Tooltips",
            Self::Rule => "Divider lines",
        }
    }

    /// The kind as a role, for a widget nothing more specific names.
    pub const fn role(self) -> SurfaceRole {
        SurfaceRole::new(self.name(), self, self.label(), self.about())
    }

    /// The built-in style, which draws the Default theme.
    pub const fn base(self) -> Style {
        const fn style(
            fill: Fill,
            outline: bool,
            shadow: Shadow,
            hover: Highlight,
            selected: Highlight,
        ) -> Style {
            Style {
                fill,
                outline,
                shadow,
                shape: Shape::Round,
                hover,
                selected,
            }
        }
        use Highlight as H;
        match self {
            Self::Bar | Self::Panel => Style {
                shape: Shape::Square,
                ..style(Fill::None, false, Shadow::None, H::None, H::None)
            },
            Self::Rule => Style {
                shape: Shape::Square,
                ..style(Fill::None, true, Shadow::None, H::None, H::None)
            },
            Self::Pane => style(Fill::Glass, true, Shadow::Contact, H::None, H::Ring),
            Self::Card | Self::Tile => style(Fill::Solid, true, Shadow::Contact, H::None, H::Ring),
            Self::Row => style(Fill::None, false, Shadow::None, H::Tint, H::Accent),
            Self::Tab => style(Fill::None, false, Shadow::None, H::Lift, H::Lift),
            Self::Chip => style(Fill::Solid, true, Shadow::Contact, H::None, H::Tint),
            Self::Button => style(Fill::Solid, true, Shadow::Contact, H::Tint, H::Tint),
            Self::IconButton => style(Fill::None, false, Shadow::None, H::Lift, H::Lift),
            Self::Field => style(Fill::Tint, true, Shadow::None, H::Tint, H::None),
            Self::Badge | Self::Keycap => style(Fill::Tint, true, Shadow::None, H::None, H::None),
            Self::Popover | Self::Dialog | Self::Sheet | Self::Toast => {
                style(Fill::Glass, true, Shadow::Float, H::None, H::Accent)
            }
            Self::Tooltip => style(Fill::Glass, true, Shadow::Contact, H::None, H::None),
        }
    }

    /// A control: it casts its shadow only on the window, since a panel or a
    /// popover already stands off it.
    pub const fn is_control(self) -> bool {
        matches!(
            self,
            Self::Row
                | Self::Tab
                | Self::Chip
                | Self::Button
                | Self::IconButton
                | Self::Field
                | Self::Badge
                | Self::Keycap
        )
    }

    /// Floats over the window, so its fill steps off the window's color
    /// whatever it is placed on.
    pub const fn floats(self) -> bool {
        matches!(
            self,
            Self::Popover | Self::Dialog | Self::Sheet | Self::Toast | Self::Tooltip
        )
    }

    /// How far its fill stands off the window.
    pub const fn elevation(self) -> Elevation {
        if self.floats() {
            Elevation::Popover
        } else if self.is_control() || matches!(self, Self::Bar | Self::Rule) {
            Elevation::Window
        } else {
            Elevation::Panel
        }
    }
}

/// One distinct piece of UI a theme can style: a kind, and the built-in
/// changes this piece makes to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceRole {
    /// What a theme file calls it: the kind's name for a kind, or a dotted
    /// name for a place, like `sidebar.row`.
    pub id: &'static str,
    pub kind: Kind,
    /// What the theme creator calls it.
    pub label: &'static str,
    /// Where it shows up, in plain words.
    pub about: &'static str,
    /// How it differs from its kind before any theme speaks.
    pub patch: StylePatch,
}

impl SurfaceRole {
    pub const fn new(
        id: &'static str,
        kind: Kind,
        label: &'static str,
        about: &'static str,
    ) -> Self {
        Self {
            id,
            kind,
            label,
            about,
            patch: StylePatch::EMPTY,
        }
    }

    pub const fn with(self, patch: StylePatch) -> Self {
        Self { patch, ..self }
    }

    /// Whether this role is its kind, not a place within it.
    pub fn is_kind(&self) -> bool {
        self.id == self.kind.name()
    }

    /// A button that shows no fill until pointed at.
    pub const QUIET_BUTTON: Self = Self::new(
        "button.quiet",
        Kind::Button,
        "Quiet buttons",
        "Buttons with no fill until pointed at: Cancel, Skip, text actions",
    )
    .with(
        StylePatch::EMPTY
            .fill(Fill::None)
            .outline(false)
            .shadow(Shadow::None)
            .hover(Highlight::Lift)
            .selected(Highlight::Lift),
    );

    /// The roles the kit itself names: every kind, and quiet buttons.
    pub const KIT: [Self; 20] = [
        Kind::Bar.role(),
        Kind::Panel.role(),
        Kind::Pane.role(),
        Kind::Card.role(),
        Kind::Tile.role(),
        Kind::Row.role(),
        Kind::Tab.role(),
        Kind::Chip.role(),
        Kind::Button.role(),
        Self::QUIET_BUTTON,
        Kind::IconButton.role(),
        Kind::Field.role(),
        Kind::Badge.role(),
        Kind::Keycap.role(),
        Kind::Popover.role(),
        Kind::Dialog.role(),
        Kind::Sheet.role(),
        Kind::Toast.role(),
        Kind::Tooltip.role(),
        Kind::Rule.role(),
    ];
}

impl From<Kind> for SurfaceRole {
    fn from(kind: Kind) -> Self {
        kind.role()
    }
}

/// One of the theme's shadows, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowMaterial {
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    /// How dark it is: 1 is the theme's shadow color as the mode sets it,
    /// 0 casts nothing.
    pub strength: f32,
}

impl ShadowMaterial {
    pub const CONTACT: Self = Self {
        y: 1.0,
        blur: 2.0,
        spread: 0.0,
        strength: 1.0,
    };
    pub const FLOAT: Self = Self {
        y: 12.0,
        blur: 32.0,
        spread: -4.0,
        strength: 1.0,
    };
}

impl Default for ShadowMaterial {
    fn default() -> Self {
        Self::CONTACT
    }
}

/// How far a shadow's ink goes on the contact shadow: the theme's shadow
/// color at this alpha, times the material's strength.
const CONTACT_ALPHA: f32 = 0.2;
/// The width of an accent ring around a chosen tile.
const RING_WIDTH: Pixels = px(2.0);
/// The width of an accent bar along a selected row.
const BAR_WIDTH: Pixels = px(3.0);
/// How strongly a control's outline shows its ink.
const CONTROL_EDGE_ALPHA: f32 = 0.1;
const DISABLED: f32 = 0.5;
/// The marker ring of the role a theme editor inspects: a magenta no theme
/// uses, so it reads on every surface. It runs just inside the edge, so a
/// translucent fill never shows it through.
const INSPECT_COLOR: Hsla = Hsla {
    h: 0.833,
    s: 1.0,
    l: 0.55,
    a: 1.0,
};

/// A role drawn in a tone, on a ground.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub role: SurfaceRole,
    pub tone: Tone,
    /// Filled in the tone: the one action to take here.
    pub prominent: bool,
    /// What it sits on; the window when unset.
    pub ground: Option<Ground>,
}

impl Surface {
    pub const fn new(role: SurfaceRole) -> Self {
        Self {
            role,
            tone: Tone::Neutral,
            prominent: false,
            ground: None,
        }
    }

    pub const fn tone(self, tone: Tone) -> Self {
        Self { tone, ..self }
    }

    pub const fn prominent(self, prominent: bool) -> Self {
        Self { prominent, ..self }
    }

    pub const fn on(self, ground: Ground) -> Self {
        Self {
            ground: Some(ground),
            ..self
        }
    }

    pub fn on_maybe(self, ground: Option<Ground>) -> Self {
        Self {
            ground: ground.or(self.ground),
            ..self
        }
    }
}

impl From<SurfaceRole> for Surface {
    fn from(role: SurfaceRole) -> Self {
        Self::new(role)
    }
}

impl From<Kind> for Surface {
    fn from(kind: Kind) -> Self {
        Self::new(kind.role())
    }
}

/// Everything a surface paints in one state.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub fill: Hsla,
    pub glass: Option<GlassMaterial>,
    pub border: Hsla,
    /// The same in every state of a role, so a highlight never moves what
    /// is inside it.
    pub border_width: Pixels,
    pub shadow: Vec<BoxShadow>,
    pub text: Hsla,
    pub radius: Pixels,
    /// What content inside it paints on.
    pub ground: Ground,
}

impl Layer {
    pub(crate) fn muted_text(&self) -> Hsla {
        self.text
            .mix_oklab(self.ground.fill, 1.0 - super::color::muted_weight())
    }
}

impl Theme {
    /// The style `role` resolves to in this theme.
    pub fn style(&self, role: SurfaceRole) -> Style {
        let mut style = role.kind.base();
        if let Some(patch) = self.styles.get(role.kind.name()) {
            patch.apply(&mut style);
        }
        role.patch.apply(&mut style);
        if !role.is_kind()
            && let Some(patch) = self.styles.get(role.id)
        {
            patch.apply(&mut style);
        }
        style
    }

    /// The corner radius `shape` gives a surface of `kind`.
    pub fn shape_radius(&self, kind: Kind, shape: Shape) -> Pixels {
        match shape {
            Shape::Square => px(0.0),
            Shape::Pill => FULL_CORNER_RADIUS,
            Shape::Round if kind == Kind::Row => {
                if self.inset_rows() {
                    self.menu_radius()
                } else {
                    px(0.0)
                }
            }
            Shape::Round => self.radius,
        }
    }

    /// The theme's contact shadow.
    pub fn contact_shadows(&self) -> Vec<BoxShadow> {
        let material = self.contact_shadow;
        if material.strength <= 0.0 {
            return Vec::new();
        }
        vec![shadow_of(
            material,
            self.shadow_color
                .alpha((CONTACT_ALPHA * material.strength).min(1.0)),
        )]
    }

    /// The theme's floating shadow: the contact shadow under a deep one.
    pub fn float_shadows(&self) -> Vec<BoxShadow> {
        let material = self.float_shadow;
        let mut shadows = self.contact_shadows();
        if material.strength > 0.0 {
            let ink = self.shadow_color;
            shadows.push(shadow_of(
                material,
                ink.alpha((ink.a * material.strength).min(1.0)),
            ));
        }
        shadows
    }

    /// The shadows `shadow` names.
    pub fn shadows_of(&self, shadow: Shadow) -> Vec<BoxShadow> {
        match shadow {
            Shadow::None => Vec::new(),
            Shadow::Contact => self.contact_shadows(),
            Shadow::Float => self.float_shadows(),
        }
    }

    /// The theme's glass when it draws more than a plain fill would.
    fn drawn_glass(&self) -> Option<GlassMaterial> {
        self.glass.filter(|material| !material.is_opaque_fill())
    }

    /// How `surface` paints in `state`.
    pub fn layer(&self, surface: impl Into<Surface>, state: PaintState) -> Layer {
        let surface = surface.into();
        let kind = surface.role.kind;
        let style = self.style(surface.role);
        let ground = surface.ground.unwrap_or_else(|| self.ground());
        let glass = match style.fill {
            Fill::Glass if !ground.glass => self.drawn_glass(),
            _ => None,
        };
        let fill = match style.fill {
            Fill::Glass if ground.glass => Fill::Tint,
            Fill::Glass if glass.is_none() => Fill::Solid,
            fill => fill,
        };
        let tone = if surface.prominent && surface.tone == Tone::Neutral {
            Tone::Primary
        } else {
            surface.tone
        };

        let soft = self.paint_on(ground, tone, Emphasis::Soft);
        let paint = if surface.prominent && fill != Fill::Glass {
            self.paint_on(ground, tone, Emphasis::Solid)
        } else if kind.is_control() {
            match fill {
                Fill::None => self.paint_on(ground, tone, Emphasis::Ghost),
                Fill::Tint => soft,
                Fill::Solid | Fill::Glass => soft.opaque(),
            }
        } else {
            let base = if kind.floats() {
                self.background.opaque()
            } else {
                ground.fill.opaque()
            };
            let step = match kind.elevation() {
                Elevation::Popover => 2,
                _ => 1,
            };
            let rest = match fill {
                Fill::None => self.transparent,
                Fill::Tint => base.washed(step),
                Fill::Solid | Fill::Glass => base.raised(step),
            };
            let over = |level: u8| rest.blend(base.washed(level));
            super::Paint {
                fill: rest,
                hover: over(1),
                active: over(2),
                current: over(2),
                ..self.paint_on(ground, tone, Emphasis::Ghost)
            }
        };

        let highlight = match state {
            PaintState::Hover | PaintState::Active => style.hover,
            PaintState::Selected | PaintState::Current => style.selected,
            _ => Highlight::None,
        };
        let washed = |paint: &super::Paint| match state {
            PaintState::Hover => paint.hover,
            PaintState::Active => paint.active,
            _ => paint.current,
        };
        let (mut fill_color, mut text) = match highlight {
            Highlight::Tint | Highlight::Lift => (washed(&paint), paint.text),
            Highlight::Accent => (paint.selected, paint.selected_text),
            Highlight::None | Highlight::Ring | Highlight::Bar => (paint.fill, paint.text),
        };
        if state == PaintState::Disabled {
            fill_color = paint.fill.opacity(DISABLED);
            text = paint.text_at(PaintState::Disabled);
        }

        let edge = if kind.is_control() {
            if soft.border_width > px(0.0) && tone != Tone::Neutral {
                soft.border
            } else {
                ground.ink.opacity(CONTROL_EDGE_ALPHA)
            }
        } else {
            self.border()
        };
        let lifts = style.hover == Highlight::Lift || style.selected == Highlight::Lift;
        let border_width = if (style.outline || lifts) && self.outlined() {
            self.outline
        } else {
            px(0.0)
        };
        let lifted = highlight == Highlight::Lift && state != PaintState::Disabled;
        let border = if glass.is_some() || border_width == px(0.0) {
            self.transparent
        } else if style.outline || lifted {
            match state {
                PaintState::Disabled => edge.opacity(DISABLED),
                _ => edge,
            }
        } else {
            self.transparent
        };

        let casts = !kind.is_control() || ground.elevation == Elevation::Window;
        let mut shadow = if casts && state != PaintState::Disabled {
            let mut shadow = self.shadows_of(style.shadow);
            if lifted && shadow.is_empty() {
                shadow = self.contact_shadows();
            }
            shadow
        } else {
            Vec::new()
        };
        if !shadow.is_empty() && glass.is_none() && !ground.glass {
            fill_color = ground.fill.opaque().blend(fill_color);
        }
        match highlight {
            Highlight::Ring => shadow.push(BoxShadow {
                color: self.accent.opaque(),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: RING_WIDTH,
                inset: false,
            }),
            Highlight::Bar => shadow.push(BoxShadow {
                color: self.accent.opaque(),
                offset: if kind == Kind::Tab {
                    point(px(0.0), -BAR_WIDTH)
                } else {
                    point(BAR_WIDTH, px(0.0))
                },
                blur_radius: px(0.0),
                spread_radius: px(0.0),
                inset: true,
            }),
            _ => {}
        }

        if self.inspect.as_deref() == Some(surface.role.id) {
            shadow.push(BoxShadow {
                color: INSPECT_COLOR,
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: RING_WIDTH,
                inset: true,
            });
        }
        let glass = glass.map(|material| {
            let tint = if surface.prominent {
                self.tone(tone).opaque()
            } else {
                fill_color.opaque()
            };
            material
                .tint(tint.alpha(material.tint.a))
                .fallback(tint)
        });
        let under = ground.fill.opaque().blend(fill_color);
        Layer {
            fill: fill_color,
            border,
            border_width,
            shadow,
            text,
            radius: self.shape_radius(kind, style.shape),
            ground: Ground {
                fill: under,
                ink: text,
                elevation: if fill == Fill::None {
                    ground.elevation
                } else {
                    ground.elevation.max(kind.elevation())
                },
                glass: ground.glass || glass.is_some(),
            },
            glass,
        }
    }

    /// What content inside `surface` paints on while it rests.
    pub fn ground_of(&self, surface: impl Into<Surface>) -> Ground {
        self.layer(surface, PaintState::Rest).ground
    }
}

fn shadow_of(material: ShadowMaterial, color: Hsla) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.0), px(material.y)),
        blur_radius: px(material.blur.max(0.0)),
        spread_radius: px(material.spread),
        inset: false,
    }
}

/// Which edge of a box a rule runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Paints a surface that follows the pointer.
pub trait InteractiveSurfaceExt: InteractiveElement + Styled + Sized {
    /// `surface` in `state`, taking its hover highlight while pointed at
    /// when it rests.
    fn surface_hover(self, surface: impl Into<Surface>, state: PaintState, cx: &App) -> Self {
        let surface = surface.into();
        let theme = cx.theme();
        let this = super::StyledExt::surface_at(self, surface, state, cx);
        if !matches!(state, PaintState::Rest | PaintState::Focused) {
            return this;
        }
        let hover = theme.layer(surface, PaintState::Hover);
        this.hover(move |style| restyle(style, &hover))
    }
}

impl<E: InteractiveElement + Styled> InteractiveSurfaceExt for E {}

/// Paints a surface that follows the pointer and presses, on an element with
/// an id.
pub trait StatefulSurfaceExt: StatefulInteractiveElement + Styled + Sized {
    /// `surface` in `state`, with its hover and pressed highlights when it
    /// rests.
    fn surface_interactive(self, surface: impl Into<Surface>, state: PaintState, cx: &App) -> Self {
        let surface = surface.into();
        let theme = cx.theme();
        let this = self.surface_hover(surface, state, cx);
        if !matches!(state, PaintState::Rest | PaintState::Focused) {
            return this;
        }
        let active = theme.layer(surface, PaintState::Active);
        this.active(move |style| restyle(style, &active))
    }
}

impl<E: StatefulInteractiveElement + Styled> StatefulSurfaceExt for E {}

pub(crate) fn restyle(style: zz_gpui::StyleRefinement, layer: &Layer) -> zz_gpui::StyleRefinement {
    let style = style
        .border_color(layer.border)
        .shadow(layer.shadow.clone())
        .text_color(layer.text);
    match layer.glass {
        Some(_) => style,
        None => style.bg(layer.fill),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeColor;

    fn theme() -> Theme {
        Theme::from(&*ThemeColor::dark())
    }

    #[test]
    fn a_role_takes_its_kind_then_the_theme_then_itself() {
        let mut theme = theme();
        let role = SurfaceRole::new("test.row", Kind::Row, "Test", "")
            .with(StylePatch::EMPTY.selected(Highlight::Tint));
        assert_eq!(theme.style(role).selected, Highlight::Tint);
        assert_eq!(theme.style(role).hover, Kind::Row.base().hover);

        theme.styles.insert(
            "row".into(),
            StylePatch::EMPTY
                .hover(Highlight::Accent)
                .selected(Highlight::Bar),
        );
        assert_eq!(theme.style(role).hover, Highlight::Accent);
        assert_eq!(theme.style(role).selected, Highlight::Tint);

        theme.styles.insert(
            "test.row".into(),
            StylePatch::EMPTY.selected(Highlight::Ring),
        );
        assert_eq!(theme.style(role).selected, Highlight::Ring);
        assert_eq!(theme.style(Kind::Row.role()).selected, Highlight::Bar);
    }

    #[test]
    fn glass_paints_a_tint_on_glass_and_solid_without_the_themes_glass() {
        let mut theme = theme();
        let popover = Kind::Popover.role();
        theme.glass = None;
        let plain = theme.layer(popover, PaintState::Rest);
        assert!(plain.glass.is_none());
        assert_eq!(plain.fill.a, 1.0);

        theme.glass = Some(GlassMaterial::frosted());
        let frosted = theme.layer(popover, PaintState::Rest);
        assert!(frosted.glass.is_some());
        assert!(frosted.ground.glass);

        let nested = theme.layer(Surface::new(popover).on(frosted.ground), PaintState::Rest);
        assert!(nested.glass.is_none());
        assert!(nested.fill.a < 1.0);
    }

    #[test]
    fn controls_cast_shadows_only_on_the_window() {
        let theme = theme();
        let button = Kind::Button.role();
        assert!(!theme.layer(button, PaintState::Rest).shadow.is_empty());
        let panel = theme.surface(Elevation::Panel);
        assert!(
            theme
                .layer(Surface::new(button).on(panel), PaintState::Rest)
                .shadow
                .is_empty()
        );
        assert!(
            !theme
                .layer(Surface::new(Kind::Card.role()).on(panel), PaintState::Rest)
                .shadow
                .is_empty()
        );
    }

    #[test]
    fn highlights_never_change_the_border_width() {
        let theme = theme();
        for role in SurfaceRole::KIT {
            let widths = PaintState::ALL
                .map(|state| theme.layer(role, state).border_width);
            assert!(
                widths.iter().all(|width| *width == widths[0]),
                "{} moves its border: {widths:?}",
                role.id
            );
        }
    }

    #[test]
    fn the_inspected_role_rings_every_surface_it_names() {
        let mut theme = theme();
        let role = Kind::Card.role();
        let rings = |theme: &Theme| {
            theme
                .layer(role, PaintState::Rest)
                .shadow
                .iter()
                .filter(|shadow| shadow.color == INSPECT_COLOR)
                .count()
        };
        assert_eq!(rings(&theme), 0);
        theme.inspect = Some("card".into());
        assert_eq!(rings(&theme), 1);
        theme.inspect = Some("row".into());
        assert_eq!(rings(&theme), 0);
    }

    #[test]
    fn a_patch_between_two_styles_names_only_what_differs() {
        let from = Kind::Button.base();
        let to = Style {
            fill: Fill::Glass,
            shape: Shape::Pill,
            ..from
        };
        let patch = StylePatch::between(from, to);
        assert_eq!(
            patch,
            StylePatch::EMPTY.fill(Fill::Glass).shape(Shape::Pill)
        );
        let mut style = from;
        patch.apply(&mut style);
        assert_eq!(style, to);
    }

    #[test]
    fn theme_files_spell_styles_in_lowercase() {
        let patch: StylePatch = serde_json::from_str(
            r#"{"fill":"glass","outline":false,"shadow":"float","shape":"pill","hover":"lift","selected":"accent"}"#,
        )
        .unwrap();
        assert_eq!(
            patch,
            StylePatch::EMPTY
                .fill(Fill::Glass)
                .outline(false)
                .shadow(Shadow::Float)
                .shape(Shape::Pill)
                .hover(Highlight::Lift)
                .selected(Highlight::Accent)
        );
        assert!(serde_json::from_str::<StylePatch>(r#"{"fil":"glass"}"#).is_err());
    }
}
