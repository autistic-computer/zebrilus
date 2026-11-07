use std::{error::Error, io};

use serde::Deserialize;
use serde_json::json;
use zz_client::place::{
    self, Fleet, FleetHost, FleetPane, FleetSession, FleetWindow, Level, Part, Place, Written,
};
use zz_client::{
    completion::PaneKindAvailability,
    verbs::{self, Run, Slot},
};
use zz_protocol::{
    CommandValueKind, PaneId, SessionId, WindowId, catalog_command_spec, command_specs,
};

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
    place: PlaceWire,
    level: u8,
    query: String,
    target: String,
    command: String,
    boosts: Vec<(PlaceWire, i64)>,
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

fn place(bytes: &mut Vec<u8>, value: &Place) {
    optional(bytes, value.host.map(u64::from));
    optional(bytes, value.session.map(|id| id.0));
    optional(bytes, value.window.map(|id| id.0));
    optional(bytes, value.pane.map(|id| id.0));
}

fn value_kind(value: CommandValueKind) -> u8 {
    match value {
        CommandValueKind::FreeForm => 0,
        CommandValueKind::Session => 1,
        CommandValueKind::Window => 2,
        CommandValueKind::Pane => 3,
        CommandValueKind::Layout => 4,
        CommandValueKind::PaneKind => 5,
        CommandValueKind::KeyTable => 6,
        CommandValueKind::SetOption => 7,
        CommandValueKind::Boolean => 8,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let fleets: Vec<_> = input.fleets.into_iter().map(fleet).collect();
    let mut bytes = Vec::new();
    let mut ends = Vec::new();
    for case in input.cases {
        let fleet = fleets.get(case.fleet).ok_or("missing fixture fleet")?;
        let scope = at(case.place);
        match case.tag {
            0 => {
                let line = place::parse(&case.query);
                optional_text(&mut bytes, line.command.as_deref());
                optional_text(&mut bytes, line.search.as_deref());
            }
            1 => {
                let value = place::read_written(&case.query, fleet);
                uint(&mut bytes, u64::from(value.is_some()));
                match value {
                    Some(Written::Place(value)) => {
                        place(&mut bytes, &value);
                        optional_text(&mut bytes, None);
                        optional(&mut bytes, None);
                    }
                    Some(Written::Path { host, path, line }) => {
                        place(&mut bytes, &Place::host(host));
                        optional_text(&mut bytes, Some(&path));
                        optional(&mut bytes, line.map(u64::from));
                    }
                    None => {}
                }
            }
            2 => optional_text(&mut bytes, place::written(fleet, &scope).as_deref()),
            3..=5 => {
                let boost = |value: &Place| {
                    case.boosts
                        .iter()
                        .rev()
                        .find(|(key, _)| at(*key) == *value)
                        .map_or(0, |(_, score)| *score)
                };
                let found = match case.tag {
                    3 => place::candidates(
                        fleet,
                        &scope,
                        &Part {
                            level: *Level::ALL
                                .get(usize::from(case.level))
                                .ok_or("invalid fixture level")?,
                            query: case.query,
                        },
                        &boost,
                    ),
                    4 => place::children(fleet, &scope, &boost),
                    _ => place::search(fleet, &scope, &case.query, &boost),
                };
                uint(&mut bytes, found.len() as u64);
                for row in found {
                    place(&mut bytes, &row.place);
                    uint(&mut bytes, row.level as u64);
                    text(&mut bytes, &row.label);
                    uint(&mut bytes, row.matches.len() as u64);
                    for hit in row.matches {
                        uint(&mut bytes, hit.start as u64);
                        uint(&mut bytes, hit.end as u64);
                    }
                    uint(
                        &mut bytes,
                        row.score.cast_unsigned().wrapping_shl(1)
                            ^ (row.score >> 63).cast_unsigned(),
                    );
                }
            }
            6 => place(&mut bytes, &fleet.current()),
            7 => optional(&mut bytes, fleet.landing_pane(&scope).map(|pane| pane.id.0)),
            8 => {
                uint(&mut bytes, u64::from(place::has_target(&case.query)));
                text(&mut bytes, &place::with_target(&case.query, &case.target));
            }
            9 => optional_text(
                &mut bytes,
                catalog_command_spec(&case.command)
                    .and_then(|spec| place::command_target(spec, &scope))
                    .as_deref(),
            ),
            _ => return Err("invalid fixture operation".into()),
        }
        ends.push(bytes.len());
    }
    let base = verbs::verbs(
        PaneKindAvailability {
            browser: true,
            editor: true,
        },
        &[],
        true,
        "",
    );
    let catalog: Vec<_> = command_specs().map(|spec| {
        let option = spec.option("-t").filter(|value| !value.unsupported);
        let code = option.map_or(0, |option| option.value.map_or(1, |kind| value_kind(kind) + 2));
        let text = base.iter().find(|verb| matches!(&verb.run, Run::Tmux{command,args} if command == spec.name && args.is_empty())).and_then(|verb| verb.slots.iter().find_map(|slot| match slot {Slot::Text(label) => Some(label.as_str()), _ => None})).unwrap_or("");
        json!([spec.name, spec.aliases, code, spec.positional_minimum(), spec.positionals.len() == 1 && spec.variadic.is_none(), text])
    }).collect();
    println!(
        "{}",
        serde_json::to_string(&json!({"bytes":bytes,"ends":ends,"catalog":catalog}))?
    );
    Ok(())
}
