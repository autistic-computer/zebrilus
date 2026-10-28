use keymap::ChromeOverride;
use std::{
    env, fmt,
    fs::{self, File},
    io::{self, ErrorKind, Read as _},
    path::{Path, PathBuf},
    time::SystemTime,
};
use zz_client::StatusBarSettings;
use zz_client::appearance::{
    ACCENTS, AccentSetting, AppIconSetting, BUILTIN_THEMES, DEFAULT_ANIMATION_SPEED, DEFAULT_THEME,
    DEFAULT_TINT_HUE, MAX_ANIMATION_SPEED, MAX_CORNER_RADIUS, MIN_ANIMATION_SPEED,
    ThemeModeSetting, is_builtin_theme,
};
use zz_client::url_input::SearchProvider;
pub use zz_daemon_client::{
    HostEntry, RejectedHost, atomic_write, configured_fleet_hosts, validate_fleet_host,
};
use zz_protocol::{ConfigOverrideEntry, MuxOptionKey};
use zz_terminal::{
    AppearanceColor, AppearanceConfigKey, CellHeightAdjustment, Color, CursorBlinkPolicy,
    CursorStyle, TerminalAppearance,
};
pub mod file_options;
pub mod import;
pub mod keymap;
pub mod mux_bindings;
pub mod themes;
#[cfg(feature = "update")]
pub mod update;

pub const CONFIG_DIRECTORY_NAME: &str = zz_protocol::app_identity::DIRECTORY;
pub const CONFIG_FILE_NAME: &str = "config";
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;

pub const MAX_PANE_MARGIN: f32 = 32.0;
pub const MAX_WINDOW_CORNER_RADIUS: f32 = 32.0;

// Tangent-circle fit of the native macOS 27 window corner, measured off a screenshot.
pub const DEFAULT_WINDOW_CORNER_RADIUS: f32 = 13.5;

pub const DEFAULT_PANE_GAPS: bool = false;
pub const DEFAULT_PANE_GLOW_STRENGTH: f32 = 1.0;
pub const DEFAULT_PIP_BACKGROUND_OPACITY: f32 = 0.8;
pub const DEFAULT_PANE_INACTIVE_OPACITY: f32 = 0.7;
pub const MIN_PANE_INACTIVE_OPACITY: f32 = 0.0;
pub const MAX_PANE_INACTIVE_OPACITY: f32 = 1.0;
pub const DEFAULT_PANE_MARGIN: f32 = 6.0;
/// The default theme's radius, until the client reads the active theme and
/// calls [`AppConfig::follow_theme_radius`].
pub const DEFAULT_WIDGET_CORNER_RADIUS: f32 = 24.0;
pub const DEFAULT_CHROME_CONTRAST: f32 = 1.0;
pub const DEFAULT_CHROME_TINT: f32 = 0.0;
pub const DEFAULT_USE_SYSTEM_TITLEBAR: bool = false;
pub const DEFAULT_WINDOW_BACKGROUND_BLUR: bool = false;
pub const DEFAULT_WINDOW_BACKGROUND_OPACITY: f32 = 0.0;
pub const DEFAULT_ANIMATIONS: bool = true;
pub const DEFAULT_TRAY: bool = true;
pub const DEFAULT_NOTIFICATIONS: bool = true;
pub const DEFAULT_QUIT_DAEMON_ON_EXIT: bool = false;
pub const DEFAULT_AUTO_RESTART_STALE_DAEMON: bool = false;
pub const DEFAULT_CHECK_FOR_UPDATES: bool = true;
#[cfg(target_os = "macos")]
pub const DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY: &str = "cmd-shift-c";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY: &str = "ctrl-shift-c";
/// Repeatable chrome binding overrides: `<table>:<key>=<action>` and
/// `<table>:<key>`.
pub const CHROME_KEYBIND_KEY: &str = "chrome-keybind";
pub const CHROME_UNBIND_KEY: &str = "chrome-unbind";
pub const DEFAULT_BROWSER_SEARCH_PROVIDER: SearchProvider = SearchProvider::Google;
pub const DEFAULT_BROWSER_EGRESS: bool = true;
pub const DEFAULT_WHICH_KEY_DELAY: f32 = 400.0;
pub const MAX_WHICH_KEY_DELAY: f32 = 2000.0;
pub const DEFAULT_EDITOR_FONT_SIZE: f32 = 13.0;
pub const MIN_EDITOR_FONT_SIZE: f32 = 8.0;
pub const MAX_EDITOR_FONT_SIZE: f32 = 32.0;
pub const DEFAULT_EDITOR_LINE_NUMBERS: bool = true;
pub const DEFAULT_EDITOR_RELATIVE_LINE_NUMBERS: bool = true;
pub const DEFAULT_EDITOR_SOFT_WRAP: bool = true;
pub const DEFAULT_EDITOR_VIM_MODE: bool = true;
pub const DEFAULT_EXPERIMENTAL_EDITOR_PANE: bool = false;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigKey {
    PaletteWindowLayout,
    PaletteShowKeys,
    PickerFocusSidebar,
    WhichKeyDelay,
    UseSystemTitlebar,
    WindowCornerRadius,
    WindowBackgroundBlur,
    WindowBackgroundOpacity,
    Animations,
    AnimationSpeed,
    Tray,
    Notifications,
    QuitDaemonOnExit,
    AutoRestartStaleDaemon,
    CheckForUpdates,
    StatusBadges,
    StatusHost,
    StatusUpdate,
    FlatSidebar,
    ProgramColors,
    ExperimentalEditorPane,
    PaneGaps,
    PipBackgroundOpacity,
    PaneGlowStrength,
    PaneInactiveOpacity,
    PaneMargin,
    WidgetCornerRadius,
    ChromeContrast,
    ChromeAccent,
    ChromeTint,
    ChromeTintHue,
    EditorFontSize,
    EditorLineNumbers,
    EditorRelativeLineNumbers,
    EditorSoftWrap,
    EditorVimMode,
    BrowserElementSelectorHotkey,
    BrowserRemoteDebuggingPort,
    BrowserSearchProvider,
    BrowserEgress,
    ThemeMode,
    InterfaceTheme,
    UiFontFamily,
    AppIcon,
    DriverAgent,
    DriverModel,
    DriverEffort,
    DriverHosts,
}

