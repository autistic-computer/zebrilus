use std::sync::Arc;

use ala::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, IntoElement, Keystroke,
    ParentElement as _, Render, Styled as _, Window, div, prelude::*, px, size,
};
use penna::{
    ActiveTheme as _, Colorize as _, Edge, Elevation, Icon, IconName, IndexPath, StyledExt as _,
    Surface,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPicker, ColorPickerState},
    compact::{
        BENTO_INSET, BentoSpan, PillLabel, PillPlacement, bento_add_tile, bento_grid, bento_tile,
        compact_bar_button, compact_floating_bar, compact_floating_group, compact_floating_pill,
        compact_hud,
    },
    feedback::{alert_actions, inline_alert},
    h_flex,
    input::{Input, InputState, NumberInput},
    interface_theme::{ThemeOverrides, builtin_themes},
    menu::{PopupMenu, PopupMenuItem},
    picker::{path_row, picker_footer, picker_header, picker_list, picker_search},
    roles,
    select::{Select, SelectState},
    settings::{
        SETTINGS_PAGE_PADDING, SettingEntry, SettingsStack,
        appearance::{
            SWATCH, THEME_MODES, ThemePicks, accent_swatches, contrast_slider,
            corner_radius_slider, picker_tile, theme_mode_control, theme_picker, theme_preview,
            tint_hue_slider, tint_slider,
        },
        large_title, settings_card_ground, settings_navigation_button,
        settings_navigation_group_label, settings_page_content,
    },
    sheet::{bottom_sheet, sheet_row},
    shell::{app_shell_surface, app_titlebar_strip},
    sidebar::{chrome_button, sidebar_footer, sidebar_panel, sidebar_titlebar},
    switch::Switch,
    text::{TextView, TextViewStyle},
    touch::{ArrowDirection, KeyRow, PageDot, ToolKeys, WhichKeyList, arrow_pad_bubble, page_dots},
    v_flex,
    which_key::{WhichKeyCap, WhichKeyHeader, WhichKeyRow, WhichKeyView},
};

use super::{
    GAP, MEDIUM, PHONE_WIDTH, PLATE_PADDING, Preferences, READING, SAFE_BOTTOM, SCREEN_RADIUS,
    Toasts, WIDE, buttons, card, dialog, heading, labeled, over_backdrop, phone_frame, tiles,
};
use crate::kit::menus::{bind_keys, pane_menu, view_menu};
use crate::story::{Framed as _, Section, stateless};

pub const WINDOW: Section = Section {
    id: "window",
    name: "Window",
    summary: "An app window: the sidebar with its buttons, navigation and footer, the title bar, and a settings page with the appearance rows.",
    build: AppWindow::view,
};

pub const CONTROLS: Section = Section {
    id: "controls",
    name: "Controls",
    summary: "A form, switches, buttons and a settings card.",
    build: Controls::view,
};

pub const FLOATING: Section = Section {
    id: "floating",
    name: "Floating",
    summary: "Menus, a picker, a dialog, toasts and which-key over the backdrop.",
    build: Floating::view,
};

pub const CONTENT: Section = Section {
    id: "content",
    name: "Content",
    summary: "Markdown with code and a table, and an alert with its actions.",
    build: |_, cx| stateless(content, cx),
};

pub const PHONE: Section = Section {
    id: "phone",
    name: "Phone",
    summary: "The phone chrome: a page with the floating pill and a HUD, the key row with its prefix armed and the arrow pad, a bindings list, a sheet and a grid page with its add tile.",
    build: Phone::view,
};

