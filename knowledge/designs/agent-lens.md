---
type: Design Plan
title: Agent lens . the agent view of a terminal pane
description: "zz stops drawing its own agent pane. Claude Code and Codex run as their real TUIs in ordinary terminal panes, and a terminal running one of them gets a lens button in its header that swaps the cell grid for the agent timeline of that same pane. The daemon tails the transcript the CLI already writes (Claude's session jsonl, the rollout Codex holds open) through the existing translators, the lens composer pastes into the TUI, and a Claude permission prompt is answered with the keys the TUI takes. The Agent pane kind, its pickers and every place that treats it as a pane type are deleted; the native drivers stay for the driver chat."
status: Built and merged to main in d20c49f4b (2026-10-23; replaces the abandoned 2026-09-17 attempt)
resource: crates/zz-daemon/src/agent/lens.rs
tags:
- agent
- agent-pane
- claude-code
- codex
- transcript
- design-plan
timestamp: 2026-10-23T00:00:00Z
---

# Rulings (fabrico, 2026-10-23)

1. The vendors' own TUIs are the workers. zz no longer draws a full agent pane; it adds a lens over
   terminal panes and, later, a driver chat in the sidebar (design canvas
   https://claude.ai/artifact/7HM478qCqYwJePXn82fcxa).
2. A terminal pane whose foreground program is Claude Code or Codex shows a lens button in its
   header. Clicking it renders the agent timeline for that pane; clicking again shows the terminal.
   The terminal process stays the source of truth.
3. The lens has no project or directory picker and no provider, model or effort picker. That code
   is deleted, not hidden.
4. Nothing may treat the agent as its own pane type any more: no `PaneKindSnapshot::Agent`, no
   `PaneTag::Agent`, no desktop or thin-client agent pane, no agent pane stories.
5. The daemon agent runtime (native drivers, turn machine, journal, peers, codex queue, fanout)
   stays, because the driver chat will run on it. A part goes only when nothing but the deleted pane
   used it.

# How the lens works

## Which panes get it

The client decides from data it already has: `zz_client::window_summary::pane_program` resolves a
terminal's `PaneSummary.process` to a program, and the ids `claude` and `codex` get the button. The
toggle is per client and is not synced: one client can look at the lens while another shows the
terminal.

## Where the timeline comes from

Opening the lens sends `AgentReplay` for the terminal's own `%pane`. For a terminal pane the daemon
first opens a lens runtime in the agent host, keyed by that pane id, so the rest of the path is the
one the agent pane used: host pump, journal sequence, fanout lane, `AgentUpdates` and `AgentState`
events, and the client reducers. The runtime lives until the pane closes.

The lens runtime (`agent/lens.rs`) is a `PaneRunner` like the native drivers. It never starts a
process; it reads files.

- **Claude Code.** `~/.claude/sessions/<pid>.json` (the peer registry `claude_peers.rs` already
  reads) maps the pane to its Claude process, session id, cwd and status. The lens picks the record
  whose process runs under the pane's process, nearest first (`record_in_pane`); it does not need
  the record's `tmux` field, which Claude writes only when a `tmux` on its PATH answers. The transcript is
  `~/.claude/projects/<sanitized cwd>/<session>.jsonl` (`claude/sessions.rs::session_file`). The
  runtime reads the last 8 MB, walks the `parentUuid` chain from the newest leaf, and feeds the
  entries through `claude::translate::Translator::history_entry`, the same translator the native
  driver uses for resume. New lines are tailed every 250 ms. A new session id in the record
  (`/clear`, `/resume`) becomes a session switch with a fresh replay; a new prompt whose parent is
  not the last line seen (`/rewind`) reloads the chain.
- **Codex.** Older Codex TUIs hold their rollout open (`lsof -p` on macOS, `/proc/<pid>/fd` on
  Linux). Codex 0.164 writes rollouts from a shared `codex app-server` daemon instead, and creates
  the file at the first prompt; then the runtime takes the newest rollout under
  `$CODEX_HOME/sessions` (last 7 day folders) with no parent, the TUI's working directory, and a
  write since the TUI started. It reads `event_msg` / `item_completed` lines and translates them
  with the Codex translator
  (`codex/rollout.rs::v2_item` plus `codex::translate::Translator::history_item`). `task_started`
  and `task_complete` mark the turn.

## Busy, idle and turns

Claude's session record carries `status` (`busy`, `idle`, `waiting`) and, while waiting,
`waitingFor`. The runtime turns those into `Activity` events, so the host phase follows the TUI
whether the turn came from the lens, from typing in the terminal, or from a peer message. The
transcript's `system` / `turn_duration` line marks a finished turn. Claude marks its record idle
before the last transcript lines reach the disk, so idle alone ends a lens turn only after two
seconds without `turn_duration` (an interrupted turn). Codex uses `task_started` and
`task_complete`.

## Input

The lens composer sends `AgentPrompt` as before. The lens runtime delivers it with
`submit_agent_text`, the bracketed paste plus Enter that `agent-send` already uses for terminals,
which waits for the text to echo before pressing Enter. The prompt line that comes back in the
transcript is matched against what was sent, so the user bubble shows once. Images are refused
(the session capabilities say so); slash commands are the CLI's own and are typed through. Stop
sends Esc.

## Approvals

Verified on 2026-10-23 against Claude Code 2.1.309 and Codex 0.163.0 in a throwaway tmux server:

- **Claude.** While its permission dialog is up, the session record reads
  `"status":"waiting","waitingFor":"permission prompt"`, and the pending `tool_use` is already the
  last assistant block in the transcript with no `tool_result`. In the dialog, `1` approves at once
  and Esc denies (the transcript then gets the rejected `tool_result` and
  `[Request interrupted by user for tool use]`, and the turn ends). The lens therefore shows a real
  permission card for that tool with Allow (`1`) and Deny (Esc). Live runs on 2026-10-23 showed
  Claude sometimes writes the `tool_use` line only after the dialog closes; the card then reads
  "Claude Code asks for permission; the terminal shows what for", still answers with `1` or Esc,
  and is replaced by the real card if the line lands first. Any other `waitingFor` shows a card
  titled "Waiting for a ... in the terminal" whose one action, Show terminal (added 10-24), switches
  the pane back to the TUI; the user answers there. A record
  caught mid-write keeps the last status while the process lives, so a torn read never ends a turn.
- **Codex.** The TUI's approval dialog takes `y`, `p` and Esc, but the rollout records nothing while
  it waits (the last line is the `custom_tool_call`), so zz cannot see that a dialog is up. The Codex
  lens shows no card; the user answers in the terminal.

## What the clients draw

The terminal header (`zz_ui::pane::pane_header`) gains a lens toggle when the program is Claude or
Codex, on desktop, web/iPad and PiP headers. With the lens on, the pane body is the timeline and the
composer under the same terminal header. The lens closes by itself when the pane goes away or the
agent CLI stops being its foreground program. A client tracks the stream only for lenses it shows:
opening sends a replay from seq 0 and starts a cursor, closing drops the cursor and anything
buffered, so reopening replays from the start. The pieces kept for the driver chat are
`AgentTimeline` and its rows (`render_user_entry`, `render_answer`), `permission_card`, the question
card, `AgentComposer` and the task tray.

# Map and deletion plan

## Deleted

| Area | What goes |
|---|---|
| zz-protocol | `PaneKindSnapshot::Agent`, `AgentDescriptor`, `ChooseTreePaneKind::Agent`, the `--kind agent` / `--provider` / `select-pane-kind -c` catalog entries, `new-agent-session`, `set-agent-provider`, `restart-agent-pane`, the `experimental-agent-pane` option. Wire version 112 to 113 |
| zz-mux | `PaneKind::Agent`, its spawn and `select-pane-kind` branches, `update_agent_cwd` / `update_agent_session` / `update_agent_provider`, `recent_agent_pane`, `MuxEffect::AgentPaneRestart`, the experimental flag in `AgentOptions` |
| zz-daemon | agent pane open/restart/close on pane effects, `agent_pane_spec`, the shadow projection terminal (`feed_agent_pane_text`, see [agent pane projection](/designs/agent-pane-projection.md)), `new_agent_session`, the subagent PiP floats (they were watch-only agent panes) and their feed runner |
| zz-client | `PaneTag::Agent`, the agent-kind branches of `pane_program`, status bar, completion and navigation, `agent_config`'s catalog and picker model |
| zz-ui | `agent_model_picker`, `agent_config_picker`, `agent_directory_button`, the history picker, `agent_pane_header`, `agent_pane_title`, `agent_status_pill`, the agent arm of `pane_kind_icon`, `ChooserPaneKind::Agent` |
| crates/zz | the Agent pane in `workspace/view.rs`, the picker's Agent choice, the New Agent Pane menu, the experimental setting, picker, history, title and session code in `agent/controller.rs` and `agent/view.rs`, the folder catalog in `file_picker.rs` (the file picker is editor-only now), `agent-send` composer commands |
| clients/app | the Agent pane in `agent_pane.rs` (pickers, history, project directories), New Agent in the phone overview, the agent panes switch in settings |
| storybook | the Agent pane story, rebuilt as a lens story over the kept components |
| knowledge | `concepts/agent-pane.md` is replaced by [the agent lens concept](/concepts/agent-lens.md); pages that describe the pane type are corrected |

## Kept

| Area | Why |
|---|---|
| native drivers, turn machine, journal, fanout, host | the driver chat runs on them; the lens reuses host, journal and fanout |
| `AgentSetConfigOption`, `AgentSetMode`, `AgentSessionOp`, `AgentAuthenticate` | runtime commands the drivers answer; no pane UI sends them now |
| `claude_peers.rs`, `codex_queue.rs`, `agent_waits.rs` | terminal `agent-send`, `@agent_state`, the peer bus |
| `claude/sessions.rs`, `claude/translate.rs`, `codex/translate.rs`, `codex/rollout.rs` (`v2_item`) | the lens readers |
| the `agent-pane` cargo feature of crates/zz | it gates the lens view and the ACP crate; the name stays to keep packaging and scripts unchanged |
| `agent-catalog`, `agent/catalog.rs`, `agent/environment.rs` | the Providers settings work owns them |

## Agent-facing CLI against terminal agents

| Verb | Terminal agents |
|---|---|
| `agent-send`, `--submit` | works through the Claude peer socket, `codex queue`, or paste; a terminal that runs neither CLI and has no `@agent_state` is refused (`send-text` types into any pane) |
| `agent-send --wait`, `--final`, `--on-block allow\|deny` | open the pane's lens runtime and wait on its turn, so Codex gets turn tracking too; `--final` and `--on-block` exit 2 when the pane runs neither CLI, plain `--wait` falls back to `@agent_state` |
| `agent-send --notify` | the caller must be a terminal pane; the reply is pasted into it |
| `agent-respond` | needs an open lens runtime (the app's lens or an earlier `--final`); Claude permission prompts only |
| `wait-for '@agent_state@%N'`, `inspect -a`, `agent-send --progress` | work |
| `wait-for agent_state@%N` (no `@`) | works while the pane has a lens runtime |
| `send-last-output` | drafts into the most recent Claude or Codex terminal of the window, without Enter |

# History

The first lens attempt (2026-09-17 to 2026-09-21) planned three readers and the same toggle while
the ACP agent pane was the alternative; it was built and deleted unmerged when the ACP pane stayed.
Since then the native drivers grew the Claude and Codex translators and the transcript tailing for
PiP subagent feeds, which are what this lens reads with. The survey from that attempt still holds:
screen scraping is ruled out, the Claude transcript's inner `message` is the public Messages API
shape while its wrapper fields change, and Codex's `item_completed` records need no unwrapping.

# Follow-ups

- A lens runtime stays open until its pane closes, even after every client closed the lens; it could
  close with its last viewer.
- Codex panes have no automatic `@agent_state`, so plain `agent-send` waits to Codex without a lens
  need a hook or OSC status; the lens runtime's busy signal could write it.
- Subagent activity inside the lens (the readers the PiP feeds used).
- Two Codex TUIs in one folder look alike to the rollout fallback, so the lens may follow the
  other one's thread; matching the prompt the lens pasted to the rollout that records it would
  settle that.
- `AgentSetConfigOption`, `AgentSetMode`, `AgentSessionOp`, `AgentAuthenticate` and the config, mode
  and auth fields of `AgentPaneWire` serve only the driver chat now; drop them if it does not use them.
