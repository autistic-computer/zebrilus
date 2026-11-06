use std::cell::RefCell;
use std::collections::BTreeSet;

use ala::{
    App, BoxShadow, EdgeMark, FULL_CORNER_RADIUS, GlassMaterial, Hsla, InteractiveElement, Pixels,
    StatefulInteractiveElement, Styled, point, px,
};
use serde::{Deserialize, Serialize};

use super::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, Ground, PaintState, PrimaryStyle, Theme,
    Tone,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fill {
    None,
    Tint,
    Solid,
    Glass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shadow {
    None,
    Contact,
    Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Square,
    Round,
    Pill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "f32", into = "f32")]
pub struct Smoothing(u8);

impl Smoothing {
    pub const CIRCLE: Self = Self(20);
    pub const THEME: Self = Self(30);
    pub const MIN: f32 = 2.0;
    pub const MAX: f32 = 5.0;

    pub fn exponent(self) -> f32 {
        f32::from(self.0) / 10.0
    }

    pub fn clamped(exponent: f32) -> Self {
        let exponent = if exponent.is_finite() {
            exponent.clamp(Self::MIN, Self::MAX)
        } else {
            Self::THEME.exponent()
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self((exponent * 10.0).round() as u8)
    }
}

impl From<Smoothing> for f32 {
    fn from(smoothing: Smoothing) -> Self {
        smoothing.exponent()
    }
}

impl TryFrom<f32> for Smoothing {
    type Error = String;

