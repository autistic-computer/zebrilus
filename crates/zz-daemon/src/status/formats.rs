use super::*;

#[derive(Clone)]
pub(crate) struct ModeRequest {
    pub(crate) pane: PaneId,
    pub(crate) view: bool,
    pub(crate) context: StatusContext<'static>,
    pub(crate) position: u32,
    pub(crate) limit: u32,
    pub(crate) vi_keys: bool,
    pub(crate) line_numbers: u8,
    pub(crate) hide_position: bool,
    pub(crate) rows: u32,
}

pub(crate) fn expand_style(
    format: &str,
    context: &StatusContext,
    hooks: &mut DaemonFormatHooks<'_>,
) -> String {
    let style = clamp_status_text(expand_status(format, context, hooks));
    if zz_protocol::parse_style(&style).is_some() {
        style
    } else {
        String::new()
    }
}

pub(crate) fn expand_modes(
    requests: &[ModeRequest],
    facts: &FormatHookFacts,
    engine: &MuxEngine,
) -> Vec<zz_protocol::ModePresentation> {
    requests
        .iter()
        .take(zz_protocol::MAX_MODE_PRESENTATIONS)
        .map(|mode| {
            let empty = BTreeMap::new();
            let line_numbers = mode_line_numbers(
                mode,
                &mut DaemonFormatHooks::command_with_variables(facts, &empty)
                    .with_option_engine(engine),
            );
            let variables = mode_position_variables(mode, line_numbers);
            let mut hooks = DaemonFormatHooks::command_with_variables(facts, &variables)
                .with_option_engine(engine);
            mode_presentation(mode, line_numbers, &mut hooks)
        })
        .collect()
}

pub(crate) const MODE_FORMATS: [&str; 9] = [
    "#{E:copy-mode-position-format}",
    "#{E:copy-mode-position-style}",
    "#{E:copy-mode-selection-style}",
    "#{E:copy-mode-match-style}",
    "#{E:copy-mode-current-match-style}",
    "#{copy-mode-line-numbers}",
    "#{E:copy-mode-line-number-style}",
    "#{E:copy-mode-current-line-number-style}",
    "#{E:copy-mode-current-line-style}",
];

pub(super) fn mode_line_numbers(mode: &ModeRequest, hooks: &mut DaemonFormatHooks<'_>) -> u8 {
    if mode.line_numbers == 0 {
        return 0;
    }
    let option = expand_status(MODE_FORMATS[5], &mode.context, hooks);
    zz_terminal::copy_line_number_mode(mode.line_numbers, &option)
}

pub(super) fn mode_position_variables(
    mode: &ModeRequest,
    line_numbers: u8,
) -> BTreeMap<String, String> {
    let (position, limit) = if line_numbers >= 2 {
        (
            mode.limit.saturating_sub(mode.position).saturating_add(1),
            mode.limit.saturating_add(mode.rows),
        )
    } else {
        (mode.position, mode.limit)
    };
    BTreeMap::from([
        ("copy_position".to_owned(), position.to_string()),
        ("copy_position_limit".to_owned(), limit.to_string()),
    ])
}

pub(super) fn mode_presentation(
    mode: &ModeRequest,
    line_numbers: u8,
    hooks: &mut DaemonFormatHooks<'_>,
) -> zz_protocol::ModePresentation {
    let [
        position,
        position_style,
        selection_style,
        match_style,
        current_match_style,
        _,
        line_number_style,
        current_line_number_style,
        current_line_style,
    ] = MODE_FORMATS;
    let (line_number_style, current_line_number_style) = if line_numbers == 0 {
        (String::new(), String::new())
    } else {
        (
            expand_style(line_number_style, &mode.context, hooks),
            expand_style(current_line_number_style, &mode.context, hooks),
        )
    };
    let current_line_style =
        if expand_status("#{copy-mode-current-line-style}", &mode.context, hooks) == "default" {
            String::new()
        } else {
            expand_style(current_line_style, &mode.context, hooks)
        };
    zz_protocol::ModePresentation {
        pane: mode.pane,
        view: mode.view,
        line_numbers,
        line_number_style,
        current_line_number_style,
        current_line_style,
        position: if mode.hide_position {
            String::new()
        } else {
            clamp_status_text(expand_status(position, &mode.context, hooks))
        },
        position_style: expand_style(position_style, &mode.context, hooks),
        selection_style: expand_style(selection_style, &mode.context, hooks),
        vi_keys: mode.vi_keys,
        match_style: expand_style(match_style, &mode.context, hooks),
        current_match_style: expand_style(current_match_style, &mode.context, hooks),
    }
}

