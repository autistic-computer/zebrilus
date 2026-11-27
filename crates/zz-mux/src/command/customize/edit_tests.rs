use super::test_support::{
    current_row, engine_with_session, output, press, preview_text, run, select, type_text,
};
use super::*;

#[test]
fn customize_array_edits_preserve_entries_and_fill_the_first_hole() {
    let (mut engine, mut context, pane) = engine_with_session();
    for option in
        tmux_options().filter(|option| option.is_array && !tmux_option_is_hook(option.name))
    {
        let target = match option.scope {
            TmuxOptionScope::Server => TmuxOptionTarget::Server,
            TmuxOptionScope::Session => TmuxOptionTarget::GlobalSession,
            TmuxOptionScope::Window | TmuxOptionScope::WindowPane => TmuxOptionTarget::GlobalWindow,
        };
        let value = match option.name {
            "pane-colours" => "red",
            "command-alias" => "review=display-message review",
            "codepoint-widths" => "U+0041=1",
            _ => "review-value",
        };
        let indexed = format!("{}[100]", option.name);
        engine
            .execute(
                &mut context,
                &customize_set_command(&indexed, target, value),
            )
            .unwrap();
        let mut expected = engine.customize_array_values(target, option.name);
        let hole = first_free_array_index(expected.keys()).unwrap();
        let mut mode = CustomizeMode::default();
        mode.expanded
            .extend((0..3).map(|index| format!("options:{index}")));
        select(&engine, pane, &mut mode, option.name);
        press(&mut engine, &mut context, pane, &mut mode, "W");
        assert!(
            matches!(&mode.prompt, Some((prompt, _)) if prompt.label.contains("[+]")),
            "{}",
            option.name
        );
        press(&mut engine, &mut context, pane, &mut mode, "C-u");
        type_text(&mut engine, &mut context, pane, &mut mode, value);
        press(&mut engine, &mut context, pane, &mut mode, "Enter");
        expected.insert(ArrayIndex::Numeric(hole), value.to_owned());
        assert_eq!(
            engine.customize_array_values(target, option.name),
            expected,
            "{} root",
            option.name
        );
        mode.expanded.insert(current_row(&engine, pane, &mode).id);
        select(&engine, pane, &mut mode, &indexed);
        press(&mut engine, &mut context, pane, &mut mode, "Enter");
        let replacement = match option.name {
            "pane-colours" => "blue",
            "codepoint-widths" => "U+0042=2",
            "command-alias" => "changed=display-message changed",
            _ => "changed-value",
        };
        press(&mut engine, &mut context, pane, &mut mode, "C-u");
        type_text(&mut engine, &mut context, pane, &mut mode, replacement);
        press(&mut engine, &mut context, pane, &mut mode, "Enter");
        expected.insert(ArrayIndex::Numeric(100), replacement.to_owned());
        assert_eq!(
            engine.customize_array_values(target, option.name),
            expected,
            "{} child",
            option.name
        );
    }
}

#[test]
fn customize_edits_hook_arrays_and_counts_their_fires_in_the_preview() {
    let (mut engine, mut context, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    mode.expanded.insert("options:3".to_owned());
    select(&engine, pane, &mut mode, "after-new-window");
    let preview = preview_text(&engine, pane, &mode);
    assert!(preview.contains("This is a session hook."), "{preview}");
    assert!(preview.contains("This is an array hook."), "{preview}");
    assert!(
        preview.contains("This hook has been fired 0 times."),
        "{preview}"
    );
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        mode.prompt
            .as_ref()
            .map(|(prompt, _)| prompt.label.as_str()),
        Some("(after-new-window[+], global) ")
    );
    type_text(
        &mut engine,
        &mut context,
        pane,
        &mut mode,
        "set-option -g @fired yes",
    );
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-hooks",
            &["-g", "after-new-window"]
        ),
        "after-new-window[0] set-option -g @fired yes"
    );
    assert_eq!(current_row(&engine, pane, &mode).name, "after-new-window");
    press(&mut engine, &mut context, pane, &mut mode, "Right");
    press(&mut engine, &mut context, pane, &mut mode, "Down");
    assert_eq!(
        current_row(&engine, pane, &mode).name,
        "after-new-window[0]"
    );
    let preview = preview_text(&engine, pane, &mode);
    assert!(
        preview.contains("Hook command: #[fg=themelightgrey]set-option -g @fired yes"),
        "{preview}"
    );
    engine.count_hook_fire(
        Some(TmuxOptionTarget::GlobalSession),
        "after-new-window",
        unix_seconds(),
    );
    let preview = preview_text(&engine, pane, &mode);
    assert!(
        preview.contains("This hook has been fired 1 times, last "),
        "{preview}"
    );
    press(&mut engine, &mut context, pane, &mut mode, "a");
    press(&mut engine, &mut context, pane, &mut mode, "C-u");
    type_text(&mut engine, &mut context, pane, &mut mode, "5");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-hooks",
            &["-g", "after-new-window"]
        ),
        "after-new-window[5] set-option -g @fired yes"
    );
    select(&engine, pane, &mut mode, "after-new-window[5]");
    press(&mut engine, &mut context, pane, &mut mode, "u");
    assert_eq!(
        mode.prompt
            .as_ref()
            .map(|(prompt, _)| prompt.label.as_str()),
        Some("Unset after-new-window[5]? ")
    );
    assert_eq!(mode.prompt_kind(), ("command", &["SINGLE", "NOFORMAT"][..]));
    press(&mut engine, &mut context, pane, &mut mode, "y");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-hooks",
            &["-g", "after-new-window"]
        ),
        "after-new-window"
    );
}

