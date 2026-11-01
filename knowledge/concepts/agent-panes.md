---
type: Concept
title: Agent panes and the flip
description: How a pane runs Claude Code or Codex as a chat (PaneKind::Agent over the vendor drivers), how flip-agent switches one pane between the CLI's own TUI and that chat on the same session (busy rules, --now, --when-idle and the armed flag, finding the TUI's session, waiting for the old process), forks and side chats, Codex on its shared app server, the chat's slash list, agent-send and agent-respond, and what the chat draws and how its stream reaches clients.
resource: crates/zz-daemon/src/daemon/agent_flip.rs
tags: [agent, agent-pane, flip, chat, claude-code, codex, driver, stream]
timestamp: 2026-11-01T23:00:00-03:00
---

# Overview

A pane runs Claude Code or Codex one of two ways, and one button switches between them:

- **The CLI.** An ordinary terminal pane running the vendor's own TUI. zz draws the terminal and
  types nothing into it.
- **The chat.** An agent pane (`PaneKind::Agent`) whose daemon runs the CLI's headless protocol
  through the native drivers (Claude Code stream-json, the Codex app server) and streams the
  conversation to clients, which draw it as a chat.

`flip-agent` turns one into the other on the same session: it stops the running process and
starts the other side resuming that session, in the same pane id and layout place. The CLIs' own
screen-only commands (`/config`, `/status`, `/mcp` and so on) are not offered in the chat; to use
one, flip to the CLI.

The 2026-10-23 [agent lens](/designs/agent-lens.md), a chat drawn over a terminal running the TUI,
was replaced by this on 2026-11-01 and its runtime deleted. The drivers, the
[turn machine](/designs/agent-turn-machine.md), the chat machine and the chat UI it used carried
over.

# Agent panes

`PaneKindSnapshot::Agent(AgentDescriptor)` (wire 118) is the pane kind:

| Field | Meaning |
|---|---|
| `provider` | `ClaudeCode` or `Codex` |
| `cwd` | the folder the driver runs in; filled from `-c`, else the terminal or donor pane's folder, else the daemon's |
| `session_id` | the vendor session to resume; the driver's session goes back into it, so a `/clear` or a fork shows here |
| `model`, `effort` | what the driver is set to once its session is ready (`SetConfigOption` after `SessionReady`) |

**Opening one.** `select-pane-kind [-t pane] [-c dir] [--resume id] [--model m] [--effort e]
agent <claude|codex>` (`--resume id` may also follow the provider) turns a picker, or a terminal,
into an agent pane. A terminal keeps its pane id and place: its process is stopped and the driver
starts once it has exited (`AGENT_CLI_EXIT_GRACE`, 5 s). `select-pane-kind [-c dir] terminal
[command ...]` turns an agent pane back into a terminal (the mux emits `PaneRespawned`), closing
its driver. Both conversions are `MuxState::convert_pane`; a pinned title survives them.

**Running it.** The daemon opens the pane's runtime (`Shared::open_agent_pane`, `agent_pane_spec`)
with the descriptor's provider, folder, session, model and effort and the pane's workspace
environment (`ZZ_PANE`, `TMUX`, `TMUX_PANE`). A resumed Claude session replays its transcript
before the driver says it is ready; a resumed Codex thread comes back through `thread/resume`.
Closing the pane closes the driver. Agent messages (`AgentReplay`, `AgentPrompt`, approvals,
settings) are accepted only for agent panes, and `AgentState` and the stream follow
[How the stream reaches a client](#how-the-stream-reaches-a-client).

**Forks and side chats.** `--fork` (with `--resume`) opens the pane on a copy of the session, and
`--at <message>` copies it only through that message's turn (`crate::agent_fork`, which
`zz tools --fork-chat` uses too): a Claude Code fork copies the transcript under a new session id,
a Codex fork asks the app server for `thread/fork` with `lastTurnId`. The copy's id becomes the
pane's session. Clients open a fork or side chat in a new pane with one command line, since a new
window's or float's pane becomes the target of the next command:

- Fork from a message: `new-window -c <dir> ; select-pane-kind --resume <session> --fork --at
  <message> agent <claude|codex>`
- Side chat in a PiP: `new-pane -c <dir> ; select-pane-kind --resume <session> --fork agent
  <claude|codex>`

