use super::test_support::{
    engine_with_session, output, press, preview_text, run, select, type_text,
};
use super::*;

#[test]
fn customize_lists_hooks_and_environment_in_their_own_sections_like_3_8() {
    let (engine, _, pane) = engine_with_session();
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &CustomizeMode::default(), &mut expand);
    let sections = rows
        .iter()
        .filter(|row| row.parent.is_none())
        .map(|row| row.name.as_str())
        .take(7)
        .collect::<Vec<_>>();
    assert_eq!(
        sections,
        [
            "Server Options",
            "Session Options",
            "Window & Pane Options",
            "Session Hooks",
            "Window & Pane Hooks",
            "Global Environment",
            "Session Environment",
        ]
    );
    let in_section = |title: &str| {
        let section = rows.iter().position(|row| row.name == title).unwrap();
        rows.iter()
            .filter(|row| row.parent == Some(section))
            .collect::<Vec<_>>()
    };
    let options = ["Server Options", "Session Options", "Window & Pane Options"]
        .into_iter()
        .flat_map(in_section)
        .collect::<Vec<_>>();
    assert_eq!(
        options
            .iter()
            .filter(|row| matches!(
                &row.item,
                Item::Option { metadata: Some(option), .. } if option.is_array
            ))
            .count(),
        8
    );
    assert!(!options.iter().any(|row| tmux_option_is_hook(&row.name)));
    let session_hooks = in_section("Session Hooks");
    let window_hooks = in_section("Window & Pane Hooks");
    assert!(
        session_hooks
            .iter()
            .any(|row| row.name == "after-new-session")
    );
    assert!(window_hooks.iter().any(|row| row.name == "pane-exited"));
    assert!(window_hooks.iter().any(|row| row.name == "window-renamed"));
    assert_eq!(
        session_hooks.len() + window_hooks.len(),
        tmux_options()
            .filter(|option| tmux_option_is_hook(option.name))
            .count()
    );
    assert!(
        session_hooks
            .iter()
            .chain(&window_hooks)
            .all(|row| matches!(row.item, Item::Option { hook: true, .. }) && row.text.is_none())
    );
}

#[test]
fn customize_classifies_a_user_option_by_its_effective_entry() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "-B", "@mon:%*:#{pane_id}"],
    );
    run(
        &mut engine,
        &mut context,
        "set-option",
        &["-t", "customize", "@mon", "plain"],
    );
    let mut mode = CustomizeMode::default();
    for section in 0..5 {
        mode.expanded.insert(format!("options:{section}"));
    }
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let sections = rows
        .iter()
        .filter(|row| row.name == "@mon")
        .map(|row| rows[row.parent.unwrap()].name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(sections, ["Session Options"]);
}

#[test]
fn customize_changed_only_compares_an_arrays_printed_values() {
    let (mut engine, mut context, pane) = engine_with_session();
    let values =
        engine.customize_array_entries(TmuxOptionTarget::GlobalSession, "update-environment");
    let (last_key, last_value) = values.last().cloned().unwrap();
    run(
        &mut engine,
        &mut context,
        "set-option",
        &["-g", "update-environment[100]", &last_value],
    );
    run(
        &mut engine,
        &mut context,
        "set-option",
        &["-gu", &format!("update-environment[{last_key}]")],
    );
    let mode = CustomizeMode {
        changed_only: true,
        ..CustomizeMode::default()
    };
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    assert!(!rows.iter().any(|row| row.name == "update-environment"));
}

