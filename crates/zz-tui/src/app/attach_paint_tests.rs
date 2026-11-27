use super::test_support::{paned_model, with_row_text};
use super::*;

fn initial_snapshot(panes: &[PaneId]) -> zz_protocol::MuxSnapshot {
    let (model, _) = paned_model();
    let mut snapshot = (*model.snapshot).clone();
    let window = &mut snapshot.sessions[0].windows[0];
    for pane in panes {
        window.panes.insert(
            *pane,
            zz_protocol::PaneSnapshot {
                id: *pane,
                title: "shell".to_owned(),
                kind: PaneKindSnapshot::Terminal,
                synchronized_input: false,
                bell: false,
                dead: false,
                dead_status: None,
                border_colour: None,
                active_border_colour: None,
                border_status_text: String::new(),
                mode: None,
                status: None,
                summary: zz_protocol::PaneSummary::default(),
            },
        );
    }
    let split = |id, axis, first, second| zz_protocol::LayoutNode::Split {
        id: zz_protocol::SplitId(id),
        axis,
        ratio: 0.5,
        first: Box::new(first),
        second: Box::new(second),
    };
    window.layout = if panes.len() == 1 {
        zz_protocol::LayoutNode::Pane(panes[0])
    } else {
        split(
            1,
            zz_protocol::Axis::Horizontal,
            split(
                2,
                zz_protocol::Axis::Vertical,
                zz_protocol::LayoutNode::Pane(panes[0]),
                zz_protocol::LayoutNode::Pane(panes[1]),
            ),
            split(
                3,
                zz_protocol::Axis::Vertical,
                zz_protocol::LayoutNode::Pane(panes[2]),
                zz_protocol::LayoutNode::Pane(panes[3]),
            ),
        )
    };
    snapshot
}

fn initial_event(payload: zz_protocol::EventPayload) -> ProtocolMessage {
    ProtocolMessage::Event(zz_protocol::Event {
        sequence: 0,
        payload,
    })
}

fn initial_view(epoch: u64) -> ProtocolMessage {
    initial_event(zz_protocol::EventPayload::ClientView(
        zz_protocol::ClientView {
            session: Some(zz_protocol::SessionId(1)),
            focused_window: Some(zz_protocol::WindowId(1)),
            layout_generation: 42,
            attachment_generation: epoch,
            ..zz_protocol::ClientView::default()
        },
    ))
}

fn initial_model(core: &ClientCore) -> Model {
    let endpoint = Endpoint::parse("unix:///tmp/zz-app-first-paint.sock").unwrap();
    Model::new(
        core,
        TerminalSize {
            columns: 79,
            rows: 24,
            cell_width_px: 8,
            cell_height_px: 16,
        },
        "host".to_owned(),
        "host".to_owned(),
        endpoint.clone(),
        endpoint,
        Vec::new(),
    )
}

