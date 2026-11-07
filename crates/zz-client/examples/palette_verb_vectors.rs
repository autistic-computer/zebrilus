use std::{error::Error, io};

use serde::Deserialize;
use serde_json::json;
use zz_client::place::{Fleet, FleetHost, FleetPane, FleetSession, FleetWindow, Place};
use zz_client::{
    completion::{self, PaneKindAvailability},
    gui_actions::{self, GUI_ACTIONS},
    verbs::{self, Filled, Run, Slot, Verb, VerbKind},
};
use zz_protocol::{PaneId, SessionId, WindowId};

type PlaceWire = (Option<u32>, Option<u64>, Option<u64>, Option<u64>);
type PaneWire = (u64, u32, String, Option<String>, String, bool, bool);
type WindowWire = (u64, u32, String, bool, u64, Vec<PaneWire>);
type SessionWire = (u64, String, String, Vec<WindowWire>);
type HostWire = (u32, String, bool, Option<u64>, Vec<SessionWire>);
type FleetWire = (u32, Vec<HostWire>);

#[derive(Deserialize)]
struct Case {
    tag: u8,
    fleet: usize,
    browser: bool,
    editor: bool,
    insert: bool,
    aliases: String,
    gui: Vec<String>,
    query: String,
    level: Option<u8>,
    index: usize,
    filled: Vec<(u8, PlaceWire, String)>,
    place: PlaceWire,
}

#[derive(Deserialize)]
struct Input {
    fleets: Vec<FleetWire>,
    cases: Vec<Case>,
}

fn fleet((here, hosts): FleetWire) -> Fleet {
    Fleet {
        here,
        hosts: hosts
            .into_iter()
            .map(|(id, name, online, current, sessions)| FleetHost {
                id,
                name,
                online,
                current: current.map(SessionId),
                sessions: sessions
                    .into_iter()
                    .map(|(id, name, label, windows)| FleetSession {
                        id: SessionId(id),
                        name,
                        label,
                        windows: windows
                            .into_iter()
                            .map(
                                |(id, index, name, active, active_pane, panes)| FleetWindow {
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
                                                FleetPane {
                                                    id: PaneId(id),
                                                    index,
                                                    title,
                                                    program,
                                                    cwd,
                                                    terminal,
                                                    needs_you,
                                                }
                                            },
                                        )
                                        .collect(),
                                },
                            )
                            .collect(),
                    })
                    .collect(),
            })
            .collect(),
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

fn optional_text(bytes: &mut Vec<u8>, value: Option<&str>) {
    uint(bytes, u64::from(value.is_some()));
    if let Some(value) = value {
        text(bytes, value);
    }
}

