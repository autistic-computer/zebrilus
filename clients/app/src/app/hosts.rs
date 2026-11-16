#[cfg(any(target_os = "ios", target_os = "android"))]
use ala::{AnyElement, Entity, IntoElement, Pixels, SharedString, Subscription, div, px};
use ala::{Context, Window, prelude::*};
use penna::Colorize as _;
use penna::{
    ActiveTheme as _, Ground, Icon, IconName, Label, TextStyle, Tone, components::button::Button,
    h_flex,
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use penna::{
    Disableable as _, Sizable as _, Size, Space,
    blocks::sheet::{
        bottom_sheet, close_sheet, floating_sheet, modal_groups, sheet_close, sheet_ground,
        sheet_groups, sheet_inset, sheet_note,
    },
    blocks::touch::{TouchGroup, TouchHeader, TouchRow, concentric},
    components::button::ButtonVariants as _,
    components::input::{Input, InputEvent, InputState},
    v_flex,
};
use zz_ui::settings::{
    SettingEntry, SettingsScrollColumn, SettingsStack, settings_card_ground, settings_page_heading,
    settings_scroll_column,
};

use super::{AppShell, settings::with_control};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::connection::{Connection, GATEWAY_LOCAL};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct SavedHosts {
    endpoints: Vec<String>,
    connected: Vec<String>,
}

impl SavedHosts {
    pub(super) fn endpoints(&self) -> &[String] {
        &self.endpoints
    }

    pub(super) fn connected(&self) -> &[String] {
        &self.connected
    }

    pub(super) fn add(&mut self, endpoint: &str) {
        if !self.endpoints.iter().any(|saved| saved == endpoint) {
            self.endpoints.push(endpoint.to_owned());
        }
    }

    pub(super) fn mark_connected(&mut self, endpoint: &str, on_screen: bool) {
        self.add(endpoint);
        let known = self.connected.iter().any(|saved| saved == endpoint);
        if on_screen {
            self.connected.retain(|saved| saved != endpoint);
            self.connected.insert(0, endpoint.to_owned());
        } else if !known {
            self.connected.push(endpoint.to_owned());
        }
    }

    pub(super) fn mark_disconnected(&mut self, endpoint: &str) {
        self.connected.retain(|saved| saved != endpoint);
    }
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
impl SavedHosts {
    pub(super) fn most_recent(&self) -> Option<&str> {
        self.connected
            .first()
            .or_else(|| self.endpoints.first())
            .map(String::as_str)
    }

    pub(super) fn remove(&mut self, endpoint: &str) {
        self.endpoints.retain(|saved| saved != endpoint);
        self.mark_disconnected(endpoint);
    }

    fn load_from(directory: &std::path::Path) -> Self {
        if let Ok(value) = std::fs::read_to_string(directory.join("hosts.json")) {
            return serde_json::from_str(&value).unwrap_or_default();
        }
        let mut hosts = Self::default();
        if let Ok(endpoint) = std::fs::read_to_string(directory.join("endpoint"))
            && !endpoint.trim().is_empty()
        {
            hosts.add(endpoint.trim());
        }
        hosts
    }

    fn save_to(&self, directory: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let value = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let temporary = directory.join("hosts.json.tmp");
        std::fs::write(&temporary, value)?;
        std::fs::rename(temporary, directory.join("hosts.json"))
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl SavedHosts {
    fn directory() -> std::path::PathBuf {
        crate::preferences::support_directory()
    }

    pub(super) fn load() -> Self {
        Self::load_from(&Self::directory())
    }

    pub(super) fn save(&self) {
        if let Err(error) = self.save_to(&Self::directory()) {
            log::warn!("Could not save hosts: {error}");
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl SavedHosts {
    #[cfg(target_family = "wasm")]
    const STORAGE_KEY: &str = if zz_protocol::app_identity::DEVELOPMENT {
        "zz-dev-web-hosts"
    } else {
        "zz-web-hosts"
    };

    fn load() -> Self {
        #[cfg(target_family = "wasm")]
        if let Some(hosts) = web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(Self::STORAGE_KEY).ok().flatten())
            .and_then(|value| serde_json::from_str(&value).ok())
        {
            return hosts;
        }
        Self::default()
    }

    pub(super) fn save(&self) {
        #[cfg(target_family = "wasm")]
        if let Some(storage) =
            web_sys::window().and_then(|window| window.local_storage().ok().flatten())
            && let Ok(value) = serde_json::to_string(self)
        {
            let _ = storage.set_item(Self::STORAGE_KEY, &value);
        }
    }

    fn keep_only(&mut self, names: Vec<String>) {
        self.connected.retain(|name| names.contains(name));
        self.endpoints = names;
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub(super) struct GatewayHost {
    name: String,
    title: String,
    local: bool,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(super) struct Hosts {
    pub(super) saved: SavedHosts,
    gateway: Vec<GatewayHost>,
    #[cfg(target_family = "wasm")]
    _reopen: Option<ala::Subscription>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl Hosts {
    pub(super) fn new(_: &mut Window, _: &mut Context<AppShell>) -> Self {
        Self {
            saved: SavedHosts::load(),
            gateway: Vec::new(),
            #[cfg(target_family = "wasm")]
            _reopen: None,
        }
    }

    pub(super) fn title(&self, key: &str) -> String {
        self.gateway
            .iter()
            .find(|host| host.name == key)
            .map_or_else(
                || {
                    if key == GATEWAY_LOCAL {
                        "This computer".to_owned()
                    } else {
                        key.to_owned()
                    }
                },
                |host| host.title.clone(),
            )
    }

    pub(super) fn location(&self, _: &str) -> Option<String> {
        None
    }

    pub(super) fn waiting(&self, key: &str) -> String {
        format!("Waiting for {}", self.title(key))
    }

    fn description(&self, key: &str) -> Option<String> {
        self.gateway
            .iter()
            .any(|host| host.name == key && host.local)
            .then(gateway_description)
    }

    fn icon(&self, key: &str) -> IconName {
        if key == GATEWAY_LOCAL {
            IconName::HardDrive
        } else {
            IconName::Globe
        }
    }

    fn fleet(&self) -> bool {
        self.gateway.len() > 1
    }

    #[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
    fn adopt(&mut self, gateway: Vec<GatewayHost>) {
        self.saved
            .keep_only(gateway.iter().map(|host| host.name.clone()).collect());
        self.gateway = gateway;
    }
}

#[cfg(target_family = "wasm")]
impl AppShell {
    pub(super) fn load_gateway_hosts(window: &mut Window, cx: &mut Context<Self>) {
        let last = SavedHosts::load()
            .connected
            .into_iter()
            .next()
            .filter(|host| host != GATEWAY_LOCAL);
        cx.spawn_in(window, async move |this, cx| {
            let hosts = gateway_hosts().await.unwrap_or_default();
            this.update_in(cx, |this, window, cx| {
                this.hosts.adopt(hosts);
                this.reconnect_saved_hosts(window, cx);
                if let Some(last) = last {
                    this.reopen_once_connected(last, window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn reopen_once_connected(&mut self, host: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(connection) = self.host_connection(&host, cx) else {
            return;
        };
        self.hosts._reopen =
            Some(
                cx.observe_in(&connection, window, move |this, connection, window, cx| {
                    let (connected, busy) = {
                        let connection = connection.read(cx);
                        (connection.connected, connection.busy())
                    };
                    if connected && this.is_attached_host(GATEWAY_LOCAL, cx) {
                        this.switch_host(&host, None, None, window, cx);
                    }
                    if connected || !busy {
                        this.hosts._reopen = None;
                    }
                }),
            );
    }
}

#[cfg(target_family = "wasm")]
async fn gateway_hosts() -> Option<Vec<GatewayHost>> {
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window()?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str("/hosts"))
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    if !response.ok() {
        return None;
    }
    let body = JsFuture::from(response.text().ok()?)
        .await
        .ok()?
        .as_string()?;
    serde_json::from_str(&body).ok()
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl Hosts {
    pub(super) fn title(&self, key: &str) -> String {
        host_title(key)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "the browser has no location for a gateway host"
    )]
    pub(super) fn location(&self, key: &str) -> Option<String> {
        Some(host_location(key))
    }

    pub(super) fn waiting(&self, key: &str) -> String {
        waiting_for(key)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "the browser describes only its own gateway"
    )]
    fn description(&self, key: &str) -> Option<String> {
        Some(key.to_owned())
    }

    fn icon(&self, key: &str) -> IconName {
        if key.starts_with("ssh://") {
            IconName::Globe
        } else {
            IconName::HardDrive
        }
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn parsed_endpoint(input: &str) -> Result<String, String> {
    destination_endpoint(input).and_then(|endpoint| {
        zz_daemon_client::Endpoint::parse(&endpoint)
            .map(|endpoint| endpoint.to_string())
            .map_err(|error| error.to_string())
    })
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(super) fn destination_endpoint(input: &str) -> Result<String, String> {
    let destination = input.trim();
    if destination.is_empty() {
        return Err("Enter a host, like user@my-mac.local.".into());
    }
    if destination.chars().any(char::is_whitespace) {
        return Err("A host must not contain spaces.".into());
    }
    Ok(
        if destination.contains("://") || destination.starts_with('/') {
            destination.to_owned()
        } else {
            format!("ssh://{destination}")
        },
    )
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(crate) fn host_title(endpoint: &str) -> String {
    if let Some(rest) = endpoint.strip_prefix("tailnet://") {
        let machine = rest.split(':').next().unwrap_or(rest);
        return if machine.parse::<std::net::Ipv4Addr>().is_ok() {
            machine.to_owned()
        } else {
            machine.split('.').next().unwrap_or(machine).to_owned()
        };
    }
    let Some(rest) = endpoint.strip_prefix("ssh://") else {
        return std::path::Path::new(endpoint)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| *stem != "default")
            .map_or_else(|| "Local socket".into(), str::to_owned);
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or_default(),
        None => host.split(':').next().unwrap_or_default(),
    }
    .to_owned()
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(super) fn host_location(endpoint: &str) -> String {
    if let Some(rest) = endpoint
        .strip_prefix("ssh://")
        .or_else(|| endpoint.strip_prefix("tailnet://"))
    {
        return rest.trim_end_matches('/').to_owned();
    }
    let parts = endpoint
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [.., parent, file] if parts.len() > 3 => format!("…/{parent}/{file}"),
        _ => endpoint.to_owned(),
    }
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
pub(super) fn waiting_for(endpoint: &str) -> String {
    if endpoint.starts_with("tailnet://") {
        format!("Waiting for {} on your tailnet", host_title(endpoint))
    } else if endpoint.starts_with("ssh://") {
        format!("Waiting for ssh on {}", host_location(endpoint))
    } else {
        format!("Waiting for {}", host_location(endpoint))
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) const ADD_HOST_SHEET: &str = "add-host-sheet";
#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) const HOST_SHEET: &str = "host-sheet";

#[cfg(any(target_os = "ios", target_os = "android"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum HostSheet {
    Add,
    Host(String),
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) struct Hosts {
    pub(super) saved: SavedHosts,
    pub(super) destination: Entity<InputState>,
    pub(super) error: Option<SharedString>,
    pub(super) sheet: Option<HostSheet>,
    pub(super) found: Vec<crate::connection::FoundHost>,
    _destination_events: Subscription,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl Hosts {
    pub(super) fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let destination = cx.new(|cx| InputState::new(window, cx).placeholder("user@host"));
        let destination_events = cx.subscribe_in(
            &destination,
            window,
            |this: &mut AppShell, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } if this.add_host(window, cx) => {
                    close_sheet(ADD_HOST_SHEET, cx);
                }
                InputEvent::Change => {
                    this.hosts.error = None;
                    cx.notify();
                }
                _ => {}
            },
        );
        Self {
            saved: SavedHosts::load(),
            destination,
            error: None,
            sheet: None,
            found: Vec::new(),
            _destination_events: destination_events,
        }
    }
}

impl AppShell {
    pub(super) fn hosts_page(
        &self,
        id: &'static str,
        title: Option<&'static str>,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> SettingsScrollColumn {
        let entries = self.host_entries(narrow, cx);
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let add = Some(if narrow {
            self.add_host_row(cx)
        } else {
            self.add_host_stack(narrow, cx)
        });
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let add: Option<SettingsStack> = None;
        let hosts = if narrow {
            SettingsStack::new()
        } else {
            SettingsStack::titled("Hosts")
        };
        settings_scroll_column(id)
            .children(title.map(|title| settings_page_heading(title, cx)))
            .when(!entries.is_empty(), |page| {
                page.child(hosts.children(entries))
            })
            .children(add)
    }

    pub(super) fn remember_host(&mut self, cx: &mut Context<Self>) {
        if let Some(endpoint) = self.connection.read(cx).endpoint() {
            let endpoint = endpoint.to_owned();
            self.hosts.saved.mark_connected(&endpoint, true);
            self.hosts.saved.save();
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn connect_recent_host(&mut self, cx: &mut Context<Self>) {
        let connection = self.connection.read(cx);
        if !connection.connected
            && !connection.busy()
            && let Some(endpoint) = self.hosts.saved.most_recent()
        {
            let endpoint = endpoint.to_owned();
            self.connection
                .update(cx, |connection, cx| connection.connect_to(endpoint, cx));
        }
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let value = self.hosts.destination.read(cx).value().to_string();
        match parsed_endpoint(&value) {
            Ok(endpoint) => {
                self.hosts.error = None;
                self.hosts
                    .destination
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.hosts.saved.add(&endpoint);
                self.connect_host(endpoint, window, cx);
                super::settings::finish_field(window, cx);
                cx.notify();
                true
            }
            Err(error) => {
                self.hosts.error = Some(error.into());
                cx.notify();
                false
            }
        }
    }

    fn host_entries(&self, narrow: bool, cx: &mut Context<Self>) -> Vec<SettingEntry> {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if !self.hosts.fleet() {
            return vec![self.gateway_entry(narrow, cx)];
        }
        let muted = cx.theme().foreground.muted();
        let nothing_attached = self.connection.read(cx).endpoint().is_none();
        self.hosts
            .saved
            .endpoints()
            .iter()
            .enumerate()
            .map(|(index, endpoint)| {
                let state = self.host_connection(endpoint, cx).map(|connection| {
                    let connection = connection.read(cx);
                    (
                        connection.connected,
                        connection.busy(),
                        !connection.background(),
                        if connection.connected {
                            "Connected".to_owned()
                        } else {
                            connection.status.clone()
                        },
                    )
                });
                let (connected, busy, on_screen) = state
                    .as_ref()
                    .map_or((false, false, false), |(connected, busy, on_screen, _)| {
                        (*connected, *busy, *on_screen)
                    });
                let color = match &state {
                    None => muted,
                    Some(_) if connected => cx.theme().success,
                    Some(_) if busy => cx.theme().warning,
                    Some(_) => cx.theme().danger,
                };
                let icon = self.hosts.icon(endpoint);
                #[cfg(any(target_os = "ios", target_os = "android"))]
                if narrow {
                    let status = match &state {
                        Some(_) if connected => "Connected",
                        Some(_) if busy => "Connecting…",
                        _ => "Not connected",
                    };
                    let view = cx.entity().downgrade();
                    let picked = endpoint.clone();
                    return SettingEntry::new(host_title(endpoint))
                        .description(host_location(endpoint))
                        .title_icon(Icon::new(icon).text_color(color))
                        .control(row_meta(status, cx))
                        .on_click(("hosts-row", index), move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.hosts.sheet = Some(HostSheet::Host(picked.clone()));
                                cx.notify();
                            });
                        });
                }
                let entry = SettingEntry::new(self.hosts.title(endpoint))
                    .when_some(self.hosts.description(endpoint), SettingEntry::description)
                    .title_icon(Icon::new(icon).text_color(color))
                    .when_some(state.as_ref(), |entry, (.., status)| {
                        entry.child(status_line(
                            status.clone(),
                            if connected || busy {
                                Tone::Neutral
                            } else {
                                Tone::Warning
                            },
                            cx,
                        ))
                    });
                let target = endpoint.clone();
                let action = if connected && !on_screen {
                    Button::new(("hosts-open", index))
                        .label("Open")
                        .on(settings_card_ground(cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.switch_host(&target, None, None, window, cx);
                            this.settings = None;
                            cx.notify();
                        }))
                } else if connected || busy {
                    Button::new(("hosts-disconnect", index))
                        .label(if connected { "Disconnect" } else { "Cancel" })
                        .on(settings_card_ground(cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.disconnect_host(&target, window, cx);
                        }))
                } else {
                    Button::new(("hosts-connect", index))
                        .when(
                            index == 0 && nothing_attached,
                            penna::components::button::ButtonVariants::primary,
                        )
                        .label("Connect")
                        .on(settings_card_ground(cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.connect_host(target.clone(), window, cx);
                        }))
                };
                #[cfg(any(target_os = "ios", target_os = "android"))]
                let install = self.host_connection(endpoint, cx).and_then(|connection| {
                    let offer = connection.read(cx).install_offer()?;
                    Some(
                        Button::new(("hosts-install", index))
                            .label(offer.label())
                            .on(settings_card_ground(cx))
                            .on_click(move |_, window, cx| {
                                install_zz(connection.clone(), window, cx);
                            }),
                    )
                });
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                let install: Option<Button> = None;
                #[cfg(any(target_os = "ios", target_os = "android"))]
                let remove = {
                    let removed = endpoint.clone();
                    Some(
                        Button::new(("hosts-remove", index))
                            .ghost()
                            .icon(IconName::Xmark)
                            .tooltip("Remove host")
                            .on(settings_card_ground(cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.disconnect_host(&removed, window, cx);
                                this.hosts.saved.remove(&removed);
                                this.hosts.saved.save();
                                cx.notify();
                            })),
                    )
                };
                #[cfg(not(any(target_os = "ios", target_os = "android")))]
                let remove: Option<Button> = None;
                with_control(
                    entry,
                    h_flex()
                        .gap_2()
                        .flex_none()
                        .children(install)
                        .child(action)
                        .children(remove),
                    narrow,
                )
            })
            .collect()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn gateway_entry(&self, narrow: bool, cx: &mut Context<Self>) -> SettingEntry {
        let connection = self.connection.read(cx);
        let connected = connection.connected;
        let entry = SettingEntry::new("This computer")
            .description(gateway_description())
            .title_icon(Icon::new(IconName::HardDrive).text_color(if connected {
                cx.theme().success
            } else {
                cx.theme().warning
            }))
            .child(status_line(
                if connected {
                    "Connected".to_owned()
                } else {
                    connection.status.clone()
                },
                if connected {
                    Tone::Neutral
                } else {
                    Tone::Warning
                },
                cx,
            ));
        with_control(
            entry,
            h_flex().child(
                Button::new("hosts-reconnect")
                    .label(if connected { "Reconnect" } else { "Retry" })
                    .on(settings_card_ground(cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.connection.update(cx, Connection::reconnect);
                    })),
            ),
            narrow,
        )
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn add_host_row(&self, cx: &mut Context<Self>) -> SettingsStack {
        let view = cx.entity().downgrade();
        SettingsStack::new().child(
            SettingEntry::new("Add host")
                .title_icon(Icon::new(IconName::Plus).text_color(cx.theme().accent))
                .control(row_meta("", cx))
                .on_click("hosts-add-row", move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.hosts.error = None;
                        this.hosts.sheet = Some(HostSheet::Add);
                        let input = this.hosts.destination.clone();
                        cx.on_next_frame(window, move |_, window, cx| {
                            input.update(cx, |input, cx| input.focus(window, cx));
                            window.request_virtual_keyboard();
                        });
                        cx.notify();
                    });
                }),
        )
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn host_sheets(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let sheet = match self.hosts.sheet.as_ref()? {
            HostSheet::Add => self.add_host_sheet(
                sheet_inset(window),
                |_, cx| close_sheet(ADD_HOST_SHEET, cx),
                |this, cx| {
                    this.hosts.sheet = None;
                    cx.notify();
                },
                cx,
            ),
            HostSheet::Host(endpoint) => self.host_sheet(endpoint, cx),
        };
        Some(floating_sheet(sheet, window))
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(super) fn add_host_sheet(
        &self,
        bottom_inset: Pixels,
        on_added: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        on_dismiss: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ground = sheet_ground(cx);
        let close =
            sheet_close("add-host-close", cx).on_click(|_, _, cx| close_sheet(ADD_HOST_SHEET, cx));
        let dismiss = cx.weak_entity();
        let typed = self.hosts.destination.read(cx).value().to_string();
        let ready = parsed_endpoint(&typed).is_ok();
        let danger = cx
            .theme()
            .paint_on(ground, Tone::Danger, penna::Emphasis::Ghost)
            .text;
        let found = self.found_machines(cx);
        let machines: Vec<_> = found.iter().map(|host| host.machine.clone()).collect();
        let endpoints: Vec<String> = found.into_iter().map(|host| host.endpoint).collect();
        let view = cx.entity().downgrade();
        let tailnet = zz_ui::compact::found_machines(&machines, move |index, window, cx| {
            let Some(endpoint) = endpoints.get(index).cloned() else {
                return;
            };
            let _ = view.update(cx, |this, cx| {
                this.hosts.saved.add(&endpoint);
                this.connect_host(endpoint, window, cx);
            });
            close_sheet(ADD_HOST_SHEET, cx);
        });
        let corner = concentric(cx.theme().phone_radius(), px(Space::S8.design_px()))
            .min(px(Size::Touch.control_px() / 2.0));
        let address = v_flex().w_full().child(TouchHeader::new("Address")).child(
            Input::new(&self.hosts.destination)
                .id("add-host-field")
                .with_size(Size::Touch)
                .rounded(corner)
                .on(ground),
        );
        bottom_sheet(
            ADD_HOST_SHEET,
            "Add machine",
            [close.into_any_element()],
            sheet_groups()
                .pb(Space::S8)
                .children(tailnet)
                .child(address)
                .children(
                    self.hosts
                        .error
                        .clone()
                        .map(|error| sheet_note(error, cx).mt(-Space::S8.rems()).text_color(danger)),
                )
                .child(
                    Button::new("add-host-connect")
                        .primary()
                        .label("Connect")
                        .with_size(Size::TouchLarge)
                        .w_full()
                        .on(ground)
                        .disabled(!ready)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.add_host(window, cx) {
                                on_added(this, cx);
                            }
                        })),
                )
                .child(
                    TouchGroup::new().ruled().row(
                        TouchRow::new("add-host-copy-key", "Copy SSH key")
                            .icon(IconName::Copy)
                            .on_click(|_, window, cx| copy_ssh_key(window, cx)),
                    ),
                )
                .child(sheet_note(
                    "Add this phone's key to authorized_keys on the host to sign in without a password.",
                    cx,
                )),
            bottom_inset,
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| on_dismiss(this, cx));
            },
        )
        .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn host_sheet(&self, endpoint: &str, cx: &mut Context<Self>) -> AnyElement {
        let (connected, busy, on_screen, status) = self
            .host_connection(endpoint, cx)
            .map(|connection| {
                let connection = connection.read(cx);
                (
                    connection.connected,
                    connection.busy(),
                    !connection.background(),
                    connection.status.clone(),
                )
            })
            .unwrap_or_default();
        let sheet = sheet_ground(cx);
        let (state, tone) = if connected {
            ("Connected".to_owned(), Tone::Neutral)
        } else if busy {
            ("Connecting…".to_owned(), Tone::Neutral)
        } else if status.is_empty() {
            ("Not connected".to_owned(), Tone::Neutral)
        } else {
            (status, Tone::Warning)
        };
        let target = endpoint.to_owned();
        let mut rows = Vec::new();
        if connected && !on_screen {
            let target = target.clone();
            rows.push(
                TouchRow::new("host-sheet-open", "Open")
                    .icon(IconName::ArrowRight)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.hosts.sheet = None;
                        this.switch_host(&target, None, None, window, cx);
                        this.settings = None;
                        cx.notify();
                    })),
            );
        }
        let toggle = target.clone();
        rows.push(if connected || busy {
            TouchRow::new(
                "host-sheet-disconnect",
                if connected { "Disconnect" } else { "Cancel" },
            )
            .icon(IconName::Stop)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.disconnect_host(&toggle, window, cx);
            }))
        } else {
            TouchRow::new("host-sheet-connect", "Connect")
                .icon(IconName::Server)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.connect_host(toggle.clone(), window, cx);
                }))
        });
        let remove = TouchRow::new("host-sheet-remove", "Remove host")
            .icon(IconName::Xmark)
            .danger()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.disconnect_host(&target, window, cx);
                this.hosts.saved.remove(&target);
                this.hosts.saved.save();
                close_sheet(HOST_SHEET, cx);
                cx.notify();
            }));
        let close =
            sheet_close("host-sheet-close", cx).on_click(|_, _, cx| close_sheet(HOST_SHEET, cx));
        let dismiss = cx.weak_entity();
        let line = |label: Label| label.on(sheet).w_full().text_center();
        bottom_sheet(
            HOST_SHEET,
            host_title(endpoint),
            [close.into_any_element()],
            modal_groups()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .px(px(12.0))
                        .pb(px(8.0))
                        .child(line(
                            Label::new(TextStyle::TouchSmall, endpoint.to_owned()).muted(),
                        ))
                        .child(line(toned(
                            Label::new(TextStyle::TouchSmall, state),
                            tone,
                            sheet,
                            cx,
                        ))),
                )
                .child(TouchGroup::new().ruled().rows(rows).row(remove)),
            px(0.0),
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| {
                    this.hosts.sheet = None;
                    cx.notify();
                });
            },
        )
        .modal()
        .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn add_host_stack(&self, narrow: bool, cx: &mut Context<Self>) -> SettingsStack {
        let destination = SettingEntry::new("Destination")
            .description("user@host, or user@host:port")
            .when_some(self.hosts.error.clone(), |entry, error| {
                entry.child(status_line(error, Tone::Warning, cx))
            });
        SettingsStack::titled("Add host")
            .child(with_control(
                destination,
                h_flex()
                    .gap_2()
                    .when(narrow, ala::Styled::w_full)
                    .child(
                        div()
                            .when(narrow, |field| field.flex_1().min_w_0())
                            .when(!narrow, |field| field.w(px(220.0)).flex_none())
                            .child(
                                Input::new(&self.hosts.destination).on(settings_card_ground(cx)),
                            ),
                    )
                    .child(
                        Button::new("hosts-add")
                            .when(self.hosts.saved.endpoints().is_empty(), |button| {
                                button.primary()
                            })
                            .label("Connect")
                            .on(settings_card_ground(cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_host(window, cx);
                            })),
                    ),
                narrow,
            ))
            .child(with_control(
                SettingEntry::new("SSH key")
                    .description("Add it to ~/.ssh/authorized_keys on the host"),
                h_flex().child(
                    Button::new("hosts-copy-ssh-key")
                        .label("Copy")
                        .on(settings_card_ground(cx))
                        .on_click(|_, window, cx| copy_ssh_key(window, cx)),
                ),
                narrow,
            ))
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) fn watch_host_notices(
    connection: &Entity<crate::connection::Connection>,
    window: &mut Window,
    cx: &mut Context<AppShell>,
) -> Subscription {
    use crate::connection::HostNotice;
    use penna::{WindowExt as _, blocks::notification::Notification};
    use zz_protocol::ClientMessageKind;
    cx.subscribe_in(
        connection,
        window,
        |_, connection, notice: &HostNotice, window, cx| {
            let title = connection
                .read(cx)
                .endpoint()
                .map_or_else(String::new, host_title);
            let notification = match notice {
                HostNotice::KeyAdded => Notification::success(format!(
                    "Added zz's key to {title}; it won't ask to sign in again"
                )),
                HostNotice::KeyFailed(reason) => {
                    Notification::warning(format!("Could not add zz's key to {title}: {reason}"))
                }
                HostNotice::Installed(outcome) => match outcome.notice(&title) {
                    (ClientMessageKind::Success, text) => Notification::success(text),
                    (ClientMessageKind::Warning, text) => Notification::warning(text),
                    (ClientMessageKind::Error, text) => Notification::error(text),
                    (ClientMessageKind::Info, text) => Notification::info(text),
                },
                HostNotice::InstallFailed(reason) => Notification::error(reason.clone()),
            };
            window.push_notification(notification, cx);
        },
    )
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) fn install_zz(
    connection: Entity<crate::connection::Connection>,
    window: &mut Window,
    cx: &mut ala::App,
) {
    use penna::{
        Emphasis, WindowExt as _, blocks::dialog::DialogButtonProps,
        components::button::ButtonVariant,
    };
    let read = connection.read(cx);
    let Some(offer) = read.install_offer() else {
        return;
    };
    if offer == zz_daemon_client::InstallOffer::Install {
        connection.update(cx, crate::connection::Connection::install_zz);
        return;
    }
    let name = read.endpoint().map_or_else(String::new, host_title);
    let (title, description) = zz_daemon_client::update_prompt(&[(name, read.session_count())]);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let accept = connection.clone();
        alert
            .title(title.clone())
            .description(description.clone())
            .button_props(
                DialogButtonProps::default()
                    .ok_variant(ButtonVariant::Paint(Tone::Danger, Emphasis::Solid))
                    .ok_text("Update and restart")
                    .cancel_text("Not now")
                    .show_cancel(true),
            )
            .on_ok(move |_, _, cx| {
                accept.update(cx, crate::connection::Connection::install_zz);
                true
            })
    });
}

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(super) fn copy_ssh_key(window: &mut Window, cx: &mut ala::App) {
    use penna::{WindowExt as _, blocks::notification::Notification};
    let notification = match zz_daemon_client::app_ssh_public_key() {
        Ok(key) => {
            cx.write_to_clipboard(ala::ClipboardItem::new_string(key));
            Notification::success("SSH key copied.")
        }
        Err(error) => Notification::error(error.to_string()),
    };
    window.push_notification(notification, cx);
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn row_meta(text: &'static str, cx: &ala::App) -> impl IntoElement {
    let theme = cx.theme();
    h_flex()
        .flex_none()
        .gap(px(6.0))
        .children((!text.is_empty()).then(|| {
            Label::new(TextStyle::Small, text)
                .muted()
                .on(settings_card_ground(cx))
        }))
        .child(
            Icon::new(IconName::ChevronRight)
                .size(px(14.0))
                .text_color(theme.foreground.faint()),
        )
}

fn status_line(text: impl Into<ala::SharedString>, tone: Tone, cx: &ala::App) -> Label {
    toned(
        Label::new(TextStyle::Caption, text),
        tone,
        settings_card_ground(cx),
        cx,
    )
}

fn toned(line: Label, tone: Tone, ground: Ground, cx: &ala::App) -> Label {
    match tone {
        Tone::Neutral => line.muted().on(ground),
        tone => line.text_color(cx.theme().tone(tone)),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn gateway_description() -> String {
    let address = gateway_address();
    if address.is_empty() {
        "zz web gateway".to_owned()
    } else {
        format!("zz web gateway at {address}")
    }
}

#[cfg(target_family = "wasm")]
fn gateway_address() -> String {
    web_sys::window()
        .and_then(|window| window.location().host().ok())
        .unwrap_or_default()
}

#[cfg(not(any(target_family = "wasm", target_os = "ios", target_os = "android")))]
fn gateway_address() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::{SavedHosts, destination_endpoint, host_location, host_title, waiting_for};

    #[test]
    fn destinations_default_to_ssh_and_reject_blank_or_spaced_input() {
        assert_eq!(
            destination_endpoint("  fabrico@mac.local ").unwrap(),
            "ssh://fabrico@mac.local"
        );
        assert_eq!(
            destination_endpoint("ssh://box:2222").unwrap(),
            "ssh://box:2222"
        );
        assert_eq!(
            destination_endpoint("/tmp/zz.sock").unwrap(),
            "/tmp/zz.sock"
        );
        assert!(destination_endpoint("   ").is_err());
        assert!(destination_endpoint("my mac").is_err());
    }

    #[test]
    fn titles_name_the_host_without_user_port_or_socket() {
        assert_eq!(host_title("ssh://fabrico@mac.local"), "mac.local");
        assert_eq!(host_title("ssh://box:2222/run/zz.sock"), "box");
        assert_eq!(host_title("ssh://me@[::1]:22"), "::1");
        assert_eq!(host_title("/tmp/zzlag.sock"), "zzlag");
        assert_eq!(host_title("tailnet://xps.example.ts.net:9923"), "xps");
        assert_eq!(host_title("tailnet://100.99.198.46"), "100.99.198.46");
        assert_eq!(
            host_title("/var/folders/5v/T/zz-dev-me/default.sock"),
            "Local socket"
        );
    }

    #[test]
    fn locations_keep_the_end_of_a_long_socket_path() {
        assert_eq!(host_location("ssh://me@box:2222"), "me@box:2222");
        assert_eq!(host_location("/tmp/zzlag.sock"), "/tmp/zzlag.sock");
        assert_eq!(
            host_location("tailnet://xps.example.ts.net"),
            "xps.example.ts.net"
        );
        assert_eq!(
            host_location("/var/folders/5v/y9xj/T//zz-dev-me/default.sock"),
            "…/zz-dev-me/default.sock"
        );
        assert_eq!(
            waiting_for("ssh://fabrico@xps"),
            "Waiting for ssh on fabrico@xps"
        );
        assert_eq!(
            waiting_for("tailnet://xps.example.ts.net:9923"),
            "Waiting for xps on your tailnet"
        );
    }

    #[test]
    fn hosts_keep_their_order_and_the_one_on_screen_reconnects_first() {
        let directory = std::env::temp_dir().join(format!(
            "zz-thin-hosts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        assert_eq!(SavedHosts::load_from(&directory), SavedHosts::default());
        std::fs::write(directory.join("endpoint"), "ssh://me@old\n").unwrap();
        let mut hosts = SavedHosts::load_from(&directory);
        assert_eq!(hosts.endpoints(), ["ssh://me@old"]);
        assert_eq!(hosts.most_recent(), Some("ssh://me@old"));
        hosts.mark_connected("ssh://me@new", true);
        hosts.mark_connected("ssh://me@other", false);
        hosts.add("ssh://me@new");
        assert_eq!(
            hosts.endpoints(),
            ["ssh://me@old", "ssh://me@new", "ssh://me@other"]
        );
        assert_eq!(hosts.connected(), ["ssh://me@new", "ssh://me@other"]);
        hosts.mark_connected("ssh://me@other", true);
        hosts.mark_connected("ssh://me@new", false);
        assert_eq!(hosts.connected(), ["ssh://me@other", "ssh://me@new"]);
        hosts.remove("ssh://me@other");
        assert_eq!(hosts.connected(), ["ssh://me@new"]);
        hosts.save_to(&directory).unwrap();
        std::fs::write(directory.join("endpoint"), "ssh://me@ignored").unwrap();
        let restored = SavedHosts::load_from(&directory);
        assert_eq!(restored, hosts);
        assert_eq!(restored.most_recent(), Some("ssh://me@new"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
