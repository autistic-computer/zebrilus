use super::test_support::command;
use super::*;

fn assert_bare_matches_format(
    engine: &mut MuxEngine,
    context: &mut ExecutionContext,
    name: &str,
    args: &[&str],
    format: &str,
) -> RawText {
    let bare = engine
        .execute(context, &command(name, args))
        .unwrap()
        .output;
    let mut explicit = command(name, args);
    explicit.args.extend([RawText::from("-F"), format.into()]);
    let formatted = engine.execute(context, &explicit).unwrap().output;
    assert_eq!(bare, formatted);
    bare
}

struct AttachedSessionHooks;

impl StatusHooks for AttachedSessionHooks {
    fn strftime(&mut self, literal: &str) -> String {
        literal.to_owned()
    }

    fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
        String::new()
    }

    fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
        (name == "session_attached").then(|| "1".to_owned())
    }
}

#[test]
fn session_alert_flag_formats_check_every_window() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .expect("session");
    let session = context.session.expect("session id");
    let first = context.window.expect("first window");
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", "background"]),
        )
        .expect("second window");
    let second = engine.state.sessions[&session]
        .windows
        .iter()
        .copied()
        .find(|window| *window != first)
        .expect("second window id");
    engine
        .execute(&mut context, &command("select-window", &["-t", "work:1"]))
        .expect("select second window");
    engine.state.set_window_activity_flag(first, true);
    engine.state.set_window_silence_flag(second, true);

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-F", "#{session_activity_flag}|#{session_silence_flag}"],
                ),
            )
            .expect("list sessions")
            .output,
        "1|1"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &[
                        "-F",
                        "#{window_index}|#{session_activity_flag}|#{session_silence_flag}",
                    ],
                ),
            )
            .expect("list windows")
            .output,
        "0|1|1\n1|1|1"
    );
    let bell_pane = engine.state.windows[&second].active_pane;
    engine.state.set_pane_bell(bell_pane, true);
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "-t", "work:0", "#{session_bell_flag}"],
                ),
            )
            .expect("display bell flag")
            .output,
        "1"
    );
}

#[test]
fn window_and_pane_lists_sort_then_filter_with_pin_line_values() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("rename-window", &["base"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-d", "-n", "z"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-d", "-n", "a"]))
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &["-O", "name", "-F", "#{line}:#{window_name}"],
                ),
            )
            .unwrap()
            .output,
        "3:a\n3:base\n3:z"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &[
                        "-O",
                        "name",
                        "-f",
                        "#{==:#{window_name},z}",
                        "-F",
                        "#{line}:#{window_name}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "3:z"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &[
                        "-O",
                        "name",
                        "-f",
                        "#{==:#{session_active},1}",
                        "-F",
                        "#{window_name}=[#{session_active}]",
                    ],
                ),
            )
            .unwrap()
            .output,
        "a=[1]\nbase=[1]\nz=[1]"
    );

    let first = context.pane.unwrap();
    engine.state.update_pane_title(first, "z").unwrap();
    let split = engine
        .execute(&mut context, &command("split-window", &["-d", "-h"]))
        .unwrap();
    let second = split
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();
    engine.state.update_pane_title(second, "a").unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &["-O", "title", "-F", "#{line}:#{pane_title}"],
                ),
            )
            .unwrap()
            .output,
        "2:a\n2:z"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &[
                        "-O",
                        "name",
                        "-f",
                        "#{==:#{pane_title},z}",
                        "-F",
                        "#{line}:#{pane_title}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "2:z"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-panes", &["-O", "z", "-F", "#{pane_title}"]),
            )
            .unwrap()
            .output,
        "a\nz"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &[
                        "-O",
                        "title",
                        "-f",
                        "#{==:#{session_active},1}",
                        "-F",
                        "#{pane_title}=[#{session_active}]",
                    ],
                ),
            )
            .unwrap()
            .output,
        "a=[1]\nz=[1]"
    );
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "list-windows",
                &["-t", "missing", "-O", "not-an-order"],
            ),
        ),
        Err(ServerError::SessionNotFound(target)) if target == "missing"
    ));
    assert!(matches!(
        engine.execute(
            &mut context,
            &command(
                "list-panes",
                &["-t", "missing", "-O", "not-an-order"],
            ),
        ),
        Err(ServerError::WindowNotFound(target)) if target == "missing"
    ));
}

