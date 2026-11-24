use std::rc::Rc;

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
    hosts::{HostRow, HostZz, hosts_heading, hosts_table},
    panes_page,
    panes_preview::PanesPreview,
    providers::{
        AgentsMachine, ProvidersAuth, ProvidersHost, agents_heading, agents_hook, machine_card,
        machine_picker,
    },
    settings_card_ground, settings_scroll_column, settings_section_index,
    settings_section_navigation, settings_title_bar,
};

use zz_protocol::{
    AgentAccount, AgentLogin, AgentLoginBrowser, AgentLoginStep,
    AgentProvider::{self, Antigravity, ClaudeCode, Codex, Cursor, Grok},
    ManualUpdate, ProviderStatus, ProviderTask, ProvidersReport,
};
use zz_ui::agent::login::login_card_keyed;

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
            summary: "The row zz adds to penna's Appearance page: Program colors under Sidebar. The kit's own rows are penna's, in its Appearance story.",
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
            summary: "The Agents page with xps picked: Check again and Update all in the heading row, the machine picker with a dot on machines that have an agent behind, one card of agent rows for the picked machine, an Install row for the agents it lacks, and the Coding agents hook last.",
            build: |_, cx| stateless(agents_xps, cx),
        },
        Section {
            id: "agents-local",
            name: "Agents, this Mac",
            summary: "What the page opens on: the local machine, everything current, several Claude Code accounts by email.",
            build: |_, cx| stateless(agents_local, cx),
        },
        Section {
            id: "agents-states",
            name: "Agents, states",
            summary: "An unreachable machine picked, checking, a Windows host that does not sign in, a sign-in card open, an install zz leaves to a package manager, installing, a failed update, one machine, ten machines and nothing behind.",
            build: |_, cx| stateless(agents_states, cx),
        },
        Section {
            id: "hosts",
            name: "Hosts",
            summary: "The Machines card of the Hosts page as a table: platform mark and name, address, the daemon's zz version, the button. One host per update state: outdated, installed and waiting for a restart, updating, a failed update with its reason under the row, one that cannot be updated from here, current, and one that is not connected. Update all sits in the heading when two or more hosts can update.",
            build: |_, cx| stateless(hosts, cx),
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
                .relative()
                .w(px(360.0))
                .h(px(420.0))
                .rounded(cx.theme().radius)
                .framed(cx)
                .overflow_hidden()
                .child(div().absolute().inset_0().flex().child(phone_index))
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

struct Machine {
    title: &'static str,
    host: ProvidersHost,
    accounts: Vec<AgentAccount>,
    signs_in: bool,
}

const ME: &str = "me@example.com";
const WORK: &str = "work@acme.example";

fn provider(
    provider: AgentProvider,
    home: &str,
    version: &str,
    latest: Option<&str>,
) -> ProviderStatus {
    ProviderStatus {
        path: Some(format!("{home}/.local/bin/{}", provider.as_str())),
        version: Some(version.to_owned()),
        latest: latest.map(str::to_owned),
        ..ProviderStatus::new(provider)
    }
}

fn account(provider: AgentProvider, email: &str, plan: &str) -> AgentAccount {
    AgentAccount {
        provider,
        id: None,
        signed_in: true,
        email: Some(email.to_owned()),
        organization: None,
        plan: Some(plan.to_owned()),
    }
}

fn machine(
    title: &'static str,
    providers: Vec<ProviderStatus>,
    accounts: Vec<AgentAccount>,
) -> Machine {
    Machine {
        title,
        host: ProvidersHost::Report(ProvidersReport {
            checking: false,
            providers,
        }),
        accounts,
        signs_in: true,
    }
}

fn this_mac() -> Machine {
    machine(
        "This Mac",
        vec![
            provider(ClaudeCode, "/Users/me", "2.1.330", Some("2.1.330")),
            provider(Codex, "/Users/me", "0.171.0", Some("0.171.0")),
            provider(Antigravity, "/Users/me", "1.5.14", None),
        ],
        vec![
            account(ClaudeCode, ME, "max_20x"),
            account(ClaudeCode, WORK, "max_20x"),
            account(Codex, ME, "pro"),
        ],
    )
}

fn xps(edit: impl FnOnce(&mut Vec<ProviderStatus>)) -> Machine {
    let mut claude = provider(ClaudeCode, "/home/me", "2.1.329", Some("2.1.330"));
    claude.task = ProviderTask::Running {
        output: "Downloading 2.1.330".to_owned(),
    };
    let mut providers = vec![
        claude,
        provider(Codex, "/home/me", "0.167.0", Some("0.171.0")),
        provider(Antigravity, "/home/me", "1.5.13", None),
    ];
    edit(&mut providers);
    machine("xps", providers, vec![account(ClaudeCode, ME, "max_20x")])
}

fn alienware() -> Machine {
    machine(
        "alienware",
        vec![
            provider(ClaudeCode, "/home/me", "2.1.330", Some("2.1.330")),
            provider(Codex, "/home/me", "0.171.0", Some("0.171.0")),
            provider(Antigravity, "/home/me", "1.5.14", None),
        ],
        vec![
            account(ClaudeCode, WORK, "max_20x"),
            account(Codex, ME, "pro"),
        ],
    )
}

fn offline(title: &'static str) -> Machine {
    Machine {
        title,
        host: ProvidersHost::Offline("unreachable".into()),
        accounts: Vec::new(),
        signs_in: false,
    }
}

fn fleet() -> Vec<Machine> {
    vec![this_mac(), xps(|_| {}), alienware(), offline("win-desktop")]
}

fn page(
    id: &'static str,
    machines: &[Machine],
    picked: usize,
    height: f32,
    cx: &App,
) -> ala::Stateful<ala::Div> {
    let reports = machines
        .iter()
        .enumerate()
        .filter_map(|(index, machine)| match &machine.host {
            ProvidersHost::Report(report) => Some((index, report)),
            ProvidersHost::Offline(_) | ProvidersHost::Waiting => None,
        });
    let update_all = (!zz_client::providers::update_all(reports).is_empty())
        .then_some(|_: &mut Window, _: &mut App| {});
    let shown = machines
        .iter()
        .map(|machine| AgentsMachine {
            title: machine.title.into(),
            host: match &machine.host {
                ProvidersHost::Report(report) => ProvidersHost::Report(report.clone()),
                ProvidersHost::Offline(state) => ProvidersHost::Offline(state.clone()),
                ProvidersHost::Waiting => ProvidersHost::Waiting,
            },
        })
        .collect::<Vec<_>>();
    div().id(id).flex().w(px(792.0)).h(px(height)).child(
        settings_scroll_column(id)
            .child(agents_heading(update_all, |_, _| {}, cx))
            .children(machine_picker(&shown, picked, |_, _, _| {}))
            .child(card(id, &machines[picked], None, cx))
            .child(agents_hook(|_, _| {}, cx)),
    )
}

fn card(
    id: &'static str,
    machine: &Machine,
    signing_in: Option<AgentProvider>,
    cx: &App,
) -> impl IntoElement {
    let login = signing_in.and_then(|provider| {
        login_card_keyed(
            id,
            &AgentLogin {
                id: 1,
                provider,
                account: None,
                pane: None,
                browser: AgentLoginBrowser::Local,
                step: AgentLoginStep::OpenUrl {
                    url: "https://auth.openai.com/oauth/authorize?client_id=app_9d1c25".to_owned(),
                },
            },
            None,
            |_, _, _| {},
            cx,
        )
    });
    let login = std::cell::RefCell::new(login);
    let auth = ProvidersAuth {
        signs_in: machine.signs_in,
        accounts: Some(&machine.accounts),
        on_auth: Rc::new(|_, _, _, _| {}),
        login: Rc::new(move |provider, _| {
            (Some(provider) == signing_in)
                .then(|| login.borrow_mut().take())
                .flatten()
        }),
    };
    machine_card(id, &machine.host, Some(&auth), |_, _, _| {}, cx)
}

fn framed_card(
    id: &'static str,
    machine: &Machine,
    signing_in: Option<AgentProvider>,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(792.0))
        .p(px(16.0))
        .child(card(id, machine, signing_in, cx))
}

