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
    ActiveTheme, Assets, BASE_UI_FONT_SIZE, CHROME_GAP, CORNER_SMOOTHING, Colorize, Disableable,
    Edge, ElementExt, Elevation, Emphasis, Fill, Ground, Highlight, Icon, IconName, IndexPath,
    InteractiveElementExt, InteractivePaintExt, InteractiveSurfaceExt, Kind, Layer,
    MACOS_TRAFFIC_LIGHT_INSET, MACOS_TRAFFIC_LIGHT_SPAN, Paint, PaintExt, PaintState,
    PrimaryStyle, ROOT_KEY_CONTEXT, RULE_WIDTH, Root, ScrollbarShow, Selectable, Shadow,
    ShadowMaterial, Shape, Side, Sizable, Size, StatefulSurfaceExt, Style, StylePatch, StyledExt,
    Surface, SurfaceRole, TITLE_BAR_HEIGHT, Theme, ThemeColor, ThemeMode, TitleBar, Tone, UiZoom,
    WindowControls, WindowExt, contact_shadow, control_shadow, cubic_ease, draws_window_controls,
    floating_shadow, h_flex, init, lifted_shadow, macos_traffic_light_clearance, oklab_lightness,
    parse_hex, rems_from_px, to_hex, v_flex, window_border, window_controls_width, window_paddings,
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
pub mod navigation;
pub mod pane;
pub mod path_picker;
pub mod picker;
pub mod roles;
pub mod settings;
pub mod shell;
pub mod terminal;
pub mod terminal_images;
pub mod which_key;
