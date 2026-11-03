use std::path::PathBuf;

use ala::{
    App, Bounds, Context, Div, QuitMode, Window, WindowBounds, WindowOptions, div, img, point,
    prelude::*, px, size,
};
use penna::{
    ActiveTheme as _, Icon, IconName, Kind, Label, PaintState, Root, Sizable as _, Space,
    StyledExt as _, Surface, Tab, TabLook, TextStyle, Theme, ThemeMode, Well,
    components::button::{Button, ButtonLook, ButtonVariants as _},
    components::switch::Switch,
    h_flex,
};
use zz_ui::{Assets, interface_theme::InterfaceTheme};

struct Comparison {
    sources: Vec<PathBuf>,
    backdrop: PathBuf,
    definition: InterfaceTheme,
    selected: usize,
    enabled: bool,
}

fn at(x: f32, y: f32) -> Div {
    div().absolute().left(px(x)).top(px(y))
}

#[allow(
    clippy::disallowed_methods,
    reason = "the comparison paints theme materials by role, side by side, to check how the macOS theme draws them"
)]
fn material(element: Div, surface: impl Into<Surface>, cx: &App) -> Div {
    element.surface(surface, cx)
}

#[allow(
    clippy::disallowed_methods,
    reason = "the comparison samples the bare floating material, which no look names"
)]
const fn glass_sample() -> Surface {
    Surface::new(Kind::Floating.role())
}

impl Comparison {
    fn apply(&self, cx: &mut App) {
        self.definition.apply(Theme::global_mut(cx));
        cx.refresh_windows();
    }
}

