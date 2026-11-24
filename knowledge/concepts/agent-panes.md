---
type: Concept
title: Agent panes and the flip
description: Native agent chats, switching to a provider's terminal UI, accounts, inter-pane messages and replay.
resource: crates/zz-daemon/src/agent/host.rs
tags: [agent, chat, terminal, replay]
timestamp: 2026-11-24T00:00:00Z
---

## Agent panes

An agent pane runs a headless provider driver under the daemon. Desktop and thin
clients draw the same conversation from its event stream. A provider running its
own TUI in a terminal is a terminal process. Both can keep working after a client detaches.

`AgentDescriptor` in `crates/zz-protocol/src/snapshot.rs` identifies the provider,
working directory, session ID, model, effort and account. `AgentHost::open` starts
a driver with the pane's workspace environment. Closing the pane closes that host.
A provider's session ID lets a new driver resume the conversation.

```sh
zz new-pane -d -P -F '#{pane_id}' -c ~/work --kind agent codex 'Review the pending changes'
zz split-window -d --kind agent claude
zz select-pane-kind -t %3 --resume SESSION agent grok
```

`new-pane`, `new-window` and `split-window` accept an agent kind, provider,
`--model`, `--effort`, `--account`, `--resume` and an optional initial brief.
A brief is limited to 1 MiB and goes to the new driver's prompt queue.
`new-pane` makes a float, `split-window` a tile and `new-window` a separate window. `select-pane-kind`
can convert a picker or replace a terminal in place. A chat takes its folder
from `-c` or the donor pane. See `agent_spawn` and the conversion commands in
`crates/zz-mux/src/command.rs`.

## Providers

| Provider | Default chat driver | Flip to TUI | Switch login slots |
|---|---|---|---|
| Claude Code (`claude`) | `claude -p` with stream-json | Yes | Yes |
| Codex (`codex`) | Codex app server | Yes | Yes |
| Antigravity (`agy`) | `agy acp` | Yes | Single login |
| Grok (`grok`) | `grok agent stdio` | Yes | Single login |
| Cursor (`cursor`, `cursor-agent`) | Cursor's ACP service | Chat only | Single login |

`AgentProvider::flips`, `AgentProvider::accounts` and `host::pane_runner` define
these choices. Explicit commands can select a different adapter for Claude Code
or Codex. Vendor flags, protocol messages, version checks and session files are
documented in [agent CLI contracts](/references/agent-cli-contracts.md).

### Antigravity

The driver selects an installed executable that supports `agy acp`, and maps
`thought_level` to effort. See `crates/zz-daemon/src/agent/agy.rs`.

### Grok

The driver adapts Grok's ACP session options, sign-in and weekly usage for the
shared chat. See `crates/zz-daemon/src/agent/grok.rs`.

### Cursor

The driver starts the ACP executable, handles its sign-in flow and exposes its
parameterized model choices. Its ACP session is separate from Cursor's TUI session.
See `crates/zz-daemon/src/agent/cursor.rs`.

### Codex on the shared app server

On Unix the default Codex command connects to the shared server, starting it
when necessary. An override with arguments or environment entries uses a private
app server.
The shared server owns the thread and its active work, so a supported flip can
rejoin a running turn. The driver passes the pane's workspace variables on thread
start, resume and fork. A thread another client already holds can retain its existing
environment. See `managed_daemon` and `thread_config` in `crates/zz-daemon/src/agent/codex/mod.rs`.

## The flip

`zz flip-agent -t %3` keeps the pane ID and layout position while switching between
a supported TUI and a native chat. The daemon resolves the CLI's session, folder
and available launch settings. It stops the old runtime and starts the other
representation with that session. A CLI without an established session can start
a fresh chat. Process and session selection is provider-specific. See
`daemon/agent_flip.rs` (`flip_plan`) and `agent/cli_session.rs` in `crates/zz-daemon/src`.

By default, a flip that would interrupt active work reports what is busy.
`--when-idle` arms the flip, `--cancel` disarms it, and `--now` can interrupt
the turn and its background work. The shared Codex server can carry active work
across a flip. The pane header offers the applicable choices. A session title
reported by the driver updates the pane subject to `allow-set-title`, and
automatic window naming uses the active chat's title.

## Driving a pane

