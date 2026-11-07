use std::{collections::BTreeMap, sync::Arc};

use serde_json::Value;
use zz_client::scrollback::{
    HistoryRing, RetainedTerminalViewport, apply_history_chunk, apply_retained_patch,
    new_retained_viewport, replace_retained_viewport,
};
use zz_protocol::{
    Event, EventPayload, PaneId, ProtocolMessage, decode_protocol_frame, encode_protocol_message,
    encode_terminal_viewport_event,
};
use zz_terminal::{
    CellWidth, Color, GRAPHEME_TABLE_BIT, PackedCell, PackedStyle, ScrollbarState, SessionStatus,
    TerminalDictionary, TerminalViewport, UnderlineStyle,
};

use super::{record, uint};

fn blob(output: &mut Vec<u8>, bytes: &[u8]) {
    uint(output, bytes.len() as u64);
    output.extend_from_slice(bytes);
}

fn style_fields(style: PackedStyle) -> [u64; 6] {
    let classes = style.class_word();
    [
        u64::from(style.foreground_raw() | (classes >> 8 & 255) << 24),
        u64::from(style.background_raw()),
        u64::from(style.underline_color_raw()),
        u64::from(style.attributes() | ((classes & 15) as u16) << 12),
        u64::from(style.underline_kind_raw()),
        u64::from(classes >> 16 & 255),
    ]
}

fn packed(output: &mut Vec<u8>, cells: &[PackedCell]) {
    uint(output, cells.len() as u64);
    for cell in cells {
        uint(output, u64::from(cell.glyph()));
        uint(output, u64::from(cell.style_id()));
        uint(output, u64::from(cell.flags()));
    }
}

#[derive(Default)]
struct Planes {
    style_keys: BTreeMap<usize, usize>,
    styles: Vec<Arc<[PackedStyle]>>,
    grapheme_keys: BTreeMap<(usize, usize), usize>,
    graphemes: Vec<(Arc<[u32]>, Arc<[u8]>)>,
}

impl Planes {
    fn dictionary(&mut self, dictionary: &TerminalDictionary) -> (usize, usize) {
        let key = dictionary.styles.as_ptr() as usize;
        let styles = if let Some(index) = self.style_keys.get(&key) {
            *index
        } else {
            let index = self.styles.len();
            self.style_keys.insert(key, index);
            self.styles.push(Arc::clone(&dictionary.styles));
            index
        };
        let key = (
            dictionary.grapheme_offsets.as_ptr() as usize,
            dictionary.grapheme_bytes.as_ptr() as usize,
        );
        let graphemes = if let Some(index) = self.grapheme_keys.get(&key) {
            *index
        } else {
            let index = self.graphemes.len();
            self.grapheme_keys.insert(key, index);
            self.graphemes.push((
                Arc::clone(&dictionary.grapheme_offsets),
                Arc::clone(&dictionary.grapheme_bytes),
            ));
            index
        };
        (styles, graphemes)
    }

    fn write(&self, output: &mut Vec<u8>) {
        uint(output, self.styles.len() as u64);
        for plane in &self.styles {
            uint(output, plane.len() as u64);
            for style in plane.iter().copied() {
                for value in style_fields(style) {
                    uint(output, value);
                }
            }
        }
        uint(output, self.graphemes.len() as u64);
        for (offsets, bytes) in &self.graphemes {
            uint(output, offsets.len() as u64);
            for offset in offsets.iter().copied() {
                uint(output, u64::from(offset));
            }
            blob(output, bytes);
        }
    }
}

fn ring(output: &mut Vec<u8>, ring: &HistoryRing, planes: &mut Planes) {
    uint(output, ring.len() as u64);
    for row in &ring.rows {
        let (styles, graphemes) = planes.dictionary(&row.dictionary);
        uint(output, styles as u64);
        uint(output, graphemes as u64);
        uint(output, row.revision);
        packed(output, &row.cells);
    }
}

fn fingerprint(output: &mut Vec<u8>, retained: &RetainedTerminalViewport, counter: u64) {
    let view = &retained.viewport;
    for value in [
        view.generation,
        view.view_generation,
        u64::from(view.dictionary_generation),
        u64::from(view.columns),
        u64::from(view.rows),
        u64::from(view.scrollbar.total),
        u64::from(view.scrollbar.offset),
        u64::from(view.scrollbar.len),
        u64::from(retained.history_scrollbar.total),
        u64::from(retained.history_scrollbar.offset),
        u64::from(retained.history_scrollbar.len),
        retained.history_mutations,
        retained.history_invalidations,
        retained.row_revision_epoch,
        retained.copy_generation,
        counter,
    ] {
        uint(output, value);
    }
    uint(output, retained.row_revisions.len() as u64);
    for revision in &retained.row_revisions {
        uint(output, *revision);
    }
    uint(output, retained.history.limit() as u64);
    uint(output, retained.below.limit() as u64);
    packed(output, &view.cells);
    let mut planes = Planes::default();
    assert_eq!(planes.dictionary(&view.dictionary), (0, 0));
    ring(output, &retained.history, &mut planes);
    ring(output, &retained.below, &mut planes);
    planes.write(output);
}

