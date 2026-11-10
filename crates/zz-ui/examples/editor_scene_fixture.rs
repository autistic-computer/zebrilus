use std::{borrow::Cow, cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc, sync::Arc};

use ala::{
    AppContext as _, Context, HeadlessAppContext, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px, size,
};
use ala_platform::{current_headless_renderer, wgpu::CosmicTextSystem};
use penna::{
    ActiveTheme as _, Label, Panel, PanelLook, Root, TextStyle, ThemeMode,
    blocks::feedback::{alert_actions, alert_ground, inline_alert},
    components::button::{Button, ButtonVariants as _},
    foundation::{interface_theme::builtin_themes, theme_css::kit_theme},
};
use serde_json::{Value, json};
use zz_client::editor_pane::{OPEN_HINT, RETRY_LABEL, Retry};

#[path = "terminal_styled_capture/mod.rs"]
mod scene_capture;

const PANE: u64 = 7;
const WIDTH: f32 = 720.;
const HEIGHT: f32 = 480.;

struct Fixture {
    failure: Option<(Retry, &'static str)>,
}

impl Render for Fixture {
    #[expect(
        clippy::disallowed_methods,
        reason = "the editor pane sets the code font and its own text size on its root"
    )]
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .id(("editor-pane", PANE))
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(px(13.0))
            .relative()
            .flex()
            .size_full()
            .overflow_hidden();
        let root = match &self.failure {
            None => root.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Label::new(TextStyle::Lead, OPEN_HINT).muted()),
            ),
            Some((retry, message)) => root.child(
                Panel::new(("editor-error", PANE), PanelLook::Plain)
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(12.0))
                    .child(
                        inline_alert(Some(retry.title().into()), *message, cx)
                            .max_w(px(360.0))
                            .child(
                                alert_actions(cx).child(
                                    Button::new(format!("editor-retry-{PANE}"))
                                        .primary()
                                        .on(alert_ground(cx))
                                        .label(RETRY_LABEL),
                                ),
                            ),
                    ),
            ),
        };
        div().size_full().bg(cx.theme().background).child(root)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let directory = PathBuf::from(args.get(1).ok_or("editor output directory required")?);
    let theme: usize = args.get(2).ok_or("editor theme required")?.parse()?;
    let scale: f32 = args.get(3).ok_or("editor scale required")?.parse()?;
    let variant: u8 = args.get(4).ok_or("editor variant required")?.parse()?;
    if theme >= 6 || ![1., 1.25, 2.].contains(&scale) || variant > 2 {
        return Err("invalid editor fixture inputs".into());
    }
    let failure = match variant {
        0 => None,
        1 => Some((
            Retry::Open(PathBuf::from("/work/missing.rs")),
            "No such file or directory (os error 2)",
        )),
        _ => Some((
            Retry::Save(PathBuf::from("/work/locked.rs")),
            "Permission denied (os error 13)",
        )),
    };
    let title = failure.as_ref().map(|(retry, _)| retry.title());
    let message = failure.as_ref().map(|(_, message)| *message);
    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Inter"));
    let scene = Rc::new(RefCell::new(Value::Null));
    let captured = scene.clone();
    let mut app = HeadlessAppContext::with_platform(text, Arc::new(penna::Assets), move || {
        let native = current_headless_renderer()?
            .ok_or_else(|| std::io::Error::other("native editor renderer unavailable"))?;
        Ok(Some(Box::new(scene_capture::Capture {
            native,
            scene: captured.clone(),
            size: size(
                ala::DevicePixels((WIDTH * scale) as i32),
                ala::DevicePixels((HEIGHT * scale) as i32),
            ),
        })))
    });
    let fonts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mjui/bench/assets/fonts");
    for name in [
        "Inter-Regular.ttf",
        "Inter-SemiBold.ttf",
        "JetBrainsMono-Regular.ttf",
    ] {
        app.text_system()
            .add_fonts(vec![Cow::Owned(fs::read(fonts.join(name))?)])?;
    }
    app.update(|cx| {
        penna::init(cx);
        let (_, source) = &builtin_themes()[theme / 2];
        let mut value = kit_theme(
            source,
            if theme.is_multiple_of(2) {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
        );
        value.font_family = "Inter".into();
        value.mono_font_family = "JetBrains Mono".into();
        value.font_size = px(penna::BASE_UI_FONT_SIZE);
        cx.set_global(value);
        cx.set_reduce_motion(true);
    });
    let window = app.open_window(size(px(WIDTH), px(HEIGHT)), |window, cx| {
        window.set_scale_factor(scale);
        let fixture = cx.new(|_| Fixture { failure });
        cx.new(|cx| {
            Root::new(fixture, window, cx)
                .bordered(false)
                .own_layers(false)
        })
    })?;
    app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
    fs::create_dir_all(&directory)?;
    app.capture_screenshot(window.into())?
        .save(directory.join("native.png"))?;
    let layouts = app
        .update_window(window.into(), |_, window, _| {
            window.layout_diagnostics(4096)
        })?
        .ok_or("editor layout diagnostic limit exceeded")?;
    fs::write(
        directory.join("layout.json"),
        serde_json::to_vec_pretty(
            &layouts
                .iter()
                .map(|layout| {
                    json!({
                        "id": layout.id, "parent": layout.parent,
                        "position": [layout.position.x, layout.position.y],
                        "size": [layout.size.width, layout.size.height],
                    })
                })
                .collect::<Vec<_>>(),
        )?,
    )?;
    fs::write(
        directory.join("input.json"),
        serde_json::to_vec_pretty(&json!({
            "width": WIDTH, "height": HEIGHT, "theme": theme, "scale": scale,
            "variant": variant, "hint": OPEN_HINT, "title": title, "message": message,
            "retry": RETRY_LABEL,
        }))?,
    )?;
    fs::write(
        directory.join("scene.json"),
        serde_json::to_vec_pretty(&*scene.borrow())?,
    )?;
    Ok(())
}
