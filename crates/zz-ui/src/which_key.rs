use std::sync::Arc;

use ala::{
    AnyElement, IntoElement, Keystroke, ParentElement as _, Pixels, SharedString, Size,
    Styled as _, div, px, size,
};
use penna::blocks::which_key::{WhichKeyCap, WhichKeyHeader, WhichKeyRow, WhichKeyView};
use zz_protocol::{KeyBindingSnapshot, KeyTableSnapshot};

const SHEET_MAX_WIDTH: f32 = 1360.0;
const SHEET_INSET: f32 = 16.0;
const SHEET_GAP: f32 = 8.0;

pub type Display = fn(&str) -> Option<Keystroke>;

pub struct WhichKeySheet {
    header: WhichKeyHeader,
    rows: Arc<[WhichKeyRow]>,
}

fn cap(keys: &[String], yours: bool, display: Display) -> WhichKeyCap {
    let parsed: Option<Vec<_>> = keys.iter().map(|key| display(key)).collect();
    WhichKeyCap {
        keys: parsed.unwrap_or_default(),
        raw: keys.join(" ").into(),
        yours,
    }
}

#[must_use]
pub fn row(row: zz_client::which_key::WhichKeyRow, display: Display) -> WhichKeyRow {
    WhichKeyRow {
        id: row.first_key().to_owned().into(),
        caps: row
            .keys
            .iter()
            .map(|set| cap(&set.keys, set.yours, display))
            .collect(),
        label: row.label.into(),
        group: row
            .group
            .map(|group| SharedString::new_static(group.title())),
        repeat: row.repeat,
    }
}

pub struct SheetKeys<'a> {
    pub tables: &'a [KeyTableSnapshot],
    pub prefix_bindings: &'a [KeyBindingSnapshot],
    pub prefix: Option<String>,
    pub display: Display,
}

impl WhichKeySheet {
    #[must_use]
    pub fn build(keys: SheetKeys<'_>, table: &str, all: bool) -> Option<Self> {
        let SheetKeys {
            tables,
            prefix_bindings,
            prefix,
            display,
        } = keys;
        let short = table == "prefix" && !all;
        let rows: Vec<_> =
            zz_client::which_key::rows(tables, table, prefix.as_deref().unwrap_or("C-b"))
                .into_iter()
                .filter(|row| row.core || !short)
                .collect();
        if rows.is_empty() {
            return None;
        }
        let more = short
            .then(|| {
                prefix_bindings
                    .iter()
                    .find(|binding| zz_client::which_key::opens_all_keys(&binding.commands))
            })
            .flatten()
            .map(|binding| cap(std::slice::from_ref(&binding.key), false, display));
        let prefix = if table == "prefix" {
            prefix.unwrap_or_default()
        } else {
            String::new()
        };
        Some(Self {
            header: WhichKeyHeader {
                table: table.to_owned().into(),
                prefix: display(&prefix),
                prefix_raw: prefix.into(),
                more,
            },
            rows: rows.into_iter().map(|each| row(each, display)).collect(),
        })
    }

    #[must_use]
    pub const fn header(&self) -> &WhichKeyHeader {
        &self.header
    }

    #[must_use]
    pub fn rows(&self) -> &[WhichKeyRow] {
        &self.rows
    }

    #[must_use]
    pub fn element(&self, top: Pixels, bottom: Pixels, canvas: Size<Pixels>) -> AnyElement {
        let available = size(
            (canvas.width - px(2.0 * SHEET_INSET)).min(px(SHEET_MAX_WIDTH)),
            canvas.height - px(2.0 * SHEET_GAP),
        );
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(top + px(SHEET_GAP))
            .bottom(bottom + px(SHEET_GAP))
            .px(px(SHEET_INSET))
            .flex()
            .flex_col()
            .justify_end()
            .items_center()
            .child(WhichKeyView::new(self.header.clone(), Arc::clone(&self.rows)).fit(available))
            .into_any_element()
    }
}
