# Concepts

<!-- okf:listing:start (managed by okf.py index — edit prose outside this fence) -->
# Concepts

* [Agent panes and the flip](agent-panes.md) - Native agent chats, switching to a provider's terminal UI, accounts, inter-pane messages and replay.
* [Unified palette and tmux command completions](command-palette.md) - Searching places and commands, filling arguments, picking files, and editing daemon prompts.
* [Daemon-owned PTY workers](pty-worker.md) - How terminal actors share worker threads and route each viewer's input and frames.
* [Session persistence and daemon lifecycle](session-persistence.md) - What survives detach, which state belongs to each viewer and what ends when the daemon restarts.
* [Cell-authoritative split-pane layout](split-pane-layout.md) - How the mux measures tiled and floating panes, identifies dividers, and restores layout strings.
* [Terminal frame (TerminalViewport)](terminal-frame.md) - The renderer-neutral terminal viewport and how clients safely apply row patches.
<!-- okf:listing:end -->
