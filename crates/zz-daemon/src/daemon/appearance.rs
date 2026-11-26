use super::*;

pub(super) fn resolve_appearance(
    color_scheme: TerminalColorScheme,
    entries: &[ConfigOverrideEntry],
) -> AppearanceLoad {
    let mut load = AppearanceLoad::defaults_for(color_scheme);
    load.root = zz_config::config_path_for_write().ok();
    apply_appearance_overrides(load, entries)
}

pub(super) fn send_terminal_themes(
    outbound: &Arc<OutboundMailbox>,
    listing: &Arc<Mutex<Option<u64>>>,
    request_id: u64,
) {
    if listing.lock().replace(request_id).is_some() {
        return;
    }
    let outbound = Arc::clone(outbound);
    let pending = Arc::clone(listing);
    let listed = std::thread::Builder::new()
        .name("zz-terminal-themes".to_owned())
        .spawn(move || {
            let themes = zz_terminal::enumerate_ghostty_themes(
                zz_config::config_path_for_write().ok().as_deref(),
            )
            .into_iter()
            .filter(|theme| theme.name.len() <= zz_protocol::MAX_TERMINAL_THEME_NAME_BYTES)
            .take(zz_protocol::MAX_TERMINAL_THEMES)
            .map(|theme| zz_protocol::TerminalTheme::new(theme.name, &theme.appearance))
            .collect();
            let Some(request_id) = pending.lock().take() else {
                return;
            };
            let _ = outbound
                .enqueue_reliable(&ProtocolMessage::TerminalThemesResponse { request_id, themes });
        });
    if let Err(error) = listed {
        listing.lock().take();
        log::warn!("could not list terminal themes: {error}");
    }
}

const CURSOR_STYLE_OVERRIDES: [(&str, Option<(CursorStyle, CursorBlinkPolicy)>); 7] = [
    ("default", None),
    (
        "blinking-block",
        Some((CursorStyle::Block, CursorBlinkPolicy::On)),
    ),
    ("block", Some((CursorStyle::Block, CursorBlinkPolicy::Off))),
    (
        "blinking-underline",
        Some((CursorStyle::Underline, CursorBlinkPolicy::On)),
    ),
    (
        "underline",
        Some((CursorStyle::Underline, CursorBlinkPolicy::Off)),
    ),
    (
        "blinking-bar",
        Some((CursorStyle::Bar, CursorBlinkPolicy::On)),
    ),
    ("bar", Some((CursorStyle::Bar, CursorBlinkPolicy::Off))),
];

pub(super) struct ResolvedTerminalWorkerOptions {
    pub(super) appearance: Arc<TerminalAppearance>,
    pub(super) allow_passthrough: bool,
    pub(super) knobs: EngineKnobs,
    pub(super) wrap_search: bool,
}

pub(super) fn terminal_worker_options(
    engine: &MuxEngine,
    base_appearance: &Arc<TerminalAppearance>,
    config_files: &str,
    pane: PaneId,
) -> Result<ResolvedTerminalWorkerOptions, ServerError> {
    let options = engine.terminal_worker_options_for_pane(pane)?;
    let window_colours = window_style_colours(engine, config_files, &options, pane);
    Ok(ResolvedTerminalWorkerOptions {
        appearance: pane_terminal_appearance(
            base_appearance,
            options.cursor_style,
            &options.cursor_colour,
            window_colours,
            &options.pane_colours,
        ),
        allow_passthrough: options.allow_passthrough,
        knobs: EngineKnobs {
            scroll_on_clear: options.scroll_on_clear,
            alternate_screen: options.alternate_screen,
            allow_rename: options.allow_rename,
            erase_byte: options.erase_byte,
            verase_byte: options.verase_byte,
            mode_keys_vi: options.mode_keys_vi,
        },
        wrap_search: options.wrap_search,
    })
}

fn window_style_colours(
    engine: &MuxEngine,
    config_files: &str,
    options: &zz_mux::TerminalWorkerOptions,
    pane: PaneId,
) -> (Option<TmuxColour>, Option<TmuxColour>) {
    if options.window_style.is_none() && options.window_active_style.is_none() {
        return (None, None);
    }
    let window = engine.state.window_for_pane(pane);
    let session = window
        .and_then(|window| engine.state.windows.get(&window))
        .map(|window| window.session);
    let resolve = |value: &Option<String>| {
        value.as_deref().and_then(|value| {
            let expanded =
                expanded_style_value(engine, config_files, value, session, window, Some(pane));
            zz_protocol::parse_style(&expanded)
        })
    };
    let base = resolve(&options.window_style);
    let active = if options.active_pane {
        resolve(&options.window_active_style)
    } else {
        None
    };
    let fg = active
        .as_ref()
        .and_then(|style| style.fg)
        .or_else(|| base.as_ref().and_then(|style| style.fg));
    let bg = active
        .and_then(|style| style.bg)
        .or_else(|| base.and_then(|style| style.bg));
    (fg, bg)
}