impl ConfigKey {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PaletteWindowLayout => "palette-window-layout",
            Self::PaletteShowKeys => "palette-show-keys",
            Self::PickerFocusSidebar => "picker-focus-sidebar",
            Self::WhichKeyDelay => "which-key-delay",
            Self::UseSystemTitlebar => "use-system-titlebar",
            Self::WindowCornerRadius => "window-corner-radius",
            Self::WindowBackgroundBlur => "window-background-blur",
            Self::WindowBackgroundOpacity => "window-background-opacity",
            Self::Animations => "animations",
            Self::AnimationSpeed => "animation-speed",
            Self::Tray => "tray",
            Self::Notifications => "notifications",
            Self::QuitDaemonOnExit => "quit-daemon-on-exit",
            Self::AutoRestartStaleDaemon => "auto-restart-stale-daemon",
            Self::CheckForUpdates => "check-for-updates",
            Self::StatusBadges => "status-badges",
            Self::StatusHost => "status-host",
            Self::StatusUpdate => "status-update",
            Self::FlatSidebar => "flat-sidebar",
            Self::ProgramColors => "program-colors",
            Self::ExperimentalEditorPane => "experimental-editor-pane",
            Self::PaneGaps => "pane-gaps",
            Self::PipBackgroundOpacity => "pip-background-opacity",
            Self::PaneGlowStrength => "pane-glow-strength",
            Self::PaneInactiveOpacity => "pane-inactive-opacity",
            Self::PaneMargin => "pane-margin",
            Self::WidgetCornerRadius => "widget-corner-radius",
            Self::ChromeContrast => "chrome-contrast",
            Self::ChromeAccent => "chrome-accent",
            Self::ChromeTint => "chrome-tint",
            Self::ChromeTintHue => "chrome-tint-hue",
            Self::EditorFontSize => "editor-font-size",
            Self::EditorLineNumbers => "editor-line-numbers",
            Self::EditorRelativeLineNumbers => "editor-relative-line-numbers",
            Self::EditorSoftWrap => "editor-soft-wrap",
            Self::EditorVimMode => "editor-vim-mode",
            Self::BrowserElementSelectorHotkey => "browser-element-selector-hotkey",
            Self::BrowserRemoteDebuggingPort => "browser-remote-debugging-port",
            Self::BrowserSearchProvider => "browser-search-provider",
            Self::BrowserEgress => "browser-egress",
            Self::ThemeMode => "theme-mode",
            Self::InterfaceTheme => "interface-theme",
            Self::UiFontFamily => "ui-font-family",
            Self::AppIcon => "app-icon",
            Self::DriverAgent => "driver-agent",
            Self::DriverModel => "driver-model",
            Self::DriverEffort => "driver-effort",
            Self::DriverHosts => "driver-hosts",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "palette-window-layout" => Some(Self::PaletteWindowLayout),
            "palette-show-keys" => Some(Self::PaletteShowKeys),
            "picker-focus-sidebar" => Some(Self::PickerFocusSidebar),
            "which-key-delay" => Some(Self::WhichKeyDelay),
            "use-system-titlebar" => Some(Self::UseSystemTitlebar),
            "window-corner-radius" => Some(Self::WindowCornerRadius),
            "window-background-blur" => Some(Self::WindowBackgroundBlur),
            "window-background-opacity" => Some(Self::WindowBackgroundOpacity),
            "animations" => Some(Self::Animations),
            "animation-speed" => Some(Self::AnimationSpeed),
            "tray" => Some(Self::Tray),
            "notifications" => Some(Self::Notifications),
            "quit-daemon-on-exit" => Some(Self::QuitDaemonOnExit),
            "auto-restart-stale-daemon" => Some(Self::AutoRestartStaleDaemon),
            "check-for-updates" => Some(Self::CheckForUpdates),
            "status-badges" => Some(Self::StatusBadges),
            "status-host" => Some(Self::StatusHost),
            "status-update" => Some(Self::StatusUpdate),
            "flat-sidebar" => Some(Self::FlatSidebar),
            "program-colors" => Some(Self::ProgramColors),
            "experimental-editor-pane" => Some(Self::ExperimentalEditorPane),
            "pane-gaps" => Some(Self::PaneGaps),
            "pip-background-opacity" => Some(Self::PipBackgroundOpacity),
            "pane-glow-strength" => Some(Self::PaneGlowStrength),
            "pane-inactive-opacity" => Some(Self::PaneInactiveOpacity),
            "pane-margin" => Some(Self::PaneMargin),
            "widget-corner-radius" => Some(Self::WidgetCornerRadius),
            "chrome-contrast" => Some(Self::ChromeContrast),
            "chrome-accent" => Some(Self::ChromeAccent),
            "chrome-tint" => Some(Self::ChromeTint),
            "chrome-tint-hue" => Some(Self::ChromeTintHue),
            "editor-font-size" => Some(Self::EditorFontSize),
            "editor-line-numbers" => Some(Self::EditorLineNumbers),
            "editor-relative-line-numbers" => Some(Self::EditorRelativeLineNumbers),
            "editor-soft-wrap" => Some(Self::EditorSoftWrap),
            "editor-vim-mode" => Some(Self::EditorVimMode),
            "browser-element-selector-hotkey" => Some(Self::BrowserElementSelectorHotkey),
            "browser-remote-debugging-port" => Some(Self::BrowserRemoteDebuggingPort),
            "browser-search-provider" => Some(Self::BrowserSearchProvider),
            "browser-egress" => Some(Self::BrowserEgress),
            "theme-mode" => Some(Self::ThemeMode),
            "interface-theme" => Some(Self::InterfaceTheme),
            "ui-font-family" => Some(Self::UiFontFamily),
            "app-icon" => Some(Self::AppIcon),
            "driver-agent" => Some(Self::DriverAgent),
            "driver-model" => Some(Self::DriverModel),
            "driver-effort" => Some(Self::DriverEffort),
            "driver-hosts" => Some(Self::DriverHosts),
            _ => None,
        }
    }

    /// The inclusive range this numeric key accepts, or `None` for a key that
    /// is not numeric.
    pub const fn numeric_range(self) -> Option<(f32, f32)> {
        match self {
            Self::PipBackgroundOpacity | Self::WindowBackgroundOpacity | Self::ChromeTint => {
                Some((0.0, 1.0))
            }
            Self::PaneGlowStrength => Some((0.0, 2.0)),
            Self::PaneInactiveOpacity => {
                Some((MIN_PANE_INACTIVE_OPACITY, MAX_PANE_INACTIVE_OPACITY))
            }
            Self::PaneMargin => Some((0.0, MAX_PANE_MARGIN)),
            Self::WidgetCornerRadius => Some((0.0, MAX_CORNER_RADIUS)),
            Self::ChromeContrast => Some((1.0, 2.0)),
            Self::AnimationSpeed => Some((MIN_ANIMATION_SPEED, MAX_ANIMATION_SPEED)),
            Self::ChromeTintHue => Some((0.0, 360.0)),
            Self::WindowCornerRadius => Some((0.0, MAX_WINDOW_CORNER_RADIUS)),
            Self::EditorFontSize => Some((MIN_EDITOR_FONT_SIZE, MAX_EDITOR_FONT_SIZE)),
            Self::WhichKeyDelay => Some((0.0, MAX_WHICH_KEY_DELAY)),
            Self::UseSystemTitlebar
            | Self::PaletteWindowLayout
            | Self::PaletteShowKeys
            | Self::PickerFocusSidebar
            | Self::WindowBackgroundBlur
            | Self::Animations
            | Self::Tray
            | Self::Notifications
            | Self::QuitDaemonOnExit
            | Self::AutoRestartStaleDaemon
            | Self::CheckForUpdates
            | Self::StatusBadges
            | Self::StatusHost
            | Self::StatusUpdate
            | Self::FlatSidebar
            | Self::ProgramColors
            | Self::ExperimentalEditorPane
            | Self::PaneGaps
            | Self::EditorLineNumbers
            | Self::EditorRelativeLineNumbers
            | Self::EditorSoftWrap
            | Self::EditorVimMode
            | Self::BrowserElementSelectorHotkey
            | Self::BrowserRemoteDebuggingPort
            | Self::BrowserSearchProvider
            | Self::BrowserEgress
            | Self::ThemeMode
            | Self::InterfaceTheme
            | Self::ChromeAccent
            | Self::UiFontFamily
            | Self::AppIcon
            | Self::DriverAgent
            | Self::DriverModel
            | Self::DriverEffort
            | Self::DriverHosts => None,
        }
    }

    /// The keys a picked theme replaces with its own: its corner radius. The
    /// accent and the tint stay.
    pub const THEME_PICKS: [Self; 1] = [Self::WidgetCornerRadius];
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConfigProvenance {
    #[default]
    Default,
    Override,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfigValue<T> {
    pub value: T,
    pub provenance: ConfigProvenance,
}

impl<T> ConfigValue<T> {
    pub const fn from_default(value: T) -> Self {
        Self {
            value,
            provenance: ConfigProvenance::Default,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BrowserConfig {
    pub element_selector_hotkey: ConfigValue<String>,
    pub remote_debugging_port: ConfigValue<Option<u16>>,
    pub search_provider: ConfigValue<SearchProvider>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            element_selector_hotkey: ConfigValue::from_default(
                DEFAULT_BROWSER_ELEMENT_SELECTOR_HOTKEY.to_owned(),
            ),
            remote_debugging_port: ConfigValue::from_default(None),
            search_provider: ConfigValue::from_default(DEFAULT_BROWSER_SEARCH_PROVIDER),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiFontConfig {
    pub family: ConfigValue<Option<String>>,
}

impl Default for UiFontConfig {
    fn default() -> Self {
        Self {
            family: ConfigValue::from_default(None),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaletteWindowLayout {
    #[default]
    Grouped,
    Flat,
}

impl PaletteWindowLayout {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grouped => "grouped",
            Self::Flat => "flat",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "grouped" => Some(Self::Grouped),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }
}

/// What zz starts the Driver Agent with: a Claude Code model, or Codex on the
/// model its own config names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DriverModel {
    /// Claude Code on the model its own settings pick.
    #[default]
    Claude,
    Opus,
    Sonnet,
    Fable,
    Codex,
}

impl DriverModel {
    pub const ALL: [Self; 5] = [
        Self::Claude,
        Self::Opus,
        Self::Sonnet,
        Self::Fable,
        Self::Codex,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Opus => "opus",
            Self::Sonnet => "sonnet",
            Self::Fable => "fable",
            Self::Codex => "codex",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code's default",
            Self::Opus => "Claude Opus",
            Self::Sonnet => "Claude Sonnet",
            Self::Fable => "Claude Fable",
            Self::Codex => "Codex's default",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|model| model.as_str() == value)
    }
}

/// How hard the driver thinks: the CLI's own setting, or one of the levels
/// Claude Code's `--effort` takes. Codex has no `max` and gets `xhigh`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DriverEffort {
    #[default]
    Default,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl DriverEffort {
    pub const ALL: [Self; 6] = [
        Self::Default,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
        Self::Max,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Default => "The CLI's default",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Xhigh => "Extra high",
            Self::Max => "Max",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|effort| effort.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppConfig {
    pub palette_window_layout: ConfigValue<PaletteWindowLayout>,
    pub palette_show_keys: ConfigValue<bool>,
    pub picker_focus_sidebar: ConfigValue<bool>,
    pub which_key_delay: ConfigValue<f32>,
    pub use_system_titlebar: ConfigValue<bool>,
    pub window_corner_radius: ConfigValue<f32>,
    pub window_background_blur: ConfigValue<bool>,
    pub window_background_opacity: ConfigValue<f32>,
    pub animations: ConfigValue<bool>,
    pub animation_speed: ConfigValue<f32>,
    pub tray: ConfigValue<bool>,
    pub notifications: ConfigValue<bool>,
    pub quit_daemon_on_exit: ConfigValue<bool>,
    pub auto_restart_stale_daemon: ConfigValue<bool>,
    pub check_for_updates: ConfigValue<bool>,
    pub status_badges: ConfigValue<bool>,
    pub status_host: ConfigValue<bool>,
    pub status_update: ConfigValue<bool>,
    pub flat_sidebar: ConfigValue<bool>,
    pub program_colors: ConfigValue<bool>,
    pub experimental_editor_pane: ConfigValue<bool>,
    pub pane_gaps: ConfigValue<bool>,
    pub pip_background_opacity: ConfigValue<f32>,
    pub pane_glow_strength: ConfigValue<f32>,
    pub pane_inactive_opacity: ConfigValue<f32>,
    pub pane_margin: ConfigValue<f32>,
    pub widget_corner_radius: ConfigValue<f32>,
    pub chrome_contrast: ConfigValue<f32>,
    pub chrome_accent: ConfigValue<AccentSetting>,
    pub chrome_tint: ConfigValue<f32>,
    pub chrome_tint_hue: ConfigValue<f32>,
    pub editor_font_size: ConfigValue<f32>,
    pub editor_line_numbers: ConfigValue<bool>,
    pub editor_relative_line_numbers: ConfigValue<bool>,
    pub editor_soft_wrap: ConfigValue<bool>,
    pub editor_vim_mode: ConfigValue<bool>,
    pub browser_egress: ConfigValue<bool>,
    pub theme_mode: ConfigValue<ThemeModeSetting>,
    /// The built-in theme `interface-theme` names, or the default when it
    /// names a saved one; see [`ParsedConfig::theme`].
    pub interface_theme: ConfigValue<&'static str>,
    pub app_icon: ConfigValue<AppIconSetting>,
    pub driver_agent: ConfigValue<bool>,
    pub driver_model: ConfigValue<DriverModel>,
    pub driver_effort: ConfigValue<DriverEffort>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let status_bar = StatusBarSettings::default();
        Self {
            palette_window_layout: ConfigValue::from_default(PaletteWindowLayout::Grouped),
            palette_show_keys: ConfigValue::from_default(true),
            picker_focus_sidebar: ConfigValue::from_default(false),
            which_key_delay: ConfigValue::from_default(DEFAULT_WHICH_KEY_DELAY),
            use_system_titlebar: ConfigValue::from_default(DEFAULT_USE_SYSTEM_TITLEBAR),
            window_corner_radius: ConfigValue::from_default(DEFAULT_WINDOW_CORNER_RADIUS),
            window_background_blur: ConfigValue::from_default(DEFAULT_WINDOW_BACKGROUND_BLUR),
            window_background_opacity: ConfigValue::from_default(DEFAULT_WINDOW_BACKGROUND_OPACITY),
            animations: ConfigValue::from_default(DEFAULT_ANIMATIONS),
            animation_speed: ConfigValue::from_default(DEFAULT_ANIMATION_SPEED),
            tray: ConfigValue::from_default(DEFAULT_TRAY),
            notifications: ConfigValue::from_default(DEFAULT_NOTIFICATIONS),
            quit_daemon_on_exit: ConfigValue::from_default(DEFAULT_QUIT_DAEMON_ON_EXIT),
            auto_restart_stale_daemon: ConfigValue::from_default(DEFAULT_AUTO_RESTART_STALE_DAEMON),
            check_for_updates: ConfigValue::from_default(DEFAULT_CHECK_FOR_UPDATES),
            status_badges: ConfigValue::from_default(status_bar.badges),
            status_host: ConfigValue::from_default(status_bar.show_host),
            status_update: ConfigValue::from_default(status_bar.show_update),
            flat_sidebar: ConfigValue::from_default(false),
            program_colors: ConfigValue::from_default(true),
            experimental_editor_pane: ConfigValue::from_default(DEFAULT_EXPERIMENTAL_EDITOR_PANE),
            pane_gaps: ConfigValue::from_default(DEFAULT_PANE_GAPS),
            pip_background_opacity: ConfigValue::from_default(DEFAULT_PIP_BACKGROUND_OPACITY),
            pane_glow_strength: ConfigValue::from_default(DEFAULT_PANE_GLOW_STRENGTH),
            pane_inactive_opacity: ConfigValue::from_default(DEFAULT_PANE_INACTIVE_OPACITY),
            pane_margin: ConfigValue::from_default(DEFAULT_PANE_MARGIN),
            widget_corner_radius: ConfigValue::from_default(DEFAULT_WIDGET_CORNER_RADIUS),
            chrome_contrast: ConfigValue::from_default(DEFAULT_CHROME_CONTRAST),
            chrome_accent: ConfigValue::from_default(AccentSetting::Theme),
            chrome_tint: ConfigValue::from_default(DEFAULT_CHROME_TINT),
            chrome_tint_hue: ConfigValue::from_default(DEFAULT_TINT_HUE),
            editor_font_size: ConfigValue::from_default(DEFAULT_EDITOR_FONT_SIZE),
            editor_line_numbers: ConfigValue::from_default(DEFAULT_EDITOR_LINE_NUMBERS),
            editor_relative_line_numbers: ConfigValue::from_default(
                DEFAULT_EDITOR_RELATIVE_LINE_NUMBERS,
            ),
            editor_soft_wrap: ConfigValue::from_default(DEFAULT_EDITOR_SOFT_WRAP),
            editor_vim_mode: ConfigValue::from_default(DEFAULT_EDITOR_VIM_MODE),
            browser_egress: ConfigValue::from_default(DEFAULT_BROWSER_EGRESS),
            theme_mode: ConfigValue::from_default(ThemeModeSetting::System),
            interface_theme: ConfigValue::from_default(DEFAULT_THEME),
            app_icon: ConfigValue::from_default(AppIconSetting::Automatic),
            driver_agent: ConfigValue::from_default(false),
            driver_model: ConfigValue::from_default(DriverModel::Claude),
            driver_effort: ConfigValue::from_default(DriverEffort::Default),
        }
    }
}

impl AppConfig {
    /// Shows the active theme's radius in `widget-corner-radius` while
    /// nothing overrides it.
    pub fn follow_theme_radius(&mut self, radius: f32) {
        if self.widget_corner_radius.provenance == ConfigProvenance::Default {
            self.widget_corner_radius.value = radius;
        }
    }

    fn boolean_value_mut(&mut self, key: ConfigKey) -> Option<&mut ConfigValue<bool>> {
        match key {
            ConfigKey::PaletteShowKeys => Some(&mut self.palette_show_keys),
            ConfigKey::PickerFocusSidebar => Some(&mut self.picker_focus_sidebar),
            ConfigKey::UseSystemTitlebar => Some(&mut self.use_system_titlebar),
            ConfigKey::WindowBackgroundBlur => Some(&mut self.window_background_blur),
            ConfigKey::Animations => Some(&mut self.animations),
            ConfigKey::Tray => Some(&mut self.tray),
            ConfigKey::Notifications => Some(&mut self.notifications),
            ConfigKey::QuitDaemonOnExit => Some(&mut self.quit_daemon_on_exit),
            ConfigKey::AutoRestartStaleDaemon => Some(&mut self.auto_restart_stale_daemon),
            ConfigKey::CheckForUpdates => Some(&mut self.check_for_updates),
            ConfigKey::StatusBadges => Some(&mut self.status_badges),
            ConfigKey::StatusHost => Some(&mut self.status_host),
            ConfigKey::StatusUpdate => Some(&mut self.status_update),
            ConfigKey::FlatSidebar => Some(&mut self.flat_sidebar),
            ConfigKey::ProgramColors => Some(&mut self.program_colors),
            ConfigKey::ExperimentalEditorPane => Some(&mut self.experimental_editor_pane),
            ConfigKey::PaneGaps => Some(&mut self.pane_gaps),
            ConfigKey::EditorLineNumbers => Some(&mut self.editor_line_numbers),
            ConfigKey::EditorRelativeLineNumbers => Some(&mut self.editor_relative_line_numbers),
            ConfigKey::EditorSoftWrap => Some(&mut self.editor_soft_wrap),
            ConfigKey::EditorVimMode => Some(&mut self.editor_vim_mode),
            ConfigKey::BrowserEgress => Some(&mut self.browser_egress),
            ConfigKey::DriverAgent => Some(&mut self.driver_agent),
            ConfigKey::WindowCornerRadius
            | ConfigKey::WhichKeyDelay
            | ConfigKey::PaletteWindowLayout
            | ConfigKey::PipBackgroundOpacity
            | ConfigKey::WindowBackgroundOpacity
            | ConfigKey::PaneGlowStrength
            | ConfigKey::PaneInactiveOpacity
            | ConfigKey::PaneMargin
            | ConfigKey::WidgetCornerRadius
            | ConfigKey::ChromeContrast
            | ConfigKey::AnimationSpeed
            | ConfigKey::ChromeAccent
            | ConfigKey::ChromeTint
            | ConfigKey::ChromeTintHue
            | ConfigKey::EditorFontSize
            | ConfigKey::BrowserElementSelectorHotkey
            | ConfigKey::BrowserRemoteDebuggingPort
            | ConfigKey::BrowserSearchProvider
            | ConfigKey::ThemeMode
            | ConfigKey::InterfaceTheme
            | ConfigKey::UiFontFamily
            | ConfigKey::AppIcon
            | ConfigKey::DriverModel
            | ConfigKey::DriverEffort
            | ConfigKey::DriverHosts => None,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ConfigDiagnostic {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct ParsedConfig {
    pub config: AppConfig,
    pub browser: BrowserConfig,
    pub ui_font: UiFontConfig,
    /// The saved theme `interface-theme` names, when it names one.
    pub theme: Option<String>,
    pub hosts: Vec<HostEntry>,
    pub rejected_hosts: Vec<RejectedHost>,
    pub driver_hosts: Vec<String>,
    pub daemon_entries: Vec<ConfigOverrideEntry>,
    pub chrome_overrides: Vec<ChromeOverride>,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ImportError {
    UnserializableValue { key: &'static str, reason: String },
    TooLarge { bytes: usize },
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnserializableValue { key, reason } => {
                write!(formatter, "cannot import `{key}`: {reason}")
            }
            Self::TooLarge { bytes } => write!(
                formatter,
                "configuration import would produce {bytes} bytes, exceeding the {MAX_CONFIG_BYTES}-byte limit"
            ),
        }
    }
}

impl std::error::Error for ImportError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigFileStamp {
    pub path: Option<PathBuf>,
    pub modified: Option<SystemTime>,
    pub len: Option<u64>,
    pub themes: themes::ThemesStamp,
}

impl ConfigFileStamp {
    pub fn detect(candidates: &[PathBuf]) -> Self {
        let Some(path) = discover_config_path(candidates) else {
            return Self::default();
        };
        let metadata = fs::metadata(&path).ok();
        Self {
            modified: metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok()),
            len: metadata.as_ref().map(fs::Metadata::len),
            themes: themes::themes_stamp(&themes::themes_directory_for(&path)),
            path: Some(path),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigPlatform {
    Unix,

    Macos,

    Windows,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ConfigEnvironment<'a> {
    pub xdg_config_home: Option<&'a Path>,
    pub home: Option<&'a Path>,

    pub appdata: Option<&'a Path>,

    pub local_appdata: Option<&'a Path>,

    pub user_profile: Option<&'a Path>,
}

pub fn current_config_platform() -> ConfigPlatform {
    #[cfg(target_os = "macos")]
    {
        ConfigPlatform::Macos
    }
    #[cfg(target_os = "windows")]
    {
        ConfigPlatform::Windows
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        ConfigPlatform::Unix
    }
}

pub fn config_candidates() -> Vec<PathBuf> {
    let xdg_config_home = nonempty_env("XDG_CONFIG_HOME");
    let home = nonempty_env("HOME");

    let appdata = nonempty_env("APPDATA");

    let local_appdata = nonempty_env("LOCALAPPDATA");

    let user_profile = nonempty_env("USERPROFILE");

    config_candidates_for(
        current_config_platform(),
        ConfigEnvironment {
            xdg_config_home: xdg_config_home.as_deref(),
            home: home.as_deref(),

            appdata: appdata.as_deref(),

            local_appdata: local_appdata.as_deref(),

            user_profile: user_profile.as_deref(),
        },
    )
}

pub fn config_candidates_for(
    platform: ConfigPlatform,
    environment: ConfigEnvironment<'_>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    push_config_candidate(&mut candidates, environment.xdg_config_home);

    match platform {
        ConfigPlatform::Unix => {
            push_home_config_candidate(&mut candidates, environment.home);
        }

        ConfigPlatform::Macos => {
            push_home_config_candidate(&mut candidates, environment.home);
            if let Some(home) = environment.home {
                push_config_candidate_path(
                    &mut candidates,
                    &home.join("Library/Application Support"),
                );
            }
        }

        ConfigPlatform::Windows => {
            push_config_candidate(&mut candidates, environment.appdata);
            push_config_candidate(&mut candidates, environment.local_appdata);
            push_home_config_candidate(&mut candidates, environment.user_profile);
            push_home_config_candidate(&mut candidates, environment.home);
        }
    }

    candidates
}

pub fn push_home_config_candidate(candidates: &mut Vec<PathBuf>, home: Option<&Path>) {
    if let Some(home) = home {
        push_config_candidate_path(candidates, &home.join(".config"));
    }
}

pub fn push_config_candidate(candidates: &mut Vec<PathBuf>, base: Option<&Path>) {
    let Some(base) = base else {
        return;
    };
    push_config_candidate_path(candidates, base);
}

pub fn push_config_candidate_path(candidates: &mut Vec<PathBuf>, base: &Path) {
    if !base.is_absolute() {
        return;
    }
    let candidate = base.join(CONFIG_DIRECTORY_NAME).join(CONFIG_FILE_NAME);
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

pub fn discover_config_path(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
}

pub fn config_path_for_write() -> io::Result<PathBuf> {
    config_write_path(&config_candidates())
}

pub fn config_write_path(candidates: &[PathBuf]) -> io::Result<PathBuf> {
    discover_config_path(candidates)
        .or_else(|| candidates.first().cloned())
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "cannot create configuration because no absolute platform configuration directory is available",
            )
        })
}

pub fn import_target_path() -> io::Result<PathBuf> {
    config_path_for_write()
}

pub fn nonempty_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn load_config(path: &Path, system_font_family: &str) -> io::Result<ParsedConfig> {
    read_config_source(path).map(|source| parse_config(&source, system_font_family))
}

pub fn read_config_source(path: &Path) -> io::Result<String> {
    let file = File::open(path)?;
    let byte_limit = u64::try_from(MAX_CONFIG_BYTES).unwrap_or(u64::MAX - 1);
    let mut source = String::new();
    file.take(byte_limit + 1).read_to_string(&mut source)?;
    if source.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("configuration exceeds the {MAX_CONFIG_BYTES}-byte limit"),
        ));
    }
    Ok(source)
}

/// Read a settings editor file, capped at `max_bytes`. A missing file starts as
/// an empty editor.
pub fn read_config_editor_source(path: &Path, max_bytes: usize) -> io::Result<String> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    };
    let byte_limit = u64::try_from(max_bytes).unwrap_or(u64::MAX - 1);
    let mut source = String::new();
    file.take(byte_limit + 1).read_to_string(&mut source)?;
    if source.len() > max_bytes {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("configuration exceeds the {max_bytes}-byte editor limit"),
        ));
    }
    Ok(source)
}

/// Atomically replace a settings editor file after enforcing its surface's cap.
pub fn write_config_editor_source(path: &Path, source: &str, max_bytes: usize) -> io::Result<()> {
    if source.len() > max_bytes {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("configuration exceeds the {max_bytes}-byte editor limit"),
        ));
    }
    atomic_write(path, source.as_bytes())
}

pub fn parse_config(source: &str, system_font_family: &str) -> ParsedConfig {
    let mut parsed = ParsedConfig::default();

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: "expected `key = value`".to_owned(),
            });
            continue;
        };
        let key = key.trim();
        let value = config_value_without_comment(value).trim();

        if let Some(name) = key.strip_prefix("host-") {
            if let Some(message) = zz_daemon_client::apply_fleet_host_entry(
                &mut parsed.hosts,
                &mut parsed.rejected_hosts,
                key,
                name,
                value,
            ) {
                parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message,
                });
            }
            continue;
        }

        if matches!(key, CHROME_KEYBIND_KEY | CHROME_UNBIND_KEY) {
            let entry = if key == CHROME_KEYBIND_KEY {
                keymap::parse_bind(value)
            } else {
                keymap::parse_unbind(value)
            };
            match entry {
                Ok(entry) => parsed.chrome_overrides.push(entry),
                Err(message) => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!("invalid `{key}`: {message}"),
                }),
            }
            continue;
        }

        if RETIRED_APPEARANCE_KEYS.contains(&key) {
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!(
                    "`{key}` was removed: a theme now bundles the look with its colors; pick one in Settings > Interface or set `interface-theme`"
                ),
            });
            continue;
        }

        if RETIRED_LOOK_KEYS.contains(&key) {
            let instead = if key == "shadow-strength" {
                "pick a theme with `interface-theme`"
            } else {
                "themes set this per role now"
            };
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!("`{key}` was removed: {instead}"),
            });
            continue;
        }

        if RETIRED_STATUS_KEYS.contains(&key) {
            let instead = if key == "status-agents" {
                "the status bar no longer shows agent activity; the sidebar shows each agent's state"
            } else {
                "the status bar no longer has a session menu; switch sessions from the sidebar"
            };
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!("`{key}` was removed: {instead}"),
            });
            continue;
        }

        if RETIRED_MATERIAL_KEYS.contains(&key) {
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!(
                    "`{key}` was removed: pick a theme with `interface-theme`, or make one in the theme creator"
                ),
            });
            continue;
        }

        if RETIRED_AGENT_KEYS.contains(&key) {
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!(
                    "`{key}` was removed: run Claude Code or Codex in a terminal pane and open its lens from the pane header"
                ),
            });
            continue;
        }

        if RETIRED_PANE_KEYS.contains(&key) {
            let instead = match key {
                "pane-corner-radius" => "pane corners follow `widget-corner-radius`",
                "pane-border-width" => {
                    "the pane edge follows the theme's glass, or its outline width without glass"
                }
                "pane-background-opacity" => {
                    "panes always show the window behind them; `pip-background-opacity` sets how much of its color a PiP lays over what is under it"
                }
                _ => "gapped panes always draw a glass frame",
            };
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!("`{key}` was removed: {instead}"),
            });
            continue;
        }

        let Some(key) = ConfigKey::parse(key) else {
            if AppearanceConfigKey::from_config_key(key).is_some()
                || MuxOptionKey::from_config_key(key).is_some()
            {
                parsed
                    .daemon_entries
                    .push((key.to_owned(), value.to_owned()));
                continue;
            }
            parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!("unsupported key `{key}`"),
            });
            continue;
        };

        if matches!(key, ConfigKey::ExperimentalEditorPane) {
            parsed
                .daemon_entries
                .push((key.as_str().to_owned(), value.to_owned()));
        }

        if let Some(target) = parsed.config.boolean_value_mut(key) {
            target.provenance = ConfigProvenance::Override;
            match parse_boolean(value) {
                Ok(enabled) => target.value = enabled,
                Err(message) => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!("invalid `{}`: {message}", key.as_str()),
                }),
            }
            continue;
        }

        if key == ConfigKey::PaletteWindowLayout {
            let target = &mut parsed.config.palette_window_layout;
            target.provenance = ConfigProvenance::Override;
            match PaletteWindowLayout::parse(value) {
                Some(layout) => target.value = layout,
                None => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: "invalid `palette-window-layout`: expected grouped or flat".to_owned(),
                }),
            }
            continue;
        }

        if key == ConfigKey::DriverModel {
            let target = &mut parsed.config.driver_model;
            target.provenance = ConfigProvenance::Override;
            match DriverModel::parse(value) {
                Some(model) => target.value = model,
                None => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!(
                        "invalid `driver-model`: expected one of {}",
                        DriverModel::ALL.map(DriverModel::as_str).join(", "),
                    ),
                }),
            }
            continue;
        }

        if key == ConfigKey::DriverHosts {
            parsed.driver_hosts = value
                .split(|character: char| character == ',' || character.is_whitespace())
                .filter(|host| !host.is_empty())
                .map(str::to_owned)
                .collect();
            continue;
        }

        if key == ConfigKey::DriverEffort {
            let target = &mut parsed.config.driver_effort;
            target.provenance = ConfigProvenance::Override;
            match DriverEffort::parse(value) {
                Some(effort) => target.value = effort,
                None => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!(
                        "invalid `driver-effort`: expected one of {}",
                        DriverEffort::ALL.map(DriverEffort::as_str).join(", "),
                    ),
                }),
            }
            continue;
        }

        if key == ConfigKey::UiFontFamily {
            let target = &mut parsed.ui_font.family;
            target.provenance = ConfigProvenance::Override;
            match parse_config_string(value).and_then(|family| {
                validate_ui_font_family(&family).map_err(str::to_owned)?;
                Ok(family)
            }) {
                Ok(family) => {
                    target.value = (family != system_font_family).then_some(family);
                }
                Err(message) => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!("invalid `{}`: {message}", key.as_str()),
                }),
            }
            continue;
        }

        if key == ConfigKey::BrowserElementSelectorHotkey {
            let target = &mut parsed.browser.element_selector_hotkey;
            target.provenance = ConfigProvenance::Override;
            match normalize_browser_hotkey(value) {
                Ok(value) => target.value = value,
                Err(message) => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!("invalid `{}`: {message}", key.as_str()),
                }),
            }
            continue;
        }

        if key == ConfigKey::BrowserRemoteDebuggingPort {
            let target = &mut parsed.browser.remote_debugging_port;
            target.provenance = ConfigProvenance::Override;
            match value.parse::<u16>() {
                Ok(0) => target.value = None,
                Ok(port @ 1024..=65535) => target.value = Some(port),
                _ => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!(
                        "invalid `{}`: expected 0 or an integer from 1024 to 65535",
                        key.as_str(),
                    ),
                }),
            }
            continue;
        }

        if key == ConfigKey::BrowserSearchProvider {
            let target = &mut parsed.browser.search_provider;
            target.provenance = ConfigProvenance::Override;
            match SearchProvider::parse(value) {
                Some(provider) => target.value = provider,
                None => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!(
                        "invalid `{}`: expected one of {}",
                        key.as_str(),
                        SearchProvider::ALL.map(SearchProvider::as_str).join(", "),
                    ),
                }),
            }
            continue;
        }

        if key == ConfigKey::InterfaceTheme {
            parsed.config.interface_theme.provenance = ConfigProvenance::Override;
            let name = value.trim();
            match BUILTIN_THEMES.iter().find(|builtin| **builtin == name) {
                Some(builtin) => {
                    parsed.config.interface_theme.value = builtin;
                    parsed.theme = None;
                }
                None if themes::is_theme_name(name) => {
                    parsed.config.interface_theme.value = DEFAULT_THEME;
                    parsed.theme = Some(name.to_owned());
                }
                None => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!(
                        "invalid `interface-theme`: expected {} or the name of a saved theme",
                        BUILTIN_THEMES.join(", ")
                    ),
                }),
            }
            continue;
        }

        if key == ConfigKey::ChromeAccent {
            parsed.config.chrome_accent.provenance = ConfigProvenance::Override;
            match AccentSetting::parse(value) {
                Some(accent) => parsed.config.chrome_accent.value = accent,
                None => parsed.diagnostics.push(ConfigDiagnostic {
                    line: line_number,
                    message: format!(
                        "invalid `chrome-accent`: expected theme, {} or a #rrggbb color",
                        ACCENTS.join(", ")
                    ),
                }),
            }
            continue;
        }

        if let Some(diagnostic) = apply_theme_key(&mut parsed.config, key, value, line_number) {
            parsed.diagnostics.push(diagnostic);
            continue;
        }
        if matches!(
            key,
            ConfigKey::ThemeMode | ConfigKey::InterfaceTheme | ConfigKey::AppIcon
        ) {
            continue;
        }

        let target = match key {
            ConfigKey::WindowCornerRadius => &mut parsed.config.window_corner_radius,
            ConfigKey::PipBackgroundOpacity => &mut parsed.config.pip_background_opacity,
            ConfigKey::WindowBackgroundOpacity => &mut parsed.config.window_background_opacity,
            ConfigKey::PaneGlowStrength => &mut parsed.config.pane_glow_strength,
            ConfigKey::PaneInactiveOpacity => &mut parsed.config.pane_inactive_opacity,
            ConfigKey::PaneMargin => &mut parsed.config.pane_margin,
            ConfigKey::WidgetCornerRadius => &mut parsed.config.widget_corner_radius,
            ConfigKey::ChromeContrast => &mut parsed.config.chrome_contrast,
            ConfigKey::AnimationSpeed => &mut parsed.config.animation_speed,
            ConfigKey::ChromeTint => &mut parsed.config.chrome_tint,
            ConfigKey::ChromeTintHue => &mut parsed.config.chrome_tint_hue,
            ConfigKey::EditorFontSize => &mut parsed.config.editor_font_size,
            ConfigKey::WhichKeyDelay => &mut parsed.config.which_key_delay,
            ConfigKey::UseSystemTitlebar
            | ConfigKey::PaletteWindowLayout
            | ConfigKey::PaletteShowKeys
            | ConfigKey::PickerFocusSidebar
            | ConfigKey::WindowBackgroundBlur
            | ConfigKey::Animations
            | ConfigKey::Tray
            | ConfigKey::Notifications
            | ConfigKey::QuitDaemonOnExit
            | ConfigKey::AutoRestartStaleDaemon
            | ConfigKey::CheckForUpdates
            | ConfigKey::StatusBadges
            | ConfigKey::ExperimentalEditorPane
            | ConfigKey::PaneGaps
            | ConfigKey::EditorLineNumbers
            | ConfigKey::EditorRelativeLineNumbers
            | ConfigKey::EditorSoftWrap
            | ConfigKey::EditorVimMode
            | ConfigKey::BrowserElementSelectorHotkey
            | ConfigKey::BrowserRemoteDebuggingPort
            | ConfigKey::BrowserSearchProvider
            | ConfigKey::BrowserEgress
            | ConfigKey::StatusHost
            | ConfigKey::StatusUpdate
            | ConfigKey::FlatSidebar
            | ConfigKey::ProgramColors
            | ConfigKey::ThemeMode
            | ConfigKey::InterfaceTheme
            | ConfigKey::ChromeAccent
            | ConfigKey::UiFontFamily
            | ConfigKey::AppIcon
            | ConfigKey::DriverAgent
            | ConfigKey::DriverModel
            | ConfigKey::DriverEffort
            | ConfigKey::DriverHosts => {
                unreachable!("handled above")
            }
        };
        if key != ConfigKey::WidgetCornerRadius {
            target.provenance = ConfigProvenance::Override;
        }

        let range = key
            .numeric_range()
            .expect("every key reaching here is a numeric value");
        match parse_numeric_value(key, value, range) {
            Ok(value) => {
                target.value = value;
                target.provenance = ConfigProvenance::Override;
            }
            Err(message) => parsed.diagnostics.push(ConfigDiagnostic {
                line: line_number,
                message: format!("invalid `{}`: {message}", key.as_str()),
            }),
        }
    }

    parsed
}

