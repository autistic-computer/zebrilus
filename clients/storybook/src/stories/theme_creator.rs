use std::sync::Arc;

use zz_gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, ElementId, Entity, Focusable as _,
    IntoElement, Keystroke, ParentElement as _, Render, SharedString, Styled as _, Window, div,
    prelude::*, px, relative, size,
};
use zz_protocol::{DisplayPanesState, PaneId, PaneIndicator, TmuxColour, WindowId};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, IconName, IndexPath, Tone,
    agent::{agent_jump_to_bottom_button, chat::chat_button},
    badge::Badge,
    browser::{BrowserHeader, browser_recent_row, browser_start_surface},
    button::{Button, ButtonVariants as _},
    command::{
        CommandPaletteSurface, PaletteHint, PalettePill, PaletteRow, PaletteStatus,
        command_palette_entry, command_palette_row_height, command_palette_section,
        unified_command_palette_input,
    },
    compact::{
        ArrowDirection, BottomSheet, HostState, PillLabel, ToolKeys, WhichKeyList,
        arrow_pad_bubble, bottom_sheet, compact_bar_button, compact_floating_bar,
        compact_floating_group, compact_floating_pill, compact_hud, compact_pane_header,
        overview_card, overview_choice_row, overview_count, overview_host_row,
        overview_section_heading, page_dots, sheet_close, sheet_row,
    },
    h_flex,
    input::{Input, InputState, NumberInput},
    interface_theme::{ThemeModeSetting, pinned_theme_mode},
    kbd::Kbd,
    menu::PopupMenu,
    navigation::workspace_chat_button,
    notification::Notification,
    overlay::{dialog_description, dialog_footer, dialog_gutter, dialog_surface, dialog_title},
    pane::{
        FloatingSurface, PaneOverlayCorner, PaneSplitAxis, pane_drag_chip, pane_drop_preview,
        pane_indicator, pane_indicator_overlay, pane_overlay_stack, pane_picker_row,
        pane_picker_surface, pane_split_surface, pane_status_badge, pane_surface, pane_sync_badge,
        pane_waiting_state, terminal_mode_indicator,
    },
    pip::PipSurface,
    select::{Select, SelectState},
    settings::{
        SettingEntry, SettingsStack,
        appearance::{picker_tile, theme_preview},
        settings_card_ground,
    },
    shell::{
        app_shell_surface, app_workspace_surface, empty_workspace, empty_workspace_action,
        empty_workspace_caption, empty_workspace_connect, empty_workspace_hint,
        empty_workspace_keys,
    },
    slider::DiscreteSlider,
    switch::Switch,
    text::{TextView, TextViewStyle},
    tooltip::Tooltip,
    v_flex,
    which_key::WhichKeyView,
};

use super::{
    agent::{
        cards::{COMMAND, permission},
        chat::{bottom_scroll, driver_input, driver_palette},
        lens,
    },
    console::{
        phone::{binding_rows, close, groups, key_row, placement, tool_keys},
        which_key::{prefix_header, prefix_rows},
    },
    kit::menus::{SplitRight, bind_keys, pane_menu, view_menu},
    workspace::{
        browser::header as browser_header,
        fixtures::{
            BUILD, LOGS, PANE_CHOICES, SERVER, TerminalPane, chrome, gap, radii, stateful,
            terminal_body,
        },
        flat_sidebar::Live,
        shell::{Shell, with_sidebar},
        status_bar::{Bar, titlebar},
    },
};
use crate::story::Framed as _;
use crate::story::{Section, Story, stateless};
use zz_ui::StyledExt as _;

pub const ID: &str = "theme-creator";

