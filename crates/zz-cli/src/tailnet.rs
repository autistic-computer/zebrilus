use std::{
    net::{SocketAddr, TcpStream},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use zz_daemon_client::{
    CommandClient,
    tailnet::{self, Allow, MODE_KEY, Mode, OwnState, PORT, Settings, Status},
};
use zz_protocol::CommandInvocation;

const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const LOGIN_POLL: Duration = Duration::from_millis(500);
const LOGIN_WAIT: Duration = Duration::from_secs(600);
const START_WAIT: Duration = Duration::from_secs(30);
const USAGE: &str = "usage: zz tailnet [status | hosts | login [--auth-key-file FILE] | logout | allow [NAME] | disallow NAME]";

pub(crate) fn run(
    arguments: impl IntoIterator<Item = String>,
    socket: &Path,
) -> Result<String, String> {
    let arguments: Vec<String> = arguments.into_iter().collect();
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["status"] => Ok(report(socket)),
        ["hosts"] => tailnet::status()
            .map(|status| zz_daemon_client::tailnet_offer::offers_json(&tailnet::offers(&status)))
            .map_err(|error| error.to_string()),
        ["login"] => login(socket, None),
        ["login", "--auth-key-file", file] => {
            let key = std::fs::read_to_string(file)
                .map_err(|error| format!("could not read {file}: {error}"))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(format!("{file} is empty"));
            }
            login(socket, Some(key.to_owned()))
        }
        ["logout"] => logout(socket),
        ["allow"] => daemon(socket, &["allow"]),
        ["allow", name] => daemon(socket, &["allow", name]),
        ["disallow", name] => daemon(socket, &["disallow", name]),
        _ => Err(USAGE.to_owned()),
    }
}

fn settings() -> Settings {
    zz_config::discover_config_path(&zz_config::config_candidates())
        .and_then(|path| zz_config::read_config_source(&path).ok())
        .map(|source| Settings::from_config(&source))
        .unwrap_or_default()
}

fn daemon(socket: &Path, arguments: &[&str]) -> Result<String, String> {
    let mut client = CommandClient::connect(socket).map_err(|error| {
        format!(
            "could not reach this machine's zz daemon at {}: {error}",
            socket.display()
        )
    })?;
    client
        .execute(CommandInvocation::new("tailnet", arguments.iter().copied()))
        .map(|output| output.trim_end().to_owned())
        .map_err(|error| error.to_string())
}

fn own_state(socket: &Path) -> Result<OwnState, String> {
    let line = daemon(socket, &["status"])?;
    OwnState::parse(&line).ok_or_else(|| format!("the daemon answered `{line}`"))
}

fn login(socket: &Path, key: Option<String>) -> Result<String, String> {
    if settings().mode != Mode::Own {
        zz_config::set_config_key_name(MODE_KEY, Mode::Own.as_str())
            .map_err(|error| format!("could not set tailnet = own: {error}"))?;
        println!("set tailnet = own in zz/config");
    }
    let mut arguments = vec!["login".to_owned()];
    arguments.extend(key);
    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
    daemon(socket, &arguments)?;
    let started = Instant::now();
    let mut shown = None;
    loop {
        match own_state(socket)? {
            OwnState::Running { name, .. } => return Ok(format!("signed in as {name}")),
            OwnState::Failed(reason) => return Err(format!("zz's own node stopped: {reason}")),
            OwnState::NeedsLogin(url) => {
                if shown.as_ref() != Some(&url) {
                    println!("To sign in, open {url}");
                    shown = Some(url);
                }
            }
            OwnState::Off(_) | OwnState::Starting => {
                if shown.is_none() && started.elapsed() > START_WAIT {
                    return Err(
                        "zz's own node did not reach the Tailscale control server; check the network and run `zz tailnet status`"
                            .to_owned(),
                    );
                }
            }
        }
        if started.elapsed() > LOGIN_WAIT {
            return Err("gave up waiting for the sign-in; run `zz tailnet login` again".to_owned());
        }
        thread::sleep(LOGIN_POLL);
    }
}

fn logout(socket: &Path) -> Result<String, String> {
    zz_config::set_config_key_name(MODE_KEY, Mode::Auto.as_str())
        .map_err(|error| format!("could not set tailnet = auto: {error}"))?;
    daemon(socket, &["logout"])?;
    Ok(
        "signed zz's own node out, deleted its keys and set tailnet = auto; remove the machine at https://login.tailscale.com/admin/machines"
            .to_owned(),
    )
}

fn report(socket: &Path) -> String {
    let settings = settings();
    if settings.mode == Mode::Own {
        return own_report(&settings, socket);
    }
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
                    .map(|machine| {
                        if machine.os.is_empty() {
                            machine.endpoint.to_string()
                        } else {
                            format!("{} ({})", machine.endpoint, machine.os)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        });
    }
    lines.join("\n")
}

fn own_report(settings: &Settings, socket: &Path) -> String {
    let mut lines = vec![format!(
        "setting    tailnet = own, tailnet-allow = {}",
        settings.allow.as_str()
    )];
    match own_state(socket) {
        Ok(state) => {
            lines.push(format!("node       {}", state.describe()));
            if let OwnState::Running { key, addresses, .. } = &state {
                let addresses: Vec<String> = addresses.iter().map(ToString::to_string).collect();
                lines.push(format!(
                    "listening  yes, on port {PORT} at {}",
                    addresses.join(", ")
                ));
                lines.push(format!(
                    "lock       if your tailnet uses Tailnet Lock, sign this node on a signing machine: tailscale lock sign {key}"
                ));
            }
        }
        Err(error) => lines.push(format!("node       {error}")),
    }
    lines.push(match settings.allow {
        Allow::Members => {
            "admits     every device the tailnet's access rules let reach this one".to_owned()
        }
        Allow::Owner => match daemon(socket, &["allow"]) {
            Ok(allowed) => format!("admits     {}", allowed.replace('\n', ", ")),
            Err(error) => format!("admits     {error}"),
        },
    });
    lines.push(
        "machines   zz's own node cannot list them; add hosts by name, such as tailnet://NAME"
            .to_owned(),
    );
    lines.join("\n")
}

fn listening(mode: Mode, status: &Status) -> String {
    if matches!(mode, Mode::Off | Mode::Own) {
        return format!("no: tailnet = {}", mode.as_str());
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
