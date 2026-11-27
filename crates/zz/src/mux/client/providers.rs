use super::*;

impl MuxClient {
    pub(crate) fn host_providers(&self, host: HostId) -> Option<&zz_protocol::ProvidersReport> {
        self.connections.get(&host)?.providers.as_ref()
    }

    pub(crate) fn providers_unrequested(&self) -> bool {
        self.connections.values().any(|connection| {
            matches!(connection.state, HostState::Connected) && !connection.providers_requested
        })
    }

    pub(crate) fn request_providers(&mut self, force: bool) {
        let hosts = self
            .connections
            .iter_mut()
            .filter(|(_, connection)| {
                matches!(connection.state, HostState::Connected)
                    && (force || !connection.providers_requested)
            })
            .map(|(host, connection)| {
                connection.providers_requested = true;
                *host
            })
            .collect::<Vec<_>>();
        for host in hosts {
            self.execute_on_host(
                host,
                CommandInvocation::new("providers", Vec::<String>::new()),
            );
            self.request_agent_accounts(host);
        }
    }

    pub(crate) fn update_provider(&self, host: HostId, provider: zz_protocol::AgentProvider) {
        self.execute_on_host(
            host,
            CommandInvocation::new("providers", ["-u", provider.as_str()]),
        );
    }

    pub(crate) fn host_agent_accounts(&self, host: HostId) -> Option<&[zz_protocol::AgentAccount]> {
        self.connections.get(&host)?.agent_accounts.as_deref()
    }

    pub(crate) fn host_agent_quotas(&self, host: HostId) -> &[zz_protocol::AgentQuota] {
        self.connections
            .get(&host)
            .map_or(&[], |connection| &connection.agent_quotas)
    }

    pub(crate) fn request_agent_accounts(&self, host: HostId) {
        if !self.host_platform(host).runs_agents() {
            return;
        }
        self.execute_on_host(
            host,
            CommandInvocation::new("agent-accounts", Vec::<String>::new()),
        );
    }

    pub(crate) fn host_signs_in(&self, host: HostId) -> bool {
        self.connections
            .get(&host)
            .and_then(|connection| connection.client.as_ref())
            .is_some_and(|client| {
                zz_client::agent_login::daemon_signs_in(&client.server_hello().capabilities)
            })
    }

    pub(crate) fn start_agent_login(
        &self,
        host: HostId,
        provider: zz_protocol::AgentProvider,
        account: zz_client::agent_login::LoginAccount<'_>,
        pane: PaneId,
    ) {
        let capabilities = self
            .connections
            .get(&host)
            .and_then(|connection| connection.client.as_ref())
            .map_or(&[][..], |client| {
                client.server_hello().capabilities.as_slice()
            });
        let command = zz_client::agent_login::entry(
            provider,
            account,
            pane,
            zz_client::agent_login::browser(host == HostId::LOCAL),
            capabilities,
        );
        self.execute_on_host(host, command);
    }

    pub(crate) fn start_provider_login(
        &self,
        host: HostId,
        provider: zz_protocol::AgentProvider,
        account: zz_client::agent_login::LoginAccount<'_>,
    ) {
        if !self.host_signs_in(host) {
            return;
        }
        let command = zz_client::agent_login::start(
            provider,
            account,
            None,
            zz_client::agent_login::browser(host == HostId::LOCAL),
        );
        self.execute_on_host(host, command);
    }

    pub(crate) fn host_agent_login(
        &self,
        host: HostId,
        pane: PaneId,
    ) -> Option<&zz_protocol::AgentLogin> {
        zz_client::agent_login::for_pane(&self.connections.get(&host)?.agent_logins, pane)
    }

    pub(crate) fn host_provider_login(
        &self,
        host: HostId,
        provider: zz_protocol::AgentProvider,
    ) -> Option<&zz_protocol::AgentLogin> {
        zz_client::agent_login::for_provider(&self.connections.get(&host)?.agent_logins, provider)
    }

    pub(crate) fn dismiss_agent_login(&mut self, host: HostId, login: u64) {
        if let Some(connection) = self.connections.get_mut(&host) {
            connection
                .agent_logins
                .retain(|known| known.id != login || !known.finished());
        }
    }

    pub(crate) fn send_agent_login(&self, host: HostId, message: &ProtocolMessage) {
        let Some(client) = self
            .connections
            .get(&host)
            .and_then(|connection| connection.client.as_ref())
        else {
            return;
        };
        if let Err(error) = client.send(message) {
            log::warn!(target: "zz::agent", "could not send a sign-in request: {error}");
        }
    }

    pub(super) fn observe_agent_login(
        &mut self,
        host: HostId,
        login: zz_protocol::AgentLogin,
        cx: &mut Context<Self>,
    ) {
        let Some(connection) = self.connections.get_mut(&host) else {
            return;
        };
        let open = match &login.step {
            zz_protocol::AgentLoginStep::OpenUrl { url }
            | zz_protocol::AgentLoginStep::PasteCode { url }
                if host == HostId::LOCAL
                    && login.browser == zz_protocol::AgentLoginBrowser::Local
                    && login.provider != zz_protocol::AgentProvider::Grok
                    && connection.opened_logins.insert(login.id) =>
            {
                Some(url.clone())
            }
            _ => None,
        };
        if zz_client::agent_login::reduce(&mut connection.agent_logins, login) {
            cx.notify();
        }
        if let Some(url) = open {
            cx.open_url(&url);
        }
    }
}