pub const STORY: Story = Story {
    id: ID,
    name: "Theme creator",
    group: "Foundation",
    summary: "Everything a theme touches, region by region. Tune the knobs, then copy the theme as JSON.",
    sections: &[
        Section {
            id: "workspace",
            name: "Workspace",
            summary: "The window with the tree sidebar, and the title bar as it reads with the sidebar hidden, its chat button dotted while the driver chat waits on you.",
            build: Workspace::view,
        },
        Section {
            id: "flat",
            name: "Flat sidebar",
            summary: "The flat sidebar with the driver chat under it, beside Claude Code with its lens open and two terminals. Drag the line over the chat to move the split.",
            build: Flat::view,
        },
        Section {
            id: "lens",
            name: "Lens",
            summary: "The agent lens over two terminals, each toggle lit while its lens is open: Claude Code at a plan approval, whose card offers Show terminal, and Codex before its first prompt.",
            build: Lens::view,
        },
        Section {
            id: "panes",
            name: "Panes",
            summary: "Badges, the drop preview, the drag chip and a popup over panes.",
            build: |_, cx| stateless(panes, cx),
        },
        Section {
            id: "pip",
            name: "PiP",
            summary: "Floats drawn as PiPs over the tiled panes, casting the float shadow: a terminal with its title in the top border, and Claude Code with its lens open.",
            build: Pip::view,
        },
        Section {
            id: "start",
            name: "Start",
            summary: "The workspace before its first session, the first run with no host, and a new split asking what it should become.",
            build: |_, cx| stateless(start, cx),
        },
        Section {
            id: "controls",
            name: "Controls",
            summary: "A form, switches, buttons and a settings card.",
            build: Controls::view,
        },
        Section {
            id: "floating",
            name: "Floating",
            summary: "Menus, the palette and its Driver mode, a dialog, toasts and which-key over the backdrop.",
            build: Floating::view,
        },
        Section {
            id: "content",
            name: "Content",
            summary: "An agent card, markdown and a new browser tab with its address focused.",
            build: |window, cx| stateful(window, cx, new_tab_address, content),
        },
        Section {
            id: "phone",
            name: "Phone",
            summary: "The phone: a terminal page with a HUD, the key row's prefix menu and arrow pad, the bindings list, the overview's pane cards and the sessions sheet.",
            build: Phone::view,
        },
        Section {
            id: "drawers",
            name: "Drawers",
            summary: "Every drawer on the phone, each its own role under Drawers, resting on the phone's bottom safe area.",
            build: |window, cx| {
                stateful(
                    window,
                    cx,
                    |window, cx| cx.new(|cx| InputState::new(window, cx).placeholder("user@host")),
                    drawers,
                )
            },
        },
    ],
};

const SHELLS: [&str; 4] = ["zsh", "fish", "bash", "nu"];
const SCROLLBACK: [&str; 5] = ["1k", "10k", "50k", "100k", "∞"];
const LAYOUTS: [&str; 2] = ["Tree", "Flat"];
const WIDE: f32 = 1040.0;
const MEDIUM: f32 = 680.0;
const GAP: f32 = 16.0;
const PLATE_PADDING: f32 = 20.0;
const WHICH_KEY_HEIGHT: f32 = 300.0;
const PANES_HEIGHT: f32 = 250.0;
const PIP_HEIGHT: f32 = 420.0;
const LENS_HEIGHT: f32 = 420.0;
const PHONE_WIDTH: f32 = 390.0;
const START_HEIGHT: f32 = 300.0;
const START_PAGE_HEIGHT: f32 = 220.0;
const PREFIX: &str = "ctrl-b";
const SESSION_KEYS: [(&str, &str); 5] = [
    ("c", "New window"),
    ("%", "Split right"),
    ("\"", "Split down"),
    ("s", "Sessions and windows"),
    ("?", "Every key binding"),
];
const PHONE_LINES: [&str; 3] = [
    "~/dev/zz main* > cargo test -p zz-ui",
    "test result: ok. 214 passed; 0 failed",
    "~/dev/zz main* > ",
];
const PHONE_SHEET_HEIGHT: f32 = 460.0;
const SCREEN_RADIUS: f32 = 55.0;
const SAFE_BOTTOM: f32 = 34.0;
const DRAWER_SCREEN_HEIGHT: f32 = 480.0;
const PHONE_PAGE_HEIGHT: f32 = 190.0;
const KEYS_PAGE_HEIGHT: f32 = 470.0;
const CARD_LINES: [&str; 6] = [
    "$ cargo test",
    "Compiling zz-ui",
    "Finished test",
    "ok. 214 passed",
    "$ git status",
    "On branch main",
];
const OVERVIEW: [(u32, &str, &[(IconName, &str, bool, bool)]); 2] = [
    (
        1,
        "editor",
        &[
            (IconName::SquareTerminal, "cargo test", true, false),
            (IconName::RobotFace, "Review sidebar", false, true),
        ],
    ),
    (
        2,
        "docs",
        &[(IconName::BrandChrome, "zed.dev", false, false)],
    ),
];
const SESSIONS: [(&str, usize, bool); 3] =
    [("work", 3, true), ("scratch", 1, false), ("logs", 2, false)];
const READING: &str = "Pinned tabs keep a `28px` floor, and the rest share what is left:\n\n\
| Tabs | Pinned | Unpinned |\n\
| ---: | ---: | ---: |\n\
| 10 | 28px | 96px |\n\
| 30 | 28px | 31px |\n\n\
```rust\n\
let floor = px(28.0) * pinned;\n\
```";
const RECENT: [&str; 3] = [
    "zed.dev/docs/key-bindings",
    "github.com/demfabris/zz/pull/412",
    "localhost:4321",
];

struct Workspace {
    shell: Shell,
}

impl Workspace {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let shell = Shell::new(window, cx);
        cx.new(|_| Self { shell }).into()
    }

    fn hero(&self, window: &Window, cx: &App) -> AnyElement {
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
                window,
                cx,
            )))
            .into_any_element()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap(px(GAP))
            .child(self.hero(window, cx))
            .child(status_bar(cx))
    }
}

