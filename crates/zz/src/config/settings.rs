mod file_options;
mod multiplexer;
mod sources;
mod terminal_preview;

use std::{
    collections::BTreeMap,
    io::{self, ErrorKind},
    path::PathBuf,
};

use ala::{
    AnyElement, App, ClipboardItem, Context, Entity, FocusHandle, Focusable, IntoElement, Render,
    SharedString, Subscription, Window, WindowControlArea, div, img, prelude::*, px,
};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, IndexPath, WindowExt as _,
    button::{Button, ButtonVariants as _},
    code_editor::{CodeEditor, CodeEditorEvent, CodeEditorState},
    h_flex,
    input::{Input, InputEvent, InputState, NumberInput},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};

use crate::{
    app_icon::AppIconSetting,
    config::{
        self, AppConfig, BrowserConfig, ConfigKey, ConfigProvenance, ConfigValue,
        remove_config_key, set_config_key, set_interface_theme,
    },
    diagnostics,
    mux::{
        client::MuxClient,
        hosts::{HostId, HostState},
    },
    window::{drag::window_drag_handle, toast},
    workspace::add_host,
};
use zz_browser::SearchProvider;
use zz_protocol::ConfigOverrideEntry;
use zz_terminal::{TerminalColorScheme, discover_ghostty_config};
use zz_ui::color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState};
use zz_ui::feedback::import_configuration_file_alert;
use zz_ui::interface_theme::{self, InterfaceTheme};
use zz_ui::settings::appearance::{
    SWATCH, ThemePicks, accent_swatches, animation_speed_slider, contrast_slider,
    corner_radius_slider, picker_tile, theme_mode_control, theme_picker, tint_hue_slider,
    tint_slider, ui_font_select, window_opacity_slider,
};
use zz_ui::settings::{
    SettingEntry, SettingsScrollColumn, SettingsSection, SettingsSelectItem, SettingsStack,
    StackPosition, settings_card_ground, settings_list_disclosure_header,
    settings_list_group_header, settings_page_content, settings_page_title, settings_preview,
    settings_scroll_column,
};

ala::actions!(zz, [OpenSettings]);

const NEEDS_PANE_GAPS: &str = "Needs pane gaps";

pub(crate) struct PendingMuxImport(pub bool);
impl ala::Global for PendingMuxImport {}

#[cfg(target_os = "macos")]
pub(crate) const KEYBIND: &str = "cmd-,";
#[cfg(not(target_os = "macos"))]
pub(crate) const KEYBIND: &str = "ctrl-,";
const CONTROL_WIDTH: f32 = 120.0;
const KEY_INPUT_WIDTH: f32 = 65.0;
const CONFIG_EDITOR_FONT_SIZE: f32 = 12.0;
const CONFIG_EDITOR_PADDING: f32 = 2.0;
const APP_ICON_PREVIEW_SIZE: f32 = 48.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ConfigFileKind {
    Mux,
    Terminal,
}

struct ConfigFileEditor {
    path: Option<PathBuf>,
    editor: Entity<CodeEditorState>,
    saved: String,
    error: Option<String>,
}

struct HostsSectionState {
    input: Entity<InputState>,
    error: Option<SharedString>,
}

use zz_ui::settings::appearance::AppearancePageItem;

fn appearance_page_items(has_window_blur: bool, advanced: bool) -> Vec<AppearancePageItem> {
    zz_ui::settings::appearance::appearance_page_items(
        cfg!(target_os = "macos"),
        has_window_blur,
        advanced,
    )
}