#[test]
fn list_formats_are_contextual_and_bare_output_uses_the_pin_templates() {
    let mut engine = MuxEngine::default();
    engine.set_format_now(1_700_000_000);
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    engine
        .execute(&mut context, &command("rename-window", &["main"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();

    let session_format = concat!(
        "#{session_name}: #{session_windows} windows (created #{t:session_created})",
        "#{?session_grouped, (group ,}#{session_group}#{?session_grouped,),}",
        "#{?session_attached, (attached),}",
    );
    let window_format = concat!(
        "#{window_index}: #{window_name}#{window_raw_flags} (#{window_panes} panes) ",
        "[#{window_width}x#{window_height}] [layout #{window_layout}] #{window_id}",
        "#{?window_active, (active),}",
    );
    let window_with_session_format = concat!(
        "#{session_name}:#{window_index}: #{window_name}#{window_raw_flags} ",
        "(#{window_panes} panes) [#{window_width}x#{window_height}] ",
    );
    let pane_format = concat!(
        "#{pane_index}: [#{pane_width}x#{pane_height}",
        "#{?pane_floating_flag, #{pane_x}#,#{pane_y}#,#{pane_z}}] ",
        "[history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}",
        "#{?pane_active, (active),}#{?pane_dead, (dead),}",
    );
    let pane_with_session_format = concat!(
        "#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}",
        "#{?pane_floating_flag, #{pane_x}#,#{pane_y}#,#{pane_z}}] ",
        "[history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}",
        "#{?pane_active, (active),}#{?pane_dead, (dead),}",
    );
    let pane_with_server_format = concat!(
        "#{session_name}:#{window_index}.#{pane_index}: [#{pane_width}x#{pane_height}",
        "#{?pane_floating_flag, #{pane_x}#,#{pane_y}#,#{pane_z}}] ",
        "[history #{history_size}/#{history_limit}, #{history_bytes} bytes] #{pane_id}",
        "#{?pane_active, (active),}#{?pane_dead, (dead),}",
    );

    assert_bare_matches_format(
        &mut engine,
        &mut context,
        "list-sessions",
        &[],
        session_format,
    );
    assert_bare_matches_format(
        &mut engine,
        &mut context,
        "list-windows",
        &[],
        window_format,
    );
    assert_bare_matches_format(
        &mut engine,
        &mut context,
        "list-windows",
        &["-a"],
        window_with_session_format,
    );
    assert_bare_matches_format(&mut engine, &mut context, "list-panes", &[], pane_format);
    assert_bare_matches_format(
        &mut engine,
        &mut context,
        "list-panes",
        &["-s"],
        pane_with_session_format,
    );
    assert_bare_matches_format(
        &mut engine,
        &mut context,
        "list-panes",
        &["-a"],
        pane_with_server_format,
    );

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &["-F", "#{session_id}:#S:#{session_windows}"],
                ),
            )
            .unwrap()
            .output,
        "$0:work:1"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &["-F", "#{session_id}:#{window_id}:#I:#W:#F"],
                ),
            )
            .unwrap()
            .output,
        "$0:@0:0:main:*"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &["-F", "#{session_id}:#{window_id}:#{pane_id}:#P:#T",],
                ),
            )
            .unwrap()
            .output,
        "$0:@0:%0:0:terminal\n$0:@0:%1:1:terminal"
    );

    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-windows", &["-F", "#{window_raw_flags}"]),
            )
            .unwrap()
            .output,
        "-\n*"
    );

    let mut hooks = AttachedSessionHooks;
    let attached = engine
        .execute_with_format_hooks(&mut context, &command("list-sessions", &[]), &mut hooks)
        .unwrap()
        .output;
    assert!(attached.ends_with(" (attached)"));
    let mut hooks = AttachedSessionHooks;
    let explicitly_attached = engine
        .execute_with_format_hooks(
            &mut context,
            &command("list-sessions", &["-F", session_format]),
            &mut hooks,
        )
        .unwrap()
        .output;
    assert_eq!(attached, explicitly_attached);

    assert!(
        engine
            .execute(&mut context, &command("list-sessions", &["-x"]))
            .is_err()
    );
}

