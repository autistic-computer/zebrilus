use std::sync::{Arc, LazyLock};

use zz_client::StatusBarSettings;
use zz_gpui::{
    AnyElement, App, AppContext as _, Entity, FocusHandle, IntoElement, ParentElement as _,
    Styled as _, Window, div, prelude::*, px,
};
use zz_ui::{
    ActiveTheme as _, Emphasis, Highlight, Icon, IconName, IndexPath, Sizable as _, StylePatch,
    SurfaceRole, ThemeMode, Tone,
    button::{Button, ButtonVariants as _},
    color_picker::{ColorPicker, ColorPickerState},
    input::{InputState, NumberInput},
    interface_theme::{
        InterfaceTheme, Materials, ThemeModeSetting, ThemeOverrides, builtin_themes,
        pinned_theme_mode,
    },
    select::{Select, SelectState},
    settings::{
        SettingEntry, SettingsSection, SettingsSelectItem, SettingsStack,
        about::{
            ABOUT_LOGO_SIZE, about_build_stack, about_copy_button, about_hero, about_project_stack,
        },
        appearance::{
            AppearancePageItem, SWATCH, ThemePicks, accent_swatches, contrast_slider,
            corner_radius_slider, interface_theme_preview, picker_tile, theme_preview,
            themes_strip, tint_hue_slider, tint_slider, ui_font_select, window_opacity_slider,
            zoom_slider,
        },
        panes_page,
        panes_preview::PanesPreview,
        providers::{ProvidersHost, providers_stack},
        settings_card_ground, settings_list_disclosure_header, settings_navigation_back_button,
        settings_navigation_back_row, settings_navigation_button, settings_page_title,
        settings_scroll_column, settings_section_index,
        status_bar_preview::status_bar_page,
    },
    switch::Switch,
};

use super::{fixtures::stateful, tree::sidebar};
use crate::story::Framed as _;
use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "settings",
    name: "Settings",
    group: "Settings",
    summary: "The settings route: its sidebar and narrow-screen index, the stacked entry rows every page is built from, the appearance pickers, and the Panes, Status bar, Agents and About pages.",
    sections: &[
        Section {
            id: "navigation",
            name: "Navigation",
            summary: "On a wide window the sidebar swaps the workspace tree for the section list, with no group labels. On a narrow one the sections become a touch-size index.",
            build: |_, cx| stateless(navigation, cx),
        },
        Section {
            id: "entries",
            name: "Stacks and entries",
            summary: "SettingsStack glues SettingEntry rows into one surface and rounds only its ends. A row is its title and its control at the right edge, with no description under it.",
            build: |window, cx| stateful(window, cx, Controls::new, entries),
        },
        Section {
            id: "appearance",
            name: "Appearance pickers",
            summary: "The Interface page's Theme rows: the mode tiles, the Style strip drawn over the look on screen, the accent swatches with a custom picker (all following the corner radius), the tint sliders (they follow the tint knobs), the saved themes strip, the Advanced heading, and the corner radius, contrast, animation speed (it follows the speed knob) and window blur opacity sliders under it.",
            build: |window, cx| stateful(window, cx, Controls::new, appearance),
        },
        Section {
            id: "themes",
            name: "Themes",
            summary: "interface_theme_preview for every built-in theme in both modes, as the theme tiles draw them.",
            build: |_, cx| stateless(themes, cx),
        },
        Section {
            id: "panes-page",
            name: "Panes page",
            summary: "panes_page with the live preview above its Layout, Appearance, Focus and Frame stacks. Frame settings dim while pane gaps are off.",
            build: |window, cx| stateful(window, cx, Controls::new, panes),
        },
        Section {
            id: "status-bar-page",
            name: "Status bar page",
            summary: "status_bar_page: a preview of the titlebar that follows the switches below it.",
            build: |_, cx| stateless(status_bar, cx),
        },
        Section {
            id: "agents",
            name: "Agents",
            summary: "The driver's settings (on or off, the model it runs, its effort), then providers_stack for each connected machine: Claude Code and Codex with the installed and newest versions, Update or Install, the updater's latest line while it runs, its error when it fails, installs zz leaves to their owner (a root-owned npm prefix with the command to copy, Nix without one), a machine still checking and one that is offline.",
            build: |_, cx| stateless(agents, cx),
        },
        Section {
            id: "about",
            name: "About",
            summary: "The About page: the logo and version, the build details to quote in a bug report, and the project links.",
            build: |_, cx| stateless(about, cx),
        },
    ],
};

