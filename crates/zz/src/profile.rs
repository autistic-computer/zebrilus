use ala::{App, Global};

pub use zz_ui::settings::SettingsSection;

#[derive(Clone, Debug)]
pub struct AppProfile {
    pub settings_sections: Vec<SettingsSection>,
    pub has_tray: bool,
    pub has_window_blur: bool,
    pub has_daemon_lifecycle: bool,
    pub has_config_import: bool,
    pub local_host: LocalHostPolicy,
    pub fixed_window: bool,
}

impl AppProfile {
    #[must_use]
    pub fn desktop() -> Self {
        let settings_sections = SettingsSection::ALL
            .into_iter()
            .filter(|section| *section != SettingsSection::Editor || cfg!(feature = "editor-pane"))
            .collect();
        Self {
            settings_sections,
            has_tray: true,
            has_window_blur: true,
            has_daemon_lifecycle: true,
            has_config_import: true,
            local_host: LocalHostPolicy::Always,
            fixed_window: false,
        }
    }
}

impl Global for AppProfile {}

#[cfg(test)]
static TEST_PROFILE: std::sync::LazyLock<AppProfile> =
    std::sync::LazyLock::new(AppProfile::desktop);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalHostPolicy {
    Always,
}

impl LocalHostPolicy {
    pub fn synthesize_local(self) -> bool {
        match self {
            Self::Always => true,
        }
    }
}

#[must_use]
pub(crate) fn profile(cx: &App) -> &AppProfile {
    #[cfg(test)]
    {
        cx.try_global::<AppProfile>().unwrap_or(&TEST_PROFILE)
    }
    #[cfg(not(test))]
    {
        cx.global::<AppProfile>()
    }
}