#[test]
fn format_activity_tracks_each_rows_window_and_pane() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first_window = context.window.unwrap();
    let first_pane = context.pane.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second_pane = context.pane.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    let second_window = context.window.unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &[
                        "-t",
                        &first_window.to_string(),
                        "-F",
                        "#{pane_id}:#{pane_active}",
                    ],
                ),
            )
            .unwrap()
            .output,
        format!("{first_pane}:0\n{second_pane}:1")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-panes",
                    &[
                        "-t",
                        &first_window.to_string(),
                        "-F",
                        "#{pane_id}#{?pane_active,(active),}",
                    ],
                ),
            )
            .unwrap()
            .output,
        format!("{first_pane}\n{second_pane}(active)")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("list-windows", &["-F", "#{window_id}:#{window_active}"],),
            )
            .unwrap()
            .output,
        format!("{first_window}:0\n{second_window}:1")
    );
}

#[test]
fn session_and_window_rows_backfill_the_active_pane_like_tmux() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first_window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second_pane = context.pane.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    let second_window = context.window.unwrap();
    let third_pane = context.pane.unwrap();
    engine.set_pane_geometry(third_pane, 80, 24);

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-sessions",
                    &[
                        "-F",
                        "#{window_index}:#{pane_id}:#{pane_width}:#{pane_active}:#{window_active}",
                    ],
                ),
            )
            .unwrap()
            .output,
        format!("1:{third_pane}:80:1:1")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &["-F", "#{window_id}:#{pane_id}:#{pane_active}"],
                ),
            )
            .unwrap()
            .output,
        format!("{first_window}:{second_pane}:1\n{second_window}:{third_pane}:1")
    );
}

#[test]
fn window_activity_conditionals_and_display_message_read_the_new_variables() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let first_window = context.window.unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-h"]))
        .unwrap();
    let second_pane = context.pane.unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-n", "second"]))
        .unwrap();
    let second_window = context.window.unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "list-windows",
                    &[
                        "-F",
                        "#{window_id}#{?window_active,*,}#{?#{!:#{window_active}},-,}",
                    ],
                ),
            )
            .unwrap()
            .output,
        format!("{first_window}-\n{second_window}*")
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &[
                        "-p",
                        "-t",
                        &second_pane.to_string(),
                        "#{pane_width}x#{pane_height}|#{pane_active}|#{window_active}"
                    ],
                ),
            )
            .unwrap()
            .output,
        "39x24|1|0"
    );
    engine
        .execute(&mut context, &command("new-session", &["-s", "other"]))
        .unwrap();
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "-t", &second_pane.to_string(), "#{session_active}"],
                ),
            )
            .unwrap()
            .output,
        "0"
    );
}

#[test]
fn window_activity_uses_injected_creation_seconds_per_target() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();

    engine.set_format_now(1_700_000_001);
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let session = context.session.unwrap();
    let first_window = context.window.unwrap();
    let first_pane = context.pane.unwrap();
    assert_eq!(
        engine.state.windows[&first_window].activity_time,
        Some(1_700_000_001)
    );

    engine.set_format_now(1_700_000_002);
    engine
        .execute(
            &mut context,
            &command("new-window", &["-d", "-n", "second"]),
        )
        .unwrap();
    let second_window = engine.state.sessions[&session]
        .windows
        .iter()
        .copied()
        .find(|window| *window != first_window)
        .unwrap();
    let second_pane = engine.state.windows[&second_window].active_pane;
    assert_eq!(
        engine.state.windows[&second_window].activity_time,
        Some(1_700_000_002)
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "-t", &first_pane.to_string(), "#{window_activity}",],
                ),
            )
            .unwrap()
            .output,
        "1700000001"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "display-message",
                    &["-p", "-t", &second_pane.to_string(), "#{window_activity}",],
                ),
            )
            .unwrap()
            .output,
        "1700000002"
    );

    engine.set_format_now(1_700_000_003);
    let split = engine
        .execute(
            &mut context,
            &command("split-window", &["-d", "-t", &first_pane.to_string()]),
        )
        .unwrap();
    let split_pane = split
        .effects
        .iter()
        .find_map(|effect| match effect {
            MuxEffect::PaneCreated { pane, .. } => Some(*pane),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        engine.state.windows[&first_window].activity_time,
        Some(1_700_000_001)
    );

    engine.set_format_now(1_700_000_004);
    engine
        .execute(
            &mut context,
            &command("break-pane", &["-d", "-s", &split_pane.to_string()]),
        )
        .unwrap();
    let broken_window = engine.state.window_for_pane(split_pane).unwrap();
    assert_eq!(
        engine.state.windows[&broken_window].activity_time,
        Some(1_700_000_004)
    );
    assert_eq!(
        engine.state.windows[&first_window].activity_time,
        Some(1_700_000_001)
    );
    assert_eq!(
        engine.state.windows[&second_window].activity_time,
        Some(1_700_000_002)
    );
}

