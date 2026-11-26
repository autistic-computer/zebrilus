---
type: Subsystem
title: tmux command set
description: Find a command, follow alias and target resolution, and find the file that executes it.
resource: crates/zz-protocol/src/catalog.rs
tags: [tmux, commands, mux-engine, targets, effects]
timestamp: 2026-11-26T00:00:00-03:00
---

# CLI contract

Use `zz --help` for the catalog and `zz <command> --help` for a command's arguments and options. Both work without a running daemon. `zz list-commands` gives the server's inventory. `zz tools` describes the native workspace verbs. The [protocol crate](/crates/zz-protocol.md) owns the catalog, which also supplies completions for the [command palette](/concepts/command-palette.md).

`list-sessions`, `list-windows`, `list-panes` and `list-clients` accept `--json` for one object per row. Values are strings, using the format names for the selected entity. `show-options --json` returns one object for the selected scope. For custom text output use `-F`. Supplying both formats is a usage error.

| Exit | How to read it |
| --- | --- |
| 0 | Success unless a child or command explicitly sets its own result. |
| 1 | Command or connection failure, including tmux usage errors. |
| 2 | Invalid zz-native command or extension usage. |
| 3 | A blocked request. For `run-pane` admission errors this means the command did not start. A child can also return its own exit status. |
| 124 | A wait expired, or the child returned 124. |
| 125 | `run-pane` timed out after starting the command, or the child returned 125. Read stderr before retrying. |

# Command aliases

`crates/zz-protocol/src/catalog.rs` owns names, built-in aliases, options and positional bounds. `resolve_command` tries an exact name or alias, then a prefix of a tmux command name, then a prefix of a native name. Ambiguity produces a diagnostic. For example, `capture` still resolves to `capture-pane` even with `capture-browser` installed.

The server's indexed `command-alias[]` option supplies user aliases. `MuxEngine::resolve_command_alias` takes the first matching definition, parses its body, appends invocation arguments to the final command, and preserves grouping for multi-command expansions. An alias can affect a stored callback differently depending on when its argument is parsed. A typed block in a chooser template resolves aliases when built. A quoted string resolves them when selected. See [config parsing](/tmux/conf-parser.md) and [selection templates](/tmux/choose-tree.md).

# Argument parsing and targets

`parse_tmux_command_options` applies the catalog to a `CommandInvocation`, including which arguments are typed command blocks. Option parsing handles short clusters, value arity, a bare `--` and the first positional. Commands with distinct flag or positional semantics validate them before acting. The pin's accepted grammar is in `compat/tmux-oracle.json`. Remaining mismatches have IDs in the [gap registry](/tmux/gaps.md).

A target's type comes from the command. Stable session, window and pane IDs use `$N`, `@N` and `%N`. Quote session IDs in the shell. A pane target can also use `session:window.pane`, and `-t :.1` selects pane index 1 in the current window. An omitted target uses the invoking context. See `MuxState::resolve_session`, `resolve_window` and `resolve_pane` in `crates/zz-mux/src/model.rs` for name matching, marked targets, relative indexes and ambiguity.

# Effects

`MuxEngine::execute` in `crates/zz-mux/src/command.rs` applies a command to [mux state](/crates/zz-mux.md) and returns an `Execution` with output and effects. The [daemon](/crates/zz-daemon.md) applies effects that need a live client, PTY, clipboard, file or job. For commands that consume caller stdin or write binary output, see the [command stream channel](/designs/command-stream-channel.md). For example, a split changes the [layout](/concepts/split-pane-layout.md) and emits a pane creation effect. A copy action selects an action on the [terminal view](/tmux/copy-mode.md), and a chooser opens state owned by the invoking client. Use the [daemon map](/crates/zz-daemon.md) (`crates/zz-daemon/src/daemon/`) for the runtime and `crates/zz-cli/src/lib.rs` for local help, attach, transport and exit handling.

The native roster in the catalog and the `commands.native-superset` registry entry identify extensions. `zz tools` includes the current syntax for running work in another pane, sending an [agent turn](/concepts/agent-panes.md), waiting for a pane, or taking a browser screenshot. List formats expose the [program status](/terminal/program-status.md) that a terminal pane publishes. For a regression involving tmux syntax, run the smallest [differential scenario](/playbooks/compat-harness.md) that observes it.

# Where each command runs

