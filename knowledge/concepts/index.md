# Concepts

<!-- okf:listing:start (managed by okf.py index — edit prose outside this fence) -->
# Concepts

* [Agent lens](agent-lens.md) - How a terminal pane running Claude Code or Codex shows an agent timeline. Covers the header toggle, the daemon lens runtime that tails the CLI's own transcript, the agent stream that carries it to clients, prompts pasted into the TUI, Claude permission prompts, and what agent-send and agent-respond do with it.
* [Unified palette and tmux command completions](command-palette.md) - The palette's line grammar (a place typed with tmux's sigils, then an action), chips and keys, the & files level that prefix F opens, the empty palette and recents, the written @host:target form, shared zz-ui presentation with key glyphs per platform, and preserved tmux prompt behavior.
* [Daemon-owned PTY worker model](pty-worker.md) - How the daemon spawns and owns one PTY-backed terminal session per pane, the thread/ownership boundary between zz-daemon and the zz-terminal worker, and the paths that carry terminal frames out and send-keys in.
* [Session persistence & daemon lifecycle](session-persistence.md) - Why mux state and terminals outlive GPUI windows, which GUI state is disk-backed, which browser descriptors can restore, what the agent lens rebuilds, and how attach, detach, eviction, per-client state, and local transport work when several devices share one session.
* [Cell-authoritative split-pane layout](split-pane-layout.md) - How a window's panes are arranged as an n-ary cell tree ported from tmux's layout.c — split/resize/preset algorithms in layout.rs, the derived binary wire projection with stable ^split ids, measurement-driven window extent, and the invariants model.rs enforces.
* [Terminal frame (TerminalViewport)](terminal-frame.md) - The immutable, renderer-neutral terminal snapshot (packed cells, interned styles, overlays, cursor, and modes) published from the worker thread and diffed into retained-grid patches.
<!-- okf:listing:end -->
