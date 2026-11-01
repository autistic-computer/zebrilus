---
type: Concept
title: Agent lens
description: How a terminal pane running Claude Code or Codex shows an agent timeline. Covers the header toggle and chat mode, the slash commands and skills it lists, the terminal PiP for the CLI's own dialogs, references to panes, chats and files as links, copy and fork on messages, how the chat draws pastes, live turns, subagents and diagrams, the model, effort and folder controls, side chats and forks, pasted images, the daemon lens runtime that tails the CLI's own transcript, joins the Codex TUI's app server, and talks to Claude Code through the zz plugin, the agent stream that carries it to clients, Claude permission prompts, and what agent-send and agent-respond do with it.
resource: crates/zz-daemon/src/agent/lens.rs
tags: [agent, lens, claude-code, codex, transcript, app-server, plugin, terminal, pane]
timestamp: 2026-10-31T23:30:00-03:00
---

# Overview

zz has no agent pane kind. Claude Code and Codex run as their own TUIs in ordinary terminal panes,
and a terminal whose foreground program is `claude` or `codex` gets a lens button in its header.
With the lens on, the pane body shows that agent's timeline and a composer in place of the cell
grid; clicking again shows the terminal. The terminal process stays the source of truth: the
timeline is read from the transcript the CLI already writes, and the composer types into the TUI.
When the CLI offers a live channel the lens uses it instead of typing: a Codex TUI on its app
server is joined as a second client (see [Codex over the app server](#codex-over-the-app-server)),
and for Claude Code a plugin zz loads into the CLI streams the running turn and takes prompts,
approvals, questions and model picks (see [The Claude Code plugin](#the-claude-code-plugin)).

The rulings, the full deletion map and the history of the earlier attempt are in the
[agent lens design](/designs/agent-lens.md). Both halves are on main (merged in `69ceca016`).

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
  `pane.header.lens` role, which tints it while the lens is open. Like the split buttons beside it,
  it shows on the focused pane and on the others only while the pointer is over the header.
- **Phone.** In the compact shell (iPhone) the lens opens by default for a terminal running an
  agent, since typing into a TUI with a phone keyboard is slow. Closing it with the toggle keeps that
  pane on its terminal until the agent exits (`CompactState::lens_dismissed` in
  `clients/app/src/app/compact.rs`). With the desktop sidebar in chat mode every terminal running an
  agent in the attached session opens with its lens on (`AppView::open_chat_lenses` in
  `crates/zz/src/workspace/view.rs`); turning it off with the toggle keeps that pane on its
  terminal until the agent exits (`lens_dismissed`). In raw mode, and on web and iPad, it opens
  only from the toggle.
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
drivers, the ACP runtime and the [turn machine](/designs/agent-turn-machine.md) stay in the source,
but no pane opens them now.

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
  `codex/rollout.rs` (`RolloutMeta`, `v2_item`) reshapes `item_completed` items into the app
  server's shapes and `codex::translate::Translator::history_item` translates them:
  - A command recorded as `[bash|zsh|sh|dash, -c|-lc, script]` shows only its script, and a
    command of several parts is titled by the whole command, not its first part.
  - A file change's map of paths becomes the list of changes with the paths untouched (adds and
    deletes as one hunk of `+` or `-` lines, updates as their diff). Only lines before a diff's
    first `@@` count as `---`/`+++` file headers.
  - Items recorded without a status (context compaction, image views, extensions) are done; an
    image path loses its `file://`.
  - Extensions: `web.search` becomes a web row (`Open <url>`, `Find "<pattern>"` or
    `Search "<query>"`, its results as output) and `clock.sleep` a "Waited 1m" step; other kinds
    are dropped.
  - Agent tools: only a spawned agent and `SubAgentActivity` are subagent rows. `wait` is a
    "Waited for agents" step, interrupting, resuming or messaging an agent are plain steps, and
    `list_agents` and `close_agent` are not shown.
  - Subagents: a started `SubAgentActivity` names the child's thread. The runtime finds the child
    rollout `rollout-*-<thread>.jsonl` next to the root, or in the last 7 day folders when the
    root sits in `$CODEX_HOME/sessions` (it keeps looking for 30 s), and feeds it through
    `Translator::child`: the child's tools nest under the agent row and
    its last message becomes the row's result. Its children's children work the same way. With
    no child file, the row waits for the root's `FINAL_ANSWER` agent message and takes the text
    after `Payload:` as its result (or ends with the turn). Child files are read where order
    matters, before an agent's end, a `FINAL_ANSWER` message, a turn's end and the end of a load,
    and there each child is read to its end, so a child longer than one 4 MiB read still finishes
    before its agent row closes; after each batch of root lines a child gets one 4 MiB read. Rows
    are ordered by their own timestamps.
  - Times: each line's `timestamp` stamps its rows, and `completed_at_ms - started_at_ms` (or a
    command's `duration`) goes in `_meta.zz.ms`, so a step starts that long before its record.
  - `token_count` lines fill the context ring (`usage_update`).

  `task_started` marks the pane busy; `task_complete` and `turn_aborted` mark it idle. A
  `task_complete` that carries an `error` (an API error, such as a model the login does not allow)
  fails the lens turn with Codex's message, so `agent-send --wait` exits 1. While a lens turn
  waits on a pane whose rollout is not found yet, the runtime looks on every tick instead of every
  5 s, and a rollout found after its turn already ended settles that turn from its last
  `task_complete`.

Claude's session record carries `status` (`busy`, `idle`, `waiting`). The runtime turns it into
`Activity { busy }` events, so the pane's phase follows the TUI whether the turn came from the
lens, from typing in the terminal, or from a peer message. While the Claude Code plugin is
connected, the plugin's turn edges decide instead (below).

# The Claude Code plugin

zz ships a Claude Code plugin whose hooks module (a "mod", `crates/zz-daemon/assets/claude-plugin`)
gives the Claude lens a live channel. The transcript stays for history on open, resume and rewind,
and is the whole story when the plugin is not there.

- **Loading.** The daemon writes the plugin into `<daemon data dir>/claude-plugin-<hash of its
  files>` (`agent/claude_plugin.rs`, `include_str!`; not in tests), and every pane it spawns gets
  that folder appended to `CLAUDE_CODE_PLUGIN_DIRS` (the user's own folders stay first) and
  `ZZ_LENS_SOCKET`, the daemon's lens socket beside its own (`<socket stem>-lens.sock`, left out
  past 100 bytes). The CLI loads the folder with no flag and no prompt, watches it and reloads the
  mod when a file in it changes, and writes about 880 KB of type files into it; one folder per
  version means a daemon of another build never swaps the mod under sessions it did not start.
  Each pane spawn touches the daemon's folder, and when a daemon first writes the plugin it removes the other
  `claude-plugin-<16 hex>` folders whose newest file is over 30 days old, so a folder an older
  daemon's sessions still use stays. The mod does nothing without `ZZ_LENS_SOCKET` and `ZZ_PANE`.
- **Channel.** One socket per daemon (`agent/lens_channel.rs`, `Hub`), bound 0600 on the first
  pane spawn that adds the plugin, with one accept thread; `shutdown_agents` closes it and removes
  the file, and a closed pane drops its state (`Hub::forget`). It speaks HTTP/1.1 over the Unix
  socket, which the mod reaches with `$.http.fetch` and `socketPath`. HTTP because a mod hook has a
  10 s budget of its own time that `$.clock.sleep` spends, while a `$` call in flight is free, so a
  permission hook can wait for the lens as long as the dialog is up only on a fetch. Every request
  names its pane (`?pane=<n>`, from `ZZ_PANE`). The mod posts its events in batches to `/events`,
  long-polls `/commands?instance=<id>` (15 s, answering `{commands, watched}`) and posts each
  permission to `/ask`, which answers `decision`, `pass` or `pending` (ask again). The mod says
  hello as soon as the session starts and retries once a second while the daemon is away, so the
  link is up whether or not a lens is open. A pane's state exists only once its plugin has spoken,
  so panes that never run Claude cost nothing.
- **Before a lens opens.** The hub keeps only each instance's latest `hello` (eight per pane),
  with its turn and model kept current from `turn`, `step`, `done`, `end` and `model`; other events
  are dropped, and `/ask` answers `pass` at once, so the TUI dialog works as if zz were not there.
  `watched` in each reply tells the mod whether a lens is attached; while it is false the mod sends
  no `text`, `thinking` or `used` events and posts no asks. A lens opening on the pane attaches
  (`Hub::attach`, through `LensPane::channel`), wakes the waiting long-poll with `watched: true`,
  and takes the held hellos; the Claude source greets them as soon as it finds the pane's record,
  inside `open`, so the first prompt after the lens opens already goes through the plugin. A lens
  that closes detaches: waiting asks pass and the mod stops streaming.
- **Events** (`agent/lens_claude_mod.rs`, `Link`). `hello` (instance, session, model, running
  turn), `turn` and `done` (main-loop `turn.start` and `turn.complete`, Esc included), `step`,
  `text` and `thinking` chunks with their block index and UTF-16 offset, `used` (PostToolUse,
  PostToolUseFailure, PermissionDenied), `end`, `model` and `ack`. A `hello` from a session other
  than the pane's record is held, not adopted: its events are dropped and its asks pass at once,
  and the lens adopts it once the record names its session while the lens has no live plugin. The
  CLI registers a fresh session id before the one `claude -c` resumes, so a `hello` can arrive
  ahead of its record. A `claude -p` never connects at all (the mod skips sessions that are not
  interactive).
- **Turns.** While the plugin polls, busy and idle come from its turn edges, so Esc in the TUI ends
  the lens turn on the next 250 ms tick. A turn that starts and ends inside one tick still reports
  busy for it, and a main-loop step counts as a start (a background agent's follow-up turn). The
  record's `waiting` is held back for 2 s in case the plugin relays it, then shows the terminal card
  as before.
- **Live text.** Text and thinking chunks become `agent_message_chunk` and `agent_thought_chunk`
  under `live:<turn>:<step>:<index>`, stamped with the mod's time and merged with transcript lines
  by time. When the transcript writes the block, it matches the oldest live block of the same kind
  and `apiBlockIndex` whose text it starts with (or that starts with it, after Esc) and adds only
  what the lens has not shown. A block joined partway or with a gap stops streaming and the
  transcript finishes it; a reload or session switch forgets what was shown, so the replay shows it
  whole. Subagent text stays on the transcript path (`_meta.zz.parent`).
- **Approvals and questions.** `tool.check` notes each call that will ask, so `classic.PermissionRequest`
  (which has no `tool_use_id`) can name its tool. The hook first runs what is beneath it, then posts
  to `/ask` and waits while the TUI dialog is up; the first answer wins. The lens draws the card
  the native driver drew (`claude::approval_request`, `plan_request`, `question_request`) and answers
  with the same decisions (`allow` with `updatedInput`, Always allow with the suggestions,
  `allow_with_mode` for a plan, `answered` for questions, several and multi-select joined with
  ", "). An answer given in the TUI withdraws the card once the record, which read `waiting` while
  the card was up, reads anything else (about 50 ms after the answer, while the tool still runs),
  on `used`, the next step of that loop, the end of that loop's turn (a background agent's card
  outlives the main turn), or when the plugin stops polling, and the waiting hook gets `pass`. A
  background agent's ask shows no dialog on the main screen and leaves the record `busy`, so its
  card stays until it is answered.
- **Actions.** A prompt goes through `$.prompt.submit({ text, asUser: true })`; the TUI shows
  "Prompt from the zz plugin" and the transcript records it with a plugin origin. Prompts with
  images, slash commands and steers are still typed. After Esc the CLI refuses plugin prompts until
  someone types ("the session was stopped and nobody has sent input since"), so a refused prompt is
  typed instead. Stop answers waiting cards with deny and interrupt and calls `$.turn.abort`, which
  leaves no "Interrupted" marker. A model pick calls `$.config.set({ key: "model" })`, which also
  saves it as the user's default, as `/model` does; effort has no API and is still typed.
- **A plugin that leaves.** A long-poll checks its connection every 250 ms, so a CLI that exits
  stops counting as polling within a quarter second, not when the poll times out; commands still
  queued for it, or that could not be written to it, come back to the lens as lost. A prompt the
  plugin never acknowledged and can no longer take is typed while the record's process still runs
  (the mod reloaded), and otherwise ends its turn with "Claude Code exited before it took the
  prompt" instead of an empty answer.
- **Fallback.** No socket, an older CLI, a policy that turns mods off, or a plugin that has hung up
  or not polled in 3 s: the lens types, reads the record and answers with keys exactly as below.

# Codex over the app server

A Codex TUI runs its threads on the managed app server it starts itself unless it is in embedded
mode. `codex_live::embedded` reads that from its arguments the way Codex 0.166 does: `-c`/`--config`
overrides other than `features.<name>=true|false`, `tui.fullscreen_transcript` and
`suppress_unstable_features_warning`, `--search`, `--profile`/`-p`, `--oss`, `--no-daemon`,
`--strict-config`, `--dangerously-bypass-hook-trust` and `--remote` (its own server) all count;
`--enable` and `--disable` become feature overrides and do not. Embedded mode the arguments do not
show (an executor or identity from the environment, a feature mismatch with the running server) is
caught because only an embedded TUI holds its rollout open. Once the runtime has found the TUI's
rollout (above), `Codex::load` joins that server as a second client (`agent/lens_codex_live.rs`,
transport in `agent/codex/control.rs`), provided no other Codex TUI runs in the thread's folder
(the folder each was started in or pointed at with `-C`): nothing ties a TUI process to its thread,
so with two TUIs in one folder the lens could not tell which thread is this pane's and keeps the
rollout path:

- It resolves `$CODEX_HOME/app-server-control/app-server-control.sock`, a symlink into
  `/tmp/codex-daemon-<uid>/` that keeps the path under `sun_path`, upgrades it to a WebSocket with
  `tungstenite`, sends `initialize` (experimental API on, rate limit, moderation and read-state
  notifications opted out), checks that `thread/loaded/list` holds the rollout's thread, and
  subscribes with `thread/resume {excludeTurns: true}`. A reader thread polls the socket every 20 ms
  and wakes the lens loop (`Lens::wake`), so live frames do not wait for the 250 ms tick.
- History still comes from the rollout, opened after the subscription. The item ids it names are kept,
  and live `item/*` frames for them are dropped, so nothing shows twice and nothing between the two
  goes missing. A live `item/completed` whose start came before the join gets its start first.
- Live frames go through the `codex::translate::Translator` the native driver uses.
  `rollout::live_item` turns an async question (`agentMessage` with `questions`) into the
  `userInputRequest` shape and a `<send_user_message_question_reply>` message into `userInputReply`,
  as the rollout reader does. Agent and reasoning deltas stream text. A batch of frames stops at each
  busy change, turn end or request, so the lens emits them in the order Codex sent them.
- Busy and idle follow `turn/started`, `turn/completed` and `thread/status/changed`; a `failed` turn
  fails the lens prompt with Codex's message. The resume reply and `thread/settings/updated` set the
  model, effort and folder. Subagent rows still read their child rollouts as above.
- Prompts go out as `turn/start` (images as data URLs, `codex::turn_input`), `/steer` as `turn/steer`
  with the running turn as `expectedTurnId`, and Stop as `turn/interrupt`, which leaves background
  terminals running as Esc does. Prompts that start with `/` or `!` are the TUI's own commands and
  are still typed into it. A refused `turn/start` fails the prompt; a refused steer hands the text
  back to the composer.
- Approvals: `item/commandExecution/requestApproval`, `item/fileChange/requestApproval` and
  `item/permissions/requestApproval` reach every subscribed client. Each becomes a card whose
  options are the request's `availableDecisions` (Allow, Always allow `<command>` for an exec policy
  amendment, Reject), shaped by the native driver's `codex::approval`; the reply is the decision the
  user picked. The first answer wins: when the TUI answers, `serverRequest/resolved` withdraws the
  card. Requests the lens does not show (MCP elicitation and the rest) are left to the TUI, and the
  lens never answers on its own. A blocking `item/tool/requestUserInput` is answered the way the
  native driver does.
- Async questions become an answerable card with the question and its options. The answer goes back
  as a `<send_user_message_question_reply>` message, through `turn/steer` while a turn runs and
  `turn/start` otherwise, and the TUI shows it as "↪ Re: <question>". The card closes when the reply
  item arrives, from either client. Unlike the rollout path, a question stays open after its turn
  ends, since Codex still takes the answer then.
- Footer model and effort picks and typed `/model x` and `/effort x` go out as
  `thread/settings/update` with the model and effort. Codex takes it mid-turn and applies it from the
  next turn, so nothing is held. Typed `/plan` sets `collaborationMode` to `plan` the same way. Folder
  changes are still typed as `/cd`.
- `thread/started` for a new top-level, non-ephemeral thread in the same folder (`/new`, `/fork` or
  `/resume` typed in the TUI) makes that thread the session once its rollout exists, when it forks
  the joined thread or this TUI is the only one in the folder. A thread created within 30 s of
  another TUI in the folder starting is that TUI's and is ignored. Any other new thread there could
  be either TUI's, so the lens leaves the server and goes back to the rollout path.
- When the TUI exits, the runtime unsubscribes, closes the socket and skips the rollout tail past
  what live frames covered. Every card still open from the server is withdrawn, here and whenever
  the link drops or the lens joins another thread. A Codex that shows up in the pane again joins
  without a reload once locating its rollout finds the same one, since threads stay loaded after
  their TUI leaves and the new one may be on a fresh thread: the rollout lines not read yet come
  first, then live frames. A connection the server closes reloads the session.
- Streamed command output keeps at most four times what a row shows (`codex::translate`), so a
  background terminal that keeps writing does not grow the lens.

Anything else (embedded mode, no socket, a failed handshake, a thread the server has not loaded)
keeps the rollout tail and typed keys this page describes. Checked live on 2026-10-31 against Codex
0.166 in an isolated `CODEX_HOME`: a turn typed in the TUI streamed into the lens in 23 deltas, a
lens prompt showed in the TUI, an approval answered in the lens closed the TUI's dialog and one
answered in the TUI withdrew the card, an async question answered in the lens showed as "↪ Re:
Which color?", a model and effort pick moved the TUI's status line to GPT-5.6-Luna medium, Stop
showed "Conversation interrupted", and a TUI started with `-c` stayed on the rollout path. A
second check the same day: with a lens joined to one TUI, a second TUI started in the same folder
and sent its first message without taking the lens, a lens on the second TUI kept the rollout path,
and only a `--no-daemon` TUI held its rollout open.

# Input and approvals

- **Prompts.** The composer sends `AgentPrompt` as before. The runtime delivers it with
  `TerminalLens::submit`, the bracketed paste plus Enter that waits for the text to echo before
  pressing Enter (`paste_and_submit`). Pasted images are saved in the daemon's paste folder and
  their paths pasted in first, without Enter (`TerminalLens::attach`); both CLIs turn a pasted
  image path into an attachment, and `[Image` counts as the paste having landed. Slash commands
  are the CLI's own and are typed through (see Slash commands below). The composer's
  placeholder names the agent ("Message Claude Code…"), and before the agent has written a
  transcript the lens shows `presentation::lens_welcome`: the agent's mark, "Claude Code is waiting
  for its first prompt" and "What you send here goes to the session in this terminal."
- **Steer.** `AgentSteer` types text into a running turn without opening a lens turn, so the
  running one keeps its tracking. No client sends it now; the daemon still takes it. The pane host
  sends the runtime `/steer <text>` and the lens pastes `<text>` like a prompt. Claude Code reads text typed mid-turn
  as a message for the running turn, and Codex's Enter steers. With no turn running, a permission
  waiting, or images, the host takes it as `AgentPrompt`. Claude Code records the message once the
  turn takes it in, at its next tool boundary, as an `attachment` line of type `queued_command`
  (`commandMode: "prompt"`, `origin.kind: "human"`); the translator turns that into the user's
  message, and other origins (peers, task notifications) stay hidden. Codex 0.164 records it as an
  `item_completed` `UserMessage` inside the same turn. Until then the timeline shows nothing for it.
- **Stop.** `AgentCancel` sends Escape to the TUI, or aborts the turn through the Claude Code
  plugin while it is connected.
- **Claude permission prompts.** With the Claude Code plugin connected, the hook carries them
  (above). Without it, while the dialog is up, the record reads `"status":"waiting"` with
  `"waitingFor":"permission prompt"`, and the pending `tool_use` is the last assistant block with no
  `tool_result`. The runtime raises a permission request for that tool with Allow (sends `1`) and
  Deny (sends Escape). Any other `waitingFor` (a plan approval, a question) raises a request with no
  options, titled "Waiting for a ... in the terminal". Clients draw it with
  `zz_ui::agent::presentation::terminal_wait_card`, whose one action, Show terminal, opens the
  terminal PiP (below) so the user answers there.
- **Codex approvals.** Without the app server the rollout records nothing while Codex's dialog
  waits, so the lens shows no card; the user answers in the terminal.
- **Codex questions.** Without the app server, `request_user_input_async` records an `AgentMessage` with `questions` and
  `delivery: "async"`. It becomes a question row marked as background from the start, so the call
  does not block its turn. Codex 0.166 shows the question editor collapsed: keys typed then
  (digits, Enter, text) go to its ordinary composer, and only its edit-queued-message key
  (Shift+Left or Alt+Up by default) opens the editor. So the lens never answers by typing. While a
  question is open the runtime reports `waiting` and raises a request with no options, titled with
  the first unanswered question, which clients draw as the terminal card; Show terminal opens the
  terminal PiP, where the user answers in Codex's editor. Codex sends each answer on its own, as a
  `UserMessage` wrapped in `<send_user_message_question_reply>` whose `questionItemId` is
  `["request_user_input_async", <call id>, <index>]`. The runtime records each answer by call and
  index, moves the card to the next unanswered question, and once every question of the call has
  one, closes the row with an answered record instead of a user bubble and withdraws the card.
  Codex drops its open questions when a live turn ends, so `task_complete`, `turn_aborted` and Stop
  close the row the same way, keeping the answers given and "no answer" for the rest. A prompt or
  steer from the lens or the terminal while a question is open goes to Codex's composer as usual
  and leaves the question open, and a model or effort pick waits only for the running turn. A
  question recorded before the running Codex process started is ignored, since its editor died
  with the old process, and its row closes with no answer. The same goes for one still open when
  `codex resume` starts a new process in the pane: the thread and rollout stay the same, so the
  runtime drops it when it sees the new process. A plain `send_user_message_async` message stays
  text.

# Slash commands

The lens runtime publishes the CLI's commands as an ACP `available_commands_update` after every
session reset and whenever Claude's skill list changes, so `/` in the composer opens the same
suggestion list the native drivers fill (`agent/lens_commands.rs`). It is read from files, like the
rest of the lens:

- **Built-ins**, a fixed list per CLI. The ones whose own UI is a dialog in the TUI carry
  `_meta.zz.terminal` and show "opens terminal": picking one types it into the CLI and opens the
  terminal PiP (`agent_completion::terminal_command`). Claude Code: `plan`, `status`, `config`,
  `context`, `usage`, `permissions`, `mcp`, `hooks`, `memory`, `resume`, `rewind`, `plugin`,
  `skills`, `btw`, `doctor`, `theme`, `login`, `logout`, `help`. Codex (0.166): `side`, `btw`,
  `permissions`, `status`, `usage`, `mcp`, `skills`, `resume`, `fork`, `mention`, `raw`, `logout`.
  The rest (`clear`, `compact`, `goal`, `subtask`, `cd`, `add-dir`, `export`, `copy`,
  `reload-skills`, `exit`; Codex `new`, `clear`, `compact`, `recap`, `goal`, `plan`, `review`,
  `init`, `diff`, `cd`, `pwd`, `rename`, `export`, `copy`, `quit`) run from the lens, and what they
  do shows in the timeline.
- **`/model` and `/effort`**, for both CLIs, are the footer menu's setters (below). Typed with a
  value (`/model sonnet`, `/effort high`) the client sends `AgentSetConfigOption` instead of a prompt,
  and sends nothing when the value is already the current one (`agent_settings::composer_setter`);
  typed bare, the command stays in the composer as `/model ` and the suggestion list shows the
  values from the session's config options, the current one first and marked, so Enter keeps it
  (`agent_completion::setting_completions`). Picking a value sets it. When the session lists no
  values yet, a bare `/model` or `/effort` goes to the CLI as a prompt and the CLI opens its own
  picker. A prompt that still arrives as
  `/model <value>` or `/effort <value>` (the HTML client, `zz agent-send`) goes to the same setter in
  the runtime (`lens_settings::typed`) and finishes at once, failed if the setter refused it.
- **Claude Code skills and commands** from `~/.claude` (or `CLAUDE_CONFIG_DIR`) and every
  `.claude` from the pane's folder up to its git root: `skills/*/SKILL.md` (frontmatter `name`,
  `description`, `argument-hint`), `commands/**/*.md` (a subfolder becomes `dir:name`), and the
  skills and commands of the plugins `enabledPlugins` turns on, named `plugin:name`. The
  transcript's `skill_listing` attachment adds the skills Claude Code itself loaded (bundled ones
  included) once it has written one.
