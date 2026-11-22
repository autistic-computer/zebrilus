---
type: Design Plan
title: Agent chat machine . one owner for everything a chat shows
description: "One pure machine in the daemon owns a driver-backed chat: the session, the turn (through the existing turn machine), every item with its parent and scope, subagents, approvals and questions, and the prompt queue. It emits typed chat events; the journal keeps them and every client folds them with one projector in zz-protocol, so a replay builds the same view the live stream did. Built and fuzzed next to the Claude and Codex drivers, which send its events on the agent stream; the desktop lens folds them and draws the chat, with asks in the composer and each subagent's chat in a PiP."
status: Built (2026-10-29, branch feat/agent-chat); events on the agent stream with replay through a compacted log; the desktop lens draws the chat with inline asks and subagents opening in place, checked live in zz Dev against Claude Code 2.1.312; 2026-10-31 subagents became one row each opening a PiP
resource: crates/zz-daemon/src/agent/chat.rs
tags:
- agent
- design-plan
timestamp: 2026-10-31T19:10:00-03:00
---

# Why

The [turn machine](agent-turn-machine.md) decides one thing: whether a turn runs. Everything else a
chat shows is derived again in five places (host, fanout lane, desktop controller, thin client
reducer, lens runner), from untyped ACP `session/update` values. That is where the bugs live:

- A background subagent's approval vanishes when any turn ends: the host and the client reducer
  clear every open permission and tell nobody, so Claude or Codex waits forever.
- Only the first open request reaches the wire.
- Subagent steps reach the client and nest, but nothing says when a child is done or waiting.
- The journal keeps `Update` records only, so a replay loses permissions, tasks and turn ends.

# Chat contract

- **One owner, typed records.** Drivers translate vendor frames into facts. The daemon owns
  turns, items, subagents and requests; clients apply its events.
- **Every request closes exactly once.** One owner mints request ids; at a turn end every
  unresolved request of that turn is closed; an answer to a closed request is never sent.
- **Children before parents.** A child's output can arrive before the spawn that owns it (Codex
  child threads before adoption, Claude frames before `task_started`); the machine buffers it.
- **Interrupt closes dangling tools.** Tools left without an end when a turn stops end as
  interrupted, never as spinners.
- **Background work outlives its turn.** Subagents and commands that run in the background keep
  their requests and end on their own.

In `chat_rows` and `zz_ui::agent::chat`: each subagent is its own row (the CLI's mark, its name,
a status glyph and how long it took), and clicking one opens its chat in a PiP docked beside the
thread (`chat_agent_pip`): its prompt, its own rows, its answer and a footer line. An answered question
shows the question and the answer under it. Mermaid and code-block actions are a separate piece of
the markdown view.

# Shape

```
vendor frame -> driver -> Fact -> ChatMachine (owns TurnMachine) -> ChatEvent -> journal / wire
                                                                          -> ChatView (every client)
```

- `zz_protocol::chat` holds the wire types and `ChatView`, the projector. `ChatView::apply` is the
  only way a client's chat changes. It checks every event against what it has (no item twice, no
  end with children open, no turn end with its items or requests open) and counts what it skips;
  the machine never emits an event the projector skips, and the tests hold it to that.
- `zz-daemon/src/agent/chat.rs` is the machine, on `zz-machine` like the turn machine. Turn facts
  go through the turn machine unchanged; its effects (`Settle`, `Interrupt`, `Drain`, `Reclaim`)
  drive the chat. The state stays small (open items, open requests, two bounded rings of closed
  ids, the queue), so a text delta is a `stay` with one emitted event and no clone.
- The lens runner (`agent/lens.rs`) runs the same bridge over what it reads from a terminal
  Claude Code or Codex session (its readiness held back until the transcript history is in, its
  busy and idle as the turn), so every lens pane already sends chat events too. A reload (a
  rewind, `/clear`, Codex `/new`, a new rollout) closes the chat's session and its
  `SessionSwitched` readies it again. Only the end of a lens prompt counts as idle: the lens sends
  `Activity { busy: false }` with its `PromptFinished`, so a `/model` or `/effort` it handles for
  the composer, or a prompt it refuses, settles the host's turn and leaves a running chat turn
  alone.
