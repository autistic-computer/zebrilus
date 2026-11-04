#![allow(clippy::pedantic, clippy::style, clippy::complexity)]

mod animation;
mod color;
pub mod content;
pub mod dismissal;
mod element_ext;
mod geometry;
pub mod highlighter;
mod index_path;
pub mod interface_theme;
mod layout;
pub mod motion;
pub mod overlay;
mod palette;
pub mod pulse;
pub mod roles;
mod space;
mod styled;
mod surface;
mod text_style;
mod theme;
mod theme_color;
pub mod theme_css;
mod units;
mod variant;
mod window_border;

pub use animation::cubic_ease;
pub use animation::{SURFACE_ENTER_DURATION, surface_enter};
pub use color::{Colorize, oklab_lightness, oklch, parse_hex, to_hex, to_oklch};
pub use element_ext::{AuthoredElement, ElementExt, InteractiveElementExt};
pub(crate) use element_ext::{author_name, author_part};
pub use geometry::Side;
pub use index_path::IndexPath;
pub use layout::{center, inline, inset, split, stack, tiles};
pub use space::Space;
pub use styled::{
    Density, Disableable, FOCUS_RING_WIDTH, RULE_WIDTH, RuleLook, Selectable, Sizable, Size,
    StyledExt, h_flex, lifted_shadow, list_row, muted_on, row_state, v_flex,
};
pub(crate) use styled::{ROW_CORNER_SMOOTHING, RoleRuleExt, row_corners, row_radius, row_surface};
pub use surface::{
    Edge, Fill, Highlight, Ink, Kind, Knobs, Layer, Shadow, ShadowMaterial, Shape, Smoothing,
    Style, StylePatch, Surface, SurfaceRole, drawn_roles, note_drawn_roles,
};
pub(crate) use surface::{StatefulSurfaceExt, restyle};
pub use text_style::TextStyle;
pub use theme::{
    ADAPTIVE_CORNER_FRACTION, ActiveTheme, CHROME_GAP, CORNER_SMOOTHING,
    PANE_HEADER_CONTROL_HEIGHT, PANE_HEADER_HEIGHT, ScrollbarShow, Theme, ThemeMode,
};
pub use theme_color::ThemeColor;
pub use units::{BASE_UI_FONT_SIZE, UiZoom, rems_from_px};
pub use variant::{
    Elevation, Emphasis, Ground, InteractivePaintExt, Paint, PaintExt, PaintState, PrimaryStyle,
    Tone,
};
pub use window_border::{window_border, window_paddings};

pub fn init(cx: &mut ala::App) {
    theme::init(cx);
}