pub fn write_fleet_host(name: &str, endpoint: &str) -> io::Result<()> {
    let path = config_path_for_write()?;
    write_fleet_host_at(&path, name, endpoint)
}

pub fn write_fleet_host_at(path: &Path, name: &str, endpoint: &str) -> io::Result<()> {
    validate_fleet_host(name, endpoint)
        .map_err(|message| io::Error::new(ErrorKind::InvalidInput, message))?;
    write_config_edit_at(path, &format!("host-{name}"), Some(endpoint)).map(drop)
}

pub fn remove_fleet_host(name: &str) -> io::Result<bool> {
    let path = config_path_for_write()?;
    remove_fleet_host_at(&path, name)
}

pub fn remove_fleet_host_at(path: &Path, name: &str) -> io::Result<bool> {
    let key = format!("host-{name}");
    let mut removed = false;
    while write_config_edit_at(path, &key, None)? {
        removed = true;
    }
    Ok(removed)
}

pub fn apply_theme_key(
    config: &mut AppConfig,
    key: ConfigKey,
    value: &str,
    line_number: usize,
) -> Option<ConfigDiagnostic> {
    let message = match key {
        ConfigKey::ThemeMode => {
            config.theme_mode.provenance = ConfigProvenance::Override;
            match ThemeModeSetting::parse(value) {
                Some(mode) => {
                    config.theme_mode.value = mode;
                    return None;
                }
                None => "expected system, light or dark".to_owned(),
            }
        }
        ConfigKey::AppIcon => {
            config.app_icon.provenance = ConfigProvenance::Override;
            match AppIconSetting::parse(value) {
                Some(setting) => {
                    config.app_icon.value = setting;
                    return None;
                }
                None => "expected automatic, light or dark".to_owned(),
            }
        }
        _ => return None,
    };
    Some(ConfigDiagnostic {
        line: line_number,
        message: format!("invalid `{}`: {message}", key.as_str()),
    })
}