struct Flat {
    flat: Entity<Live>,
    agent: AnyView,
}

impl Flat {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let flat = cx.new(|_| Live::new(None).driver());
        let agent = lens::tiled(window, cx);
        cx.new(|_| Self { flat, agent }).into()
    }
}

impl Render for Flat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.flat_app(cx)
    }
}

impl Flat {
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
}

struct Lens {
    plans: AnyView,
    first: AnyView,
}

impl Lens {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let plans = lens::tiled_plans(window, cx);
        let first = lens::tiled_first(window, cx);
        cx.new(|_| Self { plans, first }).into()
    }
}

impl Render for Lens {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let gaps = crate::pane().gaps;
        let plans = pane_surface(
            "showcase-lens-plans",
            self.plans.clone(),
            Vec::new(),
            chrome(true, true, gaps, cx),
            cx,
        );
        let first = pane_surface(
            "showcase-lens-first",
            self.first.clone(),
            Vec::new(),
            chrome(false, true, gaps, cx),
            cx,
        );
        div()
            .id("showcase-lens")
            .w_full()
            .h(px(LENS_HEIGHT))
            .overflow_hidden()
            .rounded(cx.theme().radius)
            .framed(cx)
            .bg(cx.theme().background)
            .p(gap(gaps))
            .child(pane_split_surface(
                "showcase-lens-split",
                PaneSplitAxis::Horizontal,
                0.5,
                false,
                gaps,
                gap(gaps),
                None,
                None,
                plans,
                first,
                div().absolute(),
                cx,
            ))
    }
}

struct Pip {
    lens: AnyView,
}

impl Pip {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let lens = lens::floating_claude(window, cx);
        cx.new(|_| Self { lens }).into()
    }
}

impl Render for Pip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let gaps = crate::pane().gaps;
        let tiled = pane_split_surface(
            "showcase-pip-split",
            PaneSplitAxis::Horizontal,
            0.5,
            false,
            gaps,
            gap(gaps),
            None,
            None,
            TerminalPane::new(2, "just site", SERVER).gaps(gaps).render(
                "showcase-pip-server",
                Vec::new(),
                cx,
            ),
            TerminalPane::new(3, "tail -f daemon.log", LOGS)
                .gaps(gaps)
                .render("showcase-pip-logs", Vec::new(), cx),
            div().absolute(),
            cx,
        );
        let terminal = PipSurface::new(
            "showcase-pip-terminal",
            terminal_body(BUILD, false, cx).bg(zz_gpui::transparent_black()),
        )
        .header(super::workspace::panes::pip_header(
            "cargo watch -x test",
            cx,
        ))
        .content_inset(px(8.0), px(16.0));
        v_flex().w_full().gap(px(GAP)).child(
            div()
                .id("showcase-pip")
                .relative()
                .w_full()
                .h(px(PIP_HEIGHT))
                .overflow_hidden()
                .rounded(cx.theme().radius)
                .framed(cx)
                .bg(cx.theme().background)
                .p(gap(gaps))
                .child(tiled)
                .child(
                    div()
                        .absolute()
                        .left(px(32.0))
                        .bottom(px(32.0))
                        .w(px(360.0))
                        .h(px(180.0))
                        .child(terminal),
                )
                .child(
                    div()
                        .absolute()
                        .right(px(32.0))
                        .top(px(32.0))
                        .w(px(380.0))
                        .h(px(320.0))
                        .child(self.lens.clone()),
                ),
        )
    }
}

struct Controls {
    layout: Entity<SelectState<Vec<&'static str>>>,
    host: Entity<InputState>,
    user: Entity<InputState>,
    port: Entity<InputState>,
    login: Entity<SelectState<Vec<&'static str>>>,
    restore: bool,
    animations: bool,
    sound: bool,
    scrollback: usize,
}

impl Controls {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
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
        cx.new(|_| Self {
            layout,
            host,
            user,
            port,
            login,
            restore: true,
            animations: true,
            sound: false,
            scrollback: 2,
        })
        .into()
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
                            "Mode",
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
}

impl Render for Controls {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tiles(
            window,
            vec![
                vec![self.connect(cx)],
                vec![self.preferences(cx)],
                vec![buttons(cx), self.settings(cx)],
            ],
        )
    }
}

struct Floating {
    pane_menu: Entity<PopupMenu>,
    view_menu: Entity<PopupMenu>,
    search: Entity<InputState>,
    driver: Entity<InputState>,
    driver_thread: zz_gpui::ScrollHandle,
    toasts: [Entity<Notification>; 2],
    tooltip: Entity<Tooltip>,
}

