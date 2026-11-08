mod file_options;
mod multiplexer;
mod sources;
mod terminal_preview;

use std::{
    collections::BTreeMap,
    io::{self, ErrorKind},
    path::PathBuf,
};
use zz_ui::shell::window_drag_handle;

use ala::{
    AnyElement, App, ClipboardItem, Context, Entity, FocusHandle, Focusable, IntoElement, Render,
    SharedString, Subscription, Window, WindowControlArea, div, img, prelude::*, px,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Icon, IconName, IndexPath, Label, TextStyle,
    blocks::notification::Notification,
    components::button::{Button, ButtonVariants as _},
    components::input::{Input, InputEvent, InputState, NumberInput},
    components::key_field::{KeyField, KeyFieldEvent, KeyFieldState},
    components::menu::{DropdownMenu as _, PopupMenuItem},
    components::select::{Select, SelectEvent, SelectState},
    components::switch::Switch,
    h_flex,
};

use crate::{
    app_icon::AppIconSetting,
    config::{
        self, AppConfig, BrowserConfig, ConfigKey, ConfigProvenance, ConfigValue,
        remove_config_key, set_config_key,
    },
    diagnostics,
    mux::{
        client::MuxClient,
        hosts::{HostId, HostState},
    },
    window::toast,
    workspace::add_host,
};
use penna::blocks::appearance::{AppearanceControls, kit_row, picker_tile, ui_font_select};
use zz_client::url_input::SearchProvider;
use zz_protocol::ConfigOverrideEntry;
use zz_terminal::TerminalColorScheme;
use zz_ui::interface_theme::{self, Appearance, AppearanceChange};
use zz_ui::settings::{
    SettingEntry, SettingsScrollColumn, SettingsSection, SettingsSelectItem, SettingsStack,
    settings_card_ground, settings_list_group_header, settings_page_content, settings_page_title,
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
const KEY_INPUT_WIDTH: f32 = 96.0;
const APP_ICON_PREVIEW_SIZE: f32 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ConfigFileKind {
    Mux,
    Terminal,
}

struct ConfigFileEditor {
    path: Option<PathBuf>,
    saved: String,
    error: Option<String>,
}

struct HostsSectionState {
    input: Entity<InputState>,
    error: Option<SharedString>,
}

use zz_ui::settings::appearance::{AppearancePageItem, ZzRow};

fn appearance_page_items(has_window_blur: bool) -> Vec<AppearancePageItem> {
    zz_ui::settings::appearance::appearance_page_items(cfg!(target_os = "macos"), has_window_blur)
}

pub(crate) struct SettingsView {
    mux: Entity<MuxClient>,
    observed: AppConfig,
    observed_browser: BrowserConfig,
    observed_ui_font: ConfigValue<Option<String>>,
    ui_font_family: Entity<SelectState<Vec<SettingsSelectItem>>>,
    browser_element_selector_hotkey: Entity<InputState>,
    browser_search_provider: Entity<SelectState<Vec<SettingsSelectItem>>>,
    ui_zoom: Entity<InputState>,
    observed_ui_zoom: u32,
    pane_glow_strength: Entity<InputState>,
    pane_inactive_opacity: Entity<InputState>,
    pane_margin: Entity<InputState>,
    editor_font_size: Entity<InputState>,
    which_key_delay: Entity<InputState>,
    appearance_controls: AppearanceControls,
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
        let ui_font_family = ui_font_select(Some(&shown_ui_font(&observed_ui_font)), window, cx);
        let browser_element_selector_hotkey =
            text_value_input(&observed_browser.element_selector_hotkey.value, window, cx);
        let browser_search_provider =
            search_provider_select(observed_browser.search_provider.value, window, cx);
        let ui_zoom = ui_zoom_input(window, cx);
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
        let appearance_controls = AppearanceControls::new(
            &config::appearance(cx),
            |change, _, cx| appearance_changed(change, cx),
            window,
            cx,
        );
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
            cx.observe_global::<config::DiscoveredHosts>(|_, cx| cx.notify()),
            cx.observe_global::<crate::update::UpdateState>(|_, cx| cx.notify()),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.pick_up_outside_edits(cx);
                }
            }),
            browser_hotkey_subscription(&browser_element_selector_hotkey, window, cx),
            search_provider_subscription(&browser_search_provider, window, cx),
            ui_font_subscription(&ui_font_family, window, cx),
            ui_zoom_subscription(&ui_zoom, window, cx),
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
            ui_font_family,
            browser_element_selector_hotkey,
            browser_search_provider,
            ui_zoom,
            observed_ui_zoom: crate::ui_scale::percent(cx),
            pane_glow_strength,
            pane_inactive_opacity,
            pane_margin,
            editor_font_size,
            which_key_delay,
            appearance_controls,
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
        *slot = Some(config_file_editor(kind));
        log::trace!(
            target: DIAGNOSTIC_TARGET,
            "initialize_section section={section:?} subscriptions=0 elapsed_us={}",
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
            match section {
                SettingsSection::Multiplexer => self.reload_config_editor(ConfigFileKind::Mux, cx),
                SettingsSection::Terminal => {
                    self.reload_config_editor(ConfigFileKind::Terminal, cx);
                }
                _ => {}
            }
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
            self.appearance_controls
                .sync(&config::appearance(cx), window, cx);
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
        if resolved == self.observed_ui_font {
            return;
        }
        let shown = shown_ui_font(&resolved);
        self.ui_font_family.update(cx, |select, cx| {
            select.set_selected_value(&shown, window, cx);
        });
        self.observed_ui_font = resolved;
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

    fn synchronize_terminal_editor(&mut self, cx: &mut Context<Self>) {
        let overrides = config::daemon_config_overrides(cx);
        if overrides == self.observed_appearance_overrides {
            return;
        }
        self.observed_appearance_overrides = overrides;
        if self.terminal_config_editor.is_some() {
            self.reload_config_editor(ConfigFileKind::Terminal, cx);
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
                Label::new(
                    TextStyle::Caption,
                    "Any local process that reaches the port can drive the logged-in browser.",
                )
                .flex_none()
                .text_color(cx.theme().warning),
            )
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
        let items = appearance_page_items(crate::profile::profile(cx).has_window_blur);
        let view = cx.entity();
        let resolved = *resolved;
        let appearance = config::appearance(cx);
        penna::blocks::appearance::appearance_page(
            SettingsSection::Appearance.title(),
            items,
            move |item, position, window, cx| {
                view.update(cx, |this, cx| {
                    let entry = this.appearance_item(item, &resolved, &appearance, window, cx);
                    entry.map_or_else(
                        || div().into_any_element(),
                        |entry| entry.position(position).into_any_element(),
                    )
                })
            },
        )
        .into_any_element()
    }

    fn appearance_item(
        &self,
        item: AppearancePageItem,
        resolved: &AppConfig,
        appearance: &Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<SettingEntry> {
        Some(match item {
            AppearancePageItem::UiFontFamily => self.ui_font_setting(cx),
            AppearancePageItem::TextSize => self.ui_zoom_setting(cx),
            AppearancePageItem::App(ZzRow::AppIcon) => Self::app_icon_setting(resolved, cx),
            AppearancePageItem::App(row @ ZzRow::TerminalColors) => Self::boolean_setting(
                ConfigKey::ChromeTerminalColors,
                row.title(),
                resolved.chrome_terminal_colors,
            )
            .description("Draw the window in the terminal theme's colors"),
            AppearancePageItem::App(row @ ZzRow::ProgramColors) => Self::boolean_setting(
                ConfigKey::ProgramColors,
                row.title(),
                resolved.program_colors,
            )
            .description("Fill sidebar tiles with the running program's colors"),
            AppearancePageItem::App(row @ ZzRow::WindowBackgroundBlur) => Self::boolean_setting(
                ConfigKey::WindowBackgroundBlur,
                row.title(),
                resolved.window_background_blur,
            ),
            AppearancePageItem::App(row @ ZzRow::WindowCornerRadius) => {
                Self::numeric_setting(row.title(), &self.window_corner_radius, cx)
                    .description("No effect with the system titlebar")
            }
            AppearancePageItem::App(row @ ZzRow::UseSystemTitlebar) => Self::boolean_setting(
                ConfigKey::UseSystemTitlebar,
                row.title(),
                resolved.use_system_titlebar,
            ),
            item => {
                return kit_row(
                    item,
                    appearance,
                    &self.appearance_controls,
                    false,
                    window,
                    cx,
                );
            }
        })
    }

    fn ui_zoom_setting(&self, cx: &Context<Self>) -> SettingEntry {
        SettingEntry::new("Text size").control(
            div()
                .w(px(CONTROL_WIDTH))
                .flex_none()
                .child(NumberInput::new(&self.ui_zoom).on(settings_card_ground(cx))),
        )
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
                        Some(shown_ui_font(&self.observed_ui_font))
                            .filter(|family| !family.is_empty())
                            .unwrap_or_else(|| "System default".to_owned()),
                    )
                    .on(settings_card_ground(cx)),
            ),
        )
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

        let hosts = config::known_hosts(cx);
        let discovered = config::discovered(cx);
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
                let found = config::is_discovered(&name, cx);
                let (description, action) = if found {
                    (format!("Found on Tailscale, {}", host.endpoint), "Hide")
                } else {
                    (host.endpoint.to_string(), "Remove host")
                };
                let entry = SettingEntry::new(name.clone())
                    .description(description)
                    .title_icon(Icon::new(glyph).text_color(color))
                    .control(
                        Button::new(format!("settings-remove-host-{name}"))
                            .ghost()
                            .icon(IconName::Xmark)
                            .tooltip(format!("{action} ({})", state.label()))
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
                Label::new(TextStyle::Caption, error)
                    .flex_none()
                    .text_color(cx.theme().warning),
            );
        }

        let mut tailscale_entries = vec![
            SettingEntry::new("Find my machines on Tailscale")
                .description(if discovered.searching {
                    "Looking…".to_owned()
                } else if discovered.summary.is_empty() {
                    "Machines signed in to your tailnet that run zz show up here".to_owned()
                } else {
                    discovered.summary.clone()
                })
                .control(boolean_control(
                    zz_daemon_client::tailnet::MODE_KEY,
                    discovered.enabled,
                    |enabled, _, cx| {
                        let key = zz_daemon_client::tailnet::MODE_KEY;
                        let value = if *enabled { "auto" } else { "off" };
                        if let Err(error) = config::set_config_key_name(key, value) {
                            report_write_error("set", key, &error, cx);
                        }
                        crate::tailnet::rescan(cx);
                    },
                )),
        ];
        if let Some(link) = discovered.link.clone()
            && let Some(code) = qr_code(&link)
        {
            tailscale_entries.push(
                SettingEntry::new("Add this machine to a phone")
                    .description(format!(
                        "With the Tailscale app on, scan this with the phone's camera to open {link} in zz"
                    ))
                    .child(code),
            );
        }
        if let Some(url) = discovered.login.clone() {
            tailscale_entries.push(
                SettingEntry::new("Sign in")
                    .description(format!("Open {url} to add this machine to your tailnet"))
                    .control(
                        Button::new("settings-tailnet-sign-in")
                            .label("Sign in")
                            .on(settings_card_ground(cx))
                            .on_click(move |_, _, cx| cx.open_url(&url)),
                    ),
            );
        }
        if discovered.own {
            tailscale_entries.push(
                SettingEntry::new("zz's own connection")
                    .description(
                        "Signs this machine out of the tailnet, deletes its keys and goes back to the Tailscale app",
                    )
                    .control(
                        Button::new("settings-tailnet-sign-out")
                            .ghost()
                            .label("Sign out")
                            .on(settings_card_ground(cx))
                            .on_click(|_, _, cx| crate::tailnet::sign_out(cx)),
                    ),
            );
        } else if discovered.enabled && discovered.link.is_none() && !discovered.searching {
            tailscale_entries.push(
                SettingEntry::new("Use zz's own Tailscale connection")
                    .description(
                        "For a machine without the Tailscale app: zz joins your tailnet by itself after you sign in",
                    )
                    .control(
                        Button::new("settings-tailnet-use-own")
                            .ghost()
                            .label("Use")
                            .on(settings_card_ground(cx))
                            .on_click(|_, _, cx| crate::tailnet::use_own(cx)),
                    ),
            );
        }
        if !discovered.hidden.is_empty() {
            tailscale_entries.push(
                SettingEntry::new("Hidden machines")
                    .description(discovered.hidden.join(", "))
                    .control(
                        Button::new("settings-show-hidden-hosts")
                            .ghost()
                            .label("Show")
                            .on(settings_card_ground(cx))
                            .on_click(|_, _, cx| {
                                if let Err(error) = config::show_hidden_hosts(cx) {
                                    report_write_error(
                                        "remove",
                                        zz_daemon_client::tailnet::HIDE_KEY,
                                        &error,
                                        cx,
                                    );
                                }
                                crate::tailnet::rescan(cx);
                            }),
                    ),
            );
        }

        Self::scroll_column("settings-hosts")
            .child(settings_page_title(SettingsSection::Hosts, cx))
            .child(SettingsStack::titled("Machines").children(machine_entries))
            .child(SettingsStack::titled("Tailscale").children(tailscale_entries))
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
                                .icon(penna::IconName::ExternalLink)
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
        use penna::blocks::settings::about::{
            about_build_stack, about_copy_button, about_hero, about_project_stack,
        };
        use zz_ui::settings::about::{ABOUT_LOGO_SIZE, ZZ_ABOUT, build_info};

        Self::scroll_column("settings-about")
            .child(about_hero(
                &ZZ_ABOUT,
                img(crate::app_icon::about_logo(cx.theme().mode)).size(px(ABOUT_LOGO_SIZE)),
                cx,
            ))
            .child(Self::updates_stack(cx))
            .child(about_build_stack(
                &ZZ_ABOUT,
                platform(),
                about_copy_button("settings-about-copy-build-info")
                    .on(settings_card_ground(cx))
                    .on_click(|_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(build_info(&platform())));
                        toast::push(Notification::success("Copied build information"), cx);
                    }),
                cx,
            ))
            .child(about_project_stack(&ZZ_ABOUT, cx))
            .into_any_element()
    }

    fn config_file_section(&self, kind: ConfigFileKind, cx: &mut Context<Self>) -> AnyElement {
        let error = self.config_file_editor(kind).error.clone();

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
                            SettingsStack::titled("Navigation").child(
                                Self::numeric_setting("Which-key delay", &self.which_key_delay, cx)
                                    .description("Milliseconds after the prefix, 0 turns it off"),
                            ),
                        )
                        .child(self.mux_splits_section(cx))
                    })
                    .child(self.file_options_section(kind, cx))
                    .child(self.file_import_section(kind, cx))
                    .when_some(error, |page, error| {
                        page.child(
                            Label::new(TextStyle::Caption, error)
                                .flex_none()
                                .text_color(cx.theme().warning),
                        )
                    }),
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

    fn reload_config_editor(&mut self, kind: ConfigFileKind, cx: &mut Context<Self>) {
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
                let file = self.config_file_editor_mut(kind);
                file.path = Some(path);
                file.saved = kind.editor_view(source);
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
        self.synchronize_terminal_editor(cx);
        self.synchronize_mux_splits(window, cx);
        self.synchronize_file_options(window, cx);
        self.synchronize_terminal_preview(cx);

        let content = match self.section {
            SettingsSection::Appearance => self.appearance_section(&resolved, cx),
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
                !penna::draws_window_controls(window)
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

fn qr_code(text: &str) -> Option<AnyElement> {
    const MODULE: f32 = 4.0;
    const QUIET: i32 = 4;
    let code = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).ok()?;
    let width = code.size();
    let unit = |count: i32| px(MODULE * f32::from(i16::try_from(count).unwrap_or(0)));
    let dark = (0..width)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| code.get_module(x, y))
        .collect::<Vec<_>>();
    let side = unit(width + 2 * QUIET);
    Some(
        ala::canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                window.paint_quad(ala::fill(bounds, ala::white()));
                for &(x, y) in &dark {
                    let origin = bounds.origin + ala::point(unit(x + QUIET), unit(y + QUIET));
                    window.paint_quad(ala::fill(
                        ala::Bounds::new(origin, ala::size(unit(1), unit(1))),
                        ala::black(),
                    ));
                }
            },
        )
        .w(side)
        .h(side)
        .flex_none()
        .into_any_element(),
    )
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