struct Controls {
    numbers: [Entity<InputState>; 8],
    choice: Entity<SelectState<Vec<SettingsSelectItem>>>,
    font: Entity<SelectState<Vec<SettingsSelectItem>>>,
    accent: Entity<ColorPickerState>,
    zoom: FocusHandle,
}

impl Controls {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        let values = ["50", "0.7", "100", "6", "13.5", "0.5", "100", "80"];
        let numbers = std::array::from_fn(|index| {
            let value = values[index];
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        });
        let choice = cx.new(|cx| {
            SelectState::new(
                vec![
                    SettingsSelectItem::new("Tree", "tree"),
                    SettingsSelectItem::new("Flat list", "flat"),
                ],
                Some(IndexPath::new(0)),
                window,
                cx,
            )
        });
        let font = ui_font_select(Some("Inter Variable"), window, cx);
        let accent = cx.new(|cx| ColorPickerState::new(None, window, cx));
        Self {
            numbers,
            choice,
            font,
            accent,
            zoom: cx.focus_handle(),
        }
    }

    fn number(&self, index: usize, cx: &App) -> zz_gpui::Div {
        div()
            .w(px(120.0))
            .flex_none()
            .child(NumberInput::new(&self.numbers[index]).on(settings_card_ground(cx)))
    }
}

fn navigation(_: &mut Window, cx: &mut App) -> AnyElement {
    let selected = SettingsSection::Appearance;
    let rows = SettingsSection::ALL
        .map(|section| settings_navigation_button(section, section == selected, cx));
    let list = div()
        .id("settings-nav")
        .flex()
        .flex_col()
        .w_full()
        .gap(px(2.0))
        .px(px(6.0))
        .child(settings_navigation_back_row(
            settings_navigation_back_button("settings-back"),
        ))
        .children(rows);
    let index = settings_section_index(
        &SettingsSection::ALL,
        |section| match section {
            SettingsSection::Hosts => Some("devbox".into()),
            SettingsSection::About => Some("0.16.0".into()),
            _ => None,
        },
        |_, _, _| {},
        cx,
    );
    states()
        .columns(2)
        .state(
            "sidebar, Interface selected",
            div()
                .id("settings-sidebar")
                .flex()
                .h(px(560.0))
                .child(sidebar(list, true, cx)),
        )
        .state(
            "narrow screens: section index",
            div()
                .id("settings-index")
                .flex()
                .w(px(360.0))
                .h(px(660.0))
                .rounded(cx.theme().radius)
                .framed(cx)
                .child(index),
        )
        .into_any_element()
}

fn entries(controls: &Controls, _: &mut Window, cx: &mut App) -> AnyElement {
    states()
        .state(
            "page heading",
            settings_page_title(SettingsSection::Panes, cx),
        )
        .state(
            "titled stack: switch, number and select controls",
            SettingsStack::titled("Command palette")
                .child(
                    SettingEntry::new("Command shortcuts")
                        .control(Switch::new("shortcuts").checked(true)),
                )
                .child(SettingEntry::new("UI zoom").control(controls.number(6, cx)))
                .child(
                    SettingEntry::new("Navigation layout").control(
                        Select::new(&controls.choice)
                            .title("Navigation layout")
                            .on(settings_card_ground(cx)),
                    ),
                ),
        )
        .state(
            "a single entry rounds all four corners",
            SettingsStack::new()
                .child(SettingEntry::new("Sidebar").control(Switch::new("sidebar").checked(true))),
        )
        .state(
            "disabled: no effect with the current configuration",
            SettingsStack::titled("Frame")
                .child(
                    SettingEntry::new("Pane margin")
                        .disabled(true)
                        .control(controls.number(3, cx)),
                )
                .child(
                    SettingEntry::new("Pane edge width")
                        .disabled(true)
                        .control(controls.number(5, cx)),
                ),
        )
        .state(
            "a state glyph before the title",
            SettingsStack::titled("Hosts").child(
                SettingEntry::new("devbox")
                    .title_icon(Icon::new(IconName::CircleCheck).text_color(cx.theme().success))
                    .control(Button::new("open-host").label("Disconnect")),
            ),
        )
        .state(
            "full-width content under the copy",
            SettingsStack::new().child(
                SettingEntry::new("Colors, cursor, and spacing").child(
                    div()
                        .p(px(10.0))
                        .rounded(cx.theme().radius)
                        .bg(cx
                            .theme()
                            .paint_on(settings_card_ground(cx), Tone::Neutral, Emphasis::Soft)
                            .fill)
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_size(px(12.0))
                        .child("font-family = Lilex\ncursor-style = bar\nwindow-padding-x = 8"),
                ),
            ),
        )
        .into_any_element()
}