impl Floating {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        bind_keys(cx);
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
        let driver = driver_input("", window, cx);
        cx.new(|_| Self {
            pane_menu,
            view_menu,
            search,
            driver,
            driver_thread: bottom_scroll(),
            toasts,
            tooltip,
        })
        .into()
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
}

impl Render for Floating {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width.as_f32();
        let menus = plate("showcase-menus", self.pane_menu.clone(), cx);
        let view = plate("showcase-view-menu", self.view_menu.clone(), cx);
        let palette = plate("showcase-palette", self.palette(cx), cx);
        let driver = plate(
            "showcase-driver-palette",
            driver_palette(&self.driver, &self.driver_thread, cx),
            cx,
        );
        let dialog = plate("showcase-dialog", dialog(cx), cx);
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
        v_flex()
            .w_full()
            .gap(px(GAP))
            .child(tiles(
                window,
                vec![vec![menus, driver], vec![view, toasts], vec![palette, dialog]],
            ))
            .child(which_key)
    }
}

fn new_tab_address(window: &mut Window, cx: &mut App) -> Entity<InputState> {
    let address = cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter address"));
    address.read(cx).focus_handle(cx).focus(window, cx);
    address
}

fn content(address: &Entity<InputState>, window: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    tiles(
        window,
        vec![
            vec![reading(cx)],
            vec![agent(cx)],
            vec![start_page(address, window, cx)],
        ],
    )
    .into_any_element()
}

struct Phone {
    keys: ToolKeys,
    menu: ToolKeys,
}

impl Phone {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|cx| {
            let keys = tool_keys(cx);
            let menu = tool_keys(cx);
            menu.prefix.update(cx, zz_ui::compact::PopoverKey::open);
            Self { keys, menu }
        })
        .into()
    }

    fn terminal(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let placement = placement();
        let pill = compact_floating_pill(
            "showcase-phone-pill",
            PillLabel {
                icon: IconName::SquareTerminal,
                title: "cargo test".into(),
                meta: "zz · editor".into(),
                warning: false,
            },
            page_dots(&groups(), cx),
            &placement,
            cx,
        );
        phone_frame("showcase-phone-screen", cx)
            .child(compact_pane_header(
                IconName::SquareTerminal,
                "cargo test",
                [close("showcase-phone-close", cx)],
                cx,
            ))
            .child(
                v_flex()
                    .relative()
                    .h(px(PHONE_PAGE_HEIGHT))
                    .px(px(12.0))
                    .py(px(8.0))
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .children(PHONE_LINES.map(|line| div().h(px(17.0)).child(line)))
                    .child(compact_hud("Copied", cx)),
            )
            .child(div().id("showcase-phone-keys").child(key_row(&self.keys)))
            .child(compact_floating_bar(pill, &placement, cx))
            .into_any_element()
    }

    fn keys(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        phone_frame("showcase-phone-menu", cx)
            .child(compact_pane_header(
                IconName::SquareTerminal,
                "zsh",
                [close("showcase-phone-menu-close", cx)],
                cx,
            ))
            .child(
                h_flex()
                    .h(px(KEYS_PAGE_HEIGHT))
                    .px(px(12.0))
                    .py(px(8.0))
                    .items_start()
                    .justify_end()
                    .font_family(theme.mono_font_family.clone())
                    .text_size(px(12.0))
                    .child(arrow_pad_bubble(Some(ArrowDirection::Up), cx)),
            )
            .child(
                div()
                    .id("showcase-phone-menu-keys")
                    .child(key_row(&self.menu)),
            )
            .into_any_element()
    }

    fn bindings(cx: &App) -> AnyElement {
        let rows: Arc<[_]> = binding_rows().iter().take(4).cloned().collect();
        phone_frame("showcase-phone-bindings", cx)
            .child(WhichKeyList::new(rows))
            .into_any_element()
    }

    fn overview(cx: &App) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.foreground.muted();
        let preview = || {
            v_flex()
                .px(px(6.0))
                .font_family(theme.mono_font_family.clone())
                .text_size(px(7.0))
                .line_height(px(9.0))
                .text_color(muted)
                .children(CARD_LINES.map(|line| div().whitespace_nowrap().child(line)))
                .into_any_element()
        };
        let placement = placement();
        let bar = compact_floating_bar(
            compact_floating_pill(
                "showcase-overview-session",
                PillLabel {
                    icon: IconName::Layers,
                    title: "work".into(),
                    meta: "macbook · 2 windows".into(),
                    warning: false,
                },
                zz_ui::Icon::new(IconName::ChevronUp)
                    .size(px(12.0))
                    .text_color(muted),
                &placement,
                cx,
            ),
            &placement,
            cx,
        )
        .child(compact_floating_group(
            [
                compact_bar_button("showcase-overview-settings", IconName::Settings),
                compact_bar_button("showcase-overview-close", IconName::Xmark),
            ],
            &placement,
            cx,
        ));
        let sections = OVERVIEW.iter().map(|(index, name, panes)| {
            v_flex()
                .child(overview_section_heading(*index, name, None, cx))
                .child(
                    h_flex()
                        .gap(px(zz_ui::compact::OVERVIEW_CARD_GAP))
                        .px(px(16.0))
                        .py(px(2.0))
                        .overflow_hidden()
                        .children(panes.iter().enumerate().map(
                            |(slot, (icon, title, current, attention))| {
                                let id = (*index as usize) * 10 + slot;
                                overview_card(
                                    ("showcase-overview-card", id),
                                    ("showcase-overview-press", id),
                                    icon.clone(),
                                    *title,
                                    (*icon == IconName::SquareTerminal).then(preview),
                                    *current,
                                    *attention,
                                    cx,
                                )
                            },
                        )),
                )
        });
        phone_frame("showcase-phone-overview", cx)
            .bg(theme.surface(Elevation::Panel).fill)
            .children(sections)
            .child(div().flex_1().min_h(px(12.0)))
            .child(bar)
            .into_any_element()
    }

    fn sheet(cx: &App) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.foreground.muted();
        let rows = SESSIONS
            .iter()
            .enumerate()
            .map(|(ix, (name, windows, attached))| {
                sheet_row(
                    ("showcase-session-option", ix),
                    zz_ui::roles::PHONE_ROW,
                    Some(zz_ui::Icon::new(IconName::Layers)),
                    *name,
                    cx,
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(muted)
                        .child(overview_count(*windows, "window")),
                )
                .when(*attached, |row| {
                    row.child(
                        zz_ui::Icon::new(IconName::Check)
                            .size(px(18.0))
                            .text_color(theme.accent),
                    )
                })
            });
        let sheet = bottom_sheet(
            "showcase-sessions-sheet",
            "Sessions",
            [compact_bar_button("showcase-sessions-close", IconName::Xmark).into_any_element()],
            v_flex()
                .list_inset(cx)
                .pb(px(8.0))
                .children(rows)
                .child(div().mx(px(12.0)).my(px(4.0)).rule_on(
                    zz_ui::roles::LIST_RULE,
                    zz_ui::Edge::Top,
                    cx,
                ))
                .child(sheet_row(
                    "showcase-new-session",
                    zz_ui::roles::PHONE_ROW,
                    Some(zz_ui::Icon::new(IconName::Plus)),
                    "New session",
                    cx,
                )),
            px(0.0),
            |_, _| {},
        )
        .header_role(zz_ui::roles::PHONE_BAR);
        phone_frame("showcase-phone-sheet", cx)
            .relative()
            .h(px(PHONE_SHEET_HEIGHT))
            .bg(theme.surface(Elevation::Panel).fill)
            .child(
                v_flex()
                    .p(px(16.0))
                    .gap_2()
                    .text_size(px(13.0))
                    .text_color(muted)
                    .children(PHONE_LINES.map(|line| div().child(line))),
            )
            .child(sheet)
            .into_any_element()
    }
}

