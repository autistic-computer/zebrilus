---
type: Architecture
title: Agent peer bus
description: How the daemon exchanges prompts between named terminal panes and local Claude Code sessions through Claude Code's peer registry and Unix inbox sockets.
resource: crates/zz-daemon/src/agent/claude_peers.rs
tags: [architecture, agent, messaging, claude-code]
timestamp: 2026-10-23T00:00:00-03:00
---

The daemon joins Claude Code's local peer protocol in two directions. You can target a terminal
pane running Claude Code with `agent-send`, and Claude Code can discover and message a named
terminal pane through `ListAgents` and `SendMessage`. Until 2026-10-23 an Agent pane registered as a
peer too; that pane kind is gone ([agent lens](/designs/agent-lens.md)).

The [agent messaging survey](/research/2026-09-10-agent-messaging-survey.md) records live probes
with Claude Code 2.1.267 on 2026-09-10. The implementation uses `peerProtocol: 1`; Claude Code has
not published the registry shape as a stable contract.

Peers are per machine. A record names a live local pid, delivery goes to a local Unix socket, and
replies go to a `uds:` address that must exist on the replier's machine, so `ListAgents` shows only
the panes of this machine's daemon and no daemon mirrors another host's panes. For an agent on
another host, the caller uses `zz --host NAME agent-send -t %N --wait "..."`, which prints the reply
and exits the same way as a local wait, and finds the panes with
`zz --host NAME list-panes -a -F '#{pane_id} #{pane_kind} #{@name} #{agent_state}'`.

## Target selection

`agent-send -t %N TEXT` first resolves the target pane. For a terminal pane, the daemon looks for
a Claude Code registry record whose `tmux` field ends in the exact pane component `%N`, whose pid
is alive, and whose `updatedAt` falls within 24 hours. The record's pid must also be the pane's
shell or one of its descendants: pane ids repeat across zz daemons and real tmux servers, so the
suffix alone once routed a test daemon's fixture greetings into a Claude session that happened to
sit in real tmux pane `%0`. If it finds one, it posts the existing command payload to that
record's socket. Stdin, the `--context` header, and the 1 MiB payload
limit retain their existing command behavior.

Claude Code fills its `tmux` field by running `tmux display-message` itself, so zz's tmux wrapper
must be first on the pane PATH after the user's startup files.
The bundled bash and zsh integration re-pins `ZZ_TMUX_SHIM_DIR` there so Claude Code can identify
the pane.

Both plain `agent-send` and `--submit` send a message on this route; Claude Code has no remote
composer draft. `--wait` collects a reply through a temporary daemon peer, as described below.
Without a matching Claude Code terminal peer, the daemon checks for Codex as described below,
then types the text into the terminal when it runs an agent (`claude`, `codex`, or a nonempty
`@agent_state`). Any other pane exits 1 with `no agent runs in %N; send-text types into any pane`.
Claude Code processes received messages between tool calls or starts a turn when idle, subject to
its own inbound policy.

## Registry record

The daemon reads `$CLAUDE_CONFIG_DIR/sessions` when set, otherwise `$HOME/.claude/sessions`.
It tolerates unknown JSON fields. Claude Code expects the filename `<pid>.json` to identify a
live process and `procStart` to match trimmed `TZ=UTC ps -p <pid> -o lstart=` output.

The daemon writes two kinds of record. Both share these fields:

| Field | Value |
|---|---|
| `sessionId` | Fresh UUID for this registration |
| `cwd` | Pane working directory when known, otherwise the daemon's |
| `kind`, `entrypoint` | `interactive`, `cli` |
| `peerProtocol` | `1` |
| `version`, `pidDomain` | Daemon package version; `darwin` or `linux` |
| `messagingSocketPath` | This peer's Unix inbox socket |
| `startedAt`, `updatedAt`, `statusUpdatedAt` | Unix timestamps in milliseconds |

and differ in these:

| Field | Terminal peer | Transient daemon peer |
|---|---|---|
| `pid`, `procStart` | Pane shell pid and its UTC start text | Daemon pid and its UTC start text |
| `name` | Pane `@name` value | `zz` |
| `tmux` | `<session name>:@<window id>.%<pane id>` | Omitted |
| `peerFeatures`, `status` | `[]`, `idle` | `[]`, `idle` |
| `zz` | `{"pane":"%3","kind":"terminal"}` | `{"daemon":true}` |

