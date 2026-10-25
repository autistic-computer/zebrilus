---
type: Concept
title: Agent lens
description: How a terminal pane running Claude Code or Codex shows an agent timeline. Covers the header toggle, the daemon lens runtime that tails the CLI's own transcript, the agent stream that carries it to clients, prompts pasted into the TUI, Claude permission prompts, and what agent-send and agent-respond do with it.
resource: crates/zz-daemon/src/agent/lens.rs
tags: [agent, lens, claude-code, codex, transcript, terminal, pane]
timestamp: 2026-10-24T00:00:00-03:00
---

# Overview

zz has no agent pane kind. Claude Code and Codex run as their own TUIs in ordinary terminal panes,
and a terminal whose foreground program is `claude` or `codex` gets a lens button in its header.
With the lens on, the pane body shows that agent's timeline and a composer in place of the cell
grid; clicking again shows the terminal. The terminal process stays the source of truth: the
timeline is read from the transcript the CLI already writes, and the composer types into the TUI.

The rulings, the full deletion map and the history of the earlier attempt are in the
[agent lens design](/designs/agent-lens.md). Both halves are on main (merged in `d20c49f4b`).

Until 2026-10-23 zz had a separate Agent pane (`PaneKindSnapshot::Agent`) that spawned an ACP
adapter or a native driver per pane, with project, history, provider, model and effort pickers.
That pane, its pickers, its text projection terminal, its tree status record and the subagent PiP
feeds are gone. The [daemon-owned agent runtime](/designs/agent-daemon-runtime.md) and the
[native drivers](/designs/native-agent-drivers.md) record how it worked.

# Which panes get a lens

- **Client.** `zz_client::window_summary::pane_agent` maps `pane_program` (the terminal's
  `PaneSummary.process`, wire v112) to `claude` or `codex`. The toggle is per client and not
  synced, so one client can show the lens while another shows the terminal. The header button is
  `zz_ui::pane::pane_lens_button` (`PaneLens`, sent as `TerminalPaneAction::Lens`), an eye (Solar's
  `eye`) for either agent, drawn in the
  `pane.header.lens` role, which tints it while the lens is open.
- **Phone.** In the compact shell (iPhone) the lens opens by default for a terminal running an
  agent, since typing into a TUI with a phone keyboard is slow. Closing it with the toggle keeps that
  pane on its terminal until the agent exits (`CompactState::lens_dismissed` in
  `clients/app/src/app/compact.rs`). Desktop, web and iPad open it only from the toggle.
- **Daemon.** `lens_provider` in `crates/zz-daemon/src/daemon/lens.rs` looks for a `claude` or
  `codex` basename in the pane's process list (case-insensitive, `.exe` stripped), and
  `terminal_agent` only answers for `PaneKind::Terminal`.

# Opening the lens runtime

`Shared::open_lens(pane)` runs when a client sends `AgentReplay` for a terminal's own `%pane`, and
when `agent-send --wait`, `--notify`, `--final` or `--on-block allow|deny` targets one. It adds the
pane to `lens_panes` and opens the agent runtime with an `AgentPaneSpec` whose `lens` field carries
the pane, the provider and a `TerminalLens` (the pane's pid, a paste-and-submit path and a key
path). `AgentHost::open` sees the `LensSpec` and runs `agent::lens::run_lens` on the pane's
`zz-agent-{n}` thread instead of starting a process.

From there the path is the one the old Agent pane used: host pump, fanout lane, `AgentUpdates` and
`AgentState` events, and the client reducers. The runtime lives until the pane closes
(`close_agent_panes` drops it from `lens_panes`). Agent messages for any pane that is not a
terminal are refused (`is not a terminal pane`), and a replay for a terminal running neither CLI
answers `is not running Claude Code or Codex`. The lens is Unix only; elsewhere `open_lens` returns
false.

`open_lens` is the only production caller of the agent runtime. The native Claude and Codex
drivers, the ACP runtime and the [turn machine](/designs/agent-turn-machine.md) stay in the source
for the driver chat, but no pane opens them now.

# Where the timeline comes from

`agent/lens.rs` reads files; it never starts a process.