#[test]
fn list_rows_read_session_active_from_the_invoking_client() {
    let mut engine = MuxEngine::default();
    let (first, first_window, first_pane) = engine.state.create_session("first").unwrap();
    let (_second, _, _) = engine.state.create_session("second").unwrap();
    let rows = |engine: &mut MuxEngine, context: &mut ExecutionContext| -> Vec<String> {
        [
            command(
                "list-sessions",
                &["-F", "#{session_name}=[#{session_active}]#{S:n,a}"],
            ),
            command("list-sessions", &["-f", "#{session_active}", "-F", "#S"]),
            command(
                "list-sessions",
                &["-f", "#{==:#{session_active},}", "-F", "#S"],
            ),
            command(
                "list-windows",
                &["-a", "-F", "#{session_name}=[#{session_active}]"],
            ),
            command(
                "list-panes",
                &["-a", "-F", "#{session_name}=[#{session_active}]"],
            ),
        ]
        .iter()
        .map(|invocation| {
            engine
                .execute(context, invocation)
                .unwrap()
                .output
                .to_string()
        })
        .collect()
    };

    let mut attached = ExecutionContext::new(Some(first), Some(first_window), Some(first_pane));
    attached.set_attached_client_context(Some((first, first_window, first_pane)));
    assert_eq!(
        rows(&mut engine, &mut attached),
        [
            "first=[1]an\nsecond=[0]an",
            "first",
            "",
            "first=[1]\nsecond=[0]",
            "first=[1]\nsecond=[0]",
        ]
    );

    let mut unattached = attached.clone();
    unattached.set_client_attached(false);
    assert_eq!(
        rows(&mut engine, &mut unattached),
        [
            "first=[0]nn\nsecond=[0]nn",
            "",
            "",
            "first=[0]\nsecond=[0]",
            "first=[0]\nsecond=[0]",
        ]
    );

    let mut no_client = attached.clone();
    no_client.set_no_client();
    assert_eq!(
        rows(&mut engine, &mut no_client),
        [
            "first=[]nn\nsecond=[]nn",
            "",
            "first\nsecond",
            "first=[]\nsecond=[]",
            "first=[]\nsecond=[]",
        ]
    );
}

#[test]
fn pane_start_command_formats_render_retained_argv() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    let created = engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "work", "printf", "a b", "it's", "$HOME", ""],
            ),
        )
        .unwrap();
    assert!(matches!(
        created.effects.first(),
        Some(MuxEffect::PaneCreated {
            command: Some(command),
            ..
        }) if command.as_slice() == ["printf", "a b", "it's", "$HOME", ""]
    ));
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{pane_start_command}"]),
            )
            .unwrap()
            .output,
        r#"printf "a b" "it's" "\$HOME" ''"#
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{pane_start_command_list}"],),
            )
            .unwrap()
            .output,
        r"'printf' 'a b' 'it'\''s' '$HOME' ''"
    );
}

#[test]
fn command_format_names_the_canonical_entry_of_the_running_command_item() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-s", "command-alias[20]", "zzitem=display-message"],
            ),
        )
        .unwrap();

    for spelling in ["display-message", "display", "display-mes", "zzitem"] {
        assert_eq!(
            engine
                .execute(&mut context, &command(spelling, &["-p", "#{command}"]))
                .unwrap()
                .output,
            "display-message",
            "{spelling} must report the canonical command-item entry"
        );
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "#{?command,set,unset}"]),
            )
            .unwrap()
            .output,
        "set"
    );
}