#[test]
fn a_new_layout_generation_resends_an_unchanged_terminal_geometry() {
    let pane = PaneId(7);
    let mut core = ClientCore::new();
    core.handle_message(initial_event(zz_protocol::EventPayload::Snapshot(
        initial_snapshot(&[pane]),
    )));
    core.handle_message(initial_view(1));
    let mut model = initial_model(&core);
    model.update_snapshot(Arc::clone(core.snapshot()));
    let geometry = model.terminal_geometries();
    assert_eq!(geometry.len(), 1);
    let structure = model.paint_structure();
    let mut sent = Vec::new();
    send_resizes_with(&mut model, |input| {
        sent.push(input);
        Ok(())
    })
    .unwrap();
    assert_eq!(sent.len(), 1);
    while core.poll_event().is_some() {}
    core.handle_message(initial_event(zz_protocol::EventPayload::ClientView(
        zz_protocol::ClientView {
            session: Some(zz_protocol::SessionId(1)),
            focused_window: Some(zz_protocol::WindowId(1)),
            layout_generation: 43,
            attachment_generation: 1,
            ..zz_protocol::ClientView::default()
        },
    )));
    assert!(
        std::iter::from_fn(|| core.poll_event())
            .any(|event| matches!(event, CoreEvent::SnapshotChanged))
    );
    let outcome = refresh_snapshot(
        &mut model,
        Arc::clone(core.snapshot()),
        core.layout_generation(),
        |input| {
            sent.push(input);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(model.terminal_geometries(), geometry);
    assert_eq!(model.paint_structure(), structure);
    assert!(matches!(outcome, ProtocolOutcome::Repaint));
    assert_eq!(sent.len(), 2);
    let (_, (columns, rows, cell_width_px, cell_height_px)) = geometry[0];
    for (input, layout_generation) in sent.iter().zip([42, 43]) {
        assert_eq!(
            input,
            &InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
                layout_generation,
            }
        );
    }
    refresh_snapshot(
        &mut model,
        Arc::clone(core.snapshot()),
        core.layout_generation(),
        |input| {
            sent.push(input);
            Ok(())
        },
    )
    .unwrap();
    send_resizes_with(&mut model, |input| {
        sent.push(input);
        Ok(())
    })
    .unwrap();
    assert_eq!(sent.len(), 2);
}

#[test]
fn an_attached_refresh_resends_the_same_geometry_with_its_new_generation() {
    let pane = PaneId(7);
    let mut core = ClientCore::new();
    core.handle_message(initial_event(zz_protocol::EventPayload::Snapshot(
        initial_snapshot(&[pane]),
    )));
    core.handle_message(initial_view(1));
    let mut model = initial_model(&core);
    model.update_snapshot(Arc::clone(core.snapshot()));
    let geometry = model.terminal_geometries();
    let structure = model.paint_structure();
    let mut sent = Vec::new();
    send_resizes_with(&mut model, |input| {
        sent.push(input);
        Ok(())
    })
    .unwrap();
    assert_eq!(sent.len(), 1);
    while core.poll_event().is_some() {}
    core.handle_message(initial_event(zz_protocol::EventPayload::ClientView(
        zz_protocol::ClientView {
            session: Some(zz_protocol::SessionId(1)),
            focused_window: Some(zz_protocol::WindowId(1)),
            layout_generation: 43,
            attachment_generation: 2,
            ..zz_protocol::ClientView::default()
        },
    )));
    assert!(matches!(
        core.poll_event(),
        Some(CoreEvent::Attached { session }) if session == zz_protocol::SessionId(1)
    ));
    assert!(update_snapshot(
        &mut model,
        Arc::clone(core.snapshot()),
        core.layout_generation(),
    ));
    send_resizes_with(&mut model, |input| {
        sent.push(input);
        Ok(())
    })
    .unwrap();
    assert_eq!(model.terminal_geometries(), geometry);
    assert_eq!(model.paint_structure(), structure);
    assert_eq!(sent.len(), 2);
    let (_, (columns, rows, cell_width_px, cell_height_px)) = geometry[0];
    for (input, layout_generation) in sent.iter().zip([42, 43]) {
        assert_eq!(
            input,
            &InputMessage::ResizeTerminalV2 {
                pane,
                columns,
                rows,
                cell_width_px,
                cell_height_px,
                layout_generation,
            }
        );
    }
    refresh_snapshot(
        &mut model,
        Arc::clone(core.snapshot()),
        core.layout_generation(),
        |input| {
            sent.push(input);
            Ok(())
        },
    )
    .unwrap();
    send_resizes_with(&mut model, |input| {
        sent.push(input);
        Ok(())
    })
    .unwrap();
    assert_eq!(sent.len(), 2);
}

#[test]
fn initial_batch_has_every_marker_in_the_first_paint() {
    for count in [1, 4] {
        let panes = (7..7 + count).map(PaneId).collect::<Vec<_>>();
        let snapshot = initial_snapshot(&panes);
        let mut layout_model = initial_model(&ClientCore::new());
        layout_model.attached_session = Some(zz_protocol::SessionId(1));
        layout_model.update_snapshot(Arc::new(snapshot.clone()));
        let mut messages = vec![
            initial_event(zz_protocol::EventPayload::Snapshot(snapshot)),
            initial_view(1),
        ];
        for entry in &layout_model.layout.panes {
            let content = entry.content();
            let viewport = with_row_text(
                &TerminalViewport::blank(
                    content.width,
                    content.height,
                    zz_terminal::SessionStatus::Running,
                ),
                0,
                &format!("INITIAL-{}", entry.pane.0),
            );
            messages.push(initial_event(zz_protocol::EventPayload::TerminalViewport {
                pane: entry.pane,
                viewport,
            }));
        }
        let core = Mutex::new(ClientCore::new());
        let frames = FrameInbox::default();
        let images = KittyImageInbox::default();
        let (events, incoming) = mpsc::channel();
        assert!(forward_protocol_message(
            &core,
            ProtocolMessage::Batch(zz_protocol::Batch::from_messages(1, messages).unwrap()),
            1,
            &events,
            &frames,
            &images,
            &AtomicU8::new(KITTY_GATE_PROBING),
            |_| panic!("initial full state must not need another wire request"),
        ));
        let mut model = initial_model(&lock_core(&core));
        model.update_snapshot(Arc::clone(lock_core(&core).snapshot()));
        assert_eq!(model.attached_session, Some(zz_protocol::SessionId(1)));
        assert_eq!(model.layout_generation, 42);
        let ready = incoming.try_iter().collect::<Vec<_>>();
        assert!(ready.len() < MAX_COALESCED_EVENTS);
        assert_eq!(
            ready
                .iter()
                .filter(|event| matches!(event, MainEvent::Frames(1)))
                .count(),
            1
        );
        let (written, output) = mpsc::channel();
        let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
            written.send(bytes.to_vec()).unwrap();
            Ok(())
        }));
        take_frames(&frames, &mut model, &mut renderer);
        renderer.paint(&model, true).unwrap();
        let first =
            String::from_utf8(output.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
        for pane in panes {
            assert!(first.contains(&format!("INITIAL-{}", pane.0)), "{first:?}");
        }
    }
}

