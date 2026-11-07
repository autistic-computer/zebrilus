use std::time::Duration;

use ala::{App, AsyncApp};
use zz_daemon_client::{
    Endpoint, HostEntry,
    tailnet::{self, Mode, Settings},
};

use crate::config::{self, DiscoveredHosts};

const INTERVAL: Duration = Duration::from_secs(30);

pub(crate) fn start(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            scan_into(cx).await;
            cx.background_executor().timer(INTERVAL).await;
        }
    })
    .detach();
}

pub(crate) fn rescan(cx: &mut App) {
    let mut current = config::discovered(cx);
    current.searching = true;
    config::set_discovered(current, cx);
    cx.spawn(async move |cx| scan_into(cx).await).detach();
}

async fn scan_into(cx: &mut AsyncApp) {
    let found = cx.background_executor().spawn(async move { scan() }).await;
    cx.update(|cx| config::set_discovered(found, cx));
}

fn scan() -> DiscoveredHosts {
    let settings = config::discover_config_path(&config::config_candidates())
        .and_then(|path| config::read_config_source(&path).ok())
        .map(|source| Settings::from_config(&source))
        .unwrap_or_default();
    let enabled = settings.mode != Mode::Off;
    let (summary, entries) = if enabled {
        match tailnet::status() {
            Err(error) => (error.to_string(), Vec::new()),
            Ok(status) => match status.unusable() {
                Some(reason) => (reason, Vec::new()),
                None => (
                    format!("Signed in to {} as {}", status.tailnet, status.host),
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
    }
}
