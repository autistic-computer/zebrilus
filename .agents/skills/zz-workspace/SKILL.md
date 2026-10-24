---
name: zz-workspace
description: Drive the surrounding zz workspace from inside a zz pane using the `zz` CLI to send text or piped output to an agent running in another pane, read a terminal pane's scrollback, screenshot a browser pane, navigate a browser, and route a failing command's output to an agent. Use whenever you are running inside zz (the `ZZ_PANE` environment variable is set) and the task involves another pane, a terminal's output, a browser page, or another machine.
---

# Workspace tools

Drive terminal and browser panes, and the agents in them, through the `zz` CLI. Run `zz tools`
for the full catalog or `zz tools agent` for agent verbs. Run `zz tools --skill`
for the agent guide with skill frontmatter. Regenerate it with `just tools skill`.
Run `zz tools terminal` for terminal verbs, `zz tools browser` for browser verbs,
and `zz tools advanced` for state subscriptions and terminal-agent integration.
`zz tools --context` prints the short startup context for the calling pane.
Run `zz tools --install-hooks` once so Claude Code and Codex started in a
terminal pane get it, and `zz tools --remove-hooks` to take the hook and stub
skill out again. Codex
skips a new or changed hook until you approve it with `/hooks` in Codex.
The install also adds each agent's own allow rule for `zz` (Claude Code's
`Bash(zz:*)`, a Codex `rules/zz.rules` file), so `zz` commands run without
asking, and Codex runs them outside its sandbox when the ids are written out
rather than held in variables.

## Environment

| Variable | Meaning |
| --- | --- |
| `ZZ_PANE` | Your own pane, e.g. `%3`. Do not send to yourself. |
| `ZZ_SESSION` | Your daemon session name. |
| `ZZ_SOCKET` | The daemon endpoint; the CLI honors it. |

Explicit values in `agent-command` config take precedence over these defaults.

## Targets

Start with a pane summary or selected fields:

```sh
zz inspect -t %3
zz list-panes -F '#{pane_id} #{pane_kind} #{agent_state} #{@agent_state}'
zz list-sessions
zz list-windows
```

Use `list-panes --json` for the full tmux variable set.
Use stable IDs: `%N` for a pane, `@N` for a window, `$N` for a session.
Pass the bare ID: `-t %3` works; `-t work:%3` fails with `can't find window`.
Run `zz <verb> --help` for options, or `zz list-commands [verb]` for usage.
`zz` without a verb attaches the terminal UI (`new-session -A`); with `--host NAME`, on that host.
Pane options need `-p`: `zz set-option -p -t %3 @name reviewer`.

`#{pane_kind}` is `terminal`, `browser`, `editor`, or `picker`.
`#{pane_last_command_status}` is the last completed command's exit code, or empty
when unknown; terminal panes report it from OSC 133 marks.
`#{@name}` reads a user option from pane, window, session, then global scope.

## Remote hosts

`zz list-hosts` lists the machines in `zz/config` with their state and version.
`zz --host NAME VERB` runs a verb there (NAME may be an ssh destination), and
`zz --host all VERB` runs a read-only verb on every host, prefixing text lines with
`NAME: ` and adding `host_name` to JSON lines. Pane IDs are per host and none is
yours there: pass `-t %N` from `list-panes -a` or your own
`new-window -d -P -F '#{pane_id}'` (`new-session -d -P -F '#{pane_id}'` on a host
with no session); a verb that acts on a pane, window or session exits 2 without
`-t`, and a move or swap without `-s`. Paths and pane environments are that
host's; `events` and `agent-send --progress` stream from it. Logins use zz’s own
key after the first, and `--host all` never prompts. A call that changes state
and stalls 15 s exits 1 and may have run there: check before retrying. When a
machine joins, run `zz fleet add NAME DEST`, then `zz fleet sync`; run
`zz fleet install NAME` for a missing or old zz.

## CLI contract

The `--help` flag alone, or the `help` verb, prints the command catalog. Use
`zz <verb> --help` for a command's description, usage, options, and positional
arguments; aliases and unique prefixes work too. These help forms need no daemon and exit 0. An
unknown verb exits 1; an unknown verb requested through zz’s `--help` exits 2.
Global `-h` keeps the tmux usage banner, and command `-h`
flags keep their tmux meaning.