fn appearance(controls: &Controls, _: &mut Window, cx: &mut App) -> AnyElement {
    let knobs = crate::knobs();
    let active = knobs.interface_theme();
    let light = active.colors(ThemeMode::Light);
    let dark = active.colors(ThemeMode::Dark);
    let tiles =
        div()
            .flex()
            .flex_none()
            .flex_wrap()
            .gap(px(8.0))
            .children(ThemeModeSetting::ALL.map(|mode| {
                picker_tile(
                    format!("theme-{}", mode.as_str()).into(),
                    mode.title(),
                    theme_preview(pinned_theme_mode(mode), &light, &dark, cx),
                    mode == ThemeModeSetting::System,
                    cx,
                )
            }));
    let mode = cx.theme().mode;
    let saved = InterfaceTheme {
        name: "theme-1".to_owned(),
        materials: Materials {
            radius: 4.0,
            ..active.materials.clone()
        },
        roles: [(
            SurfaceRole::MENU_ITEM.id.to_owned(),
            StylePatch::EMPTY.selected(Highlight::Tint),
        )]
        .into(),
        ..active.clone()
    };
    let overrides = ThemeOverrides {
        tint: Some(knobs.tint),
        tint_hue: Some(knobs.tint_hue),
        ..ThemeOverrides::default()
    };
    let picks = ThemePicks {
        themes: builtin_themes()
            .iter()
            .map(|(id, theme)| ((*id).into(), theme))
            .chain([("theme-1".into(), &saved)])
            .collect(),
        selected: 0,
        overrides: &overrides,
    };
    let entry = |item: AppearancePageItem| SettingEntry::new(item.title().unwrap_or_default());
    let custom = ColorPicker::new(&controls.accent, zz_gpui::transparent_black())
        .label("Accent")
        .swatch(px(SWATCH));
    states()
        .state(
            "theme rows",
            SettingsStack::titled("Interface")
                .child(SettingEntry::new("Mode").control(tiles))
                .child(
                    entry(AppearancePageItem::Themes)
                        .title_actions(
                            div()
                                .flex()
                                .gap(px(4.0))
                                .child(Button::new("theme-save").ghost().small().label("Save"))
                                .child(Button::new("theme-import").ghost().small().label("Import")),
                        )
                        .child(themes_strip(&picks, mode, |_, _, _| {}, cx)),
                )
                .child(entry(AppearancePageItem::Accent).control(accent_swatches(
                    &picks,
                    mode,
                    custom,
                    |_, _, _| {},
                    cx,
                )))
                .child(entry(AppearancePageItem::TintHue).control(tint_hue_slider(
                    knobs.tint_hue,
                    |_, _, _| {},
                    cx,
                )))
                .child(entry(AppearancePageItem::Tint).control(tint_slider(
                    knobs.tint,
                    |_, _, _| {},
                    cx,
                )))
                .child(
                    SettingEntry::new("UI font").control(
                        div().w(px(200.0)).flex_none().child(
                            Select::new(&controls.font)
                                .title("UI font")
                                .placeholder("System default")
                                .on(settings_card_ground(cx)),
                        ),
                    ),
                ),
        )
        .state(
            "sidebar rows",
            SettingsStack::titled("Sidebar").child(
                entry(AppearancePageItem::ProgramColors)
                    .control(Switch::new("program-colors").checked(knobs.program_colors)),
            ),
        )
        .state(
            "advanced heading, collapsed and expanded",
            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .children([false, true].map(|expanded| {
                    settings_list_disclosure_header(
                        if expanded {
                            "advanced-expanded"
                        } else {
                            "advanced-collapsed"
                        },
                        "Advanced",
                        expanded,
                    )
                })),
        )
        .state(
            "advanced sliders",
            SettingsStack::new()
                .child(
                    entry(AppearancePageItem::CornerRadius).control(corner_radius_slider(
                        active.materials.radius,
                        |_, _, _| {},
                        cx,
                    )),
                )
                .child(
                    entry(AppearancePageItem::ChromeContrast).control(contrast_slider(
                        knobs.contrast,
                        |_, _, _| {},
                        cx,
                    )),
                )
                .child(entry(AppearancePageItem::AnimationSpeed).control(
                    zz_ui::settings::appearance::animation_speed_slider(
                        knobs.speed,
                        |_, _, _| {},
                        cx,
                    ),
                ))
                .child(entry(AppearancePageItem::UiZoom).control(zoom_slider(
                    1.0,
                    &controls.zoom,
                    |_, _, _| {},
                    cx,
                )))
                .child(
                    entry(AppearancePageItem::WindowBackgroundOpacity)
                        .control(window_opacity_slider(0.0, |_, _, _| {}, cx)),
                ),
        )
        .into_any_element()
}

