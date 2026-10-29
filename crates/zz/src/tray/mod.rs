pub(crate) use desktop::{
    DesktopTray, focus_pane, hide, hide_or_quit, init_desktop, quit_requested, set_active, show,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayEvent {
    Toggle,
    Quit,
    Available(bool),
    NewSession,
    SwitchSession(String),
    FocusPane(crate::mux::hosts::HostId, zz_protocol::PaneId),
    OpenSettings,
    OpenLogs,
    RestartDaemon,
}

impl TrayEvent {
    fn action(&self) -> Option<String> {
        Some(match self {
            Self::Toggle => "toggle".to_owned(),
            Self::Quit => "quit".to_owned(),
            Self::NewSession => "new-session".to_owned(),
            Self::SwitchSession(name) => format!("session {name}"),
            Self::FocusPane(host, pane) if *host == crate::mux::hosts::HostId::LOCAL => {
                format!("pane {pane}")
            }
            Self::OpenSettings => "settings".to_owned(),
            Self::OpenLogs => "logs".to_owned(),
            Self::RestartDaemon => "restart-daemon".to_owned(),
            Self::Available(_) | Self::FocusPane(..) => return None,
        })
    }

    fn from_tray(event: penna::tray::TrayEvent) -> Option<Self> {
        use penna::tray::TrayEvent as Tray;
        match event {
            Tray::Activate => Some(Self::Toggle),
            Tray::Available(available) => Some(Self::Available(available)),
            Tray::Action(action) => Self::from_action(&action),
        }
    }

    fn from_action(action: &str) -> Option<Self> {
        if let Some(name) = action.strip_prefix("session ") {
            return Some(Self::SwitchSession(name.to_owned()));
        }
        if let Some(pane) = action.strip_prefix("pane ") {
            return pane
                .parse()
                .ok()
                .map(|pane| Self::FocusPane(crate::mux::hosts::HostId::LOCAL, pane));
        }
        Some(match action {
            "toggle" => Self::Toggle,
            "quit" => Self::Quit,
            "new-session" => Self::NewSession,
            "settings" => Self::OpenSettings,
            "logs" => Self::OpenLogs,
            "restart-daemon" => Self::RestartDaemon,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToggleAction {
    Hide,
    Raise,
    Show,
}

pub(crate) const fn toggle_action(visible: bool, active: bool) -> ToggleAction {
    match (visible, active) {
        (true, true) => ToggleAction::Hide,
        (true, false) => ToggleAction::Raise,
        (false, _) => ToggleAction::Show,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuitAction {
    HideToTray,
    Quit,
}

#[allow(clippy::fn_params_excessive_bools)]
#[must_use]
pub(crate) const fn quit_action(
    tray_enabled: bool,
    available: bool,
    quit_daemon_on_exit: bool,
    has_sessions: bool,
) -> QuitAction {
    if tray_enabled && available && !quit_daemon_on_exit && has_sessions {
        QuitAction::HideToTray
    } else {
        QuitAction::Quit
    }
}

#[cfg(test)]
mod tests {
    use super::{QuitAction, ToggleAction, TrayEvent, quit_action, toggle_action};
    use crate::mux::hosts::HostId;
    use zz_protocol::PaneId;

    #[test]
    fn every_menu_action_reads_back_as_the_event_it_names() {
        for event in [
            TrayEvent::Toggle,
            TrayEvent::Quit,
            TrayEvent::NewSession,
            TrayEvent::SwitchSession("work: two".into()),
            TrayEvent::FocusPane(HostId::LOCAL, PaneId(9)),
            TrayEvent::OpenSettings,
            TrayEvent::OpenLogs,
            TrayEvent::RestartDaemon,
        ] {
            let action = event.action().expect("a menu action");
            assert_eq!(TrayEvent::from_action(&action), Some(event));
        }
        assert_eq!(TrayEvent::Available(true).action(), None);
        assert_eq!(TrayEvent::from_action("sparkle"), None);
        assert_eq!(
            TrayEvent::from_tray(penna::tray::TrayEvent::Activate),
            Some(TrayEvent::Toggle)
        );
    }

    #[test]
    fn quit_hides_only_with_an_available_tray_and_preserved_sessions() {
        assert_eq!(quit_action(true, true, false, true), QuitAction::HideToTray);
        assert_eq!(quit_action(false, true, false, true), QuitAction::Quit);
        assert_eq!(quit_action(true, false, false, true), QuitAction::Quit);
        assert_eq!(quit_action(true, true, true, true), QuitAction::Quit);
        assert_eq!(quit_action(true, true, false, false), QuitAction::Quit);
    }

    #[test]
    fn the_icon_dismisses_a_window_that_already_has_focus() {
        assert_eq!(toggle_action(true, true), ToggleAction::Hide);
    }

    #[test]
    fn a_buried_window_is_summoned_rather_than_dismissed() {
        assert_eq!(toggle_action(true, false), ToggleAction::Raise);
    }

    #[test]
    fn a_hidden_window_comes_back_however_focus_reads() {
        assert_eq!(toggle_action(false, false), ToggleAction::Show);
        assert_eq!(toggle_action(false, true), ToggleAction::Show);
    }
}

mod desktop;
mod facts;
mod look;