impl Render for Phone {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tiles(
            window,
            vec![
                vec![self.keys(cx), Self::bindings(cx)],
                vec![self.terminal(cx), Self::sheet(cx)],
                vec![Self::overview(cx)],
            ],
        )
    }
}

fn drawer_screen(id: &'static str, sheet: BottomSheet, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(id)
        .relative()
        .w_full()
        .max_w(px(PHONE_WIDTH))
        .h(px(DRAWER_SCREEN_HEIGHT))
        .overflow_hidden()
        .rounded(px(SCREEN_RADIUS))
        .framed(cx)
        .bg(theme.surface(Elevation::Panel).fill)
        .child(
            v_flex()
                .p(px(24.0))
                .gap_2()
                .text_size(px(13.0))
                .text_color(theme.foreground.muted())
                .children(PHONE_LINES.map(|line| div().child(line))),
        )
        .child(sheet)
        .into_any_element()
}

fn drawer(
    id: &'static str,
    title: &'static str,
    role: zz_ui::SurfaceRole,
    content: impl IntoElement,
) -> BottomSheet {
    bottom_sheet(
        id,
        title,
        [sheet_close((id, 0_usize)).into_any_element()],
        content,
        px(SAFE_BOTTOM),
        |_, _| {},
    )
    .surface_role(role)
    .header_role(zz_ui::roles::PHONE_BAR)
}

