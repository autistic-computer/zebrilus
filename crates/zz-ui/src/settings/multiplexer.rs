use zz_protocol::MuxOptionKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuxControl {
    Key,
    Choice(&'static [(&'static str, &'static str)]),
    Switch,
    Number,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MuxSetting {
    pub key: MuxOptionKey,
    pub title: &'static str,
    pub description: Option<&'static str>,
    pub control: MuxControl,
}

impl MuxSetting {
    #[must_use]
    pub fn choices(self) -> &'static [(&'static str, &'static str)] {
        match self.control {
            MuxControl::Choice(choices) => choices,
            _ => &[],
        }
    }
}

const fn setting(key: MuxOptionKey, title: &'static str, control: MuxControl) -> MuxSetting {
    MuxSetting {
        key,
        title,
        description: None,
        control,
    }
}

pub const MUX_SETTINGS: [MuxSetting; 6] = [
    MuxSetting {
        description: Some("The key you press before each shortcut"),
        ..setting(MuxOptionKey::Prefix, "Prefix", MuxControl::Key)
    },
    setting(
        MuxOptionKey::ModeKeys,
        "Mode keys",
        MuxControl::Choice(&[("Vi", "vi"), ("Emacs", "emacs")]),
    ),
    setting(MuxOptionKey::Mouse, "Mouse", MuxControl::Switch),
    setting(
        MuxOptionKey::HistoryLimit,
        "History limit",
        MuxControl::Number,
    ),
    setting(
        MuxOptionKey::SetClipboard,
        "Clipboard",
        MuxControl::Choice(&[("On", "on"), ("External", "external"), ("Off", "off")]),
    ),
    setting(
        MuxOptionKey::EscapeTime,
        "Escape time (ms)",
        MuxControl::Number,
    ),
];

#[must_use]
pub fn mux_setting(key: MuxOptionKey) -> Option<MuxSetting> {
    MUX_SETTINGS.into_iter().find(|setting| setting.key == key)
}
