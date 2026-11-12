use std::{fmt::Write as _, io, path::PathBuf};

use ala::{
    AnyWindowHandle, App, AppContext as _, Bounds, ClipboardItem, Context, GlassMaterial, Global,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, px, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, Disableable as _, Root, StyledExt as _, TextStyle, ThemeMode,
    blocks::{
        appearance::value_slider,
        settings::{
            SettingEntry, SettingsStack, settings_card_ground, settings_reset_button,
            settings_scroll_column,
        },
    },
    components::{button::Button, slider::Slider},
    foundation::{
        glass::{GlassLook, LookField},
        interface_theme::liquid,
        tuning::{GlassTuning, MaterialField, set_glass_tuning},
    },
    h_flex,
};
use zz_protocol::app_identity::{DEVELOPMENT, DIRECTORY, platform_data_dir};

use crate::window::background::BackdropTuning;

const FILE_NAME: &str = "glass-lab.toml";
const WINDOW_SIZE: (f32, f32) = (500.0, 820.0);
const ROW_HEIGHT: f32 = 28.0;
const MODES: [ThemeMode; 2] = [ThemeMode::Light, ThemeMode::Dark];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct LabTuning {
    glass: GlassTuning,
    backdrop: BackdropTuning,
}

#[derive(Default)]
struct GlassLab {
    tuning: LabTuning,
    window: Option<AnyWindowHandle>,
}

impl Global for GlassLab {}

#[derive(Clone, Copy)]
struct Shared {
    key: &'static str,
    label: &'static str,
    range: (f32, f32),
    step: f32,
    get: fn(&LabTuning) -> f32,
    set: fn(&mut LabTuning, f32),
}

impl Shared {
    fn changed(self, tuning: &LabTuning) -> Option<f32> {
        let value = (self.get)(tuning);
        (value != (self.get)(&LabTuning::default())).then_some(value)
    }
}

#[derive(Clone, Copy)]
enum Knob {
    Look(LookField),
    Shared(Shared),
}

macro_rules! shared {
    ($key:literal, $label:literal, $range:expr, $step:expr, $($field:tt)+) => {
        Knob::Shared(Shared {
            key: $key,
            label: $label,
            range: $range,
            step: $step,
            get: |t| t.$($field)+,
            set: |t, v| t.$($field)+ = v,
        })
    };
}

const SHARED: [Knob; 9] = [
    shared!(
        "plane.raise_step",
        "Panel wash step",
        (0.0, 0.2),
        0.005,
        glass.raise_step
    ),
    shared!(
        "plane.veil_light",
        "Inactive pane veil, light",
        (0.0, 0.6),
        0.01,
        glass.veil[0]
    ),
    shared!(
        "plane.veil_dark",
        "Inactive pane veil, dark",
        (0.0, 0.6),
        0.01,
        glass.veil[1]
    ),
    shared!(
        "plane.edge_weight",
        "Border weight",
        (0.0, 0.5),
        0.01,
        glass.edge_weight
    ),
    shared!(
        "plane.divider_weight",
        "Divider weight",
        (0.0, 0.5),
        0.01,
        glass.divider_weight
    ),
    shared!(
        "floating.text_floor",
        "Text contrast floor",
        (1.0, 7.0),
        0.1,
        glass.text_floor
    ),
    shared!(
        "rim.inactive_dim",
        "Inactive rim dim",
        (0.0, 1.0),
        0.01,
        glass.rim_dim
    ),
    shared!(
        "backdrop.scale",
        "Backdrop sample scale",
        (0.05, 1.0),
        0.005,
        backdrop.scale
    ),
    shared!(
        "backdrop.fallback_blur",
        "Fallback blur radius",
        (0.0, 400.0),
        1.0,
        backdrop.fallback_blur
    ),
];

const BACKDROP: [LookField; 8] = [
    LookField::ChromeOpacity,
    LookField::WindowOpacity,
    LookField::Blur,
    LookField::Saturation,
    LookField::Contrast,
    LookField::Brightness,
    LookField::Cap,
    LookField::WallpaperTint,
];