fn drawers(destination: &Entity<InputState>, window: &mut Window, cx: &mut App) -> AnyElement {
    use zz_ui::roles;
    let cx: &App = cx;
    let muted = cx.theme().foreground.muted();
    let list = || v_flex().list_inset(cx).pb(px(8.0));
    let rule = || {
        div()
            .mx(px(12.0))
            .my(px(4.0))
            .rule_on(roles::LIST_RULE, zz_ui::Edge::Top, cx)
    };
    let plus = |id: &'static str, label: &'static str| {
        sheet_row(
            id,
            roles::PHONE_ROW,
            Some(zz_ui::Icon::new(IconName::Plus)),
            label,
            cx,
        )
    };
    let hosts = list()
        .child(overview_host_row(
            "drawer-host-macbook",
            HostState::Connected,
            "macbook",
            "2 sessions",
            true,
            cx,
        ))
        .child(overview_host_row(
            "drawer-host-xps",
            HostState::Connected,
            "xps",
            "1 session",
            false,
            cx,
        ))
        .child(
            overview_host_row(
                "drawer-host-alienware",
                HostState::Offline,
                "alienware",
                "Not connected",
                false,
                cx,
            )
            .child(Button::new("drawer-host-connect").label("Connect")),
        )
        .child(rule())
        .child(plus("drawer-host-add", "Add host"));
    let new = list()
        .child(
            div()
                .px(px(12.0))
                .pb(px(10.0))
                .text_size(px(13.0))
                .text_color(muted)
                .child("Opens as a new window in zz on macbook."),
        )
        .children(PANES.map(|(id, icon, title, detail)| {
            overview_choice_row(id, Some(zz_ui::Icon::new(icon)), title, detail, cx)
        }));
    let add_host = v_flex()
        .gap(px(12.0))
        .px(px(16.0))
        .pb(px(16.0))
        .child(
            h_flex()
                .gap(px(8.0))
                .child(div().flex_1().min_w_0().child(Input::new(destination)))
                .child(
                    Button::new("drawer-add-host-connect")
                        .primary()
                        .label("Connect"),
                ),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(muted)
                .child("user@host, or user@host:port. Hosts you are on stay connected."),
        );
    let new_pane = list().children(PANES.map(|(id, icon, title, _)| {
        sheet_row(
            id,
            roles::PHONE_ROW,
            Some(zz_ui::Icon::new(icon)),
            title,
            cx,
        )
    }));
    let tabs = list()
        .children(TABS.iter().enumerate().map(|(ix, (title, host))| {
            sheet_row(
                ("drawer-tab", ix),
                roles::PHONE_ROW,
                (ix == 0).then(|| zz_ui::Icon::new(IconName::Check)),
                *title,
                cx,
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(13.0))
                    .text_color(muted)
                    .child(*host),
            )
        }))
        .child(rule())
        .child(plus("drawer-tab-new", "New tab"));
    let rows: Arc<[_]> = binding_rows().iter().take(5).cloned().collect();
    let bindings = drawer(
        "drawer-bindings",
        "All bindings",
        roles::BINDINGS_DRAWER,
        WhichKeyList::new(rows),
    );
    tiles(
        window,
        vec![
            vec![
                drawer_screen(
                    "drawer-screen-hosts",
                    drawer("drawer-hosts", "Hosts", roles::HOSTS_DRAWER, hosts),
                    cx,
                ),
                drawer_screen(
                    "drawer-screen-new",
                    drawer("drawer-new", "New", roles::NEW_DRAWER, new),
                    cx,
                ),
            ],
            vec![
                drawer_screen(
                    "drawer-screen-add-host",
                    drawer(
                        "drawer-add-host",
                        "Add host",
                        roles::ADD_HOST_DRAWER,
                        add_host,
                    ),
                    cx,
                ),
                drawer_screen(
                    "drawer-screen-new-pane",
                    drawer(
                        "drawer-new-pane",
                        "New pane",
                        roles::NEW_PANE_DRAWER,
                        new_pane,
                    ),
                    cx,
                ),
            ],
            vec![
                drawer_screen(
                    "drawer-screen-tabs",
                    drawer("drawer-tabs", "Tabs", roles::TABS_DRAWER, tabs),
                    cx,
                ),
                drawer_screen("drawer-screen-bindings", bindings, cx),
            ],
        ],
    )
    .into_any_element()
}

const PANES: [(&str, IconName, &str, &str); 2] = [
    (
        "drawer-terminal",
        IconName::SquareTerminal,
        "Terminal",
        "A shell in the current folder",
    ),
    (
        "drawer-browser",
        IconName::Globe,
        "Browser",
        "A web page on macbook, localhost included",
    ),
];

const TABS: [(&str, &str); 3] = [
    ("zz docs", "zzmux.sh"),
    ("localhost:5173", "localhost"),
    ("Issues", "github.com"),
];

fn phone_frame(id: &'static str, cx: &App) -> zz_gpui::Stateful<zz_gpui::Div> {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .w_full()
        .max_w(px(PHONE_WIDTH))
        .overflow_hidden()
        .rounded(theme.radius)
        .framed(cx)
        .bg(theme.background)
}