const WINDOW_HEIGHT: f32 = 500.0;
const SIDEBAR_WIDTH: f32 = 220.0;
const WHICH_KEY_HEIGHT: f32 = 260.0;
const PICKER_WIDTH: f32 = 360.0;
const PHONE_PAGE_HEIGHT: f32 = 190.0;
const KEYS_PAGE_HEIGHT: f32 = 470.0;
const PHONE_SHEET_HEIGHT: f32 = 420.0;
const KINDS: [&str; 3] = ["SSH", "HTTPS", "Local"];
const DENSITY: [&str; 3] = ["Compact", "Regular", "Roomy"];
const NAVIGATION: [(&str, IconName); 5] = [
    ("General", IconName::Settings),
    ("Appearance", IconName::Palette),
    ("Keyboard", IconName::Keyboard),
    ("Notifications", IconName::Bell),
    ("About", IconName::Info),
];
const FILES: [(IconName, &str); 4] = [
    (IconName::Folder, "src"),
    (IconName::File, "src/main.rs"),
    (IconName::File, "Cargo.toml"),
    (IconName::File, "README.md"),
];
const PHONE_LINES: [&str; 3] = [
    "~/dev/app main* > make test",
    "ok: 214 passed, 0 failed",
    "~/dev/app main* > ",
];
const WINDOWS: [(&str, &str, bool); 3] = [
    ("work", "3 tabs", true),
    ("scratch", "1 tab", false),
    ("logs", "2 tabs", false),
];
const LIBRARY: [(&str, &str, BentoSpan); 3] = [
    ("Inbox", "12 unread", BentoSpan::new(2, 2)),
    ("Drafts", "3 notes", BentoSpan::new(2, 1)),
    ("Archive", "Synced", BentoSpan::new(2, 1)),
];

struct AppWindow {
    accent: Entity<ColorPickerState>,
}

impl AppWindow {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let accent = cx.new(|cx| ColorPickerState::new(None, window, cx));
        cx.new(|_| Self { accent }).into()
    }

    fn sidebar(cx: &App) -> AnyElement {
        let ground = cx.theme().ground();
        let controls = h_flex()
            .gap_1()
            .child(
                chrome_button("creator-sidebar-hide", IconName::PanelLeft, "Hide sidebar")
                    .on(ground),
            )
            .child(chrome_button("creator-sidebar-new", IconName::Plus, "New window").on(ground));
        let navigation = v_flex()
            .id("creator-navigation")
            .w_full()
            .gap(px(2.0))
            .px(px(6.0))
            .child(settings_navigation_group_label("Settings", cx))
            .children(NAVIGATION.iter().enumerate().map(|(index, (title, icon))| {
                settings_navigation_button(
                    ("creator-navigation-row", index),
                    *title,
                    icon.clone(),
                    index == 1,
                    cx,
                )
            }));
        sidebar_panel(
            "creator-sidebar",
            SIDEBAR_WIDTH,
            sidebar_titlebar("creator-sidebar-titlebar", controls, cx),
            navigation,
            ground,
            true,
            cx,
        )
        .child(sidebar_footer(
            "creator-sidebar-footer",
            chrome_button("creator-sidebar-settings", IconName::Settings, "Settings").on(ground),
            None,
            cx,
        ))
        .into_any_element()
    }

    fn page(&self, cx: &App) -> AnyElement {
        let knobs = crate::knobs();
        let active = knobs.interface_theme();
        let overrides = ThemeOverrides {
            tint: Some(knobs.tint),
            tint_hue: Some(knobs.tint_hue),
            ..ThemeOverrides::default()
        };
        let themes = builtin_themes();
        let picks = ThemePicks {
            themes: themes
                .iter()
                .map(|(id, theme)| ((*id).into(), theme))
                .collect(),
            selected: themes
                .iter()
                .position(|(id, _)| *id == knobs.interface)
                .unwrap_or(0),
            overrides: &overrides,
        };
        let custom = ColorPicker::new(&self.accent, ala::transparent_black())
            .label("Accent")
            .swatch(px(SWATCH));
        div()
            .id("creator-page")
            .size_full()
            .overflow_hidden()
            .p(px(SETTINGS_PAGE_PADDING))
            .child(
                settings_page_content()
                    .gap(px(GAP))
                    .child(
                        SettingsStack::titled("Theme")
                            .child(SettingEntry::new("Mode").control(theme_mode_control(
                                knobs.theme,
                                |_, _, _| {},
                                cx,
                            )))
                            .child(SettingEntry::new("Theme").control(theme_picker(
                                &picks,
                                |_, _, _| {},
                                |menu, _, _| menu.item(PopupMenuItem::new("Import from clipboard")),
                                cx,
                            )))
                            .child(SettingEntry::new("Accent").control(accent_swatches(
                                &picks,
                                cx.theme().mode,
                                custom,
                                |_, _, _| {},
                                cx,
                            )))
                            .child(SettingEntry::new("Tint").control(tint_hue_slider(
                                knobs.tint_hue,
                                |_, _, _| {},
                                cx,
                            )))
                            .child(SettingEntry::new("Tint strength").control(tint_slider(
                                knobs.tint,
                                |_, _, _| {},
                                cx,
                            ))),
                    )
                    .child(
                        SettingsStack::titled("Advanced")
                            .child(SettingEntry::new("Corner radius").control(
                                corner_radius_slider(active.materials.radius, |_, _, _| {}, cx),
                            ))
                            .child(SettingEntry::new("Contrast").control(contrast_slider(
                                knobs.contrast,
                                |_, _, _| {},
                                cx,
                            ))),
                    ),
            )
            .into_any_element()
    }
}