fn themes(_: &mut Window, cx: &mut App) -> AnyElement {
    let group = |mode: ThemeMode| {
        div()
            .flex()
            .flex_wrap()
            .gap(px(8.0))
            .children(builtin_themes().iter().map(|(id, theme)| {
                picker_tile(
                    format!("theme-{id}-{}", mode.name()).into(),
                    theme.name.clone(),
                    interface_theme_preview(theme, mode, cx),
                    false,
                    cx,
                )
            }))
    };
    states()
        .state("light", group(ThemeMode::Light))
        .state("dark", group(ThemeMode::Dark))
        .into_any_element()
}

fn panes(controls: &Controls, _: &mut Window, cx: &mut App) -> AnyElement {
    let page = |gaps: bool, cx: &App| {
        let pane = crate::pane();
        panes_page(
            PanesPreview {
                gaps,
                margin: pane.margin,
                inactive_opacity: pane.inactive,
            },
            [SettingEntry::new("Pane gaps").control(Switch::new("gaps").checked(gaps))],
            [SettingEntry::new("Pane background opacity").control(controls.number(0, cx))],
            [
                SettingEntry::new("Inactive pane opacity").control(controls.number(1, cx)),
                SettingEntry::new("Selected pane contrast")
                    .disabled(!gaps)
                    .control(controls.number(2, cx)),
            ],
            [SettingEntry::new("Pane margin")
                .disabled(!gaps)
                .control(controls.number(3, cx))],
            cx,
        )
    };
    states()
        .state(
            "gaps on",
            div()
                .id("panes-page")
                .flex()
                .h(px(1180.0))
                .child(page(true, cx)),
        )
        .into_any_element()
}

fn status_rows(settings: StatusBarSettings) -> SettingsStack {
    SettingsStack::new()
        .child(SettingEntry::new("Sidebar").control(Switch::new("sidebar").checked(false)))
        .child(
            SettingEntry::new("Window badges")
                .control(Switch::new("status-badges").checked(settings.badges)),
        )
}

fn status_bar(_: &mut Window, cx: &mut App) -> AnyElement {
    let page = |id: &'static str, settings: StatusBarSettings, gaps: bool, cx: &App| {
        div().id(id).flex().h(px(470.0)).child(status_bar_page(
            settings,
            gaps,
            status_rows(settings),
            cx,
        ))
    };
    states()
        .state(
            "everything on",
            page("status-defaults", StatusBarSettings::default(), false, cx),
        )
        .state(
            "badges off, pane gaps on",
            page(
                "status-minimal",
                StatusBarSettings {
                    badges: false,
                    ..StatusBarSettings::default()
                },
                true,
                cx,
            ),
        )
        .into_any_element()
}

static LOGOS: LazyLock<[Arc<zz_gpui::Image>; 2]> = LazyLock::new(|| {
    [
        include_bytes!("../../../../../assets/zz-light-512.png").as_slice(),
        include_bytes!("../../../../../assets/zz-dark-512.png").as_slice(),
    ]
    .map(|bytes| {
        Arc::new(zz_gpui::Image::from_bytes(
            zz_gpui::ImageFormat::Png,
            bytes.to_vec(),
        ))
    })
});

fn provider(
    provider: zz_protocol::AgentProvider,
    path: Option<&str>,
    version: Option<&str>,
    latest: &str,
    task: zz_protocol::ProviderTask,
) -> zz_protocol::ProviderStatus {
    zz_protocol::ProviderStatus {
        provider,
        path: path.map(str::to_owned),
        version: version.map(str::to_owned),
        latest: Some(latest.to_owned()),
        manual: None,
        task,
    }
}

fn manual(
    status: zz_protocol::ProviderStatus,
    owner: &str,
    command: Option<&str>,
) -> zz_protocol::ProviderStatus {
    zz_protocol::ProviderStatus {
        manual: Some(zz_protocol::ManualUpdate {
            owner: owner.to_owned(),
            command: command.map(str::to_owned),
        }),
        ..status
    }
}

