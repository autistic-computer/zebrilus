mod catalog;
mod host;

pub mod backdrop;
pub mod creator;
pub mod foundation;
pub mod kit;
pub mod knobs;
pub mod story;

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use ala::{AnyWindowHandle, App, AppContext as _, Task, WindowOptions};
use penna::{ActiveTheme as _, Colorize as _, Root, to_hex};
use serde_json::json;

pub use catalog::{Catalog, Control, Knob, PENNA, catalog, install};
pub use foundation::STORIES as FOUNDATION;
pub use kit::STORIES as KIT;
pub use knobs::Knobs;
pub use story::{Section, Story};

use host::SectionHost;

thread_local! {
    static KNOBS: RefCell<Knobs> = RefCell::new(Knobs::default());
    static OPEN: RefCell<Vec<AnyWindowHandle>> = const { RefCell::new(Vec::new()) };
    static DRAWN: RefCell<BTreeMap<&'static str, BTreeSet<&'static str>>> = const { RefCell::new(BTreeMap::new()) };
    static FLASH: RefCell<Option<Task<()>>> = const { RefCell::new(None) };
}

const FLASH_FRAME: Duration = Duration::from_millis(16);
const FLASH_HOLD: Duration = Duration::from_millis(700);
const FLASH_FADE: Duration = Duration::from_millis(500);

pub(crate) fn note_drawn(section: &'static str, roles: BTreeSet<&'static str>) {
    DRAWN.with_borrow_mut(|drawn| drawn.entry(section).or_default().extend(roles));
}

pub fn drawn_roles_json() -> String {
    DRAWN.with_borrow(|drawn| json!(drawn).to_string())
}

pub fn knobs() -> Knobs {
    KNOBS.with(|knobs| knobs.borrow().clone())
}

pub fn find(id: &str) -> Option<Story> {
    catalog().stories().find(|story| story.id == id)
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 1e4).round() / 1e4
}

pub fn catalog_json() -> String {
    let catalog = catalog();
    json!({
        "title": catalog.title,
        "name": catalog.name,
        "storage": catalog.storage,
        "knobs": catalog.knobs.iter().map(|knob| {
            let mut entry = json!({
                "name": knob.name,
                "label": knob.label,
                "section": knob.section,
            });
            match knob.control {
                Control::Check(on) => {
                    entry["kind"] = json!("check");
                    entry["value"] = json!(on);
                }
                Control::Range { min, max, step, value } => {
                    entry["kind"] = json!("range");
                    entry["min"] = json!(round(min));
                    entry["max"] = json!(round(max));
                    entry["step"] = json!(round(step));
                    entry["value"] = json!(round(value));
                }
            }
            entry
        }).collect::<Vec<_>>(),
    })
    .to_string()
}