const RIM_FIELDS: [MaterialField; 13] = [
    MaterialField::Bezel,
    MaterialField::Specular,
    MaterialField::GlintWidth,
    MaterialField::GlintSpread,
    MaterialField::Fresnel,
    MaterialField::LightAngle,
    MaterialField::LightDirectionality,
    MaterialField::LightAlpha,
    MaterialField::InactiveFlattening,
    MaterialField::EdgeShadow,
    MaterialField::EdgeWidth,
    MaterialField::EdgeSpread,
    MaterialField::Opacity,
];

const ANGLE_STEP: f32 = std::f32::consts::PI / 72.0;

fn material_spec(field: MaterialField) -> (&'static str, (f32, f32), f32) {
    use MaterialField as F;
    match field {
        F::Blur => ("Blur", (0.0, 80.0), 0.5),
        F::Bezel => ("Bezel", (0.0, 40.0), 0.5),
        F::Refraction => ("Refraction", (0.0, 100.0), 1.0),
        F::Dispersion => ("Dispersion", (0.0, 0.3), 0.005),
        F::TintAlpha => ("Tint alpha", (0.0, 1.0), 0.01),
        F::TintLightness => ("Tint lightness", (0.0, 1.0), 0.01),
        F::Saturation => ("Saturation", (0.0, 3.0), 0.05),
        F::Brightness => ("Brightness", (-0.3, 0.3), 0.01),
        F::Contrast => ("Contrast", (0.0, 2.0), 0.02),
        F::Specular => ("Glint strength", (0.0, 1.5), 0.01),
        F::GlintWidth => ("Glint width", (0.0, 4.0), 0.05),
        F::GlintSpread => ("Glint evenness", (0.0, 1.0), 0.01),
        F::LightAngle => (
            "Light angle",
            (-std::f32::consts::PI, std::f32::consts::PI),
            ANGLE_STEP,
        ),
        F::LightDirectionality => ("Light directionality", (0.0, 1.0), 0.01),
        F::InactiveFlattening => ("Inactive flattening", (0.0, 1.0), 0.01),
        F::LightAlpha => ("Light alpha", (0.0, 1.0), 0.01),
        F::Fresnel => ("Fresnel", (0.0, 0.3), 0.005),
        F::EdgeShadow => ("Edge shadow", (0.0, 1.0), 0.01),
        F::EdgeWidth => ("Edge width", (0.0, 4.0), 0.05),
        F::EdgeAlpha => ("Edge alpha", (0.0, 1.0), 0.01),
        F::EdgeSpread => ("Edge spread", (0.0, 1.0), 0.01),
        F::Noise => ("Noise", (0.0, 0.2), 0.005),
        F::Glow => ("Glow", (0.0, 1.0), 0.01),
        F::GlowRadius => ("Glow radius", (0.0, 300.0), 1.0),
        F::GlowX => ("Glow center x", (0.0, 1.0), 0.01),
        F::GlowY => ("Glow center y", (0.0, 1.0), 0.01),
        F::Merge => ("Merge", (0.0, 60.0), 1.0),
        F::Opacity => ("Opacity", (0.0, 1.0), 0.01),
    }
}

fn look_spec(field: LookField) -> (&'static str, (f32, f32), f32) {
    match field {
        LookField::ChromeOpacity => ("Chrome opacity", (0.0, 1.0), 0.01),
        LookField::WindowOpacity => ("Opacity without backdrop", (0.0, 1.0), 0.01),
        LookField::Blur => ("Blur radius", (0.0, 200.0), 1.0),
        LookField::Saturation => ("Saturation", (0.0, 4.0), 0.05),
        LookField::Contrast => ("Contrast", (0.0, 2.0), 0.01),
        LookField::Brightness => ("Brightness", (0.0, 2.0), 0.01),
        LookField::Cap => ("Brightness cap", (0.0, 1.0), 0.01),
        LookField::WallpaperTint => ("Wallpaper hue amount", (0.0, 1.0), 0.01),
        LookField::ContentPlate => ("Content plate", (0.0, 1.0), 0.01),
        LookField::Floating(field) | LookField::Rim(field) => material_spec(field),
    }
}

