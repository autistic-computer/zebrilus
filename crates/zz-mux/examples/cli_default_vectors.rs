use std::{
    collections::BTreeMap,
    error::Error,
    io::{self, Read, Write},
};

use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let request: Value = serde_json::from_str(&input)?;
    let mut cases = Vec::new();
    for item in request["cases"].as_array().into_iter().flatten() {
        let text = item["text"]
            .as_str()
            .ok_or("missing default command text")?;
        let injected = item["homes"].as_object();
        let homes: BTreeMap<String, String> = injected
            .into_iter()
            .flatten()
            .filter_map(|(name, value)| {
                value.as_str().map(|value| (name.clone(), value.to_owned()))
            })
            .collect();
        let parsed = if injected.is_some() {
            zz_mux::parse_config_with_expansions(
                "default-client-command",
                text,
                &homes,
                &BTreeMap::new(),
            )
        } else {
            zz_mux::parse_config("default-client-command", text)
        };
        let commands: Vec<_> = parsed
            .commands
            .iter()
            .map(|command| {
                json!({
                    "name": command.name,
                    "args": command.args.iter().map(zz_protocol::RawText::as_bytes).collect::<Vec<_>>(),
                    "source": command.source,
                    "blocks": (0..command.args.len()).filter(|index| command.argument_is_command_block(*index)).collect::<Vec<_>>()
                })
            })
            .collect();
        let diagnostics: Vec<_> = parsed
            .diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "source": diagnostic.source,
                    "line": diagnostic.line,
                    "column": diagnostic.column,
                    "message": diagnostic.message,
                })
            })
            .collect();
        let names = zz_mux::config_expansion_names("default-client-command", text);
        let mut resolved = serde_json::Map::new();
        for name in names.homes {
            let home = if injected.is_some() {
                homes.get(&name).cloned()
            } else {
                zz_mux::user_home((!name.is_empty()).then_some(name.as_str()))
            };
            resolved.insert(name, json!(home));
        }
        let current_home = if injected.is_some() {
            homes.get("").cloned()
        } else {
            zz_mux::user_home(None)
        };
        cases.push(json!({"commands": commands, "diagnostics": diagnostics, "homes": resolved, "home": current_home}));
    }
    let users: Vec<_> = request["users"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|user| {
            let name = user.as_str();
            json!({"user": name, "home": zz_mux::user_home(name)})
        })
        .collect();
    #[cfg(target_os = "linux")]
    let abi = json!({
        "record": std::mem::size_of::<libc::passwd>(),
        "directory": std::mem::offset_of!(libc::passwd, pw_dir),
        "pointer": std::mem::size_of::<*const ()>(),
    });
    #[cfg(not(target_os = "linux"))]
    let abi = Value::Null;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(
        &mut stdout,
        &json!({"cases": cases, "users": users, "abi": abi}),
    )?;
    stdout.write_all(b"\n")?;
    Ok(())
}
