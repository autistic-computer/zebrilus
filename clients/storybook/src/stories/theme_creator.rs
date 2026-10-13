use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, ElementId, Entity, IntoElement, Keystroke,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, prelude::*, px, size,
};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, IconName, IndexPath, Tone,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    command::{
        CommandPaletteSurface, PaletteHint, PalettePill, PaletteRow, PaletteStatus,
        command_palette_entry, command_palette_row_height, command_palette_section,
        unified_command_palette_input,
    },
    h_flex,
    input::{Input, InputState, NumberInput},
    interface_theme::{ThemeModeSetting, pinned_theme_mode},
    kbd::Kbd,
    menu::PopupMenu,
    notification::Notification,
    overlay::{dialog_description, dialog_footer, dialog_gutter, dialog_surface, dialog_title},
    pane::{PaneSplitAxis, pane_split_surface, pane_surface},
    select::{Select, SelectState},
    settings::{
        SettingEntry, SettingsStack,
        appearance::{picker_tile, theme_preview},
        settings_card_ground,
    },
    shell::{app_shell_surface, app_workspace_surface},
    slider::DiscreteSlider,
    switch::Switch,
    tooltip::Tooltip,
    v_flex,
    which_key::WhichKeyView,
};

use super::{
    agent::{
        cards::{COMMAND, permission},
        pane::embedded_pane,
    },
    console::which_key::{prefix_header, prefix_rows},
    kit::menus::{SplitRight, bind_keys, pane_menu, view_menu},
    workspace::{
        fixtures::{LOGS, SERVER, TerminalPane, chrome, gap},
        flat_sidebar::Live,
        shell::{Shell, with_sidebar},
        status_bar::{Bar, titlebar},
    },
};
use crate::story::Framed as _;
use crate::story::{Section, Story};
use zz_ui::StyledExt as _;

pub const ID: &str = "theme-creator";

pub const STORY: Story = Story {
    id: ID,
    name: "Theme creator",
    group: "Foundation",
    summary: "Everything a theme touches on one canvas. Tune the knobs, then copy the theme as JSON.",
    sections: &[Section {
        id: "canvas",
        name: "Canvas",
        summary: "zz's chrome, controls and floating surfaces side by side, in one window.",
        build: Showcase::view,
    }],
};

const SHELLS: [&str; 4] = ["zsh", "fish", "bash", "nu"];
const SCROLLBACK: [&str; 5] = ["1k", "10k", "50k", "100k", "∞"];
const LAYOUTS: [&str; 2] = ["Tree", "Flat"];
const WIDE: f32 = 1040.0;
const MEDIUM: f32 = 680.0;
const GAP: f32 = 16.0;
const PLATE_PADDING: f32 = 20.0;
const WHICH_KEY_HEIGHT: f32 = 300.0;

pub struct Showcase {
    shell: Shell,
    flat: Entity<Live>,
    agent: AnyView,
    layout: Entity<SelectState<Vec<&'static str>>>,
    host: Entity<InputState>,
    user: Entity<InputState>,
    port: Entity<InputState>,
    login: Entity<SelectState<Vec<&'static str>>>,
    pane_menu: Entity<PopupMenu>,
    view_menu: Entity<PopupMenu>,
    search: Entity<InputState>,
    toasts: [Entity<Notification>; 2],
    tooltip: Entity<Tooltip>,
    restore: bool,
    animations: bool,
    sound: bool,
    scrollback: usize,
}