- **Codex skills** from `$CODEX_HOME/skills` and `.codex/skills` up to the git root, listed as
  `$name` and inserted that way, and prompts from `$CODEX_HOME/prompts` as `prompts:name`.

Skills carry `_meta.zz.skill` and show "skill". Old clients skip the `_meta` field and show the
plain list.

# The terminal PiP

On the desktop a command that only the CLI's own screen can show does not flip the pane. The lens
draws the pane's live `TerminalView` in a wide PiP card over the chat, with no scrim
(`zz_ui::agent::presentation::terminal_pip`, `crates/zz/src/agent/view/harness.rs`). Its header
shows the CLI's mark, "/config · Claude Code" and Done, and keys go to the CLI. The `TerminalView`
keeps the pane's own size, so the card shows its bottom rows, where the CLI draws its dialog: the
lens measures its own height and pins the terminal's bottom edge to the card's. The PiP closes when:

- Claude Code logs the dialog closing. Every `local-command-stdout` line ("Set model to …",
  "Disabled auto-compact") makes the runtime send `ConfigOptionsChanged` with `option_id` `dialog`
  and value `closed`; clients count those (`AgentPaneState::dialogs_closed`).
- Claude Code's input box is back on screen. Closing /status, /usage, /context, /mcp or an
  unchanged /config logs nothing, so the runtime checks the screen for the `❯` line between two
  rules (`lens_settings::claude_composer_shown`) and sends the same `dialog` signal.