#[test]
fn command_argument_formats_see_canonical_names_after_resolution() {
    for spelling in ["rename-session", "rename", "rename-s"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let session = context.session.unwrap();
        engine
            .execute(&mut context, &command(spelling, &["#{command}"]))
            .unwrap();
        assert_eq!(engine.state.sessions[&session].name, "rename-session");
    }

    for spelling in ["rename-window", "renamew", "rename-w"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let window = context.window.unwrap();
        engine
            .execute(&mut context, &command(spelling, &["#{command}"]))
            .unwrap();
        assert_eq!(engine.state.windows[&window].name, "rename-window");
    }

    for spelling in ["show-options", "show", "show-o"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        engine
            .execute(
                &mut context,
                &command("set-option", &["@show-options", "session"]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(&mut context, &command(spelling, &["-v", "@#{command}"]))
                .unwrap()
                .output,
            "session"
        );
    }

    for spelling in ["show-window-options", "showw", "show-window-o"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        engine
            .execute(
                &mut context,
                &command("set-window-option", &["@show-window-options", "window"]),
            )
            .unwrap();
        assert_eq!(
            engine
                .execute(&mut context, &command(spelling, &["-v", "@#{command}"]))
                .unwrap()
                .output,
            "window"
        );
    }

    for spelling in ["select-pane", "selectp"] {
        let mut engine = MuxEngine::default();
        let mut context = ExecutionContext::default();
        engine
            .execute(&mut context, &command("new-session", &["-s", "work"]))
            .unwrap();
        let pane = context.pane.unwrap();
        engine
            .execute(&mut context, &command(spelling, &["-T", "#{command}"]))
            .unwrap();
        assert_eq!(engine.state.pane(pane).unwrap().title, "select-pane");
    }
}

#[test]
fn command_argument_formats_use_the_resolved_target_old_context() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "target-session", "-n", "target-window"],
            ),
        )
        .unwrap();
    let target_session = context.session.unwrap();
    let target_window = context.window.unwrap();
    let target_pane = context.pane.unwrap();
    let target_pane_name = target_pane.to_string();
    let target_window_name = target_window.to_string();
    engine
        .execute(
            &mut context,
            &command(
                "set-option",
                &["-t", &target_pane_name, "@target-session", "session-value"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["-t", &target_window_name, "@target-window", "window-value"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "new-session",
                &["-s", "current-session", "-n", "current-window"],
            ),
        )
        .unwrap();

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-options",
                    &["-v", "-t", &target_pane_name, "@#{session_name}"],
                ),
            )
            .unwrap()
            .output,
        "session-value"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-window-options",
                    &["-v", "-t", &target_window_name, "@#{window_name}"],
                ),
            )
            .unwrap()
            .output,
        "window-value"
    );

    engine
        .execute(
            &mut context,
            &command(
                "rename-session",
                &["-t", &target_session.to_string(), "#{session_name}-renamed"],
            ),
        )
        .unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "rename-window",
                &["-t", &target_window_name, "#{window_name}-renamed"],
            ),
        )
        .unwrap();
    assert_eq!(
        engine.state.sessions[&target_session].name,
        "target-session-renamed"
    );
    assert_eq!(
        engine.state.windows[&target_window].name,
        "target-window-renamed"
    );
}

