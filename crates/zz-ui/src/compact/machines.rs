use std::rc::Rc;

use ala::{App, Div, ParentElement as _, Styled as _, Window};
use penna::blocks::touch::{TouchGroup, TouchHeader, TouchRow, TouchTrailing};

use crate::{Disableable as _, IconName, v_flex};

use super::overview::HostState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundMachine {
    pub name: String,
    pub system: String,
    pub runs_zz: bool,
}

const CALM_STATUSES: [&str; 7] = [
    "",
    "Not connected",
    "Connecting…",
    "Connected",
    "Detached",
    "Authentication cancelled",
    "Disconnected. This action was not sent.",
];

pub fn host_state(connected: bool, busy: bool, status: &str) -> HostState {
    if connected {
        HostState::Connected
    } else if busy {
        HostState::Connecting
    } else if CALM_STATUSES.contains(&status) {
        HostState::Offline
    } else {
        HostState::Failed
    }
}

fn system_name(system: &str) -> &str {
    match system.to_ascii_lowercase().as_str() {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        _ => system,
    }
}

pub fn found_detail(machine: &FoundMachine) -> String {
    if !machine.runs_zz {
        return "No zz on it yet".to_owned();
    }
    match system_name(&machine.system) {
        "" => "Runs zz".to_owned(),
        system => format!("{system} · runs zz"),
    }
}

fn found_icon(machine: &FoundMachine) -> IconName {
    if system_name(&machine.system) == "macOS" {
        IconName::Laptop
    } else {
        IconName::HardDrive
    }
}

pub fn found_machines(
    found: &[FoundMachine],
    on_add: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Option<Div> {
    if found.is_empty() {
        return None;
    }
    let on_add = Rc::new(on_add);
    let rows = found.iter().enumerate().map(|(index, machine)| {
        let on_add = Rc::clone(&on_add);
        TouchRow::new(("found-machine", index), machine.name.clone())
            .icon(found_icon(machine))
            .detail(found_detail(machine))
            .trailing(TouchTrailing::Glyph(IconName::Plus))
            .disabled(!machine.runs_zz)
            .on_click(move |_, window, cx| on_add(index, window, cx))
    });
    Some(
        v_flex()
            .w_full()
            .child(TouchHeader::new("Found on your tailnet").count(found.len()))
            .child(TouchGroup::new().rows(rows)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_fails_only_on_a_status_that_is_not_calm() {
        assert_eq!(host_state(true, false, "Connected"), HostState::Connected);
        assert_eq!(
            host_state(false, true, "Connecting…"),
            HostState::Connecting
        );
        assert_eq!(
            host_state(false, false, "Not connected"),
            HostState::Offline
        );
        assert_eq!(
            host_state(false, false, "Authentication cancelled"),
            HostState::Offline
        );
        assert_eq!(
            host_state(
                false,
                false,
                "ssh: connect to host xps port 22: Connection refused"
            ),
            HostState::Failed
        );
    }

    #[test]
    fn found_rows_say_the_system_and_whether_zz_runs() {
        let found = |system: &str, runs_zz| FoundMachine {
            name: "xps".to_owned(),
            system: system.to_owned(),
            runs_zz,
        };
        assert_eq!(found_detail(&found("linux", true)), "Linux · runs zz");
        assert_eq!(found_detail(&found("macOS", true)), "macOS · runs zz");
        assert_eq!(found_detail(&found("", true)), "Runs zz");
        assert_eq!(found_detail(&found("linux", false)), "No zz on it yet");
    }
}
