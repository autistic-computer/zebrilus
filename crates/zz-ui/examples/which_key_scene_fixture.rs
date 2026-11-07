use std::{borrow::Cow, cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc, sync::Arc};

use ala::{
    AppContext as _, Context, HeadlessAppContext, IntoElement, Keystroke, ParentElement as _,
    Render, Styled as _, Window, div, px, size,
};
use ala_platform::{current_headless_renderer, wgpu::CosmicTextSystem};
use penna::{
    ActiveTheme as _, Root, ThemeMode,
    blocks::which_key::{WhichKeyCap, WhichKeyHeader, WhichKeyRow, WhichKeyView},
    foundation::{interface_theme::builtin_themes, theme_css::kit_theme},
};
use serde_json::{Value, json};
use zz_client::ChromeKey;
use zz_protocol::{
    CommandInvocation, Event, EventPayload, KeyBindingSnapshot, KeyTableSnapshot, KeyTables,
    ProtocolMessage, encode_protocol_message,
};

#[path = "terminal_styled_capture/mod.rs"]
mod scene_capture;

struct Fixture {
    header: WhichKeyHeader,
    rows: Arc<[WhichKeyRow]>,
    width: f32,
    height: f32,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background).child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(40.))
                .bottom(px(48.))
                .px(px(16.))
                .flex()
                .flex_col()
                .justify_end()
                .items_center()
                .child(
                    WhichKeyView::new(self.header.clone(), Arc::clone(&self.rows)).fit(size(
                        px((self.width - 32.).min(1360.)),
                        px(self.height - 16.),
                    )),
                ),
        )
    }
}

fn stroke(key: &str) -> Option<Keystroke> {
    Keystroke::parse(&zz_config::keymap::ala_source(&ChromeKey::parse(key)?)?).ok()
}

fn cap(keys: &[String], yours: bool) -> WhichKeyCap {
    let keys_parsed: Option<Vec<_>> = keys.iter().map(|key| stroke(key)).collect();
    WhichKeyCap {
        keys: keys_parsed.unwrap_or_default(),
        raw: keys.join(" ").into(),
        yours,
    }
}

fn binding(
    key: &str,
    name: &str,
    args: &[String],
    repeat: bool,
    note: Option<&str>,
) -> KeyBindingSnapshot {
    KeyBindingSnapshot {
        key: key.into(),
        commands: vec![CommandInvocation::new(name, args.iter().cloned())],
        repeat,
        note: note.map(str::to_owned),
    }
}