The daemon writes records through a temporary file and rename. It takes the socket directory from
an existing record's `messagingSocketPath` when available, otherwise `/tmp/cc-socks`, creating the
directory with mode 0700. Each peer listens on `<directory>/<pid>.sock` with mode 0600.

## Terminal peers

Set a pane's `@name` option to make a terminal agent available to Claude Code:

```sh
zz set-option -p -t %3 @name codex-1
```

A pane-scoped `@name@%N` option signal triggers registration. The daemon uses the shell pid from
`pane_runtime_facts`, without searching descendants. Claude Code sessions with their own matching
record keep that registration, so a pane running Claude Code is reached through Claude's own record.

The inbox passes each user message verbatim, including its attribution wrapper, to
`Shared::deliver_to_terminal_pane`. For the typed delivery path, it calls
`Shared::paste_and_submit`. That method uses the same paste, echo polling, and Enter path as
`send-text`, with a 2000 ms deadline and no daemon lock held during the poll. Failures produce a
warning. Terminal peers ignore control lines.

Messages submit input to the terminal, so do not name a bare shell pane. Changing `@name` updates
`name` and `nameSince` in place. Unsetting the option or closing the pane removes its peer record
and socket.

The initial typed path covered Codex, Gemini, and other terminal agents on 2026-09-10, before
the Codex queue route below. A version of zz hosted Codex's app-server per pane behind a `split-codex` verb, which
gave steering, typed state, and approvals from the CLI; it was withdrawn the same day because it
embedded one vendor's loop and its own verb into zz. The archive tag `archive/codex-host` keeps
that work. Name the pane, deliver through the agent's existing input, read state from
the bell and the title, and let the human answer approvals in the pane.

### Codex through its own queue

On 2026-09-10, live probes with Codex CLI 0.154.0 on macOS verified delivery through
`codex queue --thread <id> --message TEXT`. It prints
`Queued message <id> for thread <id>.` and exits 0. Codex polls its durable queue every ten
seconds and starts the message as a user turn when idle. Messages received during a turn wait
for the next idle. Use the same `CODEX_HOME` as the pane (default `~/.codex`) and no `-c`
configuration overrides. The daemon inherits its Codex home environment and uses the
login-shell PATH capture in `agent/environment.rs` to find and launch the installed CLI.

Codex sets the terminal title to `<thread name> | <project>`. Before its first prompt it sets
only `<project>`. The name comes from its first-prompt auto-title or `/rename`. The observed
sequence was `zz`, `Reply pong-title | zz`, then `titleprobe-42 | zz` after `/rename titleprobe-42`.
The daemon reads the pane title, as printed by `#{pane_title}`, and takes the nonempty segment
before the first ` | `. Title updates require `allow-set-title`, which defaults to on. The pane's
`@name` identifies its peer registration; it does not identify the Codex thread.

The CLI's Node wrapper can make `#{pane_current_command}` report `node`. In
`crates/zz-daemon/src/agent/codex_queue.rs`, `pane_runs_codex` checks the pane process and its
descendants for an executable basename of `codex`, using the shared descendant walk in
`claude_peers.rs`. It reads executable paths through `ps` on macOS and `/proc` on Linux.

