use std::{borrow::Cow, cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc, sync::Arc};

use ala::{
    AppContext as _, Context, HeadlessAppContext, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div, px, size, uniform_list,
};
use ala_platform::{current_headless_renderer, wgpu::CosmicTextSystem};
use penna::{
    ActiveTheme as _, Colorize as _, IconName, PanelLook, Root, ThemeMode,
    components::button::{Button, ButtonLook, ButtonVariants as _},
    foundation::{interface_theme::builtin_themes, theme_css::kit_theme},
};
use serde_json::{Value, json};
use zz_client::chooser::{TREE_HINTS, chooser_subtitle, tree_row_text, tree_title};
use zz_protocol::{
    ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeSearchState, ChooseTreeState,
    ChooseTreeTarget, ClientId, Event, EventPayload, PaneId, ProtocolMessage, SessionId, WindowId,
    encode_protocol_message,
};
use zz_ui::chooser::{
    ChooserDimensions, ChooserModal, ChooserPaneKind, ChooserSearch, chooser_has_key_gutter,
    tree_chooser_row,
};

#[path = "terminal_styled_capture/mod.rs"]
mod scene_capture;

struct Fixture {
    state: ChooseTreeState,
}

impl Render for Fixture {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items: Arc<[ChooseTreeItem]> = self.state.items.clone().into();
        let gutter = chooser_has_key_gutter(items.iter().map(|item| item.key.as_str()));
        let count = items.len();
        let selected = self.state.selected;
        let rows = uniform_list(
            "chooser-fixture-rows",
            count,
            cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                range
                    .filter_map(|index| {
                        let item = items.get(index)?;
                        let (label, detail) = tree_row_text(item);
                        let pane_kind = item.pane_kind.map(|kind| match kind {
                            ChooseTreePaneKind::Terminal => ChooserPaneKind::Terminal,
                            ChooseTreePaneKind::Browser => ChooserPaneKind::Browser,
                            ChooseTreePaneKind::Editor => ChooserPaneKind::Editor,
                            ChooseTreePaneKind::Agent => ChooserPaneKind::Agent,
                        });
                        Some(tree_chooser_row(
                            "chooser-fixture-row",
                            index,
                            item.key.clone(),
                            gutter,
                            item.target.to_string(),
                            label,
                            detail,
                            item.depth,
                            if item.has_children() {
                                if item.expanded() { "▾" } else { "▸" }
                            } else {
                                ""
                            },
                            pane_kind,
                            item.active(),
                            item.tagged(),
                            u32::try_from(index).ok() == Some(selected),
                            "JetBrains Mono",
                        ))
                    })
                    .collect()
            }),
        )
        .flex_1();
        let ground = cx.theme().ground_of(PanelLook::Picker);
        let close = Button::compact_icon("chooser-fixture-close", IconName::Xmark)
            .ghost()
            .look(ButtonLook::Flat)
            .on(ground);
        let (title, targets) = tree_title(self.state.kind);
        let search = self.state.search.as_ref().map(|state| ChooserSearch {
            prefix: if state.reverse { "?" } else { "/" }.into(),
            value: state.query.clone().into(),
        });
        let modal = ChooserModal::new(
            "chooser-fixture-modal",
            title,
            chooser_subtitle(format!("{count} {targets}"), self.state.filter_no_matches),
            ChooserDimensions {
                max_width: 600.,
                row_count: count,
            },
            rows,
            close,
            "JetBrains Mono",
        )
        .prompt((!self.state.prompt.is_empty()).then(|| self.state.prompt.clone().into()))
        .help(self.state.help)
        .search(search)
        .hints(TREE_HINTS);
        div().size_full().bg(cx.theme().background).child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_start()
                .justify_center()
                .px(px(24.))
                .py(px(22.))
                .bg(cx.theme().scrim)
                .child(modal),
        )
    }
}