- Done is pressed, which also sends Escape to the CLI. For Codex, Esc closes it too.

Codex logs nothing for its dialogs, so its PiP closes on Esc or Done. The web, iPad and iPhone
clients still flip to the terminal for these commands, since closing the lens there also closes its
stream.

# Model, effort and folder

The runtime reads what the CLI is set to from its own files (`agent/lens_settings.rs`): Claude Code
assistant lines carry `message.model`, `effort` and `cwd`, and "Set model to …" / "Set effort level
to …" lines update them at once. Codex `turn_context` lines and `event_msg` `thread_settings_applied`
lines carry `model`, the effort (`effort` or `reasoning_effort`) and `cwd`. It sends them as the
session's ACP config options (`model`, `effort` and `cwd`, in `SessionReady`, `SessionSwitched` and
`ConfigOptionsChanged`), so no wire change was needed and old clients ignore them. Clients parse them
with `zz_client::agent_settings::LensSettings`.

Until the transcript names them, the footer shows what the CLI was started with
(`Facts::claude_launch`, `Facts::codex_launch`): the agent process's own arguments (Claude Code
`--model`, `--effort`; Codex `-m`/`--model`, `-c model=…`, `-c model_reasoning_effort=…`), read by
pid (`process_info::record`), over its config (Claude Code `settings.json` under
`$CLAUDE_CONFIG_DIR` or `~/.claude`, then the folder's `.claude/settings.json` and
`settings.local.json`, keys `model` and `effortLevel`; Codex top-level `model` and
`model_reasoning_effort` in `$CODEX_HOME/config.toml`). The transcript's values win once they arrive.
Claude Code writes its `sessions/<pid>.json` record only after the first prompt, so until then the
lens takes the first process in the pane whose first or second argument is named `claude`.

