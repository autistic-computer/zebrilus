# Agent pane port

The agent port follows the pure logic in
`crates/zz-client/src/{agent_timeline,agent_flip,agent_output,agent_question,agent_completion,agent_settings,agent_lens,chat_rows}.rs`,
the chat events in `crates/zz-protocol/src/chat.rs`, and the controller and
view in `crates/zz/src/agent/{controller,view}.rs` with the chat drawn by
`crates/zz-ui/src/agent/chat.rs`. `AgentPane` in `zzmojo/agent_pane.mojo` is
the one entry point the workspace needs. It is not wired into `WorkspaceView`
yet: an agent pane (kind 4) still draws "Connecting to terminal…". The steps
to wire it are at the end of this page.

## Checks

From the repository root. Each driver takes the shared one-job build guard
itself.

| Command | What it proves |
| --- | --- |
| `python3 mojo/scripts/agent_logic_vectors.py` (`differential-agent-logic`) | Timeline, flip and output logic against `crates/zz-client/examples/agent_logic_vectors.rs`: 973 timeline cases, 16 flip cases, 9 output cases |
| `python3 mojo/scripts/chat_rows_vectors.py` (`differential-chat-rows`) | Chat event decoder, chat view reducer and chat rows against `chat_rows_vectors.rs`: 372 scripts, 18,673 events (11,023 applied, 7,460 skipped over all 13 skip kinds, 190 decode refusals), 11 duration cases, 30 reasoning cases, one refusal at the full bound |
| `python3 mojo/scripts/agent_card_vectors.py` (`differential-agent-card`) | Question card, settings and composer completions against `agent_card_vectors.rs`: 3,711 cases |
| `python3 mojo/scripts/agent_lens_vectors.py` (`differential-agent-lens`) | Stream cursor, the bounded feed and the lens helpers against `agent_lens_vectors.rs`: 1,013 cases, 405 of them cursor scripts and 404 feed scripts |
| `python3 mojo/scripts/agent_pane.py` (`test-agent-pane`) | The pane model (10 scenarios) and `AgentPane` drawn through mjui (7 scenarios), exact values; writes `target/port/agent-pane.json` |

## Modules

| Module | Responsibility |
| --- | --- |
| `agent_text` | Byte-offset text helpers on UTF-8 boundaries: slice, find, trim, lines, words, scalars |
| `agent_json` | Bounded JSON reader over a flat node table; the last duplicate key wins, as in the source decoder |
| `agent_chat` | `ChatEvent` decoder and the `ChatView` reducer: turns, items, requests, queue, phase and the 13 skip kinds |
| `chat_rows` | Units, runs, folds, labels, durations and reasoning bodies the chat draws |
| `agent_timeline` | Composer action, context usage text, timeline tool rows and diff statistics |
| `agent_flip` | Agent and CLI flip decisions |
| `agent_output` | Tool output trimming |
| `agent_question` | `QuestionCard`: picks, the "Other" answer, problems, submit steps |
| `agent_settings` | Config options, the model and effort summary, setters and the folder label |
| `agent_completion` | Slash command and setting completions, scoring and labels |
| `agent_lens` | Stream cursor with replay, pending permission parsing, restored prompts, usage, empty state and placeholder text |
| `agent_events` | `AgentFeed`: one pane's bounded consumer of agent update batches and pane state |
| `agent_pane_model` | `AgentModel`: draft, attachments, completions, permission and question answers, and the outbox of typed effects |
| `agent_pane` | `AgentPane`: the model plus layout, input routes and the drawn pane |

## Stream consumption

`AgentFeed.updates(first_seq, items)` takes one `AgentUpdates` batch as the
daemon sent it: the first sequence number and one JSON document per item. It
returns `FeedResult(refusal, replay, invalid, restarted, switched, accepted)`.

- The cursor admits each item in order. A gap or a lost stream stops the
  batch and sets `replay` to the sequence to ask for; a stale item is dropped;
  an undecodable one sets `invalid` and is skipped.
- An admitted item updates usage, settings, restored prompts, pending
  permissions and, for `chat` items, the `ChatView`. A chat event the reducer
  does not accept is counted in `ChatView.skipped` with its kind and changes
  nothing.
- `set_state` takes the pane's `AgentPaneState` (phase and failure text,
  queued prompt count, session id, title, error, config options JSON, parked
  permission id and payload). `disconnect` marks the feed offline; `lagged`
  gives the sequence to replay from.

Bounds, all refusing and none truncating:

