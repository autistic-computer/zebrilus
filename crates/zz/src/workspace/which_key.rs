use zz_ui::which_key::{SheetKeys, WhichKeySheet};

use crate::mux::{client::MuxClient, prefix::display_keystroke};

pub(super) fn sheet(mux: &MuxClient, table: &str, all: bool) -> Option<WhichKeySheet> {
    WhichKeySheet::build(
        SheetKeys {
            tables: mux.key_tables(),
            prefix_bindings: mux.prefix_bindings(),
            prefix: mux.canonical_prefix(),
            display: display_keystroke,
        },
        table,
        all,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_stock_prefix_row_gets_a_key_cap() {
        let tables = zz_protocol::KeyTables::default().snapshot();
        let rows = zz_client::which_key::rows(&tables, "prefix", "C-b");
        assert!(!rows.is_empty());
        for key in rows
            .iter()
            .flat_map(|row| row.keys.iter().flat_map(|set| &set.keys))
        {
            assert!(
                crate::mux::prefix::display_keystroke(key).is_some(),
                "{key:?} has no key cap"
            );
        }
    }
}
