use std::{error::Error, sync::Arc};

use serde_json::{Value, json};
use zz_protocol::{
    Event, EventPayload, PaneId, ProtocolMessage, decode_protocol_frame, encode_protocol_message,
};
use zz_terminal::{
    CellWidth, Color, ColourClass, Cursor, CursorStyle, GRAPHEME_TABLE_BIT, KittyLayer,
    KittyPlacement, OverlayKind, OverlaySpan, PackedCell, PackedStyle, ScrollbarState,
    SearchStatus, SessionStatus, TerminalDictionary, TerminalMode, TerminalPresentation,
    TerminalViewport, UnderlineStyle,
};

fn status(status: &SessionStatus) -> Value {
    match status {
        SessionStatus::Starting => {
            json!({"kind": 0, "exit_code": 0, "signal": null, "failure": ""})
        }
        SessionStatus::Running => json!({"kind": 1, "exit_code": 0, "signal": null, "failure": ""}),
        SessionStatus::Exited(exit) => {
            json!({"kind": 2, "exit_code": exit.code, "signal": exit.signal, "failure": ""})
        }
        SessionStatus::Failed(error) => {
            json!({"kind": 3, "exit_code": 0, "signal": null, "failure": error})
        }
    }
}

fn viewport(viewport: &TerminalViewport) -> Value {
    let mode = match viewport.mode {
        TerminalMode::Live => json!([0, 0, 0, false]),
        TerminalMode::Copy {
            position,
            total,
            hide_position,
        } => json!([1, position, total, hide_position]),
        TerminalMode::View { position, total } => json!([2, position, total, false]),
    };
    let cursor = viewport.cursor.map(|cursor| {
        json!([
            cursor.color().packed(),
            cursor.column(),
            cursor.row(),
            cursor.style() as u8,
            u8::from(cursor.visible()),
            u8::from(cursor.blinking()),
            u8::from(cursor.at_wide_tail())
        ])
    });
    let search = viewport.search.map(|search| {
        json!([
            search.current(),
            search.total,
            search.pending(),
            search.invalid_pattern()
        ])
    });
    let kitty = viewport.kitty_placements.iter().map(|placement| json!({
        "image_id": placement.image_id, "image_generation": placement.image_generation.to_string(),
        "layer": placement.layer as u8, "viewport_column": placement.viewport_col,
        "viewport_row": placement.viewport_row, "absolute_row": placement.absolute_row.to_string(),
        "cell_offset_x": placement.cell_offset_x, "cell_offset_y": placement.cell_offset_y,
        "grid_columns": placement.grid_cols, "grid_rows": placement.grid_rows,
        "pixel_width": placement.pixel_width, "pixel_height": placement.pixel_height,
        "source_rect": placement.source_rect,
    })).collect::<Vec<_>>();
    json!({
        "generation": viewport.generation.to_string(), "view_generation": viewport.view_generation.to_string(),
        "dictionary_generation": viewport.dictionary_generation, "columns": viewport.columns, "rows": viewport.rows,
        "cells": viewport.cells, "styles": viewport.styles(), "grapheme_offsets": viewport.grapheme_offsets(),
        "grapheme_bytes": viewport.grapheme_bytes(),
        "metadata": {
            "foreground": viewport.foreground.packed(), "background": viewport.background.packed(),
            "title": viewport.title(), "working_directory": viewport.working_directory(),
            "hovered_uri": viewport.hovered_uri(), "cursor": cursor,
            "scrollbar": [viewport.scrollbar.total, viewport.scrollbar.offset, viewport.scrollbar.len],
            "overlays": viewport.overlays, "mode": mode, "search": search,
            "unseen_output": viewport.unseen_output, "kitty_keyboard": viewport.kitty_keyboard,
            "mouse_tracking": viewport.mouse_tracking, "status": status(&viewport.status), "kitty": kitty,
        }
    })
}

fn frame(sequence: u64, payload: EventPayload) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence,
        payload,
    }))?)
}

