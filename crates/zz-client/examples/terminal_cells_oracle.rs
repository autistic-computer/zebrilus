use std::sync::Arc;

use serde_json::{Value, json};
use zz_terminal::{
    PackedCell, PatchError, SessionStatus, TerminalPatchRows, TerminalPatchSpan, TerminalViewport,
    TerminalViewportPatch,
};

fn words(value: u64) -> [u32; 2] {
    [(value >> 32) as u32, value as u32]
}

fn cells(values: &[PackedCell]) -> Vec<[u32; 2]> {
    values
        .iter()
        .map(|cell| {
            [
                cell.glyph(),
                u32::from(cell.style_id()) | (u32::from(cell.flags()) << 16),
            ]
        })
        .collect()
}

fn retained(columns: u16, rows: u16) -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(columns, rows, SessionStatus::default());
    viewport.generation = 0xfedc_ba98_ffff_ffff;
    viewport.view_generation = 0x0123_4567_ffff_ffff;
    viewport.dictionary_generation = 91;
    let styles = vec![viewport.styles()[0]; 4];
    Arc::make_mut(&mut viewport.dictionary).styles = styles.into();
    viewport.cells = (0..usize::from(columns) * usize::from(rows))
        .map(|index| {
            PackedCell::from_raw(
                32 + (index % 90) as u32,
                (index % 4) as u16,
                (index as u16).wrapping_mul(73) ^ 0xa53c,
            )
        })
        .collect();
    viewport
}

fn patch(viewport: &TerminalViewport) -> TerminalViewportPatch {
    let mut next = viewport.clone();
    next.generation += 1;
    next.view_generation += 1;
    TerminalViewport::diff(viewport, &next).expect("matching viewport shapes produce a patch")
}

fn record(
    vectors: &mut Vec<Value>,
    name: &str,
    viewport: &TerminalViewport,
    changed: TerminalViewportPatch,
) {
    let mut header = Vec::new();
    header.extend(words(viewport.generation));
    header.extend(words(viewport.view_generation));
    header.extend([
        viewport.dictionary_generation,
        u32::from(viewport.columns),
        u32::from(viewport.rows),
        viewport.styles().len() as u32,
        0,
        0,
    ]);
    header.extend(words(changed.base_generation));
    header.extend(words(changed.base_view_generation));
    header.extend(words(changed.generation));
    header.extend(words(changed.view_generation));
    header.extend([
        changed.dictionary_generation,
        u32::from(changed.columns),
        u32::from(changed.rows),
        changed.style_base,
        0,
        changed.grapheme_base,
        0,
        0,
        u32::from(changed.scroll > 0),
        u32::from(changed.scroll.unsigned_abs()),
        changed.changed_rows.cells().len() as u32,
    ]);
    let spans: Vec<_> = changed
        .changed_rows
        .spans()
        .iter()
        .map(|span| {
            [
                u32::from(span.row),
                u32::from(span.start),
                u32::from(span.len),
                u32::from(span.clear),
            ]
        })
        .collect();
    let payload = cells(changed.changed_rows.cells());
    let mut actual = viewport.clone();
    let status = match actual.apply_patch(changed) {
        Ok(()) => 0,
        Err(PatchError::Generation) => 1,
        Err(PatchError::Dictionary) => 2,
        Err(PatchError::Dimensions) => 3,
        Err(PatchError::Row) => 4,
        Err(other) => panic!("unexpected error in cell case {name}: {other}"),
    };
    if status != 0 {
        assert_eq!(&actual, viewport, "rejection must be atomic for {name}");
    }
    vectors.push(json!({"name": name, "header": header, "spans": spans, "current": cells(&viewport.cells), "changed": payload, "after": cells(&actual.cells), "status": status}));
}

fn scenario(vectors: &mut Vec<Value>, columns: u16, rows: u16, scroll: i16, mode: u32) {
    let viewport = retained(columns, rows);
    let mut changed = patch(&viewport);
    changed.scroll = scroll;
    let exposed = scroll.unsigned_abs();
    let mut spans = Vec::new();
    for row in 0..rows {
        let span = match mode {
            0 => Some(TerminalPatchSpan {
                row,
                start: 0,
                len: 0,
                clear: true,
            }),
            1 => Some(TerminalPatchSpan {
                row,
                start: 0,
                len: columns,
                clear: false,
            }),
            3 if (scroll > 0 && row < exposed)
                || (scroll < 0 && row >= rows.saturating_sub(exposed)) =>
            {
                Some(TerminalPatchSpan {
                    row,
                    start: 0,
                    len: columns,
                    clear: false,
                })
            }
            4 if row == rows / 2 => Some(TerminalPatchSpan {
                row,
                start: columns / 3,
                len: columns / 3,
                clear: false,
            }),
            5 => Some(TerminalPatchSpan {
                row,
                start: 0,
                len: row % (columns.saturating_add(1)),
                clear: true,
            }),
            _ => None,
        };
        spans.extend(span);
    }
    let count = spans.iter().map(|span| usize::from(span.len)).sum();
    let payload = (0..count)
        .map(|index| {
            PackedCell::from_raw(
                0x400 + (index % 400) as u32,
                3 - (index % 4) as u16,
                0xbeef_u16.wrapping_sub(index as u16),
            )
        })
        .collect();
    changed.changed_rows = TerminalPatchRows::from_spans(spans.into_iter().collect(), payload);
    record(vectors, "shift-replace-clear", &viewport, changed);
}

fn main() {
    let mut vectors = Vec::new();
    for columns in [0, 1, 4, 9] {
        for rows in [0, 1, 2, 3, 5] {
            for scroll in -6..=6 {
                for mode in 0..6 {
                    scenario(&mut vectors, columns, rows, scroll, mode);
                }
            }
            let viewport = retained(columns, rows);
            for fault in 0..5 {
                let mut changed = patch(&viewport);
                match fault {
                    0 => changed.base_generation ^= 1 << 40,
                    1 => changed.base_view_generation ^= 1 << 40,
                    2 => changed.dictionary_generation += 1,
                    3 => changed.columns += 1,
                    _ => {
                        changed.changed_rows = TerminalPatchRows::from_spans(
                            [
                                TerminalPatchSpan {
                                    row: 0,
                                    start: 0,
                                    len: 0,
                                    clear: true,
                                },
                                TerminalPatchSpan {
                                    row: 0,
                                    start: 0,
                                    len: 0,
                                    clear: true,
                                },
                            ]
                            .into_iter()
                            .collect(),
                            vec![],
                        );
                    }
                }
                record(&mut vectors, "rejected-edit", &viewport, changed);
            }
        }
    }
    for (columns, rows) in [(80, 24), (200, 80), (400, 120)] {
        for scroll in [-3, -1, 0, 1, 3] {
            for mode in [3, 4, 5] {
                scenario(&mut vectors, columns, rows, scroll, mode);
            }
        }
    }
    println!(
        "{}",
        json!({"reference": "zz_terminal::TerminalViewport::apply_patch.cells", "vectors": vectors})
    );
}