# The flip

`flip-agent [-t pane] [--now | --when-idle | --cancel]` (`crates/zz-daemon/src/daemon/agent_flip.rs`).

- **CLI to chat.** On a terminal whose process list shows `claude` or `codex` (`cli_provider`), the
  daemon finds the session the TUI is on, its folder and its model and effort
  ([below](#finding-the-tuis-session)), and runs `select-pane-kind -t <pane> -c <dir> --resume
  <id> --model <m> --effort <e> agent <cli>`. The TUI is stopped and the driver starts once both
  the terminal and the CLI process have exited, so two processes never hold the session. A TUI
  that has not sent its first prompt has no session yet, and the chat starts a new one.
- **Chat to CLI.** On an agent pane, the daemon closes the driver and waits for it and its process
  to exit (`AgentRuntime::close_and_wait`), then runs `select-pane-kind -t <pane> -c <dir>
  terminal /bin/sh -c 'exec "${SHELL:-/bin/sh}" -ic ...'` with `claude --resume <id> --model <m>
  --effort <e>` or `codex resume <id>` (`cli_session::cli_command`, `interactive_shell`), through
  the user's interactive shell so their aliases apply. Model and effort come from the driver's
  current config options, else the descriptor. Codex takes its model and effort from the thread,
  and `-c` would cut the TUI off the shared app server, so `codex resume` gets no flags.
- **Running work.** Something running refuses the flip with an error that starts with `busy:` and
  says what runs, for example `busy: a turn and 2 background tasks`:
  - a chat: a running turn or permission (`phase`), background tasks and subagents (`tasks`), and
    queued prompts (`cli_session::chat_running`);
  - a Claude TUI: its session record's `status` `busy`, `shell` (a background shell) or `waiting`.
  - Codex on the shared app server never refuses, since the turn and its background terminals
    live in the server and the other side rejoins them. A Codex TUI running embedded (started with
    `-c` overrides, `--profile`, `--no-daemon` and the like, or holding its rollout open) refuses
    like Claude, reading its rollout's last `task_started` and `task_complete`.
- **`--now`** flips anyway: the turn and every background shell and subagent of a Claude session
  stop with the process.
- **`--when-idle`** arms the flip. The pane's `PaneSummary.flip_armed` turns on (wire 118, in the
  snapshot and `TreeOp::PaneSummary`), and a watcher thread checks every 250 ms and flips once
  nothing runs, then clears it. **`--cancel`** clears it. An armed pane that closes or stops running
  the CLI drops the flip.
- The pane keeps its id and place. `inspect` lists `flip-agent` among the verbs of an agent pane
  and of a terminal running either CLI.

# Finding the TUI's session

`crates/zz-daemon/src/agent/cli_session.rs` reads the CLIs' own files, never their screens.

- **Claude Code.** The record `sessions/<pid>.json` under `$CLAUDE_CONFIG_DIR` or `~/.claude` whose
  process is the pane's or below it (`claude_peers::record_in_pane_with`) names the session, the
  folder and the status. The CLI writes it at start and removes it at exit. Model and effort are
  the transcript's last assistant line (`message.model`, `effort`) and its "Set model to" and "Set
  effort level to" lines, over the CLI's arguments and settings files (`Facts::claude_launch`);
  a model name the CLI printed becomes its alias for `--model`.
- **Codex.** The thread is the one `codex resume <id>` names, else the parentless rollout the TUI
  holds open (embedded), else the one parentless rollout in the TUI's folder modified since it
  started; two such rollouts refuse the flip and ask for `codex resume <id>`. Model and effort
  come from the rollout's `turn_context` and `thread_settings_applied` lines over `config.toml`
  and the arguments.

# Codex on the shared app server

A Codex TUI runs its threads in the app server daemon it starts (`codex app-server daemon start`),
reached over a WebSocket on `$CODEX_HOME/app-server-control/app-server-control.sock`. When
`agent-command` is plain `codex`, the chat driver connects there too (`Process::connect`,
`codex::managed_daemon`) instead of starting an app server of its own, starting the daemon first
when its socket is missing (without the pane's `ZZ_PANE`, `ZZ_SESSION`, `TMUX` or `TMUX_PANE`).
A thread then outlives whichever side drove it: a flip kills the TUI and `thread/resume`s, or
closes the chat's connection and runs `codex resume`, mid-turn included, and the server unloads an
idle thread with no client 60 s later. A configured command with arguments keeps an app server of
its own per chat.

The server's environment is its starter's, so the chat hands each thread its pane's environment:
`thread/start`, `thread/resume` and `thread/fork` carry a `config` override that sets
`shell_environment_policy.set.<NAME>` for `ZZ_PANE`, `ZZ_SESSION`, `ZZ_SOCKET`, `TMUX`,
`TMUX_PANE`, `ZZ_TMUX_EXECUTABLE`, `ZZ_TMUX_SHIM_DIR`, `ZZ_AGENT_CONTEXT` and `PATH`
(`thread_config` in `agent/codex/mod.rs`). Commands and background terminals in a chat's turns
see the chat's pane, two chats in different panes on one server each see their own, and a flip to
the chat gets it too: the TUI is gone by then, and the server rebuilds a loaded thread nobody is
subscribed to when a resume brings different overrides. Where it falls short:

- The server ignores the overrides when another client is still subscribed to the thread, so a
  second chat resuming a thread a live chat holds runs its commands with the first chat's pane.
- The TUI's own turns run with the server's environment: no pane when zz started the server, or
  the pane of the TUI that started it. `zz` called from Codex in the TUI may name no pane or the
  wrong one.

# The chat's slash list

- **Claude Code.** The driver offers the commands the CLI's `init` frame lists (`slash_commands`),
  less its `terminal_slash_commands` and the ones it refuses over stream-json (`CHAT_REFUSED`:
  `/status`, `/help`, `/memory`, `/permissions`, `/export`, `/rewind`, `/resume`, `/tasks`, `/btw`,
  `/focus`, `/fast`, `/doctor`). Until the first turn sends `init`, it offers the `initialize`
  answer's list less those. Typing one of the refused commands fails the turn with "only works in
  Claude Code's own screen; flip the pane to the CLI", and `/doctor`, which goes through the
  user's real configuration, is never sent.
- **Codex.** `/compact` and `/review`.
- **zz's own verbs** (`/btw`, `/side`, `/steer`, `/fork`, `/rewind`) and the `/model` and `/effort`
  setters come from the client (`zz_client::agent_completion`).

The driver follows a `/clear`'s new session id (`follow_session`), and the empty `result` frames
the CLI writes while it resumes (`num_turns: 0`, before its first idle `session_state_changed`)
end no turn.

# From outside: the CLI

- `agent-send [-t %N] TEXT` to an agent pane submits a prompt. `--wait`, `--json`, `--final`,
  `--on-block`, `--notify` and `--progress` work on its turn as they did for the old agent pane.
- `agent-send` with text to a terminal running Claude Code or Codex fails with "runs Claude Code in
  its terminal; flip the pane to chat first". Nothing types prompts into a TUI or posts them to its
  peer socket. `--wait ''` still waits on such a terminal's `@agent_state`.
- `agent-respond` answers a pending permission in an agent pane.
- `flip-agent` and `select-pane-kind ... agent` as above.

# What the clients draw

- **The flip button.** A terminal running either CLI and an agent pane carry a flip button in their
  header (`pane.header.flip`, `zz_ui::pane::pane_flip_button`) that sends `flip-agent -t <pane>`.
  A `busy:` answer offers Switch when idle and Switch now; an armed flip shows with Cancel.
  `zz_client::agent_flip` (`AgentFlips`) keeps that state per pane for every client.
- **The chat.** `crates/zz/src/agent` on the desktop, `clients/app/src/app/agent_pane.rs` on web,
  iPad and iPhone, and `clients/dom/src/agent.rs` in the HTML client draw an agent pane with
  `zz_ui::agent::chat`: the composer, the model and effort menu, inline questions and approvals,
  subagent PiPs, images, references, Copy and Fork, and diagrams. The storybook's Agent story
  draws them from fixed chat events.

# References in the chat

zz references in a message show as named links (`zz_ui::agent::refs`, resolved by
`zz_ui::target::resolve_target` against the same host tree the `@` menu lists):

- An `@` token as the `@` menu writes it: `@host:session:window.%pane`, `@session:window`,
  `@host:%pane`, `@host`, `@~/path/`, `@host:/path/file.rs:12`.
- In inline code, the same token, a pane or session id (`%3`, `$0`, `$0:1.%2`), or a file path with
  an extension (`crates/zz/src/main.rs:40`), resolved against the chat's folder.

A pane shows as its program and window ("bash · zebrilus 0:1"), a chat as its title, a window as
"session › name", a file as its name and line, a folder with a trailing slash, and anything on
another host adds " · host". Tokens that resolve to nothing stay as written, and fenced code
blocks are left alone. Clicking a pane, window, session, chat or host switches to it like the
palette does; a file opens in a float running `$VISUAL` or `$EDITOR` at that line, started through
the user's interactive shell like the CLIs below, and a folder opens a shell float there
(`refs::open_path`). The assistant's
markdown carries them as `zzref:` links; the user's bubble draws them as clickable ranges.

# How the chat reads

`zz_ui::agent::chat` draws the folded chat (`chat_thread`):

- **Your messages.** Claude Code wraps a paste as `<pasted_content id="…">…</pasted_content
  id="…">` in its transcript; the runtime drops the wrapper (`claude::translate::prompt_text`). A
  long message stays inside a bubble capped at 260 px that scrolls.
- **Pasted images.** The translators put a prompt's images on its `user_message_chunk` as
  `_meta.zz.images` (`{ format, data }`, base64): Claude's base64 image blocks, minus the
  `[Image #N]` placeholders, and Codex's data URLs and local image files. The bridge
  (`chat_bridge::attach_images`) keeps a message's text and base64 together under 6 MiB so one
  stream item stays under the 9 MiB batch cap, and the chat item carries them as `ItemBody::images`;
  a user item is held to that cap rather than the 1 MiB one other chat items get
  (`runtime::validate_payload`). A prompt sent through the drivers gets the same treatment:
  `runtime::prompt_echo` puts its images on its first text chunk (an empty one for a prompt that is
  only images), and the image chunks after it stay for the timeline. Both drivers drop
  `_meta.zz.images` from the raw update once the bridge has read it,
  so the stream carries each image once; the driver's journal keeps it, so a restored session's
  chat shows it again. The fanout's chat log and every client's `ChatView` keep images on the
  last 8 items that have them (an item that loses its images gives up its place), so a live chat
  and a late client's replay both show older pastes as text only. The bubble draws them first as
  140 px tiles that open full size on click; `ChatPane::sync` decodes each item's images once and
  keeps them in `ChatOpen::images`. When an item leaves the view, loses its images, or the session
  changes, `ChatPane::release` drops the decoded pixels from ala's asset cache and the sprite
  atlas, unless a live chat pane still shows the same image (an app global lists each pane's
  shown images). The desktop agent view and the thin client's agent pane hook
  `ChatPane::close_on_release`, so closing the pane frees every image it held, through its window
  or, when the window closed with it, from the asset cache alone.
- **Live turns.** The last line of a running turn says what the agent is doing now, with a shimmer,
  a turning sparkle and the time since the prompt (`chat_rows::live_line`). It takes the newest of:
  the CLI's own activity line (`ChatEvent::TurnActivity`, from Claude Code's `task_summary`), a
  running tool's own description (`ItemBody::activity`: Claude's Bash, PowerShell and Agent
  `description`, Codex's `wait` as Waiting for agents), else the running tool's verb and target
  ("Running cargo test"), and the latest reasoning heading. A message the agent writes after them
  clears them, and the line says Thinking only when nothing more specific is known. While the
  newest run of tools is still going, the line is that run's header and opens to its steps. It
  hides while the chat waits on you. The transcript only gets a reply once it is whole, so a reply
  that lands while you watch types itself in over 220 to 900 ms (`ChatPane::sync` notes when it
  arrived; replies already there on the first look show at once).
- **Batches.** Claude Code sends a sentence for each finished batch of tool calls
  (`tool_use_summary`, with the ids it covers), and the run's header shows it instead of zz's count
  ("Counted lines in a.txt" for "Ran 2 commands"), with failures and denials still counted after
  it. The chat matches it by tool id (`ChatEvent::ItemsSummarized`), so a sentence that lands after
  the reply, after the turn, or in a replay finds its rows, and a run that two sentences cover
  splits in two. Codex, subagents and a session resumed from the CLI's transcript (which keeps no
  sentences) show zz's count. Claude Code's `post_turn_summary`, a line about the whole turn sent
  after it, is not drawn: the reply sits right under the fold and says the same thing.
- **Thinking.** A finished run whose reasoning is only headings (Codex records the headings of its
  summaries; the reasoning itself is encrypted) leaves no row, while the running one still shows
  its heading. Opening a run lists its reasoning, heading removed and muted, in order with its
  steps; a run with nothing to list has no disclosure. A heading is a line that is all bold or a
  `#` heading (one to six `#` and a space), so a thought that opens with inline bold or `#42`
  keeps its text (`chat_rows::reasoning_body`). The opened text is selectable and scrolls inside
  260 px. Claude's thinking reads the same way.
- **Subagents.** One row each: the CLI's mark, the name, a status glyph (spinning, Needs you, done,
  failed, stopped) and the time it took. Clicking one opens its chat in a PiP over the thread
  (`chat_agent_pip`): the prompt it was given, its own rows, its answer and a footer; its close
  button or a second click closes it. An approval it waits on sits under its row.