pub(crate) struct SettingsView {
    mux: Entity<MuxClient>,
    observed: AppConfig,
    observed_browser: BrowserConfig,
    observed_ui_font: ConfigValue<Option<String>>,
    observed_theme_font: Option<SharedString>,
    ui_font_family: Entity<SelectState<Vec<SettingsSelectItem>>>,
    browser_element_selector_hotkey: Entity<InputState>,
    browser_search_provider: Entity<SelectState<Vec<SettingsSelectItem>>>,
    ui_zoom: Entity<InputState>,
    observed_ui_zoom: u32,
    pip_background_opacity: Entity<InputState>,
    pane_glow_strength: Entity<InputState>,
    pane_inactive_opacity: Entity<InputState>,
    pane_margin: Entity<InputState>,
    editor_font_size: Entity<InputState>,
    which_key_delay: Entity<InputState>,
    accent_picker: Entity<ColorPickerState>,
    window_corner_radius: Entity<InputState>,
    mux_config_editor: Option<ConfigFileEditor>,
    mux_split_controls: Option<multiplexer::SplitControls>,
    file_options: BTreeMap<ConfigFileKind, file_options::FileOptions>,
    file_command_sequence: u64,
    pending_file_command: Option<(u64, ConfigFileKind)>,
    observed_appearance_overrides: Vec<ConfigOverrideEntry>,
    terminal_config_editor: Option<ConfigFileEditor>,
    terminal_preview: Option<Entity<terminal_preview::TerminalPreview>>,
    hosts_state: Option<HostsSectionState>,
    appearance_advanced: bool,
    section: SettingsSection,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub(crate) fn new(mux: Entity<MuxClient>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::settings";
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let observed = config::resolved_config(cx);
        let observed_browser = config::browser_config(cx);
        let observed_ui_font = config::ui_font_family(cx);
        let observed_theme_font = config::themes(cx).active.materials.font.clone();
        let ui_font_family = ui_font_select(
            Some(&shown_ui_font(
                &observed_ui_font,
                observed_theme_font.as_ref(),
            )),
            window,
            cx,
        );
        let browser_element_selector_hotkey =
            text_value_input(&observed_browser.element_selector_hotkey.value, window, cx);
        let browser_search_provider =
            search_provider_select(observed_browser.search_provider.value, window, cx);
        let ui_zoom = ui_zoom_input(window, cx);
        let pip_background_opacity = numeric_value_input(
            ConfigKey::PipBackgroundOpacity,
            observed.pip_background_opacity.value,
            5.0,
            window,
            cx,
        );
        let pane_glow_strength = numeric_value_input(
            ConfigKey::PaneGlowStrength,
            observed.pane_glow_strength.value,
            5.0,
            window,
            cx,
        );
        let pane_inactive_opacity = numeric_value_input(
            ConfigKey::PaneInactiveOpacity,
            observed.pane_inactive_opacity.value,
            0.05,
            window,
            cx,
        );
        let pane_margin = numeric_value_input(
            ConfigKey::PaneMargin,
            observed.pane_margin.value,
            1.0,
            window,
            cx,
        );
        let editor_font_size = numeric_value_input(
            ConfigKey::EditorFontSize,
            observed.editor_font_size.value,
            1.0,
            window,
            cx,
        );
        let which_key_delay = numeric_value_input(
            ConfigKey::WhichKeyDelay,
            observed.which_key_delay.value,
            50.0,
            window,
            cx,
        );
        let accent_picker = cx.new(|cx| {
            ColorPickerState::new(custom_accent(observed.chrome_accent.value), window, cx)
        });
        let window_corner_radius = numeric_value_input(
            ConfigKey::WindowCornerRadius,
            observed.window_corner_radius.value,
            1.0,
            window,
            cx,
        );
        let subscriptions = vec![
            cx.observe(&mux, |_, _, cx| cx.notify()),
            cx.observe_global::<config::FleetHosts>(|_, cx| cx.notify()),
            cx.observe_global::<crate::update::UpdateState>(|_, cx| cx.notify()),
            browser_hotkey_subscription(&browser_element_selector_hotkey, window, cx),
            search_provider_subscription(&browser_search_provider, window, cx),
            ui_font_subscription(&ui_font_family, window, cx),
            ui_zoom_subscription(&ui_zoom, window, cx),
            numeric_input_subscription(
                &pip_background_opacity,
                ConfigKey::PipBackgroundOpacity,
                window,
                cx,
            ),
            numeric_input_subscription(
                &pane_glow_strength,
                ConfigKey::PaneGlowStrength,
                window,
                cx,
            ),
            numeric_input_subscription(
                &pane_inactive_opacity,
                ConfigKey::PaneInactiveOpacity,
                window,
                cx,
            ),
            numeric_input_subscription(&pane_margin, ConfigKey::PaneMargin, window, cx),
            numeric_input_subscription(&which_key_delay, ConfigKey::WhichKeyDelay, window, cx),
            cx.subscribe(&accent_picker, |_, _, event: &ColorPickerEvent, cx| {
                let ColorPickerEvent::Change(color) = event;
                let key = ConfigKey::ChromeAccent;
                let result = match color {
                    Some(color) => set_config_key(key, &zz_ui::to_hex(color.opaque())),
                    None => remove_config_key(key),
                };
                written(key, result, cx);
            }),
            numeric_input_subscription(
                &window_corner_radius,
                ConfigKey::WindowCornerRadius,
                window,
                cx,
            ),
        ];
        let observed_appearance_overrides = config::daemon_config_overrides(cx);
        let settings = Self {
            mux,
            observed,
            observed_browser,
            observed_ui_font,
            observed_theme_font,
            ui_font_family,
            browser_element_selector_hotkey,
            browser_search_provider,
            ui_zoom,
            observed_ui_zoom: crate::ui_scale::percent(cx),
            pip_background_opacity,
            pane_glow_strength,
            pane_inactive_opacity,
            pane_margin,
            editor_font_size,
            which_key_delay,
            accent_picker,
            window_corner_radius,
            mux_config_editor: None,
            mux_split_controls: None,
            file_options: BTreeMap::new(),
            file_command_sequence: 0,
            pending_file_command: None,
            observed_appearance_overrides,
            terminal_config_editor: None,
            terminal_preview: None,
            hosts_state: None,
            appearance_advanced: false,
            section: SettingsSection::Appearance,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        log::trace!(
            target: DIAGNOSTIC_TARGET,
            "initialize section={:?} subscriptions={} terminal_editor_initialized=false mux_editor_initialized=false elapsed_us={}",
            settings.section,
            settings._subscriptions.len(),
            diagnostics::elapsed_us(started),
        );
        settings
    }

    fn ensure_section_state(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        const DIAGNOSTIC_TARGET: &str = "zz::diagnostics::settings";
        if section == SettingsSection::Hosts && self.hosts_state.is_none() {
            let started = diagnostics::timer(DIAGNOSTIC_TARGET);
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(add_host::PLACEHOLDER));
            let subscription = cx.subscribe_in(
                &input,
                window,
                |settings, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        settings.submit_add_host(window, cx);
                    }
                },
            );
            self.hosts_state = Some(HostsSectionState { input, error: None });
            self._subscriptions.push(subscription);
            log::trace!(
                target: DIAGNOSTIC_TARGET,
                "initialize_section section={section:?} subscriptions=1 elapsed_us={}",
                diagnostics::elapsed_us(started),
            );
            return true;
        }
        let (kind, slot) = match section {
            SettingsSection::Terminal if self.terminal_config_editor.is_none() => {
                (ConfigFileKind::Terminal, &mut self.terminal_config_editor)
            }
            SettingsSection::Multiplexer if self.mux_config_editor.is_none() => {
                (ConfigFileKind::Mux, &mut self.mux_config_editor)
            }
            _ => return false,
        };
        let started = diagnostics::timer(DIAGNOSTIC_TARGET);
        let file = config_file_editor(kind, window, cx);
        let editor = file.editor.clone();
        *slot = Some(file);
        self._subscriptions
            .push(config_editor_subscription(&editor, window, cx));
        log::trace!(
            target: DIAGNOSTIC_TARGET,
            "initialize_section section={section:?} subscriptions=1 elapsed_us={}",
            diagnostics::elapsed_us(started),
        );
        true
    }

    fn submit_add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self
            .hosts_state
            .as_ref()
            .expect("Hosts state is initialized before submission")
            .input
            .clone();
        let value = input.read(cx).value();
        let existing = config::fleet_hosts(cx)
            .into_iter()
            .map(|host| host.name)
            .collect::<Vec<_>>();
        let request = match add_host::parse_add_host(&value, &existing) {
            Ok(request) => request,
            Err(message) => {
                self.hosts_state
                    .as_mut()
                    .expect("Hosts state is initialized before submission")
                    .error = Some(message.into());
                cx.notify();
                return;
            }
        };
        if let Err(error) = config::add_fleet_host(&request.name, &request.endpoint, cx) {
            self.hosts_state
                .as_mut()
                .expect("Hosts state is initialized before submission")
                .error = Some(format!("Could not write zz/config: {error}").into());
            cx.notify();
            return;
        }
        log::info!(
            target: "zz::config",
            "added fleet host name={} endpoint={}",
            request.name,
            request.endpoint,
        );
        input.update(cx, |input, cx| input.set_value("", window, cx));
        self.hosts_state
            .as_mut()
            .expect("Hosts state is initialized before submission")
            .error = None;
        cx.notify();
    }

    pub(crate) const fn section(&self) -> SettingsSection {
        self.section
    }

    pub(crate) fn focus(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    pub(crate) fn set_section(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initialized = self.ensure_section_state(section, window, cx);
        if !initialized {
            self.reload_config_editor_if_clean(section, window, cx);
        }
        self.section = section;
        if section != SettingsSection::Terminal {
            self.release_terminal_preview();
        }
        cx.notify();
    }

    pub(crate) fn release_terminal_preview(&mut self) {
        self.terminal_preview = None;
    }

    #[cfg(test)]
    pub(crate) fn has_terminal_preview(&self) -> bool {
        self.terminal_preview.is_some()
    }

    fn synchronize_terminal_preview(&mut self, cx: &mut Context<Self>) {
        if self.section != SettingsSection::Terminal {
            return;
        }
        let source = self.terminal_preview_source(cx);
        let path = self
            .config_file_editor(ConfigFileKind::Terminal)
            .path
            .clone();
        let scheme = import_color_scheme(cx);
        let preview = self.terminal_preview.get_or_insert_with(|| {
            let appearance = crate::theme::terminal_appearance(cx).map_or_else(
                || zz_terminal::AppearanceLoad::defaults_for(scheme).appearance,
                |appearance| appearance.as_ref().clone(),
            );
            cx.new(|cx| terminal_preview::TerminalPreview::new(appearance, cx))
        });
        preview.update(cx, |preview, cx| {
            preview.set_source(source, scheme, path, cx);
        });
    }

    fn synchronize_numeric_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AppConfig {
        let resolved = config::resolved_config(cx);
        if resolved != self.observed {
            self.observed = resolved;
            synchronize_f32_input(
                &self.pane_inactive_opacity,
                resolved.pane_inactive_opacity.value,
                window,
                cx,
            );
            synchronize_f32_input(&self.pane_margin, resolved.pane_margin.value, window, cx);
            synchronize_f32_input(
                &self.editor_font_size,
                resolved.editor_font_size.value,
                window,
                cx,
            );
            synchronize_f32_input(
                &self.which_key_delay,
                resolved.which_key_delay.value,
                window,
                cx,
            );
            let accent = custom_accent(resolved.chrome_accent.value);
            self.accent_picker.update(cx, |picker, cx| {
                picker.set_color(accent, window, cx);
            });
            if !numeric_input_matches_value(
                ConfigKey::PipBackgroundOpacity,
                &self.pip_background_opacity.read(cx).value(),
                resolved.pip_background_opacity.value,
            ) {
                synchronize_text_input(
                    &self.pip_background_opacity,
                    &numeric_input_text(
                        ConfigKey::PipBackgroundOpacity,
                        resolved.pip_background_opacity.value,
                    ),
                    window,
                    cx,
                );
            }
            if !numeric_input_matches_value(
                ConfigKey::PaneGlowStrength,
                &self.pane_glow_strength.read(cx).value(),
                resolved.pane_glow_strength.value,
            ) {
                synchronize_text_input(
                    &self.pane_glow_strength,
                    &numeric_input_text(
                        ConfigKey::PaneGlowStrength,
                        resolved.pane_glow_strength.value,
                    ),
                    window,
                    cx,
                );
            }
            synchronize_f32_input(
                &self.window_corner_radius,
                resolved.window_corner_radius.value,
                window,
                cx,
            );
        }
        resolved
    }

    fn synchronize_browser_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> BrowserConfig {
        let resolved = config::browser_config(cx);
        if resolved != self.observed_browser {
            self.observed_browser.clone_from(&resolved);
            synchronize_text_input(
                &self.browser_element_selector_hotkey,
                &resolved.element_selector_hotkey.value,
                window,
                cx,
            );
            let provider = resolved.search_provider.value.as_str().to_owned();
            self.browser_search_provider.update(cx, |select, cx| {
                select.set_selected_value(&provider, window, cx);
            });
        }
        resolved
    }

    fn synchronize_ui_font(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let resolved = config::ui_font_family(cx);
        let theme_font = config::themes(cx).active.materials.font.clone();
        if resolved == self.observed_ui_font && theme_font == self.observed_theme_font {
            return;
        }
        let shown = shown_ui_font(&resolved, theme_font.as_ref());
        self.ui_font_family.update(cx, |select, cx| {
            select.set_selected_value(&shown, window, cx);
        });
        self.observed_ui_font = resolved;
        self.observed_theme_font = theme_font;
    }

    fn synchronize_ui_zoom_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let percent = crate::ui_scale::percent(cx);
        if percent == self.observed_ui_zoom {
            return;
        }
        self.observed_ui_zoom = percent;
        if self
            .ui_zoom
            .read(cx)
            .value()
            .parse::<f32>()
            .is_ok_and(|shown| crate::ui_scale::is_effective_percent(shown, cx))
        {
            return;
        }
        self.ui_zoom.update(cx, |input, cx| {
            input.set_value(percent.to_string(), window, cx);
        });
    }

    fn preview_ui_zoom(input: &Entity<InputState>, cx: &mut Context<Self>) {
        let Ok(percent) = input.read(cx).value().parse::<f32>() else {
            return;
        };
        if (crate::ui_scale::MIN_UI_ZOOM..=crate::ui_scale::MAX_UI_ZOOM).contains(&percent) {
            crate::ui_scale::set_percent(percent, cx);
        }
    }

    fn commit_ui_zoom_input(
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Ok(percent) = input.read(cx).value().parse::<f32>() {
            crate::ui_scale::set_percent(percent, cx);
        }
        let percent = crate::ui_scale::percent(cx).to_string();
        if input.read(cx).value().as_ref() != percent {
            input.update(cx, |input, cx| input.set_value(percent, window, cx));
        }
    }

    fn synchronize_terminal_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let overrides = config::daemon_config_overrides(cx);
        if overrides == self.observed_appearance_overrides {
            return;
        }
        self.observed_appearance_overrides = overrides;
        if self.terminal_config_editor.is_some() {
            self.reload_config_editor_if_clean(SettingsSection::Terminal, window, cx);
        }
    }

    fn commit_numeric_input(
        &mut self,
        key: ConfigKey,
        input: &Entity<InputState>,
        normalize: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let candidate = input.read(cx).value().to_string();
        let value = match validate_numeric_value(key, &candidate) {
            Ok(value) => value,
            Err(message) => {
                if !normalize {
                    return;
                }
                toast::push(
                    Notification::error(format!("Invalid {}: {message}", key.as_str())),
                    cx,
                );
                let effective =
                    numeric_input_text(key, numeric_config_value(&self.observed, key).value);
                input.update(cx, |input, cx| input.set_value(effective, window, cx));
                return;
            }
        };
        let displayed = value.to_string();
        if normalize && input.read(cx).value().as_ref() != displayed {
            input.update(cx, |input, cx| input.set_value(displayed, window, cx));
        }
        let current = numeric_config_value(&self.observed, key);
        if normalize
            && current.provenance == ConfigProvenance::Default
            && numeric_input_matches_value(key, &candidate, current.value)
        {
            return;
        }
        let value = (value / numeric_input_scale(key)).to_string();
        match set_config_key(key, &value) {
            Ok(()) => refresh_settings_preview(key, cx),
            Err(error) => report_write_error("set", key.as_str(), &error, cx),
        }
    }

    fn commit_browser_hotkey(
        &mut self,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = ConfigKey::BrowserElementSelectorHotkey;
        let candidate = input.read(cx).value().to_string();
        let value = match config::normalize_browser_hotkey(&candidate) {
            Ok(value) => value,
            Err(message) => {
                toast::push(
                    Notification::error(format!("Invalid {}: {message}", key.as_str())),
                    cx,
                );
                synchronize_text_input(
                    input,
                    &self.observed_browser.element_selector_hotkey.value,
                    window,
                    cx,
                );
                return;
            }
        };
        synchronize_text_input(input, &value, window, cx);
        if let Err(error) = set_config_key(key, &value) {
            report_write_error("set", key.as_str(), &error, cx);
        }
    }

    fn boolean_setting(
        key: ConfigKey,
        title: &'static str,
        setting: ConfigValue<bool>,
    ) -> SettingEntry {
        SettingEntry::new(title).control(boolean_control(
            key.as_str(),
            setting.value,
            move |enabled, _, cx| {
                if let Err(error) = set_config_key(key, if *enabled { "true" } else { "false" }) {
                    report_write_error("set", key.as_str(), &error, cx);
                } else {
                    refresh_settings_preview(key, cx);
                }
            },
        ))
    }

    fn remote_debugging_setting(
        setting: ConfigValue<Option<u16>>,
        cx: &Context<Self>,
    ) -> SettingEntry {
        let key = ConfigKey::BrowserRemoteDebuggingPort;
        SettingEntry::new("Remote debugging for agents")
            .description("DevTools Protocol on 127.0.0.1:9222, for agent-browser or Playwright")
            .control(
                Switch::new(format!("settings-{}", key.as_str()))
                    .checked(setting.value.is_some())
                    .on_click(move |enabled, _, cx| {
                        let result = if *enabled {
                            set_config_key(key, "9222")
                        } else {
                            remove_config_key(key)
                        };
                        if let Err(error) = result {
                            report_write_error("set", key.as_str(), &error, cx);
                        }
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(zz_ui::rems_from_px(11.0))
                    .text_color(cx.theme().warning)
                    .child(
                        "Any local process that reaches the port can drive the logged-in browser.",
                    ),
            )
    }

    fn slider_setting(item: AppearancePageItem, slider: impl IntoElement) -> SettingEntry {
        let title = item.title().unwrap_or_default();
        SettingEntry::new(title).control(slider)
    }

    fn numeric_setting(
        title: &'static str,
        input: &Entity<InputState>,
        cx: &Context<Self>,
    ) -> SettingEntry {
        SettingEntry::new(title).control(numeric_control(input, cx))
    }
}

impl SettingsView {
    fn scroll_column(id: &'static str) -> SettingsScrollColumn {
        settings_scroll_column(id)
    }

    fn appearance_section(&self, resolved: &AppConfig, cx: &Context<Self>) -> AnyElement {
        let items = appearance_page_items(
            crate::profile::profile(cx).has_window_blur,
            self.appearance_advanced,
        );
        let view = cx.entity();
        let resolved = *resolved;
        zz_ui::settings::appearance::appearance_page(items, move |item, position, _, cx| {
            view.update(cx, |this, cx| {
                this.appearance_item(item, &resolved, position, cx)
            })
        })
        .into_any_element()
    }

    fn appearance_item(
        &self,
        item: AppearancePageItem,
        resolved: &AppConfig,
        position: StackPosition,
        cx: &Context<Self>,
    ) -> AnyElement {
        match item {
            AppearancePageItem::Title => {
                return settings_page_title(SettingsSection::Appearance, cx).into_any_element();
            }
            AppearancePageItem::Group { title } => {
                return settings_list_group_header(title, cx).into_any_element();
            }
            AppearancePageItem::Advanced { expanded } => {
                return settings_list_disclosure_header(
                    zz_ui::settings::appearance::ADVANCED_DISCLOSURE,
                    "Advanced",
                    expanded,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.appearance_advanced = !this.appearance_advanced;
                    cx.notify();
                }))
                .into_any_element();
            }
            _ => {}
        }

        let entry = match item {
            AppearancePageItem::Title
            | AppearancePageItem::Group { .. }
            | AppearancePageItem::Advanced { .. } => {
                unreachable!("returned above")
            }
            AppearancePageItem::ThemeMode => Self::theme_mode_setting(resolved, cx),
            AppearancePageItem::Accent => self.accent_setting(cx),
            AppearancePageItem::TintHue => Self::tint_hue_setting(resolved, cx),
            AppearancePageItem::Tint => Self::tint_setting(resolved, cx),
            AppearancePageItem::Themes => Self::themes_setting(cx),
            AppearancePageItem::CornerRadius => Self::slider_setting(
                item,
                corner_radius_slider(
                    resolved.widget_corner_radius.value,
                    |radius, _, cx| {
                        let key = ConfigKey::WidgetCornerRadius;
                        written(key, set_config_key(key, &format!("{radius:.0}")), cx);
                    },
                    cx,
                ),
            ),
            AppearancePageItem::UiFontFamily => self.ui_font_setting(cx),
            AppearancePageItem::UiZoom => self.ui_zoom_setting(cx),
            AppearancePageItem::AppIcon => Self::app_icon_setting(resolved, cx),
            AppearancePageItem::ChromeContrast => Self::slider_setting(
                item,
                contrast_slider(
                    resolved.chrome_contrast.value,
                    |contrast, _, cx| {
                        let key = ConfigKey::ChromeContrast;
                        written(key, set_config_key(key, &format!("{contrast:.2}")), cx);
                    },
                    cx,
                ),
            ),
            AppearancePageItem::ProgramColors => {
                let title = item.title().unwrap_or_default();
                Self::boolean_setting(ConfigKey::ProgramColors, title, resolved.program_colors)
                    .description("Fill sidebar tiles with the running program's colors")
            }
            AppearancePageItem::Animations => {
                Self::boolean_setting(ConfigKey::Animations, "Animations", resolved.animations)
            }
            AppearancePageItem::AnimationSpeed => Self::slider_setting(
                item,
                animation_speed_slider(
                    resolved.animation_speed.value,
                    |speed, _, cx| {
                        let key = ConfigKey::AnimationSpeed;
                        written(key, set_config_key(key, &format!("{speed:.2}")), cx);
                    },
                    cx,
                ),
            ),
            AppearancePageItem::WindowBackgroundBlur => Self::boolean_setting(
                ConfigKey::WindowBackgroundBlur,
                "Window blur",
                resolved.window_background_blur,
            ),
            AppearancePageItem::WindowBackgroundOpacity => Self::slider_setting(
                item,
                window_opacity_slider(
                    resolved.window_background_opacity.value,
                    |opacity, _, cx| {
                        let key = ConfigKey::WindowBackgroundOpacity;
                        written(key, set_config_key(key, &format!("{opacity:.2}")), cx);
                    },
                    cx,
                ),
            )
            .disabled(!resolved.window_background_blur.value),
            #[cfg(target_os = "linux")]
            AppearancePageItem::WindowCornerRadius => {
                Self::numeric_setting("Window corner radius", &self.window_corner_radius, cx)
                    .description("No effect with the system titlebar")
            }
            #[cfg(target_os = "linux")]
            AppearancePageItem::UseSystemTitlebar => Self::boolean_setting(
                ConfigKey::UseSystemTitlebar,
                "System titlebar",
                resolved.use_system_titlebar,
            ),
        };

        entry.position(position).into_any_element()
    }

    fn ui_zoom_setting(&self, cx: &Context<Self>) -> SettingEntry {
        SettingEntry::new("UI zoom").control(
            div()
                .w(px(CONTROL_WIDTH))
                .flex_none()
                .child(NumberInput::new(&self.ui_zoom).on(settings_card_ground(cx))),
        )
    }

    fn theme_mode_setting(resolved: &AppConfig, cx: &Context<Self>) -> SettingEntry {
        SettingEntry::new("Mode").control(theme_mode_control(
            resolved.theme_mode.value,
            |mode, _, cx| {
                if let Err(error) = set_config_key(ConfigKey::ThemeMode, mode.as_str()) {
                    report_write_error("set", ConfigKey::ThemeMode.as_str(), &error, cx);
                }
            },
            cx,
        ))
    }

    fn accent_setting(&self, cx: &Context<Self>) -> SettingEntry {
        let title = AppearancePageItem::Accent.title().unwrap_or_default();
        let mode = cx.theme().mode;
        let custom = ColorPicker::new(&self.accent_picker, ala::transparent_black())
            .label("Accent")
            .swatch(px(SWATCH));
        let swatches = with_theme_picks(cx, |picks| {
            accent_swatches(
                picks,
                mode,
                custom,
                |name, _, cx| {
                    let key = ConfigKey::ChromeAccent;
                    let result = match name {
                        Some(name) => set_config_key(key, name),
                        None => remove_config_key(key),
                    };
                    written(key, result, cx);
                },
                cx,
            )
        });
        SettingEntry::new(title).control(swatches)
    }

    fn tint_hue_setting(resolved: &AppConfig, cx: &Context<Self>) -> SettingEntry {
        let title = AppearancePageItem::TintHue.title().unwrap_or_default();
        SettingEntry::new(title).control(tint_hue_slider(
            resolved.chrome_tint_hue.value,
            |hue, _, cx| {
                let key = ConfigKey::ChromeTintHue;
                written(key, set_config_key(key, &format!("{hue:.0}")), cx);
            },
            cx,
        ))
    }

    fn tint_setting(resolved: &AppConfig, cx: &Context<Self>) -> SettingEntry {
        let title = AppearancePageItem::Tint.title().unwrap_or_default();
        SettingEntry::new(title).control(tint_slider(
            resolved.chrome_tint.value,
            |tint, _, cx| {
                let key = ConfigKey::ChromeTint;
                written(key, set_config_key(key, &format!("{tint:.2}")), cx);
            },
            cx,
        ))
    }

    fn themes_setting(cx: &Context<Self>) -> SettingEntry {
        let title = AppearancePageItem::Themes.title().unwrap_or_default();
        let picker = with_theme_picks(cx, |picks| {
            let ids = picks
                .themes
                .iter()
                .map(|(id, theme)| (id.clone(), theme.materials.font.is_some()))
                .collect::<Vec<_>>();
            theme_picker(
                picks,
                move |index, _, cx| {
                    let (id, sets_font) = &ids[index];
                    written(
                        ConfigKey::InterfaceTheme,
                        set_interface_theme(id, *sets_font),
                        cx,
                    );
                },
                |menu, _, _| {
                    menu.item(
                        PopupMenuItem::new("Import from clipboard")
                            .on_click(|_, _, cx| import_theme_from_clipboard(cx)),
                    )
                },
                cx,
            )
        });
        SettingEntry::new(title).control(picker)
    }

    fn app_icon_setting(resolved: &AppConfig, cx: &Context<Self>) -> SettingEntry {
        let setting = resolved.app_icon;
        let appearance = cx.window_appearance();
        SettingEntry::new("App Icon")
            .description("Dock and app switcher")
            .control(div().flex().flex_none().gap(px(10.0)).children(
                AppIconSetting::ALL.into_iter().map(|choice| {
                    picker_tile(
                        format!("settings-app-icon-{}", choice.as_str()).into(),
                        choice.title(),
                        img(crate::app_icon::icon_preview(crate::app_icon::variant(
                            choice, appearance,
                        )))
                        .size(px(APP_ICON_PREVIEW_SIZE)),
                        choice == setting.value,
                        cx,
                    )
                    .on_click(move |_, _, cx| {
                        if let Err(error) = set_config_key(ConfigKey::AppIcon, choice.as_str()) {
                            report_write_error("set", ConfigKey::AppIcon.as_str(), &error, cx);
                        }
                    })
                }),
            ))
    }

    fn ui_font_setting(&self, cx: &Context<Self>) -> SettingEntry {
        SettingEntry::new("UI font").control(
            div().flex_none().child(
                Select::new(&self.ui_font_family)
                    .placeholder(
                        Some(shown_ui_font(
                            &self.observed_ui_font,
                            self.observed_theme_font.as_ref(),
                        ))
                        .filter(|family| !family.is_empty())
                        .unwrap_or_else(|| "System default".to_owned()),
                    )
                    .on(settings_card_ground(cx)),
            ),
        )
    }

    fn status_bar_section(&self, resolved: &AppConfig, cx: &Context<Self>) -> AnyElement {
        zz_ui::settings::status_bar_preview::status_bar_page(
            config::status_bar_settings(cx),
            resolved.pane_gaps.value,
            SettingsStack::new()
                .child(
                    Self::boolean_setting(
                        ConfigKey::StatusBadges,
                        "Window badges",
                        resolved.status_badges,
                    )
                    .description("Bell and activity markers on window items"),
                )
                .child(
                    Self::boolean_setting(ConfigKey::StatusHost, "Host", resolved.status_host)
                        .description("Shown when the attached host is remote"),
                )
                .child(
                    Self::boolean_setting(
                        ConfigKey::StatusUpdate,
                        "Update",
                        resolved.status_update,
                    )
                    .description("Shown when a newer zz is out"),
                ),
            cx,
        )
        .into_any_element()
    }

    fn browser_section(
        &self,
        browser: &BrowserConfig,
        resolved: &AppConfig,
        cx: &Context<Self>,
    ) -> AnyElement {
        let remote_debugging_port = browser.0.remote_debugging_port;
        Self::scroll_column("settings-browser")
            .child(settings_page_title(SettingsSection::Browser, cx))
            .child(
                SettingsStack::titled("Network").child(
                    Self::boolean_setting(
                        ConfigKey::BrowserEgress,
                        "Route through the attached host",
                        resolved.browser_egress,
                    )
                    .description("localhost and private names resolve on the remote host"),
                ),
            )
            .child(
                SettingsStack::titled("Agents")
                    .child(Self::remote_debugging_setting(remote_debugging_port, cx)),
            )
            .child(SettingsStack::titled("Search").child(
                SettingEntry::new("Search engine").control(div().flex_none().child(
                    Select::new(&self.browser_search_provider).on(settings_card_ground(cx)),
                )),
            ))
            .child(
                SettingsStack::titled("Shortcuts").child(
                    SettingEntry::new("Element selector")
                        .description("Shortcut for the DOM element picker, such as cmd-shift-c")
                        .control(
                            div().w(px(CONTROL_WIDTH)).flex_none().child(
                                Input::new(&self.browser_element_selector_hotkey)
                                    .on(settings_card_ground(cx)),
                            ),
                        ),
                ),
            )
            .into_any_element()
    }

    fn hosts_section(&self, cx: &Context<Self>) -> AnyElement {
        const INPUT_WIDTH: f32 = 220.0;

        let hosts = config::fleet_hosts(cx);
        let states = self
            .mux
            .read(cx)
            .fleet_hosts()
            .filter(|(id, _, _, _)| *id != HostId::LOCAL)
            .map(|(_, name, state, _)| (name.to_owned(), state.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut machine_entries = Vec::with_capacity(hosts.len().max(1));
        if hosts.is_empty() {
            machine_entries.push(
                SettingEntry::new("No hosts yet")
                    .description("Hosts you add show up in the sidebar"),
            );
        } else {
            for host in hosts {
                let name = host.name;
                let state = states
                    .get(&name)
                    .cloned()
                    .unwrap_or(HostState::Disconnected);
                let (glyph, color) = match &state {
                    HostState::Connected => (IconName::Globe, cx.theme().success),
                    HostState::Unreachable { .. }
                    | HostState::Incompatible { .. }
                    | HostState::Missing => (IconName::Globe, cx.theme().danger),
                    HostState::Connecting
                    | HostState::Reconnecting { .. }
                    | HostState::Installing => (IconName::Globe, cx.theme().warning),
                    HostState::Disconnected => (IconName::Globe, cx.theme().foreground.muted()),
                };
                let remove_name = name.clone();
                let remove_mux = self.mux.clone();
                let entry = SettingEntry::new(name.clone())
                    .description(host.endpoint.to_string())
                    .title_icon(Icon::new(glyph).text_color(color))
                    .control(
                        Button::new(format!("settings-remove-host-{name}"))
                            .ghost()
                            .icon(IconName::Xmark)
                            .tooltip(format!("Remove host ({})", state.label()))
                            .on(settings_card_ground(cx))
                            .on_click(move |_, _, cx| {
                                let host_id = remove_mux.read(cx).fleet_hosts().find_map(
                                    |(id, current_name, _, _)| {
                                        (id != HostId::LOCAL
                                            && current_name == remove_name.as_str())
                                        .then_some(id)
                                    },
                                );
                                if let Some(host_id) = host_id {
                                    crate::workspace::sidebar::close_host(
                                        &remove_mux,
                                        host_id,
                                        &remove_name,
                                        cx,
                                    );
                                } else {
                                    match config::remove_fleet_host_live(&remove_name, cx) {
                                        Ok(removed) => log::info!(
                                            target: "zz::config",
                                            "removed fleet host name={remove_name} config_removed={removed}",
                                        ),
                                        Err(error) => log::warn!(
                                            "could not remove host-{remove_name} from zz/config: {error}",
                                        ),
                                    }
                                }
                            }),
                    );
                machine_entries.push(entry);
            }
        }

        let state = self
            .hosts_state
            .as_ref()
            .expect("Hosts state is initialized before rendering");
        let input = state.input.clone();
        let mut destination = SettingEntry::new("Destination")
            .description("user@host[:port], signed in by your own ssh")
            .control(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .w(px(INPUT_WIDTH))
                            .flex_none()
                            .child(Input::new(&input).on(settings_card_ground(cx))),
                    )
                    .child(
                        Button::new("settings-add-host")
                            .primary()
                            .label("Add")
                            .on(settings_card_ground(cx))
                            .on_click(cx.listener(|settings, _, window, cx| {
                                settings.submit_add_host(window, cx);
                            })),
                    ),
            );
        if let Some(error) = state.error.clone() {
            destination = destination.child(
                div()
                    .flex_none()
                    .text_size(zz_ui::rems_from_px(11.0))
                    .text_color(cx.theme().warning)
                    .child(error),
            );
        }

        Self::scroll_column("settings-hosts")
            .child(settings_page_title(SettingsSection::Hosts, cx))
            .child(SettingsStack::titled("Machines").children(machine_entries))
            .child(SettingsStack::titled("Add host").child(destination))
            .into_any_element()
    }

    fn agents_section(&self, cx: &mut Context<Self>) -> AnyElement {
        use zz_ui::settings::providers::{ProvidersHost, providers_stack};

        let mux = self.mux.read(cx);
        let hosts = mux
            .fleet_hosts()
            .map(|(host, name, state, _)| {
                let shown = match state {
                    HostState::Connected => mux
                        .host_providers(host)
                        .cloned()
                        .map_or(ProvidersHost::Waiting, ProvidersHost::Report),
                    state => ProvidersHost::Offline(state.label().into()),
                };
                let title = if host == HostId::LOCAL {
                    if cfg!(target_os = "macos") {
                        "This Mac"
                    } else {
                        "This computer"
                    }
                    .to_owned()
                } else {
                    name.to_owned()
                };
                (host, title, shown)
            })
            .collect::<Vec<_>>();
        if mux.providers_unrequested() {
            let mux = self.mux.clone();
            cx.defer(move |cx| mux.update(cx, |mux, _| mux.request_providers(false)));
        }
        let stacks = hosts.into_iter().map(|(host, title, shown)| {
            let mux = self.mux.clone();
            providers_stack(
                format!("{host:?}"),
                title,
                &shown,
                move |provider, _, cx| {
                    mux.read(cx).update_provider(host, provider);
                },
                cx,
            )
        });
        Self::scroll_column("settings-agents")
            .child(settings_page_title(SettingsSection::Agents, cx))
            .children(stacks)
            .child(
                h_flex().justify_end().child(
                    Button::new("settings-providers-check")
                        .label("Check again")
                        .on_click(cx.listener(|settings, _, _, cx| {
                            settings
                                .mux
                                .update(cx, |mux, _| mux.request_providers(true));
                        })),
                ),
            )
            .into_any_element()
    }

    fn advanced_section(resolved: &AppConfig, cx: &Context<Self>) -> AnyElement {
        Self::scroll_column("settings-advanced")
            .child(settings_page_title(SettingsSection::Advanced, cx))
            .child(
                SettingsStack::titled("Command palette")
                    .child(Self::palette_choice_setting(
                        ConfigKey::PaletteWindowLayout,
                        "Navigation layout",
                        resolved.palette_window_layout.value.as_str(),
                        &[("grouped", "Tree"), ("flat", "Flat")],
                        cx,
                    ))
                    .child(Self::boolean_setting(
                        ConfigKey::PaletteShowKeys,
                        "Command shortcuts",
                        resolved.palette_show_keys,
                    ))
                    .child(
                        Self::boolean_setting(
                            ConfigKey::PaletteVimMode,
                            "Vim mode",
                            resolved.palette_vim_mode,
                        )
                        .description("Esc for normal mode: j k move, l into, h up"),
                    ),
            )
            .when(crate::profile::profile(cx).has_tray, |column| {
                column.child(
                    SettingsStack::titled("Tray").child(
                        Self::boolean_setting(ConfigKey::Tray, "Tray icon", resolved.tray)
                            .description("Closing the window keeps zz running there"),
                    ),
                )
            })
            .child(
                SettingsStack::titled("Coding agents").child(
                    SettingEntry::new("Claude Code and Codex")
                        .description(
                            "A startup hook so the ones in terminal panes know they run in zz",
                        )
                        .control(
                            Button::new("settings-agent-setup")
                                .label("Set up")
                                .on(settings_card_ground(cx))
                                .on_click(|_, window, cx| {
                                    crate::config::agent_setup::set_up_agents(window, cx);
                                }),
                        ),
                ),
            )
            .child(
                SettingsStack::titled("Notifications").child(
                    Self::boolean_setting(
                        ConfigKey::Notifications,
                        "Notify when a pane needs you",
                        resolved.notifications,
                    )
                    .description("While zz is in the background"),
                ),
            )
            .when(crate::profile::profile(cx).has_daemon_lifecycle, |column| {
                column.child(
                    SettingsStack::titled("Daemon").child(
                        Self::boolean_setting(
                            ConfigKey::QuitDaemonOnExit,
                            "Quit daemon on exit",
                            resolved.quit_daemon_on_exit,
                        )
                        .description("Ends sessions that are still open"),
                    ),
                )
            })
            .when(
                cfg!(any(feature = "agent-pane", feature = "editor-pane")),
                |column| {
                    column.child(SettingsStack::titled("Experimental").when(
                        cfg!(feature = "editor-pane"),
                        |group| {
                            group.child(Self::boolean_setting(
                                ConfigKey::ExperimentalEditorPane,
                                "Editor pane",
                                resolved.experimental_editor_pane,
                            ))
                        },
                    ))
                },
            )
            .into_any_element()
    }

    fn palette_choice_setting(
        key: ConfigKey,
        title: &'static str,
        value: &'static str,
        choices: &'static [(&'static str, &'static str)],
        cx: &Context<Self>,
    ) -> SettingEntry {
        let label = choices
            .iter()
            .find(|(choice, _)| *choice == value)
            .map_or(value, |(_, label)| *label);
        SettingEntry::new(title).control(
            Button::new(format!("settings-{}", key.as_str()))
                .label(label)
                .dropdown_caret(true)
                .on(settings_card_ground(cx))
                .dropdown_menu_with_anchor(ala::Anchor::TopRight, move |menu, _, _| {
                    choices.iter().fold(menu, |menu, &(choice, label)| {
                        menu.item(PopupMenuItem::new(label).checked(choice == value).on_click(
                            move |_, _, cx| {
                                if let Err(error) = set_config_key(key, choice) {
                                    report_write_error("set", key.as_str(), &error, cx);
                                } else {
                                    refresh_settings_preview(key, cx);
                                }
                            },
                        ))
                    })
                }),
        )
    }

    fn updates_stack(cx: &Context<Self>) -> SettingsStack {
        use crate::update::{self, CheckState};

        let resolved = config::resolved_config(cx);
        let status = update::status(cx);
        let check = status.as_ref().map(|status| &status.check);
        let checking = matches!(check, Some(CheckState::Checking));
        let check_button = Button::new("settings-update-check")
            .label(if checking { "Checking…" } else { "Check now" })
            .on(settings_card_ground(cx))
            .disabled(checking)
            .on_click(|_, _, cx| update::check_now(cx))
            .into_any_element();
        let (state, control): (SharedString, AnyElement) = match (status.as_ref(), check) {
            (Some(status), Some(CheckState::Available(release))) => {
                let notes_url = release.url.clone();
                (
                    format!("zz {} is out, you have {}", release.version, status.current).into(),
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("settings-update-install")
                                .primary()
                                .label(format!("Update to {}", release.version))
                                .on(settings_card_ground(cx))
                                .on_click(|_, window, cx| update::install(window, cx)),
                        )
                        .child(
                            Button::new("settings-update-notes")
                                .icon(zz_ui::IconName::ExternalLink)
                                .label("What's new")
                                .on(settings_card_ground(cx))
                                .on_click(move |_, _, cx| cx.open_url(&notes_url)),
                        )
                        .into_any_element(),
                )
            }
            (Some(status), Some(CheckState::UpToDate)) => (
                format!(
                    "zz {} is the newest {} release",
                    status.current,
                    status.channel.label()
                )
                .into(),
                check_button,
            ),
            (_, Some(CheckState::Failed(error))) => {
                (format!("The check failed: {error}").into(), check_button)
            }
            (_, Some(CheckState::Checking)) => ("Asking GitHub".into(), check_button),
            _ => ("Not checked yet".into(), check_button),
        };
        SettingsStack::titled("Updates")
            .child(
                Self::boolean_setting(
                    ConfigKey::CheckForUpdates,
                    "Check for updates",
                    resolved.check_for_updates,
                )
                .description("Once a day, one anonymous request to GitHub"),
            )
            .child(
                SettingEntry::new("Latest release")
                    .description(state)
                    .control(control),
            )
    }

    fn about_section(cx: &Context<Self>) -> AnyElement {
        Self::scroll_column("settings-about")
            .child(zz_ui::settings::about::about_hero(
                img(crate::app_icon::about_logo(cx.theme().mode))
                    .size(px(zz_ui::settings::about::ABOUT_LOGO_SIZE)),
                cx,
            ))
            .child(Self::updates_stack(cx))
            .child(zz_ui::settings::about::about_build_stack(
                platform(),
                zz_ui::settings::about::about_copy_button("settings-about-copy-build-info")
                    .on(settings_card_ground(cx))
                    .on_click(|_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            zz_ui::settings::about::build_info(&platform()),
                        ));
                        toast::push(Notification::success("Copied build information"), cx);
                    }),
                cx,
            ))
            .child(zz_ui::settings::about::about_project_stack(cx))
            .into_any_element()
    }

    fn config_file_section(&self, kind: ConfigFileKind, cx: &mut Context<Self>) -> AnyElement {
        let file = self.config_file_editor(kind);
        let editor = file.editor.clone();
        let path = file.path.as_ref().map_or_else(
            || kind.file_name().to_owned(),
            |path| path.display().to_string(),
        );
        let dirty = file.editor.read(cx).value().as_ref() != file.saved;
        let error = file.error.clone();
        let local = {
            let mux = self.mux.read(cx);
            mux.is_connected() && mux.attached_host() == HostId::LOCAL
        };

        Self::scroll_column("settings-config-file")
            .child(
                settings_page_content()
                    .gap(px(10.0))
                    .child(settings_page_title(kind.section(), cx))
                    .when(kind == ConfigFileKind::Terminal, |page| {
                        page.child(settings_list_group_header("Preview", cx))
                            .children(self.terminal_preview.clone())
                    })
                    .when(kind == ConfigFileKind::Mux, |page| {
                        page.child(
                            SettingsStack::titled("Navigation")
                                .child(
                                    Self::boolean_setting(
                                        ConfigKey::PickerFocusSidebar,
                                        "Focus sidebar for session and window pickers",
                                        config::resolved_config(cx).picker_focus_sidebar,
                                    )
                                    .description("Prefix + s and Prefix + w"),
                                )
                                .child(
                                    Self::numeric_setting(
                                        "Which-key delay",
                                        &self.which_key_delay,
                                        cx,
                                    )
                                    .description("Milliseconds after the prefix, 0 turns it off"),
                                ),
                        )
                        .child(self.mux_splits_section(cx))
                    })
                    .child(self.file_options_section(kind, cx))
                    .child(self.file_import_section(kind, cx))
                    .child(
                        div()
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(zz_ui::rems_from_px(11.0))
                                    .text_color(cx.theme().foreground.muted())
                                    .text_ellipsis()
                                    .child(path),
                            )
                            .when(kind == ConfigFileKind::Mux, |row| {
                                row.child(
                                    Button::new("settings-reload-mux-config")
                                        .label("Reload")
                                        .disabled(
                                            dirty || !local || self.pending_file_command.is_some(),
                                        )
                                        .tooltip(if dirty {
                                            "Save your edits first"
                                        } else if local {
                                            "Reload zz/mux.conf"
                                        } else {
                                            "Connect to a local session to reload"
                                        })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.reload_mux_configuration(cx);
                                        })),
                                )
                            })
                            .child(
                                Button::new(kind.save_button_id())
                                    .label("Save")
                                    .disabled(!dirty || self.pending_file_command.is_some())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.save_config_editor(kind, cx);
                                    })),
                            ),
                    )
                    .when_some(error, |page, error| {
                        page.child(
                            div()
                                .flex_none()
                                .text_size(zz_ui::rems_from_px(11.0))
                                .text_color(cx.theme().warning)
                                .child(error),
                        )
                    })
                    .child(
                        settings_preview(cx.theme().editor_background(), cx)
                            .flex()
                            .flex_none()
                            .h(px(320.0))
                            .p(px(CONFIG_EDITOR_PADDING))
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(px(CONFIG_EDITOR_FONT_SIZE))
                            .child(CodeEditor::new(&editor)),
                    ),
            )
            .into_any_element()
    }

    fn panes_section(&self, resolved: &AppConfig, cx: &Context<Self>) -> AnyElement {
        let gaps = resolved.pane_gaps.value;
        zz_ui::settings::panes_page(
            zz_ui::settings::panes_preview::PanesPreview {
                gaps,
                margin: resolved.pane_margin.value,
                inactive_opacity: resolved.pane_inactive_opacity.value,
            },
            [Self::boolean_setting(
                ConfigKey::PaneGaps,
                "Pane gaps",
                resolved.pane_gaps,
            )],
            [Self::numeric_setting(
                "Background opacity",
                &self.pip_background_opacity,
                cx,
            )],
            [
                Self::numeric_setting("Inactive pane opacity", &self.pane_inactive_opacity, cx)
                    .description("1 turns dimming off"),
                Self::numeric_setting("Selected pane contrast", &self.pane_glow_strength, cx)
                    .when(!gaps, |entry| entry.description(NEEDS_PANE_GAPS))
                    .disabled(!gaps),
            ],
            [Self::numeric_setting("Pane margin", &self.pane_margin, cx)
                .when(!gaps, |entry| entry.description(NEEDS_PANE_GAPS))
                .disabled(!gaps)],
            cx,
        )
        .into_any_element()
    }

    fn editor_section(&self, resolved: &AppConfig, cx: &Context<Self>) -> AnyElement {
        Self::scroll_column("settings-editor")
            .child(settings_page_title(SettingsSection::Editor, cx))
            .child(
                SettingsStack::titled("Typography").child(Self::numeric_setting(
                    "Font size",
                    &self.editor_font_size,
                    cx,
                )),
            )
            .child(
                SettingsStack::titled("Display")
                    .child(Self::boolean_setting(
                        ConfigKey::EditorLineNumbers,
                        "Line numbers",
                        resolved.editor_line_numbers,
                    ))
                    .child(Self::boolean_setting(
                        ConfigKey::EditorRelativeLineNumbers,
                        "Relative line numbers",
                        resolved.editor_relative_line_numbers,
                    ))
                    .child(Self::boolean_setting(
                        ConfigKey::EditorSoftWrap,
                        "Soft wrap",
                        resolved.editor_soft_wrap,
                    ))
                    .child(Self::boolean_setting(
                        ConfigKey::EditorVimMode,
                        "Vim mode",
                        resolved.editor_vim_mode,
                    )),
            )
            .into_any_element()
    }

    fn config_file_editor(&self, kind: ConfigFileKind) -> &ConfigFileEditor {
        let editor = match kind {
            ConfigFileKind::Mux => self.mux_config_editor.as_ref(),
            ConfigFileKind::Terminal => self.terminal_config_editor.as_ref(),
        };
        editor.expect("the editor is initialized before rendering or updating its section")
    }

    fn config_file_editor_mut(&mut self, kind: ConfigFileKind) -> &mut ConfigFileEditor {
        let editor = match kind {
            ConfigFileKind::Mux => self.mux_config_editor.as_mut(),
            ConfigFileKind::Terminal => self.terminal_config_editor.as_mut(),
        };
        editor.expect("the editor is initialized before rendering or updating its section")
    }

    fn reload_config_editor_if_clean(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kind = match section {
            SettingsSection::Multiplexer => ConfigFileKind::Mux,
            SettingsSection::Terminal => ConfigFileKind::Terminal,
            _ => return,
        };
        let file = self.config_file_editor(kind);
        if file.editor.read(cx).value().as_ref() == file.saved {
            self.reload_config_editor(kind, window, cx);
        }
    }

    fn reload_config_editor(
        &mut self,
        kind: ConfigFileKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self
            .config_file_editor(kind)
            .path
            .clone()
            .map_or_else(|| config_file_path(kind), Ok);
        let path = match path {
            Ok(path) => path,
            Err(error) => {
                self.config_file_editor_mut(kind).error =
                    Some(format!("Could not locate {}: {error}", kind.file_name()));
                cx.notify();
                return;
            }
        };
        match config::read_config_editor_source(&path, kind.max_bytes()) {
            Ok(source) => {
                let source = kind.editor_view(source);
                let editor = self.config_file_editor(kind).editor.clone();
                editor.update(cx, |editor, cx| editor.set_value(&source, window, cx));
                let file = self.config_file_editor_mut(kind);
                file.path = Some(path);
                file.saved = source;
                file.error = None;
            }
            Err(error) => {
                let file = self.config_file_editor_mut(kind);
                file.path = Some(path);
                file.error = Some(format!("Could not read {}: {error}", kind.file_name()));
            }
        }
        cx.notify();
    }

    fn save_config_editor(&mut self, kind: ConfigFileKind, cx: &mut Context<Self>) {
        let source = self
            .config_file_editor(kind)
            .editor
            .read(cx)
            .value()
            .to_string();
        let path = self
            .config_file_editor(kind)
            .path
            .clone()
            .map_or_else(|| config_file_path(kind), Ok);
        let path = match path {
            Ok(path) => path,
            Err(error) => {
                self.config_file_editor_mut(kind).error =
                    Some(format!("Could not locate {}: {error}", kind.file_name()));
                toast::push(
                    Notification::error(format!("Could not save {}: {error}", kind.file_name())),
                    cx,
                );
                cx.notify();
                return;
            }
        };
        let written = match kind {
            ConfigFileKind::Mux => {
                config::write_config_editor_source(&path, &source, kind.max_bytes())
            }
            ConfigFileKind::Terminal => config::save_appearance_editor(&path, &source),
        };
        if let Err(error) = written {
            let file = self.config_file_editor_mut(kind);
            file.path = Some(path);
            file.error = Some(format!("Could not save {}: {error}", kind.file_name()));
            toast::push(
                Notification::error(format!("Could not save {}: {error}", kind.file_name())),
                cx,
            );
            cx.notify();
            return;
        }

        let file = self.config_file_editor_mut(kind);
        file.path = Some(path.clone());
        file.saved = source;
        file.error = None;
        if kind == ConfigFileKind::Mux {
            self.reload_mux_configuration(cx);
        } else {
            config::request_daemon_reload(cx);
        }
        toast::push(
            Notification::success(format!("Saved {}", path.display())),
            cx,
        );
        cx.notify();
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if cx
            .try_global::<PendingMuxImport>()
            .is_some_and(|pending| pending.0)
        {
            cx.set_global(PendingMuxImport(false));
            self.set_section(SettingsSection::Multiplexer, window, cx);
        }
        let _ = self.ensure_section_state(self.section, window, cx);
        let resolved = self.synchronize_numeric_inputs(window, cx);
        let browser = self.synchronize_browser_input(window, cx);
        self.synchronize_ui_zoom_input(window, cx);
        self.synchronize_ui_font(window, cx);
        self.synchronize_terminal_editor(window, cx);
        self.synchronize_mux_splits(window, cx);
        self.synchronize_file_options(window, cx);
        self.synchronize_terminal_preview(cx);

        let content = match self.section {
            SettingsSection::Appearance => self.appearance_section(&resolved, cx),
            SettingsSection::StatusBar => self.status_bar_section(&resolved, cx),
            SettingsSection::Browser => self.browser_section(&browser, &resolved, cx),
            SettingsSection::Editor => self.editor_section(&resolved, cx),
            SettingsSection::Panes => self.panes_section(&resolved, cx),
            SettingsSection::Hosts => self.hosts_section(cx),
            SettingsSection::Agents => self.agents_section(cx),
            SettingsSection::Advanced => Self::advanced_section(&resolved, cx),
            SettingsSection::Terminal => self.config_file_section(ConfigFileKind::Terminal, cx),
            SettingsSection::Multiplexer => self.config_file_section(ConfigFileKind::Mux, cx),
            SettingsSection::About => Self::about_section(cx),
        };
        div()
            .id("settings-route")
            .track_focus(&self.focus_handle)
            .relative()
            .flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .text_color(cx.theme().foreground)
            .when(
                !zz_ui::draws_window_controls(window)
                    && !window.is_fullscreen()
                    && !crate::profile::profile(cx).fixed_window,
                |this| {
                    this.child(window_drag_handle(
                        "settings-top-drag",
                        div()
                            .id("settings-top-strip")
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(cx.theme().title_bar_height())
                            .window_control_area(WindowControlArea::Drag),
                        window,
                        cx,
                    ))
                },
            )
            .child(content)
    }
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus()
    }
}

