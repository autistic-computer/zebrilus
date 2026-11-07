use std::{error::Error, io};

use serde::Deserialize;
use serde_json::json;
use zz_client::chooser::WindowChooserSearch;
use zz_protocol::{
    ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeState, ChooseTreeTarget,
    ClientId, PaneId, SessionId, WindowId,
};
use zz_ui::command::palette_diagnostics::palette_window_fixture;

type Item = (String, String, u8, u64, u8, u8, Option<u8>, String, String);
type Target = (u8, u64);

#[derive(Deserialize)]
struct Case {
    dataset: usize,
    query: String,
    grouped: bool,
}

#[derive(Deserialize)]
struct Seed {
    expanded: Vec<Target>,
    pending: Option<(Target, bool)>,
    selection: Option<Target>,
}

#[derive(Deserialize)]
struct Trace {
    seed: Seed,
    steps: Vec<(usize, bool)>,
}

#[derive(Deserialize)]
struct Input {
    datasets: Vec<Vec<Item>>,
    cases: Vec<Case>,
    traces: Vec<Trace>,
}

fn target((kind, id): Target) -> ChooseTreeTarget {
    match kind {
        0 => ChooseTreeTarget::Session(SessionId(id)),
        1 => ChooseTreeTarget::Window(WindowId(id)),
        2 => ChooseTreeTarget::Pane(PaneId(id)),
        _ => ChooseTreeTarget::Client(ClientId(id)),
    }
}

fn target_parts(value: ChooseTreeTarget) -> Target {
    match value {
        ChooseTreeTarget::Session(id) => (0, id.0),
        ChooseTreeTarget::Window(id) => (1, id.0),
        ChooseTreeTarget::Pane(id) => (2, id.0),
        ChooseTreeTarget::Client(id) => (3, id.0),
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

fn optional(bytes: &mut Vec<u8>, value: Option<usize>) {
    uint(bytes, u64::from(value.is_some()));
    if let Some(value) = value {
        uint(bytes, value as u64);
    }
}

fn append_target(bytes: &mut Vec<u8>, value: ChooseTreeTarget) {
    let (kind, id) = target_parts(value);
    uint(bytes, u64::from(kind));
    uint(bytes, id);
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let states: Vec<_> = input
        .datasets
        .into_iter()
        .map(|dataset| ChooseTreeState {
            items: dataset
                .into_iter()
                .map(
                    |(label, detail, kind, id, depth, flags, pane, key, text)| ChooseTreeItem {
                        label,
                        detail,
                        target: target((kind, id)),
                        depth,
                        flags,
                        pane_kind: pane.map(|kind| match kind {
                            0 => ChooseTreePaneKind::Terminal,
                            1 => ChooseTreePaneKind::Browser,
                            2 => ChooseTreePaneKind::Editor,
                            _ => ChooseTreePaneKind::Agent,
                        }),
                        key,
                        text,
                    },
                )
                .collect(),
            selected: 0,
            search: None,
            kind: ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        })
        .collect();
    let mut bytes = Vec::new();
    let mut rows = 0;
    for case in &input.cases {
        let result = palette_window_fixture(&states[case.dataset], &case.query, case.grouped);
        optional(
            &mut bytes,
            result.current_window.map(|index| index as usize),
        );
        optional(&mut bytes, result.initial);
        uint(&mut bytes, result.rows.len() as u64);
        rows += result.rows.len();
        for (row, source, score) in result.rows {
            text(&mut bytes, &row.label);
            text(&mut bytes, &row.detail);
            text(&mut bytes, &row.right);
            uint(&mut bytes, row.matches.len() as u64);
            for hit in row.matches {
                uint(&mut bytes, hit.start as u64);
                uint(&mut bytes, hit.end as u64);
            }
            uint(&mut bytes, row.muted_prefix as u64);
            uint(&mut bytes, u64::from(row.status.is_some()));
            uint(&mut bytes, u64::from(row.running));
            text(
                &mut bytes,
                &row.icon
                    .map_or_else(String::new, |icon| format!("{icon:?}")),
            );
            uint(&mut bytes, u64::from(row.indent.to_bits()));
            uint(&mut bytes, u64::from(row.expanded.is_some()));
            if let Some(expanded) = row.expanded {
                uint(&mut bytes, u64::from(expanded));
            }
            uint(&mut bytes, u64::from(row.shortcut.is_some()));
            uint(&mut bytes, u64::from(source));
            uint(&mut bytes, score as u64);
        }
        for index in result.parents.into_iter().chain(result.children) {
            optional(&mut bytes, index);
        }
    }
    let mut steps = 0;
    for trace in &input.traces {
        let mut search = WindowChooserSearch {
            expanded: trace.seed.expanded.iter().copied().map(target).collect(),
            pending: trace
                .seed
                .pending
                .map(|(value, expanded)| (target(value), expanded)),
            selection: trace.seed.selection.map(target),
        };
        for &(dataset, searching) in &trace.steps {
            let result = search.advance(&states[dataset], searching);
            uint(&mut bytes, u64::from(result.is_some()));
            if let Some((index, expanded)) = result {
                uint(&mut bytes, u64::from(index));
                uint(&mut bytes, u64::from(expanded));
            }
            uint(&mut bytes, search.expanded.len() as u64);
            for &target in &search.expanded {
                append_target(&mut bytes, target);
            }
            uint(&mut bytes, u64::from(search.pending.is_some()));
            if let Some((target, expanded)) = search.pending {
                append_target(&mut bytes, target);
                uint(&mut bytes, u64::from(expanded));
            }
            uint(&mut bytes, u64::from(search.selection.is_some()));
            if let Some(target) = search.selection {
                append_target(&mut bytes, target);
            }
            steps += 1;
        }
    }
    serde_json::to_writer(
        io::stdout().lock(),
        &json!({"bytes":bytes,"rows":rows,"cases":input.cases.len(),"steps":steps}),
    )?;
    Ok(())
}