pub fn config_value_without_comment(value: &str) -> &str {
    config_comment_start(value).map_or(value, |index| &value[..index])
}

pub fn config_comment_start(value: &str) -> Option<usize> {
    let mut quote = None;
    let mut escaped = false;
    let mut saw_value = false;
    let mut previous_was_whitespace = false;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
            saw_value |= !character.is_whitespace();
            previous_was_whitespace = character.is_whitespace();
            continue;
        }
        match character {
            '\\' if quote.is_some() => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '#' if quote.is_none() && saw_value && previous_was_whitespace => {
                return Some(index);
            }
            _ => {}
        }
        saw_value |= !character.is_whitespace();
        previous_was_whitespace = character.is_whitespace();
    }
    None
}

pub fn normalize_browser_hotkey(value: &str) -> Result<String, String> {
    let hotkey = keymap::parse_keystroke(value.trim()).ok_or_else(|| {
        "expected modifiers and a key, for example `cmd-shift-c` or `ctrl-shift-c`".to_owned()
    })?;
    if !(hotkey.key.control || hotkey.key.alt || hotkey.key.command || hotkey.function) {
        return Err("expected Control, Alt, Command/Super, or Function as a modifier".into());
    }
    Ok(hotkey.spelling())
}

pub fn parse_numeric_value(
    key: ConfigKey,
    value: &str,
    (min, max): (f32, f32),
) -> Result<f32, String> {
    let unit = if matches!(
        key,
        ConfigKey::PaneInactiveOpacity
            | ConfigKey::PipBackgroundOpacity
            | ConfigKey::WindowBackgroundOpacity
            | ConfigKey::PaneGlowStrength
            | ConfigKey::ChromeContrast
            | ConfigKey::AnimationSpeed
            | ConfigKey::ChromeTint
    ) {
        ""
    } else if key == ConfigKey::WhichKeyDelay {
        " milliseconds"
    } else if key == ConfigKey::ChromeTintHue {
        " degrees"
    } else {
        " logical pixels"
    };
    let value = value
        .parse::<f32>()
        .map_err(|_| format!("expected a number{unit}"))?;
    if !value.is_finite() {
        return Err("value must be finite".to_owned());
    }
    if !(min..=max).contains(&value) {
        return Err(format!("value must be between {min} and {max}{unit}"));
    }
    Ok(value)
}

