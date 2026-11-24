# tmux

<!-- okf:listing:start (managed by okf.py index — edit prose outside this fence) -->
# Concepts

* [Sidebar navigation and native choosers](choose-tree.md) - How tree, client and buffer choosers build rows, handle selection templates and appear on desktop or in the TUI.
* [tmux command set](commands.md) - Find a command, follow alias and target resolution, and identify which component executes it.
* [tmux config parsing and replay](conf-parser.md) - How zz chooses mux config files, parses commands and assignments, reports failures and reapplies stored overrides.
* [Copy mode and view mode](copy-mode.md) - How a client navigates frozen pane history, copies text and handles output views and application clipboard writes.
* [tmux divergence matrix](divergences.md) - How to find a compatibility decision and its evidence, with examples of differences observable in zz.
* [tmux compatibility gap report](gaps.md) - Live TODO and status report for tmux compatibility gaps, decisions, evidence, and acceptance gates.
* [Key tables](key-tables.md) - How zz looks up a key, changes a live binding, and keeps client-specific prefix and copy-mode state.
* [tmux status rows and format expansion](status-line.md) - Where tmux status formats get their values, how the raw TUI draws the rows, and which settings control the desktop bar.
* [tmux compatibility philosophy](tmux-compat.md) - What a tmux command promises in zz, which client draws it, and where to check a measured exception.
* [TUI parity campaign](tui-parity.md) - TUI parity obligations, their proof status, and progress against the fixed baseline.
<!-- okf:listing:end -->