fn agents(_: &mut Window, cx: &mut App) -> AnyElement {
    use zz_protocol::{
        AgentProvider::{ClaudeCode, Codex},
        ProviderTask, ProvidersReport,
    };

    let report = |checking, providers| {
        ProvidersHost::Report(ProvidersReport {
            checking,
            providers,
        })
    };
    let hosts = [
        (
            "This Mac",
            report(
                false,
                vec![
                    provider(
                        ClaudeCode,
                        Some("/Users/me/.local/bin/claude"),
                        Some("2.1.309"),
                        "2.1.309",
                        ProviderTask::Idle,
                    ),
                    provider(
                        Codex,
                        Some("/opt/homebrew/bin/codex"),
                        Some("0.163.0"),
                        "0.164.0",
                        ProviderTask::Idle,
                    ),
                ],
            ),
        ),
        (
            "ubuntu",
            report(
                false,
                vec![
                    provider(
                        ClaudeCode,
                        Some("/home/me/.local/bin/claude"),
                        Some("2.1.308"),
                        "2.1.309",
                        ProviderTask::Running {
                            output: "Updating Claude Code to 2.1.309…".into(),
                        },
                    ),
                    provider(
                        Codex,
                        Some("/home/me/.local/bin/codex"),
                        Some("0.159.0"),
                        "0.164.0",
                        ProviderTask::Failed {
                            message: "curl: (6) Could not resolve host: chatgpt.com".into(),
                        },
                    ),
                ],
            ),
        ),
        (
            "alienware",
            report(
                false,
                vec![
                    provider(
                        ClaudeCode,
                        Some("/home/me/.local/bin/claude"),
                        Some("2.1.309"),
                        "2.1.309",
                        ProviderTask::Idle,
                    ),
                    manual(
                        provider(
                            Codex,
                            Some("/usr/bin/codex"),
                            Some("0.163.0"),
                            "0.164.0",
                            ProviderTask::Idle,
                        ),
                        "npm under /usr",
                        Some("sudo /usr/bin/npm install -g @openai/codex@latest"),
                    ),
                ],
            ),
        ),
        (
            "nixbox",
            report(
                false,
                vec![manual(
                    provider(
                        ClaudeCode,
                        Some("/run/current-system/sw/bin/claude"),
                        Some("2.1.290"),
                        "2.1.309",
                        ProviderTask::Idle,
                    ),
                    "Nix",
                    None,
                )],
            ),
        ),
        (
            "xps",
            report(
                true,
                vec![
                    provider(ClaudeCode, None, None, "2.1.309", ProviderTask::Idle),
                    provider(
                        Codex,
                        Some("/home/me/.local/bin/codex"),
                        Some("0.164.0"),
                        "0.164.0",
                        ProviderTask::Idle,
                    ),
                ],
            ),
        ),
        ("mini", ProvidersHost::Waiting),
        ("devbox", ProvidersHost::Offline("unreachable".into())),
    ];
    let stacks = hosts
        .iter()
        .map(|(name, host)| providers_stack(*name, *name, host, |_, _, _| {}, cx));
    states()
        .state(
            "page",
            div().id("agents").flex().h(px(1700.0)).child(
                settings_scroll_column("settings-agents")
                    .child(settings_page_title(SettingsSection::Agents, cx))
                    .child(driver_stack(cx))
                    .children(stacks),
            ),
        )
        .into_any_element()
}

/// The Driver stack as the app draws it, with the driver on.
fn driver_stack(cx: &App) -> SettingsStack {
    let choice = |id: &'static str, label: &'static str| {
        Button::new(id)
            .label(label)
            .dropdown_caret(true)
            .on(settings_card_ground(cx))
    };
    SettingsStack::titled("Driver")
        .child(SettingEntry::new("Driver Agent").control(Switch::new("driver-agent").checked(true)))
        .child(SettingEntry::new("Model").control(choice("driver-model", "Claude Opus")))
        .child(SettingEntry::new("Effort").control(choice("driver-effort", "Extra high")))
}

fn about(_: &mut Window, cx: &mut App) -> AnyElement {
    let logo = Arc::clone(&LOGOS[usize::from(cx.theme().mode.is_dark())]);
    states()
        .state(
            "page",
            div().id("about").flex().h(px(780.0)).child(
                settings_scroll_column("settings-about")
                    .child(about_hero(zz_gpui::img(logo).size(px(ABOUT_LOGO_SIZE)), cx))
                    .child(about_build_stack(
                        "browser · wasm32",
                        about_copy_button("copy-build"),
                        cx,
                    ))
                    .child(about_project_stack(cx)),
            ),
        )
        .into_any_element()
}