pub fn parse_boolean(value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "on" | "yes" | "1" | "true" => Ok(true),
        "off" | "no" | "0" | "false" => Ok(false),
        _ => Err("expected a boolean (`true`/`on`/`yes`/`1` or `false`/`off`/`no`/`0`)".to_owned()),
    }
}

pub fn parse_config_string(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("value must not be empty".to_owned());
    }
    let bytes = value.as_bytes();
    if matches!(bytes.first(), Some(b'\'' | b'"')) {
        let quote = bytes[0];
        if bytes.last() != Some(&quote) || bytes.len() < 2 {
            return Err("unterminated quoted value".to_owned());
        }
        let mut parsed = String::with_capacity(value.len().saturating_sub(2));
        let mut escaped = false;
        for character in value[1..value.len() - 1].chars() {
            if escaped {
                parsed.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else {
                parsed.push(character);
            }
        }
        if escaped {
            return Err("quoted value ends with an incomplete escape".to_owned());
        }
        if parsed.is_empty() {
            return Err("value must not be empty".to_owned());
        }
        Ok(parsed)
    } else {
        Ok(value.to_owned())
    }
}

/// Write imported appearance values into `zz/config`, donor wins: each key is
/// replaced in place when it exists and appended otherwise. Cumulative keys
/// drop every prior occurrence; an empty group is a pure removal.
pub fn import_appearance_values_at(
    path: &Path,
    values: &[(AppearanceConfigKey, Vec<String>)],
) -> io::Result<()> {
    let source = match read_config_source(path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let edited = apply_import_edits(&source, values)?;
    atomic_write(path, edited.as_bytes())
}

pub fn apply_import_edits(
    source: &str,
    values: &[(AppearanceConfigKey, Vec<String>)],
) -> io::Result<String> {
    let mut edited = source.to_owned();
    for (key, group) in values {
        edited = if !is_cumulative_appearance_key(*key)
            && let [value] = group.as_slice()
        {
            edit_config_source(&edited, key.as_str(), Some(value))
        } else {
            replace_config_key_group(&edited, key.as_str(), group)
        };
        if edited.len() > MAX_CONFIG_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                ImportError::TooLarge {
                    bytes: edited.len(),
                }
                .to_string(),
            ));
        }
    }
    Ok(edited)
}

pub fn is_cumulative_appearance_key(key: AppearanceConfigKey) -> bool {
    matches!(
        key,
        AppearanceConfigKey::Palette
            | AppearanceConfigKey::FontFamily
            | AppearanceConfigKey::FontFamilyBold
            | AppearanceConfigKey::FontFamilyItalic
            | AppearanceConfigKey::FontFamilyBoldItalic
            | AppearanceConfigKey::FontFeature
    )
}

/// Appearance keys an older zz wrote. Parsing reports them, and picking a theme
/// in Settings removes them.
pub const RETIRED_APPEARANCE_KEYS: [&str; 8] = [
    "chrome-preset",
    "chrome-preset-light",
    "chrome-preset-dark",
    "chrome-background",
    "chrome-foreground",
    "chrome-success",
    "chrome-warning",
    "chrome-danger",
];

/// Material keys an older zz read, now a theme's to set. Parsing reports
/// them, and picking a theme in Settings removes them.
pub const RETIRED_MATERIAL_KEYS: [&str; 6] = [
    "interface-style",
    "widget-outline-width",
    "widget-shadow-strength",
    "widget-row-inset",
    "widget-primary",
    "widget-glass",
];

/// Look keys an older zz read. Parsing reports them and otherwise ignores them.
pub const RETIRED_LOOK_KEYS: [&str; 4] = [
    "shadow-strength",
    "widget-shadows",
    "widget-selection",
    "widget-field",
];

/// Pane keys an older zz read. Parsing reports them and otherwise ignores them.
pub const RETIRED_PANE_KEYS: [&str; 4] = [
    "pane-glass",
    "pane-corner-radius",
    "pane-border-width",
    "pane-background-opacity",
];

/// Agent pane keys an older zz read. Parsing reports them and otherwise ignores them.
pub const RETIRED_AGENT_KEYS: [&str; 2] = ["experimental-agent-pane", "agent-working-directory"];

/// Status bar keys an older zz read. Parsing reports them and otherwise ignores them.
pub const RETIRED_STATUS_KEYS: [&str; 2] = ["status-show-session", "status-agents"];

pub fn is_app_config_key(key: &str) -> bool {
    ConfigKey::parse(key).is_some()
        || RETIRED_APPEARANCE_KEYS.contains(&key)
        || RETIRED_PANE_KEYS.contains(&key)
        || RETIRED_STATUS_KEYS.contains(&key)
        || RETIRED_LOOK_KEYS.contains(&key)
        || RETIRED_MATERIAL_KEYS.contains(&key)
        || RETIRED_AGENT_KEYS.contains(&key)
        || MuxOptionKey::from_config_key(key).is_some()
        || key.starts_with("host-")
}