fn tiles(window: &Window, columns: Vec<Vec<AnyElement>>) -> impl IntoElement {
    let width = window.viewport_size().width.as_f32();
    let count = if width >= WIDE {
        3
    } else if width >= MEDIUM {
        2
    } else {
        1
    }
    .min(columns.len());
    let mut stacks: Vec<Vec<AnyElement>> = (0..count).map(|_| Vec::new()).collect();
    let mut next = 0;
    for (group, column) in columns.into_iter().enumerate() {
        for tile in column {
            let index = if count == 3 { group } else { next % count };
            next += 1;
            stacks[index].push(tile);
        }
    }
    h_flex().w_full().items_start().gap(px(GAP)).children(
        stacks
            .into_iter()
            .map(|stack| v_flex().flex_1().min_w_0().gap(px(GAP)).children(stack)),
    )
}

fn status_bar(cx: &App) -> AnyElement {
    div()
        .id("showcase-status-bar")
        .w_full()
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background)
        .child(
            titlebar(
                &Bar {
                    gaps: crate::pane().gaps,
                    ..Bar::FULL
                },
                cx,
            )
            .child(chat_button(
                workspace_chat_button("showcase-chat"),
                Some(cx.theme().warning),
            )),
        )
        .into_any_element()
}

fn panes(_: &mut Window, cx: &mut App) -> AnyElement {
    let gaps = crate::pane().gaps;
    let tags = TerminalPane::new(4, "cargo test", BUILD)
        .active(true)
        .gaps(gaps)
        .render(
            "showcase-tags",
            vec![
                pane_overlay_stack(
                    PaneOverlayCorner::TopRight,
                    vec![
                        terminal_mode_indicator(Some("Copy mode"), "12 / 340", cx)
                            .into_any_element(),
                        pane_sync_badge(cx).into_any_element(),
                    ],
                )
                .top(cx.theme().pane_header_height() + px(8.0))
                .into_any_element(),
                pane_overlay_stack(
                    PaneOverlayCorner::BottomRight,
                    vec![
                        pane_waiting_state("Waiting for %3", cx).into_any_element(),
                        pane_status_badge(IconName::CircleX, "Dead · exit 1", cx)
                            .into_any_element(),
                    ],
                )
                .into_any_element(),
            ],
            cx,
        );
    let indicators = DisplayPanesState {
        window: WindowId(1),
        duration_ms: 1000,
        indicators: vec![PaneIndicator {
            pane: PaneId(5),
            index: 1,
            select_key: b'1',
            flags: 0,
            label: String::new(),
        }],
        colour: Some(TmuxColour::Theme(7)),
        active_colour: Some(TmuxColour::Theme(6)),
    };
    let logs = TerminalPane::new(5, "tail -f daemon.log", LOGS)
        .gaps(gaps)
        .render(
            "showcase-drop",
            vec![
                pane_indicator_overlay(pane_indicator(
                    "showcase-indicator",
                    &indicators,
                    &indicators.indicators[0],
                    cx,
                ))
                .into_any_element(),
                pane_drop_preview(
                    cx.theme().pane_radius(),
                    cx.theme().pane_edge_width().max(px(1.0)),
                    cx,
                )
                .top(cx.theme().pane_header_height() + px(6.0))
                .bottom(px(6.0))
                .right(px(6.0))
                .w(relative(0.4))
                .into_any_element(),
                div()
                    .absolute()
                    .left(px(16.0))
                    .bottom(px(16.0))
                    .child(pane_drag_chip("%4", "cargo test", cx))
                    .into_any_element(),
            ],
            cx,
        );
    let popup = TerminalPane::new(6, "just site", SERVER).gaps(gaps).render(
        "showcase-popup",
        vec![
            div()
                .absolute()
                .top(cx.theme().pane_header_height() + px(16.0))
                .left(px(16.0))
                .right(px(16.0))
                .bottom(px(16.0))
                .child(
                    FloatingSurface::new(
                        "showcase-floating",
                        terminal_body(SERVER, false, cx).bg(zz_gpui::transparent_black()),
                        cx,
                    )
                    .title("display-popup · lazygit")
                    .content_inset(px(8.0), px(14.0)),
                )
                .into_any_element(),
        ],
        cx,
    );
    let right = pane_split_surface(
        "showcase-panes-right",
        PaneSplitAxis::Horizontal,
        0.5,
        false,
        gaps,
        gap(gaps),
        None,
        None,
        logs,
        popup,
        div().absolute(),
        cx,
    );
    let layout = pane_split_surface(
        "showcase-panes-split",
        PaneSplitAxis::Horizontal,
        0.34,
        false,
        gaps,
        gap(gaps),
        None,
        None,
        tags,
        right,
        div().absolute(),
        cx,
    );
    div()
        .id("showcase-panes")
        .w_full()
        .h(px(PANES_HEIGHT))
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background)
        .p(gap(gaps))
        .child(layout)
        .into_any_element()
}

