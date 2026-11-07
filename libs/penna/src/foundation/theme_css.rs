use std::{
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

use ala::{BoxShadow, GlassMaterial, Hsla, px};

use crate::{
    ADAPTIVE_CORNER_FRACTION, CORNER_SMOOTHING, Colorize as _, Elevation, Emphasis, Ground, Kind,
    Layer, PaintState, Size, Space, Surface, SurfaceRole, TextStyle, Theme, ThemeMode, Tone,
    components::button::rings_focus,
    components::icon::glyphs,
    components::input::field_layer,
    foundation::highlighter::HighlightTheme,
    foundation::interface_theme::{Glass, InterfaceTheme, builtin_themes},
    foundation::roles,
    oklab_lightness, to_hex,
};

const STATES: [PaintState; 7] = [
    PaintState::Rest,
    PaintState::Hover,
    PaintState::Active,
    PaintState::Focused,
    PaintState::Current,
    PaintState::Selected,
    PaintState::Disabled,
];

const TONES: [(Tone, &str); 4] = [
    (Tone::Primary, "primary"),
    (Tone::Danger, "danger"),
    (Tone::Warning, "warning"),
    (Tone::Success, "success"),
];

const INKED: [(Tone, &str); 5] = [
    (Tone::Primary, "primary"),
    (Tone::Danger, "danger"),
    (Tone::Warning, "warning"),
    (Tone::Success, "success"),
    (Tone::Accent, "accent"),
];

const PAINT: [(&str, &str); 14] = [
    ("fill", "transparent"),
    ("edge", "transparent"),
    ("bw", "0px"),
    ("shadow", "0 0 #0000"),
    ("r", "0px"),
    ("k", "1"),
    ("style-k", "1"),
    ("row", "0"),
    ("mark", "transparent"),
    ("backdrop", "none"),
    ("rim", "0 0 #0000"),
    ("glass", "0"),
    ("focus", "transparent"),
    ("ring-gap", "0px"),
];

const ROW_BUTTONS: [SurfaceRole; 4] = [
    SurfaceRole::QUIET_BUTTON,
    SurfaceRole::ICON_BUTTON,
    SurfaceRole::FLAT_ICON_BUTTON,
    roles::ICON_BUTTON_ROW,
];

const ROW_RADIUS_SHARE: f32 = 0.4;

pub const SYNTAX: [(&str, &str); 15] = [
    ("keyword", "keyword"),
    ("type", "type"),
    ("constructor", "constructor"),
    ("function", "function"),
    ("comment", "comment"),
    ("string", "string"),
    ("number", "number"),
    ("boolean", "boolean"),
    ("property", "property"),
    ("constant", "constant"),
    ("title", "title"),
    ("literal", "text.literal"),
    ("tag", "tag"),
    ("attribute", "attribute"),
    ("bracket", "punctuation.bracket"),
];

pub fn kit_theme(theme: &InterfaceTheme, mode: ThemeMode) -> Theme {
    let theme = InterfaceTheme {
        glass: Some(theme.glass.clone().unwrap_or(Glass::Liquid)),
        ..theme.clone()
    };
    let mut kit = Theme::from(&theme.colors(mode));
    kit.mode = mode;
    kit.highlight_theme = if mode.is_dark() {
        HighlightTheme::default_dark()
    } else {
        HighlightTheme::default_light()
    };
    theme.apply(&mut kit);
    kit
}

pub fn stylesheet(theme: &Theme, toned: &[SurfaceRole]) -> String {
    let mut css = String::new();
    for (name, initial) in PAINT {
        let _ = writeln!(
            css,
            "@property --penna-{name} {{ syntax: \"*\"; inherits: false; initial-value: {initial}; }}"
        );
    }
    write_root(&mut css, theme);
    for (_, rule) in rules(theme, toned, false) {
        css.push_str(&rule);
    }
    let mut solid = theme.clone();
    solid.glass = None;
    if theme.glass.is_some() {
        let glassy = rules(theme, toned, true);
        let _ = writeln!(css, "@media (prefers-reduced-transparency: reduce) {{");
        for (key, rule) in rules(&solid, toned, true) {
            if !glassy.contains(&(key, rule.clone())) {
                css.push_str(&rule);
            }
        }
        let _ = writeln!(css, "}}");
    }
    let selections = solid_rules(theme, toned);
    css.push_str(&selections.concat());
    if theme.glass.is_some() {
        let reduced = solid_rules(&solid, toned)
            .into_iter()
            .filter(|rule| !selections.contains(rule))
            .collect::<String>();
        if !reduced.is_empty() {
            let _ = writeln!(
                css,
                "@media (prefers-reduced-transparency: reduce) {{\n{reduced}}}"
            );
        }
    }
    css
}

fn rules(theme: &Theme, toned: &[SurfaceRole], full: bool) -> Vec<(String, String)> {
    let mut rules = Vec::new();
    let mut rule = |key: String, surface: Surface, indent: &str| {
        let mut css = String::new();
        write_role(&mut css, theme, &key, surface, indent, full);
        rules.push((key, css));
    };
    for role in roles::all() {
        rule(selector(role), Surface::new(role), "");
    }
    let window = theme.ground();
    let mut grounds: Vec<(Ground, Vec<String>)> = Vec::new();
    for container in roles::all().filter(|role| is_container(role.kind)) {
        let ground = theme.ground_of(Surface::new(container));
        if ground == window {
            continue;
        }
        match grounds.iter_mut().find(|(seen, _)| *seen == ground) {
            Some((_, containers)) => containers.push(grounded(container)),
            None => grounds.push((ground, vec![grounded(container)])),
        }
    }
    for (ground, containers) in grounds {
        let scope = format!(":is({})", containers.join(", "));
        let mut css = format!("{scope} {{\n");
        write_inks(&mut css, theme, ground);
        for role in SurfaceRole::KIT
            .into_iter()
            .filter(|role| is_content(role.kind))
            .chain(roles::all().filter(|role| role.kind == Kind::Card))
        {
            let surface = Surface::new(role).on(ground);
            let differs = STATES.iter().any(|state| {
                let plain = Surface::new(role);
                vars(theme, surface, &drawn(theme, surface, *state))
                    != vars(theme, plain, &drawn(theme, plain, *state))
            });
            if differs {
                write_role(&mut css, theme, &selector(role), surface, "  ", full);
                if has_tones(role.kind) {
                    for (key, variant) in variants(role) {
                        write_over(&mut css, theme, &key, variant.on(ground), surface);
                    }
                }
            }
        }
        let _ = writeln!(css, "}}");
        rules.push((scope, css));
    }
    for container in roles::all().filter(|role| is_container(role.kind)) {
        let ground = theme.ground_of(Surface::new(container));
        let key = format!("[data-on=\"{}\"]", container.id);
        let inks = declared_inks(theme, ground);
        let css = format!(
            "{key} {{ --penna-ground: {};{}{inks} }}\n",
            color(ground.fill),
            if inks.is_empty() { "" } else { " " }
        );
        rules.push((key, css));
    }
    let mut gapped = theme.clone();
    gapped.pane_margin = gapped.pane_margin.max(px(1.0));
    let pane = gapped.layer(Surface::new(Kind::Pane.role()), PaintState::Rest);
    let key = ".penna-gaps [data-penna=\"pane\"]".to_owned();
    let css = format!(
        "{key} {{ --penna-rim: {}; --penna-edge: {}; --penna-bw: {}; }}\n",
        pane.glass
            .map_or_else(|| "0 0 #0000".to_owned(), |material| rim(&material)),
        color(pane.border),
        length(f32::from(pane.border_width))
    );
    rules.push((key, css));
    for role in SurfaceRole::KIT
        .into_iter()
        .chain(toned.iter().copied())
        .filter(|role| has_tones(role.kind))
        .chain(roles::FLOATING_CHIPS)
    {
        for (key, surface) in variants(role) {
            let mut css = String::new();
            let full = full || role.kind == Kind::Floating;
            write_role(&mut css, theme, &key, surface, "", full);
            rules.push((key, css));
        }
    }
    rules
}

fn variants(role: SurfaceRole) -> Vec<(String, Surface)> {
    let base = Surface::new(role);
    let toned = |key: &str, surface: Surface| {
        TONES.map(|(tone, name)| (format!("{key}[data-tone=\"{name}\"]"), surface.tone(tone)))
    };
    let mut variants = toned(&selector(role), base).to_vec();
    if role.kind == Kind::Button || role == SurfaceRole::BADGE {
        let prominent = format!("{}[data-prominent]", selector(role));
        variants.push((prominent.clone(), base.prominent(true)));
        variants.extend(toned(&prominent, base.prominent(true)));
    }
    variants
}

fn solid_rules(theme: &Theme, toned: &[SurfaceRole]) -> Vec<String> {
    let mut bodies: Vec<(String, Vec<String>)> = Vec::new();
    for (ground, scope) in solid_rows(theme, toned) {
        let mut inks = String::new();
        write_inks(&mut inks, theme, ground);
        let dot = theme
            .paint_on(ground, Tone::Neutral, Emphasis::Ghost)
            .muted_at(PaintState::Rest);
        let _ = writeln!(inks, "  --penna-dot: {};", color(dot));
        let mut looks = String::new();
        for role in SurfaceRole::KIT
            .into_iter()
            .filter(|role| role.kind == Kind::Label)
        {
            let surface = Surface::new(role).on(ground);
            write_role(&mut looks, theme, &selector(role), surface, "  ", false);
            for (key, variant) in variants(role) {
                write_over(&mut looks, theme, &key, variant.on(ground), surface);
            }
        }
        for role in ROW_BUTTONS {
            let key = format!(
                "{}:not([data-prominent], [data-penna^=\"field\"] *)",
                selector(role)
            );
            write_role(
                &mut looks,
                theme,
                &key,
                Surface::new(role).on(ground),
                "  ",
                true,
            );
        }
        for body in [inks, looks] {
            match bodies.iter_mut().find(|(seen, _)| *seen == body) {
                Some((_, scopes)) => scopes.push(scope.clone()),
                None => bodies.push((body, vec![scope.clone()])),
            }
        }
    }
    bodies
        .into_iter()
        .map(|(body, scopes)| format!("{} {{\n{body}}}\n", scopes.join(", ")))
        .collect()
}

fn solid_rows(theme: &Theme, toned: &[SurfaceRole]) -> Vec<(Ground, String)> {
    let tonable = SurfaceRole::KIT
        .into_iter()
        .chain(toned.iter().copied())
        .filter(|role| has_tones(role.kind))
        .collect::<Vec<_>>();
    let mut grounds: Vec<(Ground, Vec<(PaintState, Vec<String>)>)> = Vec::new();
    for state in [PaintState::Selected, PaintState::Current] {
        for role in roles::all() {
            let plain = Surface::new(role);
            let rows = if tonable.contains(&role) {
                let mut rows = vec![(plain, format!("{}:not([data-tone])", selector(role)))];
                rows.extend(TONES.map(|(tone, name)| {
                    (
                        plain.tone(tone),
                        format!("{}[data-tone=\"{name}\"]", selector(role)),
                    )
                }));
                rows
            } else {
                vec![(plain, selector(role))]
            };
            for (surface, row) in rows {
                let ground = theme.ground_at(surface, state);
                if !ground.solid {
                    continue;
                }
                let states = match grounds.iter_mut().find(|(seen, _)| *seen == ground) {
                    Some((_, states)) => states,
                    None => {
                        grounds.push((ground, Vec::new()));
                        &mut grounds.last_mut().expect("just pushed").1
                    }
                };
                match states.iter_mut().find(|(seen, _)| *seen == state) {
                    Some((_, rows)) => rows.push(row),
                    None => states.push((state, vec![row])),
                }
            }
        }
    }
    grounds
        .into_iter()
        .map(|(ground, states)| {
            let rows = states
                .iter()
                .map(|(state, rows)| {
                    format!(
                        ":is({}):is({})",
                        rows.join(", "),
                        state_selector(*state, Kind::Item).replace('&', "")
                    )
                })
                .collect::<Vec<_>>();
            (ground, format!(":is({})", rows.join(", ")))
        })
        .collect()
}

fn inks(theme: &Theme, ground: Ground) -> Vec<(String, String)> {
    let neutral = theme.paint_on(ground, Tone::Neutral, Emphasis::Ghost);
    let mut inks = vec![
        (
            "--penna-muted".to_owned(),
            color(neutral.muted_at(PaintState::Rest)),
        ),
        (
            "--penna-faint".to_owned(),
            color(neutral.faint_at(PaintState::Rest)),
        ),
    ];
    inks.extend(INKED.map(|(tone, name)| {
        let ink = theme.paint_on(ground, tone, Emphasis::Ghost).text;
        (format!("--penna-{name}-ink"), color(ink))
    }));
    inks
}

fn declared_inks(theme: &Theme, ground: Ground) -> String {
    let window = inks(theme, theme.ground());
    inks(theme, ground)
        .into_iter()
        .filter(|ink| !window.contains(ink))
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn write_inks(css: &mut String, theme: &Theme, ground: Ground) {
    let inks = declared_inks(theme, ground);
    if !inks.is_empty() {
        let _ = writeln!(css, "  {inks}");
    }
}

pub fn icons(theme: &Theme) -> String {
    let mut css = String::new();
    for name in glyphs::names() {
        if let Some(svg) = glyphs::svg(name, f32::from(theme.radius), CORNER_SMOOTHING) {
            let _ = writeln!(
                css,
                "[data-icon=\"{name}\"] {{ --penna-icon: url(\"data:image/svg+xml,{}\"); }}",
                data_uri(&svg)
            );
        }
    }
    css
}

pub fn write_tokens(dir: &Path, toned: &[SurfaceRole]) -> io::Result<Vec<PathBuf>> {
    fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    for (id, theme) in builtin_themes() {
        let path = dir.join(format!("{id}-icons.css"));
        fs::write(&path, icons(&kit_theme(theme, ThemeMode::Dark)))?;
        written.push(path);
        for mode in [ThemeMode::Light, ThemeMode::Dark] {
            let path = dir.join(format!("{id}-{}.css", mode.name()));
            fs::write(&path, stylesheet(&kit_theme(theme, mode), toned))?;
            written.push(path);
        }
    }
    Ok(written)
}

fn data_uri(svg: &str) -> String {
    svg.replace('"', "'")
        .replace('%', "%25")
        .replace('#', "%23")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('\n', " ")
}

fn drawn(theme: &Theme, surface: Surface, state: PaintState) -> Layer {
    match surface.role.kind {
        Kind::Field => field_layer(theme, surface, state, true),
        _ => theme.layer(surface, state),
    }
}

fn is_container(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Bar | Kind::Panel | Kind::Pane | Kind::Card | Kind::Floating
    )
}

fn is_content(kind: Kind) -> bool {
    matches!(kind, Kind::Button | Kind::Item | Kind::Field | Kind::Label)
}

fn has_tones(kind: Kind) -> bool {
    matches!(kind, Kind::Button | Kind::Field | Kind::Label)
}

fn selector(role: SurfaceRole) -> String {
    format!("[data-penna=\"{}\"]", role.id)
}

fn grounded(role: SurfaceRole) -> String {
    format!("{}[data-ground]", selector(role))
}

fn state_selector(state: PaintState, kind: Kind) -> &'static str {
    match state {
        PaintState::Focused if kind == Kind::Field => "&:focus-within, &[data-state~=\"focused\"]",
        PaintState::Rest => "&",
        PaintState::Hover => "&:hover, &[data-state~=\"hover\"]",
        PaintState::Active => "&:active, &[data-state~=\"active\"]",
        PaintState::Focused => "&[data-state~=\"focused\"]",
        PaintState::Current => {
            "&[data-state~=\"current\"], &[aria-current]:not([aria-current=\"false\"])"
        }
        PaintState::Selected => "&[data-state~=\"selected\"], &[aria-selected=\"true\"]",
        PaintState::Disabled => {
            "&:disabled, &[data-state~=\"disabled\"], &[aria-disabled=\"true\"]"
        }
    }
}