| Operation | Command and behavior |
|---|---|
| Inspect | `zz inspect -t %3 --json` includes provider, model, vendor session ID, parent, state and pending request. `capture-pane -p -t %3` reads the last turn. Add `-S -` for the whole chat. |
| Send | `zz agent-send -t %3 'TEXT'` submits to the chat. Omit text to read stdin. `--context` adds a context header. |
| Wait | `--wait` returns the turn's response. `--wait ''` joins the running turn. `--final` selects text after the last tool activity, `--json` gives structured output, and `--progress` reports activity on stderr. |
| Handle a block | `--on-block wait\|fail\|allow\|deny` governs pending permissions while waiting. A failed-on-block wait exits 3. A timeout exits 124 and leaves the turn running. |
| Answer a request | `zz agent-respond -t %3 --allow\|--deny\|--option ID\|--answer TEXT [REQUEST_ID]`. Specifying the request ID protects against answering a later request. |
| Reopen a child | `select-child-chat` uses the recorded child session to select a live chat or reopen it. |
| Queues | A chat accepts up to four waiting prompts. A stopped or failed turn can return unsent prompts to the originating composer. |

`agent-send --wait` and `--notify` follow the target turn. The default timeout
is 600 seconds and `--timeout 0` waits indefinitely. With `--notify` a terminal
caller receives a message when the turn ends. A chat caller already receives a
reply to a plain send. The JSON wait result includes the pane, stop reason,
elapsed time, tool and permission counts, transcript, final text and truncation.
Final text is the message text after the last tool call or update. The retained
tails are bounded to 256 KiB of final text and 1 MiB of transcript.

`#{agent_state}` exposes `starting`, `idle`, `working`, `needs-approval`,
`needs-input` or `failed` for a chat. `#{agent_pending_permission}` gives the
request ID. Both are available through `inspect` and `list-panes -F`.
See `agent_chat_facts` and the `agent-send` command in `crates/zz-daemon/src/daemon.rs`.

A recognized terminal agent rejects a new `agent-send` prompt and directs the
caller to flip. On terminal integrations that publish `@agent_state`, a wait
can observe the state transition. Terminal input commands belong to terminal panes.
Claude Code's local named-terminal integration is described in [named terminal peers](/architecture/agent-peer-bus.md).

### Chats that open other chats

A chat an agent opens is for work the user watches or steers: a task they asked it to
hand off, a fork of a conversation, a task for another machine. The startup context and
the `zz tools` guide tell agents to keep background work of their own (searches, reading,
split-up edits) in their own subagents. See `WORKSPACE_CONTEXT` in
`crates/zz-daemon/src/daemon.rs`.

The daemon records a chat's origin, attributes messages to the sending pane
and delivers child replies to the calling chat. Child chats automatically approve tool permissions unless `agent-auto-approve`
is `off`. Local child questions and outstanding permissions can reach the caller
with the request ID. A child cannot open another
chat or send directly to its parent. Closing the parent removes its child panes.
A chat can send eight prompts to the same chat between user prompts, including
the initial brief. For a chat caller a plain send reports its result back, and a
blocking wait defaults to failing on a pending request.

A reply or an ask from another chat goes into the caller's running turn when its agent can
take a message mid-turn (Claude Code and Codex); a message that misses the turn, or a caller
that cannot be steered, gets it as its next prompt. A pane's `queued_prompts` counts only what
the user queued from the composer, and taking the queue back returns those and leaves the rest
waiting. Closing a Codex chat for good interrupts the turn it would leave on the shared
server; a flip or an account switch leaves the turn running. See `deliver_into_turn`, `unqueue`
and `end` in `crates/zz-daemon/src/agent/host.rs`.

A plain `zz --host NAME agent-send -t %N TEXT` from a chat can relay the result
through its own daemon. The remote pane is identified by host and pane ID. A
remote permission stays on its host. See `crates/zz-daemon/src/daemon/agent_relay.rs`,
`crates/zz-protocol/src/agent_mention.rs` and [agent mentions](/designs/agent-mentions.md).

## Accounts

Claude Code and Codex each have one native login active on a host. On macOS and
Linux, zz can park other logins and switch the active one. Switching affects
other chats of that provider and the CLI using its native store. The daemon
relaunches affected chats on their sessions. `agent-account -t %3 ID` checks
whether any affected chat is busy. `--when-idle` defers switching, and `--now`
allows interruption. `default` leaves the CLI's current login in use. See
`crates/zz-daemon/src/daemon/agent_account.rs` and [account storage and recovery](/designs/agent-account-swap.md).

### Listing and adding accounts

The account picker and Settings request the installed providers' accounts from
the daemon. `agent-accounts` returns a client event, and `--add` starts the
provider's sign-in. The account list distinguishes active and parked logins.
See `crates/zz-daemon/src/daemon/accounts.rs`.

