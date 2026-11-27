use super::test_support::{paned_model, with_row_text};
use super::*;

#[test]
fn only_a_snapshot_that_moves_the_frame_asks_for_a_full_repaint() {
    let (mut model, pane) = paned_model();
    let before = model.paint_structure();
    let mut renamed = (*model.snapshot).clone();
    renamed.generation = 2;
    renamed.sessions[0].windows[0].name = "renamed".to_owned();
    renamed.sessions[0].windows[0].status_label = "0:renamed*".to_owned();
    model.update_snapshot(Arc::new(renamed.clone()));
    assert_eq!(model.paint_structure(), before);

    let mut split = renamed;
    split.generation = 3;
    split.sessions[0].windows[0].layout = zz_protocol::LayoutNode::Split {
        id: zz_protocol::SplitId(1),
        axis: zz_protocol::Axis::Horizontal,
        ratio: 0.5,
        first: Box::new(zz_protocol::LayoutNode::Pane(pane)),
        second: Box::new(zz_protocol::LayoutNode::Pane(zz_protocol::PaneId(8))),
    };
    model.update_snapshot(Arc::new(split));
    assert_ne!(model.paint_structure(), before);
}

#[test]
fn a_drained_run_of_events_owes_the_strongest_paint_once() {
    let paints = [
        PendingPaint::Frames,
        PendingPaint::None,
        PendingPaint::RepaintAll,
        PendingPaint::Repaint,
    ];
    assert_eq!(
        paints
            .into_iter()
            .fold(PendingPaint::None, PendingPaint::max),
        PendingPaint::RepaintAll
    );
    assert!(PendingPaint::Frames < PendingPaint::Repaint);
}

#[test]
fn a_drained_run_of_frames_paints_every_row_any_of_them_changed() {
    let (mut model, pane) = paned_model();
    let mut snapshot = (*model.snapshot).clone();
    snapshot.generation = 2;
    snapshot.sessions[0].windows[0].panes.insert(
        pane,
        zz_protocol::PaneSnapshot {
            id: pane,
            title: "shell".to_owned(),
            kind: zz_protocol::PaneKindSnapshot::Terminal,
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
    model.update_snapshot(Arc::new(snapshot));
    let content = model.layout.panes[0].content();
    let prompt = with_row_text(
        &TerminalViewport::blank(
            content.width,
            content.height,
            zz_terminal::SessionStatus::Running,
        ),
        1,
        "$",
    );
    model.viewports.insert(pane, prompt.clone());
    let (send, receive) = mpsc::channel();
    let mut renderer = Renderer::with_sink(Box::new(move |bytes| {
        send.send(bytes.to_vec()).expect("paint receiver");
        Ok(())
    }));
    let wait = Duration::from_secs(5);
    renderer.paint(&model, true).expect("attach paint");
    receive.recv_timeout(wait).expect("attach paint written");

    let inbox = FrameInbox::default();
    let (events, _incoming) = mpsc::channel();
    let typed = with_row_text(&prompt, 1, "$ printf 'MARK-%s' split");
    inbox.publish(pane, typed.clone(), FrameDamage::Rows(vec![1]), 7, &events);
    let capacity = inbox.0.lock().unwrap().pending.capacity();
    take_frames(&inbox, &mut model, &mut renderer);
    assert_eq!(inbox.0.lock().unwrap().pending.capacity(), capacity);
    let answered = with_row_text(&with_row_text(&typed, 2, "MARK-split"), 3, "$");
    inbox.publish(pane, answered, FrameDamage::Rows(vec![2, 3]), 7, &events);
    assert_eq!(inbox.0.lock().unwrap().pending.capacity(), capacity);
    take_frames(&inbox, &mut model, &mut renderer);
    assert_eq!(inbox.0.lock().unwrap().pending.capacity(), capacity);
    renderer.paint_frames(&model).expect("drained paint");

    let painted = String::from_utf8(receive.recv_timeout(wait).expect("drained paint written"))
        .expect("paint is UTF-8");
    assert!(
        painted.contains(" printf 'MARK-%s' split"),
        "the row only the first frame changed reaches the tty: {painted:?}"
    );
    assert!(
        !painted.contains("$ printf"),
        "the unchanged prompt cell is preserved: {painted:?}"
    );
    assert!(painted.contains("MARK-split"), "{painted:?}");
}
