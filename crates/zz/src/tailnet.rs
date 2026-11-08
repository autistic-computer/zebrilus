use std::time::Duration;

use ala::{App, AsyncApp};
use zz_daemon_client::{
    CommandClient, Endpoint, HostEntry, TailnetEndpoint,
    tailnet::{self, MODE_KEY, Mode, OwnState, Settings},
    tailnet_offer::host_link,
};
use zz_protocol::CommandInvocation;

use crate::config::{self, DiscoveredHosts};

const INTERVAL: Duration = Duration::from_secs(30);
const SIGN_IN_INTERVAL: Duration = Duration::from_secs(2);

pub(crate) fn start(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            let waiting = scan_into(cx).await;
            cx.background_executor()
                .timer(if waiting { SIGN_IN_INTERVAL } else { INTERVAL })
                .await;
        }
    })
    .detach();
}

pub(crate) fn rescan(cx: &mut App) {
    let mut current = config::discovered(cx);
    current.searching = true;
    config::set_discovered(current, cx);
    cx.spawn(async move |cx| {
        scan_into(cx).await;
    })
    .detach();
}

async fn scan_into(cx: &mut AsyncApp) -> bool {
    let found = cx.background_executor().spawn(async move { scan() }).await;
    let waiting = found.own && found.link.is_none();
    cx.update(|cx| config::set_discovered(found, cx));
    waiting
}

pub(crate) fn use_own(cx: &mut App) {
    if let Err(error) = config::set_config_key_name(MODE_KEY, Mode::Own.as_str()) {
        log::warn!("could not set tailnet = own: {error}");
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .spawn(async move {
                let _ = own_command(&["login"]);
            })
            .await;
        cx.background_executor().timer(OWN_START).await;
        scan_into(cx).await;
    })
    .detach();
    rescan(cx);
}

pub(crate) fn sign_out(cx: &mut App) {
    if let Err(error) = config::set_config_key_name(MODE_KEY, Mode::Auto.as_str()) {
        log::warn!("could not set tailnet = auto: {error}");
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .spawn(async move {
                if let Err(error) = own_command(&["logout"]) {
                    log::warn!("could not sign zz's own node out: {error}");
                }
            })
            .await;
        scan_into(cx).await;
    })
    .detach();
}

const OWN_START: Duration = Duration::from_secs(3);

fn own_command(arguments: &[&str]) -> Result<String, String> {
    let socket = zz_daemon_client::default_socket_path();
    CommandClient::connect(&socket)
        .map_err(|error| error.to_string())?
        .execute(CommandInvocation::new("tailnet", arguments.iter().copied()))
        .map_err(|error| error.to_string())
}

fn own_scan(settings: Settings) -> DiscoveredHosts {
    let state = own_command(&["status"])
        .and_then(|line| OwnState::parse(&line).ok_or(line))
        .unwrap_or_else(OwnState::Failed);
    let (link, login) = match &state {
        OwnState::Running { name, .. } => (
            Some(host_link(&TailnetEndpoint {
                peer: name.clone(),
                port: Some(tailnet::PORT),
            })),
            None,
        ),
        OwnState::NeedsLogin(url) => (None, Some(url.clone())),
        _ => (None, None),
    };
    DiscoveredHosts {
        entries: Vec::new(),
        summary: match &state {
            OwnState::Running { name, .. } => format!(
                "Signed in as {name} with zz's own connection; add your other machines by name"
            ),
            OwnState::NeedsLogin(_) => "zz's own connection needs signing in".to_owned(),
            other => other.describe(),
        },
        searching: false,
        enabled: true,
        hidden: settings.hide,
        link,
        own: true,
        login,
    }
}

fn scan() -> DiscoveredHosts {
    let settings = config::discover_config_path(&config::config_candidates())
        .and_then(|path| config::read_config_source(&path).ok())
        .map(|source| Settings::from_config(&source))
        .unwrap_or_default();
    if settings.mode == Mode::Own {
        return own_scan(settings);
    }
    let enabled = settings.mode != Mode::Off;
    let mut link = None;
    let (summary, entries) = if enabled {
        match tailnet::status() {
            Err(error) => (error.to_string(), Vec::new()),
            Ok(status) => match status.unusable() {
                Some(reason) => (reason, Vec::new()),
                None => (
                    {
                        if !status.dns_name.is_empty() {
                            link = Some(zz_daemon_client::tailnet_offer::host_link(
                                &zz_daemon_client::TailnetEndpoint {
                                    peer: status.dns_name.clone(),
                                    port: Some(tailnet::PORT),
                                },
                            ));
                        }
                        format!("Signed in to {} as {}", status.tailnet, status.host)
                    },
                    tailnet::discover(&status)
                        .into_iter()
                        .filter(|machine| !settings.hide.contains(&machine.label))
                        .map(|machine| HostEntry {
                            name: machine.label,
                            endpoint: Endpoint::Tailnet(machine.endpoint),
                        })
                        .collect(),
                ),
            },
        }
    } else {
        ("Off".to_owned(), Vec::new())
    };
    DiscoveredHosts {
        entries,
        summary,
        searching: false,
        enabled,
        hidden: settings.hide,
        link,
        own: false,
        login: None,
    }
}
