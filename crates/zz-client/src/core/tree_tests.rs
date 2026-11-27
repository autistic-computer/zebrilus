use super::test_support::{drain, event, snapshot_with};
use super::*;
use zz_protocol::WindowId;

fn compact_batch(messages: Vec<ProtocolMessage>) -> ProtocolMessage {
    ProtocolMessage::Batch(zz_protocol::Batch::from_messages(1, messages).unwrap())
}

fn compact_view(epoch: u64, generation: u64) -> ClientView {
    ClientView {
        session: Some(SessionId(0)),
        focused_window: Some(WindowId(0)),
        layout_generation: generation,
        attachment_generation: epoch,
        ..ClientView::default()
    }
}

#[test]
fn compact_batch_reattach_resets_once_and_ordinary_view_keeps_frames() {
    let pane = PaneId(4);
    let frame = || {
        event(EventPayload::TerminalViewport {
            pane,
            viewport: TerminalViewport::blank(80, 24, zz_terminal::SessionStatus::Running),
        })
    };
    let mut core = ClientCore::new();
    core.handle_message(compact_batch(vec![
        event(EventPayload::Snapshot(snapshot_with(&[pane]))),
        event(EventPayload::ClientView(compact_view(1, 10))),
        frame(),
    ]));
    assert_eq!(core.layout_generation(), 10);
    assert!(core.viewport(pane).is_some());
    let events = drain(&mut core);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, CoreEvent::SnapshotChanged))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, CoreEvent::Attached { .. }))
            .count(),
        1
    );
    core.handle_message(event(EventPayload::ClientView(compact_view(1, 11))));
    assert!(core.viewport(pane).is_some());
    assert!(
        !drain(&mut core)
            .iter()
            .any(|event| matches!(event, CoreEvent::Attached { .. }))
    );
    core.handle_message(event(EventPayload::PrefixArmed { armed: true }));
    drain(&mut core);
    core.handle_message(event(EventPayload::ClientView(compact_view(2, 12))));
    assert!(core.viewport(pane).is_none());
    let events = drain(&mut core);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, CoreEvent::Attached { .. }))
    );
    assert!(events.contains(&CoreEvent::PrefixArmed { armed: false }));
}

#[test]
fn tree_base_mismatch_requests_one_sync_and_full_snapshot_recovers() {
    let original = snapshot_with(&[PaneId(4)]);
    let mut next = original.clone();
    next.generation += 1;
    next.sessions[0].name = "after".to_owned();
    let mut core = ClientCore::new();
    core.handle_message(event(EventPayload::Snapshot(original.clone())));
    let mut delta = zz_protocol::TreeDelta::between(&original, &next);
    delta.base += 20;
    core.handle_message(event(EventPayload::TreeDelta(delta.clone())));
    core.handle_message(event(EventPayload::TreeDelta(delta)));
    assert_eq!(core.snapshot().as_ref(), &original);
    assert_eq!(core.poll_outbound(), Some(Outbound::TreeSync));
    assert_eq!(core.poll_outbound(), None);
    core.handle_message(event(EventPayload::Snapshot(next.clone())));
    let mut latest = next.clone();
    latest.generation += 1;
    latest.sessions[0].name = "latest".to_owned();
    core.handle_message(event(EventPayload::TreeDelta(
        zz_protocol::TreeDelta::between(&next, &latest),
    )));
    assert_eq!(core.snapshot().as_ref(), &latest);
}

#[test]
fn shared_tree_deltas_reapply_personalized_overlay_without_drift() {
    let mut raw = snapshot_with(&[PaneId(4)]);
    let mut stamped = raw.clone();
    stamped.sessions[0].windows[0].status_label = "own label".to_owned();
    stamped.sessions[0].windows[0]
        .panes
        .get_mut(&PaneId(4))
        .unwrap()
        .border_status_text = "own border".to_owned();
    let mut view = compact_view(1, 1);
    view.overlay = zz_protocol::TreeDelta::between(&raw, &stamped).ops;
    let mut core = ClientCore::new();
    core.handle_message(compact_batch(vec![
        event(EventPayload::Snapshot(raw.clone())),
        event(EventPayload::ClientView(view.clone())),
    ]));
    for version in 2..42 {
        let before = raw.clone();
        raw.generation = version;
        raw.sessions[0].windows[0].name = format!("window-{version}");
        core.handle_message(compact_batch(vec![
            event(EventPayload::TreeDelta(zz_protocol::TreeDelta::between(
                &before, &raw,
            ))),
            event(EventPayload::ClientView(view.clone())),
        ]));
        let mut expected = raw.clone();
        view.apply(&mut expected).unwrap();
        assert_eq!(core.snapshot().as_ref(), &expected);
        assert_eq!(core.poll_outbound(), None);
    }
}

#[test]
fn batch_removal_replaces_obsolete_overlay_before_materializing() {
    let raw = snapshot_with(&[PaneId(4)]);
    let mut stamped = raw.clone();
    stamped.sessions[0].windows[0].status_label = "own label".to_owned();
    let mut view = compact_view(1, 1);
    view.overlay = zz_protocol::TreeDelta::between(&raw, &stamped).ops;
    let empty = MuxSnapshot {
        generation: 2,
        ..MuxSnapshot::default()
    };
    for payload in [
        EventPayload::TreeDelta(zz_protocol::TreeDelta::between(&raw, &empty)),
        EventPayload::Snapshot(empty),
    ] {
        let mut core = ClientCore::new();
        core.handle_message(compact_batch(vec![
            event(EventPayload::Snapshot(raw.clone())),
            event(EventPayload::ClientView(view.clone())),
        ]));
        core.handle_message(compact_batch(vec![
            event(payload),
            event(EventPayload::ClientView(ClientView::default())),
        ]));
        assert!(core.snapshot().sessions.is_empty());
        assert_eq!(core.poll_outbound(), None);
    }
}

