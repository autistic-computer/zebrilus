use super::*;

fn record_adaptive_window(
    throttle: &mut AdaptiveBeginFrameThrottle,
    begin_frames_sent: u64,
    frames_delivered: u64,
) {
    for _ in 0..begin_frames_sent {
        throttle.record_begin_frame();
    }
    for _ in 0..frames_delivered {
        throttle.record_frame_delivered();
    }
}

fn test_instant_after(started_at: Instant, elapsed: Duration) -> Instant {
    started_at
        .checked_add(elapsed)
        .expect("test instant is representable")
}

#[test]
fn adaptive_begin_frame_throttle_downshifts_after_sustained_shortfall() {
    let started_at = Instant::now();
    let mut throttle = AdaptiveBeginFrameThrottle::default();
    throttle.set_hot(true, started_at);
    record_adaptive_window(&mut throttle, 100, 80);

    assert!(throttle.update_tier(
        test_instant_after(started_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW),
        240,
    ));
    assert_eq!(throttle.divisor, 2);
    assert_eq!(throttle.effective_frame_rate(240), 120);
}

#[test]
fn adaptive_begin_frame_throttle_does_not_flap_on_one_slow_frame() {
    let started_at = Instant::now();
    let mut throttle = AdaptiveBeginFrameThrottle::default();
    throttle.set_hot(true, started_at);
    record_adaptive_window(&mut throttle, 120, 119);

    assert!(!throttle.update_tier(
        test_instant_after(started_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW),
        240,
    ));
    assert_eq!(throttle.divisor, 1);
}

#[test]
fn adaptive_begin_frame_throttle_probes_faster_after_stability() {
    let started_at = Instant::now();
    let mut throttle = AdaptiveBeginFrameThrottle::default();
    throttle.set_hot(true, started_at);
    record_adaptive_window(&mut throttle, 100, 80);
    let downshifted_at = test_instant_after(started_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW);
    assert!(throttle.update_tier(downshifted_at, 240));
    assert_eq!(throttle.divisor, 2);

    record_adaptive_window(&mut throttle, 120, 120);
    let first_healthy_window =
        test_instant_after(downshifted_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW);
    assert!(!throttle.update_tier(first_healthy_window, 240));
    assert_eq!(throttle.divisor, 2);

    record_adaptive_window(&mut throttle, 120, 114);
    let second_healthy_window =
        test_instant_after(first_healthy_window, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW);
    assert!(throttle.update_tier(second_healthy_window, 240));
    assert_eq!(throttle.divisor, 1);
}

#[test]
fn adaptive_begin_frame_throttle_respects_rate_bounds() {
    let started_at = Instant::now();
    let mut throttle = AdaptiveBeginFrameThrottle::default();
    throttle.set_hot(true, started_at);
    let mut sampled_at = started_at;
    for expected_divisor in [2, 4, 8] {
        record_adaptive_window(&mut throttle, 100, 50);
        sampled_at = test_instant_after(sampled_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW);
        assert!(throttle.update_tier(sampled_at, 240));
        assert_eq!(throttle.divisor, expected_divisor);
    }
    assert_eq!(throttle.effective_frame_rate(240), 30);
    record_adaptive_window(&mut throttle, 100, 50);
    sampled_at = test_instant_after(sampled_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW);
    assert!(!throttle.update_tier(sampled_at, 240));
    assert_eq!(throttle.divisor, 8);

    let mut low_ceiling = AdaptiveBeginFrameThrottle::default();
    low_ceiling.set_hot(true, started_at);
    record_adaptive_window(&mut low_ceiling, 24, 10);
    assert!(!low_ceiling.update_tier(
        test_instant_after(started_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW),
        24,
    ));
    assert_eq!(low_ceiling.effective_frame_rate(24), 24);
}

#[test]
fn adaptive_begin_frame_throttle_ignores_idle_windows() {
    let started_at = Instant::now();
    let mut throttle = AdaptiveBeginFrameThrottle::default();
    throttle.set_hot(true, started_at);
    record_adaptive_window(&mut throttle, 200, 0);

    assert!(!throttle.update_tier(
        test_instant_after(started_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW),
        240,
    ));
    assert_eq!(throttle.divisor, 1);
}

#[test]
fn adaptive_begin_frame_throttle_resets_divisor_when_cold() {
    let started_at = Instant::now();
    let mut throttle = AdaptiveBeginFrameThrottle::default();
    throttle.set_hot(true, started_at);
    record_adaptive_window(&mut throttle, 100, 50);
    let downshifted_at = test_instant_after(started_at, ADAPTIVE_BEGIN_FRAME_SAMPLE_WINDOW);
    assert!(throttle.update_tier(downshifted_at, 240));
    assert_eq!(throttle.divisor, 2);

    throttle.set_hot(false, downshifted_at);
    throttle.set_hot(true, downshifted_at);
    assert_eq!(throttle.divisor, 1);
    assert_eq!(throttle.effective_frame_rate(240), 240);
}
