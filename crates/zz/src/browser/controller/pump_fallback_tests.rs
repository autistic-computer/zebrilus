use super::*;

#[test]
fn pump_fallback_policy_keeps_only_live_browser_work_active() {
    assert_eq!(
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Initializing),
            ..PumpFallbackState::default()
        }
        .interval(),
        Some(ACTIVE_PUMP_INTERVAL)
    );

    assert_eq!(
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Running),
            work: PumpFallbackWork::ActiveTransition,
            ..PumpFallbackState::default()
        }
        .interval(),
        Some(ACTIVE_PUMP_INTERVAL)
    );

    assert_eq!(
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Running),
            work: PumpFallbackWork::VisibleSession,
            ..PumpFallbackState::default()
        }
        .interval(),
        Some(VISIBLE_PUMP_WATCHDOG_INTERVAL)
    );
}

#[test]
fn pump_fallback_policy_paces_animating_sessions_at_the_frame_interval() {
    assert_eq!(
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Running),
            work: PumpFallbackWork::AnimatingSession,
            frame_interval: pump_frame_interval(240),
            ..PumpFallbackState::default()
        }
        .interval(),
        Some(Duration::from_micros(4166))
    );
    assert_eq!(
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Running),
            work: PumpFallbackWork::AnimatingSession,
            frame_interval: pump_frame_interval(24),
            ..PumpFallbackState::default()
        }
        .interval(),
        Some(VISIBLE_PUMP_WATCHDOG_INTERVAL)
    );
}

#[test]
fn pump_fallback_policy_leaves_inactive_states_lazy() {
    for state in [
        PumpFallbackState::default(),
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Running),
            ..PumpFallbackState::default()
        },
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Closed),
            work: PumpFallbackWork::VisibleSession,
            ..PumpFallbackState::default()
        },
        PumpFallbackState {
            runtime_phase: Some(RuntimePhase::Running),
            work: PumpFallbackWork::VisibleSession,
            shutting_down: true,
            ..PumpFallbackState::default()
        },
    ] {
        assert_eq!(state.interval(), None);
    }
}