Add `--json` to `list-sessions`, `list-windows`, `list-panes`, `list-clients`, or
`list-hosts` for one JSON object per row in the same order as text output. Keys are the format
variable names for that entity; values are strings with the same expansion as
`#{name}`, including empty strings for unavailable values. Pane rows include
`pane_kind`, `agent_state`, `agent_pending_permission`, `browser_url`,
`pane_pb_state`, `pane_pb_progress`, and the `pane_status*` program status
variables. Use `show-options --json` for one object
mapping option names to value strings in the selected scope. Combining `-F` and
`--json` is a usage error.

| Exit code | Meaning |
| --- | --- |
| 0 | Success. |
| 1 | Command failure, a stopped agent turn, missing daemon, connection loss, or a tmux-compatible usage error (including an unknown command). |
| 2 | Usage error in a zz-native verb or extension: invalid flag, missing argument, or malformed value. |
| 3 | Blocked or unable to answer now, including `agent-send --on-block fail`; for `run-pane`, nothing ran and a retry is safe. |
| 124 | Wait timed out, including `wait-pane --exit` and `agent-send`; a child may also return 124. |
| 125 | `run-pane` typed the command and it was still running at the timeout, or its child returned 125; the timeout writes a diagnostic to stderr. |

For `wait-pane`, `run-pane`, `send-text`, and `agent-send`,
`--timeout` takes seconds, accepts fractions such as 0.5, and 0 waits forever.

Tmux-compatible commands keep the pin’s exit status, including 1 for parse and usage errors.
The error’s source determines the status: `list-panes -Z` exits 1, while zz’s
`list-panes --json -F x` extension conflict exits 2.
Commands that set an explicit exit code keep that code.

## Agents

Agents run as their own CLIs in terminal panes: start Claude Code or Codex in a pane, for example `zz split-window -h claude`. The desktop and web clients draw a lens over a terminal running either one, its timeline in place of the cell grid, switched from the pane header. The lens reads the transcript the CLI writes and types into the CLI, so the terminal stays the source of truth.

### `zz agent-send [-t %N] [--submit | --notify | --wait [--progress] [--timeout SECS] [--on-block wait|fail|allow|deny] [--json | --final]] [--context PATH[:START[-END]]] [TEXT]`

Send TEXT to the agent running in a terminal pane: Claude Code through its peer socket, Codex through `codex queue`, otherwise typed into its input, with Enter only when `--submit` is given. Read stdin when TEXT is omitted: `git diff | zz agent-send -t %3`. `--context` adds a file/line header and fences the payload; text is capped at 1 MiB.

`--submit` sends now and prints the chosen pane. `--wait`, `--notify`, `--final`, `--json` and `--on-block` go through the pane's lens: TEXT is typed into the CLI, and the transcript tells when the turn ends. `--notify` runs from inside another terminal pane and, when the target's turn ends, posts its reply back into the calling pane. `--wait` waits for the turn and prints its reply on stdout (pane ID on stderr). A pane running neither Claude Code nor Codex falls back to waiting on `@agent_state`, and `--notify`, `--final` and `--on-block allow|deny` exit 2 there. Failure, cancellation, hand-back, or timeout exits non-zero. The timeout defaults to 600 seconds; `0` waits forever. A timeout leaves the turn running. `--json` prints one object with turn facts, `final_text`, and `transcript`. `--final` prints only the text after the last tool call or tool update. Both require `--wait`; combining them is a usage error. `--on-block wait` waits for permission. `--on-block fail` prints the pending permission JSON and exits 3 while the turn continues. `--on-block allow` approves a Claude Code permission prompt and `--on-block deny` rejects it, with the keys its dialog takes. Codex approvals are answered in its terminal.

Turn facts: `final_text` contains message text after the last tool call or tool update. `tool_calls` counts tool calls the transcript showed; permission counters count only requests that reached the lens. Each buffer keeps its tail when capped: 1 MiB for the transcript and 256 KiB for final text. The JSON object has `pane`, `stop_reason`, `duration_ms`, `tool_calls`, `permissions` (`requested`, `allowed`, `denied`), `truncated`, `final_text`, and `transcript`. A blocked reply also has a nested `permission` object. Exit 0 means `end_turn`; other stop reasons exit 1 with the reply still printed, except cancellation, which keeps its existing error output. A blocked wait exits 3 and a timeout exits 124. Stderr carries the pane ID and on-block audit lines. `--progress` requires `--wait` and an explicit `-t %N`. It prints tool calls, state changes, and a heartbeat after 60 seconds without a line to stderr. Titles in progress lines stop at 120 characters.