fn base_viewport() -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(8, 4, SessionStatus::Running);
    viewport.generation = 17;
    viewport.view_generation = 21;
    viewport.dictionary_generation = 9;
    viewport.foreground = Color::rgb(233, 231, 227);
    viewport.background = Color::rgb(17, 34, 51);
    let styles = vec![
        PackedStyle::new(
            viewport.foreground,
            viewport.background,
            None,
            0,
            UnderlineStyle::None,
        ),
        PackedStyle::new(
            Color::rgb(254, 18, 35),
            Color::rgb(2, 7, 11),
            Some(Color::rgb(33, 44, 55)),
            0x1ff,
            UnderlineStyle::Curly,
        )
        .with_classes(ColourClass::Palette(201), ColourClass::Default),
    ];
    let first = "e\u{301}";
    let second = "👩\u{200d}💻";
    let graphemes = format!("{first}{second}");
    viewport.dictionary = Arc::new(TerminalDictionary::from_shared(
        styles.into(),
        vec![
            0,
            u32::try_from(first.len()).expect("short fixture"),
            u32::try_from(graphemes.len()).expect("short fixture"),
        ]
        .into(),
        graphemes.into_bytes().into(),
    ));
    let cells = Arc::make_mut(&mut viewport.cells);
    for (column, character) in "ABCDEFGH".chars().enumerate() {
        cells[column] = PackedCell::new(u32::from(character), 0, CellWidth::Narrow);
    }
    cells[8] = PackedCell::new(u32::from('界'), 1, CellWidth::Wide);
    cells[9] = PackedCell::new(0, 1, CellWidth::SpacerTail);
    cells[10] = PackedCell::new(u32::from('é'), 1, CellWidth::Narrow);
    for cell in &mut cells[16..24] {
        *cell = PackedCell::new(u32::from('x'), 1, CellWidth::Narrow);
    }
    cells[24] = PackedCell::new(GRAPHEME_TABLE_BIT, 1, CellWidth::Narrow);
    cells[25] = PackedCell::from_raw(GRAPHEME_TABLE_BIT | 1, 0, 0x1230);
    cells[31] = PackedCell::new(0, 0, CellWidth::SpacerHead);
    viewport.presentation = Arc::new(TerminalPresentation::new(
        Arc::from("native ✓\0title"),
        Some(Arc::from("/tmp/世界")),
        Some(Arc::from("https://example.org/é")),
    ));
    viewport.cursor = Some(Cursor::new(
        1,
        0,
        true,
        true,
        false,
        CursorStyle::Block,
        Color::rgb(90, 80, 70),
    ));
    viewport.scrollbar = ScrollbarState {
        total: 100,
        offset: 5,
        len: 4,
    };
    viewport.overlays = Arc::from([
        OverlaySpan::new(0, 0, 4, OverlayKind::Selection),
        OverlaySpan::with_flags(1, 0, 2, OverlayKind::SearchMatch, 1),
    ]);
    viewport.mode = TerminalMode::Copy {
        position: 2,
        total: 50,
        hide_position: false,
    };
    viewport.search = Some(SearchStatus::new(2, 5).with_pending(true));
    viewport.unseen_output = 9;
    viewport.kitty_keyboard = true;
    viewport.mouse_tracking = true;
    viewport.kitty_placements = Arc::from([KittyPlacement {
        image_id: 42,
        image_generation: 99,
        layer: KittyLayer::BelowText,
        viewport_col: -1,
        viewport_row: 1,
        absolute_row: 6,
        cell_offset_x: 2,
        cell_offset_y: 3,
        grid_cols: 3,
        grid_rows: 2,
        pixel_width: 64,
        pixel_height: 32,
        source_rect: Some((0, 0, 32, 16)),
    }]);
    viewport
}

