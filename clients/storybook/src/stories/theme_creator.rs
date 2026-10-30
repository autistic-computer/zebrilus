use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, ElementId, Entity, Focusable as _,
    IntoElement, Keystroke, ParentElement as _, Render, SharedString, Styled as _, Window, div,
    prelude::*, px, relative, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, Elevation, Emphasis, IconName, IndexPath, Tone,
    badge::Badge,
    button::{Button, ButtonVariants as _},
    compact::{
        PillLabel, compact_bar_button, compact_floating_bar, compact_floating_pill, compact_hud,
    },
    h_flex,
    input::{Input, InputState, NumberInput},
    kbd::Kbd,
    menu::PopupMenu,
    notification::Notification,
    overlay::{dialog_description, dialog_footer, dialog_gutter, dialog_surface, dialog_title},
    select::{Select, SelectState},
    settings::appearance::{picker_tile, theme_preview},
    sheet::{BottomSheet, bottom_sheet, sheet_close, sheet_row},
    slider::DiscreteSlider,
    switch::Switch,
    text::{TextView, TextViewStyle},
    tooltip::Tooltip,
    touch::{ArrowDirection, ToolKeys, WhichKeyList, arrow_pad_bubble, page_dots},
    v_flex,
    which_key::WhichKeyView,
};
use zz_protocol::{DisplayPanesState, PaneId, PaneIndicator, TmuxColour, WindowId};
use zz_terminal::{SearchQuery, SearchStatus};
use zz_ui::{
    agent::agent_jump_to_bottom_button,
    browser::{BrowserHeader, browser_recent_row, browser_start_surface},
    command::{
        CommandPaletteSurface, PaletteHint, PalettePill, PaletteRow, PaletteStatus,
        command_palette_entry, command_palette_row_height, command_palette_section,
        unified_command_palette_input,
    },
    compact::overview_count,
    interface_theme::{ThemeModeSetting, pinned_theme_mode},
    pane::{
        FloatingSurface, PaneOverlayCorner, PaneSplitAxis, pane_drag_chip, pane_drop_preview,
        pane_indicator, pane_indicator_overlay, pane_overlay_stack, pane_picker_row,
        pane_picker_surface, pane_split_surface, pane_status_badge, pane_surface, pane_sync_badge,
        pane_waiting_state, terminal_mode_indicator,
    },
    pip::PipSurface,
    settings::{SettingEntry, SettingsStack, panes_preview::PanesPreview, settings_card_ground},
    shell::{
        app_shell_surface, app_workspace_surface, empty_workspace, empty_workspace_action,
        empty_workspace_caption, empty_workspace_connect, empty_workspace_hint,
        empty_workspace_keys,
    },
    terminal::terminal_find_bar,
};

use super::{
    agent::{
        cards::{COMMAND, permission},
        lens,
    },
    console::{
        chooser::confirm_prompt,
        phone::{
            binding_rows, groups, host_bar, host_page, hosts_bar, hosts_page, key_row, placement,
            tool_keys,
        },
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
use penna::StyledExt as _;

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
            summary: "The window with the tree sidebar, and the title bar as it reads with the sidebar hidden.",
            build: Workspace::view,
        },
        Section {
            id: "flat",
            name: "Flat sidebar",
            summary: "The flat sidebar beside Claude Code with its lens open and two terminals. The keyboard cursor sits on a window, and a host that is not connected offers Connect.",
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
            summary: "The find bar, badges, the drop preview, the drag chip and a popup over panes.",
            build: |window, cx| {
                stateful(
                    window,
                    cx,
                    |window, cx| cx.new(|cx| InputState::new(window, cx).default_value("error")),
                    panes,
                )
            },
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
            summary: "The workspace before its first session, the first run with no host, a new split asking what it should become, and a new chat asking which agent to start and where.",
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
            summary: "Menus, the palette, a dialog, toasts and which-key over the backdrop.",
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
            summary: "The phone: a terminal page with a HUD, the key row's prefix menu and arrow pad, the bindings list, the sessions sheet, a host's windows and the Hosts page with its Add host tile.",
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
    "github.com/autistic-computer/zebrilus/pull/412",
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
        let flat = cx.new(|_| Live::new(None).cursor_and_offline());
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
            terminal_body(BUILD, false, cx).bg(ala::transparent_black()),
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
        let light = active.colors(penna::ThemeMode::Light);
        let dark = active.colors(penna::ThemeMode::Dark);
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
                    .child(SettingEntry::new("Mode").child(tiles))
                    .child(
                        SettingEntry::new("Navigation layout").control(
                            Select::new(&self.layout)
                                .title("Navigation layout")
                                .on(settings_card_ground(cx)),
                        ),
                    )
                    .child(
                        SettingEntry::new("Pane gaps")
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
                vec![self.preferences(cx), panes_preview()],
                vec![buttons(cx), self.settings(cx)],
            ],
        )
    }
}