The models on offer come from a catalog per CLI (`lens_settings::Catalog`):

- **Claude Code.** The aliases `opus`, `sonnet`, `haiku` and `fable`, each with the levels `low` to
  `max`.
- **Codex.** `$CODEX_HOME/models_cache.json`, the list Codex itself fetches (read up to 8 MiB,
  reread when its mtime changes): the models with `visibility` `list`, by `priority`, named by
  `display_name`, each with its `supported_reasoning_levels` and `default_reasoning_level`. Without
  the file the current model stands alone with `low`, `medium`, `high` and `xhigh`.

The current model is always one of the choices, even when the catalog hides it. The `model` option's
entries carry `_meta.zz.efforts` and `_meta.zz.defaultEffort`; the `effort` option lists the current
model's levels, named from one table (`xhigh` is "Extra high").

The composer footer shows the folder on the left and, on the right, the model and effort button and
the side chat button (`zz_ui::agent::controls::{agent_directory_chip, agent_settings_menu,
agent_side_chat_button}`). The button opens the same menu for every CLI: the models, then the
current model's levels. It takes picks while a turn runs too, on the desktop and the thin clients
alike. Picking one sends `AgentSetConfigOption`, which the runtime types into the
TUI (`lens_settings::command`) and shows at once:

- **Claude Code.** `/model <alias>` or `/effort <level>`. Its "Set model to …" line confirms it.
  With the Claude Code plugin connected, a model pick goes through `$.config.set` instead and
  nothing is typed.