fn config_file_editor(kind: ConfigFileKind) -> ConfigFileEditor {
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
    ConfigFileEditor { path, saved, error }
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

fn key_field_state(
    spelling: &str,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> Entity<KeyFieldState> {
    let keystroke = zz_ui::command::tmux_keystroke(spelling);
    cx.new(|cx| KeyFieldState::new(window, cx).value(keystroke))
}

fn key_field_control(field: &Entity<KeyFieldState>, disabled: bool, cx: &App) -> ala::Div {
    div().w(px(KEY_INPUT_WIDTH)).flex_none().child(
        KeyField::new(field)
            .disabled(disabled)
            .on(settings_card_ground(cx)),
    )
}

fn reject_recorded_key(cx: &mut App) {
    toast::push(
        Notification::info(if cfg!(target_os = "macos") {
            "Multiplexer keys can't use Command or Fn"
        } else {
            "Multiplexer keys can't use Super or Fn"
        }),
        cx,
    );
}

fn recorded_tmux_key(keystroke: &ala::Keystroke) -> Option<String> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.function {
        return None;
    }
    let commanded = modifiers.control || modifiers.alt;
    let (base, shift) = match keystroke.key.as_str() {
        "space" => ("Space".to_owned(), modifiers.shift),
        key => match zz_config::keymap::tmux_key_name(key) {
            Some(name) => (name.to_owned(), modifiers.shift),
            None => match keystroke.key_char.as_deref() {
                Some(text) if !commanded && text.chars().count() == 1 => (text.to_owned(), false),
                _ => (key.to_owned(), modifiers.shift),
            },
        },
    };
    let key = zz_client::ChromeKey {
        command: false,
        control: modifiers.control,
        alt: modifiers.alt,
        shift,
        base,
    }
    .normalized()
    .to_string();
    zz_mux::parse_tmux_key(&key).map(|_| key)
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

pub(crate) fn written(key: ConfigKey, result: std::io::Result<()>, cx: &mut App) {
    match result {
        Ok(()) => refresh_settings_preview(key, cx),
        Err(error) => report_write_error("set", key.as_str(), &error, cx),
    }
}

fn appearance_changed(change: AppearanceChange, cx: &mut App) {
    let (key, result) = match change {
        AppearanceChange::Mode(mode) => {
            let mode = interface_theme::theme_mode_setting(mode);
            (
                ConfigKey::ThemeMode,
                set_config_key(ConfigKey::ThemeMode, mode.as_str()),
            )
        }
        AppearanceChange::Controls(controls) => {
            let key = ConfigKey::ControlShape;
            let shape = interface_theme::control_shape_setting(controls);
            (key, set_config_key(key, shape.as_str()))
        }
        AppearanceChange::Glass(glass) => {
            let key = ConfigKey::InterfaceGlass;
            (
                key,
                set_config_key(key, &interface_theme::spelled_glass(&glass)),
            )
        }
        AppearanceChange::Import(theme) => {
            let mut imported = Appearance::default();
            imported.import(&theme);
            (
                ConfigKey::ChromeAccent,
                config::set_config_keys(&config::look_edits(
                    &imported.overrides,
                    theme.face.as_deref(),
                )),
            )
        }
        AppearanceChange::ResetLook => (
            ConfigKey::ChromeAccent,
            config::set_config_keys(&config::look_edits(
                &interface_theme::ThemeOverrides::default(),
                None,
            )),
        ),
        AppearanceChange::Accent(accent) => {
            let key = ConfigKey::ChromeAccent;
            let result = match accent {
                Some(accent) => set_config_key(key, &accent),
                None => remove_config_key(key),
            };
            (key, result)
        }
        AppearanceChange::TintHue(hue) => {
            let key = ConfigKey::ChromeTintHue;
            (key, set_config_key(key, &format!("{hue:.0}")))
        }
        AppearanceChange::Tint(tint) => {
            let key = ConfigKey::ChromeTint;
            (key, set_config_key(key, &format!("{tint:.2}")))
        }
        AppearanceChange::Radius(radius) => {
            let key = ConfigKey::WidgetCornerRadius;
            (key, set_config_key(key, &format!("{radius:.2}")))
        }
        AppearanceChange::Contrast(contrast) => {
            let key = ConfigKey::ChromeContrast;
            (key, set_config_key(key, &format!("{contrast:.2}")))
        }
        AppearanceChange::ReduceTransparency(reduce) => {
            let key = ConfigKey::ReduceTransparency;
            (
                key,
                set_config_key(key, if reduce { "true" } else { "false" }),
            )
        }
        AppearanceChange::UiFont(family) => (
            ConfigKey::UiFontFamily,
            config::set_ui_font_family((!family.is_empty()).then_some(family.as_str())),
        ),
        AppearanceChange::Zoom(zoom) => {
            crate::ui_scale::set_percent(zoom * 100.0, cx);
            return;
        }
        AppearanceChange::Motion(motion) => {
            let key = ConfigKey::Motion;
            (
                key,
                set_config_key(key, interface_theme::motion_setting(motion).as_str()),
            )
        }
        AppearanceChange::Density(density) => {
            let key = ConfigKey::ChromeDensity;
            (
                key,
                set_config_key(key, interface_theme::density_setting(density).as_str()),
            )
        }
    };
    written(key, result, cx);
}

fn shown_ui_font(family: &ConfigValue<Option<String>>) -> String {
    Appearance {
        ui_font_family: family.value.clone().unwrap_or_default(),
        ..Appearance::default()
    }
    .ui_font()
    .to_owned()
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
                | ConfigKey::ReduceTransparency
                | ConfigKey::ControlShape
                | ConfigKey::InterfaceGlass
                | ConfigKey::ChromeAccent
                | ConfigKey::ChromeTint
                | ConfigKey::ChromeTintHue
                | ConfigKey::ChromeDensity
                | ConfigKey::PaletteWindowLayout
                | ConfigKey::PaletteShowKeys
                | ConfigKey::PaletteVimMode
                | ConfigKey::WhichKeyDelay
                | ConfigKey::ProgramColors
                | ConfigKey::ChromeTerminalColors
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
    if matches!(key, ConfigKey::PaneGlowStrength) {
        100.0
    } else {
        1.0
    }
}

fn numeric_input_text(key: ConfigKey, value: f32) -> String {
    if matches!(key, ConfigKey::PaneGlowStrength) {
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
        ConfigKey::PaneGlowStrength => config.pane_glow_strength,
        ConfigKey::PaneInactiveOpacity => config.pane_inactive_opacity,
        ConfigKey::PaneMargin => config.pane_margin,
        ConfigKey::WidgetCornerRadius => config.widget_corner_radius,
        ConfigKey::ChromeContrast => config.chrome_contrast,
        ConfigKey::ChromeTint => config.chrome_tint,
        ConfigKey::ChromeTintHue => config.chrome_tint_hue,
        ConfigKey::WindowCornerRadius => config.window_corner_radius,
        ConfigKey::EditorFontSize => config.editor_font_size,
        ConfigKey::WhichKeyDelay => config.which_key_delay,
        ConfigKey::UseSystemTitlebar
        | ConfigKey::PaletteWindowLayout
        | ConfigKey::PaletteShowKeys
        | ConfigKey::PaletteVimMode
        | ConfigKey::WindowBackgroundBlur
        | ConfigKey::Motion
        | ConfigKey::Tray
        | ConfigKey::Notifications
        | ConfigKey::QuitDaemonOnExit
        | ConfigKey::AutoRestartStaleDaemon
        | ConfigKey::CheckForUpdates
        | ConfigKey::FlatSidebar
        | ConfigKey::ProgramColors
        | ConfigKey::ChromeTerminalColors
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
        | ConfigKey::ReduceTransparency
        | ConfigKey::ThemeMode
        | ConfigKey::ControlShape
        | ConfigKey::InterfaceGlass
        | ConfigKey::UiFontFamily
        | ConfigKey::AppIcon
        | ConfigKey::ChromeAccent
        | ConfigKey::ChromeDensity => {
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
    fn recorded_keys_save_as_tmux_spellings() {
        let recorded = |source: &str, key_char: Option<&str>| {
            let mut keystroke = ala::Keystroke::parse(source).unwrap();
            keystroke.key_char = key_char.map(str::to_owned);
            recorded_tmux_key(&keystroke)
        };
        assert_eq!(recorded("ctrl-b", None).as_deref(), Some("C-b"));
        assert_eq!(recorded("\"", Some("\"")).as_deref(), Some("\""));
        assert_eq!(recorded("shift-l", Some("L")).as_deref(), Some("L"));
        assert_eq!(recorded("alt-a", Some("å")).as_deref(), Some("M-a"));
        assert_eq!(recorded("ctrl-space", None).as_deref(), Some("C-Space"));
        assert_eq!(recorded("alt-up", None).as_deref(), Some("M-Up"));
        assert_eq!(recorded("f5", None).as_deref(), Some("F5"));
        assert_eq!(recorded("cmd-b", None), None);
    }

    #[test]
    fn percentage_inputs_convert_and_reject_invalid_edits() {
        for (key, default, max) in [(ConfigKey::PaneGlowStrength, "100", 200.0)] {
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
