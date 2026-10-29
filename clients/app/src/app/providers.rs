use ala::{Context, prelude::*};
use zz_ui::{
    ActiveTheme as _, Colorize as _, Icon, IconName,
    button::Button,
    h_flex,
    settings::{
        SettingEntry, SettingsScrollColumn, SettingsStack,
        providers::{ProvidersHost, providers_stack},
        settings_page_heading, settings_scroll_column,
    },
};

use super::AppShell;
use crate::connection::Connection;

impl AppShell {
    pub(super) fn providers_page(
        &self,
        title: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> SettingsScrollColumn {
        let connections = self.host_connections();
        if connections
            .iter()
            .any(|connection| connection.read(cx).providers_unrequested())
        {
            let pending = connections.clone();
            cx.defer(move |cx| {
                for connection in pending {
                    connection.update(cx, |connection, cx| connection.request_providers(false, cx));
                }
            });
        }
        let (online, offline): (Vec<_>, Vec<_>) = connections
            .iter()
            .enumerate()
            .partition(|(_, connection)| connection.read(cx).connected);
        let muted = cx.theme().foreground.muted();
        let offline = (!offline.is_empty()).then(|| {
            SettingsStack::titled("Not connected").children(offline.into_iter().map(
                |(_, connection)| {
                    SettingEntry::new(host_name(connection.read(cx)))
                        .title_icon(Icon::new(IconName::HardDrive).text_color(muted))
                },
            ))
        });
        let stacks = online
            .into_iter()
            .map(|(index, connection)| {
                let read = connection.read(cx);
                let shown = read
                    .core
                    .providers()
                    .cloned()
                    .map_or(ProvidersHost::Waiting, ProvidersHost::Report);
                let target = connection.downgrade();
                providers_stack(
                    format!("host-{index}"),
                    host_name(read),
                    &shown,
                    move |provider, _, cx| {
                        let _ = target.update(cx, |connection, cx| {
                            connection.update_provider(provider, cx);
                        });
                    },
                    cx,
                )
            })
            .collect::<Vec<_>>();
        settings_scroll_column("settings-providers")
            .children(title.map(|title| settings_page_heading(title, cx)))
            .children(stacks)
            .children(offline)
            .child(
                h_flex().justify_end().child(
                    Button::new("settings-providers-check")
                        .label("Check again")
                        .on_click(move |_, _, cx| {
                            for connection in &connections {
                                connection.update(cx, |connection, cx| {
                                    connection.request_providers(true, cx);
                                });
                            }
                        }),
                ),
            )
    }
}

#[cfg(target_os = "ios")]
fn host_name(connection: &Connection) -> String {
    connection
        .endpoint()
        .map_or_else(|| "Host".to_owned(), super::hosts::host_title)
}

#[cfg(not(target_os = "ios"))]
fn host_name(_: &Connection) -> String {
    "This computer".to_owned()
}