fn mode_name(mode: ThemeMode) -> &'static str {
    if mode.is_dark() { "dark" } else { "light" }
}

fn groups(mode: ThemeMode) -> Vec<(String, Vec<Knob>)> {
    let mode = mode_name(mode);
    let look = |fields: &mut dyn Iterator<Item = LookField>| fields.map(Knob::Look).collect();
    vec![
        (format!("Backdrop, {mode}"), look(&mut BACKDROP.into_iter())),
        (
            format!("Planes, {mode}"),
            look(&mut std::iter::once(LookField::ContentPlate)),
        ),
        (
            format!("Floating material, {mode}"),
            look(&mut MaterialField::ALL.into_iter().map(LookField::Floating)),
        ),
        (
            format!("Pane rim, {mode}"),
            look(&mut RIM_FIELDS.into_iter().map(LookField::Rim)),
        ),
        ("Shared".to_owned(), SHARED.to_vec()),
    ]
}

impl Knob {
    fn label(self) -> &'static str {
        match self {
            Self::Look(field) => look_spec(field).0,
            Self::Shared(shared) => shared.label,
        }
    }

    fn scale(self) -> ((f32, f32), f32) {
        match self {
            Self::Look(field) => {
                let (_, range, step) = look_spec(field);
                (range, step)
            }
            Self::Shared(shared) => (shared.range, shared.step),
        }
    }

    fn changed(self, tuning: &LabTuning, mode: ThemeMode) -> bool {
        match self {
            Self::Look(field) => {
                field.get(&lab_look(tuning, mode)) != field.get(&GlassLook::shipped(mode))
            }
            Self::Shared(shared) => shared.changed(tuning).is_some(),
        }
    }

    fn value(self, tuning: &LabTuning, mode: ThemeMode, base: &GlassMaterial) -> f32 {
        let look = lab_look(tuning, mode);
        match self {
            Self::Shared(shared) => (shared.get)(tuning),
            Self::Look(field) => field.get(&look).unwrap_or_else(|| match field {
                LookField::Floating(field) => field.get(base),
                LookField::Rim(MaterialField::EdgeShadow) => 0.0,
                LookField::Rim(field) => field.get(&look.floating.apply(*base)),
                _ => 0.0,
            }),
        }
    }

    fn set(self, tuning: &mut LabTuning, mode: ThemeMode, value: Option<f32>) {
        match self {
            Self::Shared(shared) => (shared.set)(
                tuning,
                value.unwrap_or_else(|| (shared.get)(&LabTuning::default())),
            ),
            Self::Look(field) => field.set(
                tuning.glass.look_mut(mode),
                value.or_else(|| field.get(&GlassLook::shipped(mode))),
            ),
        }
    }

    fn shown(self, value: f32) -> SharedString {
        if let Self::Look(
            LookField::Floating(MaterialField::LightAngle)
            | LookField::Rim(MaterialField::LightAngle),
        ) = self
        {
            return format!("{:.1}°", value.to_degrees()).into();
        }
        let (_, step) = self.scale();
        let decimals = if step >= 1.0 {
            0
        } else if step >= 0.1 {
            1
        } else if step >= 0.01 {
            2
        } else {
            3
        };
        format!("{value:.decimals$}").into()
    }
}

fn lab_look(tuning: &LabTuning, mode: ThemeMode) -> GlassLook {
    tuning.glass.looks[usize::from(mode.is_dark())]
}

