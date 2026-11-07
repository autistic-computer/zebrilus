use std::collections::BTreeMap;
use zz_client::{CHROME_TABLES, ChromeAction, ChromeKey, ChromeKeymap};

use super::ala_source;

pub type BoundChromeChords = BTreeMap<(&'static str, String), ChromeAction>;
pub type DroppedChromeChords = Vec<(&'static str, String, ChromeAction)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeChromeChord {
    pub action: ChromeAction,
    pub key: String,
    pub source: String,
    pub live: bool,
}

#[must_use]
pub fn bound_chords(keymap: &ChromeKeymap) -> BoundChromeChords {
    CHROME_TABLES
        .iter()
        .flat_map(|table| {
            keymap
                .table_bindings(table)
                .into_iter()
                .map(move |(key, action)| ((*table, key.to_string()), action))
        })
        .collect()
}

#[must_use]
pub fn dropped_chords(
    previous: &BoundChromeChords,
    next: &BoundChromeChords,
) -> DroppedChromeChords {
    previous
        .iter()
        .filter(|(chord, _)| !next.contains_key(*chord))
        .map(|((table, key), action)| (*table, key.clone(), *action))
        .collect()
}

#[must_use]
pub fn native_chords(
    keymap: &ChromeKeymap,
    dropped: &DroppedChromeChords,
    table: &str,
) -> Vec<NativeChromeChord> {
    let mut chords = keymap
        .table_bindings(table)
        .into_iter()
        .filter_map(|(key, action)| {
            Some(NativeChromeChord {
                action,
                source: ala_source(&key)?,
                key: key.to_string(),
                live: true,
            })
        })
        .collect::<Vec<_>>();
    chords.extend(
        dropped
            .iter()
            .filter(|(dropped, _, _)| *dropped == table)
            .filter_map(|(_, key, action)| {
                Some(NativeChromeChord {
                    action: *action,
                    source: ala_source(&ChromeKey::parse(key)?)?,
                    key: key.clone(),
                    live: false,
                })
            }),
    );
    chords
}