fn hosts(_: &mut Window, cx: &mut App) -> AnyElement {
    use zz_protocol::{HostPlatform, LinuxDistro};

    let host = |name: &'static str, platform, address: &'static str, zz| HostRow {
        name: name.into(),
        platform,
        address: address.into(),
        remove: "Remove host (connected)".into(),
        zz,
    };
    let arch = HostPlatform::Linux(LinuxDistro::Arch);
    let rows = [
        host(
            "xps",
            HostPlatform::Linux(LinuxDistro::Ubuntu),
            "me@xps",
            HostZz::Outdated {
                daemon: Some("0.30.0".into()),
                target: "0.31.0".into(),
                failed: None,
            },
        ),
        host(
            "alienware",
            arch,
            "alienware.tail1a2b.ts.net",
            HostZz::RestartPending {
                installed: "0.31.0".into(),
                failed: None,
            },
        ),
        host(
            "win-desktop",
            HostPlatform::Windows,
            "win-desktop.tail1a2b.ts.net",
            HostZz::Updating {
                progress: Some("downloading zz 0.31.0".into()),
            },
        ),
        host(
            "mini",
            HostPlatform::Macos,
            "me@mini.local",
            HostZz::Outdated {
                daemon: Some("0.30.0".into()),
                target: "0.31.0".into(),
                failed: Some("zz 0.31.0 has no headless build for macos-x86_64".into()),
            },
        ),
        host(
            "nas",
            arch,
            "me@nas:2222",
            HostZz::CannotUpdate {
                daemon: Some("0.28.0".into()),
                advice: "Update zz on nas itself".into(),
            },
        ),
        host(
            "hetzner",
            arch,
            "me@hetzner-fsn1",
            HostZz::Current("0.31.0".into()),
        ),
        host(
            "studio",
            HostPlatform::Unknown,
            "me@studio.local",
            HostZz::Offline("unreachable".into()),
        ),
    ];
    div()
        .id("hosts-table")
        .w(px(760.0))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(penna::Space::GAP_RELATED)
        .child(hosts_heading(Some(|_: &mut Window, _: &mut App| {}), cx))
        .child(hosts_table(&rows, |_, _, _| {}, |_, _, _| {}, cx))
        .into_any_element()
}