fn press_lenient(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    pane: PaneId,
    mode: &mut CustomizeMode,
    key: &str,
) -> CustomizeResult {
    let mut expand = customize_expand;
    let result = engine.customize_key(pane, mode, key, PromptHistories::default(), &mut expand);
    for command in &result.commands {
        if engine.execute(context, command).is_err() && result.stop_on_error {
            break;
        }
    }
    engine.customize_finish(pane, mode, &mut expand);
    result
}

fn rename_hook_key(new_key: &str) -> (String, CustomizeResult) {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "after-new-window", "display-message kept"],
    );
    let mut mode = CustomizeMode::default();
    mode.expanded.insert("options:3".to_owned());
    mode.expanded
        .insert("options:3/after-new-window".to_owned());
    select(&engine, pane, &mut mode, "after-new-window[0]");
    press(&mut engine, &mut context, pane, &mut mode, "a");
    press(&mut engine, &mut context, pane, &mut mode, "C-u");
    type_text(&mut engine, &mut context, pane, &mut mode, new_key);
    let result = press_lenient(&mut engine, &mut context, pane, &mut mode, "Enter");
    (
        output(
            &mut engine,
            &mut context,
            "show-hooks",
            &["-g", "after-new-window"],
        ),
        result,
    )
}

#[test]
fn customize_array_key_rename_keeps_the_entry_when_the_new_key_is_invalid() {
    let (hooks, result) = rename_hook_key("4294967296");
    assert_eq!(hooks, "after-new-window[0] display-message kept");
    assert!(result.commands.is_empty(), "{:?}", result.commands);
    assert_eq!(result.message.as_deref(), Some("Bad array key: 4294967296"));
}

#[test]
fn customize_array_key_rename_sees_an_occupied_key_in_any_spelling() {
    let (hooks, result) = rename_hook_key("00");
    assert_eq!(hooks, "after-new-window[0] display-message kept");
    assert!(result.commands.is_empty(), "{:?}", result.commands);
    assert_eq!(result.message, None);
    let (hooks, _) = rename_hook_key("007");
    assert_eq!(hooks, "after-new-window[7] display-message kept");
}

#[test]
fn customize_array_key_rename_refuses_a_key_the_command_line_cannot_spell() {
    let (hooks, result) = rename_hook_key("a]b");
    assert_eq!(hooks, "after-new-window[0] display-message kept");
    assert!(result.commands.is_empty(), "{:?}", result.commands);
    assert_eq!(result.message.as_deref(), Some("Bad array key: a]b"));
}

