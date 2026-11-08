use std::{error::Error, io};

use serde::Deserialize;
use serde_json::{Value, json};
use zz_protocol::{
    ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeState, ChooseTreeTarget,
    ClientId, PaneId, SessionId, WindowId,
};
use zz_ui::command::window_chooser_fixture;

type Item = (String, String, u8, u64, u8, u8, Option<u8>, String, String);

#[derive(Deserialize)]
struct Trace {
    initial: usize,
    grouped: bool,
    operations: Vec<Value>,
}

#[derive(Deserialize)]
struct Input {
    datasets: Vec<Vec<Item>>,
    #[serde(default)]
    selected: Vec<u32>,
    traces: Vec<Trace>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let states: Vec<_> = input
        .datasets
        .into_iter()
        .enumerate()
        .map(|(index, items)| ChooseTreeState {
            items: items
                .into_iter()
                .map(
                    |(label, detail, kind, id, depth, flags, pane, key, text)| ChooseTreeItem {
                        label,
                        detail,
                        target: match kind {
                            0 => ChooseTreeTarget::Session(SessionId(id)),
                            1 => ChooseTreeTarget::Window(WindowId(id)),
                            2 => ChooseTreeTarget::Pane(PaneId(id)),
                            _ => ChooseTreeTarget::Client(ClientId(id)),
                        },
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
            selected: input.selected.get(index).copied().unwrap_or(0),
            search: None,
            kind: ChooseTreeKind::Windows,
            filter_no_matches: false,
            prompt: String::new(),
            help: false,
        })
        .collect();
    let traces: Vec<_> = input
        .traces
        .iter()
        .map(|trace| {
            window_chooser_fixture::trace(&states, trace.initial, trace.grouped, &trace.operations)
        })
        .collect();
    serde_json::to_writer(io::stdout().lock(), &json!({"traces":traces}))?;
    Ok(())
}