pub(super) fn pane_terminal_appearance(
    base: &Arc<TerminalAppearance>,
    cursor_style: &str,
    cursor_colour: &str,
    window_colours: (Option<TmuxColour>, Option<TmuxColour>),
    pane_colours: &[(u8, String)],
) -> Arc<TerminalAppearance> {
    let cursor_style = CURSOR_STYLE_OVERRIDES
        .iter()
        .find_map(|(candidate, mapped)| (*candidate == cursor_style).then_some(*mapped))
        .flatten();
    let resolved_color = pane_cursor_color(base, cursor_colour);
    let ground = |colour: Option<TmuxColour>| {
        colour.and_then(|colour| {
            Some((
                appearance_tmux_colour(base, colour)?,
                tmux_colour_class(colour)?,
            ))
        })
    };
    let foreground = ground(window_colours.0);
    let background = ground(window_colours.1);
    let palette = pane_palette(base, pane_colours);
    if cursor_style.is_none()
        && resolved_color.is_none()
        && foreground.is_none()
        && background.is_none()
        && palette.is_none()
    {
        return Arc::clone(base);
    }
    let mut appearance = (**base).clone();
    if let Some((palette, classes)) = palette {
        appearance.palette = palette;
        appearance.palette_classes = classes;
    }
    if let Some((style, blink)) = cursor_style {
        appearance.cursor_style = style;
        appearance.cursor_blink_policy = blink;
    }
    if let Some(color) = resolved_color {
        appearance.cursor_color = color;
    }
    if let Some((color, class)) = foreground {
        appearance.foreground = color;
        appearance.default_classes[0] = Some(class);
    }
    if let Some((color, class)) = background {
        appearance.background = color;
        appearance.default_classes[1] = Some(class);
    }
    Arc::new(appearance)
}

fn tmux_colour_class(colour: TmuxColour) -> Option<ColourClass> {
    match colour {
        TmuxColour::Default | TmuxColour::Terminal => None,
        TmuxColour::Basic(entry) | TmuxColour::Indexed(entry) => Some(ColourClass::Palette(entry)),
        TmuxColour::Rgb(_) | TmuxColour::Theme(_) => Some(ColourClass::Rgb),
    }
}

fn pane_palette(
    base: &TerminalAppearance,
    pane_colours: &[(u8, String)],
) -> Option<(TerminalPalette, Vec<(u8, ColourClass)>)> {
    let mut colors = base.palette.into_array();
    let mut classes = Vec::new();
    for (index, value) in pane_colours {
        let Some((color, class)) = parse_tmux_colour(value).and_then(|colour| {
            Some((
                appearance_tmux_colour(base, colour)?,
                tmux_colour_class(colour)?,
            ))
        }) else {
            continue;
        };
        colors[usize::from(*index)] = color;
        classes.retain(|(entry, _)| entry != index);
        classes.push((*index, class));
    }
    (!classes.is_empty()).then(|| (TerminalPalette::new(colors), classes))
}

fn pane_cursor_color(appearance: &TerminalAppearance, value: &str) -> Option<Color> {
    if let Some(color) = parse_x11_color(value)
        && value.chars().any(char::is_whitespace)
    {
        return Some(color);
    }
    appearance_tmux_colour(appearance, parse_tmux_colour(value)?)
}

fn appearance_tmux_colour(appearance: &TerminalAppearance, colour: TmuxColour) -> Option<Color> {
    match colour {
        TmuxColour::Basic(index) | TmuxColour::Indexed(index) => appearance
            .palette
            .as_array()
            .get(usize::from(index))
            .copied(),
        TmuxColour::Rgb(value) => Some(Color::rgb(
            u8::try_from((value >> 16) & 0xff).expect("red component is one byte"),
            u8::try_from((value >> 8) & 0xff).expect("green component is one byte"),
            u8::try_from(value & 0xff).expect("blue component is one byte"),
        )),
        TmuxColour::Theme(index) => [0, 7, 7, 0, 2, 3, 1, 4, 6, 5]
            .get(usize::from(index))
            .and_then(|index| appearance.palette.as_array().get(*index))
            .copied(),
        TmuxColour::Default | TmuxColour::Terminal => None,
    }
}

