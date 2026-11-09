#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostState {
    Disconnected,
    Connecting,
    Reconnecting { attempt: u32 },
    Connected,
    Unreachable { reason: String },
    Incompatible { local: u16, remote: Option<u16> },
    Missing,
    Installing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallOffer {
    Install,
    Update,
}

impl HostState {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Connecting => "connecting",
            Self::Reconnecting { .. } => "reconnecting",
            Self::Connected => "connected",
            Self::Unreachable { .. } => "unreachable",
            Self::Incompatible { .. } => "incompatible",
            Self::Missing => "not installed",
            Self::Installing => "installing",
        }
    }

    pub const fn install_offer(&self) -> Option<InstallOffer> {
        match self {
            Self::Missing => Some(InstallOffer::Install),
            Self::Incompatible { remote: None, .. } => Some(InstallOffer::Update),
            Self::Incompatible {
                local,
                remote: Some(remote),
            } if *local >= *remote => Some(InstallOffer::Update),
            _ => None,
        }
    }

    pub fn failure_detail(&self) -> Option<String> {
        match self {
            Self::Unreachable { reason } => Some(reason.clone()),
            Self::Incompatible { remote: None, .. } => Some(
                "That machine runs a zz too old to report its protocol.\nUpdate zz there, then \
                 reconnect."
                    .to_owned(),
            ),
            Self::Incompatible {
                local,
                remote: Some(remote),
            } if local == remote => Some(format!(
                "That machine runs an older build of protocol v{remote}.\nUpgrade zz there, or \
                 restart its daemon, then reconnect."
            )),
            Self::Incompatible {
                local,
                remote: Some(remote),
            } => Some(format!(
                "This zz speaks protocol v{local}; that machine speaks v{remote}.\nUpgrade \
                 whichever side is older, then reconnect."
            )),
            Self::Missing => Some(
                "zz is not installed on that machine.\nRight-click it and choose Install zz."
                    .to_owned(),
            ),
            Self::Disconnected
            | Self::Connecting
            | Self::Reconnecting { .. }
            | Self::Connected
            | Self::Installing => None,
        }
    }
}


#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostIndicator { Connecting, Failed { detail: Option<String> } }

#[must_use]
pub fn indicator(state: &HostState, loaded: bool) -> Option<HostIndicator> {
    match state {
        HostState::Connected if loaded => None,
        HostState::Connected | HostState::Connecting | HostState::Reconnecting { .. } | HostState::Installing => Some(HostIndicator::Connecting),
        HostState::Disconnected | HostState::Unreachable { .. } | HostState::Incompatible { .. } | HostState::Missing => Some(HostIndicator::Failed { detail: state.failure_detail() }),
    }
}

#[must_use]
pub fn activation<Host: Copy + Eq>(host: Host, attached: Host, state: &HostState) -> Option<crate::desktop_nav::NavActivation<Host>> {
    use crate::desktop_nav::NavActivation;
    match state {
        HostState::Connected if host != attached => Some(NavActivation::AttachHost(host)),
        HostState::Disconnected | HostState::Reconnecting { .. } | HostState::Unreachable { .. } | HostState::Incompatible { .. } | HostState::Missing => Some(NavActivation::Reconnect(host)),
        _ => None,
    }
}