For each delivery, `resolve_thread` launches a short-lived `codex app-server` subprocess over
stdio to list threads. It sends newline-delimited JSON in this order, waiting for the initialize
response before the next two messages:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"zz","title":"zz","version":"0.7.0"},"capabilities":{"experimentalApi":true}}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"thread/list","params":{"limit":50,"archived":false,"cwd":"/Users/x/dev/zz","useStateDbOnly":true}}
```

The implementation sends the daemon package version in `clientInfo`. Responses omit `jsonrpc`;
the daemon matches request IDs and skips unrelated notifications such as
`remoteControl/status/changed`. It follows `nextCursor` for up to four pages and accepts exactly
one thread whose name and cwd match the pane's title name and current path, falling back to its
start path. It refuses an incomplete listing after four pages. It uses the resulting ID for
`codex queue`, since Codex can reject a session name it cannot prove unique. It reads no private
Codex files and makes no guesses from timestamps. Earlier live attempts that guessed from file
timestamps selected the wrong session.

The lookup has a ten-second overall deadline, and the queue command has a separate ten-second
deadline. The daemon closes lookup stdin, terminates the child, and waits for it on completion
or failure. It hosts no persistent vendor server, adds no vendor verb, and leaves the interactive
Codex process in control of turns and approvals.

Direct `agent-send` and `--submit` both queue the payload and print the pane ID for a
noninteractive caller. Errors exit 1 and do not fall back to typing:

- No title name: `Codex session in %N has no name yet; send it a first prompt in the pane or run /rename there`.
- No matching thread: `no Codex session named X in DIR for %N; run /rename in the pane`.
- Several matching threads: `N Codex sessions named X in DIR; run /rename in the pane`.
- Missing executable: `codex executable not found on the login-shell PATH`.
- Queue failure: Codex's trimmed stderr, with the target pane ID.

The first case falls back since 2026-10-24: a session with no title name gets the text pasted and
submitted, as a peer message does, because its first prompt is what names it.

For a terminal pane without a peer reply channel, `--wait` requires a non-empty
`@agent_state`. It uses terminal delivery and waits for a non-idle state followed
by `idle`, with no reply text. A state that stays idle for 15 seconds after sending
exits 124; the overall timeout also exits 124. `failed` exits 1; `blocked` waits
unless `--on-block fail` requests exit 3. An unset state exits 1 before sending.

A named terminal peer running Codex receives its full attributed message through the same queue.
If the session has no title name yet, the daemon logs the reason at info level and uses the
existing paste-and-submit path for that peer message. Other queue errors produce a warning and
leave the message unsubmitted. Other terminal agents retain typed delivery.

## Waiting for a reply

For a Claude Code terminal target, `agent-send --wait` requires a command or control client. The
first pending wait registers a peer for the daemon's own pid, named `zz`, with no `tmux` field.
Concurrent waits share that inbox. The daemon removes its record and socket once no wait is
pending and no wait has timed out in the last hour (`LATE_REPLY_KEEP`).

The daemon registers the message id and target socket before posting. The payload retains the
sender pane's peer name, or the `zz %N` / `zz` fallback, but its `from` address points to the daemon
inbox. Claude Code can therefore validate the reply address through the registry. A plain send
from a caller without a peer stays fire-and-forget by decision: a caller that needs the answer uses
`--wait`, and a pane that should be reachable sets `@name`; there is no daemon inbox for strays.

Replies have no reply-to id. A `type: user` line resolves the oldest unresolved wait whose target
socket equals the line's `from` address (`uds:<target socket>`). Other targets keep waiting. A
reply from a target whose last wait timed out within the hour is kept, the latest one per target,
for the next wait on that pane; other replies without a matching caller are logged and dropped.
Stdout receives the trimmed text between
the wrapper's first line and closing tag; an unwrapped reply retains its raw content. On success,
the caller receives the pane id on stderr.

Delivery updates match `action: peer_message_status`, regardless of `type`. The daemon correlates
`orig_msg_id` with the posted message id, and also handles ids listed in `dropped_msg_ids`.
`held` and `delivered` keep waiting. `expired`, `dropped`, or `status_detail: refused` fail with exit 1
and report the status and supplied `reason` or `drop_reason`. A dead target pid also fails with
exit 1. The timeout defaults to 600 seconds, and `0` waits forever. Timeout errors name the pane
and exit 124; a timeout does not cancel the remote turn.

The wait also follows the pane's `@agent_state`, which Claude Code's OSC 7501 program status or the
registry scan writes (`busy` is `working`, `waiting` is `blocked`, anything else `idle`): once the
pane has been non-idle since the send, a return to `idle` ends the turn and prints the reply if one
came, or nothing when the session answered only in its pane. `blocked` exits 3 under
`--on-block fail` and otherwise keeps waiting. A state that is already non-idle at the send counts,
so a message that lands just as an earlier turn ends can return that turn's end.

`agent-send -t %N --wait ''` (empty text, no `--context`) posts nothing. It prints a kept late reply
at once, returns at once with empty text when the pane is idle, and otherwise waits for the reply
or the end of the running turn the same way. A lead re-arms a timed-out wait this way without
nudging the worker. Terminal panes without a Claude Code record treat empty text the same way on
`@agent_state` alone; an Agent pane refuses it.

```json
{"type":"control","action":"peer_message_status","orig_msg_id":"sent-message-id","status":"dropped","reason":"peer policy","from":"uds:/tmp/cc-socks/4712.sock"}
```

## Control lines and pid verification

Claude Code sends two kinds of lines to a peer socket: `type: user` messages and `type: control`
lines such as `notify_when_idle` subscriptions and `peer_message_status` reports. Verified on
2026-09-10: it delivers messages to any socket named in a record, but it sends control lines only
when the process that accepted the connection is the record's `pid`. A record whose socket is
served by another process, which is every zz pane peer because the daemon holds the socket while
the record names the shell, never receives a subscription, and the sender reports "the
subscription could not be sent". The transient daemon peer is the exception, since its record
names the daemon's own pid, which is why delivery status reports reach pending waits.

zz therefore advertises no `peerFeatures` on pane peers and offers no idle notices.
Offering notices would need a helper process per pane that owns both the record pid
and the socket.

The sticky channel is level-triggered like tmux's `wait-for`: a signal that happened
before the wait wakes it at once. A read-then-wait loop started right after a send
can see the pre-send idle and return early. Use `zz agent-send -t %N --wait "..."`
to wait for a turn on any pane kind. Use the channel loop to observe transitions
you did not cause.

The subscription line, for reference, is:

```json
{"type":"control","action":"notify_when_idle","from":"uds:/tmp/cc-socks/4712.sock","from_mode":"bypass","msgV":1,"msg_id":"f05b57fb-0000-4000-8000-000000000000"}
```

and the notice a peer that could answer would post to that `from` address is:

```json
{"type":"control","action":"peer_idle_notice","msgV":1,"orig_msg_id":"f05b57fb-0000-4000-8000-000000000000","state":"idle","detail":"one short line","from":"uds:/tmp/cc-socks/<own pid>.sock"}
```

## Wire line

Peers exchange newline-delimited JSON over Unix stream sockets. An outbound message from a
registered terminal pane has this shape:

```json
{"msgV":1,"msg_id":"ad3b395d-02dd-46fa-a37c-f35170304716","type":"user","priority":"next","from":"uds:/tmp/cc-socks/4712.sock","message":{"role":"user","content":"<cross-session-message from=\"uds:/tmp/cc-socks/4712.sock\" from-name=\"codex-1\" from-mode=\"prompting\">\nTEXT\n</cross-session-message>"}}
```

The sender generates a UUID v4 for `msg_id` and uses `from-mode="prompting"`. It replaces a
payload's `</cross-session-message>` with `<\/cross-session-message>` before wrapping it, so
payload text cannot end the attribution wrapper early.

For a registered sending terminal pane, `from-name` uses its peer name and `from` points back to its
socket. Otherwise the sender omits both the JSON `from` field and the wrapper's `from` attribute;
`from-name` is `zz %N` when the caller pane is known, or `zz`. Claude Code validates reply
addresses against its registry. The daemon sends no auth line and reads no reply from the outbound
connection. Claude Code 2.1.304 also writes a `<pid>.<sha256>.key` file beside each record carrying
a `peerToken`, and its inbox requires the matching `{"type":"auth","token":...}` line only on
Windows (`requireAuth ?? platform==="windows"`). If it becomes required on macOS or Linux, local
delivery to Claude Code breaks until the sender reads the target's key file and sends that line
first.

One listener thread serves each registered peer. It tolerates empty liveness connections,
ignores `type: auth` lines and unrecognized messages, and accepts `type: user` lines with a string
`message.content`. The listener applies a 30-second idle deadline to connections and reads sockets
without holding the daemon lock. It logs the sender and byte count at info level, then passes the
content verbatim, including its wrapper, to `deliver_to_terminal_pane`, which uses the Codex queue
or paste path described above; the daemon peer routes replies to pending waits.

## Lifecycle and limits

In `crates/zz-daemon/src/daemon.rs`, `update_terminal_peer` registers a terminal pane when its
`@name` is set, renames the record in place when it changes, and removes it when the option is unset
or a Claude Code record of its own already names the pane. The daemon stops the listener and removes
the record and socket when `close_agent_panes` closes a pane or the daemon shuts down. At startup it removes stale records carrying the `zz` marker whose pid is
dead, together with their sockets; it leaves unmarked records alone.

On macOS, `setsockopt` fails with `EINVAL` on a Unix socket whose peer has already closed, even
though the buffered data is still readable, so the inbox treats a failed read-timeout call as
advisory instead of dropping the connection.

The module requires Unix and the daemon's `agent` feature, which belongs to the default feature
set. Windows retains the previous outbound routing. Plain sends from callers without a registered
peer still omit a reply address; socket waits provide the transient daemon address. This protocol
has no durable peer mailbox or cross-machine transport; Codex owns the durable queue used for
Codex terminal delivery. It does not change the `send-text` command
surface, install hooks, or host Codex app-server sessions.
