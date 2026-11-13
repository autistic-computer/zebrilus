use penna::blocks::touch::{StackChip, StackGroup, StackLook};

use crate::IconName;

pub const NEW_CHAT: &str = "new-chat";
pub const NEW_TERMINAL: &str = "new-terminal";
pub const NEW_BROWSER: &str = "new-browser";
pub const NEW_SESSION: &str = "new-session";
pub const ALL_MACHINES: &str = "all-machines";
pub const SETTINGS: &str = "settings";
pub const OVERVIEW: &str = "overview";
pub const RENAME_PANE: &str = "rename-pane";
pub const CLOSE_PANE: &str = "close-pane";
pub const ALL_BINDINGS: &str = "all-bindings";
pub const FIND: &str = "find";
pub const BACK: &str = "browser-back";
pub const FORWARD: &str = "browser-forward";
pub const RELOAD: &str = "browser-reload";
pub const STOP: &str = "browser-stop";
pub const ADDRESS: &str = "browser-address";
pub const TABS: &str = "browser-tabs";
pub const PICK_ELEMENT: &str = "browser-pick";
pub const OPEN_ON_PHONE: &str = "browser-open-on-phone";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineMenu<'a> {
    Unreached,
    Empty,
    Session(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneMenu<'a> {
    pub session: &'a str,
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrowserMenu<'a> {
    pub address: &'a str,
    pub back: bool,
    pub forward: bool,
    pub loading: bool,
    pub tabs: usize,
    pub open_on_phone: bool,
}

fn new_group(session: Option<&str>) -> StackGroup {
    let label = session.map_or_else(
        || "In a new session".to_owned(),
        |name| format!("New in {name}"),
    );
    StackGroup::new([
        StackChip::new(NEW_CHAT, "New chat", IconName::ChatPlus),
        StackChip::new(NEW_TERMINAL, "New terminal", IconName::SquareTerminal),
        StackChip::new(NEW_BROWSER, "New browser", IconName::Globe),
    ])
    .label(label)
}

fn alone(chip: StackChip) -> StackGroup {
    StackGroup::new([chip])
}

#[must_use]
pub fn machine_menu(state: MachineMenu) -> Vec<StackGroup> {
    let new = match state {
        MachineMenu::Unreached => Vec::new(),
        MachineMenu::Empty => vec![new_group(None)],
        MachineMenu::Session(name) => vec![
            new_group(Some(name)),
            alone(StackChip::new(
                NEW_SESSION,
                "New session",
                IconName::LayersPlus,
            )),
        ],
    };
    new.into_iter()
        .chain([
            alone(StackChip::new(
                ALL_MACHINES,
                "All machines",
                IconName::Server,
            )),
            alone(StackChip::new(SETTINGS, "Settings", IconName::Settings).look(StackLook::Quiet)),
        ])
        .collect()
}

#[must_use]
pub fn pane_menu(menu: PaneMenu) -> Vec<StackGroup> {
    let mut this_pane = vec![
        StackChip::new(CLOSE_PANE, "Close", IconName::Xmark).look(StackLook::Danger),
        StackChip::new(RENAME_PANE, "Rename", IconName::Pencil),
    ];
    if menu.terminal {
        this_pane.push(StackChip::new(FIND, "Find", IconName::Search));
    }
    let mut groups = vec![
        new_group(Some(menu.session)),
        StackGroup::new(this_pane),
        alone(StackChip::new(
            OVERVIEW,
            "Overview",
            IconName::LayoutDashboard,
        )),
    ];
    if menu.terminal {
        groups.push(alone(
            StackChip::new(ALL_BINDINGS, "All bindings", IconName::Keyboard).look(StackLook::Quiet),
        ));
    }
    groups
}

