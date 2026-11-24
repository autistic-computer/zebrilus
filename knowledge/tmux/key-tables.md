---
type: Subsystem
title: Key tables
description: How zz looks up a key, changes a live binding, and keeps client-specific prefix and copy-mode state.
resource: crates/zz-protocol/src/key.rs
tags: [tmux, keys, bindings, prefix, copy-mode, choosers]
timestamp: 2026-11-24T00:00:00-03:00
---

# Looking up a binding

Use `zz list-keys -T prefix` for the current prefix table, `zz list-keys -T copy-mode-vi` for vi copy keys, or `zz list-keys -N` for binding notes. `KeyTables::default` in `crates/zz-protocol/src/key.rs` defines the shipped bindings. The pinned tmux tables live in `compat/tmux-oracle.json`. The `keys.*` groups in the [gap registry](/tmux/gaps.md) explain zz's additions and deliberate differences.

The daemon owns mutable named tables and a `KeyEngine` for each client. It returns the binding's command list, arms the prefix, ignores the key or passes it to the surface. The default prefix is `C-b`, with no second prefix. A configured `prefix2` also arms the prefix table. `send-prefix` sends the primary prefix through to the pane and `send-prefix -2` selects the secondary one. Two attached clients can be in different tables at the same time.

# Changing bindings

| Config command | Effect |
| --- | --- |
| `bind-key c new-window` | Bind a key in the prefix table. |
| `bind-key -n F2 split-window -h` | Bind in the root table, without a prefix. |
| `bind-key -r -T prefix Right resize-pane -R` | Allow this binding to repeat inside the configured repeat window. |
| `unbind-key -T prefix Right` | Remove that binding from the named table. |

`-T` selects a named table, including a custom one, and `-N` supplies a note. An exact binding wins over the table's `Any` fallback. The shared `input_key_name` and `canonical_key` functions handle key names and modifiers. For layout-sensitive keys, use the parsed key and actual typed text when diagnosing why an unshifted binding and a shifted symbol behave differently. A vi repeat count or a jump waiting for its target belongs to the client that started it.

Clients receive the initial table snapshot and updates for changed or removed tables through the [wire protocol](/protocol/wire-protocol.md). Prefix hints should use those bindings so a rebind changes what users see. `crates/zz-client/src/prefix_keys.rs` reads them for labels and which-key hints. Client-side chrome bindings live in `crates/zz-client/src/chrome.rs`. `crates/zz-daemon/src/keys.rs` translates overlay keys, including custom chooser bindings and fixed search-editor keys.

For a key that runs the wrong command, check the effective prefix, the client's current table and `list-keys` output before changing the terminal input encoder. For attached-client proof on real input, see `compat/tui-stock-keys.sh` and `compat/tui-copy-mode.sh`. See also [copy mode](/tmux/copy-mode.md), [choosers](/tmux/choose-tree.md) and [config loading](/tmux/conf-parser.md).