#[test]
fn customize_prompts_walk_the_shared_history_of_their_type() {
    let (engine, _, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    let mut expand = customize_expand;
    let command = ["set -g @x 1".to_owned()];
    let search = ["status".to_owned()];
    let history = PromptHistories {
        command: &command,
        search: &search,
    };
    engine.customize_key(pane, &mut mode, "/", history, &mut expand);
    engine.customize_key(pane, &mut mode, "Up", history, &mut expand);
    assert_eq!(
        mode.prompt.as_ref().map(|(prompt, _)| prompt.input()),
        Some("status".to_owned())
    );
    let result = engine.customize_key(pane, &mut mode, "Enter", history, &mut expand);
    assert!(mode.prompt.is_none());
    assert_eq!(
        result.remembered,
        Some((CommandPromptType::Search, "status".to_owned()))
    );
    engine.customize_key(pane, &mut mode, "f", history, &mut expand);
    engine.customize_key(pane, &mut mode, "C-p", history, &mut expand);
    assert_eq!(
        mode.prompt.as_ref().map(|(prompt, _)| prompt.input()),
        Some("status".to_owned()),
        "the filter prompt is a search prompt too"
    );
    let result = engine.customize_key(pane, &mut mode, "Escape", history, &mut expand);
    assert_eq!(result.remembered, None);
}

#[test]
fn customize_add_prompt_errors_are_messages_not_commands() {
    let (engine, _, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    let mut expand = customize_expand;
    for (purpose, value, message) in [
        (
            PromptPurpose::AddOption {
                target: TmuxOptionTarget::GlobalSession,
                hook: false,
            },
            "mine",
            "User option must be @name value",
        ),
        (
            PromptPurpose::AddOption {
                target: TmuxOptionTarget::GlobalSession,
                hook: true,
            },
            "mine value",
            "User hook name must start with @",
        ),
        (
            PromptPurpose::AddEnvironment { session: None },
            "-",
            "Bad environment variable: -",
        ),
        (
            PromptPurpose::AddEnvironment { session: None },
            "NOVALUE",
            "Environment variable must be NAME=value",
        ),
        (
            PromptPurpose::AddKey {
                table: "root".to_owned(),
            },
            "C-a",
            "Key binding must be key command",
        ),
        (
            PromptPurpose::AddKey {
                table: "root".to_owned(),
            },
            "Nope display-message x",
            "Unknown key: Nope",
        ),
    ] {
        let result = engine.customize_answer(
            pane,
            &mut mode,
            purpose,
            Some(value.to_owned()),
            &mut expand,
        );
        assert!(result.commands.is_empty(), "{value}: {:?}", result.commands);
        assert_eq!(result.message.as_deref(), Some(message), "{value}");
    }
}

#[test]
fn customize_array_key_rename_stops_on_error() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "after-new-window", "display-message kept"],
    );
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    let mut expand = customize_expand;
    let result = engine.customize_answer(
        pane,
        &mut mode,
        PromptPurpose::ArrayKey {
            name: "after-new-window".to_owned(),
            array_key: "0".to_owned(),
            target: TmuxOptionTarget::GlobalSession,
        },
        Some("1".to_owned()),
        &mut expand,
    );
    assert!(result.stop_on_error);
    assert_eq!(
        result.commands,
        [
            customize_set_command(
                "after-new-window[1]",
                TmuxOptionTarget::GlobalSession,
                "display-message kept"
            ),
            customize_unset_command("after-new-window[0]", TmuxOptionTarget::GlobalSession),
        ]
    );
}

#[test]
fn customize_clearing_a_hidden_variable_keeps_it_hidden() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-environment",
        &["-h", "SECRET", "one"],
    );
    let mut mode = CustomizeMode::default();
    mode.expanded.insert("environment:session".to_owned());
    select(&engine, pane, &mut mode, "Session Environment");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    type_text(&mut engine, &mut context, pane, &mut mode, "-SECRET");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    select(&engine, pane, &mut mode, "-SECRET");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    type_text(&mut engine, &mut context, pane, &mut mode, "two");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-environment",
            &["-h", "SECRET"]
        ),
        "SECRET=two"
    );
    assert!(!output(&mut engine, &mut context, "show-environment", &[]).contains("SECRET"));
}

#[test]
fn customize_unset_removes_a_monitor_with_its_option() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "-B", "@mon:%*:#{pane_id}"],
    );
    let mut mode = CustomizeMode::default();
    mode.expanded.insert("options:3".to_owned());
    select(&engine, pane, &mut mode, "@mon");
    press_lenient(&mut engine, &mut context, pane, &mut mode, "u");
    press_lenient(&mut engine, &mut context, pane, &mut mode, "y");
    assert!(engine.format_monitors.is_empty());
    let mut expand = customize_expand;
    let rows = engine.customize_rows(pane, &mode, &mut expand);
    assert!(!rows.iter().any(|row| row.name == "@mon"));
}

#[test]
fn customize_reset_clears_a_hook_at_every_level_it_is_defined() {
    let (mut engine, mut context, pane) = engine_with_session();
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &["-g", "after-new-window", "display-message global"],
    );
    run(
        &mut engine,
        &mut context,
        "set-hook",
        &[
            "-t",
            "customize",
            "after-new-window",
            "display-message local",
        ],
    );
    let mut mode = CustomizeMode::default();
    mode.expanded.insert("options:3".to_owned());
    select(&engine, pane, &mut mode, "after-new-window");
    assert!(matches!(
        current_row(&engine, pane, &mode).item,
        Item::Option {
            target: TmuxOptionTarget::Session(_),
            ..
        }
    ));
    press_lenient(&mut engine, &mut context, pane, &mut mode, "d");
    press_lenient(&mut engine, &mut context, pane, &mut mode, "y");
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-hooks",
            &["-g", "after-new-window"]
        ),
        "after-new-window"
    );
    assert_eq!(
        output(
            &mut engine,
            &mut context,
            "show-hooks",
            &["-t", "customize", "after-new-window"]
        ),
        ""
    );
}

