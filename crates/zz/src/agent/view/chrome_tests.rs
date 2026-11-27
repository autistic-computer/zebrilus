use super::*;

#[test]
fn context_usage_is_bounded_and_handles_an_unknown_window() {
    assert_eq!(context_usage_fraction(0, 0), 0.0);
    assert_eq!(context_usage_fraction(25, 100), 0.25);
    assert_eq!(context_usage_fraction(125, 100), 1.0);
}

#[test]
fn context_usage_tooltip_rounds_the_counts_to_thousands() {
    assert_eq!(
        context_usage_tooltip(84_321, 200_000),
        "84k of 200k context tokens used (42%)"
    );
    assert_eq!(
        context_usage_tooltip(412_000, 1_000_000),
        "412k of 1M context tokens used (41%)"
    );
    assert_eq!(
        context_usage_tooltip(4, 0),
        "Context window usage unavailable"
    );
    let pill = |usage: ContextUsage| (usage.label(), usage.tooltip());
    assert_eq!(
        pill(ContextUsage::reported(Some((84_321, 200_000))).in_empty_chat()),
        (
            "42%".to_owned(),
            "84k of 200k context tokens used (42%)".to_owned()
        )
    );
    assert_eq!(
        pill(ContextUsage::reported(Some((12_000, 0))).in_empty_chat()),
        ("0%".to_owned(), "No context used yet".to_owned())
    );
    assert_eq!(
        pill(ContextUsage::reported(None)),
        (
            "--".to_owned(),
            "Context use shows after the next turn".to_owned()
        )
    );
}

#[test]
fn git_footer_pluralizes_the_file_count() {
    assert_eq!(git_file_count_label(0), "0 files");
    assert_eq!(git_file_count_label(1), "1 file");
    assert_eq!(git_file_count_label(12), "12 files");
}

#[test]
fn busy_chrome_follows_the_connection_and_not_a_tool_row() {
    for connection in [
        AgentConnectionState::Starting,
        AgentConnectionState::Restoring,
        AgentConnectionState::Running,
        AgentConnectionState::Cancelling,
    ] {
        assert!(pane_is_busy(connection));
    }
    for connection in [
        AgentConnectionState::Ready,
        AgentConnectionState::Failed,
        AgentConnectionState::Disconnected,
    ] {
        assert!(!pane_is_busy(connection));
    }
}

#[test]
fn rendered_errors_are_bounded_and_strip_control_bytes() {
    let rendered = rendered_error(&format!("bad\0{}", "x".repeat(4096)));
    assert_eq!(rendered, format!("bad�{}…", "x".repeat(1015)));
    assert_eq!(rendered.len(), MAX_RENDERED_ERROR_BYTES);
}