- What a Claude Code transcript says differently from stream-json, found in a live run on
  2026-10-29 (2.1.312) and handled by the shared translator and the lens:
  - Explore and other subagents run in the background by default. The launch result carries
    `toolUseResult.isAsync` (`status: "async_launched"`, `agentId`) and the end comes later as a
    user line starting `<task-notification>` with `<tool-use-id>`, `<status>` and `<result>`. The
    translator keeps the row running and ends it from that line, with the result as its text.
    Claude can hold the notification back (it was busy with a question), so the lens also ends a
    background subagent once its `subagents/agent-<id>.meta.json` says `finished: true`, with the
    subagent's last message as the result.
  - Each subagent writes its own transcript under `<session>/subagents/`, and its meta names the
    parent `toolUseId`. The lens tails those files and feeds them through the translator with that
    parent, so an opened subagent row shows its own reads, commands and answer.
  - An `AskUserQuestion` answered in the terminal ends with `toolUseResult.answers`. The translator
    passes them as `_meta.zz.answered` and the bridge records a question and answer pair, unless
    the answer already came through zz. The question's tool row leaves the run line, and the
    record stays out of the turn's fold.
  - When Claude waits on a single question with one choice (up to 9 options) and its tool call is
    already in the transcript, the lens shows the real question inline and answers by typing the
    option's number, which Claude's dialog takes as the answer. Claude sometimes writes that tool
    call only after the dialog closes; then the card only offers Show terminal.
  - The lens used to apply a tick's transcript lines before the busy flip, so a prompt typed in
    the terminal landed before its turn and was drawn out of order. Busy now goes first, a user
    line with no turn open starts one, and `turn_duration` ends it. Background work still open
    when the lens replays history stays open, since that Claude is alive.
- `zz-daemon/src/agent/chat_bridge.rs` turns what the drivers emit today (ACP updates, permission
  requests, the task list) into chat facts. Both drivers feed it from `apply` (with the same clock
  the turn machine used), `emit`, and their answer paths; the events are recorded in tests and
  dropped otherwise. History a driver restores before the session is ready is held and replayed
  once it is, chunks of one message kept together, with anything left open closed. On Codex,
  whatever still runs when a turn completes (a background terminal, a spawned agent) leaves the
  turn; the bridge asks the turn machine first, so a stale completion moves nothing.

# Replay

A client that attaches late asks the fanout lane for a replay. While the lane's memory still holds
the requested items it sends them; once older items were dropped it sends a reset and rebuilds from
the journal or from memory, and the chat events would be lost. So the lane keeps a `ChatLog`: every
chat event in order, with text appends and body changes folded into the item's start and only the
latest queue and usage kept. The journal path ends with `Restart` and that whole log, which folds to
the same view (the random test checks it at every event). The rebuild from memory goes on the
numbers the asker skips and stays within 4.5 MiB: it sends `Restart`, the events of the log still in
effect (`ChatLog::in_effect`: the last session start and end, the running turn, every open item and
the item of every open request with their parents, every open request, the turns those started in,
the queue and the usage) and then the newest events that fit, all in their original order. A
rebuilt chat can show an old turn holding only its still-open rows, and an open item without its
finished children. When the rebuild needs more numbers than the asker skips, the lane first takes
exactly the missing ones with repeats of its latest usage event (its latest queue event when it
has no usage, an empty queue when it has neither), which every viewer gets and which change no
client's chat, and the asker gets the rebuild on those numbers too. Only when the events still in
effect pass 4.5 MiB on their own does it send no chat, and the asker keeps the chat it had. The
log grows with items and turns, not with streamed text, and keeps pasted images only on the last 8
items that have them.
`ChatView` applies the same rule as it folds events, so a connected client's view holds no more
images than a replay would give it. A daemon restart needs no journal for this: the driver restores
the session's history and the bridge replays it through the machine. When the vendor's history is
gone, Claude and Codex fall back to the journal, and the bridge opens a turn at every turn mark in
it. The vendor history readers write those marks, and a live turn now journals its own start and
end mark too (`Shadow::marking_turns`), so the fallback rebuilds the turns; journals written before
2026-11-22 have none and reload with every row outside a turn. An ACP provider that cannot load a
session falls back to the journal too. There the bridge opens a turn at each run of user text, as
it does for a loaded session, so an ACP turn journals only its end mark (`Transcript::ended_turns`,
handed to the journal by `dialect::run_through`), and a turn that was stopped or failed reloads
that way; an ACP journal written without them reloads every turn as completed. When a person
declines an ask, the shadow journals a note naming the tool (`_meta.zz.declined`) the same way,
and the fallback reads it so the tool's failed end shows as declined, as it did live; Claude and
Codex already carry the decline in the tool update they journal. The fanout's journal replay
leaves the marks and notes out (`is_zz_note`), as it does `config_option_update`, and the ACP
dialect keeps the ones a fallback replays out of the stream it forwards.