### Signing in without a terminal

The daemon drives the CLI's login and sends progress to the initiating client.
A local browser can receive a loopback callback. A browser on another machine
uses the provider's supported device-code, link or manual-code flow. A sign-in
can target a chat so its steps appear above the composer. A terminal sign-in
is also available on supported paths. See `crates/zz-daemon/src/daemon/agent_login.rs`
and `AgentLoginRequest` in `crates/zz-protocol/src/providers.rs`.

### Rate limits

The daemon caches the usage each provider reports, per host account. The chat
can display five-hour and weekly windows, including reset times when supplied.
Context usage belongs to the current conversation. See `QuotaWindows` in
`crates/zz-protocol/src/providers.rs`.

## What the clients draw

Chat mode is a workspace navigation choice. It lists windows containing native
agent panes or recognized terminal agents first on each machine. One ordinary
window remains a row. Several ordinary windows share a pile. Switching the
sidebar mode changes the view of those windows. Only `flip-agent` or pane
conversion changes the runtime in a pane. See `pane_program` and `ChatMachine` in
`crates/zz-client/src/window_summary.rs` and [chat mode](/designs/chat-mode.md).

The desktop and ala thin clients share the timeline, tool calls, questions,
approval cards, queue, composer and previews in `crates/zz-ui/src/agent`. The
HTML client has its own renderer. Model, effort, modes, attachments and session
operations depend on capabilities reported by the driver. A watch-only driver
gets a read-only timeline. A fresh chat can select a folder or resume a session.

The slash menu combines driver-reported commands with client operations such
as `/steer`, `/btw`, `/fork` and `/rewind` where supported. A steer adds to
the current turn when the driver accepts it. A fork starts a separate session,
optionally at a chosen turn, and can open as a floating pane. Commands specific
to a vendor's own screen belong in its TUI. See `crates/zz-client/src/agent_completion.rs`
and `crates/zz-daemon/src/agent_fork.rs`.

The native timeline groups activity by turn. Tool runs and subagent activity
can fold into rows, while outstanding questions and permissions stay actionable.
See `crates/zz-client/src/chat_rows.rs`.

### Scrolling

Sending a prompt or pressing **Jump to latest** moves to the end. For the next
turn, the chat follows the reply until its first line reaches the top, then
holds it there for reading. Opening a chat mid-turn or watching a turn started
on another client follows the latest output. With reduced motion, jumping to
the end is immediate.

Using the wheel or scrollbar releases the hold. Scrolling up pauses following.
Scrolling back down to within 24 px of the end resumes it. Holding the left
mouse button freezes the transcript for selection. Releasing after a drag of
more than 4 px leaves following paused so the selection stays visible. A click
preserves the prior follow state. See `ChatPane::update_hold` and `let_go` in
`crates/zz-ui/src/agent/chat.rs` and `libs/ala/crates/ala/src/elements/list.rs`.

### In the composer

`@` offers workspace targets and files. Selected targets display a friendly name
and send a resolvable reference such as `@local:%12`. Changing that name by hand
makes it plain text. Provider mentions can delegate work, and a chat reference
can address a specific pane on another host. Dropped local files become path
references when the chat is local. Images can attach directly when the driver
accepts them. See [agent mentions](/designs/agent-mentions.md) and
[palette keys](/concepts/command-palette.md#chips-and-keys).

### Previews and the view pane

A chat can open code, diagrams, Markdown, images and subagent activity in a
floating view. A second preview from the same chat in the window reuses that view.
The descriptor carries up to 64 KiB of text for code, Markdown and diagrams, or an origin
item for images and subagents. It belongs to the mux, so another viewer can see
the same selection. Escape closes it. See `ViewContent` in `crates/zz-protocol/src/view.rs`
and `view_pane` in `crates/zz-mux/src/command.rs`.

## How the stream reaches a client

The daemon hosts a driver per pane. `AgentFanout` batches stream items, numbers
them per pane and sends them with live state. The client requests missing items
with `AgentReplay`. A request within the retained ring gets just those items.
Older requests can rebuild from a complete session journal or from retained
memory. Replay goes to the requester. Overlap with live delivery is resolved by
sequence number. Native CLI transcripts can also be read when resuming a vendor
session. Closing the UI leaves the driver running. See `AgentFanout::replay` in
`crates/zz-daemon/src/agent/fanout.rs`, [data flow](/architecture/data-flow.md)
and the [turn machine](/designs/agent-turn-machine.md).