#[test]
fn customize_global_edit_keys_keep_an_arrays_existing_scope() {
    let (mut engine, mut context, pane) = engine_with_session();
    for (name, target, value) in [
        (
            "update-environment",
            TmuxOptionTarget::Session(context.session.unwrap()),
            "LOCAL",
        ),
        ("pane-colours", TmuxOptionTarget::Pane(pane), "red"),
        (
            "pane-colours",
            TmuxOptionTarget::Window(context.window.unwrap()),
            "blue",
        ),
    ] {
        engine
            .execute(
                &mut context,
                &customize_set_command(&format!("{name}[100]"), target, value),
            )
            .unwrap();
        if matches!(target, TmuxOptionTarget::Window(_)) {
            engine
                .execute(
                    &mut context,
                    &CommandInvocation::new("set-option", ["-pu", "pane-colours"]),
                )
                .unwrap();
        }
        for key in ["S", "W"] {
            let mut mode = CustomizeMode::default();
            mode.expanded
                .extend((0..3).map(|index| format!("options:{index}")));
            select(&engine, pane, &mut mode, name);
            press(&mut engine, &mut context, pane, &mut mode, key);
            assert!(
                matches!(&mode.prompt, Some((_, PromptPurpose::Option { target: actual, .. })) if *actual == target),
                "{name}: {key}"
            );
        }
    }
}

#[test]
fn customize_edits_a_number_and_preserves_the_selected_option() {
    let (mut engine, mut context, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    press(&mut engine, &mut context, pane, &mut mode, "Right");
    select(&engine, pane, &mut mode, "buffer-limit");
    press(&mut engine, &mut context, pane, &mut mode, "Enter");
    press(&mut engine, &mut context, pane, &mut mode, "C-u");
    press(&mut engine, &mut context, pane, &mut mode, "7");
    assert!(!press(&mut engine, &mut context, pane, &mut mode, "Enter"));
    let row = current_row(&engine, pane, &mode);
    assert_eq!(row.name, "buffer-limit");
    assert!(matches!(row.item, Item::Option { ref value, .. } if value == "7"));
    assert!(press(&mut engine, &mut context, pane, &mut mode, "q"));
}

#[test]
fn customize_cycles_flags_and_choices_in_the_local_scope() {
    let (mut engine, mut context, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    mode.expanded.insert("options:1".to_owned());
    engine
        .execute(
            &mut context,
            &CommandInvocation::new("set-option", ["-g", "mouse", "off"]),
        )
        .unwrap();
    for (name, expected) in [("mouse", "on"), ("status-position", "top")] {
        select(&engine, pane, &mut mode, name);
        press(&mut engine, &mut context, pane, &mut mode, "Enter");
        let row = current_row(&engine, pane, &mode);
        assert!(
            matches!(&row.item, Item::Option { value, target, .. }
                if value == expected && *target == TmuxOptionTarget::Session(context.session.unwrap())),
            "{name}"
        );
    }
}

#[test]
fn customize_unset_removes_one_array_index_and_moves_up() {
    let (mut engine, mut context, pane) = engine_with_session();
    let mut mode = CustomizeMode::default();
    engine.customize_height(pane, &mut mode);
    mode.expanded.insert("options:0".to_owned());
    mode.expanded.insert("options:0/command-alias".to_owned());
    select(&engine, pane, &mut mode, "command-alias[0]");
    let before = engine.customize_array_values(TmuxOptionTarget::Server, "command-alias");
    press(&mut engine, &mut context, pane, &mut mode, "u");
    assert!(
        matches!(&mode.prompt, Some((prompt, _)) if prompt.label == "Unset command-alias[0]? ")
    );
    press(&mut engine, &mut context, pane, &mut mode, "y");
    let after = engine.customize_array_values(TmuxOptionTarget::Server, "command-alias");
    assert_eq!(after.len() + 1, before.len());
    assert!(!after.contains_key(&ArrayIndex::Numeric(0)));
    assert_eq!(current_row(&engine, pane, &mode).name, "command-alias");
}
