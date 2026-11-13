use super::*;
use penna::components::menu::MenuScope;
use std::{collections::HashSet, path::Path};
use zz_protocol::{CommandInvocation, MuxOptionKey, MuxOptions};
use zz_terminal::{
    AppearanceConfigKey, GhosttyTheme, TerminalAppearance, TerminalColorScheme, ThemeChoice,
};

const BUILT_IN_THEME: &str = "Built-in";
const CONTRAST_ON: &str = "4.5";

#[derive(Clone, Copy)]
enum FileKey {
    Mux(MuxOptionKey),
    Appearance(AppearanceConfigKey),
    ThemeSlot(TerminalColorScheme),
    Contrast,
}

impl FileKey {
    fn name(self) -> &'static str {
        match self {
            Self::Mux(key) => key.as_str(),
            Self::Appearance(key) => key.as_str(),
            Self::ThemeSlot(TerminalColorScheme::Light) => "light-theme",
            Self::ThemeSlot(TerminalColorScheme::Dark) => "dark-theme",
            Self::Contrast => AppearanceConfigKey::MinimumContrast.as_str(),
        }
    }

    fn value(self, source: &str) -> Option<String> {
        match self {
            Self::Mux(key) => config::file_options::mux_option_value(source, key),
            Self::Appearance(key) => config::file_options::appearance_value(source, key),
            Self::ThemeSlot(scheme) => {
                let (light, dark) = theme_slots(source);
                match scheme {
                    TerminalColorScheme::Light => light,
                    TerminalColorScheme::Dark => dark,
                }
            }
            Self::Contrast => {
                config::file_options::appearance_value(source, AppearanceConfigKey::MinimumContrast)
            }
        }
    }

    const fn on_interface(self) -> bool {
        matches!(
            self,
            Self::ThemeSlot(_)
                | Self::Contrast
                | Self::Appearance(AppearanceConfigKey::BackgroundOpacity)
        )
    }

    fn default_value(self) -> String {
        match self {
            Self::Mux(key) => MuxOptions::default().get(key).unwrap().value.clone(),
            Self::Appearance(AppearanceConfigKey::Theme) | Self::ThemeSlot(_) => String::new(),
            Self::Contrast => TerminalAppearance::default().minimum_contrast.to_string(),
            Self::Appearance(AppearanceConfigKey::WindowPaddingX) => {
                TerminalAppearance::default().padding_left.to_string()
            }
            Self::Appearance(AppearanceConfigKey::WindowPaddingY) => {
                TerminalAppearance::default().padding_top.to_string()
            }
            Self::Appearance(key) => {
                config::appearance_config_values(&TerminalAppearance::default(), key)
                    .ok()
                    .and_then(|values| values.into_iter().rev().find(|value| !value.is_empty()))
                    .unwrap_or_default()
            }
        }
    }
}

struct OptionRow {
    key: FileKey,
    title: &'static str,
    value: Option<String>,
    input: Entity<InputState>,
    select: Option<Entity<SelectState<Vec<SettingsSelectItem>>>>,
    key_field: Option<Entity<KeyFieldState>>,
    scopes: Vec<MenuScope>,
    numeric: bool,
}