#[test]
fn customize_environment_rows_resolve_the_3_8_context_formats() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-environment",
        &["-g", "ZZTEST_A", "one"],
    );
    run(
        &mut engine,
        &mut context,
        "set-environment",
        &["-g", "-r", "ZZTEST_R"],
    );
    run(
        &mut engine,
        &mut context,
        "set-environment",
        &["-h", "ZZTEST_H", "secret"],
    );
    let mut mode = CustomizeMode {
        filter: Some("#{&&:#{is_environment},#{m:ZZTEST*,#{environment_name}}}".to_owned()),
        format: Some(
            "#{environment_name}=#{environment_value}/#{environment_is_global}/#{environment_hidden}/#{environment_removed}/#{environment_scope}/#{is_option}#{is_key}".to_owned(),
        ),
        ..CustomizeMode::default()
    };
    mode.expanded.insert("environment:global".to_owned());
    mode.expanded.insert("environment:session".to_owned());
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let environment = rows
        .iter()
        .filter(|row| matches!(row.item, Item::Environment { .. }))
        .map(|row| (row.name.as_str(), row.text.as_deref()))
        .collect::<Vec<_>>();
    assert_eq!(
        environment,
        [
            ("ZZTEST_A", Some("ZZTEST_A=one/1/0/0//00")),
            ("-ZZTEST_R", None),
            (
                "ZZTEST_H",
                Some("ZZTEST_H=secret/0/1/0/session customize/00")
            ),
        ]
    );
    mode.filter = None;
    mode.format = None;
    select(&engine, pane, &mut mode, "ZZTEST_H");
    let preview = preview_text(&engine, pane, &mode);
    assert!(
        preview.contains("This is a session environment variable."),
        "{preview}"
    );
    assert!(preview.contains("This variable is hidden."), "{preview}");
    assert!(
        preview.contains("Variable value: #[fg=themelightgrey]secret"),
        "{preview}"
    );
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        mode.prompt
            .as_ref()
            .map(|(prompt, _)| prompt.label.as_str()),
        Some("(ZZTEST_H, for session customize) ")
    );
    press(&mut engine, &mut context, pane, &mut mode, "C-u");
    type_text(&mut engine, &mut context, pane, &mut mode, "changed");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-environment",
            &["-h", "ZZTEST_H"]
        ),
        "ZZTEST_H=changed"
    );
    select(&engine, pane, &mut mode, "Global Environment");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        mode.prompt
            .as_ref()
            .map(|(prompt, _)| prompt.label.as_str()),
        Some("New environment: ")
    );
    type_text(
        &mut engine,
        &mut context,
        pane,
        &mut mode,
        "ZZTEST_NEW=fresh",
    );
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-environment",
            &["-g", "ZZTEST_NEW"]
        ),
        "ZZTEST_NEW=fresh"
    );
}

#[test]
fn customize_hook_and_monitor_rows_resolve_the_3_8_context_formats() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "-B", "@mon:%*:#{pane_id}"],
    );
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "@event", "display-message hi"],
    );
    let mut mode = CustomizeMode {
        filter: Some("#{||:#{option_is_hook},#{option_is_monitor}}".to_owned()),
        format: Some(
            "#{option_name}:#{option_is_hook}:#{option_is_monitor}:#{option_monitor}:#{is_environment}".to_owned(),
        ),
        ..CustomizeMode::default()
    };
    for section in 0..5 {
        mode.expanded.insert(format!("options:{section}"));
    }
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let texts = rows
        .iter()
        .filter(|row| matches!(row.item, Item::Option { .. }))
        .filter_map(|row| row.text.as_deref())
        .collect::<Vec<_>>();
    assert!(
        texts.contains(&"@mon:0:1:@mon:%*:#{pane_id}:0"),
        "{texts:?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.name == "after-new-session" && row.text.is_none())
    );
    assert!(!rows.iter().any(|row| row.name == "@event"));
    mode.filter = None;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let event = rows.iter().find(|row| row.name == "@event").unwrap();
    assert_eq!(event.text.as_deref(), Some("@event:0:0::0"));
    let in_hooks = |name: &str| {
        let row = rows.iter().find(|row| row.name == name).unwrap();
        rows[row.parent.unwrap()].name.clone()
    };
    assert_eq!(in_hooks("@mon"), "Session Hooks");
    assert_eq!(in_hooks("@event"), "Session Hooks");
    mode.format = None;
    select(&engine, pane, &mut mode, "@mon");
    let preview = preview_text(&engine, pane, &mode);
    assert!(
        preview.contains("This hook runs when a monitor changes."),
        "{preview}"
    );
    assert!(preview.contains("This is a monitor hook."), "{preview}");
    assert!(
        preview.contains("Monitor: #[fg=themelightgrey]@mon:%*:##{pane_id}"),
        "{preview}"
    );
}

#[test]
fn customize_changed_only_keeps_changed_rows_and_drops_environment() {
    let (mut engine, mut context, pane) = engine_with_session();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "status-left", "changed"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-hook", ["-g", "after-new-window", "display-message hi"]),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("bind-key", ["-T", "prefix", "F7", "display-message seven"]),
        )
        .unwrap();
    let mut mode = CustomizeMode::default();
    press(&mut engine, &mut context, pane, &mut mode, "C");
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    let names = rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>();
    assert!(names.contains(&"status-left"));
    assert!(!names.contains(&"status-right"));
    assert!(names.contains(&"after-new-window"));
    assert!(names.contains(&"after-new-window[0]"));
    assert!(!names.contains(&"after-new-session"));
    assert!(names.contains(&"F7"));
    assert!(!names.contains(&"Global Environment"));
    assert!(!names.contains(&"Session Environment"));
    assert!(!names.contains(&"Server Options"));
    press(&mut engine, &mut context, pane, &mut mode, "C");
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    assert!(rows.iter().any(|row| row.name == "Global Environment"));
}