#[test]
fn held_startup_output_precedes_the_base_frame_in_the_first_physical_paint() {
    let pane = PaneId(7);
    let snapshot = initial_snapshot(&[pane]);
    let mut layout_model = initial_model(&ClientCore::new());
    layout_model.attached_session = Some(zz_protocol::SessionId(1));
    layout_model.update_snapshot(Arc::new(snapshot.clone()));
    let content = layout_model.pane_rect(pane).unwrap().content();
    let blank = TerminalViewport::blank(
        content.width,
        content.height,
        zz_terminal::SessionStatus::Running,
    );
    let direct = "/tmp/root.conf:1: INTERACTIVE_DIRECT";
    let nested = "/tmp/child.conf:1: INTERACTIVE_NESTED";
    let mut actor = with_row_text(&with_row_text(&blank, 0, direct), 1, nested);
    actor.mode = zz_terminal::TerminalMode::View {
        position: 1,
        total: 2,
    };
    let messages = vec![
        initial_event(zz_protocol::EventPayload::Snapshot(snapshot)),
        initial_view(1),
        initial_event(zz_protocol::EventPayload::CommandOutput {
            pane,
            output_id: 9,
            viewport: Some(actor),
        }),
        initial_event(zz_protocol::EventPayload::TerminalViewport {
            pane,
            viewport: with_row_text(&blank, 0, "BASE-SHELL"),
        }),
    ];
    let core = Mutex::new(ClientCore::new());
    let frames = FrameInbox::default();
    let images = KittyImageInbox::default();
    let (events, incoming) = mpsc::channel();
    assert!(forward_protocol_message(
        &core,
        ProtocolMessage::Batch(zz_protocol::Batch::from_messages(1, messages).unwrap()),
        1,
        &events,
        &frames,
        &images,
        &AtomicU8::new(KITTY_GATE_PROBING),
        |_| panic!("held startup state must not need another wire request"),
    ));
    events.send(MainEvent::Resize).unwrap();
    let mut model = initial_model(&lock_core(&core));
    model.update_snapshot(Arc::clone(lock_core(&core).snapshot()));
    assert!(model.command_output.is_none());
    let (written, output) = mpsc::channel();
    let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
        written.send(bytes.to_vec()).unwrap();
        Ok(())
    }));
    let mut deferred = None;
    let mut next = incoming.try_recv().ok();
    let mut paint = PendingPaint::None;
    let mut actions = Vec::new();
    while let Some(event) = next.take() {
        match event {
            MainEvent::Core { event, .. } => match *event {
                CoreEvent::Attached { .. } => {
                    model.set_command_output(None, None);
                    model.viewports = [(pane, lock_core(&core).viewport(pane).unwrap().clone())]
                        .into_iter()
                        .collect();
                    actions.push("attached");
                    paint = paint.max(PendingPaint::RepaintAll);
                }
                CoreEvent::CommandOutputChanged => {
                    let core = lock_core(&core);
                    model.set_command_output(
                        core.command_output_id(),
                        core.command_output()
                            .map(|(pane, frame)| (pane, frame.clone())),
                    );
                    actions.push("output");
                    paint = paint.max(PendingPaint::RepaintAll);
                }
                _ => paint = paint.max(PendingPaint::Repaint),
            },
            MainEvent::KittyImages(1) => {
                apply_kitty_updates(&mut renderer, images.take(), true);
                actions.push("reset");
            }
            MainEvent::Frames(1) => {
                take_frames(&frames, &mut model, &mut renderer);
                actions.push("frames");
                paint = paint.max(PendingPaint::Frames);
            }
            _ => panic!("unexpected event in startup paint span"),
        }
        next = next_paint_event(&incoming, &mut deferred);
    }
    assert_eq!(actions, ["attached", "reset", "output", "frames"]);
    assert_eq!(paint, PendingPaint::RepaintAll);
    assert!(matches!(deferred, Some(MainEvent::Resize)));
    assert!(incoming.try_recv().is_err());
    assert_eq!(model.command_output_focus(), Some(pane));
    renderer.invalidate();
    renderer.paint(&model, true).unwrap();
    let first = String::from_utf8(output.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    assert_eq!(first.matches("\x1b[2J").count(), 1, "{first:?}");
    assert_eq!(first.matches(direct).count(), 1, "{first:?}");
    assert_eq!(first.matches(nested).count(), 1, "{first:?}");
    assert!(first.find(direct).unwrap() < first.find(nested).unwrap());
    assert!(!first.contains("BASE-SHELL"), "{first:?}");
    assert!(output.try_recv().is_err());
}