fn boolean_control(
    id: &str,
    checked: bool,
    commit: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Switch {
    Switch::new(format!("settings-{id}"))
        .checked(checked)
        .on_click(commit)
}

fn numeric_control(input: &Entity<InputState>, cx: &App) -> ala::Div {
    div()
        .w(px(CONTROL_WIDTH))
        .flex_none()
        .child(NumberInput::new(input).on(settings_card_ground(cx)))
}

fn select_control(select: &Entity<SelectState<Vec<SettingsSelectItem>>>, cx: &App) -> ala::Div {
    div()
        .flex_none()
        .child(Select::new(select).on(settings_card_ground(cx)))
}

fn settings_select_state(
    choices: &[(&str, &str)],
    selected: &str,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Entity<SelectState<Vec<SettingsSelectItem>>> {
    let selected_index = choices
        .iter()
        .position(|(_, value)| *value == selected)
        .map(|index| IndexPath::default().row(index));
    let items = choices
        .iter()
        .map(|(label, value)| SettingsSelectItem::new(*label, *value))
        .collect();
    cx.new(|cx| SelectState::new(items, selected_index, window, cx))
}

impl ConfigFileKind {
    const fn section(self) -> SettingsSection {
        match self {
            Self::Mux => SettingsSection::Multiplexer,
            Self::Terminal => SettingsSection::Terminal,
        }
    }

    const fn file_name(self) -> &'static str {
        match self {
            Self::Mux => "zz/mux.conf",
            Self::Terminal => "zz/config",
        }
    }

    const fn save_button_id(self) -> &'static str {
        match self {
            Self::Mux => "settings-save-mux-config",
            Self::Terminal => "settings-save-terminal-config",
        }
    }

    const fn placeholder(self) -> &'static str {
        match self {
            Self::Mux => "# zz multiplexer configuration",
            Self::Terminal => "# terminal appearance, in Ghostty's key = value dialect",
        }
    }

    const fn language(self) -> &'static str {
        match self {
            Self::Mux => "tmux",
            Self::Terminal => "text",
        }
    }

    const fn max_bytes(self) -> usize {
        match self {
            Self::Mux => 1024 * 1024,
            Self::Terminal => config::MAX_CONFIG_BYTES,
        }
    }

    fn editor_view(self, source: String) -> String {
        match self {
            Self::Mux => source,
            Self::Terminal => config::appearance_editor_view(&source),
        }
    }
}