### `zz agent-respond [-t %N] (--allow | --deny | --option ID) [REQUEST_ID]`

Answer the named or oldest pending permission in a pane whose lens is open, in a desktop or web client or through a running `agent-send --wait`. Read the pending request with `zz inspect -t %N --json | jq .permission`.
Print the chosen option ID. `--allow` approves once and `--deny` rejects. Other panes exit 1 with `has no agent lens open`.

### Permissions

Terminal agents keep their own permission settings: Codex `approval_policy`, Claude Code permission rules. `zz tools --install-hooks` adds a rule that lets both run `zz` without asking.

### `zz inspect [-a | -t %N] [--json]`

Describe a pane's kind, process, geometry, exit status, progress, agent facts, and browser URL.
Read one `key: value` line per field, or use `--json` for one object with string facts and string arrays for `verbs` and `events`. Text output lists use spaces and skip empty facts; JSON keeps every key, with empty strings for unavailable facts. `agent_pending_permission` is the pending request ID, or empty. `verbs` lists what applies to this pane's kind; terminal panes with a nonempty `agent_state` also include `agent-send`, and panes with an open lens `agent-respond`. `events` names what `zz events` can report for the pane. Use `-a` for every pane in every session, ordered by session ID, window index, and pane index, with a blank line between text blocks or one JSON object per line; `-a` and `-t` cannot be combined. With neither, inspect the current pane.

```sh
zz inspect -a --json | jq -c 'select(.agent_state!="") | {pane_id,agent_state}'
```

The keys, in text output order, are `session_id`, `session_name`, `window_id`, `window_index`, `window_name`, `window_width`, `window_height`, `window_size`, `pane_id`, `pane_index`, `pane_active`, `pane_kind`, `pane_pid`, `pane_current_command`, `pane_current_path`, `pane_title`, `pane_width`, `pane_height`, `pane_dead`, `pane_dead_status`, `pane_dead_signal`, `pane_last_command_status`, `pane_pb_state`, `pane_pb_progress`, `pane_status`, `pane_status_kind`, `pane_status_progress`, `pane_status_app`, `pane_status_title`, `pane_status_message`, `pane_status_raw_title`, `pane_status_raw_message`, `pane_status_reported`, `agent_state`, `agent_pending_permission`, `permission`, `browser_url`, `verbs`, `events`.

`permission` is a nested `{"request_id":7,"tool_call":{...},"options":[...]}` object or `null` in JSON output; text output prints compact JSON on the `permission:` line, or an empty value.

### `zz events [-t %N]`

Stream hook events.
Print JSON lines, flushed per line: `{"seq":1,"event":"agent-state-changed","time":1750000000000,"hook_pane":"%3","agent_state":"working",...}`. Each line includes the hook's string variables. `time` is Unix milliseconds. Wait for the first line, `{"seq":0,"event":"ready","time":...}`, before starting work. On subscriber overflow, `gap` consumes the next sequence number; the client reconnects and continues counting without another `ready` line. Use `-t %N` for a pane, `-t @N` for a window, `-t '$N'` for a session ID, or `-t name` for a session name. Filters match exact hook fields, and a session filter also passes the events of that session's windows and panes; `ready` and `gap` always print. Omit `-t` to stream without filtering. `agent-state-changed` carries `agent_state` for every pane kind and `agent_pending_permission` with the permission ID or an empty string. Terminal panes with an open lens also emit `agent-tool-call` for new calls and completed or failed status changes. Each event carries `tool_call_id`, `tool_title`, `tool_kind`, and `tool_status`; titles collapse whitespace and stop at 200 characters: `{"seq":2,"event":"agent-tool-call","time":1750000000000,"hook_pane":"%3","tool_call_id":"call-1","tool_title":"cargo test","tool_kind":"execute","tool_status":"in_progress"}`. These agent events are stream-only; `set-hook` cannot bind them. Other `@option-changed` firings are not streamed; use a hook for those. Invalid arguments exit 2. Connection and daemon errors, or any disconnect other than overflow, exit 1, including server shutdown.

## Etiquette

- Send to another person's agent only for an authorized hand-off.
- Do not send to `$ZZ_PANE`.
- Trim piped logs to the part the recipient needs.

### `zz debug-marker [NOTE]`

Write a `user_marker` line to the daemon log for later diagnostics.
Print nothing on success.
