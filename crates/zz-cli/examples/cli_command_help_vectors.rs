use std::{collections::BTreeSet, error::Error, io};

use serde_json::{Value, json};
use zz_protocol::{
    CommandSpec, catalog_command_spec, command_specs, unimplemented_tmux_command_spec,
};

fn main() -> Result<(), Box<dyn Error>> {
    let request: Value = serde_json::from_reader(io::stdin().lock())?;
    let cases: Vec<String> = serde_json::from_value(request["cases"].clone())?;
    let names = command_specs()
        .map(|spec| spec.name)
        .chain(
            CommandSpec::UNIMPLEMENTED_TMUX_COMMANDS
                .iter()
                .filter_map(|name| unimplemented_tmux_command_spec(name))
                .map(|spec| spec.name),
        )
        .collect::<BTreeSet<_>>();
    let catalog = names
        .into_iter()
        .map(|name| {
            let spec = catalog_command_spec(name).or_else(|| unimplemented_tmux_command_spec(name));
            zz_cli::command_help(name).map(|help| {
                json!({"name": name, "help": help, "aliases": spec.map(|spec| spec.aliases).unwrap_or_default()})
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let results = cases
        .into_iter()
        .map(|name| match zz_cli::command_help(&name) {
            Ok(help) => json!({"code": 0, "help": help, "error": null}),
            Err(error) => {
                json!({"code": error.exit_code(), "help": "", "error": error.tmux_message()})
            }
        })
        .collect::<Vec<_>>();
    serde_json::to_writer(
        io::stdout().lock(),
        &json!({"catalog": catalog, "cases": results}),
    )?;
    Ok(())
}
