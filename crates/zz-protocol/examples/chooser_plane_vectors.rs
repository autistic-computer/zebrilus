use std::{error::Error, fmt::Write as _, sync::Arc};

use serde_json::{Value, json};
use zz_protocol::{
    ChooserPresentation, ChooserPreview, ChooserPreviewSize, ChooserPreviewTile, ChooserRow, Event,
    EventPayload, PromptCursor, ProtocolMessage, TmuxColour, decode_protocol_frame,
    encode_protocol_message,
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

fn preview_value(preview: &ChooserPreview) -> Value {
    match preview {
        ChooserPreview::Tiles { tiles, current } => json!({
            "kind":0,"current":current,
            "tiles":tiles.iter().map(|tile|json!({"label":tile.label,"label_style":tile.label_style,"border_style":tile.border_style,"viewport":tile.viewport.as_ref().map(viewport)})).collect::<Vec<_>>()
        }),
        ChooserPreview::Screen { viewport: screen } => {
            json!({"kind":1,"viewport":viewport(screen)})
        }
        ChooserPreview::Text { lines } => json!({"kind":2,"lines":lines}),
        ChooserPreview::Client {
            viewport: screen,
            status,
            status_style,
            status_width,
            border_style,
        } => {
            json!({"kind":3,"viewport":screen.as_ref().map(viewport),"lines":status,"status_style":status_style,"status_width":status_width,"border_style":border_style})
        }
        ChooserPreview::Markup { lines } => json!({"kind":4,"lines":lines}),
    }
}

fn plane_value(plane: &ChooserPresentation) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(plane)?;
    value["preview"] = plane.preview.as_ref().map_or(Value::Null, preview_value);
    Ok(value)
}

fn hex(bytes: &[u8]) -> Result<String, std::fmt::Error> {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}")?;
    }
    Ok(output)
}

fn frame(sequence: u64, plane: Option<ChooserPresentation>) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence,
        payload: EventPayload::ChooserPresentation {
            presentation: plane.map(Box::new),
        },
    }))?)
}

fn record(
    cases: &mut Vec<Value>,
    name: &str,
    mut bytes: Vec<u8>,
    refusal: bool,
) -> Result<(), Box<dyn Error>> {
    if bytes.len() >= 4 {
        let size = u32::try_from(bytes.len() - 4)?;
        bytes[..4].copy_from_slice(&size.to_le_bytes());
    }
    let decoded = decode_protocol_frame(&bytes).ok();
    let mut sequence = 0;
    let mut expected = Value::Null;
    let status = if refusal || decoded.is_none() {
        0
    } else if let Some(ProtocolMessage::Event(Event {
        sequence: current,
        payload: EventPayload::ChooserPresentation { presentation },
    })) = decoded.as_ref()
    {
        sequence = *current;
        expected = presentation
            .as_deref()
            .map(plane_value)
            .transpose()?
            .unwrap_or(Value::Null);
        1
    } else {
        2
    };
    cases.push(json!({"name":name,"bytes":hex(&bytes)?,"status":status,"sequence":sequence.to_string(),"expected":expected,"refusal":refusal}));
    Ok(())
}

