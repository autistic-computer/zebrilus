---
type: Subsystem
title: tmux command set
description: Find a command, follow alias and target resolution, and identify which component executes it.
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
