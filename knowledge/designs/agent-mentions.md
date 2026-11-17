---
type: Design Plan
title: Agent mentions . the composer's @ hands work to another agent
description: "In an agent chat the composer's @ picker lists agents next to chats and files. Picking one puts a mention in the prompt; the agent you are talking to (the host) writes a brief and opens the other agent as a chat in a PiP with one tmux-shaped command. The child's permission asks and its final reply come back into the host chat as queued prompts that end with the command to run next. Closing the host closes its children. Proposed, nothing built."
status: Proposed (2026-11-17; nothing built)
resource: crates/zz-ui/src/target.rs
tags:
- agent
- agent-pane
- mentions
- delegation
- design-plan
timestamp: 2026-11-17T00:00:00-03:00
---

# Summary

In an [agent chat](/concepts/agent-panes.md) the composer's `@` picker lists agents next to chats
and files. Picking one puts a mention in the prompt. The agent you are talking to (the host) writes
a brief and opens the other agent as a chat in a [PiP](/designs/pip-panes.md). The child's
permission asks and its final reply come back into the host chat.

Status 2026-11-17: proposed, nothing built. The rulings below are the owner's. Everything under
[Proposed commands](#proposed-commands) and after is proposed unless it says ruled, and
[Open](#open) lists what is not decided.

# Rulings (fabrico, 2026-11-17)

1. `@` lists agents (claude, codex). The model is a second pick.
2. `@` can also target an agent chat that is already running.
3. It is delegation, not routing: the host agent writes the brief, zz does not forward the user's
   prompt.
4. The host agent is told about the child's events, and the host answers the child's permission
   prompts. The owner chose this knowing a host can then approve a child's command with no prompt
   shown to the user.
5. Closing the host closes its children.
6. The commands stay tmux-shaped, and short enough for an agent that was never taught them.
7. The October removals were not for defects. The agent pane kind was deleted on 2026-10-23
   (`e0608066e`) and came back on 2026-11-01 (`e9dfa5cfa`); agent-to-agent notify and
   `--kind agent` went with it and were not reattached. Those commands are open to rethinking.

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
  `capture-pane -p -J -S -`, `-P -F '#{pane_id}'` and `-d`.

## Approval

`runs_only_zz` (`crates/zz-daemon/src/agent/zz_command.rs`) approves a command line that runs only
`zz` or `zz-dev`, with `cd`, `echo`, `printf` and `sleep` allowed as their own steps and `head`,
`tail`, `grep`, `rg`, `wc`, `jq`, `cut` and `tr` as pipe filters, for any verb. The Claude Code and
Codex drivers apply it at auto-approve `reads` (the default) and `all`, not at `off`. `tmux ...`
reaches the daemon through the wrapper on `PATH` but is not approved.

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
- `bench/agents/fresh/run-one.sh` still runs `split-window --kind agent --provider`.
- The `--wait` timeout message tells you to run `agent-send -t %N --wait ''`, which a chat refuses
  ("is a chat; --wait needs text to send").
- `AGENT_PANE_CONTEXT` says this pane approves `zz` commands without asking, which is false at
  auto-approve `off`, and its filter list leaves out `cut` and `tr`.
- The `events` help says `agent-state-changed` carries `agent_state` for every pane kind; a chat
  sends none.

# Proposed commands

tmux verbs keep their meaning on a chat, and two zz verbs sit on top, the way `run-pane` sits on
top for shells.

| Job | Command | Change |
| --- | --- | --- |
| Find | `list-panes -F '#{pane_id} #{pane_current_command} #{agent_state}'` | chat facts filled in formats, `inspect` and `agent-state-changed` |
| Create | `new-pane\|split-window\|new-window [-d] [-P -F fmt] [-c dir] [-t target] --kind agent claude\|codex [--model M] [--effort E] [--resume ID] [BRIEF]` | `--kind agent` comes back, made directly, brief queued, parent recorded |
| Read | `capture-pane -p -t %N` | prints the chat as plain text |
| Talk | `agent-send -t %N TEXT` | a chat caller gets the reply as a message with no flag |
| Answer | `agent-respond -t %N --allow\|--deny\|--option ID` | none |
| Close | `kill-pane -t %N` | closing a parent closes its children |
| Type | `send-keys`, `send-text`, `paste-buffer`, `pipe-pane`, `run-pane` | an error that names `agent-send` |

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
`-S -`, capped at 64 KiB like the notify text. This needs a text renderer in the daemon over the
journal. A prompt prints as `> ` and its text, a tool call as one line (its tool and title), a
reply as its text.

## Talk

`agent-send -t %N TEXT`. When the caller is a chat, the reply comes back to it as a message with no
flag, which is what `--notify` does for terminal callers. `--wait` blocks and prints it.
`--wait ''` on a chat waits for the running turn.

Proposed and not ruled: `agent-send` targets chats only, dropping the half that types into
terminals and its `--submit` flag, since `send-keys` and `send-text` type into terminals.

## Answer

`agent-respond`, unchanged. Ruling 4 relies on the approval rule above: the host's
`zz agent-respond ...` runs without a prompt at `reads` and `all`. At auto-approve `off` the host's
own command needs the user's approval.

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

A reply or an ask arrives in the calling chat as a queued prompt: it runs now if the chat is idle,
else behind what it is doing. Each one ends with the command to run next, so the host needs no ids
and no memory of the syntax.

```
[zz] %4 (codex) asks: Bash cargo test -p zz-daemon
zz agent-respond -t %4 --allow
zz agent-respond -t %4 --deny
```

```
[zz] %4 (codex) finished:
<final text, last 64 KiB>
```

The other forms follow `agent_notify_message`:

```
[zz] %4 (codex) stopped (<reason>):
<final text, last 64 KiB>
```

```
[zz] %4 (codex) failed: <message>
```

```
[zz] %4 (codex) was closed.
```

An ask with options lists one `zz agent-respond -t %4 --option ID` line per option instead of
allow and deny.

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
  which gives that verb its first client.
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
  lines in the standing context, or a short note added only to a prompt that has a mention. The
  text fits either (286 bytes):

```
@+claude or @+codex[/MODEL] asks you to delegate: write a brief that stands on its own and run zz new-pane -d --kind agent codex [--model MODEL] 'BRIEF'. @host:%N is a running chat: zz agent-send -t %N TEXT. Replies and permission asks come back to you as messages; do not wait or poll.
```

# Host chat

One row per child in the host transcript: agent, state (working, asking, done, failed, closed),
elapsed time, and what the host allowed or denied. Clicking it focuses the PiP. The reply shows as
a quoted block. The UI gets boarded in `design/mockups` before any app code.

# Build order

Each step lands alone.

1. **Chat facts and input errors.** Formats, `inspect` and `agent-state-changed` for chats; the new
   error text for the input verbs.
   - On an idle codex chat `%4`, `zz list-panes -F '#{pane_id} #{pane_current_command}
     #{agent_state}' -f '#{==:#{pane_id},%4}'` prints `%4 codex idle`.
   - `zz send-keys -t %4 x` fails with exactly `%4 is an agent chat; send with: zz agent-send -t %4
     TEXT`.
2. **`--kind agent` on the create verbs, with the brief.** Fix `bench/agents/fresh/run-one.sh`.
   - `zz new-pane -d -P -F '#{pane_id}' --kind agent codex hi` prints a pane id, and an immediate
     `zz agent-send -t <that id> again` exits 0.
   - `rg -- '--provider' bench/agents/fresh/run-one.sh` finds nothing.
3. **Parent link, replies and asks into chats, close cascade.** Also `--wait ''` on a chat.
   - Run from chat `%1`, `zz new-pane -d -P -F '#{pane_id}' --kind agent codex 'reply with ok'`
     prints `%2`; `zz display-message -p -t %2 '#{agent_parent}'` prints `%1`; the next prompt `%1`
     receives starts with `[zz] %2 (codex) finished:`.
   - After `zz kill-pane -t %1`, `zz list-panes -a -F '#{pane_id}'` lists neither `%1` nor `%2`.
4. **Agents rows in the picker and the teaching text.**
   - Picking the codex row in a composer holding `fix it` after the mention, `TargetPicker::raw_text`
     returns `@+codex fix it`.
   - The teaching text is at most 300 bytes.
5. **Host row**, after the design board.
   - While `%2` waits on a permission, `%1`'s transcript has one row for `%2` reading `codex ·
     asking`.
6. **`capture-pane` on a chat.**
   - After a turn whose prompt is `reply with ok` and whose reply is `ok`, `zz capture-pane -p -t %2`
     prints exactly `> reply with ok\nok\n`.
7. **Loose ends** listed [above](#loose-ends-found-on-the-way).
   - The `--wait` timeout message names a command a chat accepts.
   - `AGENT_PANE_CONTEXT` lists `cut` and `tr` and says the rule does not apply at `off`.

# Open

- Should `send-keys` on a chat submit on Enter and cancel on `C-c`, or only point to `agent-send`?
  The first needs a draft buffer in the daemon.
- Should `tmux ...` commands be approved like `zz ...` ones?
- Standing context, or a note in the prompt that has the mention?
- Does the child get anything beyond the brief (the host's folder, files it touched)?
- Can a child create children? There was never a depth guard.
- Replies share the caller's queue of 4 with the user's own prompts; a host with several children
  can overflow it.
- A question that wants free text has no command-line answer.
- Trimming `agent-send` to chats.
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