fn event(frame: &[u8]) -> EventPayload {
    match decode_protocol_frame(frame).expect("history source frame decodes") {
        ProtocolMessage::Event(Event { payload, .. }) => payload,
        _ => panic!("history fixture frame is not an event"),
    }
}

fn event_frame(payload: EventPayload) -> Vec<u8> {
    encode_protocol_message(&ProtocolMessage::Event(Event {
        sequence: 0,
        payload,
    }))
    .expect("history source event encodes")
}

fn viewport(ids: &[u32], generation: u64, total: u32, offset: u32) -> TerminalViewport {
    let mut view = TerminalViewport::blank(1, ids.len() as u16, SessionStatus::Running);
    view.generation = generation;
    view.view_generation = generation;
    view.scrollbar = ScrollbarState {
        total,
        offset,
        len: ids.len() as u32,
    };
    for (cell, id) in Arc::make_mut(&mut view.cells).iter_mut().zip(ids) {
        *cell = PackedCell::new(0xe000 + id, 0, CellWidth::Narrow);
    }
    view
}

struct Trace {
    name: &'static str,
    header: Vec<u8>,
    requests: Vec<u8>,
    expected: Vec<u8>,
    count: u64,
    counter: u64,
    retained: RetainedTerminalViewport,
}

impl Trace {
    fn new(name: &'static str, initial: &TerminalViewport, counter: u64, limit: usize) -> Self {
        let frame = encode_terminal_viewport_event(PaneId(0), 0, initial)
            .expect("initial history viewport encodes");
        let EventPayload::TerminalViewport { viewport, .. } = event(&frame) else {
            unreachable!()
        };
        let mut header = vec![];
        uint(&mut header, counter);
        uint(&mut header, limit as u64);
        blob(&mut header, &frame);
        let mut counter = counter;
        let mut retained = new_retained_viewport(viewport, &mut counter);
        retained.history = HistoryRing::with_limit(limit);
        let mut expected = vec![];
        fingerprint(&mut expected, &retained, counter);
        Self {
            name,
            header,
            requests: vec![],
            expected,
            count: 0,
            counter,
            retained,
        }
    }

    fn finish(mut self, vectors: &mut Vec<Value>) {
        uint(&mut self.header, self.count);
        self.header.extend_from_slice(&self.requests);
        vectors.push(record(self.name, "history", self.header, self.expected));
    }

    fn patch(&mut self, next: &TerminalViewport, mutation: u8) {
        let patch = TerminalViewport::diff(&self.retained.viewport, next)
            .expect("source history patch shape is compatible");
        let frame = event_frame(EventPayload::TerminalPatch {
            pane: PaneId(0),
            patch,
        });
        let EventPayload::TerminalPatch { mut patch, .. } = event(&frame) else {
            unreachable!()
        };
        match mutation {
            1 => patch.base_generation ^= 1 << 40,
            2 => patch.base_view_generation ^= 1 << 40,
            3 => patch.dictionary_generation = patch.dictionary_generation.wrapping_add(1),
            4 => patch.scroll = patch.rows as i16,
            _ => {}
        }
        uint(&mut self.requests, 0);
        blob(&mut self.requests, &frame);
        uint(&mut self.requests, u64::from(mutation));
        let accepted = apply_retained_patch(&mut self.retained, patch, &mut self.counter).is_ok();
        uint(&mut self.expected, u64::from(accepted));
        fingerprint(&mut self.expected, &self.retained, self.counter);
        self.count += 1;
    }

    fn replace(&mut self, next: &TerminalViewport) {
        let frame = encode_terminal_viewport_event(PaneId(0), 0, next)
            .expect("replacement viewport encodes");
        let EventPayload::TerminalViewport { viewport, .. } = event(&frame) else {
            unreachable!()
        };
        uint(&mut self.requests, 1);
        blob(&mut self.requests, &frame);
        replace_retained_viewport(&mut self.retained, viewport, &mut self.counter);
        uint(&mut self.expected, 1);
        fingerprint(&mut self.expected, &self.retained, self.counter);
        self.count += 1;
    }

