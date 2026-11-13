use std::{borrow::Cow, cell::RefCell, error::Error, fs, path::PathBuf, rc::Rc, sync::Arc};

use ala::{
    AppContext as _, Context, Entity, HeadlessAppContext, IntoElement, Render, Window, div,
    prelude::*, px, size,
};
use ala_platform::{current_headless_renderer, wgpu::CosmicTextSystem};
use penna::{
    ActiveTheme as _, IconName, Root, ThemeMode,
    blocks::feedback::alert_ground,
    components::button::{Button, ButtonVariants as _},
    components::input::InputState,
    foundation::{interface_theme::builtin_themes, theme_css::kit_theme},
};
use serde_json::{Value, json};
use zz_protocol::PaneId;
use zz_ui::{
    browser::{
        BrowserEmptyHint, BrowserErrorPanel, BrowserHeader, BrowserPickStatus, BrowserTabInfo,
        BrowserTabStrip, BrowserToolbar, browser_address, browser_recent_row,
        browser_site_controls_button, browser_start_surface, browser_toolbar_button,
    },
    pane::{PaneDragSource, PaneState, pane_header_actions, pane_header_icon_button},
};

#[path = "terminal_styled_capture/mod.rs"]
mod scene_capture;

const WIDTH: f32 = 720.;
const HEIGHT: f32 = 480.;
const NAMES: [&str; 6] = ["page", "tabs", "empty", "recents", "error", "picking"];
const RECENTS: [&str; 3] = [
    "example.com/docs",
    "autistic.computer",
    "localhost:8080/admin",
];
const ERROR: &str = "net::ERR_NAME_NOT_RESOLVED";
const PICK: &str = "Select an element · Esc to cancel";

struct Case {
    tabs: &'static [(&'static str, &'static str)],
    active: usize,
    focused: bool,
    lone: bool,
    inset: f32,
    address: &'static str,
    back: bool,
    forward: bool,
}

fn case(variant: u8) -> Case {
    match variant {
        1 => Case {
            tabs: &[
                ("example.com", "Example Domain"),
                ("Rust Programming Language", "https://www.rust-lang.org/"),
                ("localhost", "http://localhost:8080/"),
            ],
            active: 1,
            focused: false,
            lone: false,
            inset: 40.,
            address: "rust-lang.org",
            back: true,
            forward: true,
        },
        2 | 3 => Case {
            tabs: &[("New Tab", "")],
            active: 0,
            focused: true,
            lone: true,
            inset: 0.,
            address: "",
            back: false,
            forward: false,
        },
        _ => Case {
            tabs: &[("example.com", "Example Domain")],
            active: 0,
            focused: true,
            lone: true,
            inset: 0.,
            address: "example.com/docs",
            back: true,
            forward: false,
        },
    }
}

struct Fixture {
    variant: u8,
    address: Entity<InputState>,
}

