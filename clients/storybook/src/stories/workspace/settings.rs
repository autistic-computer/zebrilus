use ala::{
    AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement as _, Styled as _, Window,
    div, prelude::*, px,
};
use penna::{
    ActiveTheme as _, IconName,
    components::input::{InputState, NumberInput},
    components::switch::Switch,
};
use zz_ui::settings::{
    SettingEntry, SettingsFold, SettingsSection, SettingsStack,
    appearance::ZzRow,
    panes_page,
    panes_preview::PanesPreview,
    providers::{ProvidersHost, providers_stack},
    settings_card_ground, settings_page_title, settings_scroll_column, settings_section_index,
    settings_section_navigation, settings_title_bar,
};

use super::{fixtures::stateful, tree::sidebar};
use crate::story::Framed as _;
use crate::story::{Section, Story, stateless, states};

pub const STORY: Story = Story {
    id: "settings",
    name: "Settings",
    group: "Settings",
    summary: "The settings route: its sidebar and narrow-screen index, the stacked entry rows every page is built from, zz's appearance rows, and the Panes, Agents and About pages.",
    sections: &[
        Section {
            id: "navigation",
            name: "Navigation",
            summary: "On a wide window the sidebar swaps the workspace tree for the section list, its groups set apart by a gap. On a narrow one the groups become stacks of touch-size rows. On a phone every page opens with its title large, under a bar holding only Back; once the page scrolls the large title away, the bar takes it small and centered over a hairline.",
            build: |_, cx| stateless(navigation, cx),
        },
        Section {
            id: "appearance",
            name: "Appearance rows",
            summary: "The rows zz adds to penna's Appearance page: Program colors under Sidebar, and window blur opacity under Advanced. The kit's own rows are penna's, in its Appearance story.",
            build: |_, cx| stateless(appearance, cx),
        },
        Section {
            id: "panes-page",
            name: "Panes page",
            summary: "panes_page with the live preview above its Layout, Focus and Frame stacks. Frame settings dim while pane gaps are off.",
            build: |window, cx| stateful(window, cx, Controls::new, panes),
        },
        Section {
            id: "agents",
            name: "Agents",
            summary: "providers_stack for each connected machine: Claude Code and Codex with the installed and newest versions, Update or Install, the updater's latest line while it runs, its error when it fails, installs zz leaves to their owner (a root-owned npm prefix with the command to copy, Nix without one), a machine still checking and one that is offline.",
            build: |_, cx| stateless(agents, cx),
        },
    ],
};

struct Controls {
    numbers: [Entity<InputState>; 3],
}

impl Controls {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        let values = ["0.7", "100", "6"];
        let numbers = std::array::from_fn(|index| {
            let value = values[index];
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        });
        Self { numbers }
    }

    fn number(&self, index: usize, cx: &App) -> ala::Div {
        div()
            .w(px(120.0))
            .flex_none()
            .child(NumberInput::new(&self.numbers[index]).on(settings_card_ground(cx)))
    }
}

fn navigation(_: &mut Window, cx: &mut App) -> AnyElement {
    let list = settings_section_navigation(
        "settings-nav",
        &SettingsSection::ALL,
        SettingsSection::Appearance,
        |_, _, _| {},
    )
    .on_back(|_, _, _| {});
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
    let fold = SettingsFold::default();
    let phone_index = settings_section_index(
        &[
            SettingsSection::Appearance,
            SettingsSection::Terminal,
            SettingsSection::Hosts,
            SettingsSection::Agents,
            SettingsSection::Advanced,
            SettingsSection::About,
        ],
        |section| match section {
            SettingsSection::Hosts => Some("Local socket".into()),
            SettingsSection::About => Some("0.22.0".into()),
            _ => None,
        },
        |_, _, _| {},
        cx,
    )
    .large_title("Settings", &fold);
    let back =
        penna::blocks::compact::compact_bar_button("settings-phone-back", IconName::ArrowLeft);
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
        .state(
            "on a phone: the large title folds into the bar as the page scrolls",
            div()
                .id("settings-phone")
                .flex()
                .flex_col_reverse()
                .w(px(360.0))
                .h(px(420.0))
                .rounded(cx.theme().radius)
                .framed(cx)
                .overflow_hidden()
                .child(div().flex().flex_1().min_h_0().child(phone_index))
                .child(settings_title_bar("Settings", &fold, back)),
        )
        .into_any_element()
}

fn appearance(_: &mut Window, _: &mut App) -> AnyElement {
    let knobs = crate::knobs();
    states()
        .state(
            "sidebar rows",
            SettingsStack::titled("Sidebar").child(
                SettingEntry::new(ZzRow::ProgramColors.title())
                    .description("Fill sidebar tiles with the running program's colors")
                    .control(Switch::new("program-colors").checked(knobs.flag("program-colors"))),
            ),
        )
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
            [
                SettingEntry::new("Inactive pane opacity")
                    .description("1 turns dimming off")
                    .control(controls.number(0, cx)),
                SettingEntry::new("Selected pane contrast")
                    .when(!gaps, |entry| entry.description("Needs pane gaps"))
                    .disabled(!gaps)
                    .control(controls.number(1, cx)),
            ],
            [SettingEntry::new("Pane margin")
                .when(!gaps, |entry| entry.description("Needs pane gaps"))
                .disabled(!gaps)
                .control(controls.number(2, cx))],
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
                    .children(stacks),
            ),
        )
        .into_any_element()
}
