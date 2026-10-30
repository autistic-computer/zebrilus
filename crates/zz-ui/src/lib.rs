pub mod tmux_style;

pub(crate) use penna::{
    badge, button, check, color_picker, feedback, icon, input, kbd, list, menu, motion,
    notification, picker, pulse, scroll, segmented, select, separator, sidebar, slider, spinner,
    text, tooltip, touch,
};

pub(crate) use penna::{
    ADAPTIVE_CORNER_FRACTION, ActiveTheme, CHROME_GAP, CORNER_SMOOTHING, Colorize, Disableable,
    Edge, ElementExt, Elevation, Emphasis, Fill, Ground, Highlight, Icon, IconName, IndexPath,
    InteractiveSurfaceExt, Kind, Layer, PANE_HEADER_CONTROL_HEIGHT, Paint, PaintExt, PaintState,
    PrimaryStyle, RULE_WIDTH, Selectable, Shape, Sizable, StatefulSurfaceExt, StyledExt, Surface,
    SurfaceRole, Theme, ThemeColor, ThemeMode, Tone, WindowExt, h_flex, list_row, muted_on,
    oklab_lightness, oklch, rems_from_px, row_corners, row_state, row_surface, to_hex, to_oklch,
    v_flex,
};

#[cfg(feature = "agent")]
pub mod agent;
pub mod attachment;
pub mod browser;
pub mod chooser;
pub mod command;
pub mod compact;
pub mod files;
pub mod interface_theme;
pub mod mend;
pub mod navigation;
pub mod pane;
pub mod pip;
pub mod program;
pub mod roles;
pub mod settings;
pub mod shell;
pub mod target;
pub mod terminal;
pub mod terminal_images;
pub mod theme_css;

pub use program::Assets;

pub fn register() {
    penna::roles::register(roles::ZZ);
    penna::roles::retire(&roles::RETIRED);
    penna::interface_theme::register(&interface_theme::BUILTIN_SOURCES);
}

pub fn init(cx: &mut ala::App) {
    register();
    penna::init(cx);
}

pub const LABEL_FADE: ala::Pixels = ala::px(28.0);
