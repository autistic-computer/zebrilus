use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use ala::{Context, Entity, Window, prelude::*};
use penna::{ActiveTheme as _, Colorize as _, Icon, IconName, components::button::Button, h_flex};
use zz_client::agent_login::{
    CardAction, CardClose, LoginAccount, cancel, card, restart, retry, settled,
};
use zz_protocol::{AgentLoginBrowser, AgentProvider};
use zz_ui::agent::login::{CodeField, LoginEvent, login_card_keyed};
use zz_ui::settings::{
    SettingEntry, SettingsScrollColumn, SettingsStack,
    providers::{
        PROVIDERS, ProviderAuthAction, ProvidersAuth, ProvidersHost, agent_setup_notification,
        providers_stack_with_auth,
    },
    settings_card_ground, settings_page_heading, settings_scroll_column,
};

use super::AppShell;
use crate::connection::{AgentsSetUp, Connection};

impl AppShell {
    pub(super) fn sync_provider_logins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let connections = self.host_connections();
        let mut active_keys = Vec::new();
        let mut settled_logins = Vec::new();
        for connection in connections {
            let connection_id = connection.entity_id();
            let logins = {
                let read = connection.read(cx);
                if !read.connected {
                    continue;
                }
                PROVIDERS
                    .into_iter()
                    .filter_map(|provider| {
                        let login = read.provider_login(provider)?;
                        if settled(login) {
                            settled_logins.push((connection.clone(), login.id));
                            None
                        } else {
                            Some((provider, login.clone()))
                        }
                    })
                    .collect::<Vec<_>>()
            };
            for (provider, login) in logins {
                let key = (connection_id, provider);
                active_keys.push(key);
                let mut slot = self.provider_login_codes.remove(&key);
                let target = connection.downgrade();
                CodeField::sync(
                    &mut slot,
                    Some(&login),
                    window,
                    cx,
                    move |this, window, cx| {
                        if let Some(connection) = target.upgrade() {
                            this.provider_login_event(
                                &connection,
                                provider,
                                LoginEvent::Action(CardAction::Continue),
                                window,
                                cx,
                            );
                        }
                    },
                );
                if let Some(field) = slot {
                    self.provider_login_codes.insert(key, field);
                }
            }
        }
        self.provider_login_codes
            .retain(|key, _| active_keys.contains(key));
        if !settled_logins.is_empty() {
            cx.defer(move |cx| {
                for (connection, id) in settled_logins {
                    connection.update(cx, |connection, cx| connection.dismiss_agent_login(id, cx));
                }
            });
        }
    }

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
        let shell = cx.entity().downgrade();
        let stacks = online
            .into_iter()
            .map(|(index, connection)| {
                let connection_id = connection.entity_id();
                let read = connection.read(cx);
                let shown = read
                    .core
                    .providers()
                    .cloned()
                    .map_or(ProvidersHost::Waiting, ProvidersHost::Report);
                let signs_in = read.signs_in() && !read.core.attached_read_only();
                let accounts = read
                    .core
                    .agent_accounts()
                    .map(<[zz_protocol::AgentAccount]>::to_vec);
                let login_cards = PROVIDERS
                    .into_iter()
                    .filter_map(|provider| {
                        let login = read.provider_login(provider)?;
                        if settled(login) {
                            return None;
                        }
                        let shell = shell.clone();
                        let target = connection.downgrade();
                        let field = self.provider_login_codes.get(&(connection_id, provider));
                        let key = format!("settings-host-{index}-{}", provider.as_str());
                        let card = login_card_keyed(
                            &key,
                            login,
                            field,
                            move |event, window, cx| {
                                if let Some(connection) = target.upgrade() {
                                    shell
                                        .update(cx, |this, cx| {
                                            this.provider_login_event(
                                                &connection,
                                                provider,
                                                event,
                                                window,
                                                cx,
                                            );
                                        })
                                        .ok();
                                }
                            },
                            cx,
                        )?;
                        Some((provider, card))
                    })
                    .collect::<BTreeMap<_, _>>();
                let login_cards = RefCell::new(login_cards);
                let auth_target = connection.downgrade();
                let auth = ProvidersAuth {
                    signs_in,
                    accounts: accounts.as_deref(),
                    on_auth: Rc::new(move |provider, action, _, cx| {
                        let account = match action {
                            ProviderAuthAction::SignIn => LoginAccount::Default,
                            ProviderAuthAction::AddAccount => LoginAccount::New,
                        };
                        let _ = auth_target.update(cx, |connection, cx| {
                            connection.start_provider_login(provider, account, cx);
                        });
                    }),
                    login: Rc::new(move |provider, _| login_cards.borrow_mut().remove(&provider)),
                };
                let target = connection.downgrade();
                providers_stack_with_auth(
                    format!("host-{index}"),
                    self.connection_title(read),
                    &shown,
                    Some(&auth),
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

    fn provider_login_event(
        &mut self,
        connection: &Entity<Connection>,
        provider: AgentProvider,
        event: LoginEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(login) = connection.read(cx).provider_login(provider).cloned() else {
            return;
        };
        let Some(card) = card(&login) else {
            return;
        };
        let accounts = connection
            .read(cx)
            .core
            .agent_accounts()
            .unwrap_or_default()
            .to_vec();
        let key = (connection.entity_id(), provider);
        let dismiss = |this: &mut Self, cx: &mut Context<Self>| {
            connection.update(cx, |connection, cx| {
                connection.dismiss_agent_login(login.id, cx);
            });
            this.provider_login_codes.remove(&key);
        };
        match event {
            LoginEvent::Close if card.close == CardClose::Cancel => {
                connection.update(cx, |connection, cx| connection.send(cancel(&login), cx));
            }
            LoginEvent::Close => dismiss(self, cx),
            LoginEvent::Action(CardAction::UseCode) => {
                if login.finished() {
                    dismiss(self, cx);
                    connection.update(cx, |connection, cx| {
                        connection
                            .invoke(retry(&login, &accounts, AgentLoginBrowser::Elsewhere), cx);
                    });
                } else {
                    connection.update(cx, |connection, cx| {
                        connection.invoke(restart(&login, AgentLoginBrowser::Elsewhere), cx);
                    });
                }
            }
            LoginEvent::Action(CardAction::Retry) => {
                dismiss(self, cx);
                connection.update(cx, |connection, cx| {
                    connection.invoke(retry(&login, &accounts, login.browser), cx);
                });
            }
            LoginEvent::Action(CardAction::Continue) => {
                if let Some(message) = self
                    .provider_login_codes
                    .get(&key)
                    .and_then(|field| field.take(&login, window, cx))
                {
                    connection.update(cx, |connection, cx| connection.send(message, cx));
                }
            }
            LoginEvent::Action(CardAction::SwitchWhenIdle | CardAction::SwitchNow) => {
                dismiss(self, cx);
            }
        }
        cx.notify();
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
                    window.push_notification(agent_setup_notification(&event.0, Some(&host)), cx);
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