/// The Terminal page's editor buffer: `zz/config` with every app-side line
/// removed. Comments, blank lines, and unrecognized keys stay visible.
pub fn appearance_editor_view(source: &str) -> String {
    source
        .split_inclusive('\n')
        .filter(|line| !config_key_for_line(line).is_some_and(is_app_config_key))
        .collect()
}

/// Replace the appearance view of the file at `path` with `edited`, keeping every
/// app-side line. The inverse of [`appearance_editor_view`]. App-side keys in the
/// buffer are rejected, not spliced.
pub fn save_appearance_editor(path: &Path, edited: &str) -> io::Result<()> {
    for (index, line) in edited.lines().enumerate() {
        if let Some(key) = config_key_for_line(line).filter(|key| is_app_config_key(key)) {
            return Err(io::Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "line {}: `{key}` is managed by the other Settings pages, not this editor",
                    index + 1
                ),
            ));
        }
    }
    let source = match read_config_source(path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut merged: String = source
        .split_inclusive('\n')
        .filter(|line| config_key_for_line(line).is_some_and(is_app_config_key))
        .collect();
    if !merged.is_empty() && !merged.ends_with('\n') {
        merged.push('\n');
    }
    merged.push_str(edited);
    if merged.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("configuration edit exceeds the {MAX_CONFIG_BYTES}-byte limit"),
        ));
    }
    atomic_write(path, merged.as_bytes())
}

pub fn replace_config_key_group(source: &str, key: &str, values: &[String]) -> String {
    let mut edited = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        if config_key_for_line(line) != Some(key) {
            edited.push_str(line);
        }
    }
    for value in values {
        edited = append_config_line(&edited, key, value);
    }
    edited
}

pub fn appearance_config_values(
    appearance: &TerminalAppearance,
    key: AppearanceConfigKey,
) -> Result<Vec<String>, ImportError> {
    let values = match key {
        AppearanceConfigKey::Theme => Vec::new(),
        AppearanceConfigKey::Background => vec![serialize_rgb(appearance.background)],
        AppearanceConfigKey::Foreground => vec![serialize_rgb(appearance.foreground)],
        AppearanceConfigKey::CursorColor => vec![serialize_rgb(appearance.cursor_color)],
        AppearanceConfigKey::SelectionForeground => {
            vec![serialize_rgb(appearance.selection_foreground)]
        }
        AppearanceConfigKey::SelectionBackground => {
            vec![serialize_rgba(appearance.selection_background)]
        }
        AppearanceConfigKey::Palette => {
            let defaults = TerminalAppearance::default();
            appearance
                .palette
                .as_array()
                .iter()
                .zip(defaults.palette.as_array())
                .enumerate()
                .filter(|(_, (color, default))| color != default)
                .map(|(index, (color, _))| format!("{index}={}", serialize_rgb(*color)))
                .collect()
        }
        AppearanceConfigKey::FontFamily => serialize_font_families(&appearance.font_families),
        AppearanceConfigKey::FontFamilyBold => {
            serialize_font_families(&appearance.font_families_bold)
        }
        AppearanceConfigKey::FontFamilyItalic => {
            serialize_font_families(&appearance.font_families_italic)
        }
        AppearanceConfigKey::FontFamilyBoldItalic => {
            serialize_font_families(&appearance.font_families_bold_italic)
        }
        AppearanceConfigKey::FontSize => vec![appearance.font_size_points.to_string()],
        AppearanceConfigKey::FontFeature => {
            let mut seen_tags = Vec::with_capacity(appearance.font_features.len());
            let mut values = Vec::new();
            for feature in &appearance.font_features {
                if seen_tags.contains(&feature.tag) {
                    return Err(ImportError::UnserializableValue {
                        key: key.as_str(),
                        reason: format!(
                            "resolved appearance contains duplicate `{}` feature tags",
                            feature.tag_string()
                        ),
                    });
                }
                seen_tags.push(feature.tag);
                values.push(format!("{}={}", feature.tag_string(), feature.value));
            }
            values
        }
        AppearanceConfigKey::FontSyntheticStyle => {
            vec![serialize_font_synthetic_style(
                appearance.font_synthetic_style,
            )]
        }
        AppearanceConfigKey::FontThicken => vec![appearance.font_thicken.to_string()],
        AppearanceConfigKey::FontThickenStrength => {
            vec![appearance.font_thicken_strength.to_string()]
        }
        AppearanceConfigKey::AdjustCellHeight => match appearance.cell_height_adjustment {
            CellHeightAdjustment::None => vec![String::new()],
            CellHeightAdjustment::Pixels(value) => vec![value.to_string()],
            CellHeightAdjustment::Percent(value) => vec![format!("{value}%")],
        },
        AppearanceConfigKey::WindowPaddingX => vec![format!(
            "{},{}",
            appearance.padding_left, appearance.padding_right
        )],
        AppearanceConfigKey::WindowPaddingY => vec![format!(
            "{},{}",
            appearance.padding_top, appearance.padding_bottom
        )],
        AppearanceConfigKey::MinimumContrast => vec![appearance.minimum_contrast.to_string()],
        AppearanceConfigKey::BackgroundOpacity => vec![appearance.background_opacity.to_string()],
        AppearanceConfigKey::CursorStyle => vec![
            match appearance.cursor_style {
                CursorStyle::Bar => "bar",
                CursorStyle::Block => "block",
                CursorStyle::Underline => "underline",
                CursorStyle::BlockHollow => "block_hollow",
            }
            .to_owned(),
        ],
        AppearanceConfigKey::CursorStyleBlink => vec![
            match appearance.cursor_blink_policy {
                CursorBlinkPolicy::Off => "false",
                CursorBlinkPolicy::On => "true",
                CursorBlinkPolicy::Terminal => "terminal",
            }
            .to_owned(),
        ],
        AppearanceConfigKey::ZzFontWeight => vec![appearance.font_weight.to_string()],
        AppearanceConfigKey::ZzCursorBlinkIntervalMs => {
            vec![appearance.cursor_blink_interval_ms.to_string()]
        }
        AppearanceConfigKey::ZzSearchMatchColor => {
            vec![serialize_rgba(appearance.search_match_color)]
        }
        AppearanceConfigKey::ZzSearchCurrentColor => {
            vec![serialize_rgba(appearance.search_current_color)]
        }
        AppearanceConfigKey::ZzLinkColor => vec![serialize_rgb(appearance.link_color)],
        AppearanceConfigKey::ZzCopyCursorColor => {
            vec![serialize_rgba(appearance.copy_cursor_color)]
        }
        AppearanceConfigKey::ZzRoundedSelection => {
            vec![appearance.rounded_selection.to_string()]
        }
    };
    Ok(values)
}

pub fn serialize_font_families(families: &[String]) -> Vec<String> {
    families
        .iter()
        .map(|family| quote_appearance_value(family))
        .collect()
}

pub fn serialize_font_synthetic_style(styles: zz_terminal::FontSyntheticStyle) -> String {
    if styles.bold && styles.italic && styles.bold_italic {
        return "true".to_owned();
    }
    if !styles.bold && !styles.italic && !styles.bold_italic {
        return "false".to_owned();
    }
    [
        ("bold", styles.bold),
        ("italic", styles.italic),
        ("bold-italic", styles.bold_italic),
    ]
    .into_iter()
    .map(|(style, enabled)| {
        if enabled {
            style.to_owned()
        } else {
            format!("no-{style}")
        }
    })
    .collect::<Vec<_>>()
    .join(",")
}

pub fn serialize_rgb(color: Color) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

pub fn serialize_rgba(color: AppearanceColor) -> String {
    format!(
        "#{:02X}{:02X}{:02X}{:02X}",
        color.r, color.g, color.b, color.a
    )
}

pub fn quote_appearance_value(value: &str) -> String {
    format!("\"{value}\"")
}

pub fn validate_ui_font_family(family: &str) -> Result<(), &'static str> {
    if family.trim().is_empty() || family.chars().any(char::is_control) {
        return Err("font family must be nonempty text without control characters");
    }
    Ok(())
}

pub fn set_ui_font_family(family: Option<&str>, system_font_family: &str) -> io::Result<()> {
    let path = config_path_for_write()?;
    write_ui_font_family_at(&path, family, system_font_family).map(drop)
}

pub fn write_ui_font_family_at(
    path: &Path,
    family: Option<&str>,
    system_font_family: &str,
) -> io::Result<bool> {
    let family = family.unwrap_or(system_font_family);
    validate_ui_font_family(family)
        .map_err(|message| io::Error::new(ErrorKind::InvalidInput, message))?;
    let escaped = family.replace('\\', "\\\\").replace('"', "\\\"");
    write_config_edit_at(
        path,
        ConfigKey::UiFontFamily.as_str(),
        Some(&format!("\"{escaped}\"")),
    )
}

pub fn set_config_key(key: ConfigKey, value: &str) -> io::Result<()> {
    set_config_key_name(key.as_str(), value)
}

/// Selects the theme `name`, a built-in id or a saved theme, and drops the
/// explicit settings a theme decides and the retired appearance keys, so the
/// theme shows as designed. A theme that sets a font decides the UI font too.
pub fn set_interface_theme(name: &str, theme_sets_font: bool) -> io::Result<()> {
    if !is_builtin_theme(name) && !themes::is_theme_name(name) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("`{name}` is not a theme name"),
        ));
    }
    let path = config_path_for_write()?;
    write_interface_theme_at(&path, name, theme_sets_font)
}

pub fn write_interface_theme_at(path: &Path, name: &str, theme_sets_font: bool) -> io::Result<()> {
    let source = match read_config_source(path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let edited = interface_theme_source(&source, name, theme_sets_font);
    if edited.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("configuration edit exceeds the {MAX_CONFIG_BYTES}-byte limit"),
        ));
    }
    atomic_write(path, edited.as_bytes())
}

#[must_use]
pub fn interface_theme_source(source: &str, name: &str, theme_sets_font: bool) -> String {
    let decided = |key: &str| {
        ConfigKey::THEME_PICKS
            .iter()
            .any(|picked| picked.as_str() == key)
            || (theme_sets_font && key == ConfigKey::UiFontFamily.as_str())
    };
    let mut without_decided = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let dropped = config_key_for_line(line).is_some_and(|key| {
            decided(key)
                || RETIRED_APPEARANCE_KEYS.contains(&key)
                || RETIRED_LOOK_KEYS.contains(&key)
                || RETIRED_MATERIAL_KEYS.contains(&key)
        });
        if !dropped {
            without_decided.push_str(line);
        }
    }
    edit_config_source(
        &without_decided,
        ConfigKey::InterfaceTheme.as_str(),
        Some(name),
    )
}

