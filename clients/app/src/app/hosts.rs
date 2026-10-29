#[cfg(any(target_os = "ios", target_os = "android"))]
use ala::{AnyElement, Entity, Pixels, SharedString, Subscription, Window, px};
use ala::{Context, IntoElement, div, prelude::*};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Icon, IconName,
    button::Button,
    h_flex, rems_from_px,
    settings::{
        SettingEntry, SettingsScrollColumn, SettingsStack, settings_card_ground,
        settings_page_heading, settings_scroll_column,
    },
};
#[cfg(any(target_os = "ios", target_os = "android"))]
use zz_ui::{
    StyledExt as _,
    button::ButtonVariants as _,
    compact::{bottom_sheet, close_sheet, floating_sheet, sheet_close, sheet_inset, sheet_row},
    input::{Input, InputEvent, InputState},
    roles,
};

use super::{AppShell, settings::with_control};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::connection::Connection;

#[cfg(any(target_os = "ios", target_os = "android", test))]
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(super) struct SavedHosts {
    endpoints: Vec<String>,
    connected: Vec<String>,
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
impl SavedHosts {
    pub(super) fn endpoints(&self) -> &[String] {
        &self.endpoints
    }

    /// The hosts to reconnect at launch, the one last on screen first.
    pub(super) fn connected(&self) -> &[String] {
        &self.connected
    }

    pub(super) fn most_recent(&self) -> Option<&str> {
        self.connected
            .first()
            .or_else(|| self.endpoints.first())
            .map(String::as_str)
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
fn host_location(endpoint: &str) -> String {
    if let Some(rest) = endpoint.strip_prefix("ssh://") {
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
    _destination_events: Subscription,
}

#[cfg(any(target_os = "ios", target_os = "android"))]
impl Hosts {
    pub(super) fn new(window: &mut Window, cx: &mut Context<AppShell>) -> Self {
        let destination = cx.new(|cx| InputState::new(window, cx).placeholder("user@my-mac.local"));
        let destination_events = cx.subscribe_in(
            &destination,
            window,
            |this: &mut AppShell, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) && this.add_host(window, cx) {
                    close_sheet(ADD_HOST_SHEET, cx);
                }
            },
        );
        Self {
            saved: SavedHosts::load(),
            destination,
            error: None,
            sheet: None,
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

    #[cfg(any(target_os = "ios", target_os = "android"))]
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
        let endpoint = destination_endpoint(&value).and_then(|endpoint| {
            zz_daemon_client::Endpoint::parse(&endpoint)
                .map(|endpoint| endpoint.to_string())
                .map_err(|error| error.to_string())
        });
        match endpoint {
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

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn host_entries(&self, narrow: bool, cx: &mut Context<Self>) -> Vec<SettingEntry> {
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
                let icon = if endpoint.starts_with("ssh://") {
                    IconName::Globe
                } else {
                    IconName::HardDrive
                };
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
                let entry = SettingEntry::new(host_title(endpoint))
                    .description(endpoint.clone())
                    .title_icon(Icon::new(icon).text_color(color))
                    .when_some(state.as_ref(), |entry, (.., status)| {
                        entry.child(status_line(
                            status.clone(),
                            if connected || busy {
                                muted
                            } else {
                                cx.theme().warning
                            },
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
                            zz_ui::button::ButtonVariants::primary,
                        )
                        .label("Connect")
                        .on(settings_card_ground(cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.connect_host(target.clone(), window, cx);
                        }))
                };
                let removed = endpoint.clone();
                let remove = Button::new(("hosts-remove", index))
                    .ghost()
                    .icon(IconName::Xmark)
                    .tooltip("Remove host")
                    .on(settings_card_ground(cx))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.disconnect_host(&removed, window, cx);
                        this.hosts.saved.remove(&removed);
                        this.hosts.saved.save();
                        cx.notify();
                    }));
                with_control(
                    entry,
                    h_flex().gap_2().flex_none().child(action).child(remove),
                    narrow,
                )
            })
            .collect()
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    fn host_entries(&self, narrow: bool, cx: &mut Context<Self>) -> Vec<SettingEntry> {
        let connection = self.connection.read(cx);
        let connected = connection.connected;
        let muted = cx.theme().foreground.muted();
        let address = gateway_address();
        let entry = SettingEntry::new("This computer")
            .description(if address.is_empty() {
                "zz web gateway".to_owned()
            } else {
                format!("zz web gateway at {address}")
            })
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
                if connected { muted } else { cx.theme().warning },
            ));
        vec![with_control(
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
        )]
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn add_host_row(&self, cx: &mut Context<Self>) -> SettingsStack {
        let view = cx.entity().downgrade();
        SettingsStack::new().child(
            SettingEntry::new("Add host")
                .title_icon(Icon::new(IconName::Plus).text_color(cx.theme().accent))
                .control(row_meta("", cx))
                .on_click("hosts-add-row", move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.hosts.error = None;
                        this.hosts.sheet = Some(HostSheet::Add);
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
        let inset = sheet_inset(window);
        let sheet = match self.hosts.sheet.as_ref()? {
            HostSheet::Add => self.add_host_sheet(
                inset,
                |_, cx| close_sheet(ADD_HOST_SHEET, cx),
                |this, cx| {
                    this.hosts.sheet = None;
                    cx.notify();
                },
                cx,
            ),
            HostSheet::Host(endpoint) => self.host_sheet(endpoint, inset, cx),
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
        let muted = cx.theme().foreground.muted();
        let close =
            sheet_close("add-host-close").on_click(|_, _, cx| close_sheet(ADD_HOST_SHEET, cx));
        let dismiss = cx.weak_entity();
        let note = |text: SharedString| {
            div()
                .text_size(px(13.0))
                .line_height(px(18.0))
                .text_color(muted)
                .child(text)
        };
        bottom_sheet(
            ADD_HOST_SHEET,
            "Add host",
            [close.into_any_element()],
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .px(px(16.0))
                .pb(px(16.0))
                .child(Input::new(&self.hosts.destination))
                .child(note("user@host, or user@host:port".into()))
                .children(self.hosts.error.clone().map(|error| {
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .text_color(cx.theme().warning)
                        .child(error)
                }))
                .child(
                    Button::new("add-host-connect")
                        .primary()
                        .w_full()
                        .label("Connect")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.add_host(window, cx) {
                                on_added(this, cx);
                            }
                        })),
                )
                .child(div().my(px(6.0)).rule_on(roles::LIST_RULE, zz_ui::Edge::Top, cx))
                .child(note(
                    "Add this phone's key to authorized_keys on the host to sign in without a password."
                        .into(),
                ))
                .child(
                    Button::new("add-host-copy-key")
                        .w_full()
                        .icon(IconName::Copy)
                        .label("Copy SSH key")
                        .on_click(|_, window, cx| copy_ssh_key(window, cx)),
                ),
            bottom_inset,
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| on_dismiss(this, cx));
            },
        )
        .surface_role(roles::ADD_HOST_DRAWER)
        .header_role(roles::PHONE_BAR)
        .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn host_sheet(
        &self,
        endpoint: &str,
        bottom_inset: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
        let theme = cx.theme();
        let muted = theme.foreground.muted();
        let (state, color) = if connected {
            ("Connected".to_owned(), muted)
        } else if busy {
            ("Connecting…".to_owned(), muted)
        } else if status.is_empty() {
            ("Not connected".to_owned(), muted)
        } else {
            (status, theme.warning)
        };
        let danger = theme.danger;
        let target = endpoint.to_owned();
        let mut rows = Vec::new();
        if connected && !on_screen {
            let target = target.clone();
            rows.push(
                sheet_row(
                    "host-sheet-open",
                    roles::PHONE_ROW,
                    Some(Icon::new(IconName::ArrowRight)),
                    "Open",
                    cx,
                )
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
            sheet_row(
                "host-sheet-disconnect",
                roles::PHONE_ROW,
                Some(Icon::new(IconName::Stop)),
                if connected { "Disconnect" } else { "Cancel" },
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.disconnect_host(&toggle, window, cx);
            }))
        } else {
            sheet_row(
                "host-sheet-connect",
                roles::PHONE_ROW,
                Some(Icon::new(IconName::Server)),
                "Connect",
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.connect_host(toggle.clone(), window, cx);
            }))
        });
        rows.push(
            sheet_row(
                "host-sheet-remove",
                roles::PHONE_ROW,
                Some(Icon::new(IconName::Xmark)),
                "Remove host",
                cx,
            )
            .text_color(danger)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.disconnect_host(&target, window, cx);
                this.hosts.saved.remove(&target);
                this.hosts.saved.save();
                close_sheet(HOST_SHEET, cx);
                cx.notify();
            })),
        );
        let close =
            sheet_close("host-sheet-close").on_click(|_, _, cx| close_sheet(HOST_SHEET, cx));
        let dismiss = cx.weak_entity();
        bottom_sheet(
            HOST_SHEET,
            host_title(endpoint),
            [close.into_any_element()],
            div()
                .flex()
                .flex_col()
                .list_inset(cx)
                .pb(px(8.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .px(px(12.0))
                        .pb(px(10.0))
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .child(div().text_color(muted).child(endpoint.to_owned()))
                        .child(div().text_color(color).child(state)),
                )
                .children(rows),
            bottom_inset,
            move |_, cx| {
                let _ = dismiss.update(cx, |this, cx| {
                    this.hosts.sheet = None;
                    cx.notify();
                });
            },
        )
        .surface_role(roles::HOSTS_DRAWER)
        .header_role(roles::PHONE_BAR)
        .into_any_element()
    }

    #[cfg(any(target_os = "ios", target_os = "android"))]
    fn add_host_stack(&self, narrow: bool, cx: &mut Context<Self>) -> SettingsStack {
        let destination = SettingEntry::new("Destination")
            .description("user@host, or user@host:port")
            .when_some(self.hosts.error.clone(), |entry, error| {
                entry.child(status_line(error, cx.theme().warning))
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
pub(super) fn copy_ssh_key(window: &mut Window, cx: &mut ala::App) {
    use zz_ui::{WindowExt as _, notification::Notification};
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
            div()
                .text_size(rems_from_px(12.0))
                .text_color(theme.foreground.muted())
                .child(text)
        }))
        .child(
            Icon::new(IconName::ChevronRight)
                .size(px(14.0))
                .text_color(theme.foreground.faint()),
        )
}

fn status_line(text: impl Into<ala::SharedString>, color: ala::Hsla) -> impl IntoElement {
    div()
        .text_size(rems_from_px(11.0))
        .text_color(color)
        .child(text.into())
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
    use super::{SavedHosts, destination_endpoint, host_location, host_title};

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
            host_location("/var/folders/5v/y9xj/T//zz-dev-me/default.sock"),
            "…/zz-dev-me/default.sock"
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