fn config_file_editor(
    kind: ConfigFileKind,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> ConfigFileEditor {
    let (path, saved, error) = match config_file_path(kind) {
        Ok(path) => match config::read_config_editor_source(&path, kind.max_bytes()) {
            Ok(source) => (Some(path), kind.editor_view(source), None),
            Err(error) => (
                Some(path),
                String::new(),
                Some(format!("Could not read {}: {error}", kind.file_name())),
            ),
        },
        Err(error) => (
            None,
            String::new(),
            Some(format!("Could not locate {}: {error}", kind.file_name())),
        ),
    };
    let editor = cx.new(|cx| {
        let mut editor = CodeEditorState::new(window, cx)
            .language(kind.language())
            .soft_wrap(false)
            .placeholder(kind.placeholder())
            .default_value(&saved);
        editor.set_line_numbers(false, cx);
        editor
    });
    ConfigFileEditor {
        path,
        editor,
        saved,
        error,
    }
}

fn config_editor_subscription(
    editor: &Entity<CodeEditorState>,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Subscription {
    cx.subscribe_in(
        editor,
        window,
        move |_, _, event: &CodeEditorEvent, _, cx| {
            if matches!(event, CodeEditorEvent::Change) {
                cx.notify();
            }
        },
    )
}

fn config_file_path(kind: ConfigFileKind) -> io::Result<PathBuf> {
    match kind {
        ConfigFileKind::Mux => zz_daemon_client::mux_config_write_path().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "cannot create zz/mux.conf because neither XDG_CONFIG_HOME nor HOME is available",
            )
        }),
        ConfigFileKind::Terminal => config::import_target_path(),
    }
}

