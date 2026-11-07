use std::{error::Error, io};
use zz_terminal::Modifiers;

use serde_json::{Value, json};
use zz_client::chooser::{
    chooser_has_key_gutter, chooser_key_is_text, chooser_key_label, chooser_modal_height,
    chooser_subtitle, tree_row_text, tree_title,
};
use zz_protocol::{
    ChooseTreeItem, ChooseTreeKind, ChooseTreePaneKind, ChooseTreeSearchState, ChooseTreeState,
    ChooseTreeTarget, ClientId, Event, EventPayload, PaneId, ProtocolMessage, SessionId, WindowId,
    encode_protocol_message,
};

fn item(index: u8) -> ChooseTreeItem {
    let id = if (index / 4).is_multiple_of(2) {
        0
    } else {
        u64::MAX
    };
    ChooseTreeItem {
        label: format!("row {index} 世界\0λ"),
        detail: format!("detail {index} 🦀\n"),
        target: match index % 4 {
            0 => ChooseTreeTarget::Session(SessionId(id)),
            1 => ChooseTreeTarget::Window(WindowId(id)),
            2 => ChooseTreeTarget::Pane(PaneId(id)),
            _ => ChooseTreeTarget::Client(ClientId(id)),
        },
        depth: index,
        flags: index,
        pane_kind: match index % 5 {
            0 => None,
            1 => Some(ChooseTreePaneKind::Terminal),
            2 => Some(ChooseTreePaneKind::Browser),
            3 => Some(ChooseTreePaneKind::Editor),
            _ => Some(ChooseTreePaneKind::Agent),
        },
        key: match index % 4 {
            0 => String::new(),
            1 => "M-a".to_owned(),
            2 => "é".repeat(32),
            _ => "0".to_owned(),
        },
        text: if index.is_multiple_of(3) {
            "override λ\0世界".to_owned()
        } else {
            String::new()
        },
    }
}

fn record(state: ChooseTreeState, marked: &str) -> Result<Value, Box<dyn Error>> {
    let (title, targets) = tree_title(state.kind);
    let expected = json!({
        "title":title,
        "subtitle":chooser_subtitle(format!("{} {targets}",state.items.len()),state.filter_no_matches),
        "height":chooser_modal_height(state.items.len(),state.help,!state.prompt.is_empty()).to_bits(),
        "gutter":chooser_has_key_gutter(state.items.iter().map(|item|item.key.as_str())),
        "selected":state.selected,
        "rows":state.items.iter().map(|item|{
            let (label,detail)=tree_row_text(item);
            json!([chooser_key_label(&item.key),label,detail,item.target.to_string(),item.depth,
                if item.has_children(){if item.expanded(){2}else{1}}else{0},item.active(),item.tagged(),item.pane_kind])
        }).collect::<Vec<_>>(),
        "search":state.search.as_ref().map(|search|json!([if search.reverse{"?"}else{"/"},format!("{}{marked}",search.query)])),
    });
    let frame = encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: u64::MAX,
        payload: EventPayload::ChooseTree { state: Some(state) },
    }))?;
    Ok(json!({"frame":frame,"marked":marked,"expected":expected}))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    for kind in [
        ChooseTreeKind::Windows,
        ChooseTreeKind::Panes,
        ChooseTreeKind::Clients,
    ] {
        for count in [0_u16, 1, 9, 10, 11, 100, 256] {
            for flags in 0_u8..8 {
                cases.push(record(
                    ChooseTreeState {
                        items: (0..count)
                            .map(|index| item(u8::try_from(index).unwrap_or(u8::MAX)))
                            .collect(),
                        search: (flags & 1 != 0).then(|| ChooseTreeSearchState {
                            query: "query 世界\0".to_owned(),
                            reverse: flags & 2 != 0,
                        }),
                        selected: if flags & 2 == 0 { 0 } else { u32::MAX },
                        kind,
                        filter_no_matches: flags & 4 != 0,
                        prompt: if flags & 2 == 0 {
                            String::new()
                        } else {
                            "prompt λ\0?".to_owned()
                        },
                        help: flags & 1 != 0,
                    },
                    "marked 🦀",
                )?);
            }
        }
    }
    for size in [65_536, 65_537] {
        let mut row = item(1);
        row.label = "x".repeat(size);
        row.detail = "y".repeat(size);
        "custom".clone_into(&mut row.text);
        cases.push(record(
            ChooseTreeState {
                items: vec![row],
                search: None,
                selected: 0,
                kind: ChooseTreeKind::Panes,
                filter_no_matches: false,
                prompt: String::new(),
                help: false,
            },
            "",
        )?);
    }
    let policies: Vec<_> = (0_u8..64)
        .map(|flags| {
            json!([
                flags,
                chooser_key_is_text(
                    flags & 1 != 0,
                    flags & 2 != 0,
                    Modifiers::new(
                        flags & 4 != 0,
                        flags & 8 != 0,
                        flags & 16 != 0,
                        flags & 32 != 0,
                    )
                )
            ])
        })
        .collect();
    serde_json::to_writer(
        io::stdout().lock(),
        &json!({"cases":cases,"policies":policies}),
    )?;
    Ok(())
}
