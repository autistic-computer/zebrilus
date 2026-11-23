---
type: Design Plan
title: Agent mentions . the composer's @ hands work to another agent
description: "In an agent chat the composer's @ picker lists agents next to chats and files. Picking one puts a mention in the prompt; the agent you are talking to (the host) writes a brief and opens the other agent as a chat with one tmux-shaped command. The child approves its own tool calls unless auto-approve is off; its questions and its final reply come back into the host chat as queued prompts that end with the command to run next. Closing the host closes its children. Built: daemon, @ menu agent rows, child card, peer rows."
status: Built (2026-11-23; daemon side, @ menu agent rows, child card, quiet children kept in their card, peer rows; unverified items listed under Status)
resource: crates/zz-ui/src/target.rs
tags:
- agent
- agent-pane
- mentions
- delegation
- design-plan
timestamp: 2026-11-23T00:00:00-03:00
---

# Summary

In an [agent chat](/concepts/agent-panes.md) the composer's `@` picker lists agents next to chats
and files. Picking one puts a mention in the prompt. The agent you are talking to (the host) writes
a brief and opens the other agent as a chat, which stays as a card in the host chat until it is
clicked, when it shows as a [PiP](/designs/pip-panes.md). The child approves its own tool calls
unless `agent-auto-approve` is `off`; its questions and its final reply come back into the host
chat.

