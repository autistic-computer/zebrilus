use super::*;

impl MuxEngine {
    #[must_use]
    pub const fn buffer_limit(&self) -> usize {
        self.buffer_limit
    }

    #[must_use]
    pub const fn message_limit(&self) -> usize {
        self.message_limit
    }

    #[must_use]
    pub const fn history_trickle(&self) -> usize {
        self.history_trickle
    }

    #[must_use]
    pub const fn status_formats(&self) -> &StatusFormats {
        &self.status
    }

    #[must_use]
    pub fn status_formats_for_session(&self, session: Option<SessionId>) -> StatusFormats {
        let mut formats = self.status.clone();
        if let Some(overrides) =
            session.and_then(|session| self.session_status_options.get(&session))
        {
            for (option, value) in overrides {
                formats
                    .set(*option, Some(value))
                    .expect("stored status option was validated");
            }
        }
        formats
    }

    #[must_use]
    pub fn status_rows_for_session(&self, session: Option<SessionId>) -> u8 {
        session
            .and_then(|session| self.session_status_options.get(&session))
            .and_then(|options| options.get(&StatusOption::Enabled))
            .map_or_else(
                || self.status.rows(),
                |value| match value.as_str() {
                    "off" => 0,
                    "on" => 1,
                    value => value.parse().expect("stored status rows were validated"),
                },
            )
    }