- **Codex.** Joined to the app server, picks go out as `thread/settings/update` (see
  [Codex over the app server](#codex-over-the-app-server)). Otherwise: Codex has no `/effort`; its
  `/model` takes the level inline, so a model pick types
  `/model <slug> --reasoning=<level>`, keeping the current level when the new model takes it and the
  model's default otherwise, and a level pick types `/model <current slug> --reasoning=<level>`.
  Codex answers "Model changed to … for this session only" in the terminal, and its rollout names
  the settings in the `turn_context` it writes about a second after the next turn starts, so the
  lens lays the pick over what the rollout says, across a reload, until a rollout line names the
  settings again; a pick Codex refused shows until then. Codex refuses `/model` while a task runs,
  so a pick made mid-turn is held and typed when the turn ends; a later pick replaces a held one.
  A new Codex process in the pane drops the picks. A slug or level the catalog does not list fails
  with `SettingFailed` before anything is typed.
- **Folder.** On the local host the folder chip opens a folder picker and the runtime types
  `/cd <path>`. On a remote host it only shows the folder. Codex holds a mid-turn `/cd` the same way.

# Side chats and forks

- **Side chat.** The footer's chat-plus button opens a floating pane in the same window running a
  fork of the session: `claude --resume <session> --fork-session`, or `codex fork <session>`,
  through the user's interactive shell like a new chat (see [Starting a chat](#starting-a-chat)).
  The main chat never sees it. In chat mode the side chat gets its lens like any agent pane; in raw mode
  the header toggle opens it.
- **Copy and Fork from here.** Hovering a message shows Copy and a branch button under it; the last
  answer keeps them shown, and on touch they always show. Copy puts the message's text (for an
  answer, every text block of the turn) on the clipboard. Fork opens a new window after
  the chat's running `zz tools --fork-chat <claude|codex> <session> <message>`, which runs on the
  CLI's host through the pane's private `zz` (`crates/zz-cli/src/fork_chat.rs`):
  - Claude Code: it copies the transcript through the end of that message's turn under a new
    session id in the same project folder, then runs `claude --resume <new id>` through the user's
    interactive shell.
  - Codex: it finds the message's `turn_id` in the rollout, asks the app server for `thread/fork`
    with `lastTurnId` (through `codex app-server proxy`, or its own `codex app-server`), then runs
    `codex resume <new thread>` the same way.
  - The chat item id carries the source id (`u:<uuid>` for Claude, `u:<item id>` for Codex), and
    `zz_client::agent_settings::source_message` reads it back.

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
  (`runtime::validate_payload`). In chat mode a prompt sent through the native drivers gets the
  same treatment: `runtime::prompt_echo` puts its images on its first text chunk (an empty one
  for a prompt that is only images), and the image chunks after it stay for the timeline. The
  lens and both drivers drop `_meta.zz.images` from the raw update once the bridge has read it,
  so the stream carries each image once; the driver's journal keeps it, so a restored session's
  chat shows it again. The fanout's chat log and every client's `ChatView` keep images on the
  last 8 items that have them (an item that loses its images gives up its place), so a live chat
  and a late client's replay both show older pastes as text only. The bubble draws them first as
  140 px tiles that open full size on click; `ChatPane::sync` decodes each item's images once and
  keeps them in `ChatOpen::images`. When an item leaves the view, loses its images, or the session
  changes, `ChatPane::release` drops the decoded pixels from ala's asset cache and the sprite
  atlas, unless a live chat pane still shows the same image (an app global lists each pane's
  shown images). The desktop agent view and the thin client's lens hook
  `ChatPane::close_on_release`, so closing the pane frees every image it held, through its window
  or, when the window closed with it, from the asset cache alone.
- **Live turns.** The last line of a running turn says Thinking, or Working while a tool runs, with
  a shimmer across the word, a turning sparkle and the time since the prompt. It hides while the
  chat waits on you. The transcript only gets a reply once it is whole, so a reply that lands while
  you watch types itself in over 220 to 900 ms (`ChatPane::sync` notes when it arrived; replies
  already there on the first look show at once).
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
  header holds its label, Copy source and Open full diagram. The diagram's canvas is transparent, so
  it shows the card through it; edge and relation labels keep a chip in the node fill. Answers use
  the same markdown style as code blocks and tables elsewhere (`agent_text_view`). The chat passes
  its pane as the owner (`ChatHooks::owner`), so on the desktop an HTML block's live page can reply
  to the agent, and two panes showing the same item keep their own preview toggles.

# Starting a chat

zz starts every agent CLI (new chat, side chat, fork) through the user's interactive shell, so
aliases and shell functions such as `alias claude='claude --dangerously-skip-permissions'` apply.
The pane runs `/bin/sh -c` with `exec "${SHELL:-/bin/sh}" -ic '<command>'`
(`zz_client::agent_launch::interactive_shell`); `/bin/sh` parses that line whatever the pane's
default shell is, and the user's shell only sees a plain command line. Bash and zsh exec the
expanded command, so the pane's process is still `claude` or `codex` and the lens finds it.

A new chat starts with the model and effort last picked for that provider: Claude Code
`--model <alias> --effort <level>`, after the alias's own flags so they win, and Codex
`-c model="<model>" -c model_reasoning_effort="<level>"` (Codex refuses a second `-m`). The desktop
remembers them in `agent-launch.json` in its data folder (`crates/zz/src/agent/launch.rs`) when the
footer menu sets one, or when the lens settings change model or effort within a chat (the CLI's own
`/model` and `/effort`). The web, iPad, iPhone and HTML clients start chats without them.

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
  the watch starts. A client can follow an agent this way without attaching to its session, which
  would resize its panes, and an attached client keeps a lens live while its focused window shows
  something else. A client that both sees and watches a pane is
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

The lens draws the chat (`crates/zz-ui/src/agent/chat.rs`): `chat_thread` draws the rows
`zz_client::chat_rows` makes from the `ChatView`, inside `chat_column`, the column both apps share,
and `chat_agent_pip` draws a subagent's chat in its PiP. The permission and question cards,
`AgentComposer` and the task tray live in `crates/zz-ui/src/agent/`. Both apps still build
`AgentTimeline` (`crates/zz-ui/src/agent.rs`) and draw it only while the chat view is empty, as
against a daemon older than the chat machine or before the first chat turn arrives.
`AgentComposer` holds the `Composer` pill: `+`, which starts an `@` mention with the palette's
target picker (`zz_ui::target::TargetPicker`, inserted with its host), the field, the context ring
and the round send button; the cards over it and the branch under it are the lens's own. The
timeline keeps its `TimelineStick` / `StickSpring` follow-the-tail pin (`agent.rs`) for that case;
the display-only Markdown repair for streaming text (`zz-client/src/mend.rs`), the streamed-text
fade (`penna/src/text/veil.rs`) and the shared leased animation clock (`penna/src/pulse.rs`)
carried over unchanged.

- **Desktop.** `crates/zz/src/agent` (controller and view) behind the `agent-pane` cargo feature,
  which stays on by default; without it a stub stands in.
- **Web and iPad.** `clients/app/src/app/agent_lens.rs`.
- **HTML client.** `clients/dom/src/agent.rs` builds the lens view from the
  shared `zz-client` reducers as JSON; `clients/dom/web/js/agent.js` draws them with the design kit.
- **Storybook.** One Agent story (`clients/storybook/src/stories/agent`) draws every chat state
  from fixed chat events with the same calls, `chat_column` included, and asks the way the
  desktop draws them.

The lens has no project picker and no provider picker; the old Agent pane's pickers were deleted,
not hidden. Model, effort and folder are the composer footer's controls (see
[Model, effort and folder](#model-effort-and-folder)).

# Related

- [Agent lens design](/designs/agent-lens.md) . rulings, deletion map, follow-ups
- [Daemon-owned agent runtime](/designs/agent-daemon-runtime.md) . host, fanout, journal
- [Native agent drivers](/designs/native-agent-drivers.md) . the translators the lens reads with
- [Agent peer bus](/architecture/agent-peer-bus.md)
- [Wire protocol](/protocol/wire-protocol.md)
- [tmux command set](/tmux/commands.md)
- [zz](/crates/zz.md) and [zz-daemon](/crates/zz-daemon.md)
