pub mod floating;
mod gui_level;
mod level;
mod palette_model;
mod palette_places;
mod palette_view;

pub use gui_level::GuiLevel;
pub use level::{
    ActionContext, ActionKey, CopyAddress, LevelEffect, PALETTE_TABS, PALETTE_VERBS,
    PaletteActionLevel, install_recents, palette_keys, palette_recents, record_recent,
};
pub use palette_model::{
    PaletteChat, PaletteHostId, PaletteMode, PaletteSettings, PaletteTarget, PaletteTree,
    PaletteTreeHost, PaletteTreePane, PaletteTreeSession, PaletteTreeWindow, palette_chats,
};
pub use palette_places::palette_fleet;
pub use palette_view::{
    CommandPaletteEvent, CommandPaletteView, FolderPick, GuiEvent, PaletteBackend, mention_picker,
    target_hosts,
};

use ala::{Keystroke, SharedString};
use penna::blocks::palette::PaletteShortcut;

#[must_use]
pub fn tmux_shortcut(spelling: &SharedString) -> PaletteShortcut {
    let keys: Option<Vec<Keystroke>> = spelling.split_whitespace().map(tmux_keystroke).collect();
    match keys {
        Some(keys) if !keys.is_empty() => PaletteShortcut::Keys(keys),
        _ => PaletteShortcut::Text(spelling.clone()),
    }
}

#[must_use]
pub fn tmux_keystroke(spelling: &str) -> Option<Keystroke> {
    let (source, base) = zz_client::palette::tmux_keystroke_source(spelling)?;
    let base = base.as_str();
    let mut keystroke = Keystroke::parse(&source).ok()?;
    if base == "minus" {
        keystroke.key = "-".into();
    }
    if keystroke.key_char.is_none()
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && base.chars().count() == 1
    {
        keystroke.key_char = Some(base.to_owned());
    }
    Some(keystroke)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_spellings_become_ala_keystrokes() {
        let control = tmux_keystroke("C-b").expect("prefix");
        assert!(control.modifiers.control);
        assert_eq!(control.key, "b");
        let percent = tmux_keystroke("%").expect("percent");
        assert_eq!(percent.key, "%");
        assert_eq!(percent.key_char.as_deref(), Some("%"));
        let upper = tmux_keystroke("L").expect("upper");
        assert!(upper.modifiers.shift);
        assert_eq!(upper.key, "l");
        assert_eq!(
            tmux_keystroke("Escape").map(|key| key.key),
            Some("escape".into())
        );
        assert_eq!(
            tmux_keystroke("M-Up").map(|key| (key.key, key.modifiers.alt)),
            Some(("up".into(), true))
        );
        assert!(tmux_keystroke("F13-nope").is_none());
    }
}