#[must_use]
pub fn browser_menu(browser: BrowserMenu, session: &str) -> (Vec<StackGroup>, usize) {
    let reload = if browser.loading {
        StackChip::new(STOP, "Stop", IconName::Stop)
    } else {
        StackChip::new(RELOAD, "Reload", IconName::Redo2)
    };
    let address = browser
        .address
        .strip_prefix("https://")
        .or_else(|| browser.address.strip_prefix("http://"))
        .unwrap_or(browser.address);
    let address = match address.strip_suffix('/') {
        Some(root) if !root.contains('/') => root,
        _ => address,
    };
    let address = if address.is_empty() {
        "New tab"
    } else {
        address
    };
    let mut page = vec![
        StackChip::new(TABS, "Tabs", IconName::Layers).value(browser.tabs.to_string()),
        StackChip::new(PICK_ELEMENT, "Pick element", IconName::Inspector).disabled(browser.loading),
    ];
    if browser.open_on_phone {
        page.push(StackChip::new(
            OPEN_ON_PHONE,
            "Open on this phone",
            IconName::ExternalLink,
        ));
    }
    let own = vec![
        StackGroup::row([
            StackChip::new(BACK, "Back", IconName::ArrowLeft).disabled(!browser.back),
            StackChip::new(FORWARD, "Forward", IconName::ArrowRight).disabled(!browser.forward),
            reload,
        ]),
        alone(StackChip::new(ADDRESS, address, IconName::Globe).place()),
        StackGroup::new(page),
    ];
    let fold = own.len();
    let rest = pane_menu(PaneMenu {
        session,
        terminal: false,
    });
    (own.into_iter().chain(rest).collect(), fold)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(groups: &[StackGroup]) -> Vec<Vec<String>> {
        groups
            .iter()
            .map(|group| {
                group
                    .chips()
                    .iter()
                    .map(|chip| chip.id.to_string())
                    .collect()
            })
            .collect()
    }

    fn labels(groups: &[StackGroup]) -> Vec<Option<String>> {
        groups
            .iter()
            .map(|group| group.title().map(ToString::to_string))
            .collect()
    }

    #[test]
    fn a_machine_menu_makes_things_in_the_session_in_view() {
        let groups = machine_menu(MachineMenu::Session("dotfiles"));
        assert_eq!(
            ids(&groups),
            [
                vec![NEW_CHAT, NEW_TERMINAL, NEW_BROWSER],
                vec![NEW_SESSION],
                vec![ALL_MACHINES],
                vec![SETTINGS],
            ]
        );
        assert_eq!(
            labels(&groups),
            [Some("New in dotfiles".to_owned()), None, None, None]
        );

        let empty = machine_menu(MachineMenu::Empty);
        assert_eq!(
            ids(&empty),
            [
                vec![NEW_CHAT, NEW_TERMINAL, NEW_BROWSER],
                vec![ALL_MACHINES],
                vec![SETTINGS],
            ]
        );
        assert_eq!(labels(&empty)[0].as_deref(), Some("In a new session"));

        assert_eq!(
            ids(&machine_menu(MachineMenu::Unreached)),
            [vec![ALL_MACHINES], vec![SETTINGS]]
        );
    }

    #[test]
    fn a_browser_menu_leads_with_its_own_chips_and_folds_the_pane_behind_more() {
        let at_rest = BrowserMenu {
            address: "https://github.com/autistic-computer/zebrilus/pull/412",
            back: true,
            forward: false,
            loading: false,
            tabs: 3,
            open_on_phone: true,
        };
        let (groups, fold) = browser_menu(at_rest, "zz");
        assert_eq!(fold, 3);
        assert_eq!(
            ids(&groups),
            [
                vec![BACK, FORWARD, RELOAD],
                vec![ADDRESS],
                vec![TABS, PICK_ELEMENT, OPEN_ON_PHONE],
                vec![NEW_CHAT, NEW_TERMINAL, NEW_BROWSER],
                vec![CLOSE_PANE, RENAME_PANE],
                vec![OVERVIEW],
            ]
        );
        let off = |groups: &[StackGroup]| -> Vec<String> {
            groups
                .iter()
                .flat_map(StackGroup::chips)
                .filter(|chip| chip.disabled)
                .map(|chip| chip.id.to_string())
                .collect()
        };
        assert_eq!(off(&groups), [FORWARD]);
        assert_eq!(
            groups[1].chips()[0].label.as_ref(),
            "github.com/autistic-computer/zebrilus/pull/412"
        );
        assert!(groups[1].chips()[0].place);
        assert_eq!(groups[2].chips()[0].value.as_deref(), Some("3"));

        let (loading, _) = browser_menu(
            BrowserMenu {
                address: "",
                back: false,
                loading: true,
                tabs: 1,
                open_on_phone: false,
                ..at_rest
            },
            "zz",
        );
        assert_eq!(ids(&loading)[0], [BACK, FORWARD, STOP]);
        assert_eq!(ids(&loading)[2], [TABS, PICK_ELEMENT]);
        assert_eq!(off(&loading), [BACK, FORWARD, PICK_ELEMENT]);
        assert_eq!(loading[1].chips()[0].label.as_ref(), "New tab");
        let label = |address| {
            browser_menu(BrowserMenu { address, ..at_rest }, "zz").0[1].chips()[0]
                .label
                .to_string()
        };
        assert_eq!(label("http://localhost:8102/"), "localhost:8102");
        assert_eq!(label("https://example.com/docs/"), "example.com/docs/");
    }

    #[test]
    fn a_pane_menu_keeps_find_and_bindings_for_terminals_only() {
        let terminal = pane_menu(PaneMenu {
            session: "zz",
            terminal: true,
        });
        assert_eq!(
            ids(&terminal),
            [
                vec![NEW_CHAT, NEW_TERMINAL, NEW_BROWSER],
                vec![CLOSE_PANE, RENAME_PANE, FIND],
                vec![OVERVIEW],
                vec![ALL_BINDINGS],
            ]
        );
        assert_eq!(labels(&terminal)[0].as_deref(), Some("New in zz"));
        assert_eq!(
            ids(&pane_menu(PaneMenu {
                session: "zz",
                terminal: false,
            })),
            [
                vec![NEW_CHAT, NEW_TERMINAL, NEW_BROWSER],
                vec![CLOSE_PANE, RENAME_PANE],
                vec![OVERVIEW],
            ]
        );
    }
}