#[test]
fn initial_drain_paints_metadata_reset_and_frames_once() {
    for count in [1, 4] {
        let panes = (7..7 + count).map(PaneId).collect::<Vec<_>>();
        let snapshot = initial_snapshot(&panes);
        let mut layout_model = initial_model(&ClientCore::new());
        layout_model.attached_session = Some(zz_protocol::SessionId(1));
        layout_model.update_snapshot(Arc::new(snapshot.clone()));
        let mut messages = vec![
            initial_event(zz_protocol::EventPayload::Snapshot(snapshot)),
            initial_view(1),
            initial_event(zz_protocol::EventPayload::AppearanceChanged {
                appearance: Box::default(),
                provenance: zz_terminal::AppearanceProvenance::default(),
            }),
            initial_event(zz_protocol::EventPayload::StatusChanged {
                status: zz_protocol::StatusLine::default(),
            }),
        ];
        for entry in &layout_model.layout.panes {
            let content = entry.content();
            messages.push(initial_event(zz_protocol::EventPayload::TerminalViewport {
                pane: entry.pane,
                viewport: with_row_text(
                    &TerminalViewport::blank(
                        content.width,
                        content.height,
                        zz_terminal::SessionStatus::Running,
                    ),
                    0,
                    &format!("DRAIN-{}", entry.pane.0),
                ),
            }));
        }
        let core = Mutex::new(ClientCore::new());
        let frames = FrameInbox::default();
        let images = KittyImageInbox::default();
        let (events, incoming) = mpsc::channel();
        assert!(forward_protocol_message(
            &core,
            ProtocolMessage::Batch(zz_protocol::Batch::from_messages(1, messages).unwrap()),
            1,
            &events,
            &frames,
            &images,
            &AtomicU8::new(KITTY_GATE_PROBING),
            |_| panic!("initial full state must not need another wire request"),
        ));
        events.send(MainEvent::Resize).unwrap();
        let mut model = initial_model(&lock_core(&core));
        model.update_snapshot(Arc::clone(lock_core(&core).snapshot()));
        let (written, output) = mpsc::channel();
        let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
            written.send(bytes.to_vec()).unwrap();
            Ok(())
        }));
        let mut deferred = None;
        let mut actions = Vec::new();
        let mut paints = Vec::new();
        while let Some(event) = deferred.take().or_else(|| incoming.try_recv().ok()) {
            if matches!(event, MainEvent::KittyImages(1)) {
                apply_kitty_updates(&mut renderer, images.take(), true);
                actions.push("reset");
                continue;
            }
            let mut paint = PendingPaint::None;
            let mut next = Some(event);
            while let Some(event) = next.take() {
                match event {
                    MainEvent::Core { event, .. } => match *event {
                        CoreEvent::Attached { .. } => {
                            model.viewports = panes
                                .iter()
                                .map(|pane| {
                                    (*pane, lock_core(&core).viewport(*pane).unwrap().clone())
                                })
                                .collect();
                            actions.push("attached");
                            paint = paint.max(PendingPaint::RepaintAll);
                        }
                        CoreEvent::AppearanceChanged => {
                            paint = paint.max(PendingPaint::RepaintAll);
                        }
                        _ => paint = paint.max(PendingPaint::Repaint),
                    },
                    MainEvent::KittyImages(1) => {
                        apply_kitty_updates(&mut renderer, images.take(), true);
                        actions.push("reset");
                    }
                    MainEvent::Frames(1) => {
                        take_frames(&frames, &mut model, &mut renderer);
                        actions.push("frames");
                        paint = paint.max(PendingPaint::Frames);
                    }
                    _ => panic!("unexpected event in paint span"),
                }
                next = next_paint_event(&incoming, &mut deferred);
            }
            match paint {
                PendingPaint::RepaintAll => {
                    renderer.invalidate();
                    renderer.paint(&model, true).unwrap();
                }
                PendingPaint::Repaint => renderer.paint(&model, false).unwrap(),
                PendingPaint::Frames => renderer.paint_frames(&model).unwrap(),
                PendingPaint::None => {}
            }
            if paint != PendingPaint::None {
                paints.push(output.recv_timeout(Duration::from_secs(2)).unwrap());
            }
            if matches!(deferred, Some(MainEvent::Resize)) {
                break;
            }
        }
        assert_eq!(paints.len(), 1, "panes={count}, actions={actions:?}");
        let first = String::from_utf8(paints.concat()).unwrap();
        assert_eq!(first.matches("\x1b[2J").count(), 1, "{first:?}");
        for pane in panes {
            assert_eq!(first.matches(&format!("DRAIN-{}", pane.0)).count(), 1);
        }
        assert_eq!(actions, ["attached", "reset", "frames"]);
        assert!(matches!(deferred, Some(MainEvent::Resize)));
        assert!(incoming.try_recv().is_err());
    }
}