impl Render for AppWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let titlebar = app_titlebar_strip(
            "creator-titlebar",
            div()
                .pl_3()
                .text_sm()
                .font_weight(ala::FontWeight::MEDIUM)
                .child("Appearance"),
            h_flex().pr_2().child(
                Button::new("creator-titlebar-search")
                    .ghost()
                    .icon(IconName::Search),
            ),
            cx,
        )
        .surface(Surface::new(roles::TITLEBAR), cx)
        .into_any_element();
        div()
            .id("creator-window")
            .w_full()
            .h(px(WINDOW_HEIGHT))
            .overflow_hidden()
            .rounded(theme.radius)
            .framed(cx)
            .child(app_shell_surface(
                "creator-window-shell",
                theme.background,
                Self::sidebar(cx),
                Some(titlebar),
                self.page(cx),
                Vec::new(),
            ))
    }
}

struct Controls {
    host: Entity<InputState>,
    user: Entity<InputState>,
    port: Entity<InputState>,
    kind: Entity<SelectState<Vec<&'static str>>>,
    density: Entity<SelectState<Vec<&'static str>>>,
    preferences: Entity<Preferences>,
}

impl Controls {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        let field = |value: &'static str, window: &mut Window, cx: &mut App| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let host = field("build.example.com", window, cx);
        let user = field("deploy", window, cx);
        let port = cx.new(|cx| {
            InputState::new(window, cx)
                .step(1.0)
                .min(1.0)
                .max(65535.0)
                .default_value("22")
        });
        let kind =
            cx.new(|cx| SelectState::new(KINDS.to_vec(), Some(IndexPath::new(0)), window, cx));
        let density =
            cx.new(|cx| SelectState::new(DENSITY.to_vec(), Some(IndexPath::new(1)), window, cx));
        let preferences = Preferences::view(cx);
        cx.new(|_| Self {
            host,
            user,
            port,
            kind,
            density,
            preferences,
        })
        .into()
    }

    fn connection(&self, cx: &App) -> AnyElement {
        card("creator-connection", cx)
            .child(heading(
                "New connection",
                "Saved connections show in the sidebar.",
                cx,
            ))
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
            .child(labeled("Kind", Select::new(&self.kind), cx))
            .child(
                h_flex()
                    .pt_1()
                    .gap_2()
                    .justify_end()
                    .child(Button::new("creator-connection-cancel").label("Cancel"))
                    .child(
                        Button::new("creator-connection-save")
                            .primary()
                            .label("Connect"),
                    ),
            )
            .into_any_element()
    }

    fn settings(&self, cx: &App) -> AnyElement {
        let active = crate::knobs().interface_theme();
        let light = active.colors(penna::ThemeMode::Light);
        let dark = active.colors(penna::ThemeMode::Dark);
        let modes = h_flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(
                THEME_MODES
                    .iter()
                    .enumerate()
                    .map(|(index, (mode, title))| {
                        picker_tile(
                            format!("creator-mode-{index}").into(),
                            *title,
                            theme_preview(*mode, &light, &dark, cx),
                            mode.is_none(),
                            cx,
                        )
                    }),
            );
        div()
            .id("creator-settings")
            .w_full()
            .child(
                SettingsStack::titled("Interface")
                    .child(SettingEntry::new("Mode").child(modes))
                    .child(
                        SettingEntry::new("Density").control(
                            Select::new(&self.density)
                                .title("Density")
                                .on(settings_card_ground(cx)),
                        ),
                    )
                    .child(
                        SettingEntry::new("Compact sidebar")
                            .control(Switch::new("creator-compact-sidebar").checked(true)),
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
                vec![self.connection(cx)],
                vec![self.preferences.clone().into_any_element()],
                vec![buttons(cx), self.settings(cx)],
            ],
        )
    }
}

