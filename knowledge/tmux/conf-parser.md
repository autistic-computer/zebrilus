---
type: Subsystem
title: tmux config parsing and replay
description: How zz chooses mux config files, parses commands and assignments, reports failures and reapplies stored overrides.
resource: crates/zz-mux/src/parser.rs
tags: [tmux, parser, config, tokenizer, mux-conf]
timestamp: 2026-11-26T00:00:00-03:00
---

# Config selection

By default the daemon loads the first existing zz-owned `zz/mux.conf`. Explicit startup `-f` files replace default discovery and are processed in argument order. `reload-config` repeats the selected policy. Query `zz display-message -p '#{config_files}'` to see the selected files. The search and write paths live in `crates/zz-daemon-client/src/paths.rs` (`default_mux_config`, `mux_config_write_path`).

`import-tmux-config [path]` brings an existing tmux config into a marked block in zz's own file, then reloads it. Without a path, `discover_tmux_config` picks the first existing donor. Re-import replaces the block, preserves surrounding user content and comments unsupported commands with `# zz-unsupported:`. The daemon's `import_tmux_configuration` owns the write.

# Syntax and tokenization rules

The parser reads the complete input. It handles quoted and empty words, backslash escapes, continued lines, unquoted command separators, comments at a word boundary, environment assignments and expansion, typed brace blocks and conditional directives. A quoted brace expression stays a string. The [command layer](/tmux/commands.md) determines where a typed block is legal.

```tmux
set -g prefix C-a
bind -n F2 split-window -h
bind c new-window -n 'my window'
set -g word-separators ""
```

`ParsedConfig` in `crates/zz-mux/src/parser.rs` separates commands, ordered environment assignments and source-located diagnostics. `parse_config_characters` keeps the first syntax diagnostic and drops the file's command list. Assignments reduced before the fault can survive. The daemon distinguishes startup file input from a sourced buffer, including treatment of non-UTF-8 bytes. The `options.byte-path-residue` entry in the [registry](/tmux/gaps.md) identifies remaining text conversions.

# Daemon replay and zz/config overrides

The daemon prepares a file's command groups before executing their effects. A syntax or construction error can therefore reject an entire file. An error in an executing command has different scope. Each sibling source file gets its own preparation, and a nested file uses the alias state visible when it is loaded. Use `source-file -n` to parse without executing commands, `-v` for verbose output, `-q` for a quiet missing path and `-F` to expand a format in the path.

The daemon applies settings in this order.

1. Built-in mux defaults.
2. The chosen mux file or explicit startup files.
3. Stored mux overrides from the [application config](/configuration/app-config.md).

The stored overrides are also reapplied after `reload-config` and after an interactive `source-file`. A successful runtime command can then change the option again. For example, if a sourced prefix keeps reverting, inspect the settings override. `apply_stored_mux_config_overrides` in `crates/zz-daemon/src/daemon/config.rs`, and `replay_mux_config_files` and `parse_config_input` in `crates/zz-daemon/src/daemon/config_loading.rs`, own these transitions.

For an input grammar change, run the parser's closest test or the `smoke/config-grammar` scenario. Use `smoke/source-replay-diagnostics` for execution-time diagnostics and `compat/startup-diagnostics.sh` for causes delivered to a client on attach. See the [harness](/playbooks/compat-harness.md) and [key tables](/tmux/key-tables.md).