fn tables(variant: u8) -> Vec<KeyTableSnapshot> {
    if variant == 3 {
        return vec![KeyTableSnapshot {
            name: "custom".into(),
            bindings: (0..18)
                .map(|index| {
                    binding(
                        &format!("C-{index}"),
                        "unknown",
                        &[format!("argument-{index}")],
                        index % 2 == 0,
                        (index == 2).then_some(
                            "A very long command label is truncated at the column boundary",
                        ),
                    )
                })
                .collect(),
        }];
    }
    if variant == 4 {
        let mut bindings = Vec::new();
        for index in 0..10 {
            bindings.push(binding(
                &index.to_string(),
                "select-window",
                &["-t".into(), format!(":{index}")],
                false,
                None,
            ));
        }
        for keys in [
            ["[", "]", "{", "}"],
            ["Left", "Down", "Up", "Right"],
            ["C-a", "C-s", "C-w", "C-d"],
            ["C-1", "C-2", "C-3", "C-4"],
        ] {
            for (slot, key) in keys.into_iter().enumerate() {
                bindings.push(binding(
                    key,
                    "select-pane",
                    &[["-L", "-D", "-U", "-R"][slot].into()],
                    slot % 2 == 0,
                    None,
                ));
            }
        }
        for (key, name, note) in [
            ("M-bogus-long-name", "kill-pane", None),
            ("x", "new-window", Some("A custom new window label")),
            ("D-C-M-S-F12", "new-session", None),
            ("S-λ", "paste-buffer", None),
            ("C-bad-name", "unknown", None),
            ("MouseDown1Pane", "unknown", None),
            ("None", "clock-mode", None),
        ] {
            bindings.push(binding(key, name, &[], true, note));
        }
        return vec![KeyTableSnapshot {
            name: "prefix".into(),
            bindings,
        }];
    }
    let mut tables = KeyTables::default();
    if variant == 7 {
        tables.set_prefix("C-a");
    }
    tables.snapshot()
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let directory = PathBuf::from(args.get(1).ok_or("which-key output directory required")?);
    let theme: usize = args.get(2).ok_or("which-key theme required")?.parse()?;
    let scale: f32 = args.get(3).ok_or("which-key scale required")?.parse()?;
    let variant: u8 = args.get(4).ok_or("which-key variant required")?.parse()?;
    if theme >= 6 || ![1., 1.25, 2.].contains(&scale) || variant > 7 {
        return Err("invalid which-key fixture inputs".into());
    }
    let (width, height) = match variant {
        1 => (1280., 900.),
        2 => (420., 300.),
        3 => (640., 420.),
        4 => (760., 500.),
        6 => (640., 500.),
        7 => (480., 240.),
        _ => (720., 600.),
    };
    let prefix = match variant {
        5 => None,
        6 => Some("None"),
        7 => Some("C-a"),
        _ => Some("C-b"),
    };
    let table = if variant == 3 { "custom" } else { "prefix" };
    let all = variant != 0 && variant != 5;
    let short = table == "prefix" && !all;
    let tables = tables(variant);
    let rows: Vec<_> = zz_client::which_key::rows(&tables, table, prefix.unwrap_or("C-b"))
        .into_iter()
        .filter(|row| row.core || !short)
        .map(|row| WhichKeyRow {
            id: row.first_key().to_owned().into(),
            caps: row
                .keys
                .iter()
                .map(|set| cap(&set.keys, set.yours))
                .collect(),
            label: row.label.into(),
            group: row.group.map(|group| group.title().into()),
            repeat: row.repeat,
        })
        .collect();
    let more = short
        .then(|| tables.iter().find(|table| table.name == "prefix"))
        .flatten()
        .and_then(|table| {
            table
                .bindings
                .iter()
                .find(|binding| zz_client::which_key::opens_all_keys(&binding.commands))
        })
        .map(|binding| cap(std::slice::from_ref(&binding.key), false));
    let shown_prefix = if table == "prefix" {
        prefix.unwrap_or_default()
    } else {
        ""
    };
    let header = WhichKeyHeader {
        table: table.into(),
        prefix: stroke(shown_prefix),
        prefix_raw: shown_prefix.into(),
        more,
    };
    let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::KeyTablesChanged { tables },
    }))?;
    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Inter"));
    let scene = Rc::new(RefCell::new(Value::Null));
    let captured = scene.clone();
    let mut app = HeadlessAppContext::with_platform(text, Arc::new(penna::Assets), move || {
        let native = current_headless_renderer()?
            .ok_or_else(|| std::io::Error::other("native which-key renderer unavailable"))?;
        Ok(Some(Box::new(scene_capture::Capture {
            native,
            scene: captured.clone(),
            size: size(
                ala::DevicePixels((width * scale) as i32),
                ala::DevicePixels((height * scale) as i32),
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
    let window = app.open_window(size(px(width), px(height)), |window, cx| {
        window.set_scale_factor(scale);
        let fixture = cx.new(|_| Fixture {
            header,
            rows: rows.into(),
            width,
            height,
        });
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
    fs::write(
        directory.join("input.json"),
        serde_json::to_vec_pretty(
            &json!({ "width":width,"height":height,"theme":theme,"scale":scale,"variant":variant,"prefix":prefix,"table":table,"all":all,"frame":frame }),
        )?,
    )?;
    fs::write(
        directory.join("scene.json"),
        serde_json::to_vec_pretty(&*scene.borrow())?,
    )?;
    Ok(())
}