Status 2026-11-23: built. The daemon side (steps 1 to 4, 6 and 7 of the
[build order](#build-order)) and, in the UI lane, the `@` menu's agent rows, the child card and the
peer rows (step 5; see [UI choices](#ui-choices)), checked by the
[stress test](#stress-test-2026-11-17) batches and the
[2026-11-23 batch](#stress-test-2026-11-23). Seen on screen on 2026-11-23 in a dev build: a child
stays hidden until it is selected, shows as a floating pane when selected and hides again when you
leave it; the sidebar's stack tiles take their own panes' colours; the peer sent and reply rows;
clicking the chip focuses the peer. Not verified: expanding a reply row by clicking it; a child's
sounds and banners staying silent; typing `@` in the app for the Agents group and its Tab level; a
real permission ask on the card (`1 allowed`); phones; very long tool output; the first `zz` call
of a real Codex chat outside the rig (the zz rule only applies once Codex starts with it). The
rulings below are the owner's. Everything under
[Proposed commands](#proposed-commands) and after is proposed unless it says ruled, and
[Open](#open) lists what is not decided.

# Rulings (fabrico, 2026-11-17)

1. `@` lists agents (claude, codex). The model is a second pick.
2. `@` can also target an agent chat that is already running.
3. It is delegation, not routing: the host agent writes the brief, zz does not forward the user's
   prompt.
4. The host agent is told about the child's events, and the host answers the child's permission
   prompts. The owner chose this knowing a host can then approve a child's command with no prompt
   shown to the user. (Replaced on 2026-11-23 by ruling 1 there: a child approves its own tool
   calls, and only its questions reach the host.)
5. Closing the host closes its children.
6. The commands stay tmux-shaped, and short enough for an agent that was never taught them.
7. The October removals were not for defects. The agent pane kind was deleted on 2026-10-23
   (`e0608066e`) and came back on 2026-11-01 (`e9dfa5cfa`); agent-to-agent notify and
   `--kind agent` went with it and were not reattached. Those commands are open to rethinking.

# Rulings (fabrico, 2026-11-23)

Made after a real session where a child asked its host for permission 12 times in 4 minutes, each
ask a turn of its own in the host. They replace ruling 4 above (the host decides a child's
permission asks) and the earlier choice that a child starts stashed at the window edge.

1. A child chat always starts full auto: it runs on the `all` auto-approve tier and answers its
   own tool calls. Only its questions reach the host. The global `agent-auto-approve off` still
   wins, and then its permission asks reach the host as before.
2. A child's alerts are noise: no sound and no banner for a child. Its sidebar badge stays.
3. No forced floating pane. A child stays as its card in the host chat and opens only when the
   card is clicked.
4. zz child chats and a vendor's own subagent rows should end up feeling the same; this is the
   first step.
5. A message to a chat this chat did not open is drawn as plain rows: a sent row with a chat chip
   and a reply row, with no fold and no child card (board:
   `design/mockups/peer-message/takes.html`, take 3).

# What exists today

Checked against the tree on 2026-11-17 (main at `2380a891f`).

## The @ picker

- `TargetPicker` (`crates/zz-ui/src/target.rs`) is the composer's `@` menu
  ([command palette](/concepts/command-palette.md#targets-the-lens-composers-mentions),
  [in the composer](/concepts/agent-panes.md#in-the-composer)). Its kinds are `TargetKind` `Host`,
  `Session`, `Window`, `Pane`, `Chat`, `Dir`, `File`.
- The Chats group (`chat_items`) lists every pane the host tree has a chat for, on online hosts,
  the attached host first. The composer's own chat is left out (`TargetPicker::set_own_chat`).
- A pick is tinted text in the plain input. `zz_ui::mention::Mentions` keeps each name's range and
  raw reference, and `TargetPicker::raw_text` puts the raw token back on send, `@<host>:%12` for a
  chat (`TargetItem::reference`). It travels as plain text in `ProtocolMessage::AgentPrompt`.
  Nothing tells the agent what such a token means.
- `parse_target` reads a body that starts with `/` or `~` as a file, splits the rest on `:` into
  host, session and window, and treats a body with no `:` as a filter on the top level.
  `zz_ui::mention::ref_end` ends a token at whitespace or any of `` ` ) ] } < > " ' , `` and trims a
  trailing `. ; : ! ?`.

## What an agent knows

- Environment (`crates/zz-daemon/src/agent/environment.rs`, `entries`): `ZZ_PANE`, `ZZ_SESSION`,
  `ZZ_SOCKET`, `TMUX`, `TMUX_PANE`, `ZZ_TMUX_EXECUTABLE` and `ZZ_TMUX_SHIM_DIR`.
- Context text (`crates/zz-daemon/src/daemon.rs`): every agent gets `WORKSPACE_CONTEXT`
  (1,548 bytes), a chat also gets `AGENT_PANE_CONTEXT` (1,368 bytes). It teaches
  `list-panes -s -F '#{pane_id} #{window_name} #{pane_current_command}'`, `run-pane`,
  `capture-pane -p -J -S -`, `-P -F '#{pane_id}'` and `-d`. Since 2026-11-23 `WORKSPACE_CONTEXT`
  also carries one paragraph naming `zz new-pane -d --kind agent claude|codex|agy|grok|cursor
  [--model M] [--effort E] 'BRIEF'` (`caa8f740d`), for chats and for the session-start hook alike.

## Approval

`runs_only_zz` (`crates/zz-daemon/src/agent/zz_command.rs`) approves a command line that runs only
`zz` or `zz-dev`, with `cd`, `echo`, `printf` and `sleep` allowed as their own steps and `head`,
`tail`, `grep`, `rg`, `wc`, `jq`, `cut` and `tr` as pipe filters, for any verb. The Claude Code and
Codex drivers apply it at auto-approve `reads` (the default) and `all`, not at `off`. `tmux ...`
reaches the daemon through the wrapper on `PATH` but is not approved.

Since 2026-11-23 a chat with a parent runs on `all` unless the global tier is `off`
(`child_auto_approve`, `crates/zz-daemon/src/daemon.rs`); see [Answer](#answer).

## Making a chat

- A PiP is any non-modal float ([PiP panes](/designs/pip-panes.md)). Fork opens a chat in a float
  with `new-pane -t %N -c DIR -x 46% -y 72%`, then `select-pane-kind --resume ... --fork ... agent`
  on it (`fork_chat_command`, `crates/zz-client/src/agent_settings.rs`).
- Agent panes are made only by `select-pane-kind [-t pane] [-c dir] [--model M] [--effort E]
  [--account A] [--resume ID [--fork [--at MSG]]] agent claude|codex` (parser `agent_selection`,
  `crates/zz-mux/src/command.rs`). `new-pane`, `split-window` and `new-window` reject
  `--kind agent` with "invalid pane kind" (`validate_spawn_kind`). No prompt is taken at creation.
- When the pane was a terminal, the chat opens only after that process exits, waiting up to
  `AGENT_CLI_EXIT_GRACE` (5 s), and an `agent-send` to it in that window fails with "pane has
  exited". The mux can create non-terminal kinds directly (`view-pane` does), and the daemon
  handles an agent pane it is handed.

## Talking to a chat

- `agent-send -t %N TEXT` prints the pane id. `--wait` blocks and prints the reply, with
  `--final`, `--json`, `--on-block wait|fail|allow|deny`, `--timeout` (default 600 s) and
  `--progress`. `--notify` requires the caller to be a terminal pane
  (`submit_agent_prompt_and_notify`) and pastes `[zz] Agent %N finished: ...` into it
  (`agent_notify_message`, the reply's last 64 KiB). Its waiter uses `AgentBlockPolicy::Wait`, so
  it says nothing while the child waits on a permission.
- `agent-send` to a terminal running Claude Code or Codex fails with "flip the pane to chat
  first".
- Prompts to a busy chat queue, at most `MAX_AGENT_QUEUED_PROMPTS` (4), and overflow is handed
  back (`AgentTurnFailure::Reclaimed`). `AgentRuntime::prompt_with_waiter`
  (`crates/zz-daemon/src/agent/fanout.rs`) injects a prompt and reports its turn.
- `agent-respond -t %N --allow|--deny|--option ID [REQUEST_ID]` answers a chat's pending
  permission; `inspect --json` shows it as `permission`. A question that wants free text has no
  verb, only the `AgentAnswerQuestion` message.
- The calling pane is known only in the daemon: `ClientHello.origin`, from `ZZ_PANE`, stored on the
  client entry and read as `client.origin`. The mux sees only the `ExecutionContext`. The only
  guard against loops is "the caller is not the target"; there is no depth or cycle limit.

## tmux verbs on a chat pane

- `list-panes` and `display-message` work, but `#{pane_current_command}`, `#{pane_pid}`,
  `#{agent_state}` and `#{agent_pending_permission}` are empty.
- `send-keys`, `paste-buffer`, `capture-pane` and `pipe-pane` fail with "pane has exited: %N".
- `send-text` and `run-pane` fail with "%N is not a terminal pane". `respawn-pane` fails.
- Layout verbs and `kill-pane` work.
- A chat fires no `agent-state-changed`: the hook fires when `@agent_state` changes, and
  `apply_agent_state` only sends `EventPayload::AgentState` to GUI clients.
- No function renders a chat as plain text. The data is in the journal
  (`crates/zz-daemon/src/agent/journal.rs`).

## Loose ends found on the way

- `AgentSteer`, `AgentStopTask`, `AgentSetMode` and `AgentAuthenticate` have no sender.
  `zz-daemon-client` has `agent_set_mode` and `agent_authenticate`, but nothing calls them.
- `agent-catalog` (the model catalog for GUI pickers) has no client.
- `bench/agents/fresh/run-one.sh` still runs `split-window --kind agent --provider`. (Fixed by
  step 2.)
- The `--wait` timeout message tells you to run `agent-send -t %N --wait ''`, which a chat refuses
  ("is a chat; --wait needs text to send"). (Fixed by step 3.)
- `AGENT_PANE_CONTEXT` says this pane approves `zz` commands without asking, which is false at
  auto-approve `off`, and its filter list leaves out `cut` and `tr`. (Fixed by step 7.)
- The `events` help says `agent-state-changed` carries `agent_state` for every pane kind; a chat
  sends none. (Fixed by step 1.)

# Proposed commands

tmux verbs keep their meaning on a chat, and two zz verbs sit on top, the way `run-pane` sits on
top for shells.

| Job | Command | Change |
| --- | --- | --- |
| Find | `list-panes -F '#{pane_id} #{pane_current_command} #{agent_state}'` | built: chat facts in formats, `inspect` and `agent-state-changed` |
| Create | `new-pane\|split-window\|new-window [-d] [-P -F fmt] [-c dir] [-t target] --kind agent claude\|codex [--model M] [--effort E] [--account A] [--resume ID] [BRIEF]` | built: made directly, brief queued, parent recorded; a chat with a parent cannot create one |
| Read | `capture-pane -p -t %N` | built: the last turn as text, `-S -` the whole chat |
| Talk | `agent-send -t %N TEXT` | built: a chat caller gets the reply and the asks as messages with no flag; `--wait ''` joins the running turn |
| Answer | `agent-respond -t %N --allow\|--deny\|--option ID REQUEST_ID` | built: every line of an ask ends with its request id; a child asks only its questions unless auto-approve is `off` |
| Close | `kill-pane -t %N` | built: closing a parent closes its children |
| Type | `send-keys`, `send-text`, `paste-buffer`, `pipe-pane`, `run-pane` | built: an error that names `agent-send` (`pipe-pane`: `capture-pane -p`) |

## Find

`list-panes -F '#{pane_id} #{pane_current_command} #{agent_state}'` works on chats as on
terminals. For a chat, `#{pane_current_command}` is the provider's command name (`claude`,
`codex`). `#{agent_state}` takes the words terminals already write to `@agent_state` and
`agent_state_status` reads (`working`, `blocked`, `needs-approval`, `needs-input`, `failed`,
`done`, `idle`, `starting`). The chat's `AgentConnectionPhase` maps onto them:

| Phase | `#{agent_state}` |
| --- | --- |
| `Starting` | `starting` |
| `Ready` | `idle` |
| `Running` | `working` |
| `AwaitingPermission` | `needs-approval` |
| a pending question | `needs-input` |
| `Failed` | `failed` |

`#{agent_pending_permission}` is filled too. A chat fires `agent-state-changed` with both.
`inspect` also gains provider, model, session id and parent.

## Create

`new-pane|split-window|new-window [-d] [-P -F fmt] [-c dir] [-t target] --kind agent claude|codex
[--model M] [--effort E] [--resume ID] [BRIEF]` reuses the words `select-pane-kind` already parses.
The pane is created as an agent pane directly, so the chat is registered before the command
returns and there is no exit grace. BRIEF is queued as the first prompt. When the calling client's
origin is a chat, that chat is recorded as the parent and the brief's turn reports back to it.
`new-pane` makes the PiP; the other two give a split or a window. `select-pane-kind` stays for the
pickers and the flip.

## Read

`capture-pane -p -t %N` on a chat prints plain text: the last turn by default, the whole chat with
`-S -`, capped at 64 KiB like the notify text. A prompt prints as `> ` and its text, a tool call
as one line (its tool and title), a reply as its text. Built in step 6, from the chat state rather
than the journal (see the step).

## Talk

`agent-send -t %N TEXT`. When the caller is a chat, the reply comes back to it as a message with no
flag, which is what `--notify` does for terminal callers. `--wait` blocks and prints it.
`--wait ''` on a chat waits for the running turn.

Proposed and not ruled: `agent-send` targets chats only, dropping the half that types into
terminals and its `--submit` flag, since `send-keys` and `send-text` type into terminals.

## Answer

`agent-respond`, with the request id. Since the 2026-11-23 rulings a chat with a parent runs on the
`all` tier unless the global `agent-auto-approve` is `off` (`child_auto_approve`: `agent_pane_spec`
reads it whenever the child's runtime opens, and `Shared::set_agent_parent` sets it on a runtime
already open, since a new or reopened child opens before its parent is recorded). So by default a
child answers its own tool permissions and only its questions reach the host. At `off` its
permission asks reach the host too, and the host's own `zz agent-respond ...` then needs the
user's approval, since the approval rule above applies only at `reads` and `all`. A host keeps the
tier it opened with when the global one changes later.

Every `zz agent-respond` line of an ask ends with the ask's request id, so an answer that comes
after the request was answered (in the child's own pane, say) fails with `%N has no pending request
(already answered, or none was asked)` instead of answering a newer request of the same chat.

## Close

`kill-pane`, unchanged. Closing a parent closes its children. Closing a child while its turn is
reporting tells the parent (the closed form below).

## Type

`send-keys`, `send-text`, `paste-buffer`, `pipe-pane` and `run-pane` on a chat fail with an error
that names the verb to use, and not "pane has exited":

```
%4 is an agent chat; send with: zz agent-send -t %4 TEXT
```

# Messages into the host

A reply or an ask arrives in the calling chat as a queued prompt (an ask is a question unless
auto-approve is `off`, see [Answer](#answer)): it runs now if the chat is idle,
else behind what it is doing. Each one ends with the command to run next, so the host needs no
memory of the syntax. An ask's lines end with the id of the request they answer, so a late answer
fails with `has no pending request` rather than answering a newer request of the same chat.

```
[zz] %4 (codex) asks: Bash cargo test -p zz-daemon
zz agent-respond -t %4 --allow 12
zz agent-respond -t %4 --deny 12
```

```
[zz] %4 (codex) finished:
<final text, last 64 KiB>

If this answers the user, answer the user now and send nothing to %4. Only if something is missing: zz agent-send -t %4 'TEXT'
```

The other forms follow `agent_notify_message`:

```
[zz] %4 (codex) stopped (<reason>):
<final text, last 64 KiB>

If this answers the user, answer the user now and send nothing to %4. Only if something is missing: zz agent-send -t %4 'TEXT'
```

```
[zz] %4 (codex) failed: <message>
Tell the user, or try again with: zz agent-send -t %4 'TEXT'
```

```
[zz] %4 (codex) was closed before it replied. Tell the user; do not open it again.
```

The trailing lines came from the second stress test batch; `AgentReport::parse` strips them, so a
client reads only the reply text.

An ask with options lists one `zz agent-respond -t %4 --option ID REQUEST_ID  # LABEL` line per option
instead of allow and deny (the label is a shell comment, so the whole line runs as written).

The injected prompt carries the pane it came from, so clients draw it as a child row and not as a
user message. That is a field on `AgentPrompt` and a wire change.

An ask needs a new waiter policy. Today a `--notify` waiter uses `AgentBlockPolicy::Wait` and says
nothing when the child blocks; the new one sends the ask to the parent and keeps waiting for the
turn. Only turns started by the host's prompt report. A turn the user types into the PiP does not.

# Parent link

A parent pane id on the agent pane, set at creation from the caller's `client.origin`, exposed to
clients (for the host row) and as a format, `#{agent_parent}`. It drives the close cascade. It is a
wire change. Before this the only guard was "the caller is not the target"; there was no depth or
cycle limit, and whether a child may create children is [open](#open).

# Picker and teaching

- **Agents group.** The `@` picker gets an Agents group with a row per provider and its default
  model, and the model as a second pick, as Tab opens a level today. The New chat flow
  (`crates/zz-client/src/new_chat.rs`) has no model list to reuse: it starts the chat on the model
  and effort last picked (`AgentLaunches`, `crates/zz-client/src/agent_launch.rs`), and a chat's
  model picker reads the running chat's config options. The row's default is the last pick from
  `AgentLaunches`, and the second pick lists the daemon's model catalog from `agent-catalog`,
  which gives that verb its first client. The picker asks for every agent's catalog when it opens
  at the top level, so a row with no last pick shows the model the catalog reports as current,
  and both levels show the model's display name. A row stays blank until its catalog answers.
  The catalog's session closes on `SessionReady`, or up to 3 s later when the model option names
  no model yet, which is how Claude Code's applied model gets in.
- **Token.** The pick shows as `@codex` and leaves as `@+codex`, or `@+codex/MODEL` with a
  model. Why `+`: no target the menu writes starts with it (a file starts with `/` or `~`, every
  other target with a host or session name, and in tmux's own target grammar a leading `+` is an
  offset, not a name). With no `:` in it, `parse_target` leaves it at the top level, so it cannot
  be read as a host, session or file; `resolve_target` checks the `+` before session names. `/`
  keeps the model in the same token, and `ref_end` keeps the whole token. A model name with `]` in
  it has to be escaped with `\`, since `]` ends a token. A session literally named `+codex` is
  shadowed in mentions and stays reachable by its `$N` id.
- **Chats.** A Chats pick already exists and keeps its raw token, `@<host>:%N`.
- **Teaching.** The agent has to be told what the two tokens mean. Where it goes is open: a few
  lines in the standing context, or a short note added only to a prompt that has a mention. Built
  as the note; after the [stress test](#stress-test-2026-11-17) it is an instruction built from
  the tokens in the prompt, one sentence per token. Since 2026-11-23 only a prompt the user wrote
  gets it (see step 4), and the standing context also names `new-pane` in one paragraph (see
  [What an agent knows](#what-an-agent-knows)), so an untaught agent can hand work over too:

```
The user tagged codex: codex does this task, not you. Write it a brief that stands on its own, run zz new-pane -d --kind agent codex 'BRIEF' and end your turn. Do not do the task yourself. Its reply and any permission asks come back to you as messages.
@macbook:%2 is another chat, not you: run zz agent-send -t %2 'TEXT' and end your turn. Its reply comes back to you as a message.
```

# Host chat

One row per child in the host transcript: agent, state (working, asking, done, failed, closed),
elapsed time, and what the host allowed or denied. Clicking it brings the child into view as a
PiP. The reply shows as a quoted block. The UI gets boarded in `design/mockups` before any app
code.

A chat this chat messages but did not open (a peer) gets no card: plain rows, a sent row and a
reply row (ruling 5 of 2026-11-23, see [UI choices](#ui-choices)).

# How the host card reads the transcript

The host chat's card is drawn from the host's own transcript, with no new wire data. Every string
it reads has one definition, `zz_protocol::agent_mention`, which the daemon formats with and a
client parses with:

- The tool call that opened or messaged a child: an execute item whose command runs
  `new-pane|split-window|new-window ... --kind agent` or `agent-send`. Its output holds the stderr
  line `ReplyNotice` parses, which names the child: `new-pane: opened %N (codex)`, alone or
  followed by `; its reply comes back to you as a message; end your turn now` or `; read its
  reply with: zz agent-send -t %N --wait ''`, or `agent-send: %N's reply comes back to you as a
  message; end your turn now` or
  `agent-send: read %N's reply with: zz agent-send -t %N --wait ''`.
- The injected prompts: a user message that `AgentReport::parse` reads as finished, stopped,
  failed or closed, or that `AgentAsk::parse` reads as an ask with its answers.
- The host's own answers: an execute item whose command `AgentRespond::parse` reads as
  `zz agent-respond -t %N --allow|--deny|--option ID|--answer TEXT [REQUEST_ID]`, with or
  without the leading `zz` and inside a shell wrapper.

A pane is a child only when this chat's own timeline holds the `opened %N` line for it, which
also keeps a closed child's card in history. Every other pane the chat sends to or hears from is a
peer (`zz_client::peer`), drawn as rows, not a card.

For both drivers the command text is the tool call's `rawInput.command` (in a chat item,
`ItemBody.target` unless the driver set its own target), and the output, stdout and stderr
together, is the tool call update's `content` text (`ItemBody.text`).

# Build order

Each step lands alone.

1. **Chat facts and input errors.** Built 2026-11-17. Formats, `inspect` and
   `agent-state-changed` for chats; the new error text for the input verbs.
   - On an idle codex chat `%4`, `zz list-panes -F '#{pane_id} #{pane_current_command}
     #{agent_state}' -f '#{==:#{pane_id},%4}'` prints `%4 codex idle`.
   - `zz send-keys -t %4 x` fails with exactly `%4 is an agent chat; send with: zz agent-send -t %4
     TEXT`.
   Choices made while building it:
   - `pipe-pane` on a chat fails with `%4 is an agent chat; read it with: zz capture-pane -p -t
     %4`, not the `agent-send` text the [Type](#type) section gives. `capture-pane` on a chat
     still fails with "pane has exited" until step 6, and the agent guide leaves `pipe-pane` out
     until then.
   - A pending permission or question decides the state before the phase does, so a permission
     that outlives its turn reads `needs-approval` on a `Ready` chat. `Failed` and `Starting` win
     over a pending request.
   - The daemon keeps each chat's state and pending id on the engine (`MuxEngine::set_agent_chat`,
     not on the wire) when the runtime publishes a state, and fires `agent-state-changed` only when
     either value changes. A terminal fires it on every `@agent_state` write.
   - `#{pane_current_command}` is the provider's command name even while the terminal a chat was
     flipped from is still exiting.
   - `inspect` names the new keys `agent_provider` (`claude` or `codex`, the words the create verbs
     take), `agent_model` (the model the chat was started with, empty for the default) and
     `agent_session_id`.
   - `send-keys -R` (reset) on a chat fails with the `agent-send` text too, as does any verb that
     reaches the send-keys path (`send-prefix`).
   - A `synchronize-panes` send from a terminal whose window holds a chat still fails with "pane has
     exited", as before.
2. **`--kind agent` on the create verbs, with the brief.** Built 2026-11-17. Fix
   `bench/agents/fresh/run-one.sh`.
   - `zz new-pane -d -P -F '#{pane_id}' --kind agent codex hi` prints a pane id, and an immediate
     `zz agent-send -t <that id> again` exits 0.
   - `rg -- '--provider' bench/agents/fresh/run-one.sh` finds nothing.

   Choices made while building it:
   - `--model`, `--effort`, `--account` and `--resume` are flags of the three verbs (native, so
     they exit 2 without `--kind agent`) and are also read right after the provider, before the
     brief; the same flag in both places exits 2. `--fork` and `--at` are not taken: a fork opens
     its runtime on a thread after copying the session, so the brief could not be queued in the
     command.
   - Every parse failure of the agent words (no provider, unknown provider, a flag with no value
     or a bad value, a bad `-c`) exits 2 with the cause and a usage line. A brief that fails the
     `agent-send` text checks exits 1 with `agent-send`'s message, under the verb's name.
     `check_plain_text` moved to zz-mux so the check runs before the pane exists.
   - `-c` is format-expanded like a terminal's and must then be absolute. Without `-c` the folder
     comes from the donor a terminal would use (for `new-window`, the caller's pane or the
     destination's active pane).
   - The brief reaches the daemon as `MuxEffect::AgentBrief` (daemon-internal, no wire change) and
     is queued with `AgentRuntime::prompt`, owner `ClientInstanceId(u64::MAX)`, as `agent-send`
     does, right after the runtime opens. A handed-back brief is logged, not returned as an error.
   - The fork float's size moved to `zz_protocol::AGENT_FLOAT_WIDTH`/`AGENT_FLOAT_HEIGHT`, shared
     by `fork_chat_command` and `new-pane --kind agent`.
3. **Parent link, replies and asks into chats, close cascade.** Built 2026-11-17, with no wire
   change. Also `--wait ''` on a chat.
   - Run from chat `%1`, `zz new-pane -d -P -F '#{pane_id}' --kind agent codex 'reply with ok'`
     prints `%2`; `zz display-message -p -t %2 '#{agent_parent}'` prints `%1`; the next prompt `%1`
     receives starts with `[zz] %2 (codex) finished:`.
   - After `zz kill-pane -t %1`, `zz list-panes -a -F '#{pane_id}'` lists neither `%1` nor `%2`.

   Choices made while building it:
   - The parent lives daemon-side on the engine (`MuxEngine::agent_parents`), not on the wire.
     The "pane it came from" field on the injected prompt and anything a client needs to draw
     the host row are still to do, in the UI step.
   - A chat caller is only `client.origin` being a chat other than the target. The interactive
     fallback `--notify` uses (the focused pane of a GUI client) does not count.
   - The parent is recorded for any chat a create verb makes from a chat caller, brief or not.
     A chat with a parent cannot open a chat (exit 1), so there is one level of children.
   - Reports and asks are queued with `AgentRuntime::inject`: such a prompt goes through the
     pane's inbox, is not counted against the 4 queued prompts, keeps the byte cap (over it, it is
     logged and dropped), and is never handed back to a composer when a queue is reclaimed.
     Deliveries to one caller leave one queue in order, drained by one thread at a time
     (`Shared::deliver_to_chat`). An ask whose request is answered before its turn is dropped:
     `deliver_one` checks before it injects, and the injected prompt carries the request and a
     weak handle to the asking chat's state (`RelayedAsk`), so the host checks again when the ask
     reaches the front of its queue (`PanePump::dispatch_next`) and skips it if the request is no
     longer pending. Reports are never dropped.
   - A caller flipped to a terminal gets the message through the existing terminal delivery
     (`deliver_to_terminal_pane`), as `--notify` does. A caller that is gone or is neither a chat
     nor a terminal is logged and dropped.
   - Message cases the shapes did not cover: a cancelled turn reports `[zz] %N (NAME) stopped
     (cancelled):` (the stopped shape with no text; it read `stopped (cancelled).` until the
     formats moved into `zz_protocol::agent_mention`), a prompt handed back before its turn reports `failed: the message was handed
     back before its turn ran`, and a reply that never comes (the chat closed) reports `was
     closed.`. An ask whose tool call has no kind leaves the kind out: `asks: <title>`.
   - Asks ride the waiter as an `asks` sink beside `AgentBlockPolicy::Wait` (the waiter keeps
     waiting), called once per request id. They come only from turns that report to a caller;
     a turn the user types into the child does not ask the host. Since 2026-11-23 a child on the
     `all` tier (the default, see [Answer](#answer)) raises no permission request, so its asks
     are questions only.
   - A plain `agent-send` from a chat caller to a terminal running an agent stays silent, as for
     terminal callers. The default `--on-block fail` for chat callers applies to every `--wait`
     they run, a terminal target included.
   - `--wait ''` joins the running turn as an extra waiter (`HostCommand::Join`); the host now
     records reply text for every turn so a late joiner gets the whole reply. A joined waiter
     takes `--on-block wait` or `fail`.
   - The cascade kills the children on a thread right after the command that removed the parent,
     so `kill-pane` returns before they are gone.
4. **Agents rows in the picker and the teaching text.** The teaching text (the mention note) is
   built; the picker rows are built in the UI lane.

   Choices made while building the note:
   - The note goes with a prompt the user wrote that holds an agent token or a chat token, as a
     separate block or item, never inside the user's text. Since 2026-11-23 (`cda6b9adb`) the
     host builds it when the turn starts, from the prompt's source: a prompt from the composer,
     or from a command run by a client that is not a chat, gets it; a chat's `agent-send` or
     brief (`PromptSource::Chat`) gets only the child note a brief carries, and the reports and
     asks zz injects get none. Before that every prompt with a token got it, so a tag quoted in
     another chat's message or in a child's reply drew a note the user never wrote. Claude Code
     gets it wrapped in `<system-reminder>`, Codex as a second `input` text item, ACP agents as a second
     prompt block; replay drops that exact block, so the user's bubble shows only what they typed.
   - Detection is by token shape alone (`zz_protocol::agent_mention::mentions`, the same grammar
     the composer writes): `@+claude` and `@+codex` with an optional `/MODEL` (`\` escapes), and
     `@HOST:%N` with nothing between the host and the pane, which only the Chats rows write. The
     daemon does not check that `%N` is a chat, since the host may be another machine.
   - The note reads: `@+claude or @+codex[/MODEL] asks you to delegate: write a brief that stands
     on its own and run zz new-pane -d --kind agent claude|codex [--model MODEL] 'BRIEF'.
     @host:%N is a running chat: zz agent-send -t %N TEXT. Replies and permission asks come back
     to you as messages; do not wait or poll.` (293 bytes; `claude|codex` replaces the design's
     `codex` so the provider the token names is the one opened.)
   - `REVISION` is 6: clients can rely on a daemon at 6 or later to add the note, report child
     turns and keep the message formats. No floor moved.

   - Picking the codex row in a composer holding `fix it` after the mention, `TargetPicker::raw_text`
     returns `@+codex fix it`.
   - The teaching text is at most 300 bytes.
5. **Host row**, after the design board.
   - While `%2` waits on a permission, `%1`'s transcript has one row for `%2` reading `codex ·
     asking`.
6. **`capture-pane` on a chat.** Built 2026-11-17.
   - After a turn whose prompt is `reply with ok` and whose reply is `ok`, `zz capture-pane -p -t %2`
     prints exactly `> reply with ok\n\nok\n` (a blank line follows the prompt).

   Choices made while building it:
   - Source: the chat state the daemon keeps per pane for every client (`PaneLane::chat`, the
     `ChatLog` a client's replay recovers from), not the journal. The Claude Code and Codex
     drivers fill it, it is complete, it needs no client, and a restored chat refills it when the
     driver replays its session. The journal holds raw updates only for chats on other ACP agents,
     and such a chat has no chat state, so it is read from the pane's retained stream, which keeps
     only the recent part.
   - Shapes: `> ` and the prompt, later prompt lines indented two spaces; a blank line after the
     prompt and between turns; one line per tool call with the kind word (`read`, `edit`,
     `search`, `execute`, `fetch`, `web`, `mcp`, `tool`, `subagent`) and the title, whitespace
     collapsed; the reply's text. A turn is a prompt and what follows it up to the next prompt.
     Reasoning, plans, notices, errors and the steps of subagents are left out. The cap keeps the
     last 64 KiB.
   - Flags: any `-S` value prints the whole chat; `-E`, `-J`, `-e`, `-N`, `-T`, `-a`, `-q`, `-C`,
     `-H`, `-F`, `-L` and `-M` change nothing. Without `-p` the text goes into a paste buffer
     (`-b` names it), the same path a terminal capture takes.
   - `inspect` lists `capture-pane` among an agent pane's verbs.
7. **Loose ends** listed [above](#loose-ends-found-on-the-way). Built 2026-11-17.
   - The `--wait` timeout message names a command a chat accepts.
   - `AGENT_PANE_CONTEXT` lists `cut` and `tr` and says the rule does not apply at `off`.

   The timeout message is pinned in a test that also runs the `--wait ''` it recommends. The
   `events` help now says when `agent-state-changed` fires for a terminal and for a chat. The
   Agents section of the tools guide was checked against the code; four protocol messages with
   no sender stay under [Open](#open).

   After the steps, a pass over what each verb tells an agent (for a stress test where an agent
   gets no teaching beyond zz's own output), built 2026-11-17:
   - Every create with `--kind agent` prints `<verb>: opened %N (NAME)` on stderr, with `; its
     reply comes back to you as a message` (chat caller with a brief) or `; read its reply with:
     zz agent-send -t %N --wait ''` (any other caller with a brief).
   - A plain `agent-send` from a caller that is not a chat prints on stderr `agent-send: read
     %N's reply with: zz agent-send -t %N --wait ''`. `--notify` from a chat is a plain send.
   - `%N has no running turn; read its last reply with: zz capture-pane -p -t %N`.
   - `wait-pane` on a chat: `%N is an agent chat; wait for its turn with: zz agent-send -t %N
     --wait ''`; `show-last-output` (and `send-last-output`): `%N is an agent chat; read it
     with: zz capture-pane -p -t %N`.
   - `agent-respond` with nothing pending: `%N has no pending request (already answered, or none
     was asked)`. Since 2026-11-23 a request id that is no longer pending gives the same error (it
     said `unknown permission request`).
   - The grandchild error ends `. Do the work yourself, or say in your reply what you need`.
   - A send to a chat that already holds 4 waiting prompts fails at once, exit 1: `%N has 4
     prompts waiting; wait for its turn with: zz agent-send -t %N --wait ''`. Prompts queued for
     `agent-send` and briefs are `PromptSource::Command` and never go back to a composer; a
     race past the check is logged and, for a chat caller, reported as `failed:`.
   - `capture-pane` during a turn adds `capture-pane: %N's turn is still running` on stderr.
   - `agent-respond --answer TEXT` is built: it answers a request that asks exactly one question
     through the host path the GUI's `AgentAnswerQuestion` uses, and prints `answered request
     ID`. The free-text ask now reads `zz agent-respond -t %N --answer 'TEXT'` and then the
     dismiss line. A request with several questions is refused with `request N asks K
     questions; --answer answers one`.
   - A prompt the chat's host does not take (its agent is not running) fails with `%N's agent is
     not running (agent_state STATE); the prompt was not sent` instead of "pane has exited".
   - Left as they were: `respawn-pane` on a chat, and a `synchronize-panes` send whose window
     holds a chat.

# Stress test, 2026-11-17

60 scored runs with weak models (Haiku 5.5 and gpt-6-luna) as hosts and children, 3 per scenario
and host, against `target/debug/zz_cli` at `08ff588de`; results and command sequences in
`bench/agents/mentions/results/2026-11-17.md`.

| scenario | claude host | codex host |
| --- | --- | --- |
| m1 spawn | 2/3 | 2/3 |
| m3 open chat | 3/3 | 2/3 |
| m4 allow | 0/3 | 3/3 |
| m4b no run | 3/3 | 0/3 |
| m4c deny | 0/3 | 0/3 |
| m5 child dies | 0/3 | 1/3 |
| m6 untaught | 3/3 | 3/3 |
| m7 restraint | 3/3 | 3/3 |
| m8 child restraint | 1/3 | 1/3 |
| m8b child pushed | 3/3 | 2/3 |

No host polled, hung or respawned a child, and the guards held whenever a child hit them. What
changed because of it:

- **The note is an instruction.** Hosts did the task themselves in 10 of 42 mention runs, and one
  read `@macbook:%2 ...` as a question to itself. The note is now built from the prompt's tokens
  with the command filled in and says to end the turn (see [Picker and teaching](#picker-and-teaching)).
- **"End your turn now".** Codex hosts delegated and then did the work in the same turn (7 of 17).
  For a chat caller the stderr lines now end `; end your turn now`.
- **A note for the child.** It travels with the brief the same hidden way and says who opened the
  chat, that the final message is the reply, and that it cannot open agents or message the host.
  A Codex child also learns to rerun an `Operation not permitted` command with escalated
  permissions: Codex children at the default level never asked, so no ask reached the host. That
  is the right fix rather than a driver change: the driver already turns Codex's escalation
  request into an ask (`item/commandExecution/requestApproval`, answered by zz unless the command
  is only `zz`), but only the model can request escalation, and it does so only when told to;
  with `approval_policy = never` in the user's Codex config no escalation can be requested at all.
- **Codex runs `zz` outside its sandbox.** Every Codex host's first `zz` failed in the sandbox (22
  of 22). The driver now writes the `zz` prefix rule `zz tools --install-hooks` writes into the
  chat's Codex home when it is missing, before Codex starts.
- **Panes a child creates close with it.** A Codex child opened a terminal with `split-window` and
  used `run-pane` in it; that terminal outlived the host. Every pane a chat with a parent creates
  now gets that chat as its parent, so the close cascade takes it.
- **Option lines run as written.** A question ask's `--option ID  LABEL` line was rejected
  (`invalid permission request ID: LABEL`); the label is now a `# LABEL` comment.
- **Repeated tool ids.** The chat state rejected a tool call that started again under an id it had
  already ended, as a stale item, so the second run vanished and the item kept the first run's
  status. That fits the escalated `agent-respond` that went missing after a sandbox denial (the
  run kept no Codex log, so the reused id is inferred, not seen). A tool call that starts again
  after it ended is now a second item (`ID~2`), which `capture-pane` and the host card both see;
  the state never merged two calls into one item.
- **Errors in `capture-pane`.** An error item now prints as `error: TEXT`, so a turn that failed
  shows why.
- Not found: one Codex host stayed silent after `[zz] %2 (claude) was closed.` arrived 2 s before
  its first turn ended (`dbS5eR`). The prompt was dispatched (its echo is in the transcript), so the
  Codex turn either failed or ended with no message; the run kept no Codex session log, and
  `capture-pane` did not print errors then. The missing `cd ... && ./stamp.sh` of one Codex child
  (`0dJB0r`) was not explained either.

## Second batch

60 runs against `d3cade423` (the fixes above); results in
`bench/agents/mentions/results/2026-11-17-b.md`. 46 of 60 passed, against 35 of 60.

| scenario | host | pass before | pass after | median calls before | after | median failed before | after |
| --- | --- | --- | --- | --- | --- | --- | --- |
| m1 spawn | claude | 2/3 | 3/3 | 1 | 1 | 0 | 0 |
| m1 spawn | codex | 2/3 | 3/3 | 2 | 1 | 1 | 0 |
| m3 open chat | claude | 3/3 | 3/3 | 1 | 1 | 0 | 0 |
| m3 open chat | codex | 2/3 | 3/3 | 2 | 1 | 1 | 0 |
| m4 allow | claude | 0/3 | 1/3 | 1 | 1 | 0 | 0 |
| m4 allow | codex | 3/3 | 3/3 | 4 | 2 | 2 | 0 |
| m4b no run | claude | 3/3 | 3/3 | 1 | 1 | 0 | 0 |
| m4b no run | codex | 0/3 | 3/3 | 2 | 1 | 1 | 0 |
| m4c deny | claude | 0/3 | 0/3 | 2 | 3 | 0 | 0 |
| m4c deny | codex | 0/3 | 1/3 | 4 | 3 | 2 | 0 |
| m5 child dies | claude | 0/3 | 1/3 | 0 | 2 | 0 | 0 |
| m5 child dies | codex | 1/3 | 1/3 | 2 | 2 | 1 | 0 |
| m6 untaught | claude | 3/3 | 3/3 | 5 | 4 | 1 | 1 |
| m6 untaught | codex | 3/3 | 3/3 | 5 | 3 | 2 | 0 |
| m7 restraint | claude | 3/3 | 3/3 | 0 | 0 | 0 | 0 |
| m7 restraint | codex | 3/3 | 3/3 | 0 | 0 | 0 | 0 |
| m8 child restraint | claude | 1/3 | 3/3 | 0 | 1 | 0 | 0 |
| m8 child restraint | codex | 1/3 | 3/3 | 2 | 1 | 1 | 0 |
| m8b child pushed | claude | 3/3 | 1/3 | 4 | 4 | 1 | 1 |
| m8b child pushed | codex | 2/3 | 2/3 | 2 | 3 | 1 | 1 |

Checks that now pass:

- A Codex host's first `zz` runs with no sandbox failure: 0 sandboxed calls in 30 Codex-host
  runs (before: 22 of 22).
- Hosts delegate on the one-line spawn prompt: 18 of 18 (before: 10 of 18).
- Codex hosts no longer also do the work: 0 of 21 mention runs (before: 7 of 17).
- A terminal a child opened with `split-window` closes with the host; nothing is left.
- No empty answer after a `was closed.` report that lands while the host's first turn runs: 0
  of 6.
- Partly: a Codex child's refused write reaches the host as an ask in 3 of 6 Claude-host runs.

Changed after it:

- Hosts opened a fresh chat after `was closed.` (4 of 6 m5 runs) or after a child answered without
  the result, where `agent-send` to the open chat was the cheaper move. `was closed.` now reads
  `was closed before it replied. Tell the user; do not open it again.`, `finished:` and
  `stopped (...):` end with a blank line and `Follow up with: zz agent-send -t %N 'TEXT'`, and
  `failed:` with `Tell the user, or try again with: zz agent-send -t %N 'TEXT'`.
- Codex children followed a brief that said to stop if they needed permission and reported the
  sandbox failure (3 of 6). The child note's Codex sentence now says to rerun with escalated
  permissions `even if your brief says to stop on failure`.

Left as it is:

- A Codex child at the default level runs a command once inside its read-only sandbox before any
  ask exists: the ask only appears after the sandboxed attempt fails, so in m4c the script ran
  once (read-only) before the host could deny it. When the blocked write shows up in the
  script's own output (`./stamp.sh: line 4: ./stamped.txt: Operation not permitted`), the child
  reports that error instead of escalating, even with the note. Both are how Codex chats behave
  at that level, not something specific to mentions; changing them would take starting child
  Codex chats with an approval policy that asks before every command that is not a read, or the
  driver raising an ask itself when Codex reports a sandbox denial.
- Untaught hosts (no mention, so no note) chain `sleep` onto `agent-send` (m6, 2 of 3 Claude
  hosts). They reach `agent-send` in one failed hop, and the reply comes back anyway.
- A host that follows the note reports to the user when the child refuses (m8b, 2 runs) instead
  of reading the file itself: that is what the note asks for ("not you"), so it fails the token
  rule without being wrong.

## Third batch

26 targeted runs against `2f1048253`; results in `bench/agents/mentions/results/2026-11-17-c.md`.
After `was closed` no host opened a new chat (6 of 6), and after a reply that lacked the result
every host followed up on the same chat (12 of 12). But the follow-up line started a loop: in 2 of
3 Codex-host m4c runs the host thanked the child with `agent-send`, the child's polite reply came
back ending in the same line, and the host thanked it again; one run exchanged 24 messages over
139 s before it stopped by itself.

Changed after it:

- The line after a finished or stopped report's text is now `If this answers the user, answer the
  user now and send nothing to %N. Only if something is missing: zz agent-send -t %N 'TEXT'`. The
  failed and closed forms stay.
- A hard stop that does not depend on wording: a chat caller sends at most 8 prompts
  (`MAX_SENDS_SINCE_USER`, decided) to one chat, its brief counting as the first, until it
  receives a prompt the daemon did not inject (from the user or any client). The ninth fails, exit
  1, with `%N has had 8 messages from you since the user last wrote; answer the user now`, and
  nothing is sent. Sends from terminals are not counted.

# Stress test, 2026-11-23

28 runs against `f7a85787c` (the 2026-11-23 rulings, request ids, dropped stale asks, notes only
for prompts the user wrote, the `new-pane` paragraph): every scenario once per host, then two more
runs of each failing cell; results in `bench/agents/mentions/results/2026-11-23.md`. m4 now
checks the default (the child approves itself: stamp.sh ran, no ask reached the host, no
`agent-respond`); the new m4a and m4c run with `agent-auto-approve off` and check that each answer
names its ask's request id.

| scenario | claude host | codex host |
| --- | --- | --- |
| m1 spawn | 1/1 | 1/1 |
| m3 open chat | 1/1 | 1/1 |
| m4 child approves itself | 1/1 | 1/1 |
| m4a allow under off | 1/1 | 1/1 |
| m4b no run | 1/1 | 1/1 |
| m4c deny under off | 0/3 | 1/1 |
| m5 child dies | 1/1 | 1/1 |
| m6 untaught | 1/1 | 1/1 |
| m7 restraint | 1/1 | 1/1 |
| m8 child restraint | 1/1 | 1/1 |
| m8b child pushed | 2/3 | 1/3 |

- m4 passes with 2 host turns and no ask, where an m4 pass took 3 before. With a Claude host the
  Codex child reran `./stamp.sh` escalated after its sandbox blocked the write, and the escalation
  was approved for it; that cell passed 1 of 6 runs in the earlier batches.
- Under `off` every answer named its request: `zz agent-respond -t %2 --allow 1`, `--deny 1`.
- The two failures are the ones the second batch left as they are: a Codex child runs a command
  once in its read-only sandbox before any ask exists (m4c), and a host tells the user that the
  child refused instead of following up (m8b).
- The mention note for an agent token still ends `Its reply and any permission asks come back to
  you as messages.`, which is no longer true by default. No run tripped on it.

# UI choices

Recorded for the UI lane, which built them in `crates/zz-ui` and `crates/zz`:

- **`@` menu.** Agents is the first group on a bare `@`; typed text ranks agents in the flat
  list. The row's dim model is the last pick, else the catalog's current model, else `default`.
  Tab opens the provider's models from `agent-catalog`, its first client; a typed `codex/` or
  `+codex/` opens the models level. Agent picks are not kept as recents. In a transcript
  `@+codex` shows as `@codex`, tinted and not clickable.
- **Child card.** Built from the host's own transcript with no wire change. A pane is a child only
  when the timeline holds its `opened %N` line. An exchange starts at a tool call whose output
  carries the opened or sent line for a child and absorbs the asks, reports and the host's
  `agent-respond` calls for that pane; a second send to the same child gets its own card. A
  decision counts only when its call succeeded; a failed call followed by the same call succeeding is hidden with it; the
  header counts every decision (allowed in green). The latest card reads the pending request and
  closed state from the child's own chat. The peek shows the tool steps of the child's last turn;
  the reply is drawn as markdown; a closed card shows no time; a denial shows as `denied by NAME`.
- **Peer rows** (2026-11-23, board `design/mockups/peer-message/takes.html`, take 3). An
  `agent-send` to a pane this chat did not open stays a tool row with its own title and names the
  peer with a chip: the peer's tile at 16 with its status dot, working until the reply arrives,
  and the chat's title, or `%N` once the pane is gone; the chip takes the place of `%N` in the
  title, or follows it, and clicking it focuses the peer through the pane link path. The finished
  report is one row: a reply arrow, "Reply from", the chip, the time since the send and the
  reply's first line; clicking the row opens the whole reply under it with a rule. Stopped,
  failed and closed reports and relayed asks use the same row. Before, such a send folded into a
  child card whose click ran `select-child-chat` and failed with `%12 is not a chat %6 opened`
  (`f7a85787c`; `zz_client::peer`, `crates/zz-ui/src/agent/chat.rs`).
- **PiP.** On the desktop a child is not drawn as a float until it is the active pane, which the
  card's click makes it; stashing or throwing it hides it again with no strip
  ([PiP panes](/designs/pip-panes.md#piles-and-stashing)). The card collapses to its header while
  the child is in view.
- **Quiet.** A child plays no sound and posts no banner when it finishes, fails or asks; the
  desktop learns it is a child from `AgentParents` and drops its edges in
  `zz_client::attention::announced`. Its sidebar badge stays.

# Reopening a child

A child is its session, not its pane (fabrico, 2026-11-17). The X on its PiP keeps killing the
pane like every PiP's X; nothing asks first. Clicking the card runs `select-child-chat -t HOST
%CHILD`, which brings the child into view and prints its pane: the pane that holds it now, or a new
float in the host's window resumed on its session with the same provider, folder, model, effort and
account, parented to the host. The daemon keeps one record per child under the host's session, next
to the host's journal, so this works after a restart and after the host is resumed in another pane,
and the records go when the host's session is deleted. A child closed before its session started
cannot come back: `%N was closed before its session started; it cannot be opened again`. Once
reopened, notices and reports name the new pane, so the host's transcript gets a card for each id
the child has had. The record keeps every one of them (the original and each reopened pane), and
`select-child-chat` and the host's `agent-send -t %N` accept any of them: the send reaches the pane
that holds the child now and counts toward one set of 8 messages per child, and while the child is
closed it fails with `%N was closed; open a new chat
with: zz new-pane -d --kind agent NAME 'BRIEF'`. See
[Agent panes](/concepts/agent-panes.md#agent-panes) for the record and lookup order.

The card keeps the pane the verb printed in the chat view's state (`ChatOpen::children`), so a
reload or restart forgets it and the next click asks the daemon again. Its current pane is the
original one if alive, else that remembered one if alive, else none; the peek, the live state and
the in-view collapse read it. With no current pane a card keeps the state its report gave (done,
stopped, failed); a card closed by its report, or the latest card with no report, shows closed. Every
card keeps its PiP mark and stays clickable, and only a click the daemon refuses shows a message,
as the command's error toast. The desktop reads the printed pane from the command's response
(`MuxClient::execute_printed`), the thin client from its connection's command response.

# Open

- Does the child get anything beyond the brief (the host's folder, files it touched)? Today it
  gets the brief and starts in the folder a terminal would.
- Trimming `agent-send` to chats.

Settled along the way: `send-keys` on a chat only points to `agent-send` (step 1); a child cannot
create children (step 3); replies and asks do not count against the caller's queue of 4 (step 3);
the teaching travels as a note with the prompt the user wrote that has the mention (step 4), and
the standing context carries only the one `new-pane` paragraph; a question that wants typed text
is answered with `agent-respond --answer`; `tmux ...` commands are not approved like `zz ...` ones, since no host in the stress test needed them.
- For each message with no sender (`AgentSteer`, `AgentStopTask`, `AgentSetMode`,
  `AgentAuthenticate`): give it a verb or remove it.

# Not doing

- No MCP server and no injected tools: the host runs `zz` from its shell. The backbone plan's "Do
  not add an MCP server" stands ([agent backbone v3](/designs/agent-backbone-v3.md)).
- No cancel or steer verbs.
- No agent graph or orchestration UI beyond the host row.
- No daemon-spawned watch floats (the subagent feeds removed on 2026-10-23,
  [PiP panes](/designs/pip-panes.md#subagent-feeds-removed-2026-10-23)).

# Related

- [Agent panes and the flip](/concepts/agent-panes.md): chats, `agent-send`, `agent-respond` and
  the composer's mentions.
- [PiP panes](/designs/pip-panes.md): where a child chat opens.
- [Native agent drivers](/designs/native-agent-drivers.md): the drivers that run the chats and
  apply the approval rule.
- [Agent backbone v3](/designs/agent-backbone-v3.md): the zz CLI as the agent's tool surface.
- [Command palette](/concepts/command-palette.md#targets-the-lens-composers-mentions): the target
  grammar the `@` menu shares.