    fn history(&mut self, start: u32, total: u32, offset: u32, columns: u16, ids: &[u32]) {
        let dictionary = self.retained.viewport.dictionary.as_ref().clone();
        let rows = ids
            .iter()
            .map(|id| {
                vec![PackedCell::new(0xe000 + id, 0, CellWidth::Narrow); usize::from(columns)]
            })
            .collect();
        let frame = event_frame(EventPayload::HistoryChunk {
            pane: PaneId(0),
            start,
            total,
            offset,
            columns,
            rows,
            dictionary,
        });
        let EventPayload::HistoryChunk {
            start,
            total,
            offset,
            columns,
            rows,
            dictionary,
            ..
        } = event(&frame)
        else {
            unreachable!()
        };
        uint(&mut self.requests, 2);
        blob(&mut self.requests, &frame);
        let accepted = apply_history_chunk(
            &mut self.retained,
            start,
            total,
            offset,
            columns,
            rows,
            dictionary,
            &mut self.counter,
        );
        uint(&mut self.expected, u64::from(accepted));
        fingerprint(&mut self.expected, &self.retained, self.counter);
        self.count += 1;
    }

    fn next_ids(&self, ids: &[u32], total: u32, offset: u32) -> TerminalViewport {
        let mut next = self.retained.viewport.clone();
        next.generation = next.generation.wrapping_add(1);
        next.view_generation = next.view_generation.wrapping_add(1);
        next.scrollbar = ScrollbarState {
            total,
            offset,
            len: ids.len() as u32,
        };
        assert_eq!(next.rows as usize, ids.len());
        for (cell, id) in Arc::make_mut(&mut next.cells).iter_mut().zip(ids) {
            *cell = PackedCell::new(0xe000 + id, 0, CellWidth::Narrow);
        }
        next
    }
}