fn text_value_input(
    value: &str,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).default_value(value))
}

fn browser_hotkey_subscription(
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        |settings, input, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                settings.commit_browser_hotkey(input, window, cx);
            }
        },
    )
}

fn search_provider_select(
    selected: SearchProvider,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Entity<SelectState<Vec<SettingsSelectItem>>> {
    let choices: Vec<(&str, &str)> = SearchProvider::ALL
        .into_iter()
        .map(|provider| (provider.title(), provider.as_str()))
        .collect();
    settings_select_state(&choices, selected.as_str(), window, cx)
}

fn search_provider_subscription(
    select: &Entity<SelectState<Vec<SettingsSelectItem>>>,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Subscription {
    cx.subscribe_in(
        select,
        window,
        |_, _, event: &SelectEvent<Vec<SettingsSelectItem>>, _, cx| {
            let SelectEvent::Confirm(Some(provider)) = event else {
                return;
            };
            if let Err(error) = set_config_key(ConfigKey::BrowserSearchProvider, provider) {
                report_write_error("set", ConfigKey::BrowserSearchProvider.as_str(), &error, cx);
            }
        },
    )
}

pub(crate) fn with_theme_picks<R>(cx: &App, show: impl FnOnce(&ThemePicks) -> R) -> R {
    let themes = config::themes(cx);
    let choices = interface_theme::builtin_themes()
        .iter()
        .map(|(id, theme)| (SharedString::from(*id), theme))
        .chain(themes.saved.iter().map(|(id, theme)| (id.clone(), theme)))
        .collect::<Vec<_>>();
    let selected = choices
        .iter()
        .position(|(id, _)| *id == themes.selected)
        .unwrap_or(0);
    let overrides = config::theme_overrides(cx);
    show(&ThemePicks {
        themes: choices,
        selected,
        overrides: &overrides,
    })
}

pub(crate) fn written(key: ConfigKey, result: std::io::Result<()>, cx: &mut App) {
    match result {
        Ok(()) => refresh_settings_preview(key, cx),
        Err(error) => report_write_error("set", key.as_str(), &error, cx),
    }
}

fn custom_accent(accent: zz_client::appearance::AccentSetting) -> Option<ala::Hsla> {
    match accent {
        zz_client::appearance::AccentSetting::Color([r, g, b, a]) => {
            Some(ala::Rgba { r, g, b, a }.into())
        }
        _ => None,
    }
}

fn shown_ui_font(
    family: &ConfigValue<Option<String>>,
    theme_font: Option<&SharedString>,
) -> String {
    family
        .value
        .clone()
        .or_else(|| {
            (family.provenance == ConfigProvenance::Default)
                .then(|| theme_font.map(ToString::to_string))
                .flatten()
        })
        .unwrap_or_default()
}

fn ui_font_subscription(
    select: &Entity<SelectState<Vec<SettingsSelectItem>>>,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Subscription {
    cx.subscribe_in(
        select,
        window,
        |_, _, event: &SelectEvent<Vec<SettingsSelectItem>>, _, cx| {
            let SelectEvent::Confirm(Some(value)) = event else {
                return;
            };
            let family = (!value.is_empty()).then_some(value.as_str());
            if let Err(error) = config::set_ui_font_family(family) {
                report_write_error("set", ConfigKey::UiFontFamily.as_str(), &error, cx);
            }
        },
    )
}

fn numeric_value_input(
    key: ConfigKey,
    value: f32,
    step: f64,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Entity<InputState> {
    let (min, max) = numeric_range(key);
    cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(numeric_input_text(key, value))
            .step(step)
            .min(f64::from(min))
            .max(f64::from(max))
    })
}

fn ui_zoom_input(window: &mut Window, cx: &mut Context<SettingsView>) -> Entity<InputState> {
    let percent = crate::ui_scale::percent(cx);
    cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(percent.to_string())
            .step(f64::from(crate::ui_scale::UI_ZOOM_STEP))
            .min(f64::from(crate::ui_scale::MIN_UI_ZOOM))
            .max(f64::from(crate::ui_scale::MAX_UI_ZOOM))
    })
}