pub fn set_config_key_name(key: &str, value: &str) -> io::Result<()> {
    if value.bytes().any(|byte| matches!(byte, b'\r' | b'\n')) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "configuration values must fit on one line",
        ));
    }
    let path = config_path_for_write()?;
    write_config_edit_at(&path, key, Some(value)).map(drop)
}

pub fn remove_config_key(key: ConfigKey) -> io::Result<()> {
    remove_config_key_name(key.as_str())
}

pub fn remove_config_key_name(key: &str) -> io::Result<()> {
    let path = config_path_for_write()?;
    write_config_edit_at(&path, key, None).map(drop)
}

pub fn write_config_edit_at(path: &Path, key: &str, value: Option<&str>) -> io::Result<bool> {
    let source = match read_config_source(path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let edited = edit_config_source(&source, key, value);
    if edited == source {
        return Ok(false);
    }
    if edited.len() > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("configuration edit exceeds the {MAX_CONFIG_BYTES}-byte limit"),
        ));
    }
    atomic_write(path, edited.as_bytes())?;
    Ok(true)
}

pub fn edit_config_source(source: &str, key: &str, value: Option<&str>) -> String {
    let last_line = last_key_line_range(source, key);
    match (last_line, value) {
        (Some(line), Some(value)) => {
            let replacement = replace_line_value(&source[line.clone()], value);
            let mut edited =
                String::with_capacity(source.len() + replacement.len().saturating_sub(line.len()));
            edited.push_str(&source[..line.start]);
            edited.push_str(&replacement);
            edited.push_str(&source[line.end..]);
            edited
        }
        (Some(line), None) => {
            let mut edited = String::with_capacity(source.len().saturating_sub(line.len()));
            edited.push_str(&source[..line.start]);
            edited.push_str(&source[line.end..]);
            edited
        }
        (None, Some(value)) => append_config_line(source, key, value),
        (None, None) => source.to_owned(),
    }
}

pub fn last_key_line_range(source: &str, key: &str) -> Option<std::ops::Range<usize>> {
    let mut offset = 0;
    let mut last = None;
    for line in source.split_inclusive('\n') {
        let end = offset + line.len();
        if config_key_for_line(line) == Some(key) {
            last = Some(offset..end);
        }
        offset = end;
    }
    last
}

pub fn config_key_for_line(line: &str) -> Option<&str> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    Some(key.trim())
}

pub fn replace_line_value(line: &str, value: &str) -> String {
    let without_lf = line.strip_suffix('\n').unwrap_or(line);
    let body = without_lf.strip_suffix('\r').unwrap_or(without_lf);
    let equals = body
        .find('=')
        .expect("a matched configuration line has an equals sign");
    let value_area_start = equals + 1;
    let comment_start = config_comment_start(&body[value_area_start..])
        .map_or(body.len(), |index| value_area_start + index);
    let value_area = &body[value_area_start..comment_start];
    let (value_start, value_end) = if value_area.trim().is_empty() {
        (comment_start, comment_start)
    } else {
        (
            value_area_start + value_area.len() - value_area.trim_start().len(),
            value_area_start + value_area.trim_end().len(),
        )
    };

    let mut replacement = String::with_capacity(line.len() + value.len());
    replacement.push_str(&line[..value_start]);
    replacement.push_str(value);
    replacement.push_str(&line[value_end..]);
    replacement
}

pub fn append_config_line(source: &str, key: &str, value: &str) -> String {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut edited = String::with_capacity(source.len() + key.len() + value.len() + 4);
    edited.push_str(source);
    if !source.is_empty() && !source.ends_with('\n') {
        edited.push_str(newline);
    }
    edited.push_str(key);
    edited.push_str(" = ");
    edited.push_str(value);
    edited.push_str(newline);
    edited
}

