use std::{
    collections::BTreeSet,
    error::Error,
    io::{self, Write},
};

use serde_json::{Value, json};
use zz_client::{
    completion::PaneKindAvailability,
    palette::{needs_arguments, target_kind},
};
use zz_protocol::{CommandValueKind, command_specs};

fn kind(value: CommandValueKind) -> u8 {
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
    let specs: Vec<_> = command_specs().collect();
    let mut catalog = Vec::new();
    let mut queries: BTreeSet<String> = [
        "",
        " ",
        "\t\u{a0}\u{202f}",
        "pane",
        "session",
        "window",
        "-",
        "zz",
        "PANE",
        "SP",
        "ls",
        "splitw",
        "not-a-command",
        "é",
        "\u{212A}",
        "İ",
        "\0",
        "p\u{200b}n",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for spec in &specs {
        let allowed: Vec<_> = [false, true]
            .into_iter()
            .flat_map(|browser| {
                [false, true].map(move |editor| {
                    PaneKindAvailability { browser, editor }.allows_command(spec.name)
                })
            })
            .collect();
        catalog.push(json!({
            "name": spec.name,
            "aliases": spec.aliases,
            "description": spec.description,
            "usage": spec.usage,
            "target": target_kind(spec).map(kind),
            "needs_arguments": needs_arguments(spec),
            "allowed": allowed,
        }));
        queries.insert(spec.name.to_owned());
        queries.insert(spec.name.to_uppercase());
        queries.insert(spec.name.chars().take(2).collect());
        queries.insert(spec.name.chars().step_by(2).collect());
        queries.insert(format!(" \u{a0}{}\t", spec.name));
        for alias in spec.aliases {
            queries.insert((*alias).to_owned());
        }
    }
    let mut cases = Vec::new();
    for query in queries {
        for browser in [false, true] {
            for editor in [false, true] {
                let rows = zz_ui::command::palette_command_rows(
                    &query,
                    PaneKindAvailability { browser, editor },
                );
                let rows: Vec<Value> = rows
                    .into_iter()
                    .map(|row| {
                        let index = specs.iter().position(|spec| spec.name == row.label.as_ref());
                        json!({
                            "index": index,
                            "label": row.label.as_ref(),
                            "description": row.detail.as_ref(),
                            "hits": row.matches.into_iter().map(|range| [range.start, range.end]).collect::<Vec<_>>()
                        })
                    })
                    .collect();
                cases.push(
                    json!({"query": query, "browser": browser, "editor": editor, "rows": rows}),
                );
            }
        }
    }
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut stdout, &json!({"catalog": catalog, "cases": cases}))?;
    stdout.write_all(b"\n")?;
    Ok(())
}