fn write_verb(bytes: &mut Vec<u8>, verb: &Verb) {
    text(bytes, &verb.name);
    text(bytes, &verb.keywords);
    text(bytes, &verb.description);
    uint(
        bytes,
        match verb.kind {
            VerbKind::Tmux => 0,
            VerbKind::Gui => 1,
            VerbKind::Zz => 2,
            VerbKind::Alias => 3,
        },
    );
    match &verb.run {
        Run::Tmux { command, args } => {
            uint(bytes, 0);
            text(bytes, command);
            text(bytes, "");
            uint(bytes, args.len() as u64);
            for arg in args {
                text(bytes, arg);
            }
        }
        Run::Alias { body, command, .. } => {
            uint(bytes, 1);
            text(bytes, command);
            text(bytes, body);
            uint(bytes, 0);
        }
        Run::Gui(id) => {
            uint(bytes, 2);
            text(bytes, id);
            text(bytes, "");
            uint(bytes, 0);
        }
        Run::InsertPath => {
            uint(bytes, 3);
            text(bytes, "");
            text(bytes, "");
            uint(bytes, 0);
        }
    }
    uint(bytes, verb.slots.len() as u64);
    for slot in &verb.slots {
        match slot {
            Slot::Target(level) => {
                uint(bytes, 0);
                uint(bytes, *level as u64);
                text(bytes, "");
            }
            Slot::Text(label) => {
                uint(bytes, 1);
                uint(bytes, 0);
                text(bytes, label);
            }
            Slot::Value(label) => {
                uint(bytes, 2);
                uint(bytes, 0);
                text(bytes, label);
            }
            Slot::Path => {
                uint(bytes, 3);
                uint(bytes, 0);
                text(bytes, "");
            }
        }
    }
    uint(bytes, u64::from(verb.raw));
    text(bytes, &verb.spelled());
    optional(bytes, verb.target().map(|level| level as u64));
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let fleets: Vec<_> = input.fleets.into_iter().map(fleet).collect();
    let mut bytes = Vec::new();
    let mut ends = Vec::new();
    for case in input.cases {
        let fleet = fleets.get(case.fleet).ok_or("missing fixture fleet")?;
        let gui: Vec<_> = case
            .gui
            .iter()
            .filter_map(|id| gui_actions::gui_action(id).map(|action| action.id))
            .collect();
        let verbs = verbs::verbs(
            PaneKindAvailability {
                browser: case.browser,
                editor: case.editor,
            },
            &gui,
            case.insert,
            &case.aliases,
        );
        match case.tag {
            0 => {
                uint(&mut bytes, verbs.len() as u64);
                for verb in &verbs {
                    write_verb(&mut bytes, verb);
                }
            }
            1 => {
                let rows = verbs::match_verbs(&verbs, &case.query, |verb| {
                    case.level.is_none_or(|level| {
                        verb.target().is_some_and(|target| target as u8 == level)
                    })
                });
                uint(&mut bytes, rows.len() as u64);
                for row in rows {
                    uint(&mut bytes, row.index as u64);
                    uint(
                        &mut bytes,
                        row.score.cast_unsigned().wrapping_shl(1)
                            ^ (row.score >> 63).cast_unsigned(),
                    );
                    uint(&mut bytes, row.matches.len() as u64);
                    for hit in row.matches {
                        uint(&mut bytes, hit.start as u64);
                        uint(&mut bytes, hit.end as u64);
                    }
                }
            }
            2 => {
                let filled: Vec<_> = case
                    .filled
                    .into_iter()
                    .map(|(kind, place, text)| {
                        if kind == 0 {
                            Filled::Place(at(place))
                        } else {
                            Filled::Text(text)
                        }
                    })
                    .collect();
                let compiled = verbs
                    .get(case.index)
                    .and_then(|verb| verbs::compile(verb, &filled, fleet));
                uint(&mut bytes, u64::from(compiled.is_some()));
                if let Some(value) = compiled {
                    optional(&mut bytes, value.host.map(u64::from));
                    text(&mut bytes, &value.line);
                    text(&mut bytes, &value.display);
                    uint(&mut bytes, u64::from(value.invocation.is_some()));
                    if let Some(invocation) = value.invocation {
                        text(&mut bytes, &invocation.name);
                        uint(&mut bytes, invocation.args.len() as u64);
                        for arg in invocation.args {
                            text(
                                &mut bytes,
                                arg.to_utf8().ok_or("fixture argument is not UTF-8")?,
                            );
                        }
                    }
                }
            }
            3 => text(&mut bytes, &verbs::quote(&case.query)),
            4 => {
                let words = completion::command_words(&case.query);
                uint(&mut bytes, words.len() as u64);
                for word in words {
                    text(&mut bytes, &word);
                }
            }
            5 => {
                optional_text(
                    &mut bytes,
                    verbs::target_name(fleet, &at(case.place)).as_deref(),
                );
                optional_text(
                    &mut bytes,
                    verbs::go_to_line(fleet, &at(case.place)).as_deref(),
                );
            }
            _ => return Err("invalid fixture operation".into()),
        }
        ends.push(bytes.len());
    }
    let catalog: Vec<_> = verbs::verbs(
        PaneKindAvailability {
            browser: true,
            editor: true,
        },
        &[],
        true,
        "",
    )
    .iter()
    .map(|verb| json!([verb.name, verb.slots.len()]))
    .collect();
    let gui: Vec<_> = GUI_ACTIONS
        .iter()
        .map(|action| {
            json!([
                action.id,
                action.title,
                action.verb,
                action.aliases.join(" ")
            ])
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string(&json!({"bytes":bytes,"ends":ends,"gui":gui,"catalog":catalog}))?
    );
    Ok(())
}