fn state(variant: u8) -> ChooseTreeState {
    let count = match variant {
        0 | 3 | 4 => 7,
        1 => 12,
        _ => 0,
    };
    ChooseTreeState {
        items: (0..count)
            .map(|index| {
                let target = if variant == 4 {
                    u64::MAX - index
                } else {
                    index
                };
                ChooseTreeItem {
                    label: format!("Project {index} — terminal"),
                    detail: format!("{index} windows · running"),
                    target: match index % 4 {
                        0 => ChooseTreeTarget::Session(SessionId(target)),
                        1 => ChooseTreeTarget::Window(WindowId(target)),
                        2 => ChooseTreeTarget::Pane(PaneId(target)),
                        _ => ChooseTreeTarget::Client(ClientId(target)),
                    },
                    depth: u8::try_from(index % 3).unwrap_or(0),
                    flags: u8::try_from(index).unwrap_or(0) | 2 | if index == 3 { 8 } else { 0 },
                    pane_kind: match index % 5 {
                        0 => None,
                        1 => Some(ChooseTreePaneKind::Terminal),
                        2 => Some(ChooseTreePaneKind::Browser),
                        3 => Some(ChooseTreePaneKind::Editor),
                        _ => Some(ChooseTreePaneKind::Agent),
                    },
                    key: if variant == 4 {
                        String::new()
                    } else if index == 0 {
                        "M-a".to_owned()
                    } else {
                        index.to_string()
                    },
                    text: if index == 5 {
                        "A custom row format replaces both original fields".to_owned()
                    } else {
                        String::new()
                    },
                }
            })
            .collect(),
        search: matches!(variant, 1 | 2).then(|| ChooseTreeSearchState {
            query: "project query".to_owned(),
            reverse: variant == 2,
        }),
        selected: if variant == 3 {
            3
        } else if variant == 4 {
            6
        } else {
            1
        },
        kind: if variant == 2 {
            ChooseTreeKind::Clients
        } else {
            ChooseTreeKind::Panes
        },
        filter_no_matches: variant == 2,
        prompt: if variant == 1 {
            "Press a target shortcut".to_owned()
        } else {
            String::new()
        },
        help: variant == 1,
    }
}

fn color_bits(value: ala::Hsla) -> [u32; 8] {
    let rgba: ala::Rgba = value.into();
    [
        rgba.r, rgba.g, rgba.b, rgba.a, value.h, value.s, value.l, value.a,
    ]
    .map(f32::to_bits)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let directory = PathBuf::from(args.get(1).ok_or("chooser output directory required")?);
    let theme: usize = args.get(2).ok_or("chooser theme required")?.parse()?;
    let scale: f32 = args.get(3).ok_or("chooser scale required")?.parse()?;
    let variant: u8 = args.get(4).ok_or("chooser variant required")?.parse()?;
    if theme >= 6 || ![1., 1.25, 2.].contains(&scale) || variant > 4 {
        return Err("invalid chooser fixture inputs".into());
    }
    let (width, height) = if variant == 1 {
        (480., 300.)
    } else if variant == 4 {
        (420., 600.)
    } else {
        (720., 600.)
    };
    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Inter"));
    let scene = Rc::new(RefCell::new(Value::Null));
    let captured = scene.clone();
    let mut app = HeadlessAppContext::with_platform(text, Arc::new(penna::Assets), move || {
        let native = current_headless_renderer()?
            .ok_or_else(|| std::io::Error::other("native chooser renderer unavailable"))?;
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
    let font_roles = [
        ala::FontWeight::NORMAL,
        ala::FontWeight::MEDIUM,
        ala::FontWeight::SEMIBOLD,
    ]
    .map(|weight| {
        let mut font = ala::font("Inter");
        font.weight = weight;
        app.text_system().resolve_font(&font).0
    });
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
    let colors = app.update(|cx| {
        [
            color_bits(cx.theme().scrim),
            color_bits(cx.theme().foreground.muted()),
        ]
    });
    let state = state(variant);
    let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload: EventPayload::ChooseTree {
            state: Some(state.clone()),
        },
    }))?;
    let window = app.open_window(size(px(width), px(height)), |window, cx| {
        window.set_scale_factor(scale);
        let fixture = cx.new(|_| Fixture { state });
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
        serde_json::to_vec_pretty(&json!({
            "width":width,"height":height,"theme":theme,"scale":scale,"variant":variant,
            "frame":frame,"font_directory":fonts,"font_roles":font_roles,"colors":colors,"reduced_motion":true,
        }))?,
    )?;
    fs::write(
        directory.join("scene.json"),
        serde_json::to_vec_pretty(&*scene.borrow())?,
    )?;
    Ok(())
}