fn ui_zoom_subscription(
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |_, input, event: &InputEvent, window, cx| match event {
            InputEvent::Change => SettingsView::preview_ui_zoom(input, cx),
            InputEvent::PressEnter { .. } | InputEvent::Blur => {
                SettingsView::commit_ui_zoom_input(input, window, cx);
            }
            _ => {}
        },
    )
}

fn numeric_input_subscription(
    input: &Entity<InputState>,
    key: ConfigKey,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Subscription {
    cx.subscribe_in(
        input,
        window,
        move |settings, input, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                settings.commit_numeric_input(key, input, true, window, cx);
            } else if is_pane_setting(key) && matches!(event, InputEvent::Change) {
                settings.commit_numeric_input(key, input, false, window, cx);
            }
        },
    )
}

fn is_pane_setting(key: ConfigKey) -> bool {
    matches!(
        key,
        ConfigKey::PaneGaps
            | ConfigKey::PipBackgroundOpacity
            | ConfigKey::PaneGlowStrength
            | ConfigKey::PaneInactiveOpacity
            | ConfigKey::PaneMargin
    )
}

fn refresh_settings_preview(key: ConfigKey, cx: &mut App) {
    if !is_pane_setting(key)
        && !matches!(
            key,
            ConfigKey::WidgetCornerRadius
                | ConfigKey::ChromeContrast
                | ConfigKey::InterfaceTheme
                | ConfigKey::ChromeAccent
                | ConfigKey::ChromeTint
                | ConfigKey::ChromeTintHue
                | ConfigKey::WindowBackgroundOpacity
                | ConfigKey::PaletteWindowLayout
                | ConfigKey::PaletteShowKeys
                | ConfigKey::PaletteVimMode
                | ConfigKey::PickerFocusSidebar
                | ConfigKey::WhichKeyDelay
                | ConfigKey::StatusBadges
                | ConfigKey::StatusHost
                | ConfigKey::StatusUpdate
                | ConfigKey::ProgramColors
        )
    {
        return;
    }
    config::reload(cx);
}