# Rules

1. A session is starting, ready or ended. A turn is idle, running or stopping; waiting is derived:
   an open blocking request anywhere, a background one included.
2. Every item has one scope: the running turn, or the session (background). A child takes its
   parent's scope. Output with no turn running goes to the session scope; it never opens a turn on
   its own, so it cannot leave a turn stuck. A background item outlives its foreground parent:
   ending the parent does not end it.
3. Ending an item ends the open part of its subtree that shares its scope (text completed, tools
   interrupted) and withdraws the requests still open on those items.
4. A turn end dismisses the turn's requests, tells the driver (`Withdraw`), then ends the turn's
   open items, deepest first, then emits `TurnEnded`. Background items and their requests survive.
5. Stop (`Cancel`) emits `TurnStopping` and dismisses the turn's requests at once; the turn ends
   when the agent confirms, or after the turn machine's 10 second grace. While stopping, an idle
   from the agent ends the turn only once every command in it has ended, so a stale idle cannot
   beat a turn Codex has accepted but not started.
6. An event for an item or request that already closed is rejected as stale (the last 8192 items, as
   64-bit keys, and 1024 requests are remembered); an unknown one is rejected. A child whose parent
   has not started yet waits in a bounded buffer (256 facts) and is replayed when the parent starts.
   Requests never wait: a request opens at once, names the item it is about even when that item has
   not started, and takes the item's scope once it does, so a live request is never hidden.
7. An answer goes to the agent (`Respond`) only for an open request, and the chat records it as the
   answer. A request the agent resolved elsewhere closes once; a second resolution is ignored.
8. Exit ends everything: the turn, every item, every request, and hands the queue back. A session
   reset ends everything too but keeps the queue for the next session. When a different session
   becomes ready (a switch, a fork, a restore), the chat starts a fresh transcript: the view drops
   the old one and the machine forgets the old ids, so restored history can reuse them.
9. A submitted prompt is sent at once when the chat is idle and nothing is queued or in flight,
   and queued otherwise. The driver answers every `Send` with the prompt fact or `SendFailed`; a
   failed send hands that prompt back to the composer and sends the next one. A completed turn
   sends the next prompt, and so does a turn the agent started, whatever its outcome; a cancelled
   or failed prompt turn hands the queue back.
10. A prompt that lands while the agent runs a turn of its own takes that turn over: the chat says
    so (`TurnClaimed`), and the turn waits for the prompt's own command to start before an idle can
    end it.

# Events

`SessionReady`, `SessionEnded`, `TurnStarted { by }`, `TurnClaimed`, `TurnStopping`, `TurnEnded { outcome }`,
`ItemStarted { turn, parent, kind, body }`, `ItemAppended`, `ItemChanged`, `ItemBackgrounded`,
`ItemEnded { status }`, `RequestOpened { item, ask, blocking }`, `RequestClosed { close }`,
`QueueChanged`, `UsageChanged`, `TurnActivity { text }` (what the running turn is doing, `None`
clears it) and `ItemsSummarized { items, summary }` (a sentence for a finished batch, applied to
closed items too). Items carry start and end times, so a client can say "Worked for 1m
12s" without a clock of its own. Item kinds: user, agent, reasoning, plan, notice, error, subagent,
read, edit, search, execute, fetch, web, mcp, tool.

# Reviews

A Codex review on 2026-10-29 found 15 problems (7 high), all fixed with a test each: a background
child's approval withdrawn when its foreground parent ended; a buffered request reopening after the
agent resolved it; a request losing its item and so its scope; a takeover prompt completed before
its command started; a stop before Codex named its turn never reaching it; child-thread resolutions
dropped by the Codex driver; the shadow keeping its own clock; forgotten ids; restored history
discarded; Codex background terminals closed by the turn end; cached tool bodies never freed;
answers recorded as withdrawals; takeovers missing from the view; and two fold mistakes in
`chat_rows`.

A second round confirmed 7 of those fixes and found the rest partial, plus 12 new problems. All
are fixed with tests: an old idle beating a stop; a stale Codex completion moving the running
turn's tools; Codex agents closed by the main turn; background marks lost while a child waited for
its parent; requests with no row while their item had not started; a refused takeover that could
never end; Claude recording an answer it had not sent; sessions mixing on a switch; replay
splitting one message in two; work hidden under a closed card; nested follow-up agents losing
their parent; and Codex child output dropped before the spawn named its thread. Two limits stay by
choice: a request about an item that already closed is still shown (a live request is never
hidden), and ids are remembered within the bounds in rule 6.

