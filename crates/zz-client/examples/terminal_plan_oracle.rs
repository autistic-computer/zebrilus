use std::sync::Arc;

use serde_json::{Value, json};
use zz_terminal::{
    PackedCell, PatchError, SessionStatus, TerminalDictionaryPatch, TerminalPatchRows,
    TerminalPatchSpan, TerminalViewport, TerminalViewportPatch,
};

fn words(value: u64) -> [u32; 2] {
    [(value >> 32) as u32, value as u32]
}

fn blank(columns: u16, rows: u16) -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(columns, rows, SessionStatus::default());
    viewport.generation = 0xfedc_ba98_ffff_ffff;
    viewport.view_generation = 0x0123_4567_ffff_ffff;
    viewport.dictionary_generation = 91;
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
    patch: TerminalViewportPatch,
) {
    let mut header = Vec::new();
    header.extend(words(viewport.generation));
    header.extend(words(viewport.view_generation));
    header.extend([
        viewport.dictionary_generation,
        u32::from(viewport.columns),
        u32::from(viewport.rows),
        viewport.styles().len() as u32,
        viewport.grapheme_offsets().len().saturating_sub(1) as u32,
        viewport.grapheme_bytes().len() as u32,
    ]);
    header.extend(words(patch.base_generation));
    header.extend(words(patch.base_view_generation));
    header.extend(words(patch.generation));
    header.extend(words(patch.view_generation));
    header.extend([
        patch.dictionary_generation,
        u32::from(patch.columns),
        u32::from(patch.rows),
        patch.style_base,
        patch.dictionary.appended_styles().len() as u32,
        patch.grapheme_base,
        patch.dictionary.appended_grapheme_lengths().len() as u32,
        patch.dictionary.appended_grapheme_bytes().len() as u32,
        u32::from(patch.scroll > 0),
        u32::from(patch.scroll.unsigned_abs()),
        patch.changed_rows.cells().len() as u32,
    ]);
    let spans: Vec<_> = patch
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
    let mut actual = viewport.clone();
    let status = match actual.apply_patch(patch) {
        Ok(()) => 0,
        Err(PatchError::Generation) => 1,
        Err(PatchError::Dictionary) => 2,
        Err(PatchError::Dimensions) => 3,
        Err(PatchError::Row) => 4,
        Err(other) => panic!("unexpected error in structural case {name}: {other}"),
    };
    if status != 0 {
        assert_eq!(&actual, viewport, "rejection must be atomic for {name}");
    }
    let mut after = Vec::new();
    after.extend(words(actual.generation));
    after.extend(words(actual.view_generation));
    after.extend([
        actual.dictionary_generation,
        u32::from(actual.columns),
        u32::from(actual.rows),
        actual.styles().len() as u32,
        actual.grapheme_offsets().len().saturating_sub(1) as u32,
        actual.grapheme_bytes().len() as u32,
    ]);
    vectors.push(
        json!({"name": name, "header": header, "spans": spans, "status": status, "after": after}),
    );
}