fn synchronize_f32_input(
    input: &Entity<InputState>,
    value: f32,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) {
    if input.read(cx).value().parse::<f32>().ok() != Some(value) {
        input.update(cx, |input, cx| {
            input.set_value(value.to_string(), window, cx);
        });
    }
}

fn synchronize_text_input(
    input: &Entity<InputState>,
    value: &str,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) {
    if input.read(cx).value().as_ref() != value {
        input.update(cx, |input, cx| {
            input.set_value(value.to_owned(), window, cx);
        });
    }
}

fn numeric_range(key: ConfigKey) -> (f32, f32) {
    let (min, max) = key
        .numeric_range()
        .expect("every Settings numeric field edits a numeric key");
    let scale = numeric_input_scale(key);
    (min * scale, max * scale)
}

fn numeric_input_scale(key: ConfigKey) -> f32 {
    if matches!(
        key,
        ConfigKey::PaneGlowStrength | ConfigKey::PipBackgroundOpacity
    ) {
        100.0
    } else {
        1.0
    }
}

fn numeric_input_text(key: ConfigKey, value: f32) -> String {
    if matches!(
        key,
        ConfigKey::PaneGlowStrength | ConfigKey::PipBackgroundOpacity
    ) {
        format!("{:.2}", value * numeric_input_scale(key))
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    } else {
        value.to_string()
    }
}

fn validate_numeric_value(key: ConfigKey, value: &str) -> Result<f32, String> {
    let (min, max) = numeric_range(key);
    let rejected = || format!("enter a number between {min} and {max}");
    let value = value.parse::<f32>().map_err(|_| rejected())?;
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(rejected());
    }
    Ok(value)
}

fn numeric_input_matches_value(key: ConfigKey, text: &str, value: f32) -> bool {
    validate_numeric_value(key, text)
        .is_ok_and(|shown| (shown / numeric_input_scale(key) - value).abs() < f32::EPSILON)
}

fn numeric_config_value(config: &AppConfig, key: ConfigKey) -> ConfigValue<f32> {
    match key {
        ConfigKey::PipBackgroundOpacity => config.pip_background_opacity,
        ConfigKey::WindowBackgroundOpacity => config.window_background_opacity,
        ConfigKey::PaneGlowStrength => config.pane_glow_strength,
        ConfigKey::PaneInactiveOpacity => config.pane_inactive_opacity,
        ConfigKey::PaneMargin => config.pane_margin,
        ConfigKey::WidgetCornerRadius => config.widget_corner_radius,
        ConfigKey::ChromeContrast => config.chrome_contrast,
        ConfigKey::AnimationSpeed => config.animation_speed,
        ConfigKey::ChromeTint => config.chrome_tint,
        ConfigKey::ChromeTintHue => config.chrome_tint_hue,
        ConfigKey::WindowCornerRadius => config.window_corner_radius,
        ConfigKey::EditorFontSize => config.editor_font_size,
        ConfigKey::WhichKeyDelay => config.which_key_delay,
        ConfigKey::UseSystemTitlebar
        | ConfigKey::PaletteWindowLayout
        | ConfigKey::PaletteShowKeys
        | ConfigKey::PaletteVimMode
        | ConfigKey::PickerFocusSidebar
        | ConfigKey::WindowBackgroundBlur
        | ConfigKey::Animations
        | ConfigKey::Tray
        | ConfigKey::Notifications
        | ConfigKey::QuitDaemonOnExit
        | ConfigKey::AutoRestartStaleDaemon
        | ConfigKey::CheckForUpdates
        | ConfigKey::StatusBadges
        | ConfigKey::StatusHost
        | ConfigKey::StatusUpdate
        | ConfigKey::FlatSidebar
        | ConfigKey::ProgramColors
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
        | ConfigKey::ThemeMode
        | ConfigKey::InterfaceTheme
        | ConfigKey::UiFontFamily
        | ConfigKey::AppIcon
        | ConfigKey::ChromeAccent => {
            unreachable!("only numeric settings use numeric inputs")
        }
    }
}

fn platform() -> String {
    format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH)
}

fn import_color_scheme(cx: &App) -> TerminalColorScheme {
    crate::theme::terminal_appearance(cx).map_or(TerminalColorScheme::Dark, |appearance| {
        appearance.color_scheme
    })
}

pub(crate) fn run_import(cx: &mut App) {
    match crate::config::import::import_ghostty_config(import_color_scheme(cx)) {
        Ok(report) if report.imported_anything() => {
            log::info!(
                target: "zz::config",
                "imported external configuration ghostty_keys={}",
                report.ghostty_keys,
            );
            let mut copied = Vec::new();
            if let Some(path) = &report.config_path {
                copied.push(format!("Ghostty appearance into {}", path.display()));
            }
            toast::push(
                Notification::success(format!("Imported {}", copied.join(" and "))),
                cx,
            );
            config::request_daemon_reload(cx);
        }
        Ok(_) => {
            toast::push(
                Notification::info("Nothing to import: no Ghostty configuration found"),
                cx,
            );
        }
        Err(error) => {
            log::warn!(
                target: "zz::config",
                "could not import external configuration error={error}",
            );
            toast::push(
                Notification::error(format!("Could not import configuration: {error}")),
                cx,
            );
        }
    }
}

fn import_theme_from_clipboard(cx: &mut App) {
    let source = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .unwrap_or_default();
    match interface_theme::parse_theme(&source) {
        Ok(theme) => save_and_select_theme(theme, cx),
        Err(error) => toast::push(
            Notification::error(format!(
                "The clipboard holds no theme: {error}. Copy one with Copy JSON in the storybook."
            )),
            cx,
        ),
    }
}

fn save_and_select_theme(mut theme: InterfaceTheme, cx: &mut App) {
    let saved = config::themes::themes_directory().and_then(|directory| {
        let name = config::themes::next_theme_name(&directory);
        theme.name_after(&name);
        let path = config::themes::save_theme(&directory, &name, &theme.to_json())?;
        set_interface_theme(&name, theme.materials.font.is_some())?;
        Ok(path)
    });
    match saved {
        Ok(path) => {
            refresh_settings_preview(ConfigKey::InterfaceTheme, cx);
            toast::push(
                Notification::success(format!("Saved the theme as {}", path.display())),
                cx,
            );
        }
        Err(error) => report_write_error("save", "theme", &error, cx),
    }
}