fn placing_viewport(generation: u64) -> TerminalViewport {
    let mut viewport = TerminalViewport::blank(79, 23, zz_terminal::SessionStatus::Running);
    viewport.kitty_placements = Arc::from([zz_terminal::KittyPlacement {
        image_id: 1299,
        image_generation: generation,
        layer: zz_terminal::KittyLayer::AboveText,
        viewport_col: 0,
        viewport_row: 0,
        absolute_row: 0,
        cell_offset_x: 0,
        cell_offset_y: 0,
        grid_cols: 1,
        grid_rows: 1,
        pixel_width: 1,
        pixel_height: 1,
        source_rect: None,
    }]);
    viewport
}

#[test]
fn initial_and_later_batches_reset_before_images_before_placing_frames() {
    let pane = PaneId(7);
    let core = Mutex::new(ClientCore::new());
    let frames = FrameInbox::default();
    let images = KittyImageInbox::default();
    let gate = AtomicU8::new(KITTY_GATE_PROBING);
    let (events, incoming) = mpsc::channel();
    for epoch in [1, 2] {
        let mut messages = vec![
            initial_event(zz_protocol::EventPayload::Snapshot(initial_snapshot(&[
                pane,
            ]))),
            initial_view(epoch),
        ];
        for image_id in 1000..1300 {
            messages.push(initial_event(zz_protocol::EventPayload::KittyImageBegin {
                pane,
                image_id,
                generation: epoch,
                width: 1,
                height: 1,
                total_bytes: 4,
            }));
            messages.push(initial_event(zz_protocol::EventPayload::KittyImageChunk {
                pane,
                image_id,
                generation: epoch,
                bytes: vec![1, 2, 3, 255],
            }));
        }
        let viewport = placing_viewport(epoch);
        messages.push(initial_event(zz_protocol::EventPayload::TerminalViewport {
            pane,
            viewport,
        }));
        messages.push(ProtocolMessage::CommandResponse(CommandResponse::Error {
            request_id: 99,
            error: ServerError::InvalidCommand("tail-error".to_owned()),
            output: zz_protocol::RawText::default(),
        }));
        assert!(forward_protocol_message(
            &core,
            ProtocolMessage::Batch(zz_protocol::Batch::from_messages(epoch, messages).unwrap()),
            1,
            &events,
            &frames,
            &images,
            &gate,
            |_| panic!("initial full state must not need another wire request"),
        ));
        let ready = incoming.try_iter().collect::<Vec<_>>();
        let attached = ready.iter().position(|event| matches!(event, MainEvent::Core { event, .. } if matches!(**event, CoreEvent::Attached { .. }))).unwrap();
        let delivered = ready
            .iter()
            .position(|event| matches!(event, MainEvent::KittyImages(1)))
            .unwrap();
        let placing = ready
            .iter()
            .position(|event| matches!(event, MainEvent::Frames(1)))
            .unwrap();
        let tail = ready.iter().position(|event| matches!(event, MainEvent::Core { event, .. } if matches!(**event, CoreEvent::CommandResponse(CommandResponse::Error { request_id: 99, .. })))).unwrap();
        assert!(attached < delivered && delivered < placing && placing < tail);
        assert!(ready.len() < MAX_COALESCED_EVENTS);
        let state = images.0.lock().unwrap();
        assert_eq!(state.pending.len(), 301);
        assert!(state.pending.iter().all(
            |image| matches!(image, KittyImageUpdate::Reset) || matches!(image, KittyImageUpdate::Ready(image) if image.generation == epoch)
        ));
        drop(state);
        let state = frames.0.lock().unwrap();
        assert_eq!(state.pending.len(), 1);
        assert_eq!(
            state.pending[&pane].viewport.kitty_placements[0].image_generation,
            epoch
        );
        drop(state);
        images.take();
        frames.take();
    }
}

