use std::{
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

use ala::{BoxShadow, GlassMaterial, Hsla};

use crate::{
    ADAPTIVE_CORNER_FRACTION, CORNER_SMOOTHING, Colorize as _, Elevation, Emphasis, Ground, Kind,
    Layer, PaintState, Size, Space, Surface, SurfaceRole, TextStyle, Theme, ThemeMode, Tone,
    components::button::rings_focus,
    components::icon::glyphs,
    components::input::field_layer,
    foundation::highlighter::HighlightTheme,
    foundation::interface_theme::{InterfaceTheme, builtin_themes},
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
    for role in roles::all() {
        write_role(&mut css, theme, &selector(role), Surface::new(role), "");
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
        let _ = writeln!(css, ":is({}) {{", containers.join(", "));
        for role in SurfaceRole::KIT
            .into_iter()
            .filter(|role| is_content(role.kind))
        {
            let surface = Surface::new(role).on(ground);
            let differs = STATES.iter().any(|state| {
                let plain = Surface::new(role);
                vars(theme, surface, &drawn(theme, surface, *state))
                    != vars(theme, plain, &drawn(theme, plain, *state))
            });
            if differs {
                write_role(&mut css, theme, &selector(role), surface, "  ");
            }
        }
        let _ = writeln!(css, "}}");
    }
    for role in SurfaceRole::KIT
        .into_iter()
        .chain(toned.iter().copied())
        .filter(|role| has_tones(role.kind))
    {
        for (tone, tone_name) in TONES {
            let toned = format!("{}[data-tone=\"{tone_name}\"]", selector(role));
            let surface = Surface::new(role).tone(tone);
            write_role(&mut css, theme, &toned, surface, "");
        }
        if role.kind == Kind::Button {
            let prominent = format!("{}[data-prominent]", selector(role));
            let surface = Surface::new(role).prominent(true);
            write_role(&mut css, theme, &prominent, surface, "");
            for (tone, tone_name) in TONES {
                let toned = format!("{prominent}[data-tone=\"{tone_name}\"]");
                let surface = Surface::new(role).tone(tone).prominent(true);
                write_role(&mut css, theme, &toned, surface, "");
            }
        }
    }
    css
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
    matches!(kind, Kind::Button | Kind::Label)
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

fn write_role(css: &mut String, theme: &Theme, base: &str, surface: Surface, indent: &str) {
    let declarations = |vars: &[&(&str, String)]| {
        vars.iter()
            .map(|(name, value)| format!("--penna-{name}: {value};"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let rest = vars(theme, surface, &drawn(theme, surface, PaintState::Rest));
    let set = rest
        .iter()
        .filter(|(name, value)| !PAINT.contains(&(name, value.as_str())))
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

fn vars(theme: &Theme, surface: Surface, layer: &Layer) -> Vec<(&'static str, String)> {
    let role = surface.role;
    let fill = layer.glass.map_or(layer.fill, |material| material.tint);
    let muted = theme
        .paint_on(layer.ground, Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest);
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
    let muted = theme
        .paint(Tone::Neutral, Emphasis::Ghost)
        .muted_at(PaintState::Rest);
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
        ("--penna-muted".to_owned(), color(muted)),
        ("--penna-accent".to_owned(), color(theme.accent)),
        ("--penna-on-accent".to_owned(), color(primary.text)),
        ("--penna-primary".to_owned(), color(primary.fill)),
        ("--penna-control".to_owned(), color(control)),
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
    let facing = light.alpha((light.a * material.specular).min(1.0));
    let far =
        light.alpha((light.a * material.specular * (1.0 - material.light_directionality)).min(1.0));
    let fresnel = light.alpha((light.a * material.fresnel).min(1.0));
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
    fn roles_an_app_names_get_tone_and_prominent_rules() {
        let theme = kit_theme(&builtin_themes()[0].1, ThemeMode::Dark);
        let toned = format!("{}[data-prominent][data-tone=\"danger\"]", selector(SEND));
        assert!(stylesheet(&theme, &[SEND]).contains(&toned));
        assert!(!stylesheet(&theme, &[]).contains(&toned));
    }
}