fn report_write_error(operation: &str, key: &str, error: &std::io::Error, cx: &mut App) {
    log::warn!(
        target: "zz::config",
        "could not {operation} configuration key={key} error={error}",
    );
    toast::push(
        Notification::error(format!("Could not update {key}: {error}")),
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_inputs_convert_and_reject_invalid_edits() {
        for (key, default, max) in [
            (ConfigKey::PipBackgroundOpacity, "80", 100.0),
            (ConfigKey::PaneGlowStrength, "100", 200.0),
        ] {
            assert_eq!(numeric_range(key), (0.0, max));
            assert_eq!(validate_numeric_value(key, &max.to_string()), Ok(max));
            assert!(validate_numeric_value(key, &(max + 1.0).to_string()).is_err());
            for (factor, displayed) in [(0.0, "0"), (0.15, "15"), (0.5, "50"), (1.0, "100")] {
                assert_eq!(numeric_input_text(key, factor), displayed);
                let parsed = validate_numeric_value(key, displayed).expect("valid percentage");
                assert!((parsed / numeric_input_scale(key) - factor).abs() < f32::EPSILON);
            }
            for value in ["", "-1", "NaN", "inf", "invalid"] {
                assert!(validate_numeric_value(key, value).is_err(), "{value}");
            }
            assert!(numeric_input_matches_value(key, "50.", 0.5));
            assert!(numeric_input_matches_value(key, "0.", 0.0));
            assert!(!numeric_input_matches_value(key, "", 0.5));
            assert!(!numeric_input_matches_value(key, "50.", 1.0));
            assert_eq!(
                numeric_input_text(key, numeric_config_value(&AppConfig::default(), key).value),
                default,
            );
        }
        assert_eq!(numeric_range(ConfigKey::PaneInactiveOpacity), (0.0, 1.0));
        assert_eq!(
            numeric_input_text(ConfigKey::PaneInactiveOpacity, 0.7),
            "0.7"
        );
    }
    use zz_daemon_client::{DaemonError, Endpoint};

    #[test]
    fn advanced_lists_the_other_knobs_only_while_expanded() {
        let collapsed = appearance_page_items(true, false);
        assert!(matches!(
            collapsed.last(),
            Some(AppearancePageItem::Advanced { expanded: false })
        ));
        assert!(
            !collapsed
                .iter()
                .any(|item| matches!(item, AppearancePageItem::CornerRadius))
        );

        let expanded = appearance_page_items(true, true);
        let advanced = expanded
            .iter()
            .position(|item| matches!(item, AppearancePageItem::Advanced { expanded: true }))
            .expect("Advanced heading");
        assert!(
            expanded[..advanced]
                .iter()
                .any(|item| matches!(item, AppearancePageItem::Themes))
        );
        assert!(
            expanded[advanced..]
                .iter()
                .any(|item| matches!(item, AppearancePageItem::CornerRadius))
        );
    }

    #[ala::test]
    fn heavy_section_state_is_initialized_on_first_visit(cx: &mut ala::TestAppContext) {
        use std::{cell::RefCell, rc::Rc};

        cx.update(zz_ui::init);
        let captured = Rc::new(RefCell::new(None));
        let captured_for_window = Rc::clone(&captured);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_for_window.replace(Some(cx.entity()));
            SettingsView::new(mux, window, cx)
        });
        let settings = captured.borrow().clone().expect("captured settings view");

        settings.read_with(cx, |settings, _| {
            assert!(settings.terminal_config_editor.is_none());
            assert!(settings.mux_config_editor.is_none());
            assert!(settings.hosts_state.is_none());
        });

        let initialized = cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.ensure_section_state(SettingsSection::Hosts, window, cx)
            })
        });
        assert!(initialized);
        settings.read_with(cx, |settings, _| {
            assert!(settings.hosts_state.is_some());
            assert!(settings.terminal_config_editor.is_none());
            assert!(settings.mux_config_editor.is_none());
        });

        let initialized = cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.ensure_section_state(SettingsSection::Hosts, window, cx)
            })
        });
        assert!(!initialized);

        let initialized = cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.ensure_section_state(SettingsSection::Terminal, window, cx)
            })
        });
        assert!(initialized);
        settings.read_with(cx, |settings, _| {
            assert!(settings.terminal_config_editor.is_some());
            assert!(settings.mux_config_editor.is_none());
        });

        let initialized = cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.ensure_section_state(SettingsSection::Multiplexer, window, cx)
            })
        });
        assert!(initialized);
        settings.read_with(cx, |settings, _| {
            assert!(settings.terminal_config_editor.is_some());
            assert!(settings.mux_config_editor.is_some());
        });

        let initialized = cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.ensure_section_state(SettingsSection::Multiplexer, window, cx)
            })
        });
        assert!(!initialized);
    }

    #[ala::test]
    fn ui_font_select_follows_live_config_reloads(cx: &mut ala::TestAppContext) {
        use std::{cell::RefCell, rc::Rc};

        cx.update(zz_ui::init);
        let captured = Rc::new(RefCell::new(None));
        let captured_for_window = Rc::clone(&captured);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_for_window.replace(Some(cx.entity()));
            SettingsView::new(mux, window, cx)
        });
        let settings = captured.borrow().clone().expect("captured settings view");
        let family = "Test UI font".to_owned();
        let config = AppConfig::default();

        cx.update(|window, cx| {
            cx.set_global(config);
            cx.set_global(config::UiFontConfig(zz_config::UiFontConfig {
                family: ConfigValue {
                    value: Some(family.clone()),
                    provenance: ConfigProvenance::Override,
                },
            }));
            settings.update(cx, |settings, cx| {
                settings.ui_font_family.update(cx, |select, cx| {
                    *select = SelectState::new(
                        vec![
                            SettingsSelectItem::new("System default", ""),
                            SettingsSelectItem::new(family.clone(), family.clone()),
                        ],
                        Some(IndexPath::new(0)),
                        window,
                        cx,
                    );
                });
                settings.synchronize_numeric_inputs(window, cx);
                settings.synchronize_ui_font(window, cx);
            });
        });

        settings.read_with(cx, |settings, cx| {
            assert_eq!(
                settings
                    .ui_font_family
                    .read(cx)
                    .selected_value()
                    .map(String::as_str),
                Some(family.as_str()),
            );
        });
        cx.update(|window, cx| {
            cx.set_global(config::UiFontConfig::default());
            settings.update(cx, |settings, cx| settings.synchronize_ui_font(window, cx));
        });
        settings.read_with(cx, |settings, cx| {
            assert_eq!(
                settings
                    .ui_font_family
                    .read(cx)
                    .selected_value()
                    .map(String::as_str),
                Some(""),
            );
        });
    }

    #[ala::test]
    fn submit_add_host_rejects_a_duplicate_without_changing_config(cx: &mut ala::TestAppContext) {
        use std::{cell::RefCell, rc::Rc};

        cx.update(zz_ui::init);
        let configured = config::HostEntry {
            name: "desktop".to_owned(),
            endpoint: Endpoint::parse("ssh://desktop").expect("valid test endpoint"),
        };
        cx.update(|cx| config::set_fleet_hosts_for_test(vec![configured.clone()], cx));
        let captured = Rc::new(RefCell::new(None));
        let captured_for_window = Rc::clone(&captured);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_for_window.replace(Some(cx.entity()));
            SettingsView::new(mux, window, cx)
        });
        let settings = captured.borrow().clone().expect("captured settings view");

        cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                assert!(settings.ensure_section_state(SettingsSection::Hosts, window, cx));
                let input = settings
                    .hosts_state
                    .as_ref()
                    .expect("initialized Hosts state")
                    .input
                    .clone();
                input.update(cx, |input, cx| {
                    input.set_value("fabrico@desktop", window, cx);
                });
                settings.submit_add_host(window, cx);
            });
        });

        settings.read_with(cx, |settings, _| {
            assert_eq!(
                settings
                    .hosts_state
                    .as_ref()
                    .and_then(|state| state.error.as_deref()),
                Some("A host named `desktop` already exists."),
            );
        });
        cx.update(|_, cx| {
            assert_eq!(config::fleet_hosts(cx), vec![configured]);
        });
    }

    #[ala::test]
    fn the_zoom_field_applies_complete_values_and_ignores_partial_ones(
        cx: &mut ala::TestAppContext,
    ) {
        use std::{cell::RefCell, rc::Rc};

        cx.update(zz_ui::init);
        let captured = Rc::new(RefCell::new(None));
        let captured_for_window = Rc::clone(&captured);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(DaemonError::Thread("test client".to_owned())),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_for_window.replace(Some(cx.entity()));
            SettingsView::new(mux, window, cx)
        });
        let settings = captured.borrow().clone().expect("captured settings view");
        let input = settings.read_with(cx, |settings, _| settings.ui_zoom.clone());

        let typed = |value: &str, cx: &mut ala::VisualTestContext| {
            let value = value.to_owned();
            cx.update(|window, cx| {
                input.update(cx, |input, cx| input.set_value(value, window, cx));
                settings.update(cx, |settings, cx| {
                    SettingsView::preview_ui_zoom(&input, cx);
                    settings.synchronize_ui_zoom_input(window, cx);
                    (crate::ui_scale::percent(cx), input.read(cx).value().clone())
                })
            })
        };

        assert_eq!(
            typed("1", cx).0,
            100,
            "a prefix must not zoom to the minimum"
        );
        assert_eq!(typed("15", cx), (100, "15".into()), "nor be rewritten");
        assert_eq!(typed("150", cx), (150, "150".into()));
        assert_eq!(typed("", cx), (150, "".into()), "clearing must not zoom");
        assert_eq!(
            typed("4000", cx),
            (150, "4000".into()),
            "out of range is left to the commit"
        );

        cx.update(|window, cx| {
            settings.update(cx, |_, cx| {
                SettingsView::commit_ui_zoom_input(&input, window, cx);
            });
        });
        cx.update(|_, cx| {
            let percent = crate::ui_scale::percent(cx);
            assert!(percent < 4000, "the commit clamps what the field holds");
            assert_eq!(input.read(cx).value().as_ref(), percent.to_string());
        });

        typed("250", cx);
        cx.update(|window, cx| {
            crate::ui_scale::reset(cx);
            settings.update(cx, |settings, cx| {
                settings.synchronize_ui_zoom_input(window, cx);
            });
        });
        cx.update(|_, cx| {
            assert_eq!(crate::ui_scale::percent(cx), 100);
            assert_eq!(input.read(cx).value().as_ref(), "100");
        });
    }

    #[test]
    fn config_editor_source_round_trips_and_preserves_file_on_oversize_save() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("config");
        config::write_config_editor_source(&path, "theme-mode = dark\n", 64)
            .expect("initial write");
        assert_eq!(
            config::read_config_editor_source(&path, 64).expect("read source"),
            "theme-mode = dark\n"
        );

        let error = config::write_config_editor_source(&path, "too large", 4)
            .expect_err("oversized editor content must fail");
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(
            std::fs::read_to_string(path).expect("read unchanged file"),
            "theme-mode = dark\n"
        );
    }

    #[test]
    fn missing_config_editor_source_starts_empty() {
        let directory = tempfile::tempdir().expect("temporary directory");
        assert_eq!(
            config::read_config_editor_source(&directory.path().join("missing"), 64)
                .expect("missing source"),
            ""
        );
    }
}