impl Render for Fixture {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let variant = self.variant;
        let case = case(variant);
        let empty = matches!(variant, 2 | 3);
        let button = |id, icon, tooltip, disabled, selected| {
            browser_toolbar_button(cx, id, icon, tooltip, disabled, selected)
        };
        let state = PaneState {
            floats: false,
            lone: case.lone,
            zoomed: false,
        };
        let actions = div().flex().flex_none().items_center().gap_1().children(
            pane_header_actions(state)
                .into_iter()
                .map(|(_, icon, label)| pane_header_icon_button(label, icon, true, cx)),
        );
        let tabs = case
            .tabs
            .iter()
            .zip(1_u64..)
            .map(|((label, detail), id)| BrowserTabInfo::new(id, *label, *detail))
            .collect();
        let tabs = BrowserTabStrip::new(tabs, case.active).title(PaneDragSource::new(
            PaneId(7),
            "Browser",
            false,
            |_, _, _| {},
        ));
        let tabs = div()
            .id("browser-tab-strip-drag")
            .flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(tabs);
        let header = BrowserHeader::new(
            case.focused,
            tabs,
            actions,
            BrowserToolbar::new(
                button(
                    "browser-back",
                    IconName::ArrowLeft,
                    "Back",
                    !case.back,
                    false,
                ),
                button(
                    "browser-forward",
                    IconName::ArrowRight,
                    "Forward",
                    !case.forward,
                    false,
                ),
                button("browser-reload", IconName::Redo2, "Reload", false, false),
                browser_address(&self.address, browser_site_controls_button(cx), window, cx),
                button(
                    "browser-element-picker",
                    IconName::Inspector,
                    if variant == 5 {
                        "Cancel element picker"
                    } else {
                        "Pick an element"
                    },
                    false,
                    variant == 5,
                ),
                button(
                    "browser-more",
                    IconName::EllipsisVertical,
                    "More browser actions",
                    false,
                    false,
                ),
            )
            .show_separator(!empty),
        )
        .trailing_inset(px(case.inset));
        let mut content = div()
            .id("browser-content")
            .when(!empty, |surface| surface.bg(cx.theme().background))
            .relative()
            .flex()
            .flex_1()
            .overflow_hidden();
        content = match variant {
            2 => content.child(browser_start_surface(BrowserEmptyHint)),
            3 => content.child(browser_start_surface(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(360.0))
                    .gap(px(4.0))
                    .children(
                        RECENTS.iter().enumerate().map(|(index, url)| {
                            browser_recent_row(("browser-recent", index), *url, None, cx)
                        }),
                    ),
            )),
            4 => content.child(browser_start_surface(
                BrowserErrorPanel::new(ERROR).retry(
                    Button::new("browser-retry")
                        .primary()
                        .on(alert_ground(cx))
                        .label("Try again"),
                ),
            )),
            5 => content.child(BrowserPickStatus::new(PICK)),
            _ => content,
        };
        div().size_full().bg(cx.theme().background).child(
            div()
                .id("browser-root")
                .relative()
                .flex()
                .flex_col()
                .size_full()
                .child(
                    div()
                        .relative()
                        .h(BrowserHeader::HEIGHT)
                        .w_full()
                        .flex_none()
                        .child(
                            div()
                                .h(BrowserHeader::HEIGHT)
                                .w_full()
                                .flex_none()
                                .child(header),
                        ),
                )
                .child(content),
        )
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    let directory = PathBuf::from(args.get(1).ok_or("browser chrome output directory required")?);
    let theme: usize = args
        .get(2)
        .ok_or("browser chrome theme required")?
        .parse()?;
    let scale: f32 = args
        .get(3)
        .ok_or("browser chrome scale required")?
        .parse()?;
    let variant: u8 = args
        .get(4)
        .ok_or("browser chrome variant required")?
        .parse()?;
    if theme >= 6 || ![1., 1.25, 2.].contains(&scale) || usize::from(variant) >= NAMES.len() {
        return Err("invalid browser chrome fixture inputs".into());
    }
    let text = Arc::new(CosmicTextSystem::new_without_system_fonts("Inter"));
    let scene = Rc::new(RefCell::new(Value::Null));
    let captured = scene.clone();
    let mut app = HeadlessAppContext::with_platform(text, Arc::new(penna::Assets), move || {
        let native = current_headless_renderer()?
            .ok_or_else(|| std::io::Error::other("native browser chrome renderer unavailable"))?;
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
        let address = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search or enter a URL")
                .default_value(case(variant).address)
        });
        let fixture = cx.new(|_| Fixture { variant, address });
        cx.new(|cx| {
            Root::new(fixture, window, cx)
                .bordered(false)
                .own_layers(false)
        })
    })?;
    app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
    app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
    fs::create_dir_all(&directory)?;
    app.capture_screenshot(window.into())?
        .save(directory.join("native.png"))?;
    let layouts = app
        .update_window(window.into(), |_, window, _| {
            window.layout_diagnostics(4096)
        })?
        .ok_or("browser chrome layout diagnostic limit exceeded")?;
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
    let case = case(variant);
    fs::write(
        directory.join("input.json"),
        serde_json::to_vec_pretty(&json!({
            "width": WIDTH, "height": HEIGHT, "theme": theme, "scale": scale,
            "variant": variant, "state": NAMES[usize::from(variant)],
            "tabs": case.tabs.iter().map(|(label, detail)| json!([label, detail])).collect::<Vec<_>>(),
            "active": case.active, "focused": case.focused, "lone": case.lone,
            "inset": case.inset, "address": case.address, "back": case.back,
            "forward": case.forward, "recents": RECENTS, "error": ERROR, "pick": PICK,
        }))?,
    )?;
    fs::write(
        directory.join("scene.json"),
        serde_json::to_vec_pretty(&*scene.borrow())?,
    )?;
    Ok(())
}