A third round confirmed 18 of 27 items and found nine narrower races, all fixed with tests: a
grandchild replayed on adoption given its grandparent; a request on a child still waiting for its
parent dismissed with the turn (such requests now wait in the session scope until the item
starts); an approval about a subagent with no row; an old command's late start binding a claimed
prompt (a claimed prompt now binds only through Codex's reply or its own end); approvals and
history from before the session was ready; a full child buffer dropping the child's turn end
(deltas are no longer buffered and turn ends always are); old-session updates leaking into a new
one; live requests under finished work hidden in the fold; and restored finished spawns shown as
interrupted.

A fourth round confirmed most of those and found two more, fixed: Codex can answer a claimed
prompt's `turn/start` by steering the turn that already runs, so the reply names a command the
turn knows (acceptance now binds it anyway, and an idle cannot end a prompt Codex has not answered);
and the drivers' reset before readiness cleared approvals buffered while loading (a reset now keeps
them). A request about an item nobody has seen yet waits in the session scope until that item
starts, and restored Codex agents now get their children's history before their own end.

A fifth round confirmed every remaining item complete and found no new high-severity problem it
could tie to real Claude Code or Codex behavior.

# Tests

- `chat_tests.rs`: 34 scenarios, `chat_bridge_tests.rs`: 17, `chat_rows_tests.rs`: 28, (background approvals, children before parents, stale ends, stop
  grace, exit, queue, steer, reset) and a random test: 40 seeds of 3000 random facts in CI, run at
  1500 seeds of 6000 facts once, checking after every step that the projector skipped nothing, that
  machine and view agree on open items, requests, turn, waiting and queue, that nothing is open
  under a closed parent or in an ended turn, and that a send never gets stuck.
- The Claude and Codex replay tests run every recording through the drivers with the shadow machine
  attached and require no rejects, the same running turn as the driver, and no item outliving its
  turn.
- An ignored test in each replay suite runs every recording in `$ZZ_REPLAY_DIR` the same way and
  prints rejects and items left open. The drivers also handle these cases:
  - The Claude translator knew a tool went to the background (`task_started.is_backgrounded`) but
    never said so, so the turn end closed a background Bash or subagent and its real end arrived
    stale. It now marks the tool with `_meta.zz.background`, and the bridge moves it out of the turn.
  - A Codex `spawnAgent` call completes as soon as the agent exists, so the agent's row closed while
    its child thread kept working. The row now stays open until the child's own turn ends, and a
    follow-up turn of the same child gets a new row (`<spawn>~2`).
  - Codex reports the thread idle before an interrupted turn's last items. A stopping turn now waits
    for its open command like a running one does, still bounded by the 10 second grace.

  What the replays still reject is expected: a Claude `result` for a wake turn when the recording
  has no session state events, and a child's end that arrives after its parent was stopped.
- The ACP tapes in `agent/fixtures` were recorded with the tape rig and scrubbed: agy and Grok
  each have hello, tools, stop, queue and reload, and Cursor has hello and reload, where every
  prompt got only the refusal "Add a payment method to continue". `runtime/tape_tests.rs` plays
  each one through the real client: the raw `out` lines go over an in-memory pipe into
  `run_agent_connection` inside `dialect::run_through` with the provider's own dialect, one task on
  a local executor stepped until nothing can run. An `out` line goes in only after zz wrote the `in`
  lines before it, with response ids rewritten to the ones zz used. The player injects through the
  host channels only what a person or the host caused: prompts with the recorded text, stops,
  permission answers with the recorded option, and setting changes. When a dialect asks for its
  quota, the host's model setting after `SessionReady` and its queued prompt after
  `PromptFinished` go in right after the poll that sent that payload if the tape shows them ahead
  of the quota ask. The conformance check (`every_acp_tape_plays_through_the_real_driver`) needs
  every tape line consumed and every `in` line written by zz with the recorded method and params,
  ignoring ids, the rules text and the client version, and nothing written that the tape lacks. A
  reload tape plays as two processes on one pane's journal, the second opened on the first one's
  session.
