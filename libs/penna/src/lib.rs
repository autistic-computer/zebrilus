pub use ala;

pub(crate) const BLINK_IDLE_TIMEOUT: web_time::Duration = web_time::Duration::from_secs(10);

pub mod blocks;
pub mod components;
pub mod foundation;

pub use blocks::dock::Dock;
pub use blocks::inspector::Inspector;
pub use blocks::notice::Notice;
pub use blocks::steps::Steps;
pub use blocks::title_bar::{
    MACOS_TRAFFIC_LIGHT_SPAN, TITLE_BAR_HEIGHT, TitleBar, WindowControls, draws_window_controls,
    macos_traffic_light_clearance, window_controls_width,
};
pub use components::bar::{Bar, BarLook};
pub use components::card::{Card, CardLook};
pub use components::chip::{Chip, ChipLook, Dot};
pub use components::icon::{Assets, Icon, IconName};
pub use components::label::Label;
pub use components::panel::{Panel, PanelLook};
pub use components::row::{Row, RowLook};
pub use components::tab::{Tab, TabLook};
pub use components::tile::{Tile, TileVariant};
pub use components::well::Well;
pub use foundation::content::ContentLook;
pub use foundation::overlay::{ROOT_KEY_CONTEXT, Root, WindowExt};
pub use foundation::{
    ADAPTIVE_CORNER_FRACTION, ActiveTheme, AuthoredElement, BASE_UI_FONT_SIZE, CHROME_GAP,
    CORNER_SMOOTHING, Colorize, Density, Disableable, Edge, ElementExt, Elevation, Emphasis,
    FOCUS_RING_WIDTH, Fill, Ground, Highlight, IndexPath, Ink, InteractiveElementExt,
    InteractivePaintExt, Kind, Layer, PANE_HEADER_CONTROL_HEIGHT, PANE_HEADER_HEIGHT, Paint,
    PaintExt, PaintState, PrimaryStyle, RULE_WIDTH, RuleLook, ScrollbarShow, Selectable, Shadow,
    ShadowMaterial, Shape, Side, Sizable, Size, Smoothing, Space, Style, StylePatch, StyledExt,
    Surface, SurfaceRole, TextStyle, Theme, ThemeColor, ThemeMode, Tone, UiZoom, center,
    cubic_ease, h_flex, inline, inset, knob_shadow, list_row, muted_on, oklab_lightness, oklch,
    parse_hex, rems_from_px, row_state, split, stack, tiles, to_hex, to_oklch, v_flex,
    window_border, window_paddings,
};

pub fn init(cx: &mut ala::App) {
    foundation::init(cx);
    #[cfg(feature = "editor")]
    blocks::code_editor::init(cx);
    components::input::init(cx);
    components::menu::init(cx);
    foundation::overlay::init(cx);
    components::popover::init(cx);
    components::select::init(cx);
    components::text::init(cx);
    foundation::interface_theme::default_theme().activate(None, None, cx);
}

#[cfg(test)]
pub(crate) fn init_without_theme(cx: &mut ala::App) {
    use foundation::interface_theme::{Glass, InterfaceTheme};
    init(cx);
    InterfaceTheme {
        glass: Some(Glass::None),
        ..InterfaceTheme::default()
    }
    .activate(Some(ThemeMode::Light), None, cx);
}

#[cfg(test)]
pub(crate) fn init_tinted(cx: &mut ala::App) {
    use foundation::interface_theme::{Glass, InterfaceTheme, Tint};
    init(cx);
    InterfaceTheme {
        tint: Tint::named("blue").unwrap_or_default(),
        glass: Some(Glass::None),
        ..InterfaceTheme::default()
    }
    .activate(Some(ThemeMode::Light), None, cx);
}