#[test]
fn target_format_consumers_keep_target_and_format_client_distinct() {
    let mut engine = MuxEngine::default();
    let (first, first_window, first_pane) = engine.state.create_session("first").unwrap();
    let (second, _, second_pane) = engine.state.create_session("second").unwrap();
    let mut context = ExecutionContext::new(Some(first), Some(first_window), Some(first_pane));
    context.set_attached_client_context(Some((first, first_window, first_pane)));
    context.set_format_client(FormatClient::Attached(first));

    engine
        .execute(
            &mut context,
            &command(
                "select-pane",
                &["-t", &second_pane.to_string(), "-T", "#{session_active}"],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.pane(second_pane).unwrap().title, "0");
    engine
        .execute(
            &mut context,
            &command(
                "select-pane",
                &["-t", &first_pane.to_string(), "-T", "#{session_active}"],
            ),
        )
        .unwrap();
    assert_eq!(engine.state.pane(first_pane).unwrap().title, "1");

    let active_repeat = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &first_pane.to_string(),
                    "-N",
                    "#{?session_active,2,3}",
                ],
            ),
        )
        .unwrap();
    assert!(
        active_repeat
            .effects
            .iter()
            .any(|effect| { matches!(effect, MuxEffect::CopyModeRepeat { count: 2, .. }) })
    );
    let inactive_repeat = engine
        .execute(
            &mut context,
            &command(
                "send-keys",
                &[
                    "-t",
                    &second_pane.to_string(),
                    "-N",
                    "#{?session_active,2,3}",
                ],
            ),
        )
        .unwrap();
    assert!(
        inactive_repeat
            .effects
            .iter()
            .any(|effect| { matches!(effect, MuxEffect::CopyModeRepeat { count: 3, .. }) })
    );

    for (name, value) in [("@active", "yes"), ("@inactive", "no")] {
        engine
            .execute(&mut context, &command("set-option", &["-g", name, value]))
            .unwrap();
    }
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-options",
                    &[
                        "-g",
                        "-v",
                        "-t",
                        &first_pane.to_string(),
                        "@#{?session_active,active,inactive}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "yes"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "show-options",
                    &[
                        "-g",
                        "-v",
                        "-t",
                        &second_pane.to_string(),
                        "@#{?session_active,active,inactive}",
                    ],
                ),
            )
            .unwrap()
            .output,
        "no"
    );

    for (target, expected) in [(first_pane, "source-1"), (second_pane, "source-0")] {
        let sourced = engine
            .execute(
                &mut context,
                &command(
                    "source-file",
                    &["-F", "-t", &target.to_string(), "source-#{session_active}"],
                ),
            )
            .unwrap();
        assert!(matches!(
            sourced.effects.as_slice(),
            [MuxEffect::SourceFile { path, .. }] if path == expected
        ));
    }

    context.set_client_attached(false);
    context.set_format_client(FormatClient::Unattached);
    let unattached = engine
        .execute(
            &mut context,
            &command(
                "source-file",
                &[
                    "-F",
                    "-t",
                    &first_pane.to_string(),
                    "source-#{session_active}",
                ],
            ),
        )
        .unwrap();
    assert!(matches!(
        unattached.effects.as_slice(),
        [MuxEffect::SourceFile { path, .. }] if path == "source-0"
    ));

    context.set_no_client();
    context.set_format_client(FormatClient::NoClient);
    let clientless = engine
        .execute(
            &mut context,
            &command(
                "source-file",
                &[
                    "-F",
                    "-t",
                    &first_pane.to_string(),
                    "source-#{session_active}",
                ],
            ),
        )
        .unwrap();
    assert!(matches!(
        clientless.effects.as_slice(),
        [MuxEffect::SourceFile { path, .. }] if path == "source-"
    ));

    assert_ne!(first, second);
}

#[test]
fn command_format_stays_invariant_across_list_rows_beside_disjoint_row_facts() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    for (key, bound) in [("F1", "next-window"), ("F2", "previous-window")] {
        engine
            .execute(
                &mut context,
                &command("bind-key", &["-T", "zzitem", key, bound]),
            )
            .unwrap();
    }

    let rows = engine
        .execute(
            &mut context,
            &command(
                "lsk",
                &[
                    "-T",
                    "zzitem",
                    "-F",
                    "#{command}|#{key_string}|#{key_command}",
                ],
            ),
        )
        .unwrap()
        .output;
    let mut rows = rows.lines().collect::<Vec<_>>();
    rows.sort_unstable();
    assert_eq!(
        rows,
        ["list-keys|F1|next-window", "list-keys|F2|previous-window"]
    );

    assert_eq!(
        engine
            .execute(
                &mut context,
                &command(
                    "lscm",
                    &[
                        "-F",
                        "#{command}|#{command_list_name}|#{command_list_alias}",
                        "list-keys",
                    ],
                ),
            )
            .unwrap()
            .output,
        "list-commands|list-keys|lsk"
    );
}

