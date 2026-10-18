---
type: Design Plan
title: PiP panes . every float, thrown to one of nine places
description: "In the GUI clients every floating pane that is not modal draws as a PiP: the pane's own content in a glass card, a bar on hover with tile back, zoom and close, and a drag by that bar that you can throw, iOS style, so it glides and settles on the nearest of the nine places move-pane -P names. Underneath it stays a tmux 3.8 float with no new flag, state or wire."
status: Built (2026-10-18; desktop, web and iPad; Claude and Codex subagent feeds built)
resource: crates/zz-ui/src/pip.rs
tags:
- ui
- floating-panes
- design-plan
timestamp: 2026-10-18T00:00:00Z
---

# Rulings (fabrico, 2026-10-18)

1. PiP is not an agent feature. Any pane can float, and every non-modal float draws as a PiP in the
   GUI clients. No flag, no pane kind, no protocol field: a PiP is a [floating pane](/designs/floating-panes.md).
2. Modal floats (`display-popup`, the confirm prompt) keep `FloatingSurface` and its grips. The raw
   TUI keeps tmux's borders.
3. A drag snaps to the nine places `move-pane -P` names: `top-left`, `top-centre`, `top-right`,
   `centre-left`, `centre`, `centre-right`, `bottom-left`, `bottom-centre`, `bottom-right`. No
   stashing past an edge.
4. A window resize follows 3.8: floats are pulled in when the window shrinks and stay put when it
   grows. zz keeps no anchor.
5. A PiP can be thrown. Its release speed carries it, and it glides to the place it was headed for.

# Look

`PipSurface` (`crates/zz-ui/src/pip.rs`) wraps any pane's content element, terminal, browser or
agent, and paints four roles:

| Role | Under | What it paints |
|---|---|---|
| `pane.pip` | `pane.float` | the card: glass, outline, float shadow; an accent edge when the pane is active |
| `pane.pip.bar` | `pane.header` | the bar over the content's top edge, glass |
| `pane.pip.button` | `pane.header.button` | tile back, zoom and close, with a hover tint |
| `pane.pip.snap` | `pane.drop-zone` | the nine slots while a PiP is dragged, the lit one ringed |

At rest the card shows only the content, inside the float's border cells when it has them. The bar
shows while the pointer is over the card and always on a coarse pointer. It overlays the content, so
the PTY never resizes. Its buttons send `join-pane -s %N -t %N` (tile back), `resize-pane -Z -t %N`
and `kill-pane -t %N`.

A bordered PiP keeps the side, bottom and bottom corner grips for resizing (`resize-pane`, as
before). The top move grip and the two top corner grips are gone, because the bar covers that edge.

# Throw

`PipDrag` holds the drag in pixels: where it started, the pointer, and the frame origin, which
follows the pointer freely instead of stepping by cells. `PipThrow` keeps the last 100 ms of pointer
samples for the release speed.

On release the landing point is the frame's origin plus its speed run out at iOS's scroll
deceleration, 0.998 a millisecond, which is half a second of the speed (`projected`). The place whose
centre is nearest that point wins (`FloatPlace::nearest` in `zz_client::floating`), so a flick from
the middle reaches a corner and a slow release takes the nearest place. During the drag the slot that
pick would land on is lit.

The release sends one `move-pane -t %N -P <place>` at once, so the daemon and every other client get
the final place immediately. Only the throwing client draws the glide: an offset from that place that
`PipGlide` springs to zero on zz-gpui's analytic spring (stiffness 324, critically damped). Speed
toward the place is capped at the spring's frequency times the distance, which rules out overshoot.
Grabbing a PiP mid-glide starts the new drag from where it is drawn. With reduced motion it moves
straight there. `FloatPlace::cells` repeats `move-pane -P`'s arithmetic so the slots and the glide
end where the daemon puts the float.

# Clients

Every tiled pane's header (terminal, agent, browser) has a Float button (`picture-in-picture`) after
Split bottom and Split right. It sends `break-pane -W -s %N` (`float_pane_command` in
`zz_client::floating`), so the pane floats where tmux 3.8 puts a new float and draws as a PiP. Agent
and browser panes draw their own header inside the PiP too, and there the button is hidden
(`pane_floats`). Tile back is on the PiP bar. `prefix @`, the pane menu and the CLI still work.

- **Desktop** (`crates/zz/src/workspace/view.rs`): `FloatDragState.pip` carries the `PipDrag` for a
  move of a non-modal float; `throw_pip` sends the command and starts the glide, which
  `tick_pip_glide` advances each frame and drops once it settles and the command has settled.
- **Web and iOS** (`clients/app/src/app/floating.rs`): the same through `PipState`, with a mouse drag
  of the bar or a touch drag (`touch_drag_handle`). The glide ends when it settles and the snapshot
  shows the float at its place, or a second later.
- **Storybook**: Workspace → Panes → PiP shows the card at rest, focused, with its bar shown,
  borderless, an agent pane as a PiP, a subagent feed (`feed_pane` in the agent stories), and a
  throw with its slots. The terminal, agent and browser header stories carry Float. The theme
  creator's PiP section puts a focused terminal PiP and a feed PiP over tiled panes, with the throw
  slots on a plate, so the four `pane.pip` roles can be tuned and inspected.

# Subagent feeds

When Claude Code or Codex starts subagents, zz floats a PiP over its pane that follows them. This
works for a zz Agent pane and for the `claude` or `codex` TUI in a terminal pane. The PiP is a
watch-only agent pane fed from files the agent already writes. The agent's own session, tools and
prompt cache are not touched.

