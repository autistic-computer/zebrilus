use std::{error::Error, io};

use ala::SharedString;
use penna::{
    IconName,
    blocks::palette::{PalettePill, PaletteStatus},
};
use serde::Deserialize;
use serde_json::json;
use zz_client::{completion::PaneKindAvailability, place::Place, verbs::Filled};
use zz_protocol::{PaneId, SessionId, WindowId};
use zz_ui::command::{
    PaletteHostId, PaletteTree, PaletteTreeHost, PaletteTreePane, PaletteTreeSession,
    PaletteTreeWindow,
    palette_diagnostics::{PaletteFixtureAction, PalettePlacesFixture, palette_places_fixture},
};

type PlaceWire = (Option<u32>, Option<u64>, Option<u64>, Option<u64>);
type PaneWire = (u64, u32, String, Option<String>, String, bool, bool);
type WindowWire = (u64, u32, String, bool, u64, Vec<PaneWire>);
type SessionWire = (u64, String, String, Vec<WindowWire>);
type HostWire = (u32, String, bool, Option<u64>, Vec<SessionWire>);
type FleetWire = (u32, Vec<HostWire>);

#[derive(Deserialize)]
struct Case {
    fleet: usize,
    chips: Vec<(PlaceWire, bool)>,
    draft: Option<(usize, Vec<(u8, PlaceWire, String, String)>)>,
    query: String,
    history: Vec<String>,
    aliases: String,
    browser: bool,
    editor: bool,
    insert: bool,
}

#[derive(Deserialize)]
struct Input {
    fleets: Vec<FleetWire>,
    cases: Vec<Case>,
    #[serde(default)]
    statuses: Vec<(usize, u32, u8)>,
}