fn delayed_main_reattach_keeps_the_latest_image(legacy: bool) {
    let pane = PaneId(7);
    let core = Mutex::new(ClientCore::new());
    let frames = FrameInbox::default();
    let images = KittyImageInbox::default();
    let gate = AtomicU8::new(KITTY_GATE_ENABLED);
    let (events, incoming) = mpsc::channel();
    for epoch in [1, 2] {
        let snapshot = initial_snapshot(&[pane]);
        let mut messages = if legacy {
            vec![ProtocolMessage::Attached {
                session: zz_protocol::SessionId(1),
                snapshot,
                read_only: false,
                client_flags: String::new(),
            }]
        } else {
            vec![
                initial_event(zz_protocol::EventPayload::Snapshot(snapshot)),
                initial_view(epoch),
            ]
        };
        messages.extend([
            initial_event(zz_protocol::EventPayload::KittyImageBegin {
                pane,
                image_id: 1299,
                generation: epoch,
                width: 1,
                height: 1,
                total_bytes: 4,
            }),
            initial_event(zz_protocol::EventPayload::KittyImageChunk {
                pane,
                image_id: 1299,
                generation: epoch,
                bytes: vec![1, 2, 3, 255],
            }),
            initial_event(zz_protocol::EventPayload::TerminalViewport {
                pane,
                viewport: placing_viewport(epoch),
            }),
        ]);
        if !legacy {
            messages = vec![ProtocolMessage::Batch(
                zz_protocol::Batch::from_messages(epoch, messages).unwrap(),
            )];
        }
        for message in messages {
            assert!(forward_protocol_message(
                &core,
                message,
                1,
                &events,
                &frames,
                &images,
                &gate,
                |_| panic!("retained full state must not need another wire request"),
            ));
        }
    }
    let mut model = initial_model(&lock_core(&core));
    model.update_snapshot(Arc::clone(lock_core(&core).snapshot()));
    let (written, output) = mpsc::channel();
    let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
        written.send(bytes.to_vec()).unwrap();
        Ok(())
    }));
    renderer.enable_kitty_graphics();
    let mut final_paint = String::new();
    for event in incoming.try_iter() {
        match event {
            MainEvent::Core { event, .. } if matches!(*event, CoreEvent::Attached { .. }) => {
                model.update_snapshot(Arc::clone(lock_core(&core).snapshot()));
                renderer.invalidate();
            }
            MainEvent::KittyImages(1) => {
                apply_kitty_updates(&mut renderer, images.take(), true);
            }
            MainEvent::Frames(1) => {
                take_frames(&frames, &mut model, &mut renderer);
                renderer.paint(&model, true).unwrap();
                final_paint =
                    String::from_utf8(output.recv_timeout(Duration::from_secs(2)).unwrap())
                        .unwrap();
            }
            _ => {}
        }
    }
    assert!(
        final_paint.contains("\x1b_Ga=p"),
        "legacy={legacy} latest placement lost: {final_paint:?}"
    );
}

