use super::test_support::context;
use super::*;

#[test]
fn vocabulary_is_the_sorted_pinned_214_entry_table() {
    assert_eq!(FORMAT_VARIABLES.len(), 214);
    assert!(
        FORMAT_VARIABLES
            .windows(2)
            .all(|pair| pair[0].name < pair[1].name)
    );
    let times = FORMAT_VARIABLES
        .iter()
        .filter(|variable| variable.kind == FormatKind::Time)
        .map(|variable| variable.name)
        .collect::<Vec<_>>();
    assert_eq!(
        times,
        [
            "buffer_created",
            "client_activity",
            "client_created",
            "pane_command_end_time",
            "pane_command_start_time",
            "pane_dead_time",
            "pane_last_output_time",
            "pane_last_prompt_time",
            "session_activity",
            "session_created",
            "session_last_attached",
            "start_time",
            "window_activity",
        ]
    );
    for name in [
        "buffer_created",
        "buffer_full",
        "buffer_name",
        "buffer_sample",
        "buffer_size",
    ] {
        assert_eq!(
            format_variable(name).map(|variable| variable.backing),
            Some(FormatBacking::StatusHook),
        );
    }
    let context = context();
    for variable in &FORMAT_VARIABLES {
        let value = context
            .variable(variable.name)
            .unwrap_or_else(|| panic!("missing {}", variable.name));
        match variable.backing {
            FormatBacking::Empty => assert!(value.is_empty(), "{}", variable.name),
            FormatBacking::Zero => assert_eq!(value, "0", "{}", variable.name),
            FormatBacking::One => assert_eq!(value, "1", "{}", variable.name),
            _ => {}
        }
    }
    assert!(context.variable("not_a_tmux_variable").is_none());
    assert_eq!(
        context.variable("config_files").as_deref(),
        Some("/tmp/first.conf,/tmp/second.conf")
    );
    assert_eq!(
        StatusContext::default().variable("config_files").as_deref(),
        Some("")
    );
}