- Cut-point sweeps (`agent/cut_points.rs`) take each recording as a tape (`claude::tapes`,
  `codex::tapes`, `runtime::acp_tapes`: the payloads every recording line produced, and what the
  journal held after it) and do something to the chat at every position, then compare with the
  uninterrupted view:
  - Late attach (`every_cut_of_a_recording_compacts_to_the_view_a_live_client_holds`, and the same
    check on the random chats in `chat_tests.rs`): `Restart` plus `ChatLog::events()` at the cut,
    then the rest live, must equal the straight fold.
  - Fanout replay (`fanout.rs`,
    `every_replay_path_at_every_cut_of_a_recording_shows_the_live_chat`): the real
    `AgentFanout::replay` for a spread of `from_seq`. The ring, reopened lane and journal paths
    must give an equal view. The capped rebuild, tried at every cut size through `room`, must keep
    the phase, the running turn, the queue, the usage and every open item and request, come
    numbered from `from_seq + 1` to the lane's last number with no gap, and leave a viewer that was
    current with the same chat and no gap. Past a lane's first 32 payloads the sweep samples: it
    checks a line only after its last payload and skips a line inside a run of lines with the same
    payload kinds, the ring path takes 3 of its 9 `from_seq` values, and the rebuild takes every
    `room` up to 24 and 4 spread beyond.
  - Journal reload (`a_reload_from_the_journal_at_every_line_shows_the_live_chat`): what a real
    `AgentJournal` held at that line goes through each driver's history fallback into a fresh
    runtime. The turns, their outcomes and the rows in each must match the live ones, and every
    answered question must come back with the same item, question and answers. A turn or row still
    running live is expected to read as ended, as `Shadow::replay_history` ends them. A process
    opened on an earlier session is compared only from the line its session is ready, since its
    chat stays empty while the provider replays the history.
  - Reload (`a_second_process_shows_the_chat_the_first_one_ended_with`): on each two-process
    tape, the chat once the second process has its session must have the turns, outcomes and rows
    the first one ended with. The one allowance is a restore notice added to the background when the
    provider refused to load the session.

  A failure is grouped by signature (sweep, path, first differing field) with a count and its
  smallest example. Each sweep ends with the exact list of signatures it finds today and fails
  when that list changes either way, so a listed signature is an open bug and fixing one means
  deleting its line. Open on 2026-11-22: a rebuild whose events still in effect pass 4.5 MiB on
  their own sends no `Restart`, so that client keeps its old chat; no recording reaches it. A chat
  reloaded from the journal shows an answered question with its question and answers but without
  its options, since only the tool update that carries the answer is journalled, and permission
  cards and open questions are not journalled at all. The restore notice of a provider that refused
  to load the session is journalled, so every later reload shows one more of them. The two Codex
  rollout fixtures have no driver that plays them and are not taped.

# Next

1. The lens draws the chat on desktop, web and iPad. The desktop controller folds `Chat` events
   into a `ChatView` per pane (`AgentController::pane_chat`); the thin client already folds them
   in `LensTranscript::chat`. Once that view has a turn, both draw it with
   `zz_ui::agent::chat::chat_surface`, a list that lays out only the turns on screen and follows the
   bottom until you scroll up, holding folds, the opened subagent and the list in the shared
   `ChatPane`; each app is a `ChatHost` that hands the surface its chat. Sending
   a prompt or switching sessions follows the bottom again. Pending approvals and questions leave
   one row in the chat and are asked in the composer (see [agent panes](/concepts/agent-panes.md)),
   including a request the chat does not know. Subagents started together share one
   card with a row each (status glyph, name, a one-line preview, how long it took); a row opens and
   closes in place to show the prompt, the agent's own rows and a footer ("Done in 12s"). There is
   no drill-in page and no back button: a subagent never navigates inside the pane. The head
   request is always in view, since the composer asks it; clicking a row stops following the
   bottom so the row stays put. History replayed while no turn runs lands in the background and is drawn before
   the first turn that started after it. If one ever
   needs more room it opens as a zz split pane or a PiP, never a view inside the chat. The old driver-backed pane kind
   (`69ceca016`) stays gone, confirmed by fabrico on 2026-10-29: chat mode's windows run the CLI with the lens over it, so the lens is
   the chat surface. Checked live in zz Dev on 2026-10-29 (scratch HOME, Claude Code 2.1.312):
   parallel background subagents, a subagent opened in place, a question answered from the card
   and one answered in the terminal. The web and iPad clients were checked only by tests and the
   storybook.
2. Driver gaps the research found: ask Claude for summarized thinking, read Codex `activeFlags`
   and `willRetry`, and ask Codex for reasoning summaries (`summary: "auto"`). Claude already
   forwards subagent text (`forwardSubagentText`): each block arrives tagged `_meta.zz.parent`,
   nests under its subagent as a finished item, and the old timeline skips it.
3. Now that the GUI clients draw from `ChatView`, delete what derives agent state elsewhere: the host phase,
   the fanout turn id, the desktop `submitted` flag and the thin client's own reducer.
