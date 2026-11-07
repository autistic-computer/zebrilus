use std::io::{self, Write};

use serde_json::{Value, json};
use zz_protocol::RawText;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Value = serde_json::from_reader(io::stdin().lock())?;
    let cases: Vec<Vec<Vec<u8>>> = serde_json::from_value(request["cases"].clone())?;
    let output: Vec<Value> = cases
        .into_iter()
        .map(|arguments| {
            match zz_cli::parse_native_attach_arguments(
                arguments.into_iter().map(RawText::from_bytes),
            ) {
                Ok(options) => {
                    let command = zz_cli::native_attach_command(&options);
                    let default_attach = !options.no_update_environment
                        && options.working_directory.is_none()
                        && !options.detach_others_hangup
                        && options.session.is_none();
                    json!({
                        "command": {
                            "name": command.name,
                            "args": command.args.iter().map(RawText::as_bytes).collect::<Vec<_>>(),
                        },
                        "restart": options.restart_daemon,
                        "default_attach": default_attach,
                        "error": null,
                        "code": 0,
                    })
                }
                Err(error) => {
                    let (code, message) = zz_cli::native_attach_argument_error_output(error);
                    json!({"error": message, "code": code})
                }
            }
        })
        .collect();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer(
        &mut stdout,
        &json!({
            "cases": output,
            "native_usage": zz_cli::NATIVE_ATTACH_USAGE,
            "tmux_usage": format!("usage: attach-session {}", zz_protocol::catalog_command_spec("attach-session").ok_or("missing attach-session catalog")?.pinned_tmux_usage()),
        }),
    )?;
    stdout.write_all(b"\n")?;
    Ok(())
}