struct Floating {
    pane_menu: Entity<PopupMenu>,
    view_menu: Entity<PopupMenu>,
    search: Entity<InputState>,
    toasts: Toasts,
}

impl Floating {
    fn view(window: &mut Window, cx: &mut App) -> AnyView {
        bind_keys(cx);
        let pane_menu = PopupMenu::build(window, cx, pane_menu);
        let view_menu = PopupMenu::build(window, cx, view_menu);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Open a file"));
        let toasts = Toasts::new(cx);
        cx.new(|_| Self {
            pane_menu,
            view_menu,
            search,
            toasts,
        })
        .into()
    }

    fn picker(&self, cx: &App) -> AnyElement {
        v_flex()
            .id("creator-picker-surface")
            .w_full()
            .max_w(px(PICKER_WIDTH))
            .overflow_hidden()
            .surface(roles::PICKER, cx)
            .child(picker_header(cx).child(picker_search(&self.search, cx)))
            .child(
                picker_list().children(FILES.iter().enumerate().map(|(index, (icon, path))| {
                    path_row(
                        ("creator-picker-row", index),
                        icon.clone(),
                        *path,
                        index == 1,
                        cx,
                    )
                })),
            )
            .child(picker_footer(cx).child("4 of 128 files"))
            .into_any_element()
    }
}

impl Render for Floating {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width.as_f32();
        let menus = over_backdrop("creator-menus", self.pane_menu.clone(), cx);
        let view = over_backdrop("creator-view-menu", self.view_menu.clone(), cx);
        let toasts = over_backdrop("creator-toasts", self.toasts.stack(), cx);
        let picker = over_backdrop("creator-picker", self.picker(cx), cx);
        let dialog = over_backdrop("creator-dialog", dialog(cx), cx);
        let which_key = over_backdrop(
            "creator-which-key",
            WhichKeyView::new(header(), bindings()).fit(size(
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
                vec![vec![menus], vec![view, toasts], vec![picker, dialog]],
            ))
            .child(which_key)
    }
}

fn content(window: &mut Window, cx: &mut App) -> AnyElement {
    let cx: &App = cx;
    tiles(window, vec![vec![reading(cx)], vec![alert(cx)]]).into_any_element()
}

fn reading(cx: &App) -> AnyElement {
    let style = TextViewStyle {
        highlight_theme: Arc::clone(&cx.theme().highlight_theme),
        is_dark: cx.theme().is_dark(),
        ..TextViewStyle::default()
    };
    card("creator-reading", cx)
        .child(heading("Reading", "Code and a table in a message.", cx))
        .child(TextView::markdown("creator-markdown", READING).style(style))
        .into_any_element()
}