fn main() -> Result<(), Box<dyn Error>> {
    let base = base_viewport();
    let mut full = Vec::new();
    for variant in 0..6_u8 {
        let mut current = base.clone();
        let underline = match variant {
            0 => UnderlineStyle::None,
            1 => UnderlineStyle::Single,
            2 => UnderlineStyle::Double,
            3 => UnderlineStyle::Curly,
            4 => UnderlineStyle::Dotted,
            _ => UnderlineStyle::Dashed,
        };
        let (foreground_class, background_class) = match variant {
            0 => (ColourClass::Resolved, ColourClass::Resolved),
            1 => (ColourClass::Default, ColourClass::Default),
            2 => (ColourClass::Rgb, ColourClass::Rgb),
            3 => (ColourClass::Palette(0), ColourClass::Palette(255)),
            4 => (ColourClass::Palette(255), ColourClass::Default),
            _ => (ColourClass::Default, ColourClass::Palette(19)),
        };
        let mut styles = current.styles().to_vec();
        styles[1] = PackedStyle::new(
            Color::rgb(254, 18, 35),
            Color::rgb(2, 7, 11),
            (variant != 0).then_some(Color::rgb(33, 44, 55)),
            1 << variant,
            underline,
        )
        .with_classes(foreground_class, background_class);
        current.dictionary = Arc::new(TerminalDictionary::from_shared(
            styles.into(),
            current.grapheme_offsets().into(),
            current.grapheme_bytes().into(),
        ));
        let placement = &mut Arc::make_mut(&mut current.kitty_placements)[0];
        placement.layer = match variant % 3 {
            0 => KittyLayer::BelowBg,
            1 => KittyLayer::BelowText,
            _ => KittyLayer::AboveText,
        };
        if variant == 0 {
            placement.source_rect = None;
        }
        let style = match variant % 4 {
            0 => CursorStyle::Bar,
            1 => CursorStyle::Block,
            2 => CursorStyle::Underline,
            _ => CursorStyle::BlockHollow,
        };
        current.cursor = Some(Cursor::new(
            u16::from(variant),
            u16::from(variant % 4),
            variant.is_multiple_of(2),
            variant.is_multiple_of(3),
            variant == 4,
            style,
            Color::rgb(1, 2, 3),
        ));
        current.status = match variant % 4 {
            0 => SessionStatus::Starting,
            1 => SessionStatus::Running,
            2 => SessionStatus::exited(u32::MAX, Some("SIGTERM 世界".to_owned())),
            _ => SessionStatus::failed("bad\0status 世界"),
        };
        if variant == 0 {
            current.cursor = None;
            current.mode = TerminalMode::Live;
            current.search = None;
        } else if variant == 1 {
            current.mode = TerminalMode::Copy {
                position: 1,
                total: u32::MAX,
                hide_position: true,
            };
        } else if variant == 5 {
            current.mode = TerminalMode::View {
                position: 3,
                total: 7,
            };
            current.search = Some(SearchStatus::new(0, 0).with_invalid_pattern(true));
            current.generation = u64::MAX;
            current.view_generation = 0;
            current.dictionary_generation = u32::MAX;
        } else if variant == 4 {
            current.search = Some(SearchStatus::new(u32::MAX, u32::MAX));
        }
        let pane = if variant == 5 {
            u64::MAX
        } else {
            u64::from(variant)
        };
        let bytes = frame(
            u64::MAX - u64::from(variant),
            EventPayload::TerminalViewport {
                pane: PaneId(pane),
                viewport: current.clone(),
            },
        )?;
        full.push(json!({"bytes": bytes, "pane": pane.to_string(), "sequence": (u64::MAX - u64::from(variant)).to_string(), "viewport": viewport(&current)}));
    }
    let base_bytes = frame(
        100,
        EventPayload::TerminalViewport {
            pane: PaneId(7),
            viewport: base.clone(),
        },
    )?;
    let mut next = base.clone();
    next.generation += 1;
    next.view_generation += 1;
    next.presentation = Arc::new(TerminalPresentation::new(
        Arc::from("changed"),
        None,
        Some(Arc::from("")),
    ));
    next.cursor = Some(Cursor::new(
        4,
        1,
        true,
        true,
        true,
        CursorStyle::Block,
        Color::rgb(90, 80, 70),
    ));
    let mut styles = next.styles().to_vec();
    styles.push(
        PackedStyle::new(
            Color::rgb(1, 2, 3),
            Color::rgb(4, 5, 6),
            None,
            2,
            UnderlineStyle::Dashed,
        )
        .with_classes(ColourClass::Default, ColourClass::Palette(255)),
    );
    let mut offsets = next.grapheme_offsets().to_vec();
    let mut graphemes = next.grapheme_bytes().to_vec();
    graphemes.extend_from_slice("a\u{308}".as_bytes());
    offsets.push(u32::try_from(graphemes.len())?);
    next.dictionary = Arc::new(TerminalDictionary::from_shared(
        styles.into(),
        offsets.into(),
        graphemes.into(),
    ));
    Arc::make_mut(&mut next.cells)[0] =
        PackedCell::new(GRAPHEME_TABLE_BIT | 2, 2, CellWidth::Narrow);
    let patch = TerminalViewport::diff(&base, &next).ok_or("expected changed fixture")?;
    let patch_bytes = frame(
        101,
        EventPayload::TerminalPatch {
            pane: PaneId(7),
            patch: patch.clone(),
        },
    )?;
    let mut applied_viewport = base.clone();
    applied_viewport.apply_patch(patch.clone())?;
    let mut patches = vec![
        json!({"base": base_bytes, "bytes": patch_bytes, "scroll": patch.scroll, "viewport": viewport(&applied_viewport)}),
    ];
    let mut rejected_patches = Vec::new();
    for variant in 0..6 {
        let mut invalid = patch.clone();
        match variant {
            0 => invalid.base_generation += 1,
            1 => invalid.base_view_generation += 1,
            2 => invalid.dictionary_generation += 1,
            3 => invalid.columns += 1,
            4 => invalid.style_base += 1,
            _ => invalid.grapheme_base += 1,
        }
        let bytes = frame(
            103,
            EventPayload::TerminalPatch {
                pane: PaneId(7),
                patch: invalid.clone(),
            },
        )?;
        let mut retained = base.clone();
        let error = retained
            .apply_patch(invalid)
            .expect_err("fixture must reject the mismatched patch");
        rejected_patches.push(json!({"base": base_bytes, "bytes": bytes, "error": error.to_string(), "unchanged": viewport(&retained)}));
    }
    let mut scrolled = next.clone();
    scrolled.generation += 1;
    scrolled.view_generation += 1;
    let cells = Arc::make_mut(&mut scrolled.cells);
    cells.copy_within(8..32, 0);
    cells[24..32].fill(PackedCell::new(u32::from('z'), 0, CellWidth::Narrow));
    let scroll_patch = TerminalViewport::diff(&next, &scrolled).ok_or("expected scroll fixture")?;
    let scroll = scroll_patch.scroll;
    let previous_bytes = frame(
        101,
        EventPayload::TerminalViewport {
            pane: PaneId(7),
            viewport: next.clone(),
        },
    )?;
    let scroll_bytes = frame(
        102,
        EventPayload::TerminalPatch {
            pane: PaneId(7),
            patch: scroll_patch,
        },
    )?;
    patches.push(
        json!({"base": previous_bytes, "bytes": scroll_bytes, "scroll": scroll, "viewport": viewport(&scrolled)}),
    );
    let output = frame(
        200,
        EventPayload::CommandOutput {
            pane: PaneId(7),
            output_id: u64::MAX,
            viewport: Some(base.clone()),
        },
    )?;
    let history = frame(
        201,
        EventPayload::HistoryChunk {
            pane: PaneId(7),
            start: 5,
            total: 100,
            offset: 5,
            columns: base.columns,
            rows: vec![base.cells[..8].to_vec(), base.cells[8..16].to_vec()],
            dictionary: (*base.dictionary).clone(),
        },
    )?;
    let mut malformed = Vec::new();
    for index in (8..base_bytes.len()).step_by(7) {
        for value in [0_u8, 0x80, 0xff] {
            let mut bytes = base_bytes.clone();
            bytes[index] = value;
            let decoded = decode_protocol_frame(&bytes);
            let expected = match &decoded {
                Ok(ProtocolMessage::Event(Event {
                    sequence,
                    payload:
                        EventPayload::TerminalViewport {
                            pane,
                            viewport: view,
                        },
                })) => Some(
                    json!({"pane": pane.0.to_string(), "sequence": sequence.to_string(), "viewport": viewport(view)}),
                ),
                _ => None,
            };
            malformed
                .push(json!({"bytes": bytes, "accepted": decoded.is_ok(), "expected": expected}));
        }
    }
    let search_frame = full[4]["bytes"]
        .as_array()
        .ok_or("fixture frame must be an array")?;
    let mut bytes = search_frame
        .iter()
        .map(|byte| u8::try_from(byte.as_u64().expect("fixture byte")).expect("fixture byte fits"))
        .collect::<Vec<_>>();
    let position = bytes
        .windows(11)
        .position(|bytes| bytes == [1, 255, 255, 255, 255, 1, 255, 255, 255, 255, 15])
        .ok_or("search limit varint missing")?;
    bytes[position + 5] = 15;
    let expected = match decode_protocol_frame(&bytes)? {
        ProtocolMessage::Event(Event {
            sequence,
            payload:
                EventPayload::TerminalViewport {
                    pane,
                    viewport: view,
                },
        }) => {
            json!({"pane": pane.0.to_string(), "sequence": sequence.to_string(), "viewport": viewport(&view)})
        }
        _ => return Err("expected decoded search viewport".into()),
    };
    malformed.push(json!({"bytes": bytes, "accepted": true, "expected": expected}));
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "full": full, "patches": patches, "rejected_patches": rejected_patches, "command_output": {"bytes": output, "output_id": u64::MAX.to_string(), "viewport": viewport(&base)},
            "history": {"bytes": history, "columns": 8, "rows": 2, "cells": &base.cells[..16], "styles": base.styles(), "grapheme_offsets": base.grapheme_offsets(), "grapheme_bytes": base.grapheme_bytes()},
            "mutations": malformed,
        }))?
    );
    Ok(())
}