- **Claude Code.** The peer record `~/.claude/sessions/<pid>.json` (read by `claude_peers.rs`) maps
  the pane to its Claude process, session id, cwd and status. The transcript is
  `~/.claude/projects/<sanitized cwd>/<session>.jsonl` (`claude/sessions.rs::session_file`). On open
  the runtime reads the last 8 MiB, walks the `parentUuid` chain from the newest leaf
  (`sessions::chain_tail`) and feeds the entries through `claude::translate::Translator::history_entry`,
  the translator the native driver uses for resume. New lines are tailed every 250 ms. A new session
  id in the record (`/clear`, `/resume`) becomes a session switch with a fresh replay, and a prompt
  whose parent is not the last line seen (`/rewind`) reloads the chain. A `system` /
  `turn_duration` line marks a finished turn.
- **Codex.** The runtime looks for the rollout the pane's `codex` process holds open, through
  `/proc/<pid>/fd` on Linux or `lsof -p` on macOS, and takes the one whose `session_meta` has no
  parent. Codex 0.164 on macOS leaves the file to its shared app-server (`~/.codex/packages`), so the
  runtime falls back to the newest parentless rollout in the last 7 day folders of
  `$CODEX_HOME/sessions` whose cwd matches the pane and that was modified since the process started
  (`newest_rollout`); two Codex panes in one folder can then pick each other's session.
  `codex/rollout.rs` (`RolloutMeta`, `v2_item`) reshapes `item_completed` items and
  `codex::translate::Translator::history_item` translates them. `task_started` marks the pane busy;
  `task_complete` and `turn_aborted` mark it idle. A `task_complete` that carries an `error` (an API
  error, such as a model the login does not allow) fails the lens turn with Codex's message, so
  `agent-send --wait` exits 1. While a lens turn waits on a pane whose rollout is not found yet,
  the runtime looks on every tick instead of every 5 s, and a rollout found after its turn already
  ended settles that turn from its last `task_complete`.

Claude's session record carries `status` (`busy`, `idle`, `waiting`). The runtime turns it into
`Activity { busy }` events, so the pane's phase follows the TUI whether the turn came from the
lens, from typing in the terminal, or from a peer message.

# Input and approvals

- **Prompts.** The composer sends `AgentPrompt` as before. The runtime delivers it with
  `TerminalLens::submit`, the bracketed paste plus Enter that waits for the text to echo before
  pressing Enter (`paste_and_submit`). Images are refused with "The lens sends text only; paste
  images in the terminal." Slash commands are the CLI's own and are typed through. The composer's
  placeholder names the agent ("Message Claude Code…"), and before the agent has written a
  transcript the lens shows `presentation::lens_welcome`: the agent's mark, "Claude Code is waiting
  for its first prompt" and "What you send here is typed into the terminal."
- **Stop.** `AgentCancel` sends Escape to the TUI.
- **Claude permission prompts.** While the dialog is up, the record reads `"status":"waiting"` with
  `"waitingFor":"permission prompt"`, and the pending `tool_use` is the last assistant block with no
  `tool_result`. The runtime raises a permission request for that tool with Allow (sends `1`) and
  Deny (sends Escape). Any other `waitingFor` (a plan approval, a question) raises a request with no
  options, titled "Waiting for a ... in the terminal". Clients draw it with
  `zz_ui::agent::presentation::terminal_wait_card`, whose one action, Show terminal, switches the
  pane back to the TUI so the user answers there.
- **Codex approvals.** The rollout records nothing while Codex's dialog waits, so the lens shows no
  card; the user answers in the terminal.

# How the stream reaches a client

The delivery model is unchanged from the agent pane; the full contract is in the
[daemon-owned agent runtime](/designs/agent-daemon-runtime.md) and the byte bounds are in the
[wire protocol](/protocol/wire-protocol.md).

- **Coalescing.** The fanout gathers items into 25 ms windows (`BATCH_WINDOW` in
  `agent/fanout.rs`) on one shared flush thread that parks when nothing is gathered. Frames are
  capped at `MAX_AGENT_UPDATES_BYTES` (9 MiB).
- **Sequencing.** The fanout mints the per-pane sequence. A client keeps a cursor per pane, drops
  items at or below it, and asks for a replay when a batch starts past it.
- **A dedicated lane.** Agent frames have their own outbound slot, capped at
  `MAX_PENDING_AGENT_BYTES` (9 MiB + 64 KiB) per pane. Overflow drops the pane's queued frames and
  queues `AgentLagged`, so a slow client replays instead of being disconnected.
- **Replay.** Each pane keeps an 18 MiB ring of encoded items (`MAX_REPLAY_RING_BYTES`); a replay
  older than the ring takes the reset path the runtime design describes. When the lens runtime
  opens, it rebuilds the history from the CLI's own file.