fn alert(cx: &App) -> AnyElement {
    card("creator-alert", cx)
        .child(heading(
            "Alert",
            "A problem shown in place, with its actions.",
            cx,
        ))
        .child(
            inline_alert(
                Some("Sync stopped".into()),
                "The server closed the connection. Changes stay on this device until it is back.",
                cx,
            )
            .child(
                alert_actions(cx)
                    .child(Button::new("creator-alert-later").label("Later"))
                    .child(
                        Button::new("creator-alert-retry")
                            .primary()
                            .label("Try again"),
                    ),
            ),
        )
        .into_any_element()
}

fn cap(keys: &[&str], raw: &str) -> WhichKeyCap {
    WhichKeyCap {
        keys: keys
            .iter()
            .filter_map(|key| Keystroke::parse(key).ok())
            .collect(),
        raw: raw.to_owned().into(),
        yours: false,
    }
}

fn binding(key: &str, raw: &str, label: &str, group: &str, yours: bool) -> WhichKeyRow {
    WhichKeyRow {
        id: key.to_owned().into(),
        caps: vec![WhichKeyCap {
            yours,
            ..cap(&[key], raw)
        }],
        label: label.to_owned().into(),
        group: Some(group.to_owned().into()),
        repeat: false,
    }
}

fn header() -> WhichKeyHeader {
    WhichKeyHeader {
        table: "leader".into(),
        prefix: Keystroke::parse("ctrl-k").ok(),
        prefix_raw: "Ctrl+K".into(),
        more: Some(cap(&["?"], "?")),
    }
}

fn bindings() -> Arc<[WhichKeyRow]> {
    [
        binding("n", "N", "New window", "Windows", false),
        binding("w", "W", "Close window", "Windows", false),
        binding("m", "M", "Minimize", "Windows", false),
        binding("t", "T", "New tab", "Tabs", false),
        binding("[", "[", "Previous tab", "Tabs", false),
        binding("]", "]", "Next tab", "Tabs", false),
        binding("r", "R", "Rename tab", "Tabs", false),
        binding("f", "F", "Find", "Edit", false),
        binding("c", "C", "Copy path", "Edit", false),
        binding("v", "V", "Paste", "Edit", false),
        binding("g", "G", "Open the log", "Yours", true),
        binding("p", "P", "Pin the tab", "Yours", true),
        binding("s", "S", "Settings", "Other", false),
        binding("?", "?", "Every binding", "Other", false),
    ]
    .into()
}

fn dot(active: bool, attention: bool) -> PageDot {
    PageDot { active, attention }
}

fn groups() -> Vec<Vec<PageDot>> {
    vec![
        vec![dot(false, false), dot(true, false)],
        vec![dot(false, false), dot(false, true), dot(false, false)],
        vec![dot(false, false)],
    ]
}

fn placement() -> PillPlacement {
    PillPlacement::new(Some(px(SCREEN_RADIUS)), px(SAFE_BOTTOM))
}

fn key_row(keys: &ToolKeys) -> KeyRow {
    KeyRow::new(keys.row())
        .lead(keys.lead())
        .track_scroll(&keys.scroll)
        .on_key(|_, _, _| {})
        .on_hide(|_, _| {})
}

struct Phone {
    keys: ToolKeys,
    menu: ToolKeys,
}

