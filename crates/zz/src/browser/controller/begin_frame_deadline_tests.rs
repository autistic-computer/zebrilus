use super::*;

const fn test_browser_key(pane: u64, tab: u64) -> BrowserKey {
    (PaneId(pane), TabId(tab))
}

#[test]
fn begin_frame_deadline_skips_missed_intervals_without_bursting() {
    let started_at = Instant::now();
    let interval = Duration::from_millis(10);
    let mut deadlines = BTreeMap::new();

    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        started_at,
        interval,
        true,
    ));
    let late_turn = started_at
        .checked_add(Duration::from_millis(95))
        .expect("test instant is representable");
    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        late_turn,
        interval,
        true,
    ));
    assert_eq!(
        deadlines[&test_browser_key(1, 0)].next,
        started_at
            .checked_add(Duration::from_millis(100))
            .expect("test instant is representable")
    );
    assert!(!external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        late_turn,
        interval,
        true,
    ));
}

#[test]
fn begin_frame_deadline_sends_and_anchors_on_hot_edges() {
    let started_at = Instant::now();
    let cold_interval = VISIBLE_PUMP_WATCHDOG_INTERVAL;
    let hot_interval = pump_frame_interval(240);
    let mut deadlines = BTreeMap::new();

    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        started_at,
        cold_interval,
        false,
    ));
    let hot_at = started_at
        .checked_add(Duration::from_millis(5))
        .expect("test instant is representable");
    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        hot_at,
        hot_interval,
        true,
    ));
    assert_eq!(
        deadlines[&test_browser_key(1, 0)].next,
        hot_at
            .checked_add(hot_interval)
            .expect("test instant is representable")
    );
}

#[test]
fn begin_frame_deadline_reanchors_hot_cold_and_ceiling_changes() {
    let started_at = Instant::now();
    let hot_interval = pump_frame_interval(240);
    let slower_hot_interval = pump_frame_interval(120);
    let mut deadlines = BTreeMap::new();

    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        started_at,
        hot_interval,
        true,
    ));
    let ceiling_changed_at = started_at
        .checked_add(Duration::from_millis(1))
        .expect("test instant is representable");
    assert!(!external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        ceiling_changed_at,
        slower_hot_interval,
        true,
    ));
    assert_eq!(
        deadlines[&test_browser_key(1, 0)].next,
        ceiling_changed_at
            .checked_add(slower_hot_interval)
            .expect("test instant is representable")
    );

    let cold_at = ceiling_changed_at
        .checked_add(Duration::from_millis(1))
        .expect("test instant is representable");
    assert!(!external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        cold_at,
        VISIBLE_PUMP_WATCHDOG_INTERVAL,
        false,
    ));
    assert_eq!(
        deadlines[&test_browser_key(1, 0)].next,
        cold_at
            .checked_add(VISIBLE_PUMP_WATCHDOG_INTERVAL)
            .expect("test instant is representable")
    );

    let hot_again_at = cold_at
        .checked_add(Duration::from_millis(1))
        .expect("test instant is representable");
    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        hot_again_at,
        slower_hot_interval,
        true,
    ));
}

#[test]
fn begin_frame_deadline_selects_earliest_browser_tab() {
    let started_at = Instant::now();
    let mut deadlines = BTreeMap::new();
    let later = started_at
        .checked_add(Duration::from_millis(12))
        .expect("test instant is representable");
    let earlier = started_at
        .checked_add(Duration::from_millis(4))
        .expect("test instant is representable");
    deadlines.insert(
        test_browser_key(1, 0),
        ExternalBeginFrameDeadline {
            next: later,
            interval: Duration::from_millis(4),
            hot: true,
        },
    );
    deadlines.insert(
        test_browser_key(1, 1),
        ExternalBeginFrameDeadline {
            next: earlier,
            interval: Duration::from_millis(8),
            hot: true,
        },
    );

    assert_eq!(
        earliest_external_begin_frame_deadline(&deadlines),
        Some(earlier)
    );
}

#[test]
fn begin_frame_deadline_preserves_exact_cadence_across_late_turns() {
    let started_at = Instant::now();
    let interval = Duration::from_millis(10);
    let mut deadlines = BTreeMap::new();
    assert!(external_begin_frame_due(
        &mut deadlines,
        test_browser_key(1, 0),
        started_at,
        interval,
        true,
    ));

    for (late_ms, next_ms) in [(12, 20), (23, 30), (31, 40), (44, 50)] {
        let late_turn = started_at
            .checked_add(Duration::from_millis(late_ms))
            .expect("test instant is representable");
        assert!(external_begin_frame_due(
            &mut deadlines,
            test_browser_key(1, 0),
            late_turn,
            interval,
            true,
        ));
        assert_eq!(
            deadlines[&test_browser_key(1, 0)].next,
            started_at
                .checked_add(Duration::from_millis(next_ms))
                .expect("test instant is representable")
        );
    }
}