impl Showcase {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        bind_keys(cx);
        let shell = Shell::new(window, cx);
        let flat = cx.new(|_| Live::new(None));
        let agent = embedded_pane(window, cx);
        let layout =
            cx.new(|cx| SelectState::new(LAYOUTS.to_vec(), Some(IndexPath::new(1)), window, cx));
        let field = |value: &'static str, window: &mut Window, cx: &mut App| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let host = field("devbox.local", window, cx);
        let user = field("fabrico", window, cx);
        let port = cx.new(|cx| {
            InputState::new(window, cx)
                .step(1.0)
                .min(1.0)
                .max(65535.0)
                .default_value("22")
        });
        let login =
            cx.new(|cx| SelectState::new(SHELLS.to_vec(), Some(IndexPath::new(0)), window, cx));
        let pane_menu = PopupMenu::build(window, cx, pane_menu);
        let view_menu = PopupMenu::build(window, cx, view_menu);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Jump to a window"));
        let toast = |notification: Notification, cx: &mut App| {
            cx.new(|_| notification.autohide(false).width(px(320.0)))
        };
        let toasts = [
            toast(
                Notification::success("3 windows and 7 panes came back.").title("Session restored"),
                cx,
            ),
            toast(
                Notification::error("ssh devbox exited with status 255.").title("Connection lost"),
                cx,
            ),
        ];
        let tooltip = cx.new(|_| Tooltip::new("Split right").action(&SplitRight, None));
        cx.new(|_| Self {
            shell,
            flat,
            agent,
            layout,
            host,
            user,
            port,
            login,
            pane_menu,
            view_menu,
            search,
            toasts,
            tooltip,
            restore: true,
            animations: true,
            sound: false,
            scrollback: 2,
        })
        .into()
    }

    fn hero(&self, cx: &App) -> AnyElement {
        v_flex()
            .id("showcase-app")
            .w_full()
            .h(px(360.0))
            .overflow_hidden()
            .rounded(cx.theme().radius)
            .framed(cx)
            .child(div().flex_1().min_h_0().child(with_sidebar(
                crate::pane().gaps,
                &self.shell,
                0,
                cx,
            )))
            .into_any_element()
    }

    fn flat_app(&self, cx: &App) -> AnyElement {
        let gaps = crate::pane().gaps;
        let agent = pane_surface(
            "showcase-agent-pane",
            self.agent.clone(),
            Vec::new(),
            chrome(true, true, gaps, cx),
            cx,
        );
        let terminals = pane_split_surface(
            "showcase-flat-terminals",
            PaneSplitAxis::Vertical,
            0.5,
            false,
            gaps,
            gap(gaps),
            None,
            None,
            TerminalPane::new(2, "just site", SERVER).gaps(gaps).render(
                "showcase-flat-server",
                Vec::new(),
                cx,
            ),
            TerminalPane::new(3, "tail -f daemon.log", LOGS)
                .gaps(gaps)
                .render("showcase-flat-logs", Vec::new(), cx),
            div().absolute(),
            cx,
        );
        let layout = pane_split_surface(
            "showcase-flat-split",
            PaneSplitAxis::Horizontal,
            0.6,
            false,
            gaps,
            gap(gaps),
            None,
            None,
            agent,
            terminals,
            div().absolute(),
            cx,
        );
        div()
            .id("showcase-flat-app")
            .w_full()
            .h(px(560.0))
            .overflow_hidden()
            .rounded(cx.theme().radius)
            .framed(cx)
            .child(app_shell_surface(
                "showcase-flat-shell",
                cx.theme().background,
                self.flat.clone(),
                None,
                app_workspace_surface(
                    "showcase-flat-workspace",
                    div().size_full().p(gap(gaps)).child(layout),
                    Vec::new(),
                    cx,
                ),
                Vec::new(),
            ))
            .into_any_element()
    }

    fn status_bar(&self, cx: &App) -> AnyElement {
        div()
            .id("showcase-status-bar")
            .w_full()
            .overflow_hidden()
            .rounded(cx.theme().radius)
            .framed(cx)
            .bg(cx.theme().background)
            .child(titlebar(
                &Bar {
                    gaps: crate::pane().gaps,
                    ..Bar::FULL
                },
                cx,
            ))
            .into_any_element()
    }

    fn connect(&self, cx: &App) -> AnyElement {
        card("showcase-connect", cx)
            .child(heading("New host", "Panes on this host open over ssh.", cx))
            .child(labeled("Host", Input::new(&self.host), cx))
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .child(labeled("User", Input::new(&self.user), cx)),
                    )
                    .child(div().w(px(132.0)).child(labeled(
                        "Port",
                        NumberInput::new(&self.port),
                        cx,
                    ))),
            )
            .child(labeled("Login shell", Select::new(&self.login), cx))
            .child(
                h_flex()
                    .pt_1()
                    .gap_2()
                    .justify_end()
                    .child(Button::new("showcase-cancel").label("Cancel"))
                    .child(
                        Button::new("showcase-connect-button")
                            .primary()
                            .label("Connect"),
                    ),
            )
            .into_any_element()
    }

    fn preferences(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        card("showcase-preferences", cx)
            .child(heading("Behavior", "How sessions and panes behave.", cx))
            .child(toggle(
                "showcase-restore",
                "Restore sessions",
                "Bring windows and panes back after a restart",
                self.restore,
                cx.listener(|this, checked: &bool, _, cx| {
                    this.restore = *checked;
                    cx.notify();
                }),
                cx,
            ))
            .child(toggle(
                "showcase-animations",
                "Animations",
                "Menus and dialogs ease in",
                self.animations,
                cx.listener(|this, checked: &bool, _, cx| {
                    this.animations = *checked;
                    cx.notify();
                }),
                cx,
            ))
            .child(toggle(
                "showcase-sound",
                "Bell sound",
                "Play a sound when a pane rings",
                self.sound,
                cx.listener(|this, checked: &bool, _, cx| {
                    this.sound = *checked;
                    cx.notify();
                }),
                cx,
            ))
            .child(DiscreteSlider::new(
                "showcase-scrollback",
                "Scrollback",
                SCROLLBACK
                    .iter()
                    .map(|label| SharedString::from(*label))
                    .collect(),
                self.scrollback,
                move |index, _, cx| {
                    this.update(cx, |this, cx| {
                        this.scrollback = index;
                        cx.notify();
                    });
                },
            ))
            .into_any_element()
    }

    fn settings(&self, cx: &App) -> AnyElement {
        let active = crate::knobs().interface_theme();
        let light = active.colors(zz_ui::ThemeMode::Light);
        let dark = active.colors(zz_ui::ThemeMode::Dark);
        let tiles = h_flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(ThemeModeSetting::ALL.map(|mode| {
                picker_tile(
                    format!("showcase-theme-{}", mode.as_str()).into(),
                    mode.title(),
                    theme_preview(pinned_theme_mode(mode), &light, &dark, cx),
                    mode == ThemeModeSetting::System,
                    cx,
                )
            }));
        div()
            .id("showcase-settings")
            .w_full()
            .child(
                SettingsStack::titled("Interface")
                    .child(
                        SettingEntry::new(
                            "Theme",
                            "Follow the system light/dark setting, or pin one.",
                        )
                        .child(tiles),
                    )
                    .child(
                        SettingEntry::new(
                            "Navigation layout",
                            "Show sessions, windows, and panes as a tree or a flat list.",
                        )
                        .control(
                            Select::new(&self.layout)
                                .title("Navigation layout")
                                .on(settings_card_ground(cx)),
                        ),
                    )
                    .child(
                        SettingEntry::new("Pane gaps", "Float panes apart on the window ground.")
                            .control(Switch::new("showcase-gaps").checked(crate::pane().gaps)),
                    ),
            )
            .into_any_element()
    }

    fn buttons(&self, cx: &App) -> AnyElement {
        let keys = ["cmd-k", "cmd-shift-d", "ctrl-a"];
        card("showcase-buttons", cx)
            .child(heading("Actions", "Buttons, tags and key hints.", cx))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("showcase-primary")
                            .primary()
                            .label("New session"),
                    )
                    .child(Button::new("showcase-default").label("Detach"))
                    .child(Button::new("showcase-ghost").ghost().label("Rename"))
                    .child(Button::new("showcase-danger").danger().label("Kill")),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("showcase-split-right").icon(IconName::PanelRight))
                    .child(Button::new("showcase-split-down").icon(IconName::PanelBottom))
                    .child(
                        Button::new("showcase-search")
                            .ghost()
                            .icon(IconName::Search),
                    )
                    .child(
                        Button::new("showcase-settings")
                            .ghost()
                            .icon(IconName::Settings),
                    )
                    .child(div().flex_1())
                    .child(Button::new("showcase-small").label("Small")),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(Badge::new("connected").tone(Tone::Success))
                    .child(
                        Badge::new("main")
                            .tone(Tone::Primary)
                            .emphasis(Emphasis::Solid),
                    )
                    .child(Badge::new("3 panes"))
                    .children(
                        keys.into_iter()
                            .map(|key| Kbd::new(Keystroke::parse(key).unwrap_or_default())),
                    ),
            )
            .into_any_element()
    }

    fn dialog(&self, cx: &App) -> AnyElement {
        dialog_surface(cx)
            .w_full()
            .max_w(px(360.0))
            .child(dialog_gutter().child(dialog_title().child("Kill session \"work\"?")))
            .child(dialog_gutter().child(
                dialog_description(cx).child(
                    "Its 3 windows and 7 panes stop, and their scrollback is gone for good.",
                ),
            ))
            .child(
                dialog_gutter().child(
                    dialog_footer(cx)
                        .child(Button::new("showcase-dialog-cancel").label("Cancel"))
                        .child(
                            Button::new("showcase-dialog-kill")
                                .danger()
                                .label("Kill session"),
                        ),
                ),
            )
            .into_any_element()
    }

    fn palette(&self, cx: &App) -> AnyElement {
        let row =
            |label: &str, detail: &str, right: &str, icon: IconName, running: bool| PaletteRow {
                label: label.to_owned().into(),
                detail: detail.to_owned().into(),
                right: right.to_owned().into(),
                icon: Some(icon),
                running,
                ..Default::default()
            };
        let rows = [
            row(
                "editor",
                "2 panes",
                "current",
                IconName::PanelsTopLeft,
                false,
            ),
            row("agents", "2 panes", "", IconName::PanelsTopLeft, true),
            row("logs", "1 pane", "", IconName::PanelsTopLeft, false),
            row("train.py", "gpu-box", "", IconName::SquareTerminal, true),
        ];
        let height = px(command_palette_row_height());
        CommandPaletteSurface::new(
            unified_command_palette_input(
                &self.search,
                [PalettePill::Host {
                    label: "devbox".into(),
                    status: PaletteStatus::Online,
                }],
                cx,
            ),
            1,
        )
        .rows(
            v_flex()
                .child(
                    div()
                        .h(height)
                        .child(command_palette_section("Windows", "zz", cx)),
                )
                .children(rows.iter().enumerate().map(|(index, row)| {
                    div().h(height).child(command_palette_entry(
                        ("showcase-palette-row", index),
                        row,
                        index == 0,
                        cx,
                    ))
                })),
        )
        .hints([
            PaletteHint {
                key: "up down",
                label: "navigate",
            },
            PaletteHint {
                key: "enter",
                label: "open",
            },
            PaletteHint {
                key: "escape",
                label: "close",
            },
        ])
        .into_any_element()
    }

    fn agent(&self, cx: &App) -> AnyElement {
        card("showcase-agent", cx)
            .child(heading("Agent", "A tool call waiting for approval.", cx))
            .child(permission(
                "showcase-permission",
                "cargo test -p zz-ui",
                None,
                &COMMAND,
                0,
                true,
                cx,
            ))
            .into_any_element()
    }
}