#[test]
fn command_format_is_empty_outside_a_command_item() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "work"]))
        .unwrap();
    let window = context.window.unwrap();
    engine
        .execute(
            &mut context,
            &command(
                "set-window-option",
                &["automatic-rename-format", "[#{command}]"],
            ),
        )
        .unwrap();

    engine
        .execute(
            &mut context,
            &command("set-window-option", &["automatic-rename", "on"]),
        )
        .unwrap();

    assert_eq!(engine.state.windows[&window].name, "[]");
}

#[test]
fn an_outer_command_variable_outranks_the_command_item_name() {
    struct OuterCommand;

    impl StatusHooks for OuterCommand {
        fn strftime(&mut self, literal: &str) -> String {
            literal.to_owned()
        }

        fn shell(&mut self, _command: &str, _tag: &FormatJobTag) -> String {
            String::new()
        }

        fn variable(&mut self, name: &str, _context: &StatusContext) -> Option<String> {
            (name == "command").then(|| "outer".to_owned())
        }
    }

    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    assert_eq!(
        engine
            .execute_with_format_hooks(
                &mut context,
                &command("display-message", &["-p", "[#{command}]"]),
                &mut OuterCommand,
            )
            .unwrap()
            .output,
        "[outer]"
    );
    assert_eq!(
        engine
            .execute_with_format_hooks(
                &mut context,
                &command("lscm", &["-F", "[#{command}]", "display-message"]),
                &mut OuterCommand,
            )
            .unwrap()
            .output,
        "[outer]"
    );
    assert_eq!(
        engine
            .execute(
                &mut context,
                &command("display-message", &["-p", "[#{command}]"])
            )
            .unwrap()
            .output,
        "[display-message]"
    );
}

