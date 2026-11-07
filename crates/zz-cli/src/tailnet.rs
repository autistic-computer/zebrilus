use std::{
    net::{SocketAddr, TcpStream},
    time::Duration,
};

use zz_daemon_client::tailnet::{self, Allow, Mode, PORT, Settings, Status};

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

pub(crate) fn run(arguments: impl IntoIterator<Item = String>) -> Result<String, String> {
    let arguments: Vec<String> = arguments.into_iter().collect();
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["status"] => Ok(report()),
        ["hosts"] => tailnet::status()
            .map(|status| zz_daemon_client::tailnet_offer::offers_json(&tailnet::offers(&status)))
            .map_err(|error| error.to_string()),
        _ => Err("usage: zz tailnet [status | hosts]".to_owned()),
    }
}

fn report() -> String {
    let settings = zz_config::discover_config_path(&zz_config::config_candidates())
        .and_then(|path| zz_config::read_config_source(&path).ok())
        .map(|source| Settings::from_config(&source))
        .unwrap_or_default();
    let mut lines = vec![format!(
        "setting    tailnet = {}, tailnet-allow = {}",
        settings.mode.as_str(),
        settings.allow.as_str()
    )];
    let status = match tailnet::status() {
        Ok(status) => status,
        Err(error) => {
            lines.push(format!("tailscale  {error}"));
            return lines.join("\n");
        }
    };
    let tags = if status.tags.is_empty() {
        String::new()
    } else {
        format!(" ({})", status.tags.join(", "))
    };
    lines.push(format!(
        "tailscale  {} on {} as {}{tags}",
        status.backend, status.tailnet, status.host
    ));
    lines.push(format!("listening  {}", listening(settings.mode, &status)));
    lines.push(format!("admits     {}", admitted(settings.allow, &status)));
    if status.unusable().is_none() {
        let found = tailnet::discover(&status);
        lines.push(if found.is_empty() {
            "machines   none of yours answer on the zz port".to_owned()
        } else {
            format!(
                "machines   {}",
                found
                    .iter()
                    .map(|machine| format!("{} ({})", machine.endpoint, machine.os))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });
    }
    lines.join("\n")
}

fn listening(mode: Mode, status: &Status) -> String {
    if mode == Mode::Off {
        return "no: tailnet = off".to_owned();
    }
    if let Some(reason) = status.unusable() {
        return format!("no: {reason}");
    }
    let addresses: Vec<String> = status.addresses.iter().map(ToString::to_string).collect();
    let probe = SocketAddr::new(status.addresses[0], PORT);
    if TcpStream::connect_timeout(&probe, PROBE_TIMEOUT).is_ok() {
        format!("yes, on port {PORT} at {}", addresses.join(", "))
    } else {
        format!(
            "no: nothing answers on port {PORT} at {}; is this machine's zz daemon running?",
            addresses[0]
        )
    }
}

fn admitted(allow: Allow, status: &Status) -> String {
    match allow {
        Allow::Members => "every device the tailnet's access rules let reach this one".to_owned(),
        Allow::Owner if status.is_personal() => format!(
            "tagged devices and {}'s devices on this tailnet",
            status.tailnet
        ),
        Allow::Owner if status.tags.is_empty() => format!("{}'s untagged devices", status.login),
        Allow::Owner => {
            "nobody: this machine is tagged, so it has no owner to match; set tailnet-allow = members"
                .to_owned()
        }
    }
}