impl Render for Comparison {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let foreground = cx.theme().foreground;
        let glass_sample = glass_sample();
        let panel_ground = cx.theme().ground_of(glass_sample);
        let group_ground = cx.theme().ground_of(ButtonLook::Chrome);
        let dark = cx.theme().mode.is_dark();
        let toolbar = h_flex().gap(px(12.0)).children(
            [IconName::PanelLeft, IconName::Pencil, IconName::Search]
                .into_iter()
                .enumerate()
                .map(|(i, icon)| {
                    Button::new(("toolbar", i))
                        .icon(icon)
                        .look(ButtonLook::Chrome)
                        .with_size(px(30.0))
                        .h(px(48.0))
                        .w(px(48.0))
                        .on_click(|_, window, cx| {
                            cx.activate(true);
                            window.activate_window();
                        })
                }),
        );
        let tabs = Well::new()
            .h_flex()
            .size_full()
            .gap(Space::S2)
            .p(Space::S2)
            .rounded_full()
            .children(
                [
                    ("Terminal", IconName::Terminal),
                    ("Browser", IconName::Globe),
                    ("Agent", IconName::Bot),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (label, icon))| {
                    Tab::new(("tab", i), TabLook::Window)
                        .state(if self.selected == i {
                            PaintState::Selected
                        } else {
                            PaintState::Rest
                        })
                        .h_flex()
                        .justify_center()
                        .gap(Space::S3)
                        .flex_1()
                        .h(px(28.0))
                        .child(Icon::new(icon))
                        .child(Label::new(TextStyle::Lead, label))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = i;
                            cx.notify();
                        }))
                }),
            );
        div()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .font_family(cx.theme().font_family.clone())
            .text_color(foreground)
            .type_style(TextStyle::Lead, cx)
            .child(
                at(0.0, 430.0)
                    .w(px(600.0))
                    .h(px(290.0))
                    .overflow_hidden()
                    .child(
                        img(self.backdrop.clone())
                            .relative()
                            .top(px(-430.0))
                            .w(px(600.0))
                            .h(px(720.0)),
                    ),
            )
            .child(at(28.0, 30.0).child(Label::new(TextStyle::Display, "macOS")))
            .child(
                at(28.0, 67.0)
                    .child(Label::new(TextStyle::Body, "zz-ui • theme materials").muted()),
            )
            .child(at(28.0, 110.0).child(toolbar))
            .child(
                material(at(210.0, 110.0), ButtonLook::Chrome, cx)
                    .w(px(220.0))
                    .h(px(48.0))
                    .child(
                        h_flex()
                            .size_full()
                            .justify_around()
                            .items_center()
                            .children(
                                [IconName::ArrowLeft, IconName::ArrowRight, IconName::Search]
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, icon)| {
                                        Button::new(("group", i))
                                            .icon(icon)
                                            .ghost()
                                            .flat()
                                            .on(group_ground)
                                    }),
                            ),
                    ),
            )
            .child(at(28.0, 209.0).w(px(544.0)).h(px(36.0)).child(tabs))
            .child(
                material(at(28.0, 300.0), glass_sample, cx)
                    .w(px(544.0))
                    .h(px(120.0))
                    .child(at(24.0, 20.0).child(Label::new(TextStyle::Title, "Glass surface")))
                    .child(
                        at(24.0, 54.0).child(
                            Label::new(TextStyle::Lead, "Window, popover, and control materials")
                                .muted()
                                .on(panel_ground),
                        ),
                    )
                    .child(
                        at(470.0, 44.0).child(
                            Switch::new("enabled")
                                .checked(self.enabled)
                                .on_click(cx.listener(|this, value, _, cx| {
                                    this.enabled = *value;
                                    cx.notify();
                                })),
                        ),
                    ),
            )
            .child(
                material(at(28.0, 480.0), glass_sample, cx)
                    .w(px(544.0))
                    .h(px(100.0))
                    .child(
                        at(24.0, 22.0)
                            .child(Label::new(TextStyle::Lead, "Backdrop check").strong()),
                    )
                    .child(
                        at(24.0, 52.0).child(
                            Label::new(TextStyle::Lead, "The same material over colour")
                                .muted()
                                .on(panel_ground),
                        ),
                    )
                    .child(
                        at(398.0, 32.0).child(
                            Button::new("continue")
                                .label("Continue")
                                .on(panel_ground)
                                .primary(),
                        ),
                    ),
            )
            .child(
                at(28.0, 650.0).child(Button::new("reload").label("Reload theme").on_click(
                    cx.listener(|this, _, _, cx| match load(&this.sources) {
                        Ok(definition) => {
                            this.definition = definition;
                            this.apply(cx);
                        }
                        Err(error) => eprintln!("{error}"),
                    }),
                )),
            )
            .child(
                at(408.0, 650.0).child(
                    Button::new("appearance")
                        .label(if dark {
                            "Switch to light"
                        } else {
                            "Switch to dark"
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            Theme::change(
                                if dark {
                                    ThemeMode::Light
                                } else {
                                    ThemeMode::Dark
                                },
                                Some(window),
                                cx,
                            );
                            this.apply(cx);
                        })),
                ),
            )
            .child(
                at(28.0, 695.0).child(
                    Label::new(
                        TextStyle::Caption,
                        if window.is_window_active() {
                            "Window active"
                        } else {
                            "Window inactive"
                        },
                    )
                    .muted(),
                ),
            )
    }
}

fn load(sources: &[PathBuf]) -> Result<InterfaceTheme, String> {
    let texts = sources
        .iter()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    penna::foundation::interface_theme::parse_layers(
        &texts.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = std::env::args().nth(1).map_or_else(
        || vec![root.join("libs/penna/themes/macos.json")],
        |path| vec![PathBuf::from(path)],
    );
    let backdrop = std::env::args().nth(2).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/macos-backdrop.png"),
        PathBuf::from,
    );
    let definition = load(&sources).expect("parse theme");
    ala_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            cx.set_quit_mode(QuitMode::LastWindowClosed);
            zz_ui::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            definition.apply(Theme::global_mut(cx));
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(760.0), px(100.0)),
                        size(px(600.0), px(720.0)),
                    ))),
                    ..WindowOptions::default()
                },
                |window, cx| {
                    window.set_window_title("macOS • zz-ui comparison");
                    let view = cx.new(|_| Comparison {
                        sources,
                        backdrop,
                        definition,
                        selected: 0,
                        enabled: true,
                    });
                    cx.new(|cx| Root::new(view, window, cx).bordered(false))
                },
            )
            .expect("open comparison");
            cx.activate(true);
        });
}
