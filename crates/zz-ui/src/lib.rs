//! zz's application UI on [`zz_gpui_kit`]: panes, terminal, agent, command palette,
//! settings, and navigation, shared by the desktop app and the thin clients.
//! It re-exports the kit, so zz code imports UI from one place.

pub mod tmux_style;

pub use zz_gpui_kit::{
    badge, button, check, color_picker, dialog, highlighter, icon, input, kbd, list, menu,
    notification, overlay, popover, pulse, scroll, select, separator, slider, spinner, switch,
    text, theme, title_bar, tooltip, touch,
};

#[cfg(feature = "editor")]
pub use zz_gpui_kit::code_editor;

pub use zz_gpui_kit::{
    ADAPTIVE_CORNER_FRACTION, ActiveTheme, BASE_UI_FONT_SIZE, CHROME_GAP, CORNER_SMOOTHING,
    Colorize, Disableable, Edge, ElementExt, Elevation, Emphasis, Fill, Ground, Highlight, Icon,
    IconName, IndexPath, Ink, InteractiveElementExt, InteractivePaintExt, InteractiveSurfaceExt,
    Kind, Knobs, Layer, MACOS_TRAFFIC_LIGHT_SPAN, PANE_HEADER_CONTROL_HEIGHT, PANE_HEADER_HEIGHT,
    Paint, PaintExt, PaintState, PrimaryStyle, ROOT_KEY_CONTEXT, RULE_WIDTH, Root, ScrollbarShow,
    Selectable, Shadow, ShadowMaterial, Shape, Side, Sizable, Size, Smoothing, StatefulSurfaceExt,
    Style, StylePatch, StyledExt, Surface, SurfaceRole, TITLE_BAR_HEIGHT, Theme, ThemeColor,
    ThemeMode, TitleBar, Tone, UiZoom, WindowControls, WindowExt, cubic_ease, drawn_roles,
    draws_window_controls, h_flex, init, lifted_shadow, macos_traffic_light_clearance,
    note_drawn_roles, oklab_lightness, oklch, parse_hex, rems_from_px, row_corners, row_radius,
    row_surface, to_hex, to_oklch, v_flex, window_border, window_controls_width, window_paddings,
};

#[cfg(feature = "agent")]
pub mod agent;
pub mod attachment;
pub mod browser;
pub mod chooser;
pub mod command;
pub mod compact;
pub mod feedback;
pub mod interface_theme;
pub mod mend;
pub mod motion;
pub mod navigation;
pub mod pane;
pub mod path_picker;
pub mod picker;
pub mod pip;
pub mod program;
pub mod roles;
pub mod settings;
pub mod shell;
pub mod terminal;
pub mod terminal_images;
pub mod which_key;

pub use program::Assets;

/// How far a label too long for its box fades out at its end, instead of
/// ending in an ellipsis.
pub const LABEL_FADE: zz_gpui::Pixels = zz_gpui::px(28.0);