pub(super) fn published_appearance(inner: &ServerState) -> Arc<TerminalAppearance> {
    let styles = inner.engine.copy_mode_style_values();
    if styles == CopyModeStyleValues::default() {
        return Arc::clone(&inner.config.appearance);
    }
    let base = Arc::clone(&inner.config.appearance);
    let mut appearance = (*base).clone();
    let resolve = |value: &str| {
        let expanded = expanded_style_value(
            &inner.engine,
            &inner.config.config_files,
            value,
            None,
            None,
            None,
        );
        zz_protocol::parse_style(&expanded).map(|style| {
            (
                style.fg.and_then(|fg| appearance_tmux_colour(&base, fg)),
                style.bg.and_then(|bg| appearance_tmux_colour(&base, bg)),
            )
        })
    };
    if let Some((fg, bg)) = styles.mode.as_deref().and_then(resolve) {
        if let Some(color) = fg {
            appearance.selection_foreground = color;
        }
        if let Some(color) = bg {
            appearance.selection_background = AppearanceColor::rgba(color.r, color.g, color.b, 255);
        }
    }
    if let Some((fg, bg)) = styles.search_match.as_deref().and_then(resolve)
        && let Some(color) = bg.or(fg)
    {
        appearance.search_match_color = AppearanceColor::rgba(color.r, color.g, color.b, 255);
    }
    if let Some((fg, bg)) = styles.search_current_match.as_deref().and_then(resolve)
        && let Some(color) = bg.or(fg)
    {
        appearance.search_current_color = AppearanceColor::rgba(color.r, color.g, color.b, 255);
    }
    Arc::new(appearance)
}

pub(super) fn terminal_appearance_updates(
    inner: &ServerState,
) -> Vec<(Arc<TerminalSession>, Arc<TerminalAppearance>)> {
    let mut updates = Vec::with_capacity(
        inner.terminals.len()
            + inner
                .clients
                .values()
                .filter(|c| c.command_output.is_some())
                .count(),
    );
    for (pane, terminal) in inner.terminals.iter() {
        let appearance = terminal_worker_options(
            &inner.engine,
            &inner.config.appearance,
            &inner.config.config_files,
            *pane,
        )
        .map_or_else(
            |_| Arc::clone(&inner.config.appearance),
            |options| options.appearance,
        );
        updates.push((Arc::clone(terminal), appearance));
    }
    for output in inner
        .clients
        .values()
        .filter_map(|c| c.command_output.as_ref())
    {
        let appearance = terminal_worker_options(
            &inner.engine,
            &inner.config.appearance,
            &inner.config.config_files,
            output.pane,
        )
        .map_or_else(
            |_| Arc::clone(&inner.config.appearance),
            |options| options.appearance,
        );
        updates.push((Arc::clone(&output.terminal), appearance));
    }
    updates
}

pub(super) fn log_appearance_load(reason: &str, load: &AppearanceLoad) {
    log::info!(
        target: "zz_daemon::diagnostics::appearance",
        "loaded appearance reason={reason} root={:?} scheme={} supported={} unsupported={} invalid={} diagnostics_dropped={} bytes={} hash={} appearance={:?}",
        load.root,
        load.appearance.color_scheme.as_str(),
        load.supported,
        load.unsupported,
        load.invalid,
        load.diagnostics_dropped,
        load.bytes_read,
        load.appearance.stable_hash(),
        load.appearance,
    );
    for diagnostic in &load.diagnostics {
        match diagnostic.disposition {
            AppearanceConfigDisposition::Invalid => log::warn!(
                target: "zz_daemon::diagnostics::appearance",
                "config reason={reason} path={} line={} key={:?} disposition={:?} message={}",
                diagnostic.path.display(),
                diagnostic.line,
                diagnostic.key,
                diagnostic.disposition,
                diagnostic.message,
            ),
            _ => log::debug!(
                target: "zz_daemon::diagnostics::appearance",
                "config reason={reason} path={} line={} key={:?} disposition={:?} message={}",
                diagnostic.path.display(),
                diagnostic.line,
                diagnostic.key,
                diagnostic.disposition,
                diagnostic.message,
            ),
        }
    }
}