pub fn stories_json() -> String {
    json!(
        catalog()
            .stories()
            .map(|story| json!({
                "id": story.id,
                "name": story.name,
                "group": story.group,
                "summary": story.summary,
                "sections": story.sections.iter().map(|section| json!({
                    "id": section.id,
                    "name": section.name,
                    "summary": section.summary,
                    "mount": mount_id(&story, section),
                })).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()
    )
    .to_string()
}

pub fn glass_presets_json() -> String {
    json!(
        ala::GlassMaterial::PRESETS
            .iter()
            .map(|(id, material)| {
                let material = material();
                let values = knobs::GLASS_KNOBS
                    .iter()
                    .map(|(name, get, _)| ((*name).to_owned(), json!(get(&material))))
                    .collect::<serde_json::Map<_, _>>();
                ((*id).to_owned(), serde_json::Value::Object(values))
            })
            .collect::<serde_json::Map<_, _>>()
    )
    .to_string()
}

pub fn interface_themes_json() -> String {
    json!(
        penna::interface_theme::builtin_themes()
            .iter()
            .map(|(id, theme)| {
                let mut values = knobs::theme_knobs(theme);
                values.insert("name".to_owned(), json!(theme.name));
                let glass = |material: Option<ala::GlassMaterial>| {
                    json!(material.map(|material| {
                        knobs::GLASS_KNOBS
                            .iter()
                            .map(|(name, get, _)| ((*name).to_owned(), json!(get(&material))))
                            .collect::<serde_json::Map<_, _>>()
                    }))
                };
                values.insert("glass".to_owned(), glass(theme.materials.glass));
                values.insert("light-glass".to_owned(), glass(theme.materials.light_glass));
                ((*id).to_owned(), serde_json::Value::Object(values))
            })
            .collect::<serde_json::Map<_, _>>()
    )
    .to_string()
}

pub fn theme_file_json() -> String {
    knobs().interface_theme().to_json()
}

pub fn roles_json() -> String {
    fn walk(group: penna::SurfaceRole, depth: usize, out: &mut Vec<(usize, penna::SurfaceRole)>) {
        out.push((depth, group));
        for member in penna::roles::members(group) {
            walk(member, depth + 1, out);
        }
    }
    let knobs = knobs();
    let roles = knobs.roles;
    let materials = knobs.materials;
    json!(
        penna::Kind::ALL
            .iter()
            .map(|kind| {
                let mut members = Vec::new();
                walk(kind.role(), 0, &mut members);
                json!({
                    "title": kind.role().label,
                    "roles": members.into_iter().map(|(depth, role)| {
                        let (inherited, origins) = knobs::inherited(role, &roles, &materials);
                        let patch = roles.get(role.id).copied().unwrap_or_default();
                        let mut style = inherited;
                        patch.apply(&mut style);
                        let values = knobs::style_json(inherited);
                        json!({
                            "id": role.id,
                            "label": role.label,
                            "about": role.about,
                            "kind": role.kind.name(),
                            "depth": depth,
                            "group": role.group().map(|group| group.label),
                            "members": penna::roles::members(role).count(),
                            "knobs": knobs::knobs_json(role.knobs()),
                            "patch": patch,
                            "inherited": knobs::PROPS
                                .iter()
                                .zip(origins)
                                .map(|(prop, origin)| ((*prop).to_owned(), json!({
                                    "value": values[prop],
                                    "from": origin.role.map_or_else(|| knobs::material_origin(prop, role, &materials), |role| role.label.to_owned()),
                                    "themed": origin.themed,
                                })))
                                .collect::<serde_json::Map<_, _>>(),
                            "style": knobs::style_json(style),
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect::<Vec<_>>()
    )
    .to_string()
}

pub fn inspect(id: &str, cx: &mut App) {
    let inspect = (!id.is_empty()).then(|| ala::SharedString::from(id.to_owned()));
    let flash = inspect.is_some().then(|| {
        cx.spawn(async move |cx| {
            let start = web_time::Instant::now();
            loop {
                cx.background_executor().timer(FLASH_FRAME).await;
                let faded = start.elapsed().saturating_sub(FLASH_HOLD);
                let strength = 1.0 - faded.as_secs_f32() / FLASH_FADE.as_secs_f32();
                cx.update(|cx| ring(strength.max(0.0), cx));
                if strength <= 0.0 {
                    break;
                }
            }
        })
    });
    penna::Theme::global_mut(cx).inspect = inspect;
    ring(1.0, cx);
    FLASH.with_borrow_mut(|current| *current = flash);
}

fn ring(strength: f32, cx: &mut App) {
    let theme = penna::Theme::global_mut(cx);
    theme.inspect_strength = strength;
    if strength <= 0.0 {
        theme.inspect = None;
    }
    for window in cx.windows() {
        window.update(cx, |_, window, _| window.refresh()).ok();
    }
}

pub fn import_theme(json: &str) -> Result<String, String> {
    let theme = penna::interface_theme::parse_theme(json)?;
    Ok(serde_json::Value::Object(knobs::theme_knobs(&theme)).to_string())
}

pub fn add_font(bytes: Vec<u8>, cx: &mut App) -> Result<Vec<String>, String> {
    let before = cx.text_system().all_font_names();
    cx.text_system()
        .add_fonts(vec![Cow::Owned(bytes)])
        .map_err(|error| format!("{error:#}"))?;
    Ok(cx
        .text_system()
        .all_font_names()
        .into_iter()
        .filter(|name| !before.contains(name))
        .collect())
}

pub fn fonts_json(cx: &App) -> String {
    let mut names = cx.text_system().all_font_names();
    names.sort();
    names.dedup();
    json!(names).to_string()
}

pub fn mount_id(story: &Story, section: &Section) -> String {
    format!("section-{}-{}", story.id, section.id)
}

pub fn show_story(story: Story, cx: &mut App) -> Result<(), String> {
    DRAWN.with_borrow_mut(BTreeMap::clear);
    for handle in OPEN.with(|open| std::mem::take(&mut *open.borrow_mut())) {
        handle
            .update(cx, |_, window, _| window.remove_window())
            .ok();
    }
    let knobs = knobs();
    for section in story.sections {
        let mount = format!("#{}", mount_id(&story, section));
        let handle = cx
            .open_window(
                WindowOptions {
                    mount: Some(mount.into()),
                    focus: false,
                    ..Default::default()
                },
                |window, cx| {
                    window.set_window_title(section.name);
                    window.set_zoom(knobs.zoom);
                    window.set_default_corner_smoothing(penna::CORNER_SMOOTHING);
                    window.set_adaptive_corner_fraction(Some(0.45));
                    let content = (section.build)(window, cx);
                    let floats = catalog().floats(story.id, section.id);
                    let host = cx.new(|_| SectionHost::new(section.id, content, floats));
                    cx.new(|cx| Root::new(host, window, cx).bordered(false))
                },
            )
            .map_err(|error| format!("{}: {error:#}", section.id))?;
        OPEN.with(|open| open.borrow_mut().push(handle.into()));
    }
    Ok(())
}

pub fn set_knobs(query: &str, cx: &mut App) -> Result<(), String> {
    let knobs = Knobs::parse(query)?;
    knobs.apply(cx);
    KNOBS.with(|current| *current.borrow_mut() = knobs);
    Ok(())
}

#[allow(
    clippy::disallowed_methods,
    reason = "the HTML knob panel paints itself from the kit's surface steps through CSS variables"
)]
pub fn theme_json(cx: &App) -> String {
    let theme = cx.theme();
    json!({
        "mode": theme.mode.name(),
        "background": to_hex(theme.background),
        "raised": to_hex(theme.background.raised(1)),
        "raised2": to_hex(theme.background.raised(2)),
        "raised3": to_hex(theme.background.raised(3)),
        "foreground": to_hex(theme.foreground),
        "muted": to_hex(theme.foreground.muted()),
        "border": to_hex(theme.border()),
        "accent": to_hex(theme.accent),
        "radius": f32::from(theme.radius),
    })
    .to_string()
}

#[cfg(target_family = "wasm")]
pub use web::start;

#[cfg(target_family = "wasm")]
mod web {
    use std::cell::{Cell, RefCell};

    use wasm_bindgen::prelude::*;

    use crate::catalog::{Catalog, catalog, install};

    fn launch(cx: &mut ala::App) {
        cx.text_system()
            .add_fonts(vec![
                super::Cow::Borrowed(include_bytes!("../fonts/inter/InterVariable.ttf")),
                super::Cow::Borrowed(include_bytes!("../fonts/inter/InterVariable-Italic.ttf")),
                super::Cow::Borrowed(include_bytes!("../fonts/lilex/Lilex-Regular.ttf")),
                super::Cow::Borrowed(include_bytes!("../fonts/lilex/Lilex-Bold.ttf")),
                super::Cow::Borrowed(include_bytes!("../fonts/lilex/Lilex-Italic.ttf")),
                super::Cow::Borrowed(include_bytes!("../fonts/lilex/Lilex-BoldItalic.ttf")),
            ])
            .expect("failed to load the interface fonts");
        (catalog().init)(cx);
        let theme = penna::Theme::global_mut(cx);
        theme.font_family = super::knobs::UI_FONT.into();
        theme.mono_font_family = "Lilex".into();
        super::KNOBS.with(|knobs| knobs.borrow().apply(cx));
    }

    thread_local! {
        static APPLICATION: RefCell<Option<ala::ApplicationHandle>> = const { RefCell::new(None) };
        static READY: Cell<bool> = const { Cell::new(false) };
    }

    fn with_app<R>(f: impl FnOnce(&mut ala::App) -> R) -> Result<R, JsValue> {
        APPLICATION.with(|application| {
            application
                .borrow()
                .as_ref()
                .map(|handle| handle.update(f))
                .ok_or_else(|| JsValue::from_str("the storybook is not running"))
        })
    }

    pub fn start(catalog: &'static Catalog, assets: impl ala::AssetSource) -> Result<(), JsValue> {
        if APPLICATION.with(|application| application.borrow().is_some()) {
            return Err(JsValue::from_str("the storybook is already running"));
        }
        install(catalog);
        ala_platform::web_init();
        let application = ala_platform::single_threaded_web().with_assets(assets);
        let handle = application.run_embedded(|cx| {
            launch(cx);
            READY.with(|ready| ready.set(true));
        });
        APPLICATION.with(|application| application.replace(Some(handle)));
        Ok(())
    }

    #[cfg(feature = "page")]
    #[wasm_bindgen]
    pub fn run() -> Result<(), JsValue> {
        start(&crate::PENNA, penna::Assets)
    }

    #[wasm_bindgen(js_name = catalog)]
    pub fn catalog_info() -> String {
        super::catalog_json()
    }

    #[wasm_bindgen]
    pub fn is_ready() -> bool {
        READY.with(Cell::get)
    }

    #[wasm_bindgen]
    pub fn stories() -> String {
        super::stories_json()
    }

    #[wasm_bindgen]
    pub fn glass_presets() -> String {
        super::glass_presets_json()
    }

    #[wasm_bindgen]
    pub fn interface_themes() -> String {
        super::interface_themes_json()
    }

    #[wasm_bindgen]
    pub fn theme_file() -> String {
        super::theme_file_json()
    }

    #[wasm_bindgen]
    pub fn roles() -> String {
        super::roles_json()
    }

    #[wasm_bindgen]
    pub fn drawn_roles() -> String {
        super::drawn_roles_json()
    }

    #[wasm_bindgen]
    pub fn inspect(id: &str) -> Result<(), JsValue> {
        with_app(|cx| super::inspect(id, cx))
    }

    #[wasm_bindgen]
    pub fn show(story: &str) -> Result<(), JsValue> {
        let story = super::find(story).ok_or_else(|| JsValue::from_str("no such story"))?;
        with_app(|cx| super::show_story(story, cx))?.map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen]
    pub fn set_knobs(query: &str) -> Result<(), JsValue> {
        with_app(|cx| super::set_knobs(query, cx))?.map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen]
    pub fn import_theme(json: &str) -> Result<String, JsValue> {
        super::import_theme(json).map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen]
    pub fn add_font(bytes: Vec<u8>) -> Result<String, JsValue> {
        with_app(|cx| super::add_font(bytes, cx))?
            .map(|names| serde_json::json!(names).to_string())
            .map_err(|error| JsValue::from_str(&error))
    }

    #[wasm_bindgen]
    pub fn fonts() -> Result<String, JsValue> {
        with_app(|cx| super::fonts_json(cx))
    }

    #[wasm_bindgen]
    pub fn theme() -> Result<String, JsValue> {
        with_app(|cx| super::theme_json(cx))
    }
}