fn to_text(tuning: &LabTuning) -> String {
    let mut text = String::new();
    for mode in MODES {
        let (look, shipped) = (lab_look(tuning, mode), GlassLook::shipped(mode));
        for field in LookField::every() {
            let value = field.get(&look);
            if value != field.get(&shipped)
                && let Some(value) = value
            {
                writeln!(text, "{}.{} = {value}", mode_name(mode), field.key()).ok();
            }
        }
    }
    for knob in SHARED {
        if let Knob::Shared(shared) = knob
            && let Some(value) = shared.changed(tuning)
        {
            writeln!(text, "{} = {value}", shared.key).ok();
        }
    }
    text
}

fn from_text(text: &str) -> LabTuning {
    let mut tuning = LabTuning::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let Some(value) = value.parse::<f32>().ok().filter(|value| value.is_finite()) else {
            log::warn!(target: "zz::glass_lab", "ignoring glass lab line {line:?}");
            continue;
        };
        let look = key.split_once('.').and_then(|(named, field)| {
            let mode = MODES.into_iter().find(|mode| mode_name(*mode) == named)?;
            Some((mode, LookField::parse(field)?))
        });
        let shared = SHARED.into_iter().find(|knob| match knob {
            Knob::Shared(shared) => shared.key == key,
            Knob::Look(_) => false,
        });
        match (look, shared) {
            (Some((mode, field)), _) => Knob::Look(field).set(&mut tuning, mode, Some(value)),
            (None, Some(Knob::Shared(shared))) => (shared.set)(&mut tuning, value),
            (None, _) => log::warn!(target: "zz::glass_lab", "ignoring glass lab line {line:?}"),
        }
    }
    tuning
}

fn saved_path() -> io::Result<PathBuf> {
    let data = platform_data_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "could not resolve the current user's application-data directory",
        )
    })?;
    Ok(data.join(DIRECTORY).join(FILE_NAME))
}

pub(crate) fn init(cx: &mut App) {
    if !DEVELOPMENT {
        return;
    }
    let tuning = saved_path()
        .and_then(std::fs::read_to_string)
        .map(|text| from_text(&text))
        .unwrap_or_default();
    set_glass_tuning(tuning.glass);
    cx.set_global(tuning.backdrop);
    cx.set_global(GlassLab {
        tuning,
        window: None,
    });
}

pub(crate) fn opens_at_launch() -> bool {
    DEVELOPMENT && std::env::var_os("ZZ_GLASS_LAB").is_some_and(|value| value == "1")
}

pub(crate) fn available() -> bool {
    DEVELOPMENT
}

pub(crate) fn app_windows(cx: &App) -> Vec<AnyWindowHandle> {
    let lab = cx.try_global::<GlassLab>().and_then(|lab| lab.window);
    cx.windows()
        .into_iter()
        .filter(|window| lab.is_none_or(|lab| lab.window_id() != window.window_id()))
        .collect()
}

pub(crate) fn is_open(cx: &App) -> bool {
    cx.try_global::<GlassLab>()
        .is_some_and(|lab| lab.window.is_some())
}

pub(crate) fn set_open(open: bool, cx: &mut App) {
    let Some(window) = cx.try_global::<GlassLab>().map(|lab| lab.window) else {
        return;
    };
    match (window, open) {
        (Some(window), false) => {
            window
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        }
        (None, true) => open_window(cx),
        (Some(_), true) | (None, false) => {}
    }
}

fn open_window(cx: &mut App) {
    let (width, height) = WINDOW_SIZE;
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(width), px(height)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Glass lab".into()),
                ..Default::default()
            }),
            window_background: WindowBackgroundAppearance::Opaque,
            kind: WindowKind::Floating,
            focus: false,
            app_id: Some(DIRECTORY.into()),
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|_| GlassLabView);
            cx.new(|cx| Root::new(view, window, cx))
        },
    );
    match opened {
        Ok(handle) => {
            if cx.has_global::<GlassLab>() {
                cx.global_mut::<GlassLab>().window = Some(handle.into());
            }
        }
        Err(error) => log::warn!(target: "zz::glass_lab", "could not open the glass lab: {error}"),
    }
}

