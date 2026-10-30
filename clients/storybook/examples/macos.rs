use std::path::PathBuf;

use ala::{
    App, Bounds, Context, Div, FontWeight, QuitMode, Window, WindowBounds, WindowOptions, div, img,
    point, prelude::*, px, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, Fill, IconName, Kind, Root, Selectable as _, Shadow, Shape,
    Sizable as _, StylePatch, StyledExt as _, SurfaceRole, Theme, ThemeMode,
    button::{Button, ButtonVariants as _},
    h_flex,
    switch::Switch,
};
use zz_ui::{Assets, interface_theme::InterfaceTheme, roles};

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

impl Comparison {
    fn apply(&self, cx: &mut App) {
        self.definition.apply(Theme::global_mut(cx));
        cx.refresh_windows();
    }
}

impl Render for Comparison {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let foreground = cx.theme().foreground;
        let muted = foreground.muted();
        let glass_sample = SurfaceRole::new(
            "comparison.glass",
            Kind::Card,
            "Glass sample",
            "Reference material sample",
        )
        .with(StylePatch::EMPTY.fill(Fill::Glass).outline(false));
        let panel_ground = cx.theme().ground_of(glass_sample);
        let group_ground = cx.theme().ground_of(roles::CHROME_BUTTON);
        let dark = cx.theme().mode.is_dark();
        let toolbar = h_flex().gap(px(12.0)).children(
            [IconName::PanelLeft, IconName::Pencil, IconName::Search]
                .into_iter()
                .enumerate()
                .map(|(i, icon)| {
                    Button::new(("toolbar", i))
                        .icon(icon)
                        .surface_role(roles::CHROME_BUTTON)
                        .with_size(px(30.0))
                        .h(px(48.0))
                        .w(px(48.0))
                        .on_click(|_, window, cx| {
                            cx.activate(true);
                            window.activate_window();
                        })
                }),
        );
        let tabs = h_flex()
            .size_full()
            .gap(px(4.0))
            .p(px(4.0))
            .surface(
                SurfaceRole::new("comparison.track", Kind::Card, "Tab track", "A tab group").with(
                    StylePatch::EMPTY
                        .fill(Fill::Tint)
                        .outline(false)
                        .shadow(Shadow::None)
                        .shape(Shape::Pill),
                ),
                cx,
            )
            .children(
                [
                    ("Terminal", IconName::Terminal),
                    ("Browser", IconName::Globe),
                    ("Agent", IconName::Bot),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (label, icon))| {
                    Button::new(("tab", i))
                        .label(label)
                        .icon(icon)
                        .ghost()
                        .surface_role(roles::WINDOW_TAB)
                        .selected(self.selected == i)
                        .flex_1()
                        .h(px(28.0))
                        .text_size(px(14.0))
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
            .text_size(px(14.0))
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
            .child(
                at(28.0, 30.0)
                    .text_size(px(26.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("macOS"),
            )
            .child(
                at(28.0, 67.0)
                    .text_size(px(13.0))
                    .text_color(muted)
                    .child("zz-ui • theme materials"),
            )
            .child(at(28.0, 110.0).child(toolbar))
            .child(
                at(210.0, 110.0)
                    .w(px(220.0))
                    .h(px(48.0))
                    .surface(roles::CHROME_BUTTON, cx)
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
                at(28.0, 300.0)
                    .w(px(544.0))
                    .h(px(120.0))
                    .surface(glass_sample, cx)
                    .child(
                        at(24.0, 20.0)
                            .text_size(px(19.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Glass surface"),
                    )
                    .child(
                        at(24.0, 54.0)
                            .text_color(muted)
                            .child("Window, popover, and control materials"),
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
                at(28.0, 480.0)
                    .w(px(544.0))
                    .h(px(100.0))
                    .surface(glass_sample, cx)
                    .child(
                        at(24.0, 22.0)
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Backdrop check"),
                    )
                    .child(
                        at(24.0, 52.0)
                            .text_color(muted)
                            .child("The same material over colour"),
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
            .child(at(28.0, 695.0).text_size(px(11.0)).text_color(muted).child(
                if window.is_window_active() {
                    "Window active"
                } else {
                    "Window inactive"
                },
            ))
    }
}

fn load(sources: &[PathBuf]) -> Result<InterfaceTheme, String> {
    let texts = sources
        .iter()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    penna::interface_theme::parse_layers(&texts.iter().map(String::as_str).collect::<Vec<_>>())
}

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = std::env::args().nth(1).map_or_else(
        || {
            vec![
                root.join("libs/penna/themes/macos.json"),
                root.join("crates/zz-ui/themes/macos.json"),
            ]
        },
        |path| vec![PathBuf::from(path)],
    );
    let backdrop = std::env::args().nth(2).map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/macos-backdrop.png"),
        PathBuf::from,
    );
    zz_ui::register();
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
                    window.set_default_corner_smoothing(penna::CORNER_SMOOTHING);
                    window.set_adaptive_corner_fraction(Some(0.45));
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
