use super::*;

#[test]
fn frame_inbox_keeps_only_the_latest_viewport_per_pane() {
    let inbox = FrameInbox::default();
    let (events, incoming) = mpsc::channel();
    let first = TerminalViewport::blank(80, 24, zz_terminal::SessionStatus::Running);
    let second = TerminalViewport::blank(120, 40, zz_terminal::SessionStatus::Running);

    inbox.publish(PaneId(1), first, FrameDamage::Rows(vec![1]), 7, &events);
    inbox.publish(PaneId(1), second, FrameDamage::Rows(vec![2]), 7, &events);

    assert!(matches!(incoming.recv().unwrap(), MainEvent::Frames(7)));
    assert!(incoming.try_recv().is_err());
    let pending = inbox.take();
    assert_eq!(pending[&PaneId(1)].viewport.columns, 120);
    assert_eq!(pending[&PaneId(1)].damage, FrameDamage::Rows(vec![1, 2]));
}

#[test]
fn frame_capacity_return_keeps_new_publication_and_attachment_reset() {
    for reset in [false, true] {
        let inbox = FrameInbox::default();
        let (events, incoming) = mpsc::channel();
        let first = TerminalViewport::blank(80, 24, zz_terminal::SessionStatus::Running);
        for pane in 1..=16 {
            inbox.publish(PaneId(pane), first.clone(), FrameDamage::All, 7, &events);
        }
        assert!(matches!(incoming.recv().unwrap(), MainEvent::Frames(7)));
        let mut completed = inbox.take();
        let capacity = completed.capacity();
        completed.clear();
        if reset {
            inbox.clear();
        }
        inbox.publish(PaneId(1), first, FrameDamage::Rows(vec![1]), 7, &events);
        inbox.publish(
            PaneId(1),
            TerminalViewport::blank(120, 40, zz_terminal::SessionStatus::Running),
            FrameDamage::Rows(vec![2]),
            7,
            &events,
        );
        inbox.recycle(completed);
        {
            let state = inbox.0.lock().unwrap();
            assert_eq!(state.pending.capacity(), capacity);
            assert_eq!(state.pending.len(), 1);
            assert!(state.wake_pending);
        }
        assert!(matches!(incoming.recv().unwrap(), MainEvent::Frames(7)));
        assert!(incoming.try_recv().is_err());
        let mut pending = inbox.take();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[&PaneId(1)].viewport.columns, 120);
        assert_eq!(pending[&PaneId(1)].damage, FrameDamage::Rows(vec![1, 2]));
        pending.clear();
        inbox.recycle(pending);
        let state = inbox.0.lock().unwrap();
        assert!(state.pending.is_empty());
        assert_eq!(state.pending.capacity(), capacity);
        assert!(!state.wake_pending);
    }
}

#[test]
fn frame_inbox_removes_a_retired_synthetic_pane_before_delivery() {
    let inbox = FrameInbox::default();
    let (events, incoming) = mpsc::channel();
    inbox.publish(
        PaneId(1),
        TerminalViewport::blank(20, 8, zz_terminal::SessionStatus::Running),
        FrameDamage::All,
        7,
        &events,
    );
    inbox.publish(
        PaneId(2),
        TerminalViewport::blank(30, 10, zz_terminal::SessionStatus::Running),
        FrameDamage::All,
        7,
        &events,
    );
    inbox.remove(PaneId(1));

    assert!(matches!(incoming.recv().unwrap(), MainEvent::Frames(7)));
    let pending = inbox.take();
    assert!(!pending.contains_key(&PaneId(1)));
    assert!(pending.contains_key(&PaneId(2)));
}
