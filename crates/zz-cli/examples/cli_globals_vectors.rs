use std::io;
use std::os::unix::ffi::OsStringExt as _;
use std::path::PathBuf;

use serde_json::{Value, json};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input: Value = serde_json::from_reader(io::stdin().lock())?;
    let cases: Vec<Vec<Vec<u8>>> = serde_json::from_value(input["cases"].clone())?;
    let features: Vec<Vec<String>> = serde_json::from_value(input["features"].clone())?;
    let masks = features
        .iter()
        .map(|specs| {
            zz_daemon_client::terminal_feature_mask(
                specs
                    .iter()
                    .filter(|spec| spec.len() <= 200)
                    .take(16)
                    .map(String::as_str),
            )
        })
        .collect::<Vec<_>>();
    let output = cases
        .into_iter()
        .map(|case| {
            let cleaned = zz_cli::diagnostics::application_args_from(
                case.into_iter().map(std::ffi::OsString::from_vec),
            );
            let parsed = zz_cli::application_arguments(
                cleaned.iter().cloned(),
                PathBuf::from("/oracle/default.sock"),
            );
            let path = parsed
                .as_ref()
                .ok()
                .map(|value| value.socket_path.as_os_str().as_encoded_bytes());
            json!({
                "cleaned": cleaned.iter().map(zz_protocol::RawText::as_bytes).collect::<Vec<_>>(),
                "debug": format!("{parsed:?}"),
                "path": path,
            })
        })
        .collect::<Vec<Value>>();
    println!(
        "{}",
        serde_json::to_string(&json!({
            "cases": output,
            "version": format!("zz {}", env!("CARGO_PKG_VERSION")),
            "directory": zz_protocol::app_identity::DIRECTORY,
            "features": zz_daemon_client::TERMINAL_FEATURES,
            "masks": masks,
            "default_socket": zz_daemon_client::default_socket_path().as_os_str().as_encoded_bytes(),
        }))?
    );
    Ok(())
}