#[test]
fn malformed_batch_does_not_partially_reduce() {
    let mut core = ClientCore::new();
    let valid =
        zz_protocol::encode_protocol_message(&event(EventPayload::Snapshot(snapshot_with(&[
            PaneId(4),
        ]))))
        .unwrap();
    core.handle_message(ProtocolMessage::Batch(zz_protocol::Batch {
        sequence: 1,
        frames: vec![valid, vec![0, 1]],
    }));
    assert!(core.snapshot().sessions.is_empty());
    assert_eq!(core.poll_outbound(), Some(Outbound::TreeSync));
}

#[test]
fn reused_cell_buffers_follow_patches_while_clones_are_held() {
    let pane = PaneId(4);
    let mut adoptable = 0;
    for seed in 1..=4_u64 {
        let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let mut next = move |bound: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % bound as u64) as usize
        };
        let mut core = ClientCore::new();
        let mut reference = TerminalViewport::blank(12, 6, zz_terminal::SessionStatus::Running);
        core.handle_message(event(EventPayload::TerminalViewport {
            pane,
            viewport: reference.clone(),
        }));
        let mut held: Vec<(TerminalViewport, Vec<PackedCell>)> = Vec::new();
        for step in 0..600 {
            let mut current = reference.clone();
            current.generation += 1;
            let cells = Arc::make_mut(&mut current.cells);
            let len = cells.len();
            match next(6) {
                0 => {
                    let shift = (1 + next(2)) * 12;
                    cells.copy_within(shift.., 0);
                    cells[len - shift..].fill(PackedCell::EMPTY);
                }
                1 => {
                    let row = next(6) * 12;
                    let start = row + next(12);
                    cells[start..row + 12].fill(PackedCell::EMPTY);
                }
                _ => {}
            }
            for _ in 0..next(5) {
                let index = next(len);
                let glyph = u32::from(b'a') + next(26) as u32;
                cells[index] = PackedCell::new(glyph, 0, zz_terminal::CellWidth::Narrow);
            }
            adoptable += usize::from(
                core.spare_cells
                    .get(&pane)
                    .is_some_and(|spare| Arc::strong_count(&spare.cells) == 1),
            );
            if next(97) == 0 {
                core.handle_message(event(EventPayload::TerminalViewport {
                    pane,
                    viewport: current.clone(),
                }));
            } else {
                let patch = TerminalViewport::diff(&reference, &current).unwrap();
                core.handle_message(event(EventPayload::TerminalPatch { pane, patch }));
            }
            reference = current;
            drain(&mut core);
            assert_eq!(
                core.viewport(pane).unwrap().cells[..],
                reference.cells[..],
                "seed {seed} step {step}"
            );
            for (clone, cells) in &held {
                assert_eq!(clone.cells[..], cells[..], "seed {seed} step {step}");
            }
            match next(4) {
                0 if held.len() < 3 => {}
                1 => {
                    held.drain(..held.len().min(1));
                    continue;
                }
                2 => {
                    held.clear();
                    continue;
                }
                _ => held.clear(),
            }
            let clone = core.viewport(pane).unwrap().clone();
            let cells = clone.cells.to_vec();
            held.push((clone, cells));
        }
    }
    assert!(adoptable > 0);
}

#[test]
fn retained_history_is_opt_in_and_a_chunk_reports_a_viewport_change() {
    let pane = PaneId(3);
    let frame = |rows: [u32; 3], offset: u32, generation: u64| {
        let mut viewport = TerminalViewport::blank(1, 3, zz_terminal::SessionStatus::Running);
        viewport.generation = generation;
        viewport.view_generation = generation;
        viewport.scrollbar = zz_terminal::ScrollbarState {
            total: offset + 3,
            offset,
            len: 3,
        };
        for (cell, row) in Arc::make_mut(&mut viewport.cells).iter_mut().zip(rows) {
            *cell = PackedCell::new(0xe000 + row, 0, zz_terminal::CellWidth::Narrow);
        }
        viewport
    };
    let scrolled = |core: &mut ClientCore| {
        let first = frame([5, 6, 7], 5, 1);
        let patch = TerminalViewport::diff(&first, &frame([6, 7, 8], 6, 2)).unwrap();
        assert_eq!(patch.scroll, -1);
        core.handle_message(event(EventPayload::TerminalViewport {
            pane,
            viewport: first,
        }));
        core.handle_message(event(EventPayload::TerminalPatch { pane, patch }));
        drain(core);
    };

    let mut plain = ClientCore::new();
    scrolled(&mut plain);
    assert!(plain.retained_viewport(pane).unwrap().history.is_empty());

    let mut retaining = ClientCore::new();
    retaining.retain_history();
    scrolled(&mut retaining);
    assert_eq!(retaining.retained_viewport(pane).unwrap().history.len(), 1);
    let rows = [3, 4]
        .map(|row| {
            vec![PackedCell::new(
                0xe000 + row,
                0,
                zz_terminal::CellWidth::Narrow,
            )]
        })
        .to_vec();
    let dictionary = retaining
        .viewport(pane)
        .unwrap()
        .dictionary
        .as_ref()
        .clone();
    assert!(retaining.apply_history_chunk(pane, 3, 9, 6, 1, rows, dictionary));
    assert_eq!(retaining.retained_viewport(pane).unwrap().history.len(), 3);
    assert_eq!(
        drain(&mut retaining),
        [CoreEvent::ViewportChanged {
            pane,
            damage: ViewportDamage::Rows(Vec::new()),
        }]
    );
}