`crates/zz-daemon/src/agent/feed.rs` is the vendor-neutral part: a `FeedSource` per vendor reports
each child's heading, new updates and phase (running, done, stopped), and `Feed` turns that into the
agent stream. `crates/zz-daemon/src/daemon/subagent_feeds.rs` finds the parents and owns the floats:
a `zz-subagents` worker thread wakes every second while a Claude or Codex pane exists, and posts what
it found to the loop.

## Claude Code

- **Finding them:** a Claude Agent pane gives its session id from the runtime's wire state. A
  terminal pane running `claude` gives it through the peer record
  (`claude_peers::record_for_pane`). The worker finds `<projects>/<slug>/<session>/subagents/` and
  lists the subagents that are running.
- **Running or done** (`crates/zz-daemon/src/agent/claude/subagents.rs`, `Completions`): the parent
  transcript decides, read incrementally from its last 8 MiB.
  - A background subagent is done when its `<task-notification>` arrives with a status:
    `completed` is done, any other status is stopped.
  - A foreground one is done when its Agent call gets a tool_result.
  - `finished` or `stoppedByUser` in `agent-<id>.meta.json`, and an interrupt line, settle it at
    once.
  - When the parent transcript can't be read, an `end_turn` line is the fallback. A subagent that
    ends its turn while its own background shell runs is still running.
- **Lines:** each `agent-<id>.jsonl` line runs through the Claude translator
  (`Translator::sidechain_entry`), so steps, thoughts and answers render like any Claude turn. The
  heading is `<type> · <description>` from `agent-<id>.meta.json`.
- **Stop:** `HostCommand::StopTask` to the parent, which becomes Claude's `stop_task` with the agent
  id.

## Codex

- **Finding them** (`crates/zz-daemon/src/agent/codex/children.rs`): Codex writes every thread to
  `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*-<thread>.jsonl`. A child's first line is its
  `session_meta`: `parent_thread_id`, `session_id` (the root thread), `agent_path`,
  `agent_nickname`, `agent_role` and `subagent_history_start_ordinal`. `running_children` reads the
  newest two day folders and groups the running children by root thread.
- **Parents:** a Codex Agent pane gives its thread id from the runtime's wire state. The `codex` TUI
  can't be asked for its thread, so a terminal pane whose process tree runs a `codex` binary
  (`pane_runs_codex`) takes the root whose `cwd` matches its own, when it is the only codex TUI in
  that folder and no Agent pane owns that root.
- **Running or done:** the child's own `task_started`, `task_complete` and `turn_aborted` lines after
  its start ordinal.
- **Lines:** the rollout's `item_completed` items (commands, messages, reasoning) are reshaped into
  app-server items and run through the Codex translator (`Translator::history_item`). The heading is
  `<role or nickname> · <agent path>`, for example `Pauli · daemon reader a`.
- **Stop:** `HostCommand::InterruptThread` to the parent, which sends Codex's `turn/interrupt` with
  the child's thread and running turn on the pane's app-server connection.

## Both

- **The float:** `new-pane -d --kind agent -T Subagents` at the parent's top-right corner, 64x18
  outer cells, with pane option `@zz-subagents=%<parent>`.
  - `agent_pane_spec` reads that option and hands the pane a `FeedSpec`. `AgentHost::open` then runs
    `run_subagent_feed` instead of an agent process.
  - The float closes 5 s after the last subagent finishes, and when its parent goes.
  - A float you close stays closed until a new subagent starts.
  - Feed floats the daemon no longer tracks, for example after a restart, are closed.
- **The feed** (`Feed`): one turn per subagent.
  - Each turn is headed by a user bubble with the child's heading, then its lines.
  - Turns stay whole: finished subagents come first, in the order they finished, and the subagent
    being followed is last, so its updates stream into the live turn.
  - When a subagent finishes while another is followed, the feed replays its timeline as
    `SessionSwitched` so the finished turn lands above the live one. It then re-sends
    `Activity { busy }` so the live turn stays live.
  - The subagents still running are the pane's task tray (`TasksChanged`), and `Activity` drives the
    Running status. A terminal parent has no channel to stop through, so its feed sends no tray.
- **Watch-only:** the feed's Ready carries `AgentSessionCapabilities { watch: true }`. It is JSON on
  the agent stream, so no protocol bump. Desktop and web/iOS draw the task tray and errors in place
  of the composer. A prompt that reaches the feed anyway is refused.
- **Feeds stay off the peer bus.** They can't take a prompt.

Verified live on 2026-10-18 against a throwaway daemon and the web client:
- Claude Code 2.1.302 on Haiku: a `claude` TUI in a terminal pane and a Claude Agent pane each
  started subagents, and the float opened at the parent's corner. It showed each subagent's heading,
  a live trace with its latest thought, its answer, and the running count. It kept a finished turn
  above the live one, and closed 5 s after the last subagent finished. Stop on a subagent stopped it.
- Codex on GPT-6-Luna: a Codex Agent pane and a `codex` TUI in a terminal pane each spawned two
  children, and each got its own float with the children's commands. Stop on one child of the Agent
  pane wrote `turn_aborted` to its rollout and dropped it from the tray while the other kept going.

# Not done

- Two `codex` TUIs in the same folder get no feed, since nothing outside Codex says which root is
  whose.
- An agent or browser PiP shows two rows on hover: the PiP bar over the pane's own header.
- iOS native browser views are hidden under floats by their snapshot place, not the place drawn
  during a drag or glide.
- The web client draws floats in window cells. When the window is larger than the canvas (a
  manually sized window), a float lands off the tiled panes it belongs to.
