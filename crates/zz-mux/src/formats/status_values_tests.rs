use super::test_support::{Stub, context, expand};
use super::*;

#[test]
fn scoped_pane_values_include_terminal_and_native_names_without_session_names() {
    let values = context().scoped_format_values("pane", &mut Stub);
    assert_eq!(
        values.get("pane_id").and_then(serde_json::Value::as_str),
        Some("%7")
    );
    assert!(values.get("alternate_on").is_some());
    assert!(values.get("pane_kind").is_some());
    assert_eq!(
        values
            .get("pane_format")
            .and_then(serde_json::Value::as_str),
        Some("1")
    );
    assert!(values.get("session_name").is_none());
    assert_eq!(
        values
            .get("agent_state")
            .and_then(serde_json::Value::as_str),
        Some("")
    );
}

#[test]
fn scoped_values_resolve_hooks_before_context_values() {
    struct Hooks;

    impl StatusHooks for Hooks {
        fn strftime(&mut self, _literal: &str) -> String {
            String::new()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            match name {
                "pane_id" => Some("%42".to_owned()),
                "pane_kind" => Some("agent".to_owned()),
                _ => None,
            }
        }
    }

    let context = context();
    let values = context.scoped_format_values("pane", &mut Hooks);
    for name in ["pane_id", "pane_kind", "pane_dead_time", "agent_state"] {
        assert_eq!(
            values[name].as_str().unwrap(),
            expand_format_values(&format!("#{{{name}}}"), &context, &mut Hooks)
        );
    }
    assert_eq!(values["pane_id"], "%42");
    assert_eq!(values["pane_kind"], "agent");
}

#[test]
fn pin_null_backings_are_empty_instead_of_zero() {
    let context = context();
    for name in [
        "client_cell_height",
        "client_cell_width",
        "client_control_mode",
        "client_discarded",
        "client_height",
        "client_pid",
        "client_prefix",
        "client_readonly",
        "client_uid",
        "client_utf8",
        "client_width",
        "client_written",
        "mouse_x",
        "mouse_y",
        "pane_pipe_pid",
        "session_group_attached",
        "session_group_many_attached",
        "session_group_size",
        "window_bigger",
        "window_manual_height",
        "window_manual_width",
    ] {
        assert_eq!(context.variable(name).as_deref(), Some(""), "{name}");
    }
}

#[test]
fn session_active_expands_null_false_and_true_as_a_boolean() {
    for (value, expected) in [
        (None, "|no|0"),
        (Some(false), "0|no|0"),
        (Some(true), "1|yes|1"),
    ] {
        let mut context = context();
        context.session_active = value;
        assert_eq!(
            expand_format_values(
                "#{session_active}|#{?session_active,yes,no}|#{!!:#{session_active}}",
                &context,
                &mut Stub,
            ),
            expected
        );
    }
}

#[test]
fn session_activity_expands_as_seconds_and_time() {
    assert_eq!(expand("#{session_activity}"), "1700000000");
    assert_eq!(
        expand_format_values("#{t/f/%Y:session_activity}", &context(), &mut Stub),
        "2023"
    );
    assert!(
        expand_format_values("#{t:session_activity}", &context(), &mut Stub).ends_with(" 2023")
    );
}

#[test]
fn window_activity_keeps_plain_boolean_equality_and_time_views_consistent() {
    assert_eq!(
        expand_format_values(
            "#{window_activity}|#{?window_activity,yes,no}|#{==:#{window_activity},1700000000}|#{t/f/%Y:window_activity}",
            &context(),
            &mut Stub,
        ),
        "1700000000|yes|1|2023"
    );

    let mut context = context();
    context.window_activity = None;
    assert_eq!(
        expand_format_values(
            "#{window_activity}|#{?window_activity,yes,no}|#{==:#{window_activity},}",
            &context,
            &mut Stub,
        ),
        "|no|1"
    );
    assert_eq!(
        StatusContext::default()
            .variable("window_activity")
            .as_deref(),
        Some("")
    );
}
