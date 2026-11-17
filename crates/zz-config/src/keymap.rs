use zz_client::{
    BROWSER_TABLE, ChromeAction, ChromeKeymap, ChromeProfile,
    chrome_override::{ChromeOverride, parse_keystroke},
};

pub fn configured_keymap(overrides: &[ChromeOverride], hotkey: &str) -> ChromeKeymap {
    let mut keymap = ChromeKeymap::for_profile(ChromeProfile::DESKTOP);
    if let Some(chord) = parse_keystroke(hotkey).filter(|chord| !chord.function) {
        for (bound, action) in keymap.table_bindings(BROWSER_TABLE) {
            if action == ChromeAction::BrowserElementSelector && bound != chord.key {
                keymap.unbind(BROWSER_TABLE, &bound.to_string());
            }
        }
        let _ = keymap.bind(
            BROWSER_TABLE,
            &chord.key.to_string(),
            ChromeAction::BrowserElementSelector.name(),
        );
    } else {
        log::warn!(target: "zz::config", "keeping the built-in element selector chord: `{hotkey}` is not a chord zz can bind");
    }
    for error in keymap.apply(overrides) {
        log::warn!(target: "zz::config", "ignoring chrome binding for unknown action `{}`", error.0);
    }
    keymap
}