fn main() {
    let mut vectors = Vec::new();
    for columns in [0, 1, 4, 9] {
        for rows in [0, 1, 2, 3, 5] {
            let viewport = blank(columns, rows);
            for scroll in -6_i16..=6 {
                for clear in [false, true] {
                    for length in [0, columns / 2, columns, columns.saturating_add(1)] {
                        let mut changed = patch(&viewport);
                        changed.scroll = scroll;
                        let spans: Vec<_> = (0..rows)
                            .map(|row| TerminalPatchSpan {
                                row,
                                start: 0,
                                len: length,
                                clear,
                            })
                            .collect();
                        changed.changed_rows = TerminalPatchRows::from_spans(
                            spans.into_iter().collect(),
                            vec![PackedCell::EMPTY; usize::from(length) * usize::from(rows)],
                        );
                        record(&mut vectors, "scroll-edge", &viewport, changed);
                    }
                }
            }
            let cases = [
                vec![],
                vec![(0, 0, 0)],
                vec![(rows, 0, 0)],
                vec![(0, columns, 0)],
                vec![(0, columns, 1)],
                vec![(0, columns.saturating_add(1), 0)],
                vec![(0, 0, columns), (0, 0, 0)],
                vec![(1, 0, 0), (0, 0, 0)],
                vec![(0, 0, columns), (2, 0, columns)],
            ];
            for spans in cases {
                for extra_cells in [0, 1] {
                    let mut changed = patch(&viewport);
                    let count = spans
                        .iter()
                        .map(|(_, _, length)| usize::from(*length))
                        .sum::<usize>();
                    changed.changed_rows = TerminalPatchRows::from_spans(
                        spans
                            .iter()
                            .map(|&(row, start, len)| TerminalPatchSpan {
                                row,
                                start,
                                len,
                                clear: false,
                            })
                            .collect(),
                        vec![PackedCell::EMPTY; count + extra_cells],
                    );
                    record(&mut vectors, "span-order-and-budget", &viewport, changed);
                }
            }
            for fault in 0..8 {
                let mut changed = patch(&viewport);
                match fault {
                    0 => changed.base_generation ^= 1 << 40,
                    1 => changed.base_view_generation ^= 1 << 40,
                    2 => changed.dictionary_generation += 1,
                    3 => changed.columns += 1,
                    4 => changed.rows += 1,
                    5 => {
                        changed.generation = 0;
                        changed.view_generation = u64::MAX;
                    }
                    6 => {
                        changed.style_base = u32::MAX;
                        changed.grapheme_base = u32::MAX;
                    }
                    _ => {
                        changed.base_generation ^= 1;
                        changed.dictionary_generation += 1;
                        changed.columns += 1;
                    }
                }
                record(&mut vectors, "generation-and-header", &viewport, changed);
            }
        }
    }
    let viewport = blank(4, 3);
    for added in [0, 1, 65534, 65535, 65536] {
        for base in [0, 1, 7] {
            let mut changed = patch(&viewport);
            changed.style_base = base;
            changed.dictionary = TerminalDictionaryPatch::from_parts(
                vec![viewport.styles()[0]; added],
                vec![],
                vec![],
            );
            record(&mut vectors, "dictionary-style-cap", &viewport, changed);
        }
    }
    for base in [0, 1, u32::MAX] {
        for styles in [0, 1] {
            let mut changed = patch(&viewport);
            changed.grapheme_base = base;
            changed.dictionary = TerminalDictionaryPatch::from_parts(
                vec![viewport.styles()[0]; styles],
                vec![1, 2],
                b"xyz".to_vec(),
            );
            record(&mut vectors, "dictionary-grapheme-base", &viewport, changed);
        }
    }
    for count in [65535, 65536] {
        let mut retained = viewport.clone();
        Arc::make_mut(&mut retained.dictionary).styles = vec![viewport.styles()[0]; count].into();
        for added in [0, 1] {
            let mut changed = patch(&retained);
            changed.dictionary = TerminalDictionaryPatch::from_parts(
                vec![viewport.styles()[0]; added],
                vec![],
                vec![],
            );
            record(&mut vectors, "dictionary-retained-cap", &retained, changed);
        }
    }
    let viewport = blank(1, u16::MAX);
    for scroll in [i16::MIN, i16::MAX] {
        let mut changed = patch(&viewport);
        changed.scroll = scroll;
        changed.changed_rows = TerminalPatchRows::from_spans(
            (0..u16::MAX)
                .map(|row| TerminalPatchSpan {
                    row,
                    start: 0,
                    len: 0,
                    clear: true,
                })
                .collect(),
            vec![],
        );
        record(&mut vectors, "maximum-span-count", &viewport, changed);
    }
    println!(
        "{}",
        json!({"reference": "zz_terminal::TerminalViewport::apply_patch", "vectors": vectors})
    );
}