| Bound | Value | Refusal |
| --- | --- | --- |
| One batch | 9 MiB | `FEED_REFUSED_BATCH`, counted in `refused_batches` |
| Pane state: session id, config options, permission payload, queued prompts | 16 KiB, 256 KiB, 64 KiB, 4 | `FEED_REFUSED_STATE`, counted in `refused_states` |
| Pending permissions, options per permission | 64, 32 | counted in `refused_pending` |
| Restored prompts held, their bytes | 64, 6 MiB | counted in `refused_restores` |
| Chat items, turns, requests | 10,000 each | `CHAT_REFUSED_FULL` (14), counted in `ChatView.refused` |
| Chat text held | 32 MiB | `CHAT_REFUSED_FULL` |

The chat bounds are Mojo only: the source view grows without a limit. A
refused event leaves the view as it was.

## The pane

`AgentPane(pane, client)` owns an `AgentModel`. The integrator calls:

| Call | Meaning |
| --- | --- |
| `descriptor(provider, session_id, model, effort)` | The pane's tree descriptor. Provider 0 is Codex, 1 is Claude Code; any other value raises `unknown agent provider N` |
| `updates(first_seq, items)`, `set_state(state)`, `lagged()`, `disconnect()` | The stream, as above. A replay the feed asks for and a restored prompt's acknowledgement go to the outbox |
| `tick(now_ms)` | The clock the live turn's elapsed time is drawn from |
| `build(ui, theme, key)` | Draws the pane in the current box and applies this frame's clicks |
| `set_visible`, `set_focused`, `layout` | Presentation state; `build` calls `layout` with the box mjui gave `key` |
| `pointer(...)`, `wheel(...)` | True when the point is inside the pane. A vertical wheel stops following the tail |
| `key(event)` | `AGENT_KEY_PASS` (0): not the pane's. `AGENT_KEY_TAKEN` (1): handled. `AGENT_KEY_TEXT` (2): pass the typed text to `text(...)` |
| `text(text)` | Inserts typed or pasted text at the caret, or into the "Other" answer while one is being typed |
| `command(command, option, value)` | `AGENT_COMMAND_SUBMIT` (0), `CANCEL` (1), `UNQUEUE` (2), `SET_OPTION` (3, with the option id and value), `FOLLOW` (4), `DISMISS` (5, cancels the head permission). Returns a `PANE_*` code; an unknown command raises `unsupported agent command N` |
| `take_outbox()` | The effects to send, in order |

Keys the pane takes while focused: Enter sends, queues or answers; Shift-Enter
inserts a newline; Backspace, Delete, Left, Right, Home and End edit the
draft; Up, Down and Escape drive the completion list while it is open; with a
permission or question waiting and an empty composer, Up, Down, Tab, the
digits and Escape drive the card as the source does. Keys with Ctrl, Alt or
Cmd pass through.

The composer completes slash commands the agent advertised and the settings
the stream reported. Sending `/model <value>` or `/effort <value>` becomes a
set-option effect instead of a prompt; `/model` or `/effort` alone leaves the
command in the composer so its choices list.

Effects, one `AgentEffect(kind, text, images, request, option, answers)` each:

| Kind | Daemon message |
| --- | --- |
| `EFFECT_PROMPT` (0) | `AgentPrompt { pane, text, images }` |
| `EFFECT_CANCEL` (1) | `AgentCancel { pane }` |
| `EFFECT_UNQUEUE` (2) | `AgentUnqueue { pane }` |
| `EFFECT_PERMISSION` (3) | `AgentRespondPermission { pane, request_id: request, option_id: option }`; no option cancels the request |
| `EFFECT_ANSWER` (4) | `AgentAnswerQuestion { pane, request_id: request, answers }` |
| `EFFECT_SET_OPTION` (5) | `AgentSetConfigOption { pane, option_id: option, value: text }` |
| `EFFECT_REPLAY` (6) | `AgentReplay { pane, from_seq: request }` |
| `EFFECT_ACKNOWLEDGE` (7) | `AgentAcknowledgePromptRestore { pane, reclaim_id: request }` |

Refusals are `PANE_REFUSED_OFFLINE` (1), `READ_ONLY` (2), `PHASE` (3),
`EMPTY` (4), `IMAGES` (5), `QUEUE` (6), `OUTBOX` (7), `TEXT` (8),
`REQUEST` (9) and `OPTION` (10). The model holds at most 64 pending effects,
a 6 MiB prompt with its images, 64 images, 4 queued prompts and 4,096 bytes
per option id or value. The view remembers at most 64 opened turns and 64
opened runs.

What `build` draws, top to bottom:

