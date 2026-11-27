use super::*;

#[test]
fn a_page_left_cold_gets_slower_begin_frames() {
    let start = Instant::now();
    let now = start + Duration::from_secs(10);
    let recent = start + Duration::from_millis(9_500);
    let stale = start + Duration::from_millis(7_999);
    assert_eq!(
        cold_external_begin_frame_interval(Some(now + Duration::from_millis(1)), now),
        VISIBLE_PUMP_WATCHDOG_INTERVAL
    );
    assert_eq!(
        cold_external_begin_frame_interval(Some(recent), now),
        VISIBLE_PUMP_WATCHDOG_INTERVAL
    );
    assert_eq!(
        cold_external_begin_frame_interval(Some(stale), now),
        EXTERNAL_BEGIN_FRAME_COLD_INTERVAL
    );
    assert_eq!(
        cold_external_begin_frame_interval(None, now),
        EXTERNAL_BEGIN_FRAME_COLD_INTERVAL
    );
}

#[test]
fn display_frame_rate_ceiling_uses_fastest_reported_rate_and_rounds() {
    assert_eq!(
        select_display_frame_rate_ceiling(vec![None, Some(59.94), Some(143.856)]),
        144
    );
}

#[test]
fn display_frame_rate_ceiling_falls_back_without_reported_rates() {
    assert_eq!(
        select_display_frame_rate_ceiling(vec![None, None]),
        DEFAULT_DISPLAY_FRAME_RATE_CEILING
    );
    assert_eq!(
        select_display_frame_rate_ceiling(Vec::new()),
        DEFAULT_DISPLAY_FRAME_RATE_CEILING
    );
}

#[test]
fn display_frame_rate_ceiling_clamps_to_cef_bounds() {
    assert_eq!(select_display_frame_rate_ceiling(vec![Some(0.4)]), 1);
    assert_eq!(
        select_display_frame_rate_ceiling(vec![Some(360.0)]),
        MAX_DISPLAY_FRAME_RATE_CEILING
    );
}

#[test]
fn pane_frame_rate_caps_unfocused_animation_unless_wheel_boosted() {
    assert_eq!(effective_pane_frame_rate(120, true, false), 120);
    assert_eq!(effective_pane_frame_rate(120, false, true), 120);
    assert_eq!(effective_pane_frame_rate(120, false, false), 30);
    assert_eq!(effective_pane_frame_rate(24, false, false), 24);
}

#[test]
fn first_frame_watchdog_pauses_and_rearms_without_stale_timeouts() {
    let session = SessionId(17);
    let mut watchdog = FirstFrameWatchdog::new(session);

    let first_generation = watchdog.arm().expect("first arm starts a timer");
    assert!(watchdog.matches(session, first_generation));
    assert_eq!(watchdog.arm(), None);

    watchdog.pause();
    assert!(!watchdog.matches(session, first_generation));
    let second_generation = watchdog.arm().expect("resume starts a new timer");
    assert_ne!(second_generation, first_generation);
    assert!(watchdog.matches(session, second_generation));
    assert!(!watchdog.matches(SessionId(18), second_generation));
}

#[test]
fn display_frame_rate_ceiling_retries_after_empty_probe() {
    for unavailable_rates in [Vec::new(), vec![None, None]] {
        let first_probe = reported_display_frame_rate_ceiling(unavailable_rates);
        let (fallback, cached) = resolve_frame_rate_ceiling_value(None, first_probe);
        assert_eq!(fallback, DEFAULT_DISPLAY_FRAME_RATE_CEILING);
        assert_eq!(cached, None);

        let retry_probe = reported_display_frame_rate_ceiling(vec![Some(239.914)]);
        let (resolved, cached) = resolve_frame_rate_ceiling_value(cached, retry_probe);
        assert_eq!(resolved, MAX_DISPLAY_FRAME_RATE_CEILING);
        assert_eq!(cached, Some(MAX_DISPLAY_FRAME_RATE_CEILING));
    }
}