    fn try_from(exponent: f32) -> Result<Self, Self::Error> {
        if (2.0..=10.0).contains(&exponent) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Ok(Self((exponent * 10.0).round() as u8))
        } else {
            Err(format!("smoothing must be from 2 to 10, not {exponent}"))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Highlight {
    None,
    Tint,
    Accent,
    Ring,
    Bar,
    Glass,
    Edge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ink {
    Bright,
    Rest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    pub fill: Fill,
    pub outline: bool,
    pub shadow: Shadow,
    pub shape: Shape,
    pub smoothing: Smoothing,
    pub hover: Highlight,
    pub selected: Highlight,
    pub ink: Ink,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StylePatch {
    pub fill: Option<Fill>,
    pub outline: Option<bool>,
    pub shadow: Option<Shadow>,
    pub shape: Option<Shape>,
    pub smoothing: Option<Smoothing>,
    pub hover: Option<Highlight>,
    pub selected: Option<Highlight>,
    pub ink: Option<Ink>,
}

impl StylePatch {
    pub const EMPTY: Self = Self {
        fill: None,
        outline: None,
        shadow: None,
        shape: None,
        smoothing: None,
        hover: None,
        selected: None,
        ink: None,
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

    pub const fn smoothing(self, smoothing: Smoothing) -> Self {
        Self {
            smoothing: Some(smoothing),
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

    pub const fn ink(self, ink: Ink) -> Self {
        Self {
            ink: Some(ink),
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
        if let Some(smoothing) = self.smoothing {
            style.smoothing = smoothing;
        }
        if let Some(hover) = self.hover {
            style.hover = hover;
        }
        if let Some(selected) = self.selected {
            style.selected = selected;
        }
        if let Some(ink) = self.ink {
            style.ink = ink;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Bar,
    Panel,
    Pane,
    Card,
    Floating,
    Button,
    Item,
    Field,
    Label,
    Rule,
}

impl Kind {
    pub const ALL: [Self; 10] = [
        Self::Bar,
        Self::Panel,
        Self::Pane,
        Self::Card,
        Self::Floating,
        Self::Button,
        Self::Item,
        Self::Field,
        Self::Label,
        Self::Rule,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Panel => "panel",
            Self::Pane => "pane",
            Self::Card => "card",
            Self::Floating => "floating",
            Self::Button => "button",
            Self::Item => "item",
            Self::Field => "field",
            Self::Label => "label",
            Self::Rule => "rule",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Bar => "Bars",
            Self::Panel => "Panels",
            Self::Pane => "Panes",
            Self::Card => "Cards",
            Self::Floating => "Floating",
            Self::Button => "Buttons",
            Self::Item => "Items",
            Self::Field => "Fields",
            Self::Label => "Labels",
            Self::Rule => "Dividers",
        }
    }

    const fn about(self) -> &'static str {
        match self {
            Self::Bar => {
                "Strips across a window or sheet: the title bar, sheet headers, the phone bar"
            }
            Self::Panel => "Columns set into the window, like the sidebar",
            Self::Pane => "Content panes and split views",
            Self::Card => {
                "Boxes in the flow: message bubbles, setting groups, code blocks, previews"
            }
            Self::Floating => {
                "Everything over the content: menus, palettes, dialogs, sheets, toasts and tooltips"
            }
            Self::Button => {
                "Everything you press to do something: buttons, icon buttons, and the rows of menus, palettes and pickers"
            }
            Self::Item => {
                "What you pick that stays picked or open: sidebar rows, tabs, list steps, picker tiles"
            }
            Self::Field => {
                "Text fields, selects and anything you type into. Selected is the one you are typing in"
            }
            Self::Label => "Badges, key hints and other small marks",
            Self::Rule => "Divider lines",
        }
    }

    pub const fn role(self) -> SurfaceRole {
        SurfaceRole::new(self.name(), self, self.label(), self.about())
    }

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
                smoothing: Smoothing::THEME,
                hover,
                selected,
                ink: Ink::Bright,
            }
        }
        use Highlight as H;
        match self {
            Self::Bar => Style {
                shape: Shape::Square,
                ..style(Fill::None, false, Shadow::None, H::None, H::None)
            },
            Self::Panel => Style {
                shape: Shape::Square,
                ..style(Fill::Solid, false, Shadow::None, H::None, H::None)
            },
            Self::Rule => Style {
                shape: Shape::Square,
                ..style(Fill::None, true, Shadow::None, H::None, H::None)
            },
            Self::Pane => style(Fill::Glass, false, Shadow::None, H::None, H::Ring),
            Self::Card => style(Fill::Solid, false, Shadow::None, H::None, H::Ring),
            Self::Floating => style(Fill::Glass, false, Shadow::Float, H::None, H::None),
            Self::Button => style(Fill::Tint, false, Shadow::None, H::Tint, H::Tint),
            Self::Item => style(Fill::None, false, Shadow::None, H::Tint, H::Accent),
            Self::Field => style(Fill::Tint, false, Shadow::None, H::Tint, H::Edge),
            Self::Label => style(Fill::Tint, false, Shadow::None, H::None, H::None),
        }
    }

    pub const fn is_control(self) -> bool {
        matches!(self, Self::Button | Self::Item | Self::Field | Self::Label)
    }

    pub const fn floats(self) -> bool {
        matches!(self, Self::Floating)
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurfaceRole {
    pub(crate) id: &'static str,
    pub(crate) kind: Kind,
    pub(crate) parent: Option<&'static SurfaceRole>,
    pub(crate) label: &'static str,
    pub(crate) about: &'static str,
    pub(crate) patch: StylePatch,
    pub(crate) row: bool,
    pub(crate) capsule: Option<bool>,
}

impl SurfaceRole {
    pub(crate) const fn new(
        id: &'static str,
        kind: Kind,
        label: &'static str,
        about: &'static str,
    ) -> Self {
        Self {
            id,
            kind,
            parent: None,
            label,
            about,
            patch: StylePatch::EMPTY,
            row: false,
            capsule: None,
        }
    }

    pub(crate) const fn under(
        parent: &'static SurfaceRole,
        id: &'static str,
        label: &'static str,
        about: &'static str,
    ) -> Self {
        Self {
            parent: Some(parent),
            row: parent.row,
            ..Self::new(id, parent.kind, label, about)
        }
    }

    pub(crate) const fn with(self, patch: StylePatch) -> Self {
        Self { patch, ..self }
    }

    pub(crate) const fn rows(self) -> Self {
        Self { row: true, ..self }
    }

    pub(crate) const fn capsule(self, capsule: bool) -> Self {
        Self {
            capsule: Some(capsule),
            ..self
        }
    }

    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn kind(&self) -> Kind {
        self.kind
    }

    pub const fn label(&self) -> &'static str {
        self.label
    }

    pub const fn about(&self) -> &'static str {
        self.about
    }

    pub const fn patch(&self) -> StylePatch {
        self.patch
    }

    pub const fn capsules(&self) -> bool {
        match (self.capsule, self.parent) {
            (Some(capsule), _) => capsule,
            (None, Some(parent)) => parent.capsules(),
            (None, None) => matches!(self.kind, Kind::Button | Kind::Field),
        }
    }

    pub fn is_kind(&self) -> bool {
        self.id == self.kind.name()
    }

    pub fn group(&self) -> Option<Self> {
        match self.parent {
            Some(parent) => Some(*parent),
            None if self.is_kind() => None,
            None => Some(self.kind.role()),
        }
    }

    pub fn lineage(self) -> impl Iterator<Item = Self> {
        std::iter::successors(Some(self), Self::group)
    }

    pub const POPOVER: Self = Self::new(
        "popover",
        Kind::Floating,
        "Menus and popovers",
        "Menus, dropdowns and popovers",
    );

    pub const DIALOG: Self = Self::new(
        "dialog",
        Kind::Floating,
        "Dialogs",
        "Dialogs and modal pickers",
    );

    pub const SHEET: Self = Self::new(
        "sheet",
        Kind::Floating,
        "Sheets",
        "Sheets that rise from the bottom",
    );

    pub const DRAWER: Self = Self::under(
        &Self::SHEET,
        "drawer",
        "Drawers",
        "Sheets a touch screen opens in place of a dropdown, resting above the bottom safe area",
    );

    pub const TOAST: Self = Self::new(
        "toast",
        Kind::Floating,
        "Toasts",
        "Notifications and banners",
    );

    pub const TOOLTIP: Self = Self::new(
        "tooltip",
        Kind::Floating,
        "Tooltips",
        "Tooltips and key hints",
    );

    pub const ICON_BUTTON: Self = Self::new(
        "icon-button",
        Kind::Button,
        "Icon buttons",
        "Buttons that are only an icon",
    )
    .with(
        StylePatch::EMPTY
            .fill(Fill::None)
            .selected(Highlight::Accent),
    );

    pub const QUIET_BUTTON: Self = Self::new(
        "button.quiet",
        Kind::Button,
        "Quiet buttons",
        "Buttons with no fill until pointed at: Cancel, Skip, text actions",
    )
    .with(
        StylePatch::EMPTY
            .fill(Fill::None)
            .selected(Highlight::Accent),
    );

    pub const OUTLINE_BUTTON: Self = Self::under(
        &Self::QUIET_BUTTON,
        "button.outline",
        "Outline buttons",
        "Quiet buttons drawn with an outline",
    )
    .with(StylePatch::EMPTY.fill(Fill::None).outline(true));

    pub const FLAT_ICON_BUTTON: Self = Self::under(
        &Self::ICON_BUTTON,
        "icon-button.flat",
        "Flat icon buttons",
        "Icon buttons set into a bar, a field or a row, that wash and never fill",
    )
    .with(StylePatch::EMPTY.selected(Highlight::Tint));

    pub const MENU_ITEM: Self = Self::new(
        "menu-item",
        Kind::Button,
        "Menu items",
        "Rows you press to run or pick something: menus, palettes, pickers, dropdowns",
    )
    .rows()
    .capsule(false)
    .with(
        StylePatch::EMPTY
            .fill(Fill::None)
            .selected(Highlight::Accent),
    );

    pub const ROW: Self = Self::new(
        "row",
        Kind::Item,
        "Rows",
        "Rows that show which one is open or current",
    )
    .rows()
    .capsule(true);

    pub const TAB: Self =
        Self::new("tab", Kind::Item, "Tabs", "Tabs and window pills").capsule(true);

    pub const TILE: Self = Self::new("tile", Kind::Item, "Tiles", "Pickable thumbnails").with(
        StylePatch::EMPTY
            .fill(Fill::Solid)
            .hover(Highlight::None)
            .selected(Highlight::Ring),
    );

    pub const WELL: Self = Self::new(
        "well",
        Kind::Card,
        "Wells",
        "The area things are arranged on, like screens on a desk",
    )
    .with(StylePatch::EMPTY.fill(Fill::Tint));

    pub const SCREEN_TILE: Self = Self::new(
        "tile.screen",
        Kind::Card,
        "Screen tiles",
        "A screen drawn to scale on a well",
    );

    pub const CARD_TILE: Self = Self::new(
        "tile.card",
        Kind::Card,
        "Card tiles",
        "A card for something you can place, in a dock under a well",
    );

    pub const INSPECTOR: Self = Self::under(
        &crate::foundation::roles::SIDEBAR,
        "inspector",
        "Inspectors",
        "The selected thing's details, in a column beside the content",
    );

    pub const TEXT_AREA: Self = Self::new(
        "field.multiline",
        Kind::Field,
        "Text areas",
        "Fields that hold more than one line, and framed code editors",
    )
    .with(StylePatch::EMPTY.shape(Shape::Round))
    .capsule(false);

    pub const OUTLINE_FIELD: Self = Self::new(
        "field.outline",
        Kind::Field,
        "Outline fields",
        "Fields with no fill, drawn with an outline",
    )
    .with(StylePatch::EMPTY.fill(Fill::None).outline(true));

    pub const OUTLINE_TEXT_AREA: Self = Self::under(
        &Self::TEXT_AREA,
        "field.multiline.outline",
        "Outline text areas",
        "Text areas with no fill, drawn with an outline",
    )
    .with(StylePatch::EMPTY.fill(Fill::None).outline(true));

    pub const BADGE: Self =
        Self::new("badge", Kind::Label, "Badges", "Small labels and counts").capsule(true);

    pub const OUTLINE_BADGE: Self = Self::under(
        &Self::BADGE,
        "badge.outline",
        "Outline badges",
        "Badges with no fill, drawn with an outline",
    )
    .with(StylePatch::EMPTY.fill(Fill::None).outline(true));

    pub const CHIP: Self = Self::under(
        &Self::BADGE,
        "chip",
        "Chips",
        "A name or a state with a dot or an icon, and a detail after it",
    )
    .with(
        StylePatch::EMPTY
            .shape(Shape::Pill)
            .selected(Highlight::Tint),
    );

    pub const OUTLINE_CHIP: Self = Self::under(
        &Self::CHIP,
        "chip.outline",
        "Outline chips",
        "Chips with no fill, drawn with an outline",
    )
    .with(StylePatch::EMPTY.fill(Fill::None).outline(true));

    pub const STEP: Self = Self::under(
        &Self::BADGE,
        "step",
        "Steps",
        "The numbered dots of a step strip and the lines between them",
    )
    .with(
        StylePatch::EMPTY
            .shape(Shape::Pill)
            .selected(Highlight::Accent),
    );

    pub const NOTICE: Self = Self::new(
        "notice",
        Kind::Card,
        "Notices",
        "A problem or a note above a page or in a panel, tinted by its tone",
    );

    pub const KEYCAP: Self = Self::new("keycap", Kind::Label, "Keycaps", "Key hints");

    pub const ICON_TILE: Self = Self::new(
        "icon-tile",
        Kind::Label,
        "Icon tiles",
        "The tile behind the icon that leads a sheet row, and the icon's ink",
    )
    .with(StylePatch::EMPTY.ink(Ink::Rest));

    pub const GRABBER: Self = Self::under(
        &Self::BADGE,
        "grabber",
        "Grabbers",
        "The handles on the phone bar and on sheets",
    )
    .with(StylePatch::EMPTY.shape(Shape::Pill));

    pub(crate) const OUTLINES: [Self; 5] = [
        Self::OUTLINE_BUTTON,
        Self::OUTLINE_FIELD,
        Self::OUTLINE_TEXT_AREA,
        Self::OUTLINE_BADGE,
        Self::OUTLINE_CHIP,
    ];

    pub const KIT: [Self; 40] = [
        Kind::Bar.role(),
        Kind::Panel.role(),
        Self::INSPECTOR,
        Kind::Pane.role(),
        Kind::Card.role(),
        Self::NOTICE,
        Kind::Floating.role(),
        Self::POPOVER,
        Self::DIALOG,
        Self::SHEET,
        Self::DRAWER,
        Self::TOAST,
        Self::TOOLTIP,
        Kind::Button.role(),
        Self::QUIET_BUTTON,
        Self::OUTLINE_BUTTON,
        Self::ICON_BUTTON,
        Self::FLAT_ICON_BUTTON,
        Self::MENU_ITEM,
        Kind::Item.role(),
        Self::ROW,
        Self::TAB,
        Self::TILE,
        Self::WELL,
        Self::SCREEN_TILE,
        Self::CARD_TILE,
        Kind::Field.role(),
        Self::OUTLINE_FIELD,
        Self::TEXT_AREA,
        Self::OUTLINE_TEXT_AREA,
        Kind::Label.role(),
        Self::BADGE,
        Self::OUTLINE_BADGE,
        Self::CHIP,
        Self::OUTLINE_CHIP,
        Self::STEP,
        Self::KEYCAP,
        Self::ICON_TILE,
        Self::GRABBER,
        Kind::Rule.role(),
    ];
}

impl From<Kind> for SurfaceRole {
    fn from(kind: Kind) -> Self {
        kind.role()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ShadowMaterial {
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
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
        y: 10.0,
        blur: 30.0,
        spread: -6.0,
        strength: 0.8,
    };
}

impl Default for ShadowMaterial {
    fn default() -> Self {
        Self::CONTACT
    }
}

const CONTACT_ALPHA: f32 = 0.2;
const RING_WIDTH: Pixels = px(2.0);
const BAR_WIDTH: Pixels = px(3.0);
const BAR_LENGTH: Pixels = px(16.0);
const CONTROL_EDGE_ALPHA: f32 = 0.1;
const HIGHLIGHT_STRENGTH: f32 = 0.8;
const DISABLED: f32 = 0.5;
const INSPECT_COLOR: Hsla = Hsla {
    h: 0.833,
    s: 1.0,
    l: 0.55,
    a: 1.0,
};

thread_local! {
    static DRAWN: RefCell<Option<BTreeSet<&'static str>>> = const { RefCell::new(None) };
}

pub fn note_drawn_roles() {
    DRAWN.with_borrow_mut(|drawn| *drawn = Some(BTreeSet::new()));
}

pub fn drawn_roles() -> BTreeSet<&'static str> {
    DRAWN.with_borrow_mut(|drawn| drawn.take().unwrap_or_default())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    pub(crate) role: SurfaceRole,
    pub tone: Tone,
    pub prominent: bool,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub fill: Hsla,
    pub glass: Option<GlassMaterial>,
    pub border: Hsla,
    pub border_width: Pixels,
    pub shadow: Vec<BoxShadow>,
    pub mark: Option<EdgeMark>,
    pub text: Hsla,
    pub radius: Pixels,
    pub smoothing: f32,
    pub ground: Ground,
}

impl Layer {
    pub(crate) fn muted_text(&self) -> Hsla {
        if self.ground.solid {
            return self.text;
        }
        self.text
            .mix_oklab(self.ground.fill, 1.0 - super::color::muted_weight())
    }
}

impl Theme {
    pub fn group_reaches(&self, group: SurfaceRole, role: SurfaceRole) -> bool {
        role.lineage().any(|node| node.id == group.id)
    }

    fn built_in(node: SurfaceRole, style: &mut Style) {
        if let Some(group) = node.group() {
            Self::built_in(group, style);
        }
        node.patch.apply(style);
    }

    pub fn style(&self, role: SurfaceRole) -> Style {
        DRAWN.with_borrow_mut(|drawn| {
            if let Some(drawn) = drawn {
                drawn.extend(role.lineage().map(|node| node.id));
            }
        });
        let mut style = role.kind.base();
        style.smoothing = self.smoothing;
        Self::built_in(role, &mut style);
        style
    }

    pub fn style_of(&self, surface: impl Into<Surface>) -> Style {
        self.style(surface.into().role)
    }

    pub fn shape_radius(&self, surface: impl Into<Surface>, shape: Shape) -> Pixels {
        let role = surface.into().role;
        match shape {
            Shape::Square => px(0.0),
            Shape::Pill => FULL_CORNER_RADIUS,
            Shape::Round if role.row && !self.inset_rows() => px(0.0),
            Shape::Round if role.capsules() && self.pill_controls() => FULL_CORNER_RADIUS,
            Shape::Round if role.row => self.menu_radius(),
            Shape::Round if role.kind.is_control() => self
                .control_radius()
                .min(px(super::interface_theme::MAX_CORNER_RADIUS)),
            Shape::Round => self.radius,
        }
    }

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

    pub fn lifted_shadows(&self) -> Vec<BoxShadow> {
        let strength = self.contact_shadow.strength;
        if strength <= 0.0 {
            return Vec::new();
        }
        let ink = self.shadow_color.alpha((0.1 * strength).min(1.0));
        vec![
            BoxShadow {
                color: ink,
                offset: point(px(0.), px(4.)),
                blur_radius: px(6.),
                spread_radius: px(-1.),
                inset: false,
            },
            BoxShadow {
                color: ink,
                offset: point(px(0.), px(2.)),
                blur_radius: px(4.),
                spread_radius: px(-2.),
                inset: false,
            },
        ]
    }

    pub fn float_shadows(&self) -> Vec<BoxShadow> {
        let material = self.float_shadow;
        if material.strength <= 0.0 {
            return Vec::new();
        }
        let ink = self.shadow_color;
        vec![shadow_of(
            material,
            ink.alpha((ink.a * material.strength).min(1.0)),
        )]
    }

    pub fn shadows_of(&self, shadow: Shadow) -> Vec<BoxShadow> {
        match shadow {
            Shadow::None => Vec::new(),
            Shadow::Contact => self.contact_shadows(),
            Shadow::Float => self.float_shadows(),
        }
    }

    fn drawn_glass(&self) -> Option<GlassMaterial> {
        self.glass.filter(|material| !material.is_opaque_fill())
    }

    pub fn layer(&self, surface: impl Into<Surface>, state: PaintState) -> Layer {
        let surface = surface.into();
        let state = match state {
            PaintState::Focused if crate::components::button::rings_focus(surface.role) => {
                PaintState::Rest
            }
            state => state,
        };
        let kind = surface.role.kind;
        let style = self.style(surface.role);
        let ground = surface.ground.unwrap_or_else(|| self.ground());
        let tone = if surface.prominent && surface.tone == Tone::Neutral {
            Tone::Primary
        } else if !surface.prominent && ground.solid {
            Tone::Neutral
        } else {
            surface.tone
        };
        let opaque = tone != Tone::Neutral
            && super::roles::FLOATING_CHIPS
                .iter()
                .any(|chip| chip.id == surface.role.id);
        let pane = kind == Kind::Pane;
        let mut glass = match style.fill {
            Fill::Glass if pane && self.pane_margin <= px(0.0) => None,
            Fill::Glass if !ground.glass && !opaque => self.drawn_glass(),
            _ => None,
        };
        let fill = match style.fill {
            Fill::Glass if pane => Fill::None,
            Fill::Glass if opaque => Fill::Solid,
            Fill::Glass if ground.glass => Fill::Tint,
            Fill::Glass if glass.is_none() => Fill::Solid,
            fill => fill,
        };

        let soft = self.paint_on(ground, tone, Emphasis::Soft);
        let solid = self.paint_on(ground, tone, Emphasis::Solid);
        let base = if kind.floats() {
            self.background.opaque()
        } else {
            ground.fill.opaque()
        };
        let see_through = !kind.floats() && ground.fill.a < 1.0;
        let step = match kind.elevation() {
            Elevation::Popover => 2,
            _ => 1,
        };
        let toned = !(tone == Tone::Neutral
            || (tone == Tone::Primary && self.primary == PrimaryStyle::Foreground));
        let glass_fill = soft.opaque().fill;
        let paint = if surface.prominent && fill != Fill::Glass {
            solid
        } else if kind.is_control() || opaque {
            match fill {
                Fill::None => self.paint_on(ground, tone, Emphasis::Ghost),
                Fill::Tint => soft,
                Fill::Solid => soft.opaque(),
                Fill::Glass => super::Paint {
                    fill: glass_fill,
                    ..soft.opaque()
                },
            }
        } else {
            let rest = match fill {
                Fill::None => self.transparent,
                Fill::Tint if toned => soft.fill,
                Fill::Tint => base.washed(step),
                Fill::Solid | Fill::Glass if see_through => base.washed(step),
                Fill::Solid | Fill::Glass => self.raise(base, step),
            };
            let over = |level: u8| match fill {
                Fill::None => base.washed(level),
                Fill::Tint => base.washed(step + level),
                Fill::Solid | Fill::Glass if see_through => base.washed(step + level),
                Fill::Solid | Fill::Glass => rest.blend(base.washed(level)),
            };
            super::Paint {
                fill: rest,
                hover: over(1),
                active: over(2),
                current: over(2),
                ..self.paint_on(ground, tone, Emphasis::Ghost)
            }
        };
        let paint = if surface.prominent {
            super::Paint {
                text: solid.text,
                selected_text: solid.selected_text,
                ..paint
            }
        } else {
            paint
        };

        let highlight = match state {
            PaintState::Hover | PaintState::Active => style.hover,
            PaintState::Selected | PaintState::Current => style.selected,
            PaintState::Focused if kind == Kind::Field => style.selected,
            _ => Highlight::None,
        };
        let highlight = match highlight {
            Highlight::Glass if glass.is_some() || ground.glass || self.drawn_glass().is_none() => {
                Highlight::Accent
            }
            highlight => highlight,
        };
        let highlight = match highlight {
            Highlight::Accent if tone == Tone::Neutral && !self.tinted() => Highlight::Tint,
            highlight => highlight,
        };
        if highlight == Highlight::Glass {
            glass = self.drawn_glass();
        }
        let washed = |paint: &super::Paint| match state {
            PaintState::Hover => paint.hover,
            PaintState::Active => paint.active,
            _ => paint.current,
        };
        let lifted = base.blend(super::color::toward(
            paint.fill,
            washed(&paint),
            HIGHLIGHT_STRENGTH,
        ));
        let (mut fill_color, mut text) = match highlight {
            Highlight::Tint => (
                super::color::toward(paint.fill, washed(&paint), HIGHLIGHT_STRENGTH),
                paint.text,
            ),
            Highlight::Bar if surface.role.row => (
                super::color::toward(paint.fill, washed(&paint), HIGHLIGHT_STRENGTH),
                paint.text,
            ),
            Highlight::Accent => {
                let lit = if tone == Tone::Neutral {
                    Tone::Primary
                } else {
                    tone
                };
                let solid = self.paint_on(ground, lit, Emphasis::Solid);
                (solid.fill, solid.text)
            }
            Highlight::Glass => {
                let opacity = glass.map_or(1.0, |material| material.tint.a).max(0.01);
                (
                    super::color::toward(base, lifted, 1.0 / opacity),
                    paint.text,
                )
            }
            Highlight::None | Highlight::Ring | Highlight::Bar | Highlight::Edge => {
                (paint.fill, paint.text)
            }
        };
        if state == PaintState::Disabled {
            fill_color = paint.fill.opacity(DISABLED);
            text = paint.text_at(PaintState::Disabled);
        } else if state == PaintState::Rest
            && style.ink == Ink::Rest
            && tone == Tone::Neutral
            && !surface.prominent
        {
            text = text.rest();
        }

        let pip = surface.role.id == super::roles::FLOATING_PIP.id;
        let rimless =
            (pane && self.pane_margin > px(0.0) || pip) && !glass.as_ref().is_some_and(draws_rim);
        let edge = if kind.is_control() {
            if soft.border_width > px(0.0) && tone != Tone::Neutral {
                soft.border
            } else {
                ground.ink.opacity(CONTROL_EDGE_ALPHA)
            }
        } else if rimless {
            self.divider_color()
        } else {
            self.border()
        };
        let edged = rimless;
        let border_width = if style.outline || edged {
            self.outline
        } else {
            px(0.0)
        };
        let border = if border_width == px(0.0) || (glass.is_some() && !edged) {
            self.transparent
        } else {
            match state {
                PaintState::Disabled => edge.opacity(DISABLED),
                _ => edge,
            }
        };

        let (border, border_width) = if SurfaceRole::OUTLINES
            .iter()
            .any(|role| role.id == surface.role.id)
            && glass.is_none()
        {
            let paint = self.paint_on(ground, tone, Emphasis::Outline);
            let edge = match state {
                PaintState::Disabled => paint.border.opacity(DISABLED),
                _ => paint.border,
            };
            (edge, paint.border_width)
        } else {
            (border, border_width)
        };
        let casts = !kind.is_control() || ground.elevation == Elevation::Window;
        let mut shadow = if casts && state != PaintState::Disabled {
            self.shadows_of(style.shadow)
        } else {
            Vec::new()
        };
        let lit = self.tone(Tone::Primary).opaque();
        match highlight {
            Highlight::Ring => shadow.push(BoxShadow {
                color: lit,
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: RING_WIDTH,
                inset: false,
            }),
            Highlight::Bar if !surface.role.row => shadow.push(BoxShadow {
                color: lit,
                offset: point(px(0.0), -BAR_WIDTH),
                blur_radius: px(0.0),
                spread_radius: px(0.0),
                inset: true,
            }),
            _ => {}
        }
        if shadow.iter().any(|shadow| !shadow.inset)
            && glass.is_none()
            && !ground.glass
            && !see_through
        {
            fill_color = ground.fill.opaque().blend(fill_color);
        }

        if self.inspect_strength > 0.0
            && self
                .inspect
                .as_deref()
                .is_some_and(|inspect| surface.role.lineage().any(|node| inspect == node.id))
        {
            shadow.push(BoxShadow {
                color: INSPECT_COLOR.alpha(self.inspect_strength.min(1.0)),
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(0.0),
                spread_radius: RING_WIDTH,
                inset: true,
            });
        }
        let glass = glass.map(|material| {
            if pane {
                let tint = if see_through {
                    self.transparent
                } else {
                    ground.fill
                };
                return material
                    .blur(px(0.0))
                    .refraction(px(0.0))
                    .dispersion(0.0)
                    .tint(tint)
                    .fallback(tint);
            }
            let tint = if surface.prominent {
                self.tone(tone).opaque()
            } else {
                fill_color.opaque()
            };
            let fallback = if highlight == Highlight::Glass {
                lifted
            } else {
                tint
            };
            material
                .tint(tint.alpha(material.tint.a))
                .fallback(fallback)
        });
        let mark =
            (highlight == Highlight::Bar && surface.role.row && state != PaintState::Disabled)
                .then(|| EdgeMark {
                    color: lit,
                    width: BAR_WIDTH,
                    length: BAR_LENGTH,
                });
        let shown = if highlight == Highlight::Glass {
            lifted
        } else {
            fill_color
        };
        let under = if see_through {
            ground.fill.blend(shown)
        } else {
            ground.fill.opaque().blend(shown)
        };
        let solid_fill =
            highlight == Highlight::Accent || (surface.prominent && fill != Fill::Glass);
        let radius = self.shape_radius(surface, style.shape);
        Layer {
            fill: fill_color,
            mark,
            border,
            border_width,
            shadow,
            text,
            radius,
            smoothing: if radius >= FULL_CORNER_RADIUS {
                Smoothing::CIRCLE.exponent()
            } else if surface.role.row {
                style.smoothing.exponent().min(super::ROW_CORNER_SMOOTHING)
            } else {
                style.smoothing.exponent()
            },
            ground: Ground {
                fill: under,
                ink: text,
                elevation: if fill == Fill::None {
                    ground.elevation
                } else {
                    ground.elevation.max(kind.elevation())
                },
                glass: ground.glass || (glass.is_some() && !pane),
                solid: ground.solid || solid_fill,
            },
            glass,
        }
    }

    pub fn ground_of(&self, surface: impl Into<Surface>) -> Ground {
        self.ground_at(surface, PaintState::Rest)
    }

    pub fn ground_at(&self, surface: impl Into<Surface>, state: PaintState) -> Ground {
        self.layer(surface, state).ground
    }

    pub fn radius_of(&self, surface: impl Into<Surface>) -> Pixels {
        let surface = surface.into();
        self.shape_radius(surface, self.style(surface.role).shape)
    }
}

pub fn draws_rim(material: &GlassMaterial) -> bool {
    let lit = material.bezel > px(0.0) && (material.specular > 0.0 || material.fresnel > 0.0);
    let edged =
        material.edge_shadow > 0.0 && material.edge_width > px(0.0) && material.edge_color.a > 0.0;
    lit || edged
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

pub(crate) trait InteractiveSurfaceExt: InteractiveElement + Styled + Sized {
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

pub(crate) trait StatefulSurfaceExt: StatefulInteractiveElement + Styled + Sized {
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

pub(crate) fn restyle(style: ala::StyleRefinement, layer: &Layer) -> ala::StyleRefinement {
    let mut style = style
        .border_color(layer.border)
        .shadow(layer.shadow.clone())
        .text_color(layer.text);
    if let Some(mark) = layer.mark {
        style = style.edge_mark(mark);
    }
    match layer.glass {
        Some(material) => style.glass(material).bg(ala::transparent_black()),
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

    fn liquid() -> GlassMaterial {
        super::super::interface_theme::liquid(crate::ThemeMode::Dark)
    }

    #[test]
    fn a_control_radius_rounds_controls_and_leaves_the_rest_on_the_radius() {
        let mut theme = theme();
        theme.radius = px(8.0);
        theme.row_inset = px(4.0);
        let button = Kind::Button.role();
        assert_eq!(
            theme.shape_radius(Surface::new(button), Shape::Round),
            px(8.0)
        );

        theme.control_radius = Some(px(4.0));
        assert_eq!(theme.control_radius(), px(4.0));
        assert_eq!(
            theme.shape_radius(Surface::new(button), Shape::Round),
            px(4.0)
        );
        assert_eq!(
            theme.shape_radius(Surface::new(Kind::Field.role()), Shape::Round),
            px(4.0)
        );
        assert_eq!(
            theme.shape_radius(Surface::new(SurfaceRole::TAB), Shape::Round),
            px(4.0)
        );
        assert_eq!(
            theme.shape_radius(Surface::new(SurfaceRole::POPOVER), Shape::Round),
            px(8.0)
        );
        assert_eq!(
            theme.shape_radius(Surface::new(Kind::Card.role()), Shape::Round),
            px(8.0)
        );
        assert_eq!(
            theme.shape_radius(Surface::new(SurfaceRole::MENU_ITEM), Shape::Round),
            px(4.0)
        );
    }

    #[test]
    fn a_bar_marks_a_row_with_a_washed_pill_and_a_field_with_a_line() {
        let theme = theme();
        let bar = StylePatch::EMPTY.selected(Highlight::Bar);
        let menu_item =
            SurfaceRole::under(&SurfaceRole::MENU_ITEM, "test.row", "Test", "").with(bar);
        let field = SurfaceRole::new("test.field", Kind::Field, "Test", "").with(bar);

        let row = theme.layer(Surface::new(menu_item), PaintState::Selected);
        let mark = row.mark.expect("a selected row carries the pill");
        assert_eq!(mark.color, theme.foreground.opaque());
        assert!(row.shadow.iter().all(|shadow| !shadow.inset));
        assert_ne!(
            row.fill,
            theme.layer(Surface::new(menu_item), PaintState::Rest).fill
        );
        assert!(
            theme
                .layer(Surface::new(menu_item), PaintState::Rest)
                .mark
                .is_none()
        );

        let field = theme.layer(Surface::new(field), PaintState::Focused);
        assert!(field.mark.is_none());
        assert!(field.shadow.iter().any(|shadow| shadow.inset));
    }

    #[test]
    fn glass_paints_a_tint_on_glass_and_solid_without_the_themes_glass() {
        let mut theme = theme();
        let popover = SurfaceRole::POPOVER;
        theme.glass = None;
        let plain = theme.layer(Surface::new(popover), PaintState::Rest);
        assert!(plain.glass.is_none());
        assert_eq!(plain.fill.a, 1.0);

        theme.glass = Some(liquid());
        let glassy = theme.layer(Surface::new(popover), PaintState::Rest);
        assert!(glassy.glass.is_some());
        assert!(glassy.ground.glass);

        let nested = theme.layer(Surface::new(popover).on(glassy.ground), PaintState::Rest);
        assert!(nested.glass.is_none());
        assert!(nested.fill.a < 1.0);
    }

    #[test]
    fn chrome_items_select_as_glass_and_gapped_panes_take_the_rim() {
        let mut theme = theme();
        theme.primary = PrimaryStyle::Accent;
        theme.glass = Some(liquid());
        let chrome = [
            super::super::roles::SIDEBAR_ROW,
            super::super::roles::ROW_NAV,
            super::super::roles::TAB_WINDOW,
            super::super::roles::TAB_PAGE,
        ];
        for role in chrome {
            let selected = theme.layer(Surface::new(role), PaintState::Selected);
            assert!(selected.glass.is_some(), "{}", role.id);
            assert!(
                selected.ground.glass && !selected.ground.solid,
                "{}",
                role.id
            );
        }
        let pane = Surface::new(Kind::Pane.role());
        assert!(theme.layer(pane, PaintState::Rest).glass.is_none());
        theme.pane_margin = px(8.0);
        let gapped = theme.layer(pane, PaintState::Rest);
        let rim = gapped.glass.expect("a pane with gaps is glass");
        assert_eq!(
            (rim.blur, rim.refraction, rim.tint),
            (px(0.0), px(0.0), theme.ground().fill)
        );
        assert!(!gapped.ground.glass);

        theme.glass = None;
        let lit = theme.paint_on(theme.ground(), Tone::Primary, Emphasis::Solid);
        for role in chrome {
            let selected = theme.layer(Surface::new(role), PaintState::Selected);
            assert!(selected.glass.is_none(), "{}", role.id);
            assert_eq!(selected.fill, lit.fill, "{}", role.id);
            assert!(selected.ground.solid, "{}", role.id);
        }
        assert!(theme.layer(pane, PaintState::Rest).glass.is_none());
    }

    #[test]
    fn a_chrome_selection_reads_as_its_highlight_and_a_rimless_pane_or_pip_takes_a_hairline() {
        let mut theme = theme();
        let row = Surface::new(super::super::roles::SIDEBAR_ROW);
        theme.glass = Some(liquid());
        let glass = theme
            .layer(row, PaintState::Selected)
            .glass
            .expect("a selected sidebar row is glass");
        theme.glass = None;
        let base = theme.ground().fill.opaque();
        let highlight = base.blend(theme.layer(row, PaintState::Selected).fill);
        assert_eq!(glass.fallback, Some(highlight));
        let shown = ala::Rgba::from(super::super::color::toward(
            base,
            glass.tint.opaque(),
            glass.tint.a,
        ));
        let highlight = ala::Rgba::from(highlight);
        for (shown, highlight) in [
            (shown.r, highlight.r),
            (shown.g, highlight.g),
            (shown.b, highlight.b),
        ] {
            assert!((shown - highlight).abs() < 0.002, "{shown} {highlight}");
        }

        let pane = Surface::new(Kind::Pane.role());
        let pip = Surface::new(super::super::roles::FLOATING_PIP);
        for (surface, margin) in [(pane, 8.0), (pip, 0.0)] {
            theme.pane_margin = px(margin);
            for (glass, hairline) in [
                (None, true),
                (Some(liquid().bezel(px(0.0)).edge_shadow(0.0)), true),
                (Some(liquid()), false),
            ] {
                theme.glass = glass;
                let layer = theme.layer(surface, PaintState::Rest);
                assert_eq!(
                    (layer.border, layer.border_width),
                    if hairline {
                        (theme.divider_color(), theme.outline)
                    } else {
                        (theme.transparent, px(0.0))
                    },
                    "{} {glass:?}",
                    surface.role.id
                );
            }
        }
    }

    #[test]
    fn a_see_through_window_stays_see_through_under_panels_and_gapped_panes() {
        let mut theme = theme();
        theme.glass = Some(liquid());
        theme.pane_margin = px(8.0);
        theme.window_opacity = 0.93;
        let window = theme.ground();
        assert_eq!(window.fill, theme.background.alpha(0.93));
        let panel = theme.layer(
            Surface::new(Kind::Panel.role()).on(window),
            PaintState::Rest,
        );
        assert_eq!(panel.fill, theme.background.opaque().washed(1));
        assert!(panel.ground.fill.a < 1.0);
        let card = theme.layer(
            Surface::new(super::super::roles::SETTINGS_CARD),
            PaintState::Rest,
        );
        assert_eq!(card.fill, theme.background.opaque().washed(1));
        let pane = theme.layer(Surface::new(Kind::Pane.role()).on(window), PaintState::Rest);
        assert_eq!(pane.glass.expect("a gapped pane is glass").tint.a, 0.0);
    }

    #[test]
    fn a_glass_highlight_turns_a_solid_surface_to_glass_while_it_lasts() {
        let mut theme = theme();
        theme.glass = Some(liquid());
        let tab = SurfaceRole::new("test.tab", Kind::Item, "Test", "").with(
            StylePatch::EMPTY
                .fill(Fill::Solid)
                .selected(Highlight::Glass),
        );
        let rest = theme.layer(Surface::new(tab), PaintState::Rest);
        let selected = theme.layer(Surface::new(tab), PaintState::Selected);
        assert!(rest.glass.is_none());
        assert!(!rest.ground.glass);
        assert!(selected.glass.is_some());
        assert!(selected.ground.glass);
        assert_eq!(selected.border_width, rest.border_width);

        let nested = theme.layer(Surface::new(tab).on(selected.ground), PaintState::Selected);
        assert!(nested.glass.is_none());
        assert_eq!(
            nested.fill,
            theme
                .layer(
                    Surface::new(tab.with(tab.patch.selected(Highlight::Tint))).on(selected.ground),
                    PaintState::Selected
                )
                .fill
        );

        theme.glass = None;
        let washed = theme.layer(Surface::new(tab), PaintState::Selected);
        assert!(washed.glass.is_none());
        assert!(washed.shadow.is_empty());
        assert_ne!(
            washed.fill,
            theme.layer(Surface::new(tab), PaintState::Rest).fill
        );
    }

    #[test]
    fn a_prominent_glass_surface_takes_the_ink_of_its_tone() {
        let mut theme = theme();
        theme.glass = Some(liquid());
        let role = SurfaceRole::new("test.send", Kind::Button, "Test", "")
            .with(StylePatch::EMPTY.fill(Fill::Glass));
        let layer = theme.layer(Surface::new(role).prominent(true), PaintState::Rest);
        assert!(layer.glass.is_some());
        assert_eq!(layer.text, theme.paint(Tone::Primary, Emphasis::Solid).text);
    }

    #[test]
    fn a_toned_floating_chip_draws_its_tone_opaque_and_only_the_neutral_one_is_glass() {
        let mut theme = theme();
        theme.glass = Some(liquid());
        let chip = Surface::new(crate::foundation::roles::CHIP_FLOATING);
        let ground = theme.ground();
        assert!(theme.layer(chip, PaintState::Rest).glass.is_some());

        let soft = theme.layer(chip.tone(Tone::Danger), PaintState::Rest);
        assert!(soft.glass.is_none());
        assert_eq!(
            soft.fill,
            theme
                .paint_on(ground, Tone::Danger, Emphasis::Soft)
                .opaque()
                .fill
        );
        assert_eq!(soft.fill.a, 1.0);

        let solid = theme.layer(chip.tone(Tone::Danger).prominent(true), PaintState::Rest);
        assert!(solid.glass.is_none());
        assert_eq!(
            solid.fill,
            theme.paint_on(ground, Tone::Danger, Emphasis::Solid).fill
        );
        assert_eq!(solid.shadow, theme.float_shadows());

        let callout = Surface::new(crate::foundation::roles::INLINE_PANEL).tone(Tone::Danger);
        assert!(theme.layer(callout, PaintState::Rest).glass.is_some());
    }

    #[test]
    fn restyling_a_glass_layer_carries_its_glass() {
        let mut theme = theme();
        theme.glass = Some(liquid());
        let hover = theme.layer(Surface::new(SurfaceRole::POPOVER), PaintState::Hover);
        assert!(hover.glass.is_some());
        let restyled = restyle(ala::StyleRefinement::default().bg(hover.fill), &hover);
        assert_eq!(restyled.glass, hover.glass);
        assert!(
            restyled
                .background
                .and_then(|fill| fill.color())
                .is_none_or(|color| color.is_transparent())
        );
    }

    #[test]
    fn a_tinted_surface_deepens_on_hover_and_selection() {
        let theme = theme();
        let role = SurfaceRole::new("test.tile", Kind::Card, "Test", "").with(
            StylePatch::EMPTY
                .fill(Fill::Tint)
                .shadow(Shadow::None)
                .hover(Highlight::Tint)
                .selected(Highlight::Tint),
        );
        let rest = theme.layer(Surface::new(role), PaintState::Rest).fill;
        let hover = theme.layer(Surface::new(role), PaintState::Hover).fill;
        let current = theme.layer(Surface::new(role), PaintState::Current).fill;
        assert!(hover.a > rest.a, "{rest:?} {hover:?}");
        assert!(current.a > hover.a, "{hover:?} {current:?}");
        let clear = SurfaceRole::new("test.clear", Kind::Card, "Test", "").with(
            StylePatch::EMPTY
                .fill(Fill::None)
                .shadow(Shadow::None)
                .hover(Highlight::Tint),
        );
        assert!(theme.layer(Surface::new(clear), PaintState::Hover).fill.a > 0.0);
    }

    #[test]
    fn a_ring_never_shows_through_its_fill() {
        let theme = theme();
        let role = SurfaceRole::new("test.ring", Kind::Card, "Test", "").with(
            StylePatch::EMPTY
                .fill(Fill::Tint)
                .shadow(Shadow::None)
                .selected(Highlight::Ring),
        );
        assert_eq!(
            theme.layer(Surface::new(role), PaintState::Current).fill.a,
            1.0
        );
        assert!(theme.layer(Surface::new(role), PaintState::Rest).fill.a < 1.0);
    }

    #[test]
    fn controls_cast_shadows_only_on_the_window() {
        let theme = theme();
        let button = SurfaceRole::new("test.button", Kind::Button, "Test", "")
            .with(StylePatch::EMPTY.shadow(Shadow::Contact));
        assert!(
            !theme
                .layer(Surface::new(button), PaintState::Rest)
                .shadow
                .is_empty()
        );
        let panel = theme.surface(Elevation::Panel);
        assert!(
            theme
                .layer(Surface::new(button).on(panel), PaintState::Rest)
                .shadow
                .is_empty()
        );
        assert!(
            !theme
                .layer(
                    Surface::new(SurfaceRole::POPOVER).on(panel),
                    PaintState::Rest
                )
                .shadow
                .is_empty()
        );
    }

    #[test]
    fn highlights_never_change_the_border_width() {
        let theme = theme();
        for role in SurfaceRole::KIT {
            let widths =
                PaintState::ALL.map(|state| theme.layer(Surface::new(role), state).border_width);
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
                .layer(Surface::new(role), PaintState::Rest)
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
        theme.inspect = Some("card".into());
        theme.inspect_strength = 0.0;
        assert_eq!(rings(&theme), 0);
    }

    #[test]
    fn resting_ink_sits_a_step_down_until_pointed_at() {
        let theme = theme();
        let bright = Kind::Button.role();
        let rest = SurfaceRole::new("button.rest", Kind::Button, "Rest", "")
            .with(StylePatch::EMPTY.ink(Ink::Rest));
        let text =
            |role: SurfaceRole, state: PaintState| theme.layer(Surface::new(role), state).text;
        assert_ne!(text(rest, PaintState::Rest), text(bright, PaintState::Rest));
        for state in [PaintState::Hover, PaintState::Active, PaintState::Current] {
            assert_eq!(text(rest, state), text(bright, state), "{state:?}");
        }
    }

    #[test]
    fn inspecting_a_kind_rings_every_role_of_it() {
        let mut theme = theme();
        theme.inspect = Some("button".into());
        let ringed = |theme: &Theme, role: SurfaceRole| {
            theme
                .layer(Surface::new(role), PaintState::Rest)
                .shadow
                .iter()
                .any(|shadow| shadow.color == INSPECT_COLOR)
        };
        assert!(ringed(&theme, Kind::Button.role()));
        assert!(ringed(&theme, SurfaceRole::QUIET_BUTTON));
        assert!(!ringed(&theme, Kind::Card.role()));
    }

    #[test]
    fn noting_drawn_roles_names_each_role_and_its_kind() {
        let theme = theme();
        theme.style(SurfaceRole::QUIET_BUTTON);
        assert!(drawn_roles().is_empty());
        note_drawn_roles();
        theme.style(SurfaceRole::QUIET_BUTTON);
        let drawn = drawn_roles();
        assert_eq!(
            drawn,
            BTreeSet::from([SurfaceRole::QUIET_BUTTON.id, "button"])
        );
        theme.style(Kind::Card.role());
        assert!(drawn_roles().is_empty());
    }

    #[test]
    fn content_on_a_tinted_selection_takes_its_ink() {
        let mut theme = theme();
        let chip = |theme: &Theme, tone, ground| {
            theme.layer(
                Surface::new(SurfaceRole::CHIP).tone(tone).on(ground),
                PaintState::Rest,
            )
        };
        let washed = theme.ground_at(Surface::new(SurfaceRole::MENU_ITEM), PaintState::Selected);
        assert_eq!(
            chip(&theme, Tone::Success, washed).fill,
            theme.success.opacity(0.12)
        );

        theme.accent = crate::parse_hex("#ad4f00").unwrap();
        theme.primary = PrimaryStyle::Accent;
        let picked = theme.ground_at(Surface::new(SurfaceRole::MENU_ITEM), PaintState::Selected);
        let toned = chip(&theme, Tone::Success, picked);
        let neutral = chip(&theme, Tone::Neutral, picked);
        assert_eq!(toned.fill, neutral.fill);
        assert_eq!(crate::to_hex(toned.text), "#ebebeb");
        let label = theme.paint_on(picked, Tone::Danger, Emphasis::Ghost);
        assert_eq!(crate::to_hex(label.text), "#ebebeb");
        assert_eq!(crate::to_hex(label.muted_at(PaintState::Rest)), "#ebebeb");
    }

    #[test]
    fn a_graphite_selection_gives_its_content_the_row_ink() {
        let mut theme = theme();
        theme.accent = crate::parse_hex("#aaaaaa").unwrap();
        theme.primary = PrimaryStyle::Accent;
        let picked = theme.ground_at(Surface::new(SurfaceRole::MENU_ITEM), PaintState::Selected);
        assert_eq!(crate::to_hex(picked.fill), "#aaaaaa");
        let chip = |tone| {
            theme.layer(
                Surface::new(SurfaceRole::CHIP).tone(tone).on(picked),
                PaintState::Rest,
            )
        };
        assert_eq!(chip(Tone::Success).fill, chip(Tone::Neutral).fill);
        assert_eq!(crate::to_hex(chip(Tone::Success).text), "#010101");
        let label = theme.paint_on(picked, Tone::Danger, Emphasis::Ghost);
        assert_eq!(crate::to_hex(label.text), "#010101");
        assert_eq!(crate::to_hex(label.muted_at(PaintState::Rest)), "#010101");
        let listed = theme
            .paint(Tone::Neutral, Emphasis::Ghost)
            .ground_at(PaintState::Selected);
        let label = theme.paint_on(listed, Tone::Danger, Emphasis::Ghost);
        assert_eq!(crate::to_hex(label.muted_at(PaintState::Rest)), "#010101");
    }

    #[test]
    fn a_picked_row_washes_in_the_ink_until_a_tint_fills_it() {
        let mut theme = theme();
        let menu = theme.ground_of(Surface::new(SurfaceRole::POPOVER));
        let washed = SurfaceRole::under(&SurfaceRole::MENU_ITEM, "test.row", "Test", "")
            .with(StylePatch::EMPTY.selected(Highlight::Tint));
        for (role, ground) in [
            (SurfaceRole::MENU_ITEM, menu),
            (SurfaceRole::ROW, theme.ground()),
        ] {
            let picked = theme.layer(Surface::new(role).on(ground), PaintState::Selected);
            let wash = theme.layer(Surface::new(washed).on(ground), PaintState::Selected);
            assert_eq!(
                (picked.fill, picked.text),
                (wash.fill, wash.text),
                "{}",
                role.id
            );
        }

        let tint = crate::parse_hex("#ad4f00").unwrap();
        theme.accent = tint;
        theme.primary = PrimaryStyle::Accent;
        let on_tint = crate::parse_hex("#ebebeb").unwrap();
        for (role, ground) in [
            (SurfaceRole::MENU_ITEM, menu),
            (SurfaceRole::ROW, theme.ground()),
            (SurfaceRole::TAB, theme.ground()),
        ] {
            let picked = theme.layer(Surface::new(role).on(ground), PaintState::Selected);
            assert_eq!(crate::to_hex(picked.fill), "#ad4f00", "{}", role.id);
            assert_eq!(
                crate::to_hex(picked.text),
                crate::to_hex(on_tint),
                "{}",
                role.id
            );
        }
    }
}