pub(super) struct FileOptions {
    rows: Vec<OptionRow>,
    source: String,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub(super) fn synchronize_file_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kind = match self.section {
            SettingsSection::Multiplexer => ConfigFileKind::Mux,
            SettingsSection::Terminal | SettingsSection::Appearance => ConfigFileKind::Terminal,
            _ => return,
        };
        let source = self.config_file_editor(kind).saved.clone();
        if let std::collections::btree_map::Entry::Vacant(vacant) = self.file_options.entry(kind) {
            let mut definitions = match kind {
                ConfigFileKind::Mux => vec![
                    (FileKey::Mux(MuxOptionKey::Prefix), "Prefix", false),
                    (FileKey::Mux(MuxOptionKey::ModeKeys), "Mode keys", false),
                    (FileKey::Mux(MuxOptionKey::Mouse), "Mouse", false),
                    (
                        FileKey::Mux(MuxOptionKey::HistoryLimit),
                        "History limit",
                        true,
                    ),
                    (FileKey::Mux(MuxOptionKey::SetClipboard), "Clipboard", false),
                    (
                        FileKey::Mux(MuxOptionKey::EscapeTime),
                        "Escape time (ms)",
                        true,
                    ),
                ],
                ConfigFileKind::Terminal => vec![
                    (
                        FileKey::Appearance(AppearanceConfigKey::FontFamily),
                        "Font family",
                        false,
                    ),
                    (
                        FileKey::Appearance(AppearanceConfigKey::FontSize),
                        "Font size",
                        true,
                    ),
                    (
                        FileKey::ThemeSlot(TerminalColorScheme::Light),
                        "Light theme",
                        false,
                    ),
                    (
                        FileKey::ThemeSlot(TerminalColorScheme::Dark),
                        "Dark theme",
                        false,
                    ),
                    (FileKey::Contrast, "Terminal text contrast", false),
                    (
                        FileKey::Appearance(AppearanceConfigKey::CursorStyle),
                        "Cursor style",
                        false,
                    ),
                    (
                        FileKey::Appearance(AppearanceConfigKey::CursorStyleBlink),
                        "Cursor blink",
                        false,
                    ),
                    (
                        FileKey::Appearance(AppearanceConfigKey::BackgroundOpacity),
                        "Background opacity (%)",
                        true,
                    ),
                    (
                        FileKey::Appearance(AppearanceConfigKey::WindowPaddingX),
                        "Padding X",
                        true,
                    ),
                    (
                        FileKey::Appearance(AppearanceConfigKey::WindowPaddingY),
                        "Padding Y",
                        true,
                    ),
                ],
            };
            if cfg!(target_os = "macos") && kind == ConfigFileKind::Terminal {
                definitions.insert(
                    2,
                    (
                        FileKey::Appearance(AppearanceConfigKey::FontThicken),
                        "Font smoothing",
                        false,
                    ),
                );
            }
            let themes = if kind == ConfigFileKind::Terminal {
                zz_terminal::enumerate_ghostty_themes_for(
                    import_color_scheme(cx),
                    config::import_target_path().ok().as_deref(),
                )
            } else {
                Vec::new()
            };
            let mut subscriptions = Vec::new();
            let mut rows = Vec::new();
            for (index, (key, title, numeric)) in definitions.into_iter().enumerate() {
                let value = key.value(&source);
                let displayed =
                    display_value(key, value.as_deref().unwrap_or(&key.default_value()));
                let input = cx.new(|cx| {
                    let input = InputState::new(window, cx).default_value(displayed);
                    let input = if numeric {
                        input.step(if key.name() == "background-opacity" {
                            5.0
                        } else {
                            1.0
                        })
                    } else {
                        input
                    };
                    if key.name() == "background-opacity" {
                        input.min(0.0).max(100.0)
                    } else if matches!(key.name(), "window-padding-x" | "window-padding-y") {
                        input.min(0.0)
                    } else {
                        input
                    }
                });
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    move |this, input, event, window, cx| {
                        if matches!(event, InputEvent::Change)
                            && this.section == SettingsSection::Terminal
                        {
                            cx.notify();
                        }
                        if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                            let mut value = input.read(cx).value().to_string();
                            if key.name() == "background-opacity" {
                                let Ok(percent) = value.parse::<f32>() else {
                                    return;
                                };
                                if !(0.0..=100.0).contains(&percent) {
                                    return;
                                }
                                value = (percent / 100.0).to_string();
                            }
                            this.commit_file_option(kind, index, Some(&value), window, cx);
                        }
                    },
                ));
                let selected = select_value(key, value.as_deref().unwrap_or(&key.default_value()));
                let choices = option_choices(key);
                let mut scopes = Vec::new();
                let select = if matches!(key, FileKey::Appearance(AppearanceConfigKey::FontFamily))
                {
                    let (select, font_scopes) = terminal_font_select(
                        &display_value(key, value.as_deref().unwrap_or_default()),
                        window,
                        cx,
                    );
                    scopes = font_scopes;
                    Some(select)
                } else if let FileKey::ThemeSlot(scheme) = key {
                    let (select, theme_scopes) = theme_select(
                        scheme,
                        value.as_deref().unwrap_or_default(),
                        &themes,
                        window,
                        cx,
                    );
                    scopes = theme_scopes;
                    Some(select)
                } else if choices.is_empty() {
                    None
                } else {
                    let refs = choices
                        .iter()
                        .map(|(label, value)| (label.as_str(), value.as_str()))
                        .collect::<Vec<_>>();
                    Some(settings_select_state(&refs, &selected, window, cx))
                };
                if let Some(select) = &select {
                    subscriptions.push(cx.subscribe_in(
                        select,
                        window,
                        move |this, _, event, window, cx| {
                            if let SelectEvent::Confirm(Some(value)) = event {
                                this.commit_file_option(
                                    kind,
                                    index,
                                    (!value.is_empty()).then_some(value.as_str()),
                                    window,
                                    cx,
                                );
                            }
                        },
                    ));
                }
                let key_field = matches!(key, FileKey::Mux(MuxOptionKey::Prefix)).then(|| {
                    let spelling = value.clone().unwrap_or_else(|| key.default_value());
                    let field = key_field_state(&spelling, window, cx);
                    subscriptions.push(cx.subscribe_in(
                        &field,
                        window,
                        move |this, field, event: &KeyFieldEvent, window, cx| {
                            let KeyFieldEvent::Record(keystroke) = event;
                            match recorded_tmux_key(keystroke) {
                                Some(spelling) => {
                                    this.commit_file_option(
                                        kind,
                                        index,
                                        Some(&spelling),
                                        window,
                                        cx,
                                    );
                                }
                                None => reject_recorded_key(cx),
                            }
                            let row = &this.file_options[&kind].rows[index];
                            let saved =
                                row.value.clone().unwrap_or_else(|| row.key.default_value());
                            field.update(cx, |field, cx| {
                                field.set_value(zz_ui::command::tmux_keystroke(&saved), cx);
                            });
                        },
                    ));
                    field
                });
                rows.push(OptionRow {
                    key,
                    title,
                    value,
                    input,
                    select,
                    key_field,
                    scopes,
                    numeric,
                });
            }
            vacant.insert(FileOptions {
                rows,
                source: source.clone(),
                _subscriptions: subscriptions,
            });
        }
        let controls = self.file_options.get_mut(&kind).unwrap();
        if controls.source != source {
            controls.source.clone_from(&source);
            for row in &mut controls.rows {
                row.value = row.key.value(&source);
                let value = row.value.clone().unwrap_or_else(|| row.key.default_value());
                row.input.update(cx, |input, cx| {
                    input.set_value(display_value(row.key, &value), window, cx);
                });
                if let Some(field) = &row.key_field {
                    field.update(cx, |field, cx| {
                        field.set_value(zz_ui::command::tmux_keystroke(&value), cx);
                    });
                }
                if let Some(select) = &row.select {
                    let selected = if row.key.name() == "font-family" {
                        display_value(row.key, row.value.as_deref().unwrap_or_default())
                    } else {
                        select_value(row.key, &value)
                    };
                    select.update(cx, |select, cx| {
                        select.set_selected_value(&selected, window, cx);
                    });
                }
            }
        }
    }

    fn file_controls_disabled(&self, kind: ConfigFileKind, cx: &App) -> bool {
        let file = self.config_file_editor(kind);
        file.error.is_some()
            || self.pending_file_command.is_some()
            || (kind == ConfigFileKind::Mux && !self.mux.read(cx).local_is_connected())
    }

    pub(super) fn terminal_preview_source(&self, cx: &App) -> String {
        let mut source = self
            .config_file_editor(ConfigFileKind::Terminal)
            .saved
            .clone();
        let Some(controls) = self.file_options.get(&ConfigFileKind::Terminal) else {
            return source;
        };
        for row in &controls.rows {
            let FileKey::Appearance(key) = row.key else {
                continue;
            };
            if row.select.is_some() {
                continue;
            }
            let value = row.input.read(cx).value();
            let saved = display_value(
                row.key,
                row.value.as_deref().unwrap_or(&row.key.default_value()),
            );
            if value.as_ref() == saved {
                continue;
            }
            let value = if key == AppearanceConfigKey::BackgroundOpacity {
                let Ok(percent) = value.parse::<f32>() else {
                    continue;
                };
                if !(0.0..=100.0).contains(&percent) {
                    continue;
                }
                (percent / 100.0).to_string()
            } else {
                value.to_string()
            };
            if let Ok(edited) =
                config::file_options::edit_appearance_option(&source, key, Some(&value))
            {
                source = edited;
            }
        }
        source
    }

    pub(super) fn file_options_section(
        &self,
        kind: ConfigFileKind,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(controls) = self.file_options.get(&kind) else {
            return div().into_any_element();
        };
        let mut stack = SettingsStack::titled(if kind == ConfigFileKind::Mux {
            "Options"
        } else {
            "Display"
        });
        for (index, row) in controls.rows.iter().enumerate() {
            if kind == ConfigFileKind::Terminal && row.key.on_interface() {
                continue;
            }
            stack = stack.child(self.file_option_entry(kind, index, cx));
        }
        div().flex_none().child(stack).into_any_element()
    }

    pub(super) fn terminal_look_entry(
        &self,
        row: ZzRow,
        cx: &Context<Self>,
    ) -> Option<SettingEntry> {
        let key = match row {
            ZzRow::LightTheme => FileKey::ThemeSlot(TerminalColorScheme::Light),
            ZzRow::DarkTheme => FileKey::ThemeSlot(TerminalColorScheme::Dark),
            ZzRow::BackgroundOpacity => FileKey::Appearance(AppearanceConfigKey::BackgroundOpacity),
            ZzRow::TerminalContrast => FileKey::Contrast,
            _ => return None,
        };
        let index = self
            .file_options
            .get(&ConfigFileKind::Terminal)?
            .rows
            .iter()
            .position(|option| option.key.name() == key.name())?;
        Some(self.file_option_entry(ConfigFileKind::Terminal, index, cx))
    }

    fn file_option_entry(
        &self,
        kind: ConfigFileKind,
        index: usize,
        cx: &Context<Self>,
    ) -> SettingEntry {
        let controls = &self.file_options[&kind];
        let row = &controls.rows[index];
        let disabled = self.file_controls_disabled(kind, cx);
        let description = match row.key.name() {
            "light-theme" | "dark-theme" => Some("Replaces custom terminal colors"),
            "minimum-contrast" => Some("Lifts terminal text under 4.5:1 against its background"),
            "prefix" => Some("The key you press before each shortcut"),
            _ => None,
        };
        let entry = SettingEntry::new(row.title)
            .when_some(description, SettingEntry::description)
            .disabled(disabled);
        let control = if matches!(row.key.name(), "mouse" | "font-thicken") {
            let default = row.key.default_value();
            let enabled = matches!(
                row.value.as_deref().unwrap_or(&default),
                "on" | "true" | "yes" | "1"
            );
            let is_appearance = matches!(row.key, FileKey::Appearance(_));
            boolean_control(
                row.key.name(),
                enabled,
                cx.listener(move |this, enabled: &bool, window, cx| {
                    this.commit_file_option(
                        kind,
                        index,
                        Some(match (*enabled, is_appearance) {
                            (true, true) => "true",
                            (false, true) => "false",
                            (true, false) => "on",
                            (false, false) => "off",
                        }),
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
        } else if matches!(row.key, FileKey::Contrast) {
            let enabled = row
                .value
                .as_deref()
                .unwrap_or(&row.key.default_value())
                .parse::<f32>()
                .is_ok_and(|minimum| minimum > 1.0);
            boolean_control(
                row.key.name(),
                enabled,
                cx.listener(move |this, enabled: &bool, window, cx| {
                    this.commit_file_option(
                        kind,
                        index,
                        Some(if *enabled { CONTRAST_ON } else { "1" }),
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
        } else if let Some(select) = &row.select {
            if !row.scopes.is_empty() || row.key.name() == "font-family" {
                div()
                    .flex_none()
                    .child(
                        Select::new(select)
                            .search_placeholder(if row.key.name() == "font-family" {
                                "Search fonts"
                            } else {
                                "Search themes"
                            })
                            .scopes(row.scopes.clone())
                            .on(settings_card_ground(cx)),
                    )
                    .into_any_element()
            } else {
                select_control(select, cx).into_any_element()
            }
        } else if let Some(field) = &row.key_field {
            key_field_control(field, disabled, cx).into_any_element()
        } else if row.numeric {
            numeric_control(&row.input, cx).into_any_element()
        } else {
            div()
                .w(px(200.0))
                .child(Input::new(&row.input).on(settings_card_ground(cx)))
                .into_any_element()
        };
        entry.control(control)
    }

    pub(super) fn file_import_section(
        &self,
        kind: ConfigFileKind,
        cx: &Context<Self>,
    ) -> AnyElement {
        if !crate::profile::profile(cx).has_config_import {
            return div().into_any_element();
        }
        let (title, description) = match kind {
            ConfigFileKind::Mux => ("Import from tmux", "Copies a tmux config into zz/mux.conf"),
            ConfigFileKind::Terminal => (
                "Import from Ghostty",
                "Copies a Ghostty config into zz/config",
            ),
        };
        SettingsStack::titled("Import")
            .child(
                SettingEntry::new(title).description(description).control(
                    h_flex()
                        .gap_2()
                        .flex_none()
                        .child(
                            Button::new("open-config-file")
                                .label("Open")
                                .on(settings_card_ground(cx))
                                .tooltip(format!("Open {} in your text editor", kind.file_name()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_config_file(kind, cx);
                                })),
                        )
                        .child(
                            Button::new("import-config-file")
                                .label("Import…")
                                .on(settings_card_ground(cx))
                                .disabled(self.file_controls_disabled(kind, cx))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose_file_import(kind, window, cx);
                                })),
                        ),
                ),
            )
            .into_any_element()
    }

    fn commit_file_option(
        &mut self,
        kind: ConfigFileKind,
        index: usize,
        value: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_controls_disabled(kind, cx) {
            return;
        }
        let key = self.file_options[&kind].rows[index].key;
        let saved = self.file_options[&kind].rows[index].value.as_deref();
        if (saved == value
            || value.is_some_and(|value| {
                display_value(key, value)
                    == display_value(key, saved.unwrap_or(&key.default_value()))
            }))
            && !(matches!(
                key,
                FileKey::Appearance(AppearanceConfigKey::Theme) | FileKey::ThemeSlot(_)
            ) && value.is_some())
        {
            return;
        }
        let result = (|| -> io::Result<()> {
            let path = self
                .config_file_editor(kind)
                .path
                .clone()
                .map_or_else(|| config_file_path(kind), Ok)?;
            let source = config::read_config_editor_source(&path, kind.max_bytes())?;
            if kind.editor_view(source.clone()) != self.config_file_editor(kind).saved {
                return Err(io::Error::other(
                    "configuration changed on disk; reload before editing",
                ));
            }
            match key {
                FileKey::Mux(key) => {
                    let edited = config::file_options::edit_mux_option(&source, key, value)?;
                    config::write_config_editor_source(&path, &edited, kind.max_bytes())?;
                }
                FileKey::Appearance(key) => {
                    config::file_options::write_appearance_option(&path, key, value)?;
                }
                FileKey::ThemeSlot(scheme) => {
                    let (light, dark) = theme_slots(&source);
                    let picked = value.map(str::to_owned);
                    let (light, dark) = match scheme {
                        TerminalColorScheme::Light => (picked, dark),
                        TerminalColorScheme::Dark => (light, picked),
                    };
                    config::file_options::write_appearance_option(
                        &path,
                        AppearanceConfigKey::Theme,
                        ThemeChoice::spelled(light.as_deref(), dark.as_deref()).as_deref(),
                    )?;
                }
                FileKey::Contrast => {
                    config::file_options::write_appearance_option(
                        &path,
                        AppearanceConfigKey::MinimumContrast,
                        value,
                    )?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            report_write_error("set", key.name(), &error, cx);
            return;
        }
        self.reload_config_editor(kind, cx);
        if let FileKey::Mux(option) = key {
            let expected = key
                .value(&self.config_file_editor(kind).saved)
                .unwrap_or_else(|| key.default_value());
            self.run_file_command(
                kind,
                CommandInvocation::new("reload-config", [] as [&str; 0]),
                Some((option, expected)),
                window,
                cx,
            );
        } else {
            config::reload(cx);
        }
    }

    fn open_config_file(&mut self, kind: ConfigFileKind, cx: &mut Context<Self>) {
        let opened = self
            .config_file_editor(kind)
            .path
            .clone()
            .map_or_else(|| config_file_path(kind), Ok)
            .and_then(|path| {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)?;
                Ok(path)
            });
        match opened {
            Ok(path) => open_in_text_editor(&path, cx),
            Err(error) => report_write_error("open", kind.file_name(), &error, cx),
        }
    }

    pub(super) fn pick_up_outside_edits(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        for kind in [ConfigFileKind::Mux, ConfigFileKind::Terminal] {
            let editor = match kind {
                ConfigFileKind::Mux => self.mux_config_editor.as_ref(),
                ConfigFileKind::Terminal => self.terminal_config_editor.as_ref(),
            };
            let Some(path) = editor.and_then(|editor| editor.path.clone()) else {
                continue;
            };
            let Ok(source) = config::read_config_editor_source(&path, kind.max_bytes()) else {
                continue;
            };
            if kind.editor_view(source) != self.config_file_editor(kind).saved {
                self.reload_config_editor(kind, cx);
                changed = true;
            }
        }
        if changed && self.pending_file_command.is_none() {
            self.reload_mux_configuration(cx);
        }
    }

    fn choose_file_import(
        &mut self,
        kind: ConfigFileKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_controls_disabled(kind, cx) {
            return;
        }
        let selected = cx.prompt_for_paths(ala::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, window| {
            let path = selected.await.ok()?.ok()??.into_iter().next()?;
            window
                .update(|window, cx| {
                    this.update(cx, |this, cx| this.import_file(kind, &path, window, cx))
                })
                .ok()?
                .ok()
        })
        .detach();
    }

    fn import_file(
        &mut self,
        kind: ConfigFileKind,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_controls_disabled(kind, cx) {
            return;
        }
        if kind == ConfigFileKind::Mux {
            self.run_file_command(
                kind,
                CommandInvocation::new(
                    "import-tmux-config",
                    ["--".to_owned(), path.display().to_string()],
                ),
                None,
                window,
                cx,
            );
        } else {
            match config::import::import_ghostty_config_from(path, import_color_scheme(cx)) {
                Ok(_) => {
                    self.reload_config_editor(kind, cx);
                    config::reload(cx);
                    toast::push(
                        Notification::success(format!("Imported {}", path.display())),
                        cx,
                    );
                }
                Err(error) => report_write_error("import", kind.file_name(), &error, cx),
            }
        }
    }

    fn run_file_command(
        &mut self,
        kind: ConfigFileKind,
        command: CommandInvocation,
        expected: Option<(MuxOptionKey, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_pending_mux_split();
        self.file_command_sequence += 1;
        let sequence = self.file_command_sequence;
        self.pending_file_command = Some((sequence, kind));
        let server_id = self.mux.read(cx).local_server_id();
        let job = cx
            .background_executor()
            .spawn(async move { execute_file_command(command, expected, server_id) });
        cx.spawn_in(window, async move |this, window| {
            let result = job.await;
            let _ = window.update(|_, cx| {
                let _ = this.update(cx, |this, cx| {
                    if this.pending_file_command != Some((sequence, kind)) {
                        return;
                    }
                    this.pending_file_command = None;
                    this.reload_config_editor(kind, cx);
                    match result {
                        Ok(output) if !output.trim().is_empty() => {
                            toast::push(Notification::success(output), cx);
                        }
                        Ok(_) => {}
                        Err(error) => toast::push(Notification::error(error), cx),
                    }
                    cx.notify();
                });
            });
        })
        .detach();
        cx.spawn_in(window, async move |this, window| {
            window.background_executor().timer(std::time::Duration::from_secs(5)).await;
            let _ = window.update(|_, cx| { let _ = this.update(cx, |this, cx| {
                if this.pending_file_command == Some((sequence, kind)) {
                    this.pending_file_command = None;
                    this.reload_config_editor(kind, cx);
                    toast::push(Notification::info("The daemon has not confirmed the configuration. Open the file to review it."), cx);
                    cx.notify();
                }
            }); });
        }).detach();
        cx.notify();
    }
}

fn execute_file_command(
    command: CommandInvocation,
    expected: Option<(MuxOptionKey, String)>,
    server_id: Option<u64>,
) -> Result<String, String> {
    const DAEMON_CHANGED: &str = "The local daemon changed. Reconnect before editing.";
    let server_id = server_id.ok_or_else(|| DAEMON_CHANGED.to_owned())?;
    let mut client = config::local_command_client()?;
    let output = client
        .execute_on_server(server_id, command)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| DAEMON_CHANGED.to_owned())?;
    if let Some((key, expected)) = expected {
        let actual = client
            .execute(CommandInvocation::new(
                "show-options",
                ["-gqv", key.as_str()],
            ))
            .map_err(|error| error.to_string())?;
        if !mux_confirmation_matches(key, &expected, actual.trim()) {
            return Err(format!(
                "Saved {} = {}, but the daemon reports {}. Open the file to review it.",
                key.as_str(),
                expected,
                actual.trim()
            ));
        }
    }
    Ok(output)
}

fn terminal_font_select(
    selected: &str,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> (Entity<SelectState<Vec<SettingsSelectItem>>>, Vec<MenuScope>) {
    let scopes = penna::blocks::appearance::monospaced_scopes("Default", cx);
    let mut families = cx.text_system().all_font_names();
    families.retain(|family| !family.starts_with('.') && !family.trim().is_empty());
    if !selected.is_empty() {
        families.push(selected.to_owned());
    }
    families.sort_by_cached_key(|family| family.to_lowercase());
    families.dedup();
    let index = (!selected.is_empty())
        .then(|| families.iter().position(|family| family == selected))
        .flatten()
        .map_or(0, |index| index + 1);
    let items = std::iter::once(SettingsSelectItem::new("Default", ""))
        .chain(
            families
                .into_iter()
                .map(|family| SettingsSelectItem::new(family.clone(), family).preview_font()),
        )
        .collect();
    let select = cx.new(|cx| SelectState::new(items, Some(IndexPath::new(index)), window, cx));
    (select, scopes)
}

fn open_in_text_editor(path: &Path, cx: &App) {
    #[cfg(target_os = "macos")]
    {
        use zz_daemon_client::unmasked::SpawnUnmasked as _;
        if let Ok(mut child) = std::process::Command::new("open")
            .args(["-t", "--"])
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn_unmasked()
        {
            cx.background_executor()
                .spawn(async move { child.wait() })
                .detach();
            return;
        }
    }
    cx.open_with_system(path);
}

fn mux_confirmation_matches(key: MuxOptionKey, expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    match key {
        MuxOptionKey::HistoryLimit | MuxOptionKey::EscapeTime => expected
            .parse::<u64>()
            .ok()
            .zip(actual.parse::<u64>().ok())
            .is_some_and(|(expected, actual)| expected == actual),
        MuxOptionKey::Prefix => zz_mux::parse_tmux_key(expected)
            .zip(zz_mux::parse_tmux_key(actual))
            .is_some_and(|(expected, actual)| expected == actual),
        MuxOptionKey::Mouse => {
            matches!(expected, "true" | "yes" | "1") && actual == "on"
                || matches!(expected, "false" | "no" | "0") && actual == "off"
        }
        _ => false,
    }
}

fn select_value(key: FileKey, value: &str) -> String {
    if key.name() == "cursor-style-blink" {
        match value {
            "true" | "yes" | "1" => "on".to_owned(),
            "false" | "no" | "0" => "off".to_owned(),
            _ => value.to_owned(),
        }
    } else {
        value.to_owned()
    }
}

fn display_value(key: FileKey, value: &str) -> String {
    if key.name() == "font-family"
        && let Some(unquoted) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
    {
        return unquoted.to_owned();
    }
    if matches!(key.name(), "window-padding-x" | "window-padding-y")
        && let Some((first, second)) = value.split_once(',')
        && first.trim() == second.trim()
    {
        return first.trim().to_owned();
    }
    if key.name() == "background-opacity" {
        value
            .parse::<f32>()
            .map_or_else(|_| value.to_owned(), |value| (value * 100.0).to_string())
    } else {
        value.to_owned()
    }
}

fn theme_slots(source: &str) -> (Option<String>, Option<String>) {
    config::file_options::appearance_value(source, AppearanceConfigKey::Theme)
        .and_then(|value| ThemeChoice::parse(&value).ok())
        .map(|choice| choice.slots(&config::import_target_path().unwrap_or_default()))
        .unwrap_or_default()
}

fn theme_select(
    scheme: TerminalColorScheme,
    selected: &str,
    themes: &[GhosttyTheme],
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> (Entity<SelectState<Vec<SettingsSelectItem>>>, Vec<MenuScope>) {
    let mut choices = vec![(BUILT_IN_THEME, "")];
    choices.extend(
        themes
            .iter()
            .map(|theme| (theme.name.as_str(), theme.name.as_str())),
    );
    if !selected.is_empty() && !themes.iter().any(|theme| theme.name == selected) {
        choices.push((selected, selected));
    }
    let select = settings_select_state(&choices, selected, window, cx);
    let own = themes
        .iter()
        .filter(|theme| theme.appearance.background_scheme() == scheme)
        .map(|theme| theme.name.clone())
        .collect::<HashSet<_>>();
    let label = match scheme {
        TerminalColorScheme::Light => "Light",
        TerminalColorScheme::Dark => "Dark",
    };
    let scopes = vec![
        MenuScope::new(label, move |name| {
            name == BUILT_IN_THEME || own.contains(name)
        }),
        MenuScope::all("All themes"),
    ];
    (select, scopes)
}

fn option_choices(key: FileKey) -> Vec<(String, String)> {
    let choices = match key.name() {
        "mode-keys" => vec![("Vi", "vi"), ("Emacs", "emacs")],
        "set-clipboard" => vec![("On", "on"), ("External", "external"), ("Off", "off")],
        "cursor-style" => vec![
            ("Block", "block"),
            ("Bar", "bar"),
            ("Underline", "underline"),
            ("Hollow block", "block_hollow"),
        ],
        "cursor-style-blink" => vec![("On", "on"), ("Off", "off"), ("Terminal", "terminal")],
        _ => Vec::new(),
    };
    choices
        .into_iter()
        .map(|(label, value)| (label.to_owned(), value.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[ala::test]
    fn font_smoothing_control_saves_toggle_and_reset(cx: &mut ala::TestAppContext) {
        use std::{cell::RefCell, rc::Rc};

        cx.update(zz_ui::init);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config");
        let source = "# keep this\nfont-size = 13\n";
        std::fs::write(&path, source).unwrap();
        let captured = Rc::new(RefCell::new(None));
        let captured_for_window = Rc::clone(&captured);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_for_window.replace(Some(cx.entity()));
            SettingsView::new(mux, window, cx)
        });
        let settings = captured.borrow().clone().unwrap();
        cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.set_section(SettingsSection::Terminal, window, cx);
                let file = settings.config_file_editor_mut(ConfigFileKind::Terminal);
                file.path = Some(path.clone());
                file.saved = source.to_owned();
                file.error = None;
                settings.synchronize_file_options(window, cx);
                for (key, value) in [
                    (AppearanceConfigKey::FontThicken, Some("true")),
                    (AppearanceConfigKey::FontThicken, Some("false")),
                    (AppearanceConfigKey::FontThicken, None),
                ] {
                    let index = settings.file_options[&ConfigFileKind::Terminal]
                        .rows
                        .iter()
                        .position(|row| row.key.name() == key.as_str())
                        .unwrap();
                    settings.commit_file_option(ConfigFileKind::Terminal, index, value, window, cx);
                    settings.synchronize_file_options(window, cx);
                    let saved = std::fs::read_to_string(&path).unwrap();
                    assert_eq!(
                        config::file_options::appearance_value(&saved, key).as_deref(),
                        value
                    );
                    assert_eq!(settings.terminal_preview_source(cx), saved);
                }
            });
        });
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
    }

    #[ala::test]
    fn terminal_preview_uses_drafts_without_changing_saved_settings(cx: &mut ala::TestAppContext) {
        use ala::EntityInputHandler as _;
        use std::{cell::RefCell, rc::Rc};

        cx.update(zz_ui::init);
        let captured = Rc::new(RefCell::new(None));
        let captured_for_window = Rc::clone(&captured);
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let mux = cx.new(|cx| {
                MuxClient::new(
                    Err(zz_daemon_client::DaemonError::Thread(
                        "test client".to_owned(),
                    )),
                    zz_daemon_client::default_socket_path(),
                    cx,
                )
            });
            captured_for_window.replace(Some(cx.entity()));
            SettingsView::new(mux, window, cx)
        });
        let settings = captured.borrow().clone().unwrap();
        let preview = cx.update(|window, cx| {
            settings.update(cx, |settings, cx| {
                settings.set_section(SettingsSection::Terminal, window, cx);
                let saved = "font-size = 12\nbackground-opacity = 1\n";
                let file = settings.config_file_editor_mut(ConfigFileKind::Terminal);
                file.saved = saved.to_owned();
                settings.synchronize_file_options(window, cx);
                let rows = &settings.file_options[&ConfigFileKind::Terminal].rows;
                for (key, draft) in [("font-size", "18"), ("background-opacity", "65")] {
                    rows.iter()
                        .find(|row| row.key.name() == key)
                        .unwrap()
                        .input
                        .update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.replace_text_in_range(None, draft, window, cx);
                            assert_eq!(input.value().as_ref(), draft);
                        });
                }
                let source = settings.terminal_preview_source(cx);
                assert_eq!(
                    config::file_options::appearance_value(&source, AppearanceConfigKey::FontSize)
                        .as_deref(),
                    Some("18")
                );
                assert_eq!(
                    config::file_options::appearance_value(
                        &source,
                        AppearanceConfigKey::BackgroundOpacity,
                    )
                    .as_deref(),
                    Some("0.65")
                );
                assert_eq!(
                    settings.config_file_editor(ConfigFileKind::Terminal).saved,
                    saved
                );
                settings.synchronize_terminal_preview(cx);
                let preview = settings.terminal_preview.as_ref().unwrap().downgrade();
                settings.set_section(SettingsSection::Appearance, window, cx);
                assert!(settings.terminal_preview.is_some());
                settings.set_section(SettingsSection::Browser, window, cx);
                assert!(settings.terminal_preview.is_none());
                preview
            })
        });
        assert!(preview.upgrade().is_none());
    }

    #[test]
    fn mux_confirmation_accepts_daemon_normalized_values() {
        assert!(mux_confirmation_matches(
            MuxOptionKey::EscapeTime,
            "0010",
            "10"
        ));
        assert!(mux_confirmation_matches(MuxOptionKey::Mouse, "true", "on"));
        assert!(!mux_confirmation_matches(
            MuxOptionKey::HistoryLimit,
            "10",
            "20"
        ));
        assert!(!mux_confirmation_matches(
            MuxOptionKey::EscapeTime,
            "invalid",
            "10"
        ));
    }
}
