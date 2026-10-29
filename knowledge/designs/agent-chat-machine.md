---
type: Design Plan
title: Agent chat machine . one owner for everything a chat shows
description: "One pure machine in the daemon owns a driver-backed chat: the session, the turn (through the existing turn machine), every item with its parent and scope, subagents, approvals and questions, and the prompt queue. It emits typed chat events; the journal keeps them and every client folds them with one projector in zz-protocol, so a replay builds the same view the live stream did. Built and fuzzed next to the Claude and Codex drivers, which send its events on the agent stream; the desktop lens folds them and draws the chat, with asks inline and subagents opening in place."
status: Built (2026-10-29, branch feat/agent-chat); events on the agent stream with replay through a compacted log; the desktop lens draws the chat with inline asks and subagents opening in place, checked live in zz Dev against Claude Code 2.1.312
resource: crates/zz-daemon/src/agent/chat.rs
tags:
- agent
- design-plan
timestamp: 2026-10-29T00:00:00-03:00
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

In `chat_rows` and `zz_ui::agent::chat`: one card for subagents started together
with a compact row each; the subagent's prompt and a footer line when its row is opened in place;
question → answer pairs for answered questions (the run line already counted failures). A subagent
opens in place. Mermaid and code-block actions are a separate piece of the markdown view.

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
  busy and idle as the turn), so every lens pane already sends chat events too.
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
latest queue and usage kept. Both rebuild paths end with `Restart` and that log, which folds to the
same view (the random test checks it on every seed). The log grows with items and turns, not with
streamed text. A daemon restart needs no journal for this: the driver restores the session's
history and the bridge replays it through the machine.

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
`QueueChanged`, `UsageChanged`. Items carry start and end times, so a client can say "Worked for 1m
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

# Next

1. The lens draws the chat on desktop, web and iPad. The desktop controller folds `Chat` events
   into a `ChatView` per pane (`AgentController::pane_chat`); the thin client already folds them
   in `LensTranscript::chat`. Once that view has a turn, both draw `zz_ui::agent::chat::chat_body`
   in place of the old timeline, holding folds, the opened subagent and follow-scroll in the
   shared `ChatPane`. Pending approvals and questions sit inline where the chat puts them, built by
   the same permission and question cards the composer used to dock (`ChatHooks::ask`); a request
   the chat does not know still docks above the composer. Subagents started together share one
   card with a row each (status glyph, name, a one-line preview, how long it took); a row opens and
   closes in place to show the prompt, the agent's own rows and a footer ("Done in 12s"). There is
   no drill-in page and no back button: a subagent never navigates inside the pane. The keys that
   answer the head request (Enter, digits, Escape) do nothing while the thread is scrolled away
   from it, so nothing is approved out of sight; clicking a row stops following the bottom so the
   row stays put. History replayed while no turn runs lands in the background and is drawn before
   the first turn that started after it. If one ever
   needs more room it opens as a zz split pane or a PiP, never a view inside the chat. The old driver-backed pane kind
   (`d20c49f4b`) stays gone, confirmed by fabrico on 2026-10-29: chat mode's windows run the CLI with the lens over it, so the lens is
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