fn start(window: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    tiles(
        window,
        vec![
            vec![empty_session(cx)],
            vec![first_run(cx)],
            vec![new_pane(cx)],
        ],
    )
    .into_any_element()
}

fn empty_session(cx: &App) -> AnyElement {
    let key = |key: &str| Keystroke::parse(key).ok();
    let actions = [
        empty_workspace_action(
            "showcase-new-session",
            key("enter"),
            "New session",
            true,
            true,
            cx,
        )
        .into_any_element(),
        empty_workspace_action(
            "showcase-open-settings",
            key("cmd-,"),
            "Settings",
            false,
            false,
            cx,
        )
        .into_any_element(),
        empty_workspace_caption("In a session", cx).into_any_element(),
    ];
    let hints = SESSION_KEYS.iter().map(|(chord, label)| {
        empty_workspace_hint(key(PREFIX).into_iter().chain(key(chord)), *label, cx)
            .into_any_element()
    });
    window_tile(
        "showcase-empty-session",
        empty_workspace(
            "showcase-empty-workspace",
            empty_workspace_keys(actions.into_iter().chain(hints)),
            cx,
        ),
        cx,
    )
}

fn first_run(cx: &App) -> AnyElement {
    window_tile(
        "showcase-first-run",
        empty_workspace(
            "showcase-first-run-workspace",
            empty_workspace_connect(
                Button::new("showcase-add-host")
                    .primary()
                    .label("Add host…"),
                cx,
            ),
            cx,
        ),
        cx,
    )
}

fn new_pane(cx: &App) -> AnyElement {
    let gaps = crate::pane().gaps;
    let rows = PANE_CHOICES
        .iter()
        .enumerate()
        .map(|(index, (title, icon, shortcut))| {
            pane_picker_row(
                ("showcase-pane-choice", index),
                title,
                icon.clone(),
                shortcut,
                index == 0,
                true,
                cx,
            )
            .into_any_element()
        })
        .collect::<Vec<_>>();
    let pane = pane_surface(
        "showcase-new-pane-surface",
        pane_picker_surface("showcase-new-pane-picker", radii(gaps, cx), rows, cx),
        Vec::new(),
        chrome(true, false, gaps, cx),
        cx,
    );
    window_tile(
        "showcase-new-pane",
        div().size_full().p(gap(gaps)).child(pane),
        cx,
    )
}

fn window_tile(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .id(id)
        .w_full()
        .h(px(START_HEIGHT))
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background)
        .child(content)
        .into_any_element()
}

fn reading(cx: &App) -> AnyElement {
    let style = TextViewStyle {
        highlight_theme: Arc::clone(&cx.theme().highlight_theme),
        is_dark: cx.theme().is_dark(),
        ..TextViewStyle::default()
    }
    .code_role(zz_ui::roles::CODE)
    .figure_role(zz_ui::roles::FIGURE);
    card("showcase-reading", cx)
        .child(heading(
            "Reading",
            "Code and a table in an agent's answer.",
            cx,
        ))
        .child(TextView::markdown("showcase-markdown", READING).style(style))
        .into_any_element()
}

fn start_page(address: &Entity<InputState>, window: &Window, cx: &App) -> AnyElement {
    let recents = v_flex().w_full().max_w(px(360.0)).gap(px(4.0)).children(
        RECENT
            .into_iter()
            .enumerate()
            .map(|(index, url)| browser_recent_row(("showcase-recent", index), url, None, cx)),
    );
    v_flex()
        .id("showcase-start-page")
        .w_full()
        .h(BrowserHeader::HEIGHT + px(START_PAGE_HEIGHT))
        .overflow_hidden()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background.opaque())
        .child(browser_header(true, 2, 1, address, false, window, cx))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(browser_start_surface(recents)),
        )
        .into_any_element()
}

fn buttons(cx: &App) -> AnyElement {
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

fn dialog(cx: &App) -> AnyElement {
    dialog_surface(cx)
        .w_full()
        .max_w(px(360.0))
        .child(dialog_gutter().child(dialog_title().child("Kill session \"work\"?")))
        .child(
            dialog_gutter().child(
                dialog_description(cx).child(
                    "Its 3 windows and 7 panes stop, and their scrollback is gone for good.",
                ),
            ),
        )
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

fn agent(cx: &App) -> AnyElement {
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
        .child(
            h_flex()
                .gap_2()
                .child(agent_jump_to_bottom_button("showcase-jump", cx))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.muted())
                        .child("Jump to latest"),
                ),
        )
        .into_any_element()
}

fn card(id: &'static str, cx: &App) -> zz_gpui::Stateful<zz_gpui::Div> {
    v_flex()
        .id(id)
        .w_full()
        .p(px(16.0))
        .gap_3()
        .rounded(cx.theme().radius)
        .framed(cx)
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
        .framed(cx)
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