#[test]
fn delayed_main_compact_reattach_keeps_the_latest_image() {
    delayed_main_reattach_keeps_the_latest_image(false);
}

#[test]
fn delayed_main_legacy_reattach_keeps_the_latest_image() {
    delayed_main_reattach_keeps_the_latest_image(true);
}

#[test]
fn image_free_attachments_deliver_reset_even_when_graphics_are_rejected() {
    let pane = PaneId(7);
    for legacy in [false, true] {
        let snapshot = initial_snapshot(&[pane]);
        let (mut model, _) = paned_model();
        model.update_snapshot(Arc::new(snapshot.clone()));
        model.viewports.insert(pane, placing_viewport(1));
        let (written, output) = mpsc::channel();
        let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
            written.send(bytes.to_vec()).unwrap();
            Ok(())
        }));
        renderer.enable_kitty_graphics();
        renderer.install_kitty_image(KittyImageData {
            pane,
            image_id: 1299,
            generation: 1,
            width: 1,
            height: 1,
            bytes: vec![1, 2, 3, 255],
        });
        renderer.paint(&model, true).unwrap();
        let previous =
            String::from_utf8(output.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
        assert!(previous.contains("\x1b_Ga=p"));
        let core = Mutex::new(ClientCore::new());
        let frames = FrameInbox::default();
        let images = KittyImageInbox::default();
        let (events, incoming) = mpsc::channel();
        let message = if legacy {
            ProtocolMessage::Attached {
                session: zz_protocol::SessionId(1),
                snapshot,
                read_only: false,
                client_flags: String::new(),
            }
        } else {
            ProtocolMessage::Batch(
                zz_protocol::Batch::from_messages(
                    1,
                    vec![
                        initial_event(zz_protocol::EventPayload::Snapshot(snapshot)),
                        initial_view(1),
                    ],
                )
                .unwrap(),
            )
        };
        assert!(forward_protocol_message(
            &core,
            message,
            1,
            &events,
            &frames,
            &images,
            &AtomicU8::new(KITTY_GATE_DISABLED),
            |_| panic!("attach must not need another wire request"),
        ));
        let ready = incoming.try_iter().collect::<Vec<_>>();
        let attached = ready.iter().position(|event| matches!(event, MainEvent::Core { event, .. } if matches!(**event, CoreEvent::Attached { .. }))).unwrap();
        let delivered = ready
            .iter()
            .position(|event| matches!(event, MainEvent::KittyImages(1)))
            .unwrap();
        assert!(attached < delivered);
        let updates = images.take();
        assert!(matches!(updates.as_slice(), [KittyImageUpdate::Reset]));
        apply_kitty_updates(&mut renderer, updates, false);
        renderer.invalidate();
        renderer.paint(&model, true).unwrap();
        let reset =
            String::from_utf8(output.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
        assert!(
            reset.contains("\x1b_Ga=d,d=I"),
            "legacy={legacy}: {reset:?}"
        );
        assert!(!reset.contains("\x1b_Ga=p"), "legacy={legacy}: {reset:?}");
    }
}

#[test]
fn unrelated_request_zero_error_preserves_pending_attach_recovery() {
    let (mut model, _) = paned_model();
    assert_eq!(model.finish_client_focus_attach(), None);
    model.begin_client_focus_attach();
    assert_eq!(model.client_focus_changed(false), None);
    let mut attempt = AttachAttempt::Explicit;

    let unrelated = ServerError::InvalidCommand("unrelated input error".to_owned());
    assert!(!attach_attempt_owns_missing_response(attempt, &unrelated));
    assert!(attempt.is_pending());
    assert!(model.client_focus_attach_pending());

    let missing = ServerError::SessionNotFound("missing".to_owned());
    assert!(attach_attempt_owns_missing_response(attempt, &missing));
    let recovered = model.fail_client_focus_attach();
    attempt = AttachAttempt::Idle;
    assert_eq!(
        recovered,
        Some(InputMessage::ClientFocus { focused: false })
    );
    assert!(!attempt.is_pending());
    assert!(!attach_attempt_owns_missing_response(attempt, &missing));
}
