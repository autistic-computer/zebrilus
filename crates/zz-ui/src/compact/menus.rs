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
    let mut groups = vec![
        new_group(Some(menu.session)),
        StackGroup::new([
            StackChip::new(CLOSE_PANE, "Close", IconName::Xmark).look(StackLook::Danger),
            StackChip::new(RENAME_PANE, "Rename", IconName::Pencil),
        ]),
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
    fn a_pane_menu_keeps_bindings_for_terminals_only() {
        let terminal = pane_menu(PaneMenu {
            session: "zz",
            terminal: true,
        });
        assert_eq!(
            ids(&terminal),
            [
                vec![NEW_CHAT, NEW_TERMINAL, NEW_BROWSER],
                vec![CLOSE_PANE, RENAME_PANE],
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