impl Render for Showcase {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width.as_f32();
        let menus = plate(
            "showcase-menus",
            h_flex().items_start().gap_4().child(self.pane_menu.clone()),
            cx,
        );
        let view = plate("showcase-view-menu", self.view_menu.clone(), cx);
        let palette = plate("showcase-palette", self.palette(cx), cx);
        let dialog = plate("showcase-dialog", self.dialog(cx), cx);
        let toasts = plate(
            "showcase-toasts",
            v_flex()
                .gap_3()
                .items_center()
                .children(self.toasts.iter().cloned())
                .child(self.tooltip.clone()),
            cx,
        );
        let which_key = plate(
            "showcase-which-key",
            WhichKeyView::new(prefix_header(), prefix_rows()).fit(size(
                px((width - 2.0 * PLATE_PADDING - 2.0).max(0.0)),
                px(WHICH_KEY_HEIGHT),
            )),
            cx,
        );
        let tiles = [
            vec![self.connect(cx), self.preferences(cx), view],
            vec![menus, self.buttons(cx), toasts],
            vec![palette, dialog, self.agent(cx), self.settings(cx)],
        ];
        let columns = if width >= WIDE {
            3
        } else if width >= MEDIUM {
            2
        } else {
            1
        };
        let mut stacks: Vec<Vec<AnyElement>> = (0..columns).map(|_| Vec::new()).collect();
        let mut next = 0;
        for (group, column) in tiles.into_iter().enumerate() {
            for tile in column {
                let index = if columns == 3 { group } else { next % columns };
                next += 1;
                stacks[index].push(tile);
            }
        }
        v_flex()
            .w_full()
            .gap(px(GAP))
            .child(self.hero(cx))
            .child(self.flat_app(cx))
            .child(self.status_bar(cx))
            .child(
                h_flex().w_full().items_start().gap(px(GAP)).children(
                    stacks
                        .into_iter()
                        .map(|stack| v_flex().flex_1().min_w_0().gap(px(GAP)).children(stack)),
                ),
            )
            .child(which_key)
    }
}

