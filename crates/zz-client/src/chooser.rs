use zz_protocol::{ChooseBufferItem, ChooseTreeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChooserHint {
    pub keys: &'static [&'static str],
    pub label: &'static str,
}

const fn hint(keys: &'static [&'static str], label: &'static str) -> ChooserHint {
    ChooserHint { keys, label }
}

pub const SEARCH_HINTS: &[ChooserHint] = &[hint(&["enter"], "accept"), hint(&["escape"], "cancel")];

pub const TREE_HINTS: &[ChooserHint] = &[
    hint(&["up", "down"], "navigate"),
    hint(&["left", "right"], "expand"),
    hint(&["enter"], "choose"),
    hint(&["/"], "search"),
    hint(&["escape"], "close"),
];

pub const BUFFER_HINTS: &[ChooserHint] = &[
    hint(&["up", "down"], "navigate"),
    hint(&["enter"], "paste"),
    hint(&["d"], "delete"),
    hint(&["/"], "search"),
    hint(&["escape"], "close"),
];

#[must_use]
pub fn chooser_key_label(key: &str) -> String {
    if key.is_empty() {
        String::new()
    } else {
        format!("({key})")
    }
}

pub fn chooser_has_key_gutter<'a>(keys: impl IntoIterator<Item = &'a str>) -> bool {
    keys.into_iter().any(|key| !key.is_empty())
}

pub fn chooser_subtitle(summary: impl Into<String>, filter_no_matches: bool) -> String {
    let summary = summary.into();
    if filter_no_matches {
        format!("{summary} · filter: no matches")
    } else {
        summary
    }
}

#[must_use]
pub const fn tree_title(kind: ChooseTreeKind) -> (&'static str, &'static str) {
    match kind {
        ChooseTreeKind::Windows => ("Choose window", "sessions and windows"),
        ChooseTreeKind::Panes => ("Choose pane", "sessions, windows, and panes"),
        ChooseTreeKind::Clients => ("Choose client", "clients"),
    }
}

#[must_use]
pub fn buffer_row_text(item: &ChooseBufferItem, now: u64) -> (String, String, String, String) {
    if !item.text.is_empty() {
        return (
            item.text.clone(),
            String::new(),
            String::new(),
            String::new(),
        );
    }
    let bytes = item.size_bytes;
    let size = if bytes < 1_024 {
        format!("{bytes} B")
    } else {
        let (unit, label) = if bytes < 1_048_576 {
            (1_024, "KiB")
        } else if bytes < 1_073_741_824 {
            (1_048_576, "MiB")
        } else {
            (1_073_741_824, "GiB")
        };
        let tenths = bytes.saturating_mul(10).saturating_add(unit / 2) / unit;
        format!("{}.{:01} {label}", tenths / 10, tenths % 10)
    };
    let age = now.saturating_sub(item.created_unix_seconds);
    let age = match age {
        0..=59 => format!("{age}s"),
        60..=3_599 => format!("{}m", age / 60),
        3_600..=86_399 => format!("{}h", age / 3_600),
        _ => format!("{}d", age / 86_400),
    };
    (item.name.clone(), item.preview.clone(), size, age)
}

#[cfg(test)]
mod tests {
    use super::buffer_row_text;

    #[test]
    fn buffer_rows_show_compact_metadata_and_respect_custom_formats() {
        let mut item = zz_protocol::ChooseBufferItem {
            name: "buffer0".into(),
            preview: "hello".into(),
            size_bytes: 1_536,
            created_unix_seconds: 10,
            key: "0".into(),
            text: String::new(),
            tagged: true,
        };
        assert_eq!(
            buffer_row_text(&item, 80),
            (
                "buffer0".into(),
                "hello".into(),
                "1.5 KiB".into(),
                "1m".into()
            )
        );
        assert_eq!(buffer_row_text(&item, 0).3, "0s");
        item.text = "<<buffer0>>".into();
        assert_eq!(
            buffer_row_text(&item, 80),
            (
                "<<buffer0>>".into(),
                String::new(),
                String::new(),
                String::new()
            )
        );
    }
}
