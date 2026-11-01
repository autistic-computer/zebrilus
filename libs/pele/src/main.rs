use pele::{PeleError, RenderOptions, render};
use serde::Deserialize;
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Request {
    source: String,
    #[serde(default)]
    options: RenderOptions,
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let result = serde_json::from_str::<Request>(&line)
            .map_err(|error| PeleError::syntax(error.to_string(), 0, 0))
            .and_then(|request| render(&request.source, &request.options));
        let response = match result {
            Ok(result) => {
                let mut value = serde_json::to_value(result).expect("serializable result");
                value["ok"] = true.into();
                value
            }
            Err(error) => serde_json::json!({"ok": false, "error": error}),
        };
        serde_json::to_writer(&mut stdout, &response)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}