impl Phone {
    fn view(_: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|cx| {
            let keys = ToolKeys::new(cx);
            let menu = ToolKeys::new(cx);
            menu.prefix.update(cx, |key, cx| key.set_armed(true, cx));
            Self { keys, menu }
        })
        .into()
    }

    fn page(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let placement = placement();
        let pill = compact_floating_pill(
            "creator-phone-pill",
            PillLabel {
                icon: IconName::SquareTerminal,
                title: "make test".into(),
                meta: "work · build".into(),
                warning: false,
            },
            page_dots(&groups(), cx),
            &placement,
            cx,
        );
        phone_frame("creator-phone-page", cx)
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
            .child(div().id("creator-phone-keys").child(key_row(&self.keys)))
            .child(compact_floating_bar(pill, &placement, cx))
            .into_any_element()
    }

    fn keys(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        phone_frame("creator-phone-menu", cx)
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
                    .id("creator-phone-menu-keys")
                    .child(key_row(&self.menu)),
            )
            .into_any_element()
    }

    fn bindings(cx: &App) -> AnyElement {
        let rows: Arc<[_]> = bindings().iter().take(4).cloned().collect();
        phone_frame("creator-phone-bindings", cx)
            .child(WhichKeyList::new(rows))
            .into_any_element()
    }

    fn sheet(cx: &App) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.foreground.muted();
        let rows = WINDOWS
            .iter()
            .enumerate()
            .map(|(index, (name, detail, current))| {
                sheet_row(
                    ("creator-window-option", index),
                    roles::PHONE_ROW,
                    Some(Icon::new(IconName::Layers)),
                    *name,
                    cx,
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(px(13.0))
                        .text_color(muted)
                        .child(*detail),
                )
                .when(*current, |row| {
                    row.child(
                        Icon::new(IconName::Check)
                            .size(px(18.0))
                            .text_color(theme.accent),
                    )
                })
            });
        let sheet = bottom_sheet(
            "creator-windows-sheet",
            "Windows",
            [compact_bar_button("creator-windows-close", IconName::Xmark).into_any_element()],
            v_flex()
                .list_inset(cx)
                .pb(px(8.0))
                .children(rows)
                .child(
                    div()
                        .mx(px(12.0))
                        .my(px(4.0))
                        .rule_on(roles::LIST_RULE, Edge::Top, cx),
                )
                .child(sheet_row(
                    "creator-new-window",
                    roles::PHONE_ROW,
                    Some(Icon::new(IconName::Plus)),
                    "New window",
                    cx,
                )),
            px(0.0),
            |_, _| {},
        )
        .header_role(roles::PHONE_BAR);
        phone_frame("creator-phone-sheet", cx)
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

    fn grid(window: &Window, cx: &App) -> AnyElement {
        let viewport = window.viewport_size().width.as_f32();
        let columns = if viewport >= WIDE {
            3.0
        } else if viewport >= MEDIUM {
            2.0
        } else {
            1.0
        };
        let column = (viewport - 2.0 * crate::host::PADDING - GAP * (columns - 1.0)) / columns;
        let grid = (column.min(PHONE_WIDTH) - 2.0 - BENTO_INSET * 2.0).max(0.0);
        let muted = cx.theme().foreground.muted();
        let mut cells = LIBRARY
            .iter()
            .enumerate()
            .map(|(index, (title, detail, span))| {
                (
                    *span,
                    bento_tile(("creator-tile", index), ("creator-tile-press", index), cx)
                        .child(
                            v_flex()
                                .p(px(12.0))
                                .gap(px(2.0))
                                .child(div().text_size(px(15.0)).child(*title))
                                .child(div().text_size(px(13.0)).text_color(muted).child(*detail)),
                        )
                        .into_any_element(),
                )
            })
            .collect::<Vec<_>>();
        cells.push((
            BentoSpan::new(4, 1),
            bento_add_tile("creator-add", "Add folder", cx).into_any_element(),
        ));
        let placement = placement();
        let bar =
            compact_floating_bar(div().flex_1(), &placement, cx).child(compact_floating_group(
                [compact_bar_button(
                    "creator-grid-settings",
                    IconName::Settings,
                )],
                &placement,
                cx,
            ));
        phone_frame("creator-phone-grid", cx)
            .child(
                v_flex()
                    .pb(px(16.0))
                    .child(large_title("Library", cx))
                    .child(div().px(px(BENTO_INSET)).child(bento_grid(px(grid), cells))),
            )
            .child(bar)
            .into_any_element()
    }
}

impl Render for Phone {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        tiles(
            window,
            vec![
                vec![self.keys(cx), Self::bindings(cx)],
                vec![self.page(cx), Self::sheet(cx)],
                vec![Self::grid(window, cx)],
            ],
        )
    }
}
