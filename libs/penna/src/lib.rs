pub use ala;

pub(crate) const BLINK_IDLE_TIMEOUT: web_time::Duration = web_time::Duration::from_secs(10);

pub mod badge;
pub mod button;
mod canvas;
pub mod check;
mod chip;
#[cfg(feature = "editor")]
pub mod code_editor;
pub mod color_picker;
pub mod compact;
pub mod dismissal;
pub mod feedback;
pub mod foundation;
pub mod highlighter;
pub mod icon;
pub mod input;
mod inspector;
pub mod interface_theme;
pub mod kbd;
mod label;
pub mod list;
pub mod menu;
pub mod motion;
mod notice;
pub mod overlay;
pub mod picker;
pub mod popover;
pub mod pulse;
pub mod roles;
pub mod scroll;
pub mod segmented;
pub mod select;
pub mod separator;
pub mod settings;
pub mod sheet;
pub mod shell;
pub mod sidebar;
pub mod slider;
pub mod spinner;
mod steps;
pub mod switch;
pub mod text;
pub mod theme_css;
pub mod title_bar;
pub mod tooltip;
pub mod touch;
#[cfg(feature = "tray")]
pub mod tray;
pub mod which_key;

pub mod dialog {
    pub use super::overlay::{AlertDialog, Dialog, DialogButtonProps};
}

pub mod notification {
    pub use super::overlay::Notification;
}

pub use canvas::{Dock, Tile, TileVariant, Well};
pub use chip::{Chip, Dot};
pub use foundation::{
    ADAPTIVE_CORNER_FRACTION, ActiveTheme, BASE_UI_FONT_SIZE, CHROME_GAP, CORNER_SMOOTHING,
    Colorize, Disableable, Edge, ElementExt, Elevation, Emphasis, FOCUS_RING_WIDTH, Fill, Ground,
    Highlight, IndexPath, Ink, InteractiveElementExt, InteractivePaintExt, InteractiveSurfaceExt,
    Kind, Knobs, Layer, PANE_HEADER_CONTROL_HEIGHT, PANE_HEADER_HEIGHT, Paint, PaintExt,
    PaintState, PrimaryStyle, RULE_WIDTH, ScrollbarShow, Selectable, Shadow, ShadowMaterial, Shape,
    Side, Sizable, Size, Smoothing, Space, StatefulSurfaceExt, Style, StylePatch, StyledExt,
    Surface, SurfaceRole, TextStyle, Theme, ThemeColor, ThemeMode, Tone, UiZoom, cubic_ease,
    h_flex, lifted_shadow, list_row, muted_on, oklab_lightness, oklch, parse_hex, rems_from_px,
    row_corners, row_radius, row_state, row_surface, to_hex, to_oklch, v_flex, window_border,
    window_paddings,
};
pub use icon::{Assets, Icon, IconName};
pub use inspector::Inspector;
pub use label::Label;
pub use notice::Notice;
pub use overlay::{ROOT_KEY_CONTEXT, Root, WindowExt};
pub use steps::Steps;
pub use title_bar::{
    MACOS_TRAFFIC_LIGHT_SPAN, TITLE_BAR_HEIGHT, TitleBar, WindowControls, draws_window_controls,
    macos_traffic_light_clearance, window_controls_width,
};

pub fn register(
    roles: &'static [SurfaceRole],
    retired: &'static [&'static str],
    themes: &'static [(&'static str, &'static str)],
) {
    roles::register(roles);
    roles::retire(retired);
    interface_theme::register(themes);
    let _ = interface_theme::builtin_themes();
}

pub fn init(cx: &mut ala::App) {
    foundation::init(cx);
    #[cfg(feature = "editor")]
    code_editor::init(cx);
    input::init(cx);
    menu::init(cx);
    overlay::init(cx);
    popover::init(cx);
    select::init(cx);
    text::init(cx);
    interface_theme::default_theme().activate(None, None, cx);
}

#[cfg(test)]
pub(crate) fn init_without_theme(cx: &mut ala::App) {
    init(cx);
    interface_theme::InterfaceTheme::default().activate(Some(ThemeMode::Light), None, cx);
}