#[test]
fn gap_queries_are_silent_sorted_and_catalog_backed() {
    let mut engine = MuxEngine::default();
    let mut context = ExecutionContext::default();
    engine
        .execute(&mut context, &command("new-session", &["-s", "w"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "B"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-session", &["-d", "-s", "A"]))
        .unwrap();
    engine
        .execute(&mut context, &command("split-window", &["-d", "-t", "B:0"]))
        .unwrap();
    engine
        .execute(&mut context, &command("new-window", &["-d", "-t", "B:2"]))
        .unwrap();

    let windows = engine
        .execute(
            &mut context,
            &command(
                "list-windows",
                &["-a", "-F", "#{session_name}:#{window_index}"],
            ),
        )
        .unwrap();
    assert_eq!(windows.output, "A:0\nB:0\nB:2\nw:0");
    let panes = engine
        .execute(
            &mut context,
            &command(
                "list-panes",
                &["-a", "-F", "#{session_name}:#{window_index}.#{pane_index}"],
            ),
        )
        .unwrap();
    assert_eq!(panes.output, "A:0.0\nB:0.0\nB:0.1\nB:2.0\nw:0.0");
    let session_panes = engine
        .execute(
            &mut context,
            &command(
                "list-panes",
                &[
                    "-s",
                    "-t",
                    "B:0",
                    "-F",
                    "#{session_name}:#{window_index}.#{pane_index}",
                ],
            ),
        )
        .unwrap();
    assert_eq!(session_panes.output, "B:0.0\nB:0.1\nB:2.0");

    for pattern in ["ma*", "zzz*"] {
        let found = engine
            .execute(
                &mut context,
                &command("find-window", &["-CiNrTZ", "-t", "w:0", pattern]),
            )
            .unwrap();
        assert!(found.output.is_empty());
        let [MuxEffect::ChooseTree { filter, kind, .. }] = found.effects.as_slice() else {
            panic!("find-window must open the native pane chooser");
        };
        assert_eq!(*kind, ChooseTreeKind::Panes);
        assert_eq!(
            filter.as_deref(),
            Some("#{||:#{C/ri:PATTERN},#{||:#{m/ri:PATTERN,#{window_name}},#{m/ri:PATTERN,#{pane_title}}}}".replace("PATTERN", pattern).as_str())
        );
    }

    let row = engine
        .execute(&mut context, &command("list-commands", &["move-window"]))
        .unwrap();
    assert_eq!(
        row.output,
        "move-window (movew) [-abdkr] [-s src-window] [-t dst-window]"
    );
    let formatted = engine
        .execute(
            &mut context,
            &command(
                "list-commands",
                &[
                    "-F",
                    "#{command_list_name}|#{command_list_alias}|#{command_list_usage}",
                    "start",
                ],
            ),
        )
        .unwrap();
    assert_eq!(formatted.output, "start-server|start|");
    assert_eq!(
        engine
            .execute(&mut context, &command("list-commands", &["kill-s"]))
            .expect_err("ambiguous list-commands query"),
        ServerError::InvalidCommand(
            "ambiguous command: kill-s, could be: kill-server, kill-session".to_owned()
        )
    );
    let commands = engine
        .execute(&mut context, &command("list-commands", &[]))
        .unwrap();
    let rows = commands.output.lines().collect::<Vec<_>>();
    assert_eq!(rows.len(), COMMAND_SPECS.len() + DAEMON_COMMAND_SPECS.len());
    assert!(rows.windows(2).all(|rows| rows[0] < rows[1]));
    assert!(rows.contains(&"kill-server "));
    assert!(rows.contains(&"list-commands (lscm) [-F format] [command]"));
    assert!(rows.contains(&"start-server (start) "));
    assert!(rows.contains(
        &"agent-send [-t target-pane] [--submit] [--wait] [--notify] [--progress] [--timeout seconds] [--on-block wait|fail|allow|deny] [--json] [--final] [--context context] [text ...]"
    ));
    assert!(rows.contains(&"send-last-output [-t target-pane]"));
    assert!(rows.contains(&"capture-browser [-t target-pane] [-o output-path]"));
    assert!(rows.contains(&"debug-marker [note ...]"));
    assert!(rows.contains(
        &"tools [--skill] [--context] [--install-hooks] [--remove-hooks] [--fork-chat] [section]"
    ));
    assert!(rows.contains(
        &"capture-pane (capturep) [-aCeFHJLMNpqT] [-b buffer-name] [-E end-line] [-S start-line] [-t target-pane]"
    ));
    assert!(rows.contains(&"if-shell (if) [-bF] [-t target-pane] shell-command command [command]"));
    assert!(rows.contains(
        &"paste-buffer (pasteb) [-dprS] [-s separator] [-b buffer-name] [-t target-pane]"
    ));
    assert!(rows.contains(
        &"run-shell (run) [-bCE] [-c start-directory] [-d delay] [-t target-pane] [shell-command [argument ...]]"
    ));
    assert!(rows.contains(&"wait-for (wait) [-ELSUlv] [-F format] [-w waiter] name"));
    assert!(rows.contains(&"pipe-pane (pipep) [-IOo] [-t target-pane] [shell-command]"));
    assert_eq!(
        engine
            .execute(&mut context, &command("list-commands", &["capturep"]))
            .unwrap()
            .output,
        "capture-pane (capturep) [-aCeFHJLMNpqT] [-b buffer-name] [-E end-line] [-S start-line] [-t target-pane]"
    );
    for name in ["if-shell", "if"] {
        assert_eq!(
            engine
                .execute(&mut context, &command("list-commands", &[name]))
                .unwrap()
                .output,
            "if-shell (if) [-bF] [-t target-pane] shell-command command [command]"
        );
    }
    for name in ["run-shell", "run"] {
        assert_eq!(
            engine
                .execute(&mut context, &command("list-commands", &[name]))
                .unwrap()
                .output,
            "run-shell (run) [-bCE] [-c start-directory] [-d delay] [-t target-pane] [shell-command [argument ...]]"
        );
    }
    for name in ["wait-for", "wait"] {
        assert_eq!(
            engine
                .execute(&mut context, &command("list-commands", &[name]))
                .unwrap()
                .output,
            "wait-for (wait) [-ELSUlv] [-F format] [-w waiter] name"
        );
    }
    for name in ["pipe-pane", "pipep"] {
        assert_eq!(
            engine
                .execute(&mut context, &command("list-commands", &[name]))
                .unwrap()
                .output,
            "pipe-pane (pipep) [-IOo] [-t target-pane] [shell-command]"
        );
    }
    assert_eq!(
        engine
            .execute(&mut context, &command("start", &[]))
            .unwrap(),
        Execution::default()
    );
}