pub fn config_overrides_for_host(
    entries: Vec<ConfigOverrideEntry>,
    remote: bool,
) -> Vec<ConfigOverrideEntry> {
    if !remote {
        return entries;
    }
    entries
        .into_iter()
        .filter(|(key, _)| {
            !matches!(
                MuxOptionKey::from_config_key(key),
                Some(MuxOptionKey::ExperimentalEditorPane)
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_gui_value_is_one_the_config_reads() {
        use zz_client::gui_actions::{GUI_ACTIONS, GuiValues};
        for action in &GUI_ACTIONS {
            let Some(key) = action.config_key else {
                continue;
            };
            assert!(ConfigKey::parse(key).is_some(), "{key}");
            let values: Vec<&str> = match action.values {
                GuiValues::Fixed(choices) => choices.iter().map(|choice| choice.value).collect(),
                GuiValues::Listed => BUILTIN_THEMES.to_vec(),
            };
            for value in values {
                let parsed = parse_config(&format!("{key} = {value}\n"), "monospace");
                assert!(
                    parsed.diagnostics.is_empty(),
                    "{key} = {value}: {:?}",
                    parsed.diagnostics
                );
            }
        }
    }

    #[test]
    fn the_driver_is_off_until_turned_on_and_takes_a_model_and_effort() {
        let defaults = parse_config("", "monospace").config;
        assert!(!defaults.driver_agent.value);
        assert_eq!(defaults.driver_model.value, DriverModel::Claude);
        assert_eq!(defaults.driver_effort.value, DriverEffort::Default);

        let parsed = parse_config(
            "driver-agent = on\ndriver-model = codex\ndriver-effort = xhigh\n",
            "monospace",
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(parsed.config.driver_agent.value);
        assert_eq!(parsed.config.driver_model.value, DriverModel::Codex);
        assert_eq!(parsed.config.driver_effort.value, DriverEffort::Xhigh);

        assert!(parsed.driver_hosts.is_empty());
        let hosts = parse_config("driver-hosts = ubuntu, xps local\n", "monospace");
        assert_eq!(hosts.driver_hosts, ["ubuntu", "xps", "local"]);

        let invalid = parse_config("driver-model = gpt\ndriver-effort = huge\n", "monospace");
        assert_eq!(invalid.diagnostics.len(), 2);
        assert_eq!(invalid.config.driver_model.value, DriverModel::Claude);
        assert_eq!(invalid.config.driver_effort.value, DriverEffort::Default);
    }

    #[test]
    fn palette_preferences_validate() {
        let defaults = parse_config("", "monospace");
        assert_eq!(
            defaults.config.palette_window_layout.value,
            PaletteWindowLayout::Grouped
        );
        assert!(defaults.config.palette_show_keys.value);

        let source = "palette-window-layout = flat\npalette-show-keys = off\n";
        let parsed = parse_config(source, "monospace");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(
            parsed.config.palette_window_layout.value,
            PaletteWindowLayout::Flat
        );
        assert!(!parsed.config.palette_show_keys.value);

        let invalid = parse_config(
            "palette-window-layout = grid\npalette-show-keys = maybe\n",
            "monospace",
        );
        assert_eq!(invalid.diagnostics.len(), 2);
        assert_eq!(
            invalid.config.palette_window_layout.value,
            PaletteWindowLayout::Grouped
        );
        assert!(invalid.config.palette_show_keys.value);
    }

    #[test]
    fn windows_config_write_uses_native_roots_without_unix_home() {
        let root = tempfile::tempdir().unwrap();
        let xdg = root.path().join("xdg");
        let roaming = root.path().join("roaming");
        let local = root.path().join("local");
        let profile = root.path().join("profile");
        for (environment, expected_root) in [
            (
                ConfigEnvironment {
                    xdg_config_home: Some(&xdg),
                    appdata: Some(&roaming),
                    ..ConfigEnvironment::default()
                },
                xdg.clone(),
            ),
            (
                ConfigEnvironment {
                    appdata: Some(&roaming),
                    local_appdata: Some(&local),
                    user_profile: Some(&profile),
                    ..ConfigEnvironment::default()
                },
                roaming.clone(),
            ),
            (
                ConfigEnvironment {
                    appdata: Some(Path::new("relative")),
                    local_appdata: Some(&local),
                    user_profile: Some(&profile),
                    ..ConfigEnvironment::default()
                },
                local.clone(),
            ),
            (
                ConfigEnvironment {
                    user_profile: Some(&profile),
                    ..ConfigEnvironment::default()
                },
                profile.join(".config"),
            ),
        ] {
            let candidates = config_candidates_for(ConfigPlatform::Windows, environment);
            let path = config_write_path(&candidates).unwrap();
            assert_eq!(
                path,
                expected_root
                    .join(CONFIG_DIRECTORY_NAME)
                    .join(CONFIG_FILE_NAME)
            );
            write_config_edit_at(&path, "picker-focus-sidebar", Some("true")).unwrap();
            write_config_edit_at(&path, "picker-focus-sidebar", Some("false")).unwrap();
            assert_eq!(
                read_config_source(&path).unwrap(),
                "picker-focus-sidebar = false\n"
            );
            assert_eq!(discover_config_path(&candidates), Some(path));
        }
    }

    #[test]
    fn config_write_keeps_an_existing_lower_priority_file() {
        let root = tempfile::tempdir().unwrap();
        let roaming = root.path().join("roaming");
        let local = root.path().join("local");
        let candidates = config_candidates_for(
            ConfigPlatform::Windows,
            ConfigEnvironment {
                appdata: Some(&roaming),
                local_appdata: Some(&local),
                ..ConfigEnvironment::default()
            },
        );
        let existing = &candidates[1];
        fs::create_dir_all(existing.parent().unwrap()).unwrap();
        fs::write(existing, "# keep this\npicker-focus-sidebar = true\n").unwrap();
        let path = config_write_path(&candidates).unwrap();
        assert_eq!(&path, existing);
        write_config_edit_at(&path, "picker-focus-sidebar", Some("false")).unwrap();
        assert_eq!(
            read_config_source(existing).unwrap(),
            "# keep this\npicker-focus-sidebar = false\n"
        );
        assert!(!candidates[0].exists());
    }

    #[test]
    fn config_write_rejects_missing_or_relative_roots() {
        for environment in [
            ConfigEnvironment::default(),
            ConfigEnvironment {
                appdata: Some(Path::new("relative")),
                user_profile: Some(Path::new("relative")),
                ..ConfigEnvironment::default()
            },
        ] {
            let candidates = config_candidates_for(ConfigPlatform::Windows, environment);
            assert_eq!(
                config_write_path(&candidates).unwrap_err().kind(),
                ErrorKind::NotFound
            );
        }
    }

    #[test]
    fn configuration_discovery_keeps_build_identities_separate() {
        let root = tempfile::tempdir().unwrap();
        let other = if zz_protocol::app_identity::DEVELOPMENT {
            "zz"
        } else {
            "zz-dev"
        };
        let home = root.path();
        for base in [
            home.join(".config"),
            home.join("Library/Application Support"),
        ] {
            let path = base.join(other).join("config");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "pane-margin = 99\n").unwrap();
        }
        let candidates = config_candidates_for(
            ConfigPlatform::Macos,
            ConfigEnvironment {
                home: Some(home),
                ..ConfigEnvironment::default()
            },
        );
        assert_eq!(discover_config_path(&candidates), None);
        let own = config_write_path(&candidates).unwrap();
        fs::create_dir_all(own.parent().unwrap()).unwrap();
        fs::write(&own, "pane-margin = 2\n").unwrap();
        assert_eq!(discover_config_path(&candidates), Some(own));
    }

    #[test]
    fn retired_appearance_keys_diagnose_and_stay_out_of_the_terminal_editor() {
        let source = RETIRED_APPEARANCE_KEYS
            .iter()
            .fold(String::new(), |mut source, key| {
                use std::fmt::Write as _;
                let _ = writeln!(source, "{key} = something");
                source
            });
        let parsed = parse_config(&source, "monospace");
        assert_eq!(parsed.diagnostics.len(), RETIRED_APPEARANCE_KEYS.len());
        for (index, key) in RETIRED_APPEARANCE_KEYS.into_iter().enumerate() {
            assert_eq!(ConfigKey::parse(key), None);
            assert_eq!(parsed.diagnostics[index].line, index + 1);
            assert!(
                parsed.diagnostics[index]
                    .message
                    .starts_with(&format!("`{key}` was removed")),
                "{}",
                parsed.diagnostics[index].message
            );
        }
        assert_eq!(appearance_editor_view(&source), "");
        assert_eq!(
            parsed.config.interface_theme,
            ConfigValue::from_default(DEFAULT_THEME)
        );
    }

    #[test]
    fn retired_status_bar_lines_are_reported_and_otherwise_ignored() {
        let source = "status-badges = false\nstatus-show-session = false\nstatus-agents = false\n";
        let parsed = parse_config(source, "monospace");
        assert_eq!(parsed.diagnostics.len(), 2);
        assert_eq!(parsed.diagnostics[0].line, 2);
        assert!(
            parsed.diagnostics[0]
                .message
                .starts_with("`status-show-session` was removed"),
            "{}",
            parsed.diagnostics[0].message
        );
        assert_eq!(parsed.diagnostics[1].line, 3);
        assert_eq!(
            parsed.diagnostics[1].message,
            "`status-agents` was removed: the status bar no longer shows agent activity; the sidebar shows each agent's state"
        );
        assert!(is_app_config_key("status-show-session"));
        assert!(is_app_config_key("status-agents"));
        assert!(!parsed.config.status_badges.value);
    }

    #[test]
    fn a_retired_pane_glass_line_is_reported_and_otherwise_ignored() {
        let source = "pane-gaps = true\npane-glass = true\n";
        let parsed = parse_config(source, "monospace");
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].line, 2);
        assert!(
            parsed.diagnostics[0]
                .message
                .starts_with("`pane-glass` was removed"),
            "{}",
            parsed.diagnostics[0].message
        );
        assert_eq!(ConfigKey::parse("pane-glass"), None);
        assert!(is_app_config_key("pane-glass"));
        assert_eq!(appearance_editor_view(source), "");
        assert_eq!(
            parsed.config,
            AppConfig {
                pane_gaps: ConfigValue {
                    value: true,
                    provenance: ConfigProvenance::Override,
                },
                ..AppConfig::default()
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn config_edits_through_a_symlink_edit_its_target() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("dotfiles-config");
        fs::write(&target, "# mine\ntray = true\n").expect("write target");
        let link = directory.path().join(CONFIG_FILE_NAME);
        std::os::unix::fs::symlink(&target, &link).expect("link config");

        assert!(write_config_edit_at(&link, "tray", Some("false")).expect("edit"));

        assert!(
            fs::symlink_metadata(&link)
                .expect("link")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(&target).expect("read target"),
            "# mine\ntray = false\n"
        );
    }

    #[test]
    fn animation_speed_takes_a_share_of_the_designed_speed_within_its_range() {
        let parsed = parse_config("animation-speed = 0.5\n", "monospace");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.config.animation_speed.value, 0.5);

        let parsed = parse_config("animation-speed = 5\n", "monospace");
        assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
        assert_eq!(parsed.config.animation_speed.value, DEFAULT_ANIMATION_SPEED);
    }

    #[test]
    fn interface_theme_writer_drops_what_the_theme_decides_and_retired_keys() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join(CONFIG_FILE_NAME);
        fs::write(
            &path,
            "widget-corner-radius = 25\nchrome-contrast = 1.2\nshadow-strength = 0.4\n\
             widget-shadows = none\nwidget-selection = wash\nwidget-field = outline\n\
             widget-shadow-strength = 0.5\nwidget-outline-width = 0\nwidget-glass = off\n\
             interface-style = flat\nchrome-preset-dark = nord\n\
             chrome-accent = green\nchrome-tint = 0.4\nui-font-family = \"Fira Sans\"\n",
        )
        .expect("write config");

        write_interface_theme_at(&path, "default", false).expect("write theme");
        let source = fs::read_to_string(&path).expect("read theme");
        let parsed = parse_config(&source, "monospace");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.config.interface_theme.value, "default");
        assert_eq!(parsed.theme, None);
        assert_eq!(
            parsed.config.widget_corner_radius,
            ConfigValue::from_default(DEFAULT_WIDGET_CORNER_RADIUS)
        );
        assert_eq!(parsed.config.chrome_contrast.value, 1.2);
        assert_eq!(
            parsed.config.chrome_accent.value,
            AccentSetting::Named("green")
        );
        assert_eq!(parsed.config.chrome_tint.value, 0.4);
        assert_eq!(parsed.ui_font.family.value.as_deref(), Some("Fira Sans"));

        write_interface_theme_at(&path, "round", true).expect("write theme");
        let source = fs::read_to_string(&path).expect("read theme");
        let parsed = parse_config(&source, "monospace");
        assert_eq!(parsed.theme.as_deref(), Some("round"));
        assert_eq!(parsed.ui_font.family, ConfigValue::from_default(None));

        let parsed = parse_config("interface-theme = round", "monospace");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.theme.as_deref(), Some("round"));
        assert_eq!(parsed.config.interface_theme.value, DEFAULT_THEME);
        assert_eq!(
            parsed.config.interface_theme.provenance,
            ConfigProvenance::Override
        );

        let parsed = parse_config(
            "interface-theme = round\ninterface-theme = default",
            "monospace",
        );
        assert_eq!(parsed.theme, None);
        assert_eq!(parsed.config.interface_theme.value, "default");

        let parsed = parse_config("interface-theme = ../round", "monospace");
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.theme, None);
        assert!(set_interface_theme("../round", false).is_err());
    }

    #[test]
    fn appearance_keys_set_accent_and_tint_and_report_bad_values() {
        let parsed = parse_config(
            "interface-style = capsule\nchrome-accent = #FF8800\n\
             chrome-tint = 0.3\nchrome-tint-hue = 145\n\
             chrome-accent = chartreuse\nchrome-tint-hue = 400\nwidget-glass = off\n\
             pane-corner-radius = 4\nwidget-shadows = none\n",
            "monospace",
        );
        let config = parsed.config;
        assert_eq!(config.chrome_accent.value.spelled(), "#ff8800");
        assert_eq!(config.chrome_tint.value, 0.3);
        assert_eq!(config.chrome_tint_hue.value, 145.0);
        let messages = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 6, "{messages:?}");
        assert_eq!(
            messages[0],
            "`interface-style` was removed: pick a theme with `interface-theme`, or make one in the theme creator"
        );
        assert!(messages[1].contains("expected theme, blue"));
        assert!(messages[2].contains("between 0 and 360"));
        assert_eq!(
            messages[3],
            "`widget-glass` was removed: pick a theme with `interface-theme`, or make one in the theme creator"
        );
        assert_eq!(
            messages[4],
            "`pane-corner-radius` was removed: pane corners follow `widget-corner-radius`"
        );
        assert_eq!(
            messages[5],
            "`widget-shadows` was removed: themes set this per role now"
        );
        for key in RETIRED_MATERIAL_KEYS
            .into_iter()
            .chain(["pane-corner-radius", "pane-border-width"])
        {
            assert!(is_app_config_key(key));
            assert_eq!(ConfigKey::parse(key), None);
        }

        let mut config = parse_config("", "monospace").config;
        config.follow_theme_radius(18.0);
        assert_eq!(config.widget_corner_radius.value, 18.0);
        let mut config = parse_config("widget-corner-radius = 4\n", "monospace").config;
        config.follow_theme_radius(18.0);
        assert_eq!(config.widget_corner_radius.value, 4.0);
    }

    #[test]
    fn retired_agent_pane_keys_are_reported_and_ignored() {
        let parsed = parse_config(
            "experimental-agent-pane = off\nagent-working-directory = \"/work\"\n",
            "monospace",
        );
        assert!(parsed.daemon_entries.is_empty());
        assert_eq!(parsed.diagnostics.len(), 2);
        assert!(
            parsed
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.message.contains("was removed"))
        );
    }

    #[test]
    fn browser_remote_debugging_port_defaults_to_disabled() {
        let parsed = parse_config("", "monospace");
        assert_eq!(
            parsed.browser.remote_debugging_port,
            ConfigValue::from_default(None),
        );
        assert_eq!(
            ConfigKey::parse(ConfigKey::BrowserRemoteDebuggingPort.as_str()),
            Some(ConfigKey::BrowserRemoteDebuggingPort),
        );
    }

    #[test]
    fn browser_remote_debugging_port_accepts_zero_and_allowed_ports() {
        for (source, expected) in [
            ("0", None),
            ("1024", Some(1024)),
            ("9222", Some(9222)),
            ("65535", Some(65535)),
        ] {
            let parsed = parse_config(
                &format!("browser-remote-debugging-port = {source}\n"),
                "monospace",
            );
            assert!(parsed.diagnostics.is_empty(), "{source}");
            assert_eq!(parsed.browser.remote_debugging_port.value, expected);
            assert_eq!(
                parsed.browser.remote_debugging_port.provenance,
                ConfigProvenance::Override
            );
        }
        let parsed = parse_config(
            "browser-remote-debugging-port = 9222\nbrowser-remote-debugging-port = 0\n",
            "monospace",
        );
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.browser.remote_debugging_port.value, None);
    }

    #[test]
    fn browser_remote_debugging_port_rejects_invalid_values_and_retains_previous_port() {
        for source in ["1", "1023", "65536", "-1", "9222.0", "\"9222\"", "port", ""] {
            let parsed = parse_config(
                &format!(
                    "browser-remote-debugging-port = 9222\nbrowser-remote-debugging-port = {source}\n"
                ),
                "monospace",
            );
            assert_eq!(parsed.diagnostics.len(), 1, "{source}");
            assert_eq!(parsed.diagnostics[0].line, 2);
            assert!(
                parsed.diagnostics[0]
                    .message
                    .contains("invalid `browser-remote-debugging-port`")
            );
            assert_eq!(parsed.browser.remote_debugging_port.value, Some(9222));
        }
    }
}