    #[must_use]
    pub fn status_format_array_for_session(
        &self,
        session: Option<SessionId>,
    ) -> BTreeMap<u32, String> {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.array_option_readback(target, "status-format", true)
            .map(|(array, _)| {
                array
                    .iter()
                    .filter_map(|(index, value)| match index {
                        ArrayIndex::Numeric(index) => Some((*index, value.clone())),
                        ArrayIndex::Named(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[must_use]
    pub fn status_row_variables_for_session(
        &self,
        _session: Option<SessionId>,
    ) -> StatusRowVariables {
        self.format_option_snapshot()
    }

    #[must_use]
    pub fn message_styles_for_session(&self, session: Option<SessionId>) -> (String, String) {
        let overrides = session.and_then(|session| self.session_options.get(&session));
        (
            overrides
                .and_then(|values| values.get(&SessionOption::MessageStyle))
                .unwrap_or(&self.global_session_options.message_style)
                .clone(),
            overrides
                .and_then(|values| values.get(&SessionOption::MessageCommandStyle))
                .unwrap_or(&self.global_session_options.message_command_style)
                .clone(),
        )
    }

    #[must_use]
    pub fn message_line_for_session(&self, session: Option<SessionId>) -> u8 {
        session
            .and_then(|session| self.session_options.get(&session))
            .and_then(|values| values.get(&SessionOption::MessageLine))
            .unwrap_or(&self.global_session_options.message_line)
            .parse()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn set_titles_for_session(&self, session: Option<SessionId>) -> bool {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.scalar_option_effective(target, "set-titles") == Some("on")
    }

    #[must_use]
    pub fn set_titles_string_for_session(&self, session: Option<SessionId>) -> String {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.scalar_option_effective(target, "set-titles-string")
            .unwrap_or_default()
            .to_owned()
    }

    #[must_use]
    pub fn display_panes_format_for_window(&self, window: WindowId) -> String {
        self.scalar_option_effective(TmuxOptionTarget::Window(window), "display-panes-format")
            .unwrap_or_default()
            .to_owned()
    }

    #[must_use]
    pub fn pane_border_style_values(&self, pane: PaneId) -> PaneBorderStyleValues {
        let target = TmuxOptionTarget::Pane(pane);
        PaneBorderStyleValues {
            border: self
                .scalar_option_explicit(target, "pane-border-style")
                .map(str::to_owned),
            active_border: self
                .scalar_option_explicit(target, "pane-active-border-style")
                .map(str::to_owned),
        }
    }

    #[must_use]
    pub fn has_pane_border_style_settings(&self) -> bool {
        self.has_stored_scalar_settings(&["pane-border-style", "pane-active-border-style"])
    }

    #[must_use]
    pub fn window_style_values(&self, pane: PaneId) -> WindowStyleValues {
        let target = TmuxOptionTarget::Pane(pane);
        WindowStyleValues {
            style: self
                .scalar_option_explicit(target, "window-style")
                .map(str::to_owned),
            active_style: self
                .scalar_option_explicit(target, "window-active-style")
                .map(str::to_owned),
        }
    }

    #[must_use]
    pub fn pane_colours_for_pane(&self, pane: PaneId) -> Vec<(u8, String)> {
        let Some((array, _)) =
            self.array_option_readback(TmuxOptionTarget::Pane(pane), "pane-colours", true)
        else {
            return Vec::new();
        };
        array
            .iter()
            .filter_map(|(index, value)| match index {
                ArrayIndex::Numeric(index) => u8::try_from(*index)
                    .ok()
                    .map(|index| (index, value.clone())),
                ArrayIndex::Named(_) => None,
            })
            .collect()
    }

    #[must_use]
    pub fn has_window_style_settings(&self) -> bool {
        self.has_stored_scalar_settings(&["window-style", "window-active-style"])
    }

    #[must_use]
    pub fn copy_mode_style_values(&self) -> CopyModeStyleValues {
        let explicit = |name| {
            self.stored_scalars
                .global_window
                .get(name)
                .map(String::to_owned)
        };
        CopyModeStyleValues {
            mode: explicit("mode-style"),
            search_match: explicit("copy-mode-match-style"),
            search_current_match: explicit("copy-mode-current-match-style"),
        }
    }

    fn has_stored_scalar_settings(&self, names: &[&str]) -> bool {
        let contains = |table: &ScalarTable| names.iter().any(|name| table.contains_key(*name));
        contains(&self.stored_scalars.global_window)
            || self.stored_scalars.windows.values().any(contains)
            || self.stored_scalars.panes.values().any(contains)
    }

    #[must_use]
    pub fn status_customized_for_session(&self, session: Option<SessionId>) -> bool {
        !self.explicit_status_options.is_empty()
            || session.is_some_and(|session| {
                self.session_explicit_status_options
                    .get(&session)
                    .is_some_and(|options| !options.is_empty())
            })
    }

    #[must_use]
    pub fn window_status_formats(&self, window: WindowId) -> WindowStatusFormats {
        let mut formats = self.window_status.clone();
        if let Some(overrides) = self.window_status_options.get(&window) {
            for (option, value) in overrides {
                formats
                    .set(*option, Some(value))
                    .expect("stored window status option was validated");
            }
        }
        formats
    }

    #[must_use]
    pub const fn focus_events(&self) -> bool {
        self.server_options.focus_events
    }

    #[must_use]
    pub const fn extended_keys(&self) -> &str {
        self.server_options.extended_keys.as_str()
    }

    #[must_use]
    pub const fn history_file(&self) -> &str {
        self.server_options.history_file.as_str()
    }

    #[must_use]
    pub const fn prefix_timeout_ms(&self) -> u32 {
        self.server_options.prefix_timeout_ms
    }

    #[must_use]
    pub const fn prompt_history_limit(&self) -> usize {
        self.server_options.prompt_history_limit
    }

    #[must_use]
    pub(super) fn session_knobs(&self, session: SessionId) -> SessionOptions {
        let mut values = self.global_session_options.clone();
        if let Some(overrides) = self.session_options.get(&session) {
            for (option, value) in overrides {
                values
                    .set_command(*option, Some(value))
                    .expect("stored session option was validated");
            }
        }
        values
    }

    #[must_use]
    pub fn bell_action_for_session(&self, session: SessionId) -> BellAction {
        self.session_knobs(session).bell_action
    }

    #[must_use]
    pub fn visual_bell_for_session(&self, session: SessionId) -> VisualBell {
        self.session_knobs(session).visual_bell
    }

    #[must_use]
    pub fn activity_action_for_session(&self, session: SessionId) -> BellAction {
        BellAction::parse(&self.session_knobs(session).activity_action).unwrap_or(BellAction::Other)
    }

    #[must_use]
    pub fn silence_action_for_session(&self, session: SessionId) -> BellAction {
        BellAction::parse(&self.session_knobs(session).silence_action).unwrap_or(BellAction::Other)
    }

    #[must_use]
    pub fn visual_activity_for_session(&self, session: SessionId) -> VisualBell {
        self.scalar_option_effective(TmuxOptionTarget::Session(session), "visual-activity")
            .and_then(VisualBell::parse)
            .unwrap_or(VisualBell::Off)
    }

    #[must_use]
    pub fn visual_silence_for_session(&self, session: SessionId) -> VisualBell {
        self.scalar_option_effective(TmuxOptionTarget::Session(session), "visual-silence")
            .and_then(VisualBell::parse)
            .unwrap_or(VisualBell::Off)
    }

    #[must_use]
    pub fn pane_prompt_keys_are_vi(&self, pane: PaneId) -> bool {
        let session = self
            .state
            .window_for_pane(pane)
            .map(|window| self.state.windows[&window].session);
        self.prompt_key_options(session).0
    }

    #[must_use]
    pub fn prompt_key_options(&self, session: Option<SessionId>) -> (bool, String) {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        let vi = self.scalar_option_effective(target, "status-keys") == Some("vi");
        let separators = session.map_or(self.global_word_separators.as_str(), |session| {
            self.word_separators_for_session(session)
        });
        (vi, separators.to_owned())
    }

    #[must_use]
    pub fn prompt_cursor_options(&self, session: Option<SessionId>) -> [(u8, String); 2] {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        let value = |name: &str| {
            exact_tmux_option(name)
                .and_then(|option| self.tmux_option_readback(option, target, true).ok())
                .flatten()
                .map(|(value, _)| value)
                .unwrap_or_default()
        };
        let read = |style: &str, colour: &str| {
            let style = match value(style).as_str() {
                "blinking-block" => 1,
                "block" => 2,
                "blinking-underline" => 3,
                "underline" => 4,
                "blinking-bar" => 5,
                "bar" => 6,
                _ => 0,
            };
            (style, value(colour))
        };
        [
            read("prompt-cursor-style", "prompt-cursor-colour"),
            read(
                "prompt-command-cursor-style",
                "prompt-command-cursor-colour",
            ),
        ]
    }

    fn window_option_override(&self, window: WindowId, option: WindowOption) -> Option<&str> {
        self.window_options
            .get(&window)?
            .get(&option)
            .map(String::as_str)
    }

    #[must_use]
    pub fn monitor_activity_for_window(&self, window: WindowId) -> bool {
        self.window_option_override(window, WindowOption::MonitorActivity)
            .map_or(self.global_window_options.monitor_activity, |value| {
                value == "on"
            })
    }

    #[must_use]
    pub fn monitor_bell_for_window(&self, window: WindowId) -> bool {
        self.window_option_override(window, WindowOption::MonitorBell)
            .map_or(self.global_window_options.monitor_bell, |value| {
                value == "on"
            })
    }

    #[must_use]
    pub fn monitor_silence_for_window(&self, window: WindowId) -> u32 {
        self.window_option_override(window, WindowOption::MonitorSilence)
            .and_then(|value| value.parse().ok())
            .unwrap_or(self.global_window_options.monitor_silence_seconds)
    }

    #[must_use]
    pub fn key_table_for_session(&self, session: SessionId) -> String {
        let table = self
            .session_options
            .get(&session)
            .and_then(|values| values.get(&SessionOption::KeyTable))
            .unwrap_or(&self.global_session_options.key_table);
        if table.is_empty() {
            "root".to_owned()
        } else {
            table.clone()
        }
    }

    #[must_use]
    pub fn detach_on_destroy_for_session(&self, session: SessionId) -> String {
        self.scalar_option_effective(TmuxOptionTarget::Session(session), "detach-on-destroy")
            .unwrap_or("on")
            .to_owned()
    }

    #[must_use]
    pub fn exit_empty_explicit(&self) -> Option<bool> {
        self.scalar_option_explicit(TmuxOptionTarget::Server, "exit-empty")
            .map(|value| value == "on")
    }

    #[must_use]
    pub fn exit_unattached_explicit(&self) -> Option<bool> {
        self.scalar_option_explicit(TmuxOptionTarget::Server, "exit-unattached")
            .map(|value| value == "on")
    }

    #[must_use]
    pub fn destroy_unattached_policy_for_session(&self, session: SessionId) -> Option<String> {
        self.scalar_option_explicit(TmuxOptionTarget::Session(session), "destroy-unattached")
            .map(str::to_owned)
    }

    #[must_use]
    pub fn destroy_unattached_explicit_anywhere(&self) -> bool {
        self.stored_scalars
            .global_session
            .contains_key("destroy-unattached")
            || self
                .stored_scalars
                .sessions
                .values()
                .any(|options| options.contains_key("destroy-unattached"))
    }

    pub fn display_panes_delay(
        &self,
        window: WindowId,
        delay: Option<&str>,
    ) -> Result<u32, ServerError> {
        delay.map_or_else(
            || Ok(self.window_knobs(window).display_panes_time_ms),
            |value| {
                parse_strtonum(value, 0, i64::from(u32::MAX), "delay")
                    .map(|value| u32::try_from(value).expect("delay is bounded"))
            },
        )
    }

    #[must_use]
    pub fn display_panes_options(&self, window: WindowId) -> DisplayPanesOptions {
        let target = TmuxOptionTarget::Window(window);
        let value = |name: &str| {
            self.scalar_option_effective(target, name)
                .unwrap_or_default()
                .to_owned()
        };
        DisplayPanesOptions {
            time_ms: self.window_knobs(window).display_panes_time_ms,
            colour: value("display-panes-colour"),
            active_colour: value("display-panes-active-colour"),
            format: value("display-panes-format"),
            border_style: value("display-panes-border-style"),
        }
    }

    #[must_use]
    pub fn global_default_size(&self) -> &str {
        self.global_session_options.default_size.as_str()
    }

    #[must_use]
    pub fn default_size_for_session(&self, session: SessionId) -> (u16, u16) {
        parse_default_size(&self.session_knobs(session).default_size)
            .unwrap_or(DEFAULT_WINDOW_EXTENT)
    }

    #[must_use]
    pub(super) fn window_knobs(&self, window: WindowId) -> WindowOptions {
        let mut values = self.global_window_options.clone();
        if let Some(overrides) = self.window_options.get(&window) {
            for (option, value) in overrides {
                values
                    .set_command(*option, Some(value))
                    .expect("stored window option was validated");
            }
        }
        values
    }

    #[must_use]
    pub fn preset_options_for_window(&self, window: WindowId) -> PresetOptions {
        self.window_knobs(window).preset
    }

    #[must_use]
    pub fn window_size(&self, window: WindowId) -> WindowSize {
        self.window_options
            .get(&window)
            .and_then(|overrides| overrides.get(&WindowOption::WindowSize))
            .and_then(|value| {
                [
                    WindowSize::Largest,
                    WindowSize::Smallest,
                    WindowSize::Manual,
                    WindowSize::Latest,
                ]
                .into_iter()
                .find(|size| size.as_str() == value)
            })
            .unwrap_or(self.global_window_options.window_size)
    }

    #[must_use]
    pub fn pane_border_status(&self, window: WindowId) -> PaneBorderStatus {
        self.scalar_option_effective(TmuxOptionTarget::Window(window), "pane-border-status")
            .map_or(PaneBorderStatus::Off, PaneBorderStatus::parse)
    }

    #[must_use]
    pub fn displayed_pane_border_status(&self, window: WindowId) -> PaneBorderStatus {
        let hidden = self
            .state
            .windows
            .get(&window)
            .and_then(|window| window.zoomed_pane)
            .is_some_and(|pane| self.pane_status_hidden.contains(&pane));
        if hidden {
            PaneBorderStatus::Off
        } else {
            self.pane_border_status(window)
        }
    }

    pub fn set_pane_status_hidden(&mut self, pane: PaneId, hidden: bool) {
        let changed = if hidden {
            self.pane_status_hidden.insert(pane)
        } else {
            self.pane_status_hidden.remove(&pane)
        };
        if changed {
            self.state.bump_generation();
        }
    }

    #[must_use]
    pub fn pane_border_lines(&self, window: WindowId) -> PaneBorderLines {
        let mut values = self.global_pane_options.clone();
        if let Some(overrides) = self.window_pane_options.get(&window) {
            for (option, value) in overrides {
                values
                    .set_command(*option, Some(value))
                    .expect("stored window-pane option was validated");
            }
        }
        PaneBorderLines::parse(&values.pane_border_lines)
    }

    #[must_use]
    pub fn pane_border_indicators(&self, window: WindowId) -> PaneBorderIndicators {
        PaneBorderIndicators::parse(&self.window_knobs(window).pane_border_indicators)
    }

    #[must_use]
    pub fn clock_mode_options(&self, window: WindowId) -> (String, u8) {
        let knobs = self.window_knobs(window);
        let style = match knobs.clock_mode_style.as_str() {
            "12" => 0,
            "12-with-seconds" => 2,
            "24-with-seconds" => 3,
            _ => 1,
        };
        (knobs.clock_mode_colour, style)
    }

    #[must_use]
    pub fn pane_border_format(&self, pane: PaneId) -> String {
        self.scalar_option_effective(TmuxOptionTarget::Pane(pane), "pane-border-format")
            .unwrap_or_default()
            .to_owned()
    }

    #[must_use]
    pub(super) fn pane_knobs(&self, pane: PaneId) -> PaneOptions {
        let mut values = self.global_pane_options.clone();
        if let Some(window) = self.state.window_for_pane(pane)
            && let Some(overrides) = self.window_pane_options.get(&window)
        {
            for (option, value) in overrides {
                values
                    .set_command(*option, Some(value))
                    .expect("stored window-pane option was validated");
            }
        }
        if let Some(overrides) = self.pane_options.get(&pane) {
            for (option, value) in overrides {
                values
                    .set_command(*option, Some(value))
                    .expect("stored pane option was validated");
            }
        }
        values
    }

    #[must_use]
    pub fn allow_set_title(&self, pane: PaneId) -> bool {
        self.pane_knobs(pane).allow_set_title
    }

    #[must_use]
    pub fn backspace_byte(&self) -> Option<u8> {
        let value = self.server_options.backspace.as_str();
        if let Some(code) = tmux_hex_ascii_code(value) {
            return Some(code);
        }
        if let Some(index) = TMUX_CONTROL_KEY_NAMES
            .iter()
            .position(|name| *name == value)
        {
            return u8::try_from(index).ok();
        }
        if value == "Space" {
            return Some(b' ');
        }
        let base = value.strip_prefix("C-");
        let mut characters = base.unwrap_or(value).chars();
        let (Some(character), None) = (characters.next(), characters.next()) else {
            return None;
        };
        if base.is_none() {
            return u8::try_from(u32::from(character)).ok();
        }
        match character {
            '?' => Some(0x7f),
            '@'..='_' => u8::try_from(u32::from(character))
                .ok()
                .map(|byte| byte - 0x40),
            'a'..='z' => u8::try_from(u32::from(character))
                .ok()
                .map(|byte| byte - 0x60),
            _ => None,
        }
    }

    #[must_use]
    pub fn verase_byte(&self) -> u8 {
        let value = self.server_options.backspace.as_str();
        if let Some(index) = TMUX_CONTROL_KEY_NAMES
            .iter()
            .position(|name| *name == value)
        {
            return u8::try_from(index).unwrap_or(0x7f);
        }
        if value == "Space" {
            return b' ';
        }
        if tmux_hex_ascii_code(value).is_some() {
            return 0x7f;
        }
        let mut characters = value.chars();
        match (characters.next(), characters.next()) {
            (Some(character), None) => u8::try_from(u32::from(character))
                .ok()
                .filter(|byte| *byte < 0x7f)
                .unwrap_or(0x7f),
            _ => 0x7f,
        }
    }

    #[must_use]
    pub fn remain_on_exit_format_for_pane(&self, pane: PaneId) -> String {
        let Some(option) = exact_tmux_option("remain-on-exit-format") else {
            return String::new();
        };
        self.tmux_option_readback(option, TmuxOptionTarget::Pane(pane), true)
            .ok()
            .flatten()
            .map(|(value, _)| value)
            .unwrap_or_default()
    }

    pub fn terminal_worker_options_for_pane(
        &self,
        pane: PaneId,
    ) -> Result<TerminalWorkerOptions, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let pane_options = self.pane_knobs(pane);
        let styles = self.window_style_values(pane);
        Ok(TerminalWorkerOptions {
            allow_passthrough: pane_options.allow_passthrough != AllowPassthrough::Off,
            allow_rename: pane_options.allow_rename,
            alternate_screen: pane_options.alternate_screen,
            scroll_on_clear: pane_options.scroll_on_clear,
            erase_byte: self.backspace_byte(),
            verase_byte: self.verase_byte(),
            mode_keys_vi: self.mode_keys_for_window(window) == ModeKeys::Vi,
            remain_on_exit_format: self.remain_on_exit_format_for_pane(pane),
            wrap_search: self.window_knobs(window).wrap_search,
            cursor_style: pane_options.cursor_style.as_str(),
            cursor_colour: pane_options.cursor_colour,
            window_style: styles.style,
            window_active_style: styles.active_style,
            active_pane: self
                .state
                .windows
                .get(&window)
                .is_some_and(|window| window.active_pane == pane),
            pane_colours: self.pane_colours_for_pane(pane),
        })
    }

    pub(super) fn pane_knobs_for_window(&self, window: WindowId) -> PaneOptions {
        let mut values = self.global_pane_options.clone();
        if let Some(overrides) = self.window_pane_options.get(&window) {
            for (option, value) in overrides {
                values
                    .set_command(*option, Some(value))
                    .expect("stored window-pane option was validated");
            }
        }
        values
    }

    pub fn set_default_mode_keys(&mut self, value: &str) -> Result<(), ServerError> {
        self.global_mode_keys = parse_mode_keys(Some(value), self.global_mode_keys)?;
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        Ok(())
    }

    pub fn set_default_status_keys(&mut self, value: &str) -> Result<(), ServerError> {
        let keys = parse_mode_keys(Some(value), self.global_mode_keys)?;
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        self.stored_scalars
            .global_session
            .insert("status-keys", keys.as_str().to_owned());
        Ok(())
    }

    pub fn initialize_default_shell(&mut self, value: impl Into<String>) {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        self.global_default_shell = value.into();
    }

    pub fn initialize_default_editor(&mut self, value: impl Into<String>) {
        self.format_options_generation = self.format_options_generation.wrapping_add(1);
        self.server_options.editor = value.into();
    }

    pub fn global_default_shell(&self) -> &str {
        &self.global_default_shell
    }

    pub fn default_shell_for_session(&self, session: SessionId) -> Result<&str, ServerError> {
        if !self.state.sessions.contains_key(&session) {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        Ok(self
            .session_default_shells
            .get(&session)
            .map_or(self.global_default_shell.as_str(), String::as_str))
    }

    pub fn default_command_for_session(&self, session: SessionId) -> Result<&str, ServerError> {
        if !self.state.sessions.contains_key(&session) {
            return Err(ServerError::MissingTarget(session.to_string()));
        }
        Ok(self
            .session_default_commands
            .get(&session)
            .map_or(self.global_default_command.as_str(), String::as_str))
    }

    pub fn popup_options_for_window(&self, window: WindowId) -> Result<PopupOptions, ServerError> {
        if !self.state.windows.contains_key(&window) {
            return Err(ServerError::MissingTarget(window.to_string()));
        }
        Ok(PopupOptions {
            style: self
                .window_popup_styles
                .get(&window)
                .cloned()
                .unwrap_or_else(|| self.global_popup_style.clone()),
            border_style: self
                .window_popup_border_styles
                .get(&window)
                .cloned()
                .unwrap_or_else(|| self.global_popup_border_style.clone()),
            border_lines: self
                .window_popup_border_lines
                .get(&window)
                .copied()
                .unwrap_or(self.global_popup_border_lines),
        })
    }

    pub fn menu_options_for_window(&self, window: WindowId) -> Result<MenuOptions, ServerError> {
        if !self.state.windows.contains_key(&window) {
            return Err(ServerError::MissingTarget(window.to_string()));
        }
        Ok(MenuOptions {
            style: self
                .window_menu_styles
                .get(&window)
                .cloned()
                .unwrap_or_else(|| self.global_menu_style.clone()),
            selected_style: self
                .window_menu_selected_styles
                .get(&window)
                .cloned()
                .unwrap_or_else(|| self.global_menu_selected_style.clone()),
            border_style: self
                .window_menu_border_styles
                .get(&window)
                .cloned()
                .unwrap_or_else(|| self.global_menu_border_style.clone()),
            border_lines: self
                .window_menu_border_lines
                .get(&window)
                .copied()
                .unwrap_or(self.global_menu_border_lines),
        })
    }

    #[must_use]
    pub fn mux_option_value(&self, option: MuxOptionKey) -> String {
        match option {
            MuxOptionKey::Prefix => if self.keys.prefix() == " " {
                "Space"
            } else {
                self.keys.prefix()
            }
            .to_owned(),
            MuxOptionKey::ModeKeys => self.global_mode_keys.as_str().to_owned(),
            MuxOptionKey::HistoryLimit => self.global_history_limit.to_string(),
            MuxOptionKey::WordSeparators => self.global_word_separators.clone(),
            MuxOptionKey::CopyCommand => self.copy_command.clone(),
            MuxOptionKey::SetClipboard => self.set_clipboard.as_str().to_owned(),
            MuxOptionKey::BufferLimit => self.buffer_limit.to_string(),
            MuxOptionKey::HistoryTrickle => self.history_trickle.to_string(),
            MuxOptionKey::SynchronizePanes => if self.state.global_synchronize_panes() {
                "on"
            } else {
                "off"
            }
            .to_owned(),
            MuxOptionKey::ExperimentalEditorPane => if self.experimental_editor_pane {
                "on"
            } else {
                "off"
            }
            .to_owned(),
            MuxOptionKey::AgentCommand => self.agent.command.clone(),
            MuxOptionKey::AgentClaudeCodeCommand => self.agent.claude_code_command.clone(),
            MuxOptionKey::AgentAutoApprove => self.agent.auto_approve.as_str().to_owned(),
            MuxOptionKey::Mouse => tmux_flag(self.global_mouse).to_owned(),
            MuxOptionKey::EscapeTime => self.escape_time_ms.to_string(),
            MuxOptionKey::Prefix2 => self
                .scalar_option_effective(TmuxOptionTarget::GlobalSession, "prefix2")
                .unwrap_or("None")
                .to_owned(),
            MuxOptionKey::FocusFollowsMouse => {
                tmux_flag(self.effective_focus_follows_mouse(None)).to_owned()
            }
            MuxOptionKey::ExtendedKeys => self.extended_keys().to_owned(),
            MuxOptionKey::FocusEvents => tmux_flag(self.focus_events()).to_owned(),
            MuxOptionKey::ClearOnAttach => self
                .scalar_option_effective(TmuxOptionTarget::Server, "clear-on-attach")
                .unwrap_or("on")
                .to_owned(),
            MuxOptionKey::CommandAlias => self
                .array_option(TmuxOptionTarget::Server, "command-alias")
                .map(|aliases| aliases.values().cloned().collect::<Vec<_>>().join("\n"))
                .unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn effective_mouse(&self, session: Option<SessionId>) -> bool {
        session.map_or(self.global_mouse, |session| self.mouse_for_session(session))
    }

    #[must_use]
    pub fn effective_focus_follows_mouse(&self, session: Option<SessionId>) -> bool {
        let target = session.map_or(TmuxOptionTarget::GlobalSession, TmuxOptionTarget::Session);
        self.scalar_option_effective(target, "focus-follows-mouse") == Some("on")
    }

    #[must_use]
    pub const fn agent_options(&self) -> &AgentOptions {
        &self.agent
    }

    pub fn history_limit_for_pane(&self, pane: PaneId) -> Result<usize, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let session = self.state.windows[&window].session;
        Ok(self.history_limit_for_session(session))
    }

    #[must_use]
    pub fn history_limit_for_session(&self, session: SessionId) -> usize {
        self.session_history_limits
            .get(&session)
            .copied()
            .unwrap_or(self.global_history_limit)
    }

    #[must_use]
    pub fn base_index_for_session(&self, session: SessionId) -> u32 {
        self.session_base_indices
            .get(&session)
            .copied()
            .unwrap_or(self.global_base_index)
    }

    #[must_use]
    pub fn renumber_windows_for_session(&self, session: SessionId) -> bool {
        self.session_renumber_windows
            .get(&session)
            .copied()
            .unwrap_or(self.global_renumber_windows)
    }

    pub(super) fn pane_base_index_for_window(&self, window: WindowId) -> u32 {
        self.state.pane_base_index(window)
    }

    pub fn word_separators_for_pane(&self, pane: PaneId) -> Result<&str, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let session = self.state.windows[&window].session;
        Ok(self.word_separators_for_session(session))
    }

    #[must_use]
    pub fn word_separators_for_session(&self, session: SessionId) -> &str {
        self.session_word_separators
            .get(&session)
            .map_or(self.global_word_separators.as_str(), String::as_str)
    }

    #[must_use]
    pub fn default_terminal_for_spawn(&self) -> &str {
        self.default_terminal.as_deref().unwrap_or(DEFAULT_TERMINAL)
    }

    #[must_use]
    pub fn initial_repeat_time_for_session(&self, session: SessionId) -> u32 {
        self.session_initial_repeat_time_ms
            .get(&session)
            .copied()
            .unwrap_or(self.global_initial_repeat_time_ms)
    }

    #[must_use]
    pub fn repeat_time_for_session(&self, session: SessionId) -> u32 {
        self.session_repeat_time_ms
            .get(&session)
            .copied()
            .unwrap_or(self.global_repeat_time_ms)
    }

    pub fn display_time_for_pane(&self, pane: PaneId) -> Result<u32, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        let session = self.state.windows[&window].session;
        Ok(self.display_time_for_session(session))
    }

    pub fn retain_exited_pane(&self, pane: PaneId, failed: bool) -> Result<bool, ServerError> {
        Ok(match self.remain_on_exit_for_pane(pane)? {
            RemainOnExit::Off => false,
            RemainOnExit::On | RemainOnExit::Key => true,
            RemainOnExit::Failed | RemainOnExit::FailedKey => failed,
        })
    }

    #[must_use]
    pub fn dead_pane_dismisses_on_key(&self, pane: PaneId) -> bool {
        self.state.pane(pane).is_some_and(|pane| pane.dead)
            && self
                .remain_on_exit_for_pane(pane)
                .is_ok_and(|value| matches!(value, RemainOnExit::Key | RemainOnExit::FailedKey))
    }

    #[must_use]
    pub fn display_time_for_session(&self, session: SessionId) -> u32 {
        self.session_display_time_ms
            .get(&session)
            .copied()
            .unwrap_or(self.global_display_time_ms)
    }

    pub fn display_time_for_client_session(&self, session: Option<SessionId>) -> u32 {
        session.map_or(self.global_display_time_ms, |session| {
            self.display_time_for_session(session)
        })
    }

    pub(super) fn mouse_for_session(&self, session: SessionId) -> bool {
        self.session_mouse
            .get(&session)
            .copied()
            .unwrap_or(self.global_mouse)
    }

    pub(super) fn automatic_rename_format_for_window(&self, window: WindowId) -> &str {
        self.window_automatic_rename_formats
            .get(&window)
            .map_or(self.global_automatic_rename_format.as_str(), String::as_str)
    }

    pub(super) fn remain_on_exit_for_window(&self, window: WindowId) -> RemainOnExit {
        self.window_remain_on_exit
            .get(&window)
            .copied()
            .unwrap_or(self.global_remain_on_exit)
    }

    pub(super) fn remain_on_exit_for_pane(
        &self,
        pane: PaneId,
    ) -> Result<RemainOnExit, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        Ok(self
            .pane_remain_on_exit
            .get(&pane)
            .copied()
            .unwrap_or_else(|| self.remain_on_exit_for_window(window)))
    }

    pub fn copy_mode_table_for_pane(&self, pane: PaneId) -> Result<&'static str, ServerError> {
        let window = self
            .state
            .window_for_pane(pane)
            .ok_or_else(|| ServerError::MissingTarget(pane.to_string()))?;
        Ok(self.mode_keys_for_window(window).table())
    }

    pub(super) fn mode_keys_for_window(&self, window: WindowId) -> ModeKeys {
        self.window_mode_keys
            .get(&window)
            .copied()
            .unwrap_or(self.global_mode_keys)
    }

    pub fn synchronized_input_targets(&self, source: PaneId) -> Result<Vec<PaneId>, ServerError> {
        self.state.synchronized_input_targets(source)
    }

    pub fn pane_input_off(&self, pane: PaneId) -> Result<bool, ServerError> {
        self.state.pane_input_off(pane)
    }

    #[must_use]
    pub fn editor_option(&self) -> String {
        self.global_tmux_option_value("editor")
            .unwrap_or_else(|| "/usr/bin/vi".to_owned())
    }

    #[must_use]
    pub fn terminal_features_option(&self) -> Vec<String> {
        self.array_option_readback(TmuxOptionTarget::Server, "terminal-features", true)
            .into_iter()
            .flat_map(|(array, _)| array.values().cloned())
            .collect()
    }

    pub fn user_keys_option(&self) -> Vec<String> {
        let Some(array) = self.stored_arrays.server.get("user-keys") else {
            return Vec::new();
        };
        let mut keys = Vec::new();
        for (index, value) in array {
            let ArrayIndex::Numeric(index) = index else {
                continue;
            };
            let Ok(index) = usize::try_from(*index) else {
                continue;
            };
            if index > MAX_USER_KEY {
                continue;
            }
            if keys.len() <= index {
                keys.resize(index + 1, String::new());
            }
            keys[index].clone_from(value);
        }
        keys
    }

    #[must_use]
    pub fn user_key_claims(&self, bytes: &[u8]) -> bool {
        self.stored_arrays
            .server
            .get("user-keys")
            .is_some_and(|array| {
                array.values().any(|value| {
                    let sequence = value.as_bytes();
                    !sequence.is_empty()
                        && (bytes.starts_with(sequence) || sequence.starts_with(bytes))
                })
            })
    }

    pub fn terminal_overrides_option(&self) -> Vec<String> {
        self.array_option_readback(TmuxOptionTarget::Server, "terminal-overrides", true)
            .into_iter()
            .flat_map(|(array, _)| array.values().cloned())
            .collect()
    }
}

pub(super) fn parse_tmux_flag_value(
    value: Option<&str>,
    current: bool,
) -> Result<bool, ServerError> {
    let Some(value) = value else {
        return Ok(!current);
    };
    if value.is_empty() {
        return Ok(!current);
    }
    if value == "1"
        || ["on", "yes"]
            .iter()
            .any(|spelling| value.eq_ignore_ascii_case(spelling))
    {
        return Ok(true);
    }
    if value == "0"
        || ["off", "no"]
            .iter()
            .any(|spelling| value.eq_ignore_ascii_case(spelling))
    {
        return Ok(false);
    }
    Err(ServerError::InvalidCommand(format!("bad value: {value}")))
}

pub(super) fn parse_remain_on_exit(
    value: Option<&str>,
    current: RemainOnExit,
) -> Result<RemainOnExit, ServerError> {
    let Some(value) = value else {
        return Ok(current.toggled());
    };
    if value.is_empty() {
        return Ok(current.toggled());
    }
    match value {
        "off" => Ok(RemainOnExit::Off),
        "on" => Ok(RemainOnExit::On),
        "failed" => Ok(RemainOnExit::Failed),
        "key" => Ok(RemainOnExit::Key),
        "failed-key" => Ok(RemainOnExit::FailedKey),
        _ => Err(ServerError::InvalidCommand(format!(
            "unknown value: {value}"
        ))),
    }
}

pub(super) fn parse_flag_value(value: Option<&str>, current: bool) -> Result<bool, ServerError> {
    let Some(value) = value else {
        return Ok(!current);
    };
    if value.is_empty() {
        return Ok(!current);
    }
    if value == "1"
        || ["on", "yes", "true"]
            .iter()
            .any(|spelling| value.eq_ignore_ascii_case(spelling))
    {
        return Ok(true);
    }
    if value == "0"
        || ["off", "no", "false"]
            .iter()
            .any(|spelling| value.eq_ignore_ascii_case(spelling))
    {
        return Ok(false);
    }
    Err(ServerError::InvalidCommand(format!(
        "invalid flag value: {value}"
    )))
}

pub(super) fn parse_mode_keys(
    value: Option<&str>,
    current: ModeKeys,
) -> Result<ModeKeys, ServerError> {
    match value {
        None => Ok(current.toggled()),
        Some("vi") => Ok(ModeKeys::Vi),
        Some("emacs") => Ok(ModeKeys::Emacs),
        Some(value) => Err(ServerError::InvalidCommand(format!(
            "invalid mode-keys value: {value}"
        ))),
    }
}

pub(super) fn parse_history_limit(value: &str) -> Result<usize, ServerError> {
    let limit = value.parse::<usize>().map_err(|_| {
        ServerError::InvalidCommand(format!("invalid history-limit value: {value}"))
    })?;
    if limit > MAX_HISTORY_LIMIT {
        return Err(ServerError::InvalidCommand(format!(
            "history-limit must be between 0 and {MAX_HISTORY_LIMIT}"
        )));
    }
    Ok(limit)
}

pub(super) fn parse_index_option(value: &str, maximum: u32) -> Result<u32, ServerError> {
    match value.parse::<i128>() {
        Ok(index) if index < 0 => Err(ServerError::InvalidCommand(format!(
            "value is too small: {value}"
        ))),
        Ok(index) if index > i128::from(maximum) => Err(ServerError::InvalidCommand(format!(
            "value is too large: {value}"
        ))),
        Ok(index) => Ok(u32::try_from(index).expect("bounded index fits u32")),
        Err(_) if decimal_digits(value.strip_prefix('-')) => Err(ServerError::InvalidCommand(
            format!("value is too small: {value}"),
        )),
        Err(_) if decimal_digits(Some(value.strip_prefix('+').unwrap_or(value))) => Err(
            ServerError::InvalidCommand(format!("value is too large: {value}")),
        ),
        Err(_) => Err(ServerError::InvalidCommand(format!(
            "value is invalid: {value}"
        ))),
    }
}

pub(super) fn decimal_digits(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
}

pub(super) fn validate_word_separators(value: &str) -> Result<(), ServerError> {
    if value.len() > MAX_WORD_SEPARATORS_BYTES {
        return Err(ServerError::InvalidCommand(format!(
            "word-separators exceeds {MAX_WORD_SEPARATORS_BYTES} bytes"
        )));
    }
    Ok(())
}