The daemon checks `DAEMON_COMMAND_DISPATCHES` in `crates/zz-daemon/src/daemon/command_routing.rs`
first. A command listed there runs in the daemon file below and never reaches the engine. Every
other command goes to the `match` in `MuxEngine::execute_without_alias_expansion_inner` in
`crates/zz-mux/src/command.rs`, which calls a handler in one subject file under
`crates/zz-mux/src/command/`. Aliases such as `splitw` or `capturep` resolve to the names below
first. The engine's arms for `list-clients`, `refresh-client` and `show-messages` only refuse,
because the daemon answers those. The [zz-mux map](/crates/zz-mux.md) and the
[zz-daemon map](/crates/zz-daemon.md) describe each file.

| Command | File | Handler |
| --- | --- | --- |
| `agent-account` | `crates/zz-daemon/src/daemon/agent_account.rs` | `agent_account` |
| `agent-accounts` | `crates/zz-daemon/src/daemon/accounts.rs` | `agent_accounts` |
| `agent-catalog` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `agent_catalog` |
| `agent-respond` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `agent_respond` |
| `agent-send` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `agent_send` |
| `attach-session` | `crates/zz-mux/src/command/sessions.rs` | `attach_session` |
| `bind-key` | `crates/zz-mux/src/command/key_bindings.rs` | `bind_key` |
| `break-pane` | `crates/zz-mux/src/command/pane_spawn.rs` | `break_pane` |
| `build-info` | `crates/zz-daemon/src/daemon/command_routing.rs` | refused, the zz CLI runs it |
| `capture-browser` | `crates/zz-daemon/src/daemon/gui_requests.rs` | `capture_browser` |
| `capture-pane` | `crates/zz-daemon/src/daemon/capture.rs` | `capture_pane` |
| `capture-simulator` | `crates/zz-daemon/src/daemon/simulators.rs` | `simulator_command` |
| `choose-buffer` | `crates/zz-mux/src/command/modes.rs` | `choose_buffer` |
| `choose-client` | `crates/zz-mux/src/command/modes.rs` | `choose_client` |
| `choose-path` | `crates/zz-mux/src/command/modes.rs` | `choose_path` |
| `choose-tree` | `crates/zz-mux/src/command/modes.rs` | `choose_tree` |
| `clear-history` | `crates/zz-mux/src/command/modes.rs` | `clear_history` |
| `clear-prompt-history` | `crates/zz-daemon/src/daemon/history_io.rs` | `prompt_history_command` |
| `clock-mode` | `crates/zz-mux/src/command/modes.rs` | `clock_mode` |
| `command-prompt` | `crates/zz-mux/src/command/messages_and_prompts.rs` | `command_prompt` |
| `confirm-before` | `crates/zz-daemon/src/daemon/prompts.rs` | `confirm_before` |
| `copy-file` | `crates/zz-daemon/src/daemon/command_routing.rs` | refused, the zz CLI runs it |
| `copy-mode` | `crates/zz-mux/src/command/modes.rs` | `copy_mode` |
| `copy-mode-search-prompt` | `crates/zz-mux/src/command/modes.rs` | `copy_mode_search_prompt` |
| `customize-mode` | `crates/zz-mux/src/command/customize.rs` | `customize_mode` |
| `debug-marker` | `crates/zz-daemon/src/daemon/command_routing.rs` | `debug_marker` |
| `delete-buffer` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `detach-client` | `crates/zz-mux/src/command/sessions.rs` | `detach_client` |
| `display-menu` | `crates/zz-daemon/src/daemon/overlays.rs` | `display_menu` |
| `display-message` | `crates/zz-mux/src/command/messages_and_prompts.rs` | `display_message` |
| `display-panes` | `crates/zz-mux/src/command/modes.rs` | `display_panes` |
| `display-popup` | `crates/zz-daemon/src/daemon/overlays.rs` | `display_popup` |
| `events` | `crates/zz-daemon/src/daemon/command_routing.rs` | refused, the zz CLI runs it |
| `find-window` | `crates/zz-mux/src/command/windows.rs` | `find_window` |
| `fleet` | `crates/zz-daemon/src/daemon/command_routing.rs` | refused, the zz CLI runs it |
| `flip-agent` | `crates/zz-daemon/src/daemon/agent_flip.rs` | `flip_agent` |
| `focus-sidebar` | `crates/zz-mux/src/command/modes.rs` | `focus_sidebar` |
| `has-session` | `crates/zz-mux/src/command/sessions.rs` | `has_session` |
| `html` | `crates/zz-daemon/src/daemon/artifacts.rs` | `html` |
| `if-shell` | `crates/zz-daemon/src/daemon/shell_jobs.rs` | `if_shell` |
| `import-tmux-config` | `crates/zz-mux/src/command.rs` | inline in the dispatch `match` |
| `inspect` | `crates/zz-daemon/src/daemon/inspect.rs` | `inspect` |
| `join-pane` | `crates/zz-mux/src/command/pane_spawn.rs` | `join_pane` |
| `kill-pane` | `crates/zz-mux/src/command/panes.rs` | `kill_pane` |
| `kill-server` | `crates/zz-mux/src/command.rs` | inline in the dispatch `match` |
| `kill-session` | `crates/zz-mux/src/command/sessions.rs` | `kill_session` |
| `kill-window` | `crates/zz-mux/src/command/windows.rs` | `kill_window` |
| `last-pane` | `crates/zz-mux/src/command/panes.rs` | `last_pane` |
| `last-window` | `crates/zz-mux/src/command/windows.rs` | `last_window` |
| `list-buffers` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `list-clients` | `crates/zz-daemon/src/daemon/clients.rs` | `list_clients` |
| `list-commands` | `crates/zz-mux/src/command/key_bindings.rs` | `list_commands` |
| `list-hosts` | `crates/zz-daemon/src/daemon/command_routing.rs` | refused, the zz CLI runs it |
| `list-keys` | `crates/zz-mux/src/command/key_bindings.rs` | `list_keys` |
| `list-panes` | `crates/zz-mux/src/command/panes.rs` | `list_panes` |
| `list-sessions` | `crates/zz-mux/src/command/sessions.rs` | `list_sessions` |
| `list-windows` | `crates/zz-mux/src/command/windows.rs` | `list_windows` |
| `load-browser-url` | `crates/zz-daemon/src/daemon/gui_requests.rs` | `load_browser_url` |
| `load-buffer` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `lock-client` | `crates/zz-daemon/src/daemon/clients.rs` | `lock_command` |
| `lock-server` | `crates/zz-daemon/src/daemon/clients.rs` | `lock_command` |
| `lock-session` | `crates/zz-daemon/src/daemon/clients.rs` | `lock_command` |
| `move-pane` | `crates/zz-mux/src/command/pane_spawn.rs` | `join_pane` |
| `move-window` | `crates/zz-mux/src/command/windows.rs` | `move_window` |
| `new-pane` | `crates/zz-mux/src/command/pane_spawn.rs` | `new_pane` |
| `new-session` | `crates/zz-mux/src/command/sessions.rs` | `new_session` |
| `new-window` | `crates/zz-mux/src/command/windows.rs` | `new_window` |
| `next-layout` | `crates/zz-mux/src/command/panes.rs` | `select_layout` |
| `next-window` | `crates/zz-mux/src/command/windows.rs` | `step_window` |
| `paste-buffer` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `pipe-pane` | `crates/zz-daemon/src/daemon/pane_pipes.rs` | `pipe_pane` |
| `press-simulator` | `crates/zz-daemon/src/daemon/simulators.rs` | `simulator_command` |
| `previous-layout` | `crates/zz-mux/src/command/panes.rs` | `select_layout` |
| `previous-window` | `crates/zz-mux/src/command/windows.rs` | `step_window` |
| `providers` | `crates/zz-daemon/src/daemon/providers.rs` | `providers` |
| `read-file` | `crates/zz-daemon/src/daemon/file_commands.rs` | `read_file` |
| `refresh-client` | `crates/zz-daemon/src/daemon/clients.rs` | `refresh_client` |
| `reload-config` | `crates/zz-mux/src/command.rs` | inline in the dispatch `match` |
| `rename-session` | `crates/zz-mux/src/command/sessions.rs` | `rename_session` |
| `rename-window` | `crates/zz-mux/src/command/windows.rs` | `rename_window` |
| `resize-pane` | `crates/zz-mux/src/command/panes.rs` | `resize_pane` |
| `resize-window` | `crates/zz-mux/src/command/windows.rs` | `resize_window` |
| `respawn-pane` | `crates/zz-mux/src/command/panes.rs` | `respawn_pane` |
| `respawn-window` | `crates/zz-mux/src/command/panes.rs` | `respawn_window` |
| `restart-server` | `crates/zz-daemon/src/daemon/restart.rs` | `restart_server` |
| `rotate-window` | `crates/zz-mux/src/command/panes.rs` | `rotate_window` |
| `run-pane` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `run_pane` |
| `run-shell` | `crates/zz-daemon/src/daemon/shell_jobs.rs` | `run_shell` |
| `save-buffer` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `select-child-chat` | `crates/zz-daemon/src/daemon/agent_chats.rs` | `select_child_chat` |
| `select-layout` | `crates/zz-mux/src/command/panes.rs` | `select_layout` |
| `select-pane` | `crates/zz-mux/src/command/panes.rs` | `select_pane` |
| `select-pane-kind` | `crates/zz-mux/src/command/pane_spawn.rs` | `select_pane_kind` |
| `select-window` | `crates/zz-mux/src/command/windows.rs` | `select_window` |
| `send-keys` | `crates/zz-mux/src/command/send_keys.rs` | `send_keys` |
| `send-last-output` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `send_last_output` |
| `send-prefix` | `crates/zz-mux/src/command/send_keys.rs` | `send_prefix` |
| `send-text` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `send_text` |
| `server-access` | `crates/zz-mux/src/command/sessions.rs` | `server_access` |
| `set-browser-profile` | `crates/zz-mux/src/command/pane_spawn.rs` | `set_browser_profile` |
| `set-browser-tabs` | `crates/zz-mux/src/command/pane_spawn.rs` | `set_browser_tabs` |
| `set-browser-url` | `crates/zz-mux/src/command/pane_spawn.rs` | `set_browser_url` |
| `set-buffer` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `set-editor-path` | `crates/zz-mux/src/command/pane_spawn.rs` | `set_editor_path` |
| `set-environment` | `crates/zz-mux/src/command/environment.rs` | `set_environment` |
| `set-hook` | `crates/zz-mux/src/command/hooks.rs` | `set_hook` |
| `set-option` | `crates/zz-mux/src/command/set_option.rs` | `set_option` |
| `set-simulator-device` | `crates/zz-mux/src/command/pane_spawn.rs` | `set_simulator_device` |
| `set-window-option` | `crates/zz-mux/src/command/set_option.rs` | `set_option` |
| `show-buffer` | `crates/zz-daemon/src/daemon/buffers.rs` | `buffer_command_for_client` |
| `show-environment` | `crates/zz-mux/src/command/environment.rs` | `show_environment` |
| `show-hooks` | `crates/zz-mux/src/command/hooks.rs` | `show_hooks` |
| `show-last-output` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `show_last_output` |
| `show-messages` | `crates/zz-daemon/src/daemon/history_io.rs` | `show_messages` |
| `show-options` | `crates/zz-mux/src/command/show_options.rs` | `show_options` |
| `show-prompt-history` | `crates/zz-daemon/src/daemon/history_io.rs` | `prompt_history_command` |
| `show-window-options` | `crates/zz-mux/src/command/show_options.rs` | `show_options` |
| `source-file` | `crates/zz-mux/src/command.rs` | `source_file` |
| `split-window` | `crates/zz-mux/src/command/pane_spawn.rs` | `split_window` |
| `start-server` | `crates/zz-mux/src/command.rs` | inline in the dispatch `match` |
| `suspend-client` | `crates/zz-mux/src/command/sessions.rs` | `suspend_client` |
| `swap-pane` | `crates/zz-mux/src/command/panes.rs` | `swap_pane` |
| `swap-window` | `crates/zz-mux/src/command/windows.rs` | `swap_window` |
| `swipe-simulator` | `crates/zz-daemon/src/daemon/simulators.rs` | `simulator_command` |
| `switch-client` | `crates/zz-daemon/src/daemon/clients.rs` | `switch_client` |
| `switch-mode` | `crates/zz-mux/src/command/modes.rs` | `switch_mode` |
| `tailnet` | `crates/zz-daemon/src/tailnet.rs` | `crate::tailnet::command` |
| `tap-simulator` | `crates/zz-daemon/src/daemon/simulators.rs` | `simulator_command` |
| `tools` | `crates/zz-daemon/src/daemon/command_routing.rs` | `workspace_tools_catalog` |
| `type-simulator` | `crates/zz-daemon/src/daemon/simulators.rs` | `simulator_command` |
| `unbind-key` | `crates/zz-mux/src/command/key_bindings.rs` | `unbind_key` |
| `update` | `crates/zz-daemon/src/daemon/command_routing.rs` | refused, the zz CLI runs it |
| `update-server` | `crates/zz-daemon/src/daemon/update_server.rs` | `update_server` |
| `view-pane` | `crates/zz-mux/src/command/pane_spawn.rs` | `view_pane` |
| `wait-for` | `crates/zz-daemon/src/daemon/wait_channels.rs` | `wait_for_in` |
| `wait-pane` | `crates/zz-daemon/src/daemon/agent_commands.rs` | `wait_pane` |
| `write-file` | `crates/zz-daemon/src/daemon/file_commands.rs` | `write_file` |