fn base_plane() -> ChooserPresentation {
    ChooserPresentation {
        selected: 0,
        rows: vec![
            ChooserRow {
                name: "λ\0世界".into(),
                text: "#[fg=red]row\n😀".into(),
                align: true,
            },
            ChooserRow::default(),
        ],
        sort: "size".into(),
        view: "preview".into(),
        filter: true,
        selection_style: "fg=colour201,bg=default,bold".into(),
        border_style: "fg=#123456".into(),
        prompt_style: "reverse".into(),
        preview_size: ChooserPreviewSize::Normal,
        preview: None,
        prompt_cursor: PromptCursor {
            style: 255,
            colour: Some(TmuxColour::Theme(255)),
        },
        prompt_column: u16::MAX,
        prompt_line: "filter: λ\0世界".into(),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut cases = Vec::new();
    let viewport = base_viewport();
    let previews = [
        None,
        Some(ChooserPreview::Tiles {
            tiles: vec![
                ChooserPreviewTile {
                    label: "current\0tile".into(),
                    label_style: "bold".into(),
                    border_style: "double".into(),
                    viewport: Some(viewport.clone()),
                },
                ChooserPreviewTile {
                    label: "empty".into(),
                    label_style: String::new(),
                    border_style: String::new(),
                    viewport: None,
                },
            ],
            current: u32::MAX,
        }),
        Some(ChooserPreview::Screen {
            viewport: viewport.clone(),
        }),
        Some(ChooserPreview::Text {
            lines: vec![String::new(), "λ\0世界\n🦀".into()],
        }),
        Some(ChooserPreview::Client {
            viewport: Some(viewport.clone()),
            status: vec!["#[bg=red]λ".into(), String::new()],
            status_style: "fg=terminal".into(),
            status_width: u32::MAX,
            border_style: "double".into(),
        }),
        Some(ChooserPreview::Client {
            viewport: None,
            status: vec![],
            status_style: String::new(),
            status_width: 0,
            border_style: String::new(),
        }),
        Some(ChooserPreview::Markup {
            lines: vec!["#[fg=red]λ".into(), "#[default]x".into()],
        }),
    ];
    for sequence in [0, 128, u64::MAX] {
        record(&mut cases, "absent", frame(sequence, None)?, false)?;
        for preview in &previews {
            for selected in [0, 127, u32::MAX] {
                for size in [
                    ChooserPreviewSize::Normal,
                    ChooserPreviewSize::Off,
                    ChooserPreviewSize::Big,
                ] {
                    let mut plane = base_plane();
                    plane.selected = selected;
                    plane.preview_size = size;
                    plane.preview.clone_from(preview);
                    record(
                        &mut cases,
                        "typed-preview",
                        frame(sequence, Some(plane))?,
                        false,
                    )?;
                }
            }
        }
    }
    for colour in [
        None,
        Some(TmuxColour::Basic(255)),
        Some(TmuxColour::Indexed(255)),
        Some(TmuxColour::Rgb(0x00ff_ffff)),
        Some(TmuxColour::Default),
        Some(TmuxColour::Terminal),
        Some(TmuxColour::Theme(255)),
    ] {
        for style in [0, 1, 255] {
            let mut plane = base_plane();
            plane.prompt_cursor = PromptCursor { style, colour };
            record(&mut cases, "cursor-colour", frame(128, Some(plane))?, false)?;
        }
    }
    let base = frame(0, Some(base_plane()))?;
    for length in 8..base.len() {
        record(&mut cases, "truncated", base[..length].to_vec(), false)?;
    }
    for offset in 11..base.len() {
        for value in [0, 2, 127, 255] {
            let mut bytes = base.clone();
            bytes[offset] = value;
            record(&mut cases, "mutated", bytes, false)?;
        }
    }
    for preview in &previews {
        let mut plane = base_plane();
        plane.preview.clone_from(preview);
        let mut bytes = frame(128, Some(plane))?;
        bytes.extend([0, 255, 127]);
        record(&mut cases, "outer-trailing", bytes, false)?;
    }
    let mut screen = base_plane();
    screen.preview = Some(ChooserPreview::Screen {
        viewport: viewport.clone(),
    });
    let screen = frame(0, Some(screen))?;
    for offset in 11..screen.len() {
        for value in [0, 255] {
            let mut bytes = screen.clone();
            bytes[offset] = value;
            record(&mut cases, "nested-viewport-mutation", bytes, false)?;
        }
    }
    record(
        &mut cases,
        "unrelated-message",
        encode_protocol_message(&ProtocolMessage::TreeSync)?,
        false,
    )?;
    record(
        &mut cases,
        "unrelated-event",
        encode_protocol_message(&ProtocolMessage::Event(Event {
            sequence: 1,
            payload: EventPayload::ServerStopping,
        }))?,
        false,
    )?;
    for count in [65536, 65537] {
        let mut plane = base_plane();
        plane.rows = vec![ChooserRow::default(); count];
        record(
            &mut cases,
            "row-bound",
            frame(0, Some(plane))?,
            count > 65536,
        )?;
    }
    for count in [1024 * 1024, 1024 * 1024 + 1] {
        let mut plane = base_plane();
        plane.prompt_line = "x".repeat(count);
        record(
            &mut cases,
            "text-bound",
            frame(0, Some(plane))?,
            count > 1024 * 1024,
        )?;
    }
    for count in [256, 257] {
        let mut plane = base_plane();
        plane.preview = Some(ChooserPreview::Tiles {
            tiles: vec![
                ChooserPreviewTile {
                    label: String::new(),
                    label_style: String::new(),
                    border_style: String::new(),
                    viewport: Some(viewport.clone())
                };
                count
            ],
            current: 0,
        });
        record(
            &mut cases,
            "viewport-count-bound",
            frame(0, Some(plane))?,
            count > 256,
        )?;
    }
    let mut plane = base_plane();
    let large = TerminalViewport::blank(2048, 2048, SessionStatus::Running);
    plane.preview = Some(ChooserPreview::Tiles {
        tiles: vec![
            ChooserPreviewTile {
                label: String::new(),
                label_style: String::new(),
                border_style: String::new(),
                viewport: Some(large)
            };
            3
        ],
        current: 0,
    });
    record(
        &mut cases,
        "expanded-cells-bound",
        frame(0, Some(plane))?,
        true,
    )?;
    let mut plane = base_plane();
    plane.preview = Some(ChooserPreview::Text {
        lines: vec!["x".repeat(1024 * 1024); 33],
    });
    record(
        &mut cases,
        "aggregate-text-bound",
        frame(0, Some(plane))?,
        true,
    )?;
    let path = std::env::args().nth(1).ok_or("missing output path")?;
    std::fs::write(path, serde_json::to_vec(&cases)?)?;
    Ok(())
}