- **Answered questions.** The question, with the answer under it.
- **Diagrams.** A mermaid or SVG block sits in a card on the `code` role, like a code block, whose
  header holds its label, Copy source and Open full diagram. pele (`libs/pele`) draws mermaid:
  flowchart, sequence, class, state, ER, gantt, pie and quadrant. Any other type stays an ordinary
  code block of its source, and a supported source that does not parse shows the error with its
  line and column. The diagram's canvas is transparent, so it shows the card through it; nodes fill
  with the card blended with the soft neutral paint, text is the app font at 13 px measured by the
  app's text system (ala's SVG renderer draws `system-ui` in the face that text system uses), and
  series colors come from primary, success, warning, danger and muted. A flowchart, class, state or
  ER diagram that runs across and is wider than 640 px turns to run down when that fits. A figure
  draws at its own size, scaled down only to fit the transcript's width, so a diagram that turned or
  a long sequence keeps its 13 px labels however tall it gets. Answers use the same markdown style
  as code blocks and tables elsewhere (`agent_text_view`). The chat passes its pane as the owner
  (`ChatHooks::owner`), so on the desktop an HTML block's live page can reply to the agent, and two
  panes showing the same item keep their own preview toggles.

# How the stream reaches a client

The full delivery contract is in the
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
  older than the ring takes the reset path the runtime design describes. A driver that resumes a
  session replays its history from the CLI's own file first.