struct Floating {
    pane_menu: Entity<PopupMenu>,
    view_menu: Entity<PopupMenu>,
    search: Entity<InputState>,
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
        cx.new(|_| Self {
            pane_menu,
            view_menu,
            search,
            toasts,
            tooltip,
        })
        .into()
    }

    fn palette(&self, cx: &App) -> AnyElement {
        let row = |label: &str, detail: &str, right: &str, running: bool| PaletteRow {
            label: label.to_owned().into(),
            muted_prefix: label.rfind(" › ").map_or(0, |at| at + " › ".len()),
            detail: detail.to_owned().into(),
            right: right.to_owned().into(),
            icon: Some(IconName::SquareTerminal),
            running,
            ..Default::default()
        };
        let rows = [
            row("zz › 1 editor › 0 nvim", "Terminal", "current", false),
            row("zz › 2 agents › 0 claude", "Agent · running", "", true),
            row("logs › 1 tail › 0 tail -f", "Terminal", "", false),
        ];
        let height = px(command_palette_row_height());
        CommandPaletteSurface::new(
            unified_command_palette_input(
                &self.search,
                [
                    PalettePill::Host {
                        label: "devbox".into(),
                        status: PaletteStatus::Online,
                    },
                    PalettePill::Verb("read pane".into()),
                ],
                cx,
            ),
            1,
        )
        .usage("runs  capture-pane -p -t zz:1.0")
        .rows(
            v_flex()
                .child(
                    div()
                        .h(height)
                        .child(command_palette_section("Panes on devbox", "", cx)),
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
                key: "enter",
                label: "pick",
            },
            PaletteHint {
                key: "backspace",
                label: "back",
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
        let chooser = plate(
            "showcase-chooser",
            confirm_prompt("showcase-chooser-modal", cx),
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
                vec![vec![menus], vec![view, toasts], vec![palette, dialog]],
            ))
            .child(chooser)
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
            menu.prefix.update(cx, |key, cx| key.set_armed(true, cx));
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
        phone_frame("showcase-phone-overview", cx)
            .child(host_page(cx))
            .child(host_bar(
                "showcase-overview-bar",
                "macbook",
                "2 sessions",
                0,
                cx,
            ))
            .into_any_element()
    }

    fn hosts(cx: &App) -> AnyElement {
        phone_frame("showcase-phone-hosts", cx)
            .child(hosts_page(cx))
            .child(hosts_bar(cx))
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
                    Some(penna::Icon::new(IconName::Layers)),
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
                        penna::Icon::new(IconName::Check)
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
                    penna::Edge::Top,
                    cx,
                ))
                .child(sheet_row(
                    "showcase-new-session",
                    zz_ui::roles::PHONE_ROW,
                    Some(penna::Icon::new(IconName::Plus)),
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
                vec![Self::overview(cx), Self::hosts(cx)],
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
    role: penna::SurfaceRole,
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
            .rule_on(roles::LIST_RULE, penna::Edge::Top, cx)
    };
    let plus = |id: &'static str, label: &'static str| {
        sheet_row(
            id,
            roles::PHONE_ROW,
            Some(penna::Icon::new(IconName::Plus)),
            label,
            cx,
        )
    };
    let hosts = list()
        .child(
            v_flex()
                .gap(px(4.0))
                .px(px(12.0))
                .pb(px(10.0))
                .text_size(px(13.0))
                .child(div().text_color(muted).child("ssh://fabri@alienware"))
                .child(
                    div()
                        .text_color(cx.theme().warning)
                        .child("ssh: connect to host alienware port 22: Connection refused"),
                ),
        )
        .child(sheet_row(
            "drawer-host-connect",
            roles::PHONE_ROW,
            Some(penna::Icon::new(IconName::Server)),
            "Connect",
            cx,
        ))
        .child(
            sheet_row(
                "drawer-host-remove",
                roles::PHONE_ROW,
                Some(penna::Icon::new(IconName::Xmark)),
                "Remove host",
                cx,
            )
            .text_color(cx.theme().danger),
        );
    let note = |text: &'static str| {
        div()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(muted)
            .child(text)
    };
    let add_host = v_flex()
        .gap(px(10.0))
        .px(px(16.0))
        .pb(px(16.0))
        .child(Input::new(destination))
        .child(note("user@host, or user@host:port"))
        .child(
            Button::new("drawer-add-host-connect")
                .primary()
                .w_full()
                .label("Connect"),
        )
        .child(
            div()
                .my(px(6.0))
                .rule_on(roles::LIST_RULE, penna::Edge::Top, cx),
        )
        .child(note(
            "Add this phone's key to authorized_keys on the host to sign in without a password.",
        ))
        .child(
            Button::new("drawer-add-host-key")
                .w_full()
                .icon(IconName::Copy)
                .label("Copy SSH key"),
        );
    let new_pane = list().children(PANES.map(|(id, icon, title, _)| {
        sheet_row(
            id,
            roles::PHONE_ROW,
            Some(penna::Icon::new(icon)),
            title,
            cx,
        )
    }));
    let tabs = list()
        .children(TABS.iter().enumerate().map(|(ix, (title, host))| {
            sheet_row(
                ("drawer-tab", ix),
                roles::PHONE_ROW,
                (ix == 0).then(|| penna::Icon::new(IconName::Check)),
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
                    drawer("drawer-hosts", "alienware", roles::HOSTS_DRAWER, hosts),
                    cx,
                ),
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
            ],
            vec![
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
                drawer_screen(
                    "drawer-screen-tabs",
                    drawer("drawer-tabs", "Tabs", roles::TABS_DRAWER, tabs),
                    cx,
                ),
            ],
            vec![drawer_screen("drawer-screen-bindings", bindings, cx)],
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

fn phone_frame(id: &'static str, cx: &App) -> ala::Stateful<ala::Div> {
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
        .child(titlebar(
            &Bar {
                gaps: crate::pane().gaps,
                ..Bar::FULL
            },
            cx,
        ))
        .into_any_element()
}

fn panes(find: &Entity<InputState>, _: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
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
                        terminal_find_bar(
                            find,
                            &SearchQuery::literal("error"),
                            Some(SearchStatus::new(2, 7)),
                            |_, _, _| {},
                            cx,
                        )
                        .into_any_element(),
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
                        terminal_body(SERVER, false, cx).bg(ala::transparent_black()),
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
        0.44,
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
            vec![new_pane(cx), new_agent(cx)],
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

fn new_agent(cx: &App) -> AnyElement {
    let gaps = crate::pane().gaps;
    let picker = crate::stories::workspace::panes::chat_picker_surface(
        "showcase-new-agent-picker",
        &zz_client::new_chat::ChatPicker::default(),
        true,
        cx,
    );
    let pane = pane_surface(
        "showcase-new-agent-surface",
        picker,
        Vec::new(),
        chrome(true, false, gaps, cx),
        cx,
    );
    window_tile(
        "showcase-new-agent",
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

fn panes_preview() -> AnyElement {
    let pane = crate::pane();
    div()
        .id("showcase-panes-preview")
        .w_full()
        .child(PanesPreview {
            gaps: pane.gaps,
            margin: pane.margin,
            inactive_opacity: pane.inactive,
        })
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

fn card(id: &'static str, cx: &App) -> ala::Stateful<ala::Div> {
    v_flex()
        .id(id)
        .w_full()
        .p(px(16.0))
        .gap_3()
        .rounded(cx.theme().radius)
        .framed(cx)
        .bg(cx.theme().background)
}

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
                .font_weight(ala::FontWeight::SEMIBOLD)
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