- The thread in a scroll area: the empty state (the welcome title and detail,
  or the connection state's message), or one entry per turn. A
  closed turn folds everything before its final answer under "Worked for 3s",
  "You stopped after …" or "Failed after …"; a click opens the fold. A run of
  tool calls is one row, "Ran cargo check", that opens to its steps. Agent
  text streams into a growing text view; a live turn ends with its live line
  or "Thinking" and the elapsed time. Plans, notices, subagents, background
  agents and answered asks draw as their units.
- "Jump to latest" while the thread is not following its tail.
- The question card, or the permission card with one button per option.
- The queued prompt count with Unqueue.
- The error line, attachments ("Image 1 ×" removes one), and the completion
  list.
- The composer: placeholder or the draft with its caret, and the action
  button titled Send, Queue or Stop.
- The settings line (the stream's model and effort summary, else the
  descriptor's) and the context usage.

Widget keys are `sub(key, slot)`: scroll 1, tail 2, action 3, unqueue 4,
composer 5, jump 6, question submit 7, dismiss 8, draft 9, permission options
from 16, completions from 64, attachments from 128, question choices from 256
(question × 64 + option), item text from 4,096, turn folds from 20,000, runs
from 40,000 and live lines from 60,000.

## Differences from the source

- The decoder refuses array-form structs and enums and map-form unit
  variants, which the source decoder accepts; the daemon writes neither.
  Floats are not interpreted, and a number is refused as too large by its
  decimal exponent.
- A tool call with no title shows "Tool approval".
- The thread is a column of the last 48 entries, with "N earlier rows" above
  them, not a virtual list over the whole chat.
- Following resumes on Jump to latest, on sending and on `AGENT_COMMAND_FOLLOW`.
  Scrolling back to the bottom by hand does not resume it.
- Queued prompts show as a count from the pane state, not as rows with their
  text.
- The composer is the pane's own buffer with a caret: no selection, no
  history, no paste of images. Attachments come in through
  `AgentModel.attach`.
- Enter answers a waiting ask only while the draft is blank and nothing is
  attached.

## Not ported

- `agent_transcript.rs`: the legacy timeline transcript. Only its pending
  permission behavior lives in the feed. A sent prompt is not echoed locally;
  it shows when the daemon's chat event arrives.
- `agent_launch.rs`, `new_chat.rs`, `providers.rs` and
  `agent_command_model.rs`: launching, the new chat form, the provider list
  and the zz command model.
- Markdown, code blocks, diagrams, diffs and images in the thread: text draws
  plain and wrapped, images as a count.
- Message actions (copy, fork, time stamp), the session list, steering, stop
  task, set mode and authenticate: the pane sends none of `AgentSteer`,
  `AgentStopTask`, `AgentSetMode`, `AgentAuthenticate` or `AgentSessionOp`.
- A settings popover. Settings change through the composer, as below, or
  through `AGENT_COMMAND_SET_OPTION`.
- Pixel checks against native renders of the source chat view. The pane's
  look is penna's widgets, not a pixel port of `zz_ui::agent::chat`.

## Wiring steps

None of this is done; each step names the file that owns it.

1. `stream_transport.mojo`: decode the three agent events per pane instead of
   leaving them unhandled, into a bounded queue the backend drains:
   `AgentUpdates { pane, first_seq, items }` as the sequence and the item
   bytes, `AgentState { pane, state }` into `AgentPaneState` (phase and
   failure message, `queued_prompts`, `session_id`, `title`, `error`, the
   config options blob, and the pending permission's id and payload with
   `parked` set), and `AgentLagged { pane, next_seq }`.
2. `stream_transport.mojo` or `backend.mojo`: encode the eight messages in the
   effects table, and expose the client instance id the daemon assigned.
3. `view.mojo`, state: hold the agent panes by pane id, at most 64 as the
   browser deck does. After each poll, for every kind 4 pane in the snapshot
   create `AgentPane(pane, client_id)` when new and call
   `descriptor(provider, session_id, model, effort)` from its `PaneKind`;
   drop panes that left the tree.
4. `view.mojo`, events: hand each queued event to its pane with `updates`,
   `set_state` or `lagged`; call `disconnect` on every pane when the
   connection drops. Then drain `take_outbox()` of every pane into the
   messages of step 2.
5. `view.mojo`, drawing: in the pane loop, next to the kind 0 and kind 2
   branches, add `elif kinds.get(item.pane, UInt8(1)) == 4:` and call
   `tick(now_ms)`, `set_visible(True)`, `set_focused(...)` and
   `build(ui, self.theme, key)` with a key unique to the pane. Panes not drawn
   this frame get `set_visible(False)`.
6. `view.mojo` and `workspace_keys.mojo`, input: with an agent pane focused,
   run the `ui` table and the prefix first as for a browser pane, then give
   the press to `AgentPane.key`. On `AGENT_KEY_TEXT` pass the event's text to
   `text`; on `AGENT_KEY_PASS` continue to the daemon. Paste goes to `text`.
   Pointer and wheel events inside the pane's box go to `pointer` and `wheel`;
   mjui's own hit testing delivers the clicks during `build`.
7. `mojo/docs/app-migration.md`: update the agent row.