fn tree((here, hosts): FleetWire) -> PaletteTree {
    PaletteTree {
        attached: PaletteHostId(here),
        chats: Vec::new(),
        chat_mode: false,
        hosts: hosts
            .into_iter()
            .map(|(id, name, online, current, sessions)| {
                let right = format!("{} sessions", sessions.len());
                PaletteTreeHost {
                    id: PaletteHostId(id),
                    name,
                    detail: String::new(),
                    right,
                    status: if online {
                        PaletteStatus::Online
                    } else {
                        PaletteStatus::Offline
                    },
                    sessions: sessions
                        .into_iter()
                        .map(|(id, name, label, windows)| PaletteTreeSession {
                            id: SessionId(id),
                            name: label,
                            tmux_name: name,
                            active: current == Some(id),
                            windows: windows
                                .into_iter()
                                .map(|(id, index, name, active, active_pane, panes)| {
                                    PaletteTreeWindow {
                                        id: WindowId(id),
                                        index,
                                        name,
                                        active,
                                        active_pane: PaneId(active_pane),
                                        panes: panes
                                            .into_iter()
                                            .map(
                                                |(
                                                    id,
                                                    index,
                                                    title,
                                                    program,
                                                    cwd,
                                                    terminal,
                                                    needs_you,
                                                )| {
                                                    let detail = format!("pane:{title}");
                                                    let running =
                                                        program.as_deref() == Some("running");
                                                    PaletteTreePane {
                                                        id: PaneId(id),
                                                        index,
                                                        label: title,
                                                        program: program.map(SharedString::from),
                                                        detail: detail.into(),
                                                        icon: if terminal {
                                                            IconName::SquareTerminal
                                                        } else {
                                                            IconName::Plus
                                                        },
                                                        running,
                                                        needs_you,
                                                        terminal,
                                                        cwd,
                                                    }
                                                },
                                            )
                                            .collect(),
                                    }
                                })
                                .collect(),
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

fn status(value: PaletteStatus) -> u64 {
    match value {
        PaletteStatus::Online => 0,
        PaletteStatus::Running => 1,
        PaletteStatus::Waiting => 2,
        PaletteStatus::Offline => 3,
    }
}

fn at(value: PlaceWire) -> Place {
    Place {
        host: value.0,
        session: value.1.map(SessionId),
        window: value.2.map(WindowId),
        pane: value.3.map(PaneId),
    }
}

fn uint(bytes: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        bytes.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
}

fn text(bytes: &mut Vec<u8>, value: &str) {
    uint(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn optional(bytes: &mut Vec<u8>, value: Option<u64>) {
    uint(bytes, u64::from(value.is_some()));
    if let Some(value) = value {
        uint(bytes, value);
    }
}

fn place(bytes: &mut Vec<u8>, value: &Place) {
    optional(bytes, value.host.map(u64::from));
    optional(bytes, value.session.map(|id| id.0));
    optional(bytes, value.window.map(|id| id.0));
    optional(bytes, value.pane.map(|id| id.0));
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let mut trees: Vec<_> = input.fleets.into_iter().map(tree).collect();
    for (index, host, value) in input.statuses {
        let host = trees
            .get_mut(index)
            .and_then(|tree| tree.hosts.iter_mut().find(|entry| entry.id.0 == host))
            .ok_or("missing status fixture host")?;
        host.status = match value {
            0 => PaletteStatus::Online,
            1 => PaletteStatus::Running,
            2 => PaletteStatus::Waiting,
            3 => PaletteStatus::Offline,
            _ => return Err("invalid status fixture".into()),
        };
    }
    let mut bytes = Vec::new();
    let mut ends = Vec::new();
    let mut chrome = Vec::new();
    let mut chrome_ends = Vec::new();
    for case in input.cases {
        let tree = trees.get(case.fleet).ok_or("missing fixture tree")?.clone();
        let draft = case.draft.map(|(index, filled)| {
            (
                index,
                filled
                    .into_iter()
                    .map(|(kind, place, text, label)| {
                        (
                            if kind == 0 {
                                Filled::Place(at(place))
                            } else {
                                Filled::Text(text)
                            },
                            label,
                        )
                    })
                    .collect(),
            )
        });
        let result = palette_places_fixture(PalettePlacesFixture {
            tree,
            chips: case
                .chips
                .into_iter()
                .map(|(place, auto)| (at(place), auto))
                .collect(),
            draft,
            query: case.query,
            history: case.history,
            aliases: case.aliases,
            availability: PaneKindAvailability {
                browser: case.browser,
                editor: case.editor,
            },
            insert_path: case.insert,
        });
        uint(&mut chrome, result.usage.len() as u64);
        for usage in result.usage {
            uint(&mut chrome, u64::from(usage.is_some()));
            if let Some(usage) = usage {
                text(&mut chrome, &usage);
            }
        }
        chrome_ends.push(chrome.len());
        uint(&mut bytes, result.rows.len() as u64);
        for (row, action) in result.rows {
            text(&mut bytes, &row.label);
            text(&mut bytes, &row.detail);
            text(&mut bytes, &row.right);
            uint(&mut bytes, row.matches.len() as u64);
            for hit in row.matches {
                uint(&mut bytes, hit.start as u64);
                uint(&mut bytes, hit.end as u64);
            }
            uint(&mut bytes, row.muted_prefix as u64);
            optional(&mut bytes, row.status.map(status));
            uint(&mut bytes, u64::from(row.running));
            text(
                &mut bytes,
                &row.icon
                    .map_or_else(String::new, |icon| format!("{icon:?}")),
            );
            uint(&mut bytes, u64::from(row.indent.to_bits()));
            uint(&mut bytes, u64::from(row.expanded.is_some()));
            if let Some(value) = row.expanded {
                uint(&mut bytes, u64::from(value));
            }
            uint(&mut bytes, u64::from(row.shortcut.is_some()));
            let (kind, target, index, command) = match action {
                PaletteFixtureAction::None => (0, Place::default(), 0, String::new()),
                PaletteFixtureAction::Place(place) => (1, place, 0, String::new()),
                PaletteFixtureAction::Verb(index) => (2, Place::default(), index, String::new()),
                PaletteFixtureAction::Command(command) => (3, Place::default(), 0, command),
                PaletteFixtureAction::History(command) => (4, Place::default(), 0, command),
                PaletteFixtureAction::Fill(place) => (5, place, 0, String::new()),
                PaletteFixtureAction::Submit => (6, Place::default(), 0, String::new()),
                PaletteFixtureAction::Other => return Err("unexpected fixture action".into()),
            };
            uint(&mut bytes, kind);
            place(&mut bytes, &target);
            uint(&mut bytes, index as u64);
            text(&mut bytes, &command);
        }
        uint(&mut bytes, result.pills.len() as u64);
        for pill in result.pills {
            let (kind, prefix, label, state) = match pill {
                PalettePill::Mode { prefix, label } => (0, prefix, label, None),
                PalettePill::Status {
                    label,
                    status: state,
                } => (1, "".into(), label, Some(state)),
                PalettePill::Code(label) => (2, "".into(), label, None),
                PalettePill::Crumb(label) => (3, "".into(), label, None),
                PalettePill::Action(label) => (4, "".into(), label, None),
            };
            uint(&mut bytes, kind);
            text(&mut bytes, &prefix);
            text(&mut bytes, &label);
            optional(&mut bytes, state.map(status));
        }
        text(&mut bytes, &result.placeholder);
        optional(&mut bytes, result.initial.map(|index| index as u64));
        ends.push(bytes.len());
    }
    println!(
        "{}",
        serde_json::to_string(
            &json!({"bytes":bytes,"ends":ends,"chrome":chrome,"chrome_ends":chrome_ends})
        )?
    );
    Ok(())
}