pub(crate) fn forget_closed(cx: &mut App) {
    let Some(window) = cx.try_global::<GlassLab>().and_then(|lab| lab.window) else {
        return;
    };
    if !cx
        .windows()
        .iter()
        .any(|open| open.window_id() == window.window_id())
    {
        cx.global_mut::<GlassLab>().window = None;
    }
}

fn change(cx: &mut App, edit: impl FnOnce(&mut LabTuning)) {
    if !cx.has_global::<GlassLab>() {
        return;
    }
    let lab = cx.global_mut::<GlassLab>();
    let before = lab.tuning;
    edit(&mut lab.tuning);
    let tuning = lab.tuning;
    if tuning == before {
        return;
    }
    set_glass_tuning(tuning.glass);
    cx.set_global(tuning.backdrop);
    crate::config::apply_window_background_appearance(cx);
    crate::theme::refresh_current_theme(cx);
    save(&tuning, cx);
}

fn save(tuning: &LabTuning, cx: &App) {
    let path = match saved_path() {
        Ok(path) => path,
        Err(error) => {
            log::warn!(target: "zz::glass_lab", "glass lab values are not saved: {error}");
            return;
        }
    };
    let text = to_text(tuning);
    cx.background_executor()
        .spawn(async move {
            let written = if text.is_empty() {
                match std::fs::remove_file(&path) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    other => other,
                }
            } else {
                path.parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, text))
            };
            if let Err(error) = written {
                log::warn!(target: "zz::glass_lab", "saving glass lab values failed: {error}");
            }
        })
        .detach();
}

struct GlassLabView;

impl Render for GlassLabView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mode = theme.mode;
        let tuning = cx
            .try_global::<GlassLab>()
            .map(|lab| lab.tuning)
            .unwrap_or_default();
        let base = crate::config::shown_theme(cx)
            .glass(mode)
            .unwrap_or_else(|| liquid(mode));
        let ground = settings_card_ground(cx);
        let changed = !to_text(&tuning).is_empty();
        let mut index = 0_usize;
        let stacks = groups(mode).into_iter().map(|(title, knobs)| {
            SettingsStack::new().child(SettingEntry::new(title).children(knobs.into_iter().map(
                |knob| {
                    index += 1;
                    knob_row(index, knob, &tuning, mode, &base, ground, cx)
                },
            )))
        });
        let lines = lab_look(&tuning, mode).lines(mode);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background.opaque())
            .text_color(theme.foreground)
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .pt(px(12.0))
                    .child(div().flex_1())
                    .child(
                        Button::new("glass-lab-copy")
                            .label("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(lines.clone()));
                            }),
                    )
                    .child(
                        Button::new("glass-lab-reset")
                            .label("Reset")
                            .disabled(!changed)
                            .on_click(|_, _, cx| {
                                change(cx, |tuning| *tuning = LabTuning::default());
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(settings_scroll_column("glass-lab-knobs").children(stacks)),
            )
    }
}

fn knob_row(
    index: usize,
    knob: Knob,
    tuning: &LabTuning,
    mode: ThemeMode,
    base: &GlassMaterial,
    ground: penna::Ground,
    cx: &App,
) -> ala::Div {
    let ((min, max), step) = knob.scale();
    let value = knob.value(tuning, mode, base);
    let slider = Slider::new(
        ("glass-lab-slider", index),
        knob.label(),
        value.clamp(min, max),
        min..=max,
        move |value, _, cx| change(cx, |tuning| knob.set(tuning, mode, Some(value))),
    )
    .step(step)
    .display(move |value| knob.shown(value));
    h_flex()
        .h(px(ROW_HEIGHT))
        .gap(px(8.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .type_style(TextStyle::Body, cx)
                .child(knob.label()),
        )
        .child(value_slider(slider, cx))
        .child(
            settings_reset_button(
                ("glass-lab-reset", index),
                "Reset",
                knob.changed(tuning, mode),
            )
            .on(ground)
            .on_click(move |_, _, cx| change(cx, |tuning| knob.set(tuning, mode, None))),
        )
}