- **Visibility.** The heavy stream goes to clients for whom the pane is a lens pane in their
  focused window, honoring zoom (`visible_agent_panes`), and to clients watching the pane.
  `AgentState` goes to every client attached to the session and to the pane's watchers, and a
  reattaching client gets it for every lens pane there (`send_agent_resync`, `session_agent_panes`).
- **Watchers.** A client that sends `AgentReplay` for a pane watches it from then on, attached or
  not (`watched_agents` on the client entry): it gets the pane's `AgentUpdates`, `AgentState` and
  `AgentLagged` until it disconnects or the pane closes, and the pane's `AgentState` right away when
  the watch starts. This is how the desktop's driver chat follows a Driver Agent on another host without
  attaching to its session, which would resize its panes, and how an attached client keeps a lens
  live while its focused window shows something else. A client that both sees and watches a pane is
  sent each frame once, and a watched pane leaving the visible set keeps its queued frames. There is
  no unwatch message.

# From outside: the CLI

The daemon's `zz tools agent` text (`WORKSPACE_TOOLS_AGENT` in `crates/zz-daemon/src/daemon.rs`) is
the reference; the [command set](/tmux/commands.md) has the rows.

- `agent-send [-t %N] [TEXT]` sends to the agent in a terminal: Claude Code through its peer socket
  ([agent peer bus](/architecture/agent-peer-bus.md)), Codex through `codex queue`, otherwise typed
  into its input.
- `--wait`, `--notify`, `--final`, `--json` and `--on-block` go through the pane's lens: the text is
  typed into the CLI and the transcript says when the turn ends. A terminal running neither CLI
  falls back to waiting on `@agent_state`, and `--notify`, `--final` and `--on-block allow|deny`
  exit 2 there. `--on-block allow|deny` answers a Claude permission prompt with the keys above.
  `--wait ''` (no text) skips the lens: it waits on `@agent_state` for the running turn and prints
  nothing, so a caller can wait again after a timeout without typing anything.
- `agent-respond` answers a pending permission only in a pane whose lens is open (in a client or
  through a running `agent-send --wait`); other panes exit 1 with `has no agent lens open`.
- `inspect` lists `agent-respond` in `verbs` for a pane with an open lens, and its `permission`
  object comes from the lens runtime. `events` emits `agent-tool-call` for terminals with an open
  lens.
- `send-last-output -t %N` types a terminal's last command and output into the window's most
  recently focused terminal running Claude Code or Codex (`recent_agent_terminal`), without Enter.
- `#{agent_state}` reads a terminal's `@agent_state` (fed by OSC 7501, OSC 9;4 progress and the
  Claude peer sampler); it is empty for other pane kinds. `#{agent_pending_permission}` is always
  empty now. The lens writes no `@agent_state` and no tree status record.

# What the clients draw

The kept pieces are `AgentTimeline` and its rows, the permission and question cards,
`AgentComposer` and the task tray (`crates/zz-ui/src/agent.rs` and `crates/zz-ui/src/agent/`). The
timeline mechanics carried over unchanged: the `TimelineStick` / `StickSpring` follow-the-tail pin
(`agent.rs`), the display-only Markdown repair for streaming text (`zz-ui/src/mend.rs`), the
streamed-text fade (`zz-gpui-kit/src/text/veil.rs`) and the shared leased animation clock
(`zz-gpui-kit/src/pulse.rs`).

- **Desktop.** `crates/zz/src/agent` (controller and view) behind the `agent-pane` cargo feature,
  which stays on by default; without it a stub stands in.
- **Web and iPad.** `clients/app/src/app/agent_lens.rs`.
- **Storybook.** An agent lens story replaces the Agent pane story.

The lens has no project or directory picker and no provider, model or effort picker; that code was
deleted, not hidden.

# Related

- [Agent lens design](/designs/agent-lens.md) . rulings, deletion map, follow-ups
- [Daemon-owned agent runtime](/designs/agent-daemon-runtime.md) . host, fanout, journal
- [Native agent drivers](/designs/native-agent-drivers.md) . the translators the lens reads with
- [Agent peer bus](/architecture/agent-peer-bus.md)
- [Wire protocol](/protocol/wire-protocol.md)
- [tmux command set](/tmux/commands.md)
- [zz](/crates/zz.md) and [zz-daemon](/crates/zz-daemon.md)