fn agents_xps(_: &mut Window, cx: &mut App) -> AnyElement {
    page("agents-xps", &fleet(), 1, 470.0, cx).into_any_element()
}

fn agents_local(_: &mut Window, cx: &mut App) -> AnyElement {
    page("agents-local", &fleet(), 0, 470.0, cx).into_any_element()
}

fn agents_states(_: &mut Window, cx: &mut App) -> AnyElement {
    let windows = Machine {
        title: "win-desktop",
        signs_in: false,
        ..machine(
            "win-desktop",
            vec![
                provider(ClaudeCode, "C:\\Users\\me", "2.1.330", Some("2.1.330")),
                provider(Codex, "C:\\Users\\me", "0.171.0", Some("0.171.0")),
            ],
            Vec::new(),
        )
    };
    let quiet = |edit: fn(&mut Vec<ProviderStatus>)| {
        xps(|providers| {
            providers[0].version = Some("2.1.330".to_owned());
            providers[0].task = ProviderTask::Idle;
            edit(providers);
        })
    };
    let package_manager = quiet(|providers| {
        providers[1].manual = Some(ManualUpdate {
            owner: "pacman".to_owned(),
            command: Some("sudo pacman -Syu openai-codex".to_owned()),
        });
    });
    let installing = quiet(|providers| {
        providers.push(provider(
            Cursor,
            "/home/me",
            "2026.11.19-f005e9c1a7b3d2e4f6a8b0c2",
            Some("2026.11.21-0a1b2c3d4e5f6a7b8c9d0e1f"),
        ));
        let mut grok = ProviderStatus::new(Grok);
        grok.latest = Some("1.0.93".to_owned());
        grok.task = ProviderTask::Running {
            output: "Downloading grok 1.0.93 (14.2 of 38.0 MB)".to_owned(),
        };
        providers.push(grok);
    });
    let failed = quiet(|providers| {
        providers[1].task = ProviderTask::Failed {
            message:
                "npm error EACCES: permission denied, rename '/usr/lib/node_modules/@openai/codex'"
                    .to_owned(),
        };
    });
    let checking = Machine {
        host: ProvidersHost::Waiting,
        ..xps(|_| {})
    };
    let all_current = vec![
        this_mac(),
        quiet(|providers| {
            providers[1].version = Some("0.171.0".to_owned());
            providers[2].version = Some("1.5.14".to_owned());
        }),
        alienware(),
        offline("win-desktop"),
    ];
    let many = [
        "mac-mini-office",
        "thinkpad-x1",
        "build-arm64",
        "homelab-nas",
        "hetzner-fsn1",
        "pi-kitchen",
    ]
    .into_iter()
    .map(|title| Machine {
        title,
        ..alienware()
    });
    let many = fleet().into_iter().chain(many).collect::<Vec<_>>();
    states()
        .state(
            "unreachable machine picked",
            page("agents-unreachable", &fleet(), 3, 380.0, cx),
        )
        .state(
            "checking",
            framed_card("agents-checking", &checking, None, cx),
        )
        .state(
            "a reachable Windows machine that does not sign in",
            framed_card("agents-windows", &windows, None, cx),
        )
        .state(
            "signing in to Codex",
            framed_card("agents-signing-in", &quiet(|_| {}), Some(Codex), cx),
        )
        .state(
            "installed by a package manager",
            framed_card("agents-package-manager", &package_manager, None, cx),
        )
        .state(
            "installing Grok, a long Cursor version behind",
            framed_card("agents-installing", &installing, None, cx),
        )
        .state(
            "update failed",
            framed_card("agents-failed", &failed, None, cx),
        )
        .state(
            "one machine only",
            page("agents-one", &[this_mac()], 0, 400.0, cx),
        )
        .state(
            "ten machines: the picker scrolls sideways",
            page("agents-many", &many, 1, 470.0, cx),
        )
        .state(
            "the narrowest settings column, 480 wide",
            page("agents-narrow", &fleet(), 1, 470.0, cx).w(px(480.0)),
        )
        .state(
            "nothing behind anywhere",
            page("agents-current", &all_current, 0, 470.0, cx),
        )
        .into_any_element()
}
