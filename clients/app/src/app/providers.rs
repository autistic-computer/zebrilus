use ala::{Context, Entity, Window, prelude::*};
use penna::{ActiveTheme as _, Colorize as _, Icon, IconName, components::button::Button, h_flex};
use zz_ui::settings::{
    SettingEntry, SettingsScrollColumn, SettingsStack,
    providers::{ProvidersHost, agent_setup_notification, providers_stack},
    settings_card_ground, settings_page_heading, settings_scroll_column,
};

use super::AppShell;
use crate::connection::{AgentsSetUp, Connection};

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
                    SettingEntry::new(self.connection_title(connection.read(cx)))
                        .title_icon(Icon::new(IconName::HardDrive).text_color(muted))
                },
            ))
        });
        let wide = title.is_some();
        let setup_rows = online
            .iter()
            .filter(|(_, connection)| wide && connection.read(cx).sets_up_agents())
            .map(|(index, connection)| {
                let target = (*connection).clone();
                SettingEntry::new(self.connection_title(connection.read(cx)))
                    .title_icon(Icon::new(IconName::HardDrive).text_color(muted))
                    .description(
                        "A startup hook so Claude Code and Codex in terminal panes know they run in zz",
                    )
                    .control(
                        Button::new(("settings-agent-setup", *index))
                            .label("Set up")
                            .on(settings_card_ground(cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.set_up_agents(&target, window, cx);
                            })),
                    )
            })
            .collect::<Vec<_>>();
        let setup = (!setup_rows.is_empty())
            .then(|| SettingsStack::titled("Coding agents").children(setup_rows));
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
                    self.connection_title(read),
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
            .children(setup)
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

impl AppShell {
    fn set_up_agents(
        &mut self,
        connection: &Entity<Connection>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.agent_setup
            .retain(|(watched, _)| watched.upgrade().is_some());
        if !self
            .agent_setup
            .iter()
            .any(|(watched, _)| watched.entity_id() == connection.entity_id())
        {
            let subscription = cx.subscribe_in(
                connection,
                window,
                |this, connection, event: &AgentsSetUp, window, cx| {
                    use penna::WindowExt as _;
                    let host = this.connection_title(connection.read(cx));
                    window.push_notification(
                        agent_setup_notification(event.0.clone(), Some(&host)),
                        cx,
                    );
                },
            );
            self.agent_setup
                .push((connection.downgrade(), subscription));
        }
        connection.update(cx, Connection::set_up_agents);
    }

    fn connection_title(&self, connection: &Connection) -> String {
        connection
            .endpoint()
            .map_or_else(|| "Host".to_owned(), |key| self.hosts.title(key))
    }
}
