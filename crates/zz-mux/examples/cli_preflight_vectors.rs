use std::{collections::BTreeMap, error::Error, io};

use serde::Deserialize;
use serde_json::{Value, json};
use zz_protocol::{
    CommandInvocation, CommandResolution, NATIVE_COMMAND_NAMES, RawText, command_specs,
    resolve_command, unimplemented_tmux_command_spec,
};

#[derive(Deserialize)]
struct InputCommand {
    name: String,
    args: Vec<Vec<u8>>,
}

#[derive(Deserialize)]
struct Input {
    cases: Vec<Vec<InputCommand>>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let input: Input = serde_json::from_reader(io::stdin().lock())?;
    let mut specs = command_specs()
        .map(|spec| (spec.name, spec))
        .collect::<BTreeMap<_, _>>();
    for name in ["link-window", "unlink-window"] {
        specs.insert(
            name,
            unimplemented_tmux_command_spec(name).ok_or("missing spec")?,
        );
    }
    let catalog = specs
        .values()
        .map(|spec| {
            json!({
                "name": spec.name,
                "aliases": spec.aliases,
                "native": NATIVE_COMMAND_NAMES.contains(&spec.name),
                "minimum": spec.positional_minimum(),
                "maximum": spec.positional_maximum(),
                "usage": spec.pinned_tmux_usage(),
                "options": spec.options.iter().map(|option| json!({
                    "name": option.name,
                    "value": option.value.is_some(),
                    "attached": option.attached_value,
                    "optional": option.optional_value,
                    "native": option.native,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let cases = input.cases.into_iter().map(|commands| {
        let commands = commands.into_iter().map(|command| {
            CommandInvocation::new(command.name, command.args.into_iter().map(RawText::from_bytes))
        }).collect::<Vec<_>>();
        let first = commands.first().map_or("", |command| command.name.as_str());
        let canonical = match resolve_command(first) {
            CommandResolution::Canonical(name) | CommandResolution::Unimplemented(name) => name,
            _ => first,
        };
        let starts_server = matches!(canonical, "attach-session" | "list-commands" | "list-keys" | "new-session" | "start-server");
        match zz_mux::validate_static_command_chain(&commands) {
            Ok(()) => json!({"canonical": canonical, "starts_server": starts_server, "code": 0, "error": null}),
            Err(error) => json!({"canonical": canonical, "starts_server": starts_server, "code": error.exit_code(), "error": error.tmux_message()}),
        }
    }).collect::<Vec<Value>>();
    println!(
        "{}",
        serde_json::to_string(&json!({"catalog": catalog, "cases": cases}))?
    );
    Ok(())
}