- **Visibility.** The heavy stream goes to clients for whom the pane is an agent pane in their
  focused window, honoring zoom (`visible_agent_panes`), and to clients watching the pane.
  `AgentState` goes to every client attached to the session and to the pane's watchers, and a
  reattaching client gets it for every agent pane there (`send_agent_resync`, `session_agent_panes`).
- **Watchers.** A client that sends `AgentReplay` for a pane watches it from then on, attached or
  not (`watched_agents` on the client entry): it gets the pane's `AgentUpdates`, `AgentState` and
  `AgentLagged` until it disconnects or the pane closes, and the pane's `AgentState` right away when
  the watch starts. A client can follow an agent this way without attaching to its session, which
  would resize its panes, and an attached client keeps a chat live while its focused window shows
  something else. A client that both sees and watches a pane is
  sent each frame once, and a watched pane leaving the visible set keeps its queued frames. There is
  no unwatch message.

# Related

- [Agent lens design](/designs/agent-lens.md) . the design this replaced
- [Daemon-owned agent runtime](/designs/agent-daemon-runtime.md) . host, fanout, journal
- [Native agent drivers](/designs/native-agent-drivers.md) . the drivers agent panes run
- [Chat mode](/designs/chat-mode.md) . what a new pane offers in each sidebar view
- [Agent peer bus](/architecture/agent-peer-bus.md)
- [Wire protocol](/protocol/wire-protocol.md)
- [tmux command set](/tmux/commands.md)
- [zz](/crates/zz.md) and [zz-daemon](/crates/zz-daemon.md)