const THEME_SLOTS: [(&str, u8); zz_protocol::COLOUR_THEME_COUNT] = [
    ("black", 0),
    ("white", 7),
    ("light-grey", 7),
    ("dark-grey", 0),
    ("green", 2),
    ("yellow", 3),
    ("red", 1),
    ("blue", 4),
    ("cyan", 6),
    ("magenta", 5),
];

pub(super) fn theme_formats() -> &'static [String] {
    static FORMATS: LazyLock<Vec<String>> = LazyLock::new(|| {
        std::iter::once("#{theme}".to_owned())
            .chain(["light", "dark"].into_iter().flat_map(|half| {
                THEME_SLOTS
                    .iter()
                    .map(move |(suffix, _)| format!("#{{E:{half}-theme-{suffix}}}"))
            }))
            .collect()
    });
    &FORMATS
}

pub(crate) fn status_line_needs(
    engine: &MuxEngine,
    formats: &StatusFormats,
    row_formats: &BTreeMap<u32, String>,
    title_format: Option<&str>,
    message_styles: &(String, String),
) -> FormatNeeds {
    engine.cached_format_needs(status_line_templates(
        formats,
        row_formats,
        title_format,
        message_styles,
    ))
}

pub(crate) fn status_line_templates<'a>(
    formats: &'a StatusFormats,
    row_formats: &'a BTreeMap<u32, String>,
    title_format: Option<&'a str>,
    message_styles: &'a (String, String),
) -> impl Iterator<Item = &'a str> {
    let lines = if formats.enabled {
        usize::from(formats.lines).min(MAX_STATUS_ROWS)
    } else {
        0
    };
    let end = u32::try_from(lines).expect("status row count fits u32");
    [
        formats.left.as_str(),
        formats.right.as_str(),
        formats.style.as_str(),
        message_styles.0.as_str(),
        message_styles.1.as_str(),
    ]
    .into_iter()
    .chain(row_formats.range(..end).map(|(_, format)| format.as_str()))
    .chain(title_format)
    .chain(["#{socket_path}:#{session_path}:#{pane_current_path}"])
    .chain(theme_formats().iter().map(String::as_str))
}

pub(super) fn resolve_theme_colours(
    request: &StatusRequest,
    hooks: &mut DaemonFormatHooks<'_>,
) -> zz_protocol::ThemeColours {
    let mode = expand_status("#{theme}", &request.context, hooks);
    if mode == "terminal" {
        return zz_protocol::ThemeColours(THEME_SLOTS.map(|(_, index)| TmuxColour::Basic(index)));
    }
    let half = match mode.as_str() {
        "light" => "light",
        "dark" => "dark",
        _ => match request.client_scheme {
            Some(TerminalColorScheme::Light) => "light",
            _ => "dark",
        },
    };
    zz_protocol::ThemeColours(THEME_SLOTS.map(|(suffix, _)| {
        let expanded = expand_status(
            &format!("#{{E:{half}-theme-{suffix}}}"),
            &request.context,
            hooks,
        );
        match zz_protocol::parse_tmux_colour(&expanded) {
            Some(TmuxColour::Theme(_)) | None => TmuxColour::Basic(8),
            Some(colour) => colour,
        }
    }))
}