pub fn vectors(vectors: &mut Vec<Value>) {
    for counter in [0, 1, u64::MAX - 3] {
        let mut trace = Trace::new(
            "retained-rows-counter-wrap-and-shifts",
            &viewport(&[10, 11, 12], 1, 100, 97),
            counter,
            10_000,
        );
        trace.history(94, 100, u32::MAX, 1, &[7, 8, 9]);
        trace.history(91, 101, 97, 1, &[4, 5, 6]);
        trace.history(90, 100, 97, 1, &[4, 5, 6]);
        trace.history(91, 100, 97, 1, &[4, 5, 6]);
        let next = trace.next_ids(&[9, 10, 11], 100, 96);
        trace.patch(&next, 0);
        let next = trace.next_ids(&[8, 9, 10], 100, 95);
        trace.patch(&next, 0);
        let next = trace.next_ids(&[9, 10, 11], 100, 96);
        trace.patch(&next, 0);
        for mutation in 1..=4 {
            let next = trace.next_ids(&[10, 11, 12], 101, 97);
            trace.patch(&next, mutation);
        }
        let next = trace.next_ids(&[10, 11, 12], 101, 97);
        trace.patch(&next, 0);
        let next = trace.next_ids(&[11, 12, 13], 101, 97);
        trace.patch(&next, 0);
        trace.replace(&viewport(&[50, 51], 100, 200, 180));
        trace.finish(vectors);
    }
    let mut trace = Trace::new(
        "retained-jumps-reanchor-before-invalidation",
        &viewport(&[10, 11, 12], 1, 100, 10),
        1,
        10_000,
    );
    trace.history(0, 100, 10, 1, &(0..10).collect::<Vec<_>>());
    for (ids, offset) in [([2, 3, 4], 2), ([8, 9, 10], 8), ([66, 67, 68], 6)] {
        let next = trace.next_ids(&ids, 100, offset);
        trace.patch(&next, 0);
    }
    let next = trace.next_ids(&[90, 91, 92], 100, 6);
    trace.patch(&next, 0);
    trace.finish(vectors);
    for limit in [0, 1, 5, 10_000] {
        let mut trace = Trace::new(
            "history-cap-and-offset-identity",
            &viewport(&[100, 101, 102], 1, 1000, 100),
            1,
            limit,
        );
        trace.history(94, 1000, 3, 1, &[94, 95, 96, 97, 98, 99]);
        for frame in 0..24 {
            let offset = 101 + frame;
            let next = trace.next_ids(&[offset, offset + 1, offset + 2], 1000, offset);
            trace.patch(&next, 0);
        }
        trace.finish(vectors);
    }
    let mut trace = Trace::new(
        "reanchor-across-below-ring-cap-and-gap",
        &viewport(&[1000, 1001, 1002], 1, 2000, 1000),
        1,
        10_000,
    );
    trace.history(488, 2000, 1000, 1, &(488..1000).collect::<Vec<_>>());
    trace.history(0, 2000, 1000, 1, &(0..488).collect::<Vec<_>>());
    for (ids, offset) in [
        ([0, 1, 2], 0),
        ([510, 511, 512], 510),
        ([200, 201, 202], 200),
    ] {
        let next = trace.next_ids(&ids, 2000, offset);
        trace.patch(&next, 0);
    }
    trace.finish(vectors);
    let mut initial = viewport(&[0, 1, 2], 1, 3, 0);
    let dictionary = Arc::make_mut(&mut initial.dictionary);
    let mut styles = dictionary.styles.to_vec();
    for value in 0..80_u8 {
        styles.push(PackedStyle::new(
            Color::rgb(value, 0, 0),
            initial.background,
            None,
            0,
            UnderlineStyle::None,
        ));
    }
    dictionary.styles = styles.into();
    let cluster = format!("a{}", "\u{301}".repeat(200));
    let mut bytes = vec![];
    let mut offsets = vec![0];
    for _ in 0..3 {
        bytes.extend_from_slice(cluster.as_bytes());
        offsets.push(bytes.len() as u32);
    }
    dictionary.grapheme_offsets = offsets.into();
    dictionary.grapheme_bytes = bytes.into();
    let mut trace = Trace::new(
        "independent-compaction-crosses-both-plane-thresholds",
        &initial,
        1,
        10_000,
    );
    for frame in 1_u16..=16 {
        let mut next = trace.retained.viewport.clone();
        next.generation += 1;
        next.view_generation += 1;
        next.scrollbar.total += 1;
        next.scrollbar.offset += 1;
        let dictionary = Arc::make_mut(&mut next.dictionary);
        let glyph = if frame % 2 == 1 {
            let mut styles = dictionary.styles.to_vec();
            styles.push(PackedStyle::new(
                Color::rgb(0, frame as u8, 0),
                next.background,
                None,
                0,
                UnderlineStyle::None,
            ));
            dictionary.styles = styles.into();
            0xe002 + u32::from(frame)
        } else {
            let text = format!(
                "{}{}",
                char::from_u32(0x400 + u32::from(frame)).unwrap(),
                "\u{301}".repeat(200)
            );
            let glyph = GRAPHEME_TABLE_BIT | (dictionary.grapheme_offsets.len() - 1) as u32;
            let mut offsets = dictionary.grapheme_offsets.to_vec();
            let mut bytes = dictionary.grapheme_bytes.to_vec();
            bytes.extend_from_slice(text.as_bytes());
            offsets.push(bytes.len() as u32);
            dictionary.grapheme_offsets = offsets.into();
            dictionary.grapheme_bytes = bytes.into();
            glyph
        };
        let style = (dictionary.styles.len() - 1) as u16;
        let cells = Arc::make_mut(&mut next.cells);
        cells.copy_within(1.., 0);
        cells[2] = PackedCell::new(glyph, style, CellWidth::Narrow);
        trace.patch(&next, 0);
    }
    trace.finish(vectors);
    for alternating in [false, true] {
        let mut trace = Trace::new(
            "independent-history-dictionary-planes",
            &viewport(&[0, 1, 2], 1, 3, 0),
            1,
            10_000,
        );
        for frame in 1_u16..=160 {
            let mut next = trace.retained.viewport.clone();
            next.generation += 1;
            next.view_generation += 1;
            next.scrollbar.total += 1;
            next.scrollbar.offset += 1;
            let dictionary = Arc::make_mut(&mut next.dictionary);
            let glyph = if !alternating || frame % 2 == 1 {
                let mut styles = dictionary.styles.to_vec();
                styles.push(PackedStyle::new(
                    Color::rgb((frame >> 8) as u8, frame as u8, 0),
                    next.background,
                    None,
                    0,
                    UnderlineStyle::None,
                ));
                dictionary.styles = styles.into();
                0xe002 + u32::from(frame)
            } else {
                let text = format!(
                    "{}\u{301}",
                    char::from_u32(0x400 + u32::from(frame)).unwrap()
                );
                let glyph = GRAPHEME_TABLE_BIT | (dictionary.grapheme_offsets.len() - 1) as u32;
                let mut offsets = dictionary.grapheme_offsets.to_vec();
                let mut bytes = dictionary.grapheme_bytes.to_vec();
                bytes.extend_from_slice(text.as_bytes());
                offsets.push(bytes.len() as u32);
                dictionary.grapheme_offsets = offsets.into();
                dictionary.grapheme_bytes = bytes.into();
                glyph
            };
            let style = (dictionary.styles.len() - 1) as u16;
            let cells = Arc::make_mut(&mut next.cells);
            cells.copy_within(1.., 0);
            cells[2] = PackedCell::new(glyph, style, CellWidth::Narrow);
            trace.patch(&next, 0);
        }
        trace.finish(vectors);
    }
}