fn card(id: &'static str, cx: &App) -> zz_gpui::Stateful<zz_gpui::Div> {
    v_flex()
        .id(id)
        .w_full()
        .p(px(16.0))
        .gap_3()
        .rounded(cx.theme().radius)
        .surface_edge(cx)
        .bg(cx.theme().background)
}

/// Floating surfaces sit on a plate that shows the backdrop knob behind
/// them, the way they float over panes in zz.
fn plate(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    let backdrop = crate::backdrop::backdrop(crate::knobs().backdrop, cx);
    div()
        .id(id)
        .relative()
        .w_full()
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .surface_edge(cx)
        .bg(cx.theme().surface(Elevation::Panel).fill)
        .children(backdrop)
        .child(
            h_flex()
                .relative()
                .w_full()
                .p(px(PLATE_PADDING))
                .justify_center()
                .child(content),
        )
        .into_any_element()
}

fn heading(title: &'static str, description: &'static str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .font_weight(zz_gpui::FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground.muted())
                .child(description),
        )
}

fn labeled(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1p5()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().foreground.muted())
                .child(label),
        )
        .child(control)
}

fn toggle(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    checked: bool,
    on_click: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .gap_3()
        .justify_between()
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.muted())
                        .child(description),
                ),
        )
        .child(
            Switch::new(ElementId::from(id))
                .checked(checked)
                .on_click(on_click),
        )
}