fn declarations(vars: &[&(&str, String)]) -> String {
    vars.iter()
        .map(|(name, value)| format!("--penna-{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn write_role(
    css: &mut String,
    theme: &Theme,
    base: &str,
    surface: Surface,
    indent: &str,
    full: bool,
) {
    let mut rest = vars(theme, surface, &drawn(theme, surface, PaintState::Rest));
    if full {
        let unset = PAINT
            .iter()
            .filter(|(name, _)| rest.iter().all(|(set, _)| set != name))
            .map(|(name, initial)| (*name, (*initial).to_owned()))
            .collect::<Vec<_>>();
        rest.extend(unset);
    }
    let set = rest
        .iter()
        .filter(|(name, value)| full || !PAINT.contains(&(name, value.as_str())))
        .collect::<Vec<_>>();
    let _ = writeln!(css, "{indent}{base} {{ {}", declarations(&set));
    for state in STATES.into_iter().skip(1) {
        let all = vars(theme, surface, &drawn(theme, surface, state));
        let changed = all
            .iter()
            .filter(|var| !rest.contains(var))
            .collect::<Vec<_>>();
        if !changed.is_empty() {
            let _ = writeln!(
                css,
                "{indent}  {} {{ {} }}",
                state_selector(state, surface.role.kind),
                declarations(&changed)
            );
        }
    }
    let _ = writeln!(css, "{indent}}}");
}

fn write_over(css: &mut String, theme: &Theme, base: &str, surface: Surface, under: Surface) {
    let at = |surface: Surface, state| vars(theme, surface, &drawn(theme, surface, state));
    let rest = at(surface, PaintState::Rest);
    let below = at(under, PaintState::Rest);
    let set = rest
        .iter()
        .filter(|var| !below.contains(var))
        .collect::<Vec<_>>();
    let states = STATES
        .into_iter()
        .skip(1)
        .filter_map(|state| {
            let below = at(under, state);
            let all = at(surface, state);
            let changed = all
                .iter()
                .filter(|var| {
                    if set.iter().any(|(name, _)| *name == var.0) {
                        !rest.contains(var)
                    } else {
                        !below.contains(var)
                    }
                })
                .collect::<Vec<_>>();
            (!changed.is_empty()).then(|| {
                format!(
                    "    {} {{ {} }}\n",
                    state_selector(state, surface.role.kind),
                    declarations(&changed)
                )
            })
        })
        .collect::<String>();
    if set.is_empty() && states.is_empty() {
        return;
    }
    let _ = writeln!(css, "  {base} {{ {}", declarations(&set));
    css.push_str(&states);
    let _ = writeln!(css, "  }}");
}

fn vars(theme: &Theme, surface: Surface, layer: &Layer) -> Vec<(&'static str, String)> {
    let role = surface.role;
    let fill = layer.glass.map_or(layer.fill, |material| material.tint);
    let muted = layer.muted_text();
    let mut vars = vec![
        ("fill", color(fill)),
        ("ink", color(layer.text)),
        ("muted", color(muted)),
        ("edge", color(layer.border)),
        ("bw", length(f32::from(layer.border_width))),
        ("shadow", shadows(&layer.shadow)),
        ("r", length(f32::from(layer.radius))),
        ("k", number(layer.smoothing.log2())),
        (
            "style-k",
            number(theme.style(role).smoothing.exponent().log2()),
        ),
        ("row", if role.row { "1" } else { "0" }.to_owned()),
        (
            "mark",
            layer
                .mark
                .map_or_else(|| "transparent".to_owned(), |mark| color(mark.color)),
        ),
    ];
    if rings_focus(role) {
        let ground = surface.ground.unwrap_or_else(|| theme.ground());
        let ring = theme.paint_on(ground, surface.tone, Emphasis::Ghost).focus;
        vars.push(("focus", color(ring)));
        vars.push((
            "ring-gap",
            length(f32::from(super::focus::ring_gap(fill, ring))),
        ));
    }
    if let Some(material) = layer.glass {
        let themed = theme.glass.as_ref();
        let (backdrop, rim) = (backdrop(&material), rim(&material));
        if themed.is_some_and(|glass| self::backdrop(glass) == backdrop) {
            vars.push(("backdrop", "var(--penna-glass-backdrop)".to_owned()));
        } else {
            vars.push(("backdrop", backdrop));
        }
        if themed.is_some_and(|glass| self::rim(glass) == rim) {
            vars.push(("rim", "var(--penna-glass-rim)".to_owned()));
        } else {
            vars.push(("rim", rim));
        }
        vars.push(("glass", "1".to_owned()));
    }
    vars
}

fn write_root(css: &mut String, theme: &Theme) {
    let primary = theme.paint(Tone::Primary, Emphasis::Solid);
    let control = theme
        .paint(Tone::Neutral, Emphasis::Soft)
        .fill_at(PaintState::Active);
    let knob = if oklab_lightness(theme.background) >= oklab_lightness(theme.foreground) {
        theme.background
    } else {
        theme.foreground
    };
    let thumb = theme.foreground.wash();
    let mono = theme.mono_font_family.as_ref();
    let font = theme.font_family.as_ref();
    let mut root = vec![
        ("color-scheme".to_owned(), theme.mode.name().to_owned()),
        ("--penna-bg".to_owned(), color(theme.background)),
        ("--penna-fg".to_owned(), color(theme.foreground)),
        (
            "--penna-fg-muted".to_owned(),
            color(theme.foreground.muted()),
        ),
        (
            "--penna-fg-faint".to_owned(),
            color(theme.foreground.faint()),
        ),
        ("--penna-accent".to_owned(), color(theme.accent)),
        ("--penna-on-accent".to_owned(), color(primary.text)),
        ("--penna-primary".to_owned(), color(primary.fill)),
        ("--penna-control".to_owned(), color(control)),
        ("--penna-wash".to_owned(), color(theme.background.washed(1))),
        ("--penna-knob".to_owned(), color(knob)),
        ("--penna-thumb".to_owned(), color(thumb)),
        ("--penna-success".to_owned(), color(theme.success)),
        ("--penna-warning".to_owned(), color(theme.warning)),
        ("--penna-danger".to_owned(), color(theme.danger)),
        ("--penna-border".to_owned(), color(theme.border())),
        ("--penna-divider".to_owned(), color(theme.divider_color())),
        ("--penna-scrim".to_owned(), color(theme.scrim)),
        (
            "--penna-thumb".to_owned(),
            color(crate::components::scroll::theme_thumb_color(theme)),
        ),
        (
            "--penna-panel".to_owned(),
            color(theme.surface(Elevation::Panel).fill),
        ),
        (
            "--penna-popover".to_owned(),
            color(theme.surface(Elevation::Popover).fill),
        ),
        ("--penna-radius".to_owned(), length(f32::from(theme.radius))),
        (
            "--penna-control-radius".to_owned(),
            length(f32::from(theme.control_radius())),
        ),
        (
            "--penna-window-k".to_owned(),
            number(theme.smoothing.exponent().log2()),
        ),
        (
            "--penna-focus-neutral".to_owned(),
            color(theme.paint(Tone::Neutral, Emphasis::Ghost).focus),
        ),
        (
            "--penna-knob-ring-gap".to_owned(),
            length(f32::from(super::focus::ring_gap(
                knob,
                theme.paint(Tone::Neutral, Emphasis::Ghost).focus,
            ))),
        ),
        (
            "--penna-pane-radius".to_owned(),
            length(f32::from(theme.pane_radius())),
        ),
        (
            "--penna-bar".to_owned(),
            length(f32::from(theme.pane_header_height())),
        ),
        (
            "--penna-outline".to_owned(),
            length(f32::from(theme.outline)),
        ),
        (
            "--penna-row-inset".to_owned(),
            length(f32::from(theme.row_inset)),
        ),
        ("--penna-row-share".to_owned(), number(ROW_RADIUS_SHARE)),
        (
            "--penna-corner-fraction".to_owned(),
            number(ADAPTIVE_CORNER_FRACTION),
        ),
        (
            "--penna-shadow-contact".to_owned(),
            shadows(&theme.contact_shadows()),
        ),
        (
            "--penna-shadow-float".to_owned(),
            shadows(&theme.float_shadows()),
        ),
        (
            "--penna-shadow-lifted".to_owned(),
            shadows(&theme.lifted_shadows()),
        ),
        ("--penna-font".to_owned(), font_stack(font, false)),
        ("--penna-mono".to_owned(), font_stack(mono, true)),
        (
            "--penna-font-size".to_owned(),
            length(f32::from(theme.font_size)),
        ),
    ];
    root.extend(inks(theme, theme.ground()));
    root.extend(paint_vars(theme));
    root.extend(scale_vars());
    root.extend(syntax_vars(theme));
    if let Some(glass) = theme.glass {
        root.extend(glass_vars(&glass));
    }
    let _ = writeln!(css, ":root {{");
    for (name, value) in root {
        let _ = writeln!(css, "  {name}: {value};");
    }
    let _ = writeln!(css, "}}");
}

fn paint_vars(theme: &Theme) -> Vec<(String, String)> {
    let outline = theme.paint(Tone::Neutral, Emphasis::Outline);
    let quiet = |state| {
        theme
            .layer(Surface::new(SurfaceRole::QUIET_BUTTON), state)
            .fill
    };
    let mut vars = vec![
        ("--penna-outline-hover".to_owned(), color(outline.hover)),
        ("--penna-outline-pressed".to_owned(), color(outline.active)),
        (
            "--penna-quiet-hover".to_owned(),
            color(quiet(PaintState::Hover)),
        ),
        (
            "--penna-quiet-pressed".to_owned(),
            color(quiet(PaintState::Active)),
        ),
    ];
    for (tone, name) in TONES {
        let solid = theme.paint(tone, Emphasis::Solid);
        vars.push((format!("--penna-{name}-hover"), color(solid.hover)));
        vars.push((format!("--penna-{name}-pressed"), color(solid.active)));
        vars.push((format!("--penna-on-{name}"), color(solid.text)));
    }
    vars
}

fn scale_vars() -> Vec<(String, String)> {
    let named = |prefix: &str, name: &str, space: Space| {
        (
            format!("--penna-{prefix}-{name}"),
            length(space.design_px()),
        )
    };
    let mut vars: Vec<(String, String)> = Space::ALL
        .iter()
        .map(|space| named("space", &space.design_px().to_string(), *space))
        .collect();
    vars.extend(
        Space::GAPS
            .iter()
            .map(|(name, space)| named("gap", name, *space)),
    );
    vars.extend(
        Space::INSETS
            .iter()
            .map(|(name, space)| named("inset", name, *space)),
    );
    for style in TextStyle::ALL {
        let name = style.name();
        vars.push((
            format!("--penna-text-{name}-size"),
            length(style.design_px()),
        ));
        vars.push((
            format!("--penna-text-{name}-line"),
            length(style.line_height_px()),
        ));
        vars.push((
            format!("--penna-text-{name}-weight"),
            number(style.weight().0),
        ));
    }
    for size in Size::LADDER {
        vars.push((
            format!("--penna-control-{}", size.name()),
            length(size.control_px()),
        ));
    }
    vars
}

fn syntax_vars(theme: &Theme) -> Vec<(String, String)> {
    SYNTAX
        .iter()
        .filter_map(|(name, capture)| {
            let ink = theme.highlight_theme.style(capture)?.color?;
            Some((format!("--penna-syntax-{name}"), color(ink)))
        })
        .collect()
}

fn glass_vars(material: &GlassMaterial) -> Vec<(String, String)> {
    let var = |name: &str| format!("--penna-glass-{name}");
    vec![
        (var("blur"), length(f32::from(material.blur))),
        (var("bezel"), length(f32::from(material.bezel))),
        (var("refraction"), length(f32::from(material.refraction))),
        (var("dispersion"), number(material.dispersion)),
        (var("tint"), color(material.tint)),
        (var("specular"), number(material.specular)),
        (var("glint"), length(f32::from(material.glint_width))),
        (
            var("light-angle"),
            format!("{}rad", number(material.light_angle)),
        ),
        (
            var("light-directionality"),
            number(material.light_directionality),
        ),
        (var("light-color"), color(material.light_color)),
        (var("fresnel"), number(material.fresnel)),
        (var("edge-shadow"), number(material.edge_shadow)),
        (var("edge-width"), length(f32::from(material.edge_width))),
        (var("edge-color"), color(material.edge_color)),
        (var("edge-spread"), number(material.edge_spread)),
        (var("backdrop"), backdrop(material)),
        (var("rim"), rim(material)),
    ]
}

fn backdrop(material: &GlassMaterial) -> String {
    format!(
        "blur({}) saturate({}) {}",
        length(f32::from(material.blur)),
        number(material.saturation),
        tone(material.contrast.max(0.0), material.brightness),
    )
}

fn tone(contrast: f32, brightness: f32) -> String {
    if brightness >= 0.0 {
        let lift = 1.0 + 2.0 * brightness;
        format!(
            "contrast({}) brightness({})",
            number(contrast / lift),
            number(lift)
        )
    } else {
        let slope = contrast - 2.0 * brightness;
        format!(
            "brightness({}) contrast({})",
            number(contrast / slope),
            number(slope)
        )
    }
}

fn rim(material: &GlassMaterial) -> String {
    let (x, y) = (material.light_angle.cos(), material.light_angle.sin());
    let glint = f32::from(material.glint_width);
    let edge = f32::from(material.edge_width);
    let bezel = f32::from(material.bezel);
    let light = material.light_color;
    let lit = if bezel > 0.0 { light.a } else { 0.0 };
    let facing = light.alpha((lit * material.specular).min(1.0));
    let far =
        light.alpha((lit * material.specular * (1.0 - material.light_directionality)).min(1.0));
    let fresnel = light.alpha((lit * material.fresnel).min(1.0));
    let dark = material.edge_color;
    let ring_alpha =
        material.edge_shadow * (material.edge_spread + (1.0 - material.edge_spread) * 0.35);
    let far_alpha = material.edge_shadow * (1.0 - material.edge_spread) * 0.65;
    [
        shadow(true, -x * glint, -y * glint, 0.0, 0.0, facing),
        shadow(true, x * glint, y * glint, 0.0, 0.0, far),
        shadow(true, 0.0, 0.0, bezel * 2.0, 0.0, fresnel),
        shadow(false, 0.0, 0.0, 0.0, edge, dark.alpha(dark.a * ring_alpha)),
        shadow(
            false,
            -x * edge,
            -y * edge,
            0.0,
            0.0,
            dark.alpha(dark.a * far_alpha),
        ),
    ]
    .join(", ")
}

fn shadows(shadows: &[BoxShadow]) -> String {
    if shadows.is_empty() {
        return "0 0 #0000".to_owned();
    }
    shadows
        .iter()
        .map(|shadow_| {
            shadow(
                shadow_.inset,
                f32::from(shadow_.offset.x),
                f32::from(shadow_.offset.y),
                f32::from(shadow_.blur_radius) * 2.0,
                f32::from(shadow_.spread_radius),
                shadow_.color,
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn shadow(inset: bool, x: f32, y: f32, blur: f32, spread: f32, ink: Hsla) -> String {
    format!(
        "{}{} {} {} {} {}",
        if inset { "inset " } else { "" },
        length(x),
        length(y),
        length(blur),
        length(spread),
        color(ink)
    )
}

fn font_stack(family: &str, mono: bool) -> String {
    let fallback = if mono {
        "ui-monospace, \"SF Mono\", Menlo, monospace"
    } else {
        "system-ui, -apple-system, \"Inter Variable\", Inter, sans-serif"
    };
    if family.starts_with('.') {
        fallback.to_owned()
    } else {
        format!("\"{family}\", {fallback}")
    }
}

fn color(color: Hsla) -> String {
    if color.a <= 0.0 {
        "transparent".to_owned()
    } else {
        to_hex(color)
    }
}

fn length(value: f32) -> String {
    if value.abs() >= 9999.0 {
        "9999px".to_owned()
    } else {
        format!("{}px", number(value))
    }
}

fn number(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded == 0.0 {
        "0".to_owned()
    } else {
        format!("{rounded}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEND: SurfaceRole =
        SurfaceRole::new("test.send", Kind::Button, "Send", "A button an app adds");

    #[test]
    fn every_role_and_state_gets_a_rule_in_every_builtin_theme() {
        for (id, theme) in builtin_themes() {
            for mode in [ThemeMode::Light, ThemeMode::Dark] {
                let css = stylesheet(&kit_theme(theme, mode), &[]);
                for role in roles::all() {
                    assert!(css.contains(&selector(role)), "{id} {mode:?} {}", role.id);
                }
                assert_eq!(css.matches('{').count(), css.matches('}').count());
                assert!(!css.contains("NaN"), "{id} {mode:?}");
            }
        }
    }

    #[test]
    fn glass_tone_adds_brightness_after_contrast_like_the_shader() {
        assert_eq!(tone(1.12, 0.07), "contrast(0.982) brightness(1.14)");
        assert_eq!(tone(1.0, -0.02), "brightness(0.962) contrast(1.04)");
        assert_eq!(tone(1.0, 0.0), "contrast(1) brightness(1)");
    }

    #[test]
    fn toned_and_prominent_controls_keep_their_paint_on_a_ground() {
        let theme = kit_theme(&builtin_themes()[0].1, ThemeMode::Light);
        let css = stylesheet(&theme, &[]);
        let start = css
            .find(":is([data-penna=\"panel\"][data-ground]")
            .expect("the panel's ground has a scope");
        let end = start + css[start..].find("\n}\n").expect("the scope closes");
        let scope = &css[start..end];
        assert!(scope.contains(
            "  [data-penna=\"badge\"][data-tone=\"danger\"] { --penna-fill: #cc27371f; --penna-ink: #942c2f;"
        ));
        assert!(scope.contains(
            "  [data-penna=\"button\"][data-prominent] { --penna-fill: #1c1c1a; --penna-ink: #ebebeb;"
        ));
    }

    #[test]
    fn a_ground_carries_the_inks_that_differ_from_the_windows() {
        let css = stylesheet(&kit_theme(&builtin_themes()[0].1, ThemeMode::Light), &[]);
        let start = css
            .find(":is([data-penna=\"panel\"][data-ground]")
            .expect("the panel's ground has a scope");
        let scope = &css[start..];
        let inks = &scope[scope.find('\n').expect("the scope opens")..];
        assert!(inks.starts_with("\n  --penna-muted: #525250; --penna-faint: #858582;\n"));
        assert!(css.contains(
            "[data-on=\"card\"] { --penna-ground: #e9e9e6; --penna-muted: #525250; --penna-faint: #858582; }"
        ));
        assert!(css.contains("[data-on=\"bar\"] { --penna-ground: #f6f6f3; }"));
    }

    #[test]
    fn a_solid_selection_sets_the_ground_inside_it_after_every_other_scope() {
        let (_, macos) = &builtin_themes()[1];
        let css = stylesheet(&kit_theme(macos, ThemeMode::Light), &[]);
        let media = css
            .find("@media (prefers-reduced-transparency: reduce)")
            .expect("glass has a reduced transparency block");
        let solid = &css[media..];
        let scope = |opening: &str| {
            let start = solid.find(opening).expect("the selection has a scope");
            let end = start + solid[start..].find("\n}\n").expect("the scope closes");
            &solid[start..end]
        };
        let tint = scope("\n:is(:is([data-penna=\"button.quiet\"]:not([data-tone])");
        assert!(!tint.contains("[data-penna=\"sidebar.row\"]"));
        let reduced = &css[css
            .rfind("@media (prefers-reduced-transparency: reduce)")
            .expect("glass has a reduced transparency block")..];
        assert!(reduced.contains("[data-penna=\"sidebar.row\"]"));
        assert!(reduced.contains("  --penna-dot: #ebebeb;"));
        assert!(tint.contains("[data-penna=\"menu-item\"][data-tone=\"primary\"]"));
        assert!(tint.contains("  --penna-muted: #ebebeb; --penna-faint: #79a4e8;"));
        assert!(tint.contains("--penna-success-ink: #ebebeb;"));
        assert!(tint.ends_with("\n  --penna-dot: #ebebeb;"));
        let danger = scope("\n:is(:is([data-penna=\"button.quiet\"][data-tone=\"danger\"]");
        assert!(danger.contains("[data-penna=\"menu-item\"][data-tone=\"danger\"]"));
        assert!(danger.contains("  --penna-muted: #ebebeb; --penna-faint: #e38a87;"));
        assert!(
            solid.contains(
                "  [data-penna=\"chip\"] { --penna-fill: #ffffff0a; --penna-ink: #ebebeb;"
            )
        );
        assert!(solid.contains(
            "  [data-penna=\"icon-button.row\"]:not([data-prominent], [data-penna^=\"field\"] *) { --penna-fill: transparent; --penna-ink: #ebebeb; --penna-muted: #ebebeb; --penna-edge: transparent; --penna-bw: 0px; --penna-shadow: 0 0 #0000; --penna-r: 9999px; --penna-k: 1; --penna-style-k: 2; --penna-row: 0; --penna-mark: transparent; --penna-focus: #0a64ddcc; --penna-ring-gap: 0px;"
        ));
        let ink = stylesheet(&kit_theme(&builtin_themes()[0].1, ThemeMode::Light), &[]);
        assert!(!ink.contains("[data-penna=\"menu-item\"]:not([data-tone])"));
        assert!(ink.contains("[data-penna=\"menu-item\"][data-tone=\"primary\"]"));
        let quiet =
            "  [data-penna=\"button.quiet\"]:not([data-prominent], [data-penna^=\"field\"] *) {";
        let start = ink
            .find(quiet)
            .expect("a toned row's selection repaints its quiet buttons");
        let rule = &ink[start..start + ink[start..].find('\n').expect("the rule ends")];
        assert!(rule.contains("--penna-ring-gap: 0px;"));
    }

    #[test]
    fn roles_an_app_names_get_tone_and_prominent_rules() {
        let theme = kit_theme(&builtin_themes()[0].1, ThemeMode::Dark);
        let toned = format!("{}[data-prominent][data-tone=\"danger\"]", selector(SEND));
        assert!(stylesheet(&theme, &[SEND]).contains(&toned));
        assert!(!stylesheet(&theme, &[]).contains(&toned));
    }
}
