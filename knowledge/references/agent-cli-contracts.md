---
type: Reference
title: Agent CLI contracts
description: What zz relies on from the agent CLIs it drives (Claude Code, Codex, Antigravity, Grok Build, Cursor) that the vendor can change without notice, the version each was last checked at, which changes break zz quietly, and the commands that recheck them after an update.
resource: crates/zz-daemon/src/agent/
tags: [agent, claude-code, codex, antigravity, grok, cursor, acp, compatibility, reference]
timestamp: 2026-11-22T00:00:00-03:00
---

# Why this page exists

zz drives five vendor CLIs through interfaces none of them promise to keep: hidden flags,
undocumented stream frames, private files on disk, error strings. All five update themselves, so a
chat can break on a morning when nobody touched zz. Some changes fail loudly (the CLI refuses a flag
and the pane shows its stderr). The dangerous ones fail quietly: resume starts a new session, a turn
never settles, an approval goes through without its answers. This page lists what zz relies on,
marks the quiet failures, and gives the commands that check them.

# When to check

* A CLI updated: its `--version` no longer matches the table below.
* Before `just release ship`.
* A user reports a chat that hangs, loses history, or misreads a model, mode or account.
* Code starts relying on a new vendor flag, frame, field, file, env var or string: add it here in
  the same change, with how to check it.

After a check, update the table: the version, the date, and how far the check went.

# Last checked

| CLI | Installed 2026-11-20 | Last checked against | How far |
|---|---|---|---|
| Claude Code (`claude`) | 2.1.331 (2026-11-24) | 2.1.292, 2026-10-07 | Full: the fixtures in `agent/claude/fixtures` were recorded on it. 2.1.330 passed the flag probe, the binary grep and the `auth status` keys below, and `get_settings` with no prompt sent showed the last `--model` and `--effort` applied. `resume.ndjson` (an agent resumed with `SendMessage`), `background.ndjson` (a background agent's approval across the end of its turn) and `nested.ndjson` (an approval from an agent a background agent started) were recorded on 2.1.330. Live turns through the tape rig on 2026-11-22 (hello, tools with an approval, stop, queue, reload with `--resume`), each with its session transcript. 2.1.331 passed the flag probe (with `--model` and `--effort`), the binary grep and the `auth status` keys on 2026-11-24, and its `initialize` listed Fable as `claude-fable-5-2[1m]` with `displayName` `Fable` |
| Codex (`codex`) | 0.171.0 (shared daemon 0.171.0) | 0.159.0, 2026-10-07 | Full: `tools.ndjson` and `subagent.ndjson` in `agent/codex/fixtures`; `app-server-live.jsonl` is 0.166.0. 0.171.0 had the schema check and a binary grep, all names present, `thread/unsubscribe` included (2026-11-21), and a turn with no model call on a private app server for `turn/steer` refusals, `turn/start` during a turn and resuming a thread mid-turn. Live turns through the tape rig on 2026-11-22 over a shared daemon in the scratch HOME (hello, tools with an approval, stop, queue, reload with `thread/resume`), each with its rollout file |
| Antigravity (`agy`) | 1.5.14 (2026-11-22) | 1.5.14, 2026-11-22 | The ACP handshake, `agy acp --help` and the TUI flags on 1.5.14; live turns through the tape rig (hello, tools with approvals, stop, queue, reload). Its `initialize` still reports `agentInfo.version` 1.5.13. 1.5.13 had live turns including the artifact signal |
| Grok Build (`grok`) | 1.0.92 (2026-11-21) | 1.0.92, 2026-11-22 | Live turns through the tape rig on 1.0.92 (hello, tools with an approval, stop, queue, reload), including `_meta.rules` and `_x.ai/billing` |
| Cursor (`cursor-agent`) | 2026.11.19 | 2026.09.18, 2026-11-20 | Live turns on 2026.09.18. 2026.11.19 passed the initialize probe, the model list (`agent-catalog`) and, on 2026-11-22, `authenticate`, `session/new` and `session/load` through the tape rig; every prompt there ended at once with the reply "Add a payment method to continue", so no turn ran |

# Cheap checks, no sign-in

Run these after any update. Each takes seconds and costs nothing.

**Versions.** `claude --version`, `codex --version`, `agy --version`, `grok --version`,
`cursor-agent --version`.

**Claude Code flags.** A parse probe tells a renamed or removed flag apart from a working one,
hidden flags included. Run it outside a Claude Code session (unset `CLAUDECODE` and the other
`CLAUDE_CODE_*` variables zz strips, `lib.rs`):

```bash
claude -p --output-format stream-json --input-format stream-json --verbose --include-partial-messages --replay-user-messages --permission-prompt-tool stdio --await-initialize --allow-dangerously-skip-permissions --resume-session-at=x </dev/null
```

`--resume-session-at requires --resume` means every flag parsed. `error: unknown option '--…'`
names the one that went away. Add `--fork-session`, `--session-id`, `--append-system-prompt`,
`--model`, `--effort` and `--permission-mode default` the same way when touching those paths.

**Claude Code control requests and env vars.** The binary carries the names it handles; a count of
0 means the name is gone:

```bash
B="$(readlink -f "$(command -v claude)")"; for s in can_use_tool command_lifecycle session_state_changed get_settings get_usage generate_session_title side_question apply_flag_settings stop_task agentProgressSummaries forwardSubagentText terminal_slash_commands CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES claude_authenticate claude_oauth_callback claude_oauth_wait_for_completion; do printf '%s=%s\n' "$s" "$(grep -a -c "$s" "$B")"; done
```

**Claude Code accounts.** `claude auth status --json | jq -c keys` must still list `loggedIn`,
`email`, `orgId`, `orgName`, `planTier` (or `subscriptionType`) and `configDirectory`. A login
signed in on the installed version must still carry its ids: this prints `true` for it, and
`false` for an older login the CLI has not added them to (on Linux, read
`<config>/.credentials.json` instead):

```bash
security find-generic-password -a "$USER" -s "Claude Code-credentials" -w | jq -c '.claudeAiOauth | has("accountUuid") and has("organizationUuid")'
```

**Codex protocol.** Codex prints its own app-server schema. Check the names zz sends and reads
against it (`--experimental`, because zz asks for the experimental API):

```bash
D=$(mktemp -d); codex app-server generate-json-schema --experimental --out "$D"; for s in clientUserMessageId beforeTurnId developerInstructions useStateDbOnly excludeTurns subAgentActivity collabAgentToolCall thread/backgroundTerminals/list item/tool/requestUserInput item/permissions/requestApproval turn/steer review/start thread/compact/start account/rateLimits/read thread/unsubscribe; do printf '%s=%s\n' "$s" "$(rg -l -F "$s" "$D" | wc -l)"; done
```

`jq '.properties | keys' "$D/v2/ThreadForkParams.json"` (or any other `*Params.json`) shows a
request's fields. `codex app-server daemon version` prints the shared daemon's version, which can
differ from `codex --version`. `codex --help`, `codex resume --help` and `codex queue --help` show
new flags; a new flag that takes a value has to join the list `cli_session.rs` skips.

**Codex around a running turn.** `thread/shellCommand` with a `sleep` runs a turn with no model
call. On a private app server (`codex app-server --listen unix://<sock>` with `CODEX_HOME` set to an
empty directory, signed out) it shows how the server answers while a turn runs: `turn/steer`
refusals, `turn/start` joining the turn, and what a second connection sees when it
`thread/resume`s after the first one closed. The socket speaks WebSocket, like the daemon's.

**ACP handshake.** One `initialize` line creates no session and works signed out. Keep stdin open
for a moment, because Grok exits on end of input before it answers:

```bash
for c in "agy acp" "grok agent stdio" "cursor-agent acp"; do { echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}'; sleep 5; } | timeout 15 $c 2>/dev/null | head -1 | jq -c --arg c "$c" '{c: $c, v: .result.protocolVersion, load: .result.agentCapabilities.loadSession, session: (.result.agentCapabilities.sessionCapabilities | keys), auth: [.result.authMethods[].id]}'; done
```

Each must answer `protocolVersion` 1
and `agentCapabilities.loadSession`. agy and Grok must keep `sessionCapabilities.close` (without
it zz cancels instead of closing; Cursor has never offered it). Cursor must still offer the `cursor_login` auth method, which zz sends
before its first session. Also: `agy acp --help` must contain the text `agy acp`, which is how zz
tells a new enough `agy` from an old one (`agent/agy.rs`).

**Latest versions for Settings.** Each must print a version, and the Cursor one must work from an
ssh session with the keychain locked:

```bash
curl -fsSL https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json | jq -r .version; grok update --check --json | jq -r .latestVersion; CURSOR_AGENT_CLI_ASSUME_SSH=0 cursor-agent about --format json | jq -r .latestVersion
```

**ACP TUI flags for flip-agent.** `agy --help` must list `--conversation`, `--model`, `--effort`;
`grok --help` must list `--resume`, `--model`, `--effort`.

# Live checks, signed in

Run these when a cheap check shows a change, before a release, or after a version jump of more than
a few releases.

* **Claude Code and Codex: replay recordings.** Record a new session with the driver's frames
  (`{t,dir,msg}` per line, like the files in `agent/claude/fixtures` and `agent/codex/fixtures`),
  then run `cargo test -p zz-daemon agent::claude` (or `agent::codex`) and
  `ZZ_REPLAY_DIR=<dir> cargo test -p zz-daemon -- --ignored recordings_in_a_folder_drive_the_chat_machine`.
  The 2026-10-07 recordings came from an outside harness. The daemon now records its own: with
  `ZZ_AGENT_TAPE_DIR=<dir>` set, every agent process it starts writes
  `<provider>-<start>-<pid>-<n>.ndjson` there (`agent/tape.rs`): a `meta` line with the provider,
  program, arguments and folder, then every line zz sends (`in`) and the CLI prints (`out`), parsed
  as JSON or kept as text, and an `exit` line. Stderr is not recorded. Claude Code and Codex are
  taped at their pipes or Codex's shared-daemon socket (`child.rs`), the ACP providers at the
  connection's line hook (`runtime.rs`). The `meta` line also carries `started`, the wall clock in
  unix milliseconds, to place a tape line against a vendor transcript line.
  `bench/agents/tapes/record.sh <provider> <scenario>` runs a private daemon with it in a scratch
  HOME and workspace and keeps the run under `/private/tmp/zz-tapes/`, with Claude Code's session
  transcript (and its `subagents/` folder) or Codex's rollout files copied from that HOME into
  `vendor/`. A raw tape or vendor file holds the login, home paths, the host name, the account's
  email and ids, plan names and rate-limit figures until `bench/agents/tapes/scrub.py <raw> <out>
  [<raw> <out> ...]` takes them out, so it never leaves that folder unscrubbed. Scrub a run's
  tapes and vendor files in one call, so the account ids they share get the same stand-ins. The
  scrub also moves timestamps to UTC (`+00:00`) and keeps, of each Claude Code transcript
  attachment other than `queued_command`, only `type`, `uuid`, `parentUuid`, `timestamp` and the
  attachment's own type: `sessions::transcript` reads the same updates from the blanked file, and
  the rest holds the CLI's own prompt and skill listings. Codex rollouts stay under
  `/private/tmp/zz-tapes/` and are not fixtures, since zz reads a rollout only for its metadata
  and they hold Codex's base instructions and tool definitions. To replay a tape, scrub it into
  `crates/zz-daemon/src/agent/fixtures/<provider>-<scenario>.ndjson` (a Claude Code transcript
  beside it as `<provider>-<scenario>.transcript.jsonl`), add it to `RECORDED` in
  `agent/runtime/tape_tests.rs` for an ACP provider or to `NATIVE` in
  `agent/claude/replay_tests.rs` or `agent/codex/replay_tests.rs`, and run `cargo test -p
  zz-daemon agent::`: the ACP player checks what zz writes against the tape, the Claude Code and
  Codex drivers play it through their own runtimes, a Claude Code transcript is reloaded at every
  line, and the cut-point sweeps run on all of them.
* **Every provider: a chat on a private daemon.** Build the CLI (`ZZ_DEV_BUILD=1 cargo build -p
  zz-cli`), run it with `--socket /tmp/<name>.sock`, make a pane an agent with
  `select-pane-kind -t %0 -c DIR agent <provider>`, then `agent-send -t %0 --wait --final`. Read the
  chat with `capture-pane -p`. One turn that reads, edits and runs a command covers most rows below;
  a second turn after `kill-server` and a resume covers sessions on disk.

# Quiet failures

The tables list only what breaks without an error. Everything else (a flag the CLI refuses, a
request it rejects, a binary it no longer ships) shows its stderr or an error in the pane.
"Pinned" says whether a test fails when zz's own literal changes; no test can tell when the vendor
changes.

## Claude Code

Driver: `agent/claude/` (`mod.rs` frames and flags, `translate.rs` rows, `sessions.rs` history).

| What zz relies on | Where | If it changes | Pinned |
|---|---|---|---|
| Transcript path `<config>/projects/<cwd with every non-alphanumeric as ->/<uuid>.jsonl`, with a base36 hash past 200 characters | `sessions.rs` | Resume quietly starts a new session, fork says there is nothing to fork, history is empty | Test of zz's own sanitizer |
| Transcript entries: types `user`, `assistant`, `system`, `attachment`, `custom-title`, `ai-title`; fields `uuid`, `parentUuid`, `isSidechain`, `message.id`, `toolUseResult` | `sessions.rs`, `cli_session.rs` | Replayed history skips or doubles messages; titles fall back | Hand-written |
| A turn ends on `command_lifecycle` (`command_uuid`, `state`), else on `system/session_state_changed` (asked for with `CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1`) | `mod.rs`, `turn.rs` | Both gone: the spinner never stops | Recording |
| `--replay-user-messages` echoes carry `isReplay` | `translate.rs` | Every prompt shows twice | Recording |
| A `user` frame sent while a turn runs takes `priority`: `now` is read at once and the running tool call steps aside (a shell command goes to the background), `next` is read with the next tool result and interrupts nothing. zz sends `now` for a steer the user typed and `next` for a reply or ask from another chat | `mod.rs` | `next` ignored: a chat's reply waits for the turn to end, or stops the command the host is running | 2.1.331 binary lists `now`, `next`, `later`; `bench/agents/tour` run, 2026-11-24 |
| `result.stop_reason` says how the reply ended: zz keeps `end_turn`, `max_tokens`, `refusal` and `model_context_window_exceeded` as the turn's stop reason and ignores the rest (a steered turn's `result` says `tool_use` with `terminal_reason` `aborted_tools`). The CLI retries on its own after an output limit, so `max_tokens` should only show once those retries ran out | `mod.rs` | A refused or cut-off reply reads as finished: `agent-send --wait` exits 0 and a mentioning chat shows the child as done | Recording for `end_turn` and `tool_use`; the other values from the 2.1.330 binary's schema text, never seen in a recording |
| Each `assistant` frame carries the block just streamed by `stream_event`, keyed by message id and index | `translate.rs` | Text shows twice or goes missing | Recording |
| `initialize` fields `agentProgressSummaries`, `forwardSubagentText` | `mod.rs` | Subagent progress lines and text stop | Sent in `resume.ndjson`; no progress summary or subagent text showed up in it |
| `get_settings` (`applied.model`, `applied.effort`), `get_usage` (`rate_limits`), `generate_session_title` | `mod.rs` | Model badge, quota bars and titles stop | Fakes only; never confirmed against a real CLI |
| Tool names zz special-cases: `AskUserQuestion` (answers), `ExitPlanMode` (mode switch), `TodoWrite`, `TaskCreate`, `TaskUpdate` (plan panel), `Bash` (auto-approve of `zz` commands) | `translate.rs`, `mod.rs` | A renamed question or plan tool is approved with no answers or no mode switch; other renames lose their rows | Recording for `Agent`, `Edit`, `AskUserQuestion` |
| Resuming a finished agent: `SendMessage` input `to` (the agent id), `summary`, `message`; then `task_started` with the agent's id as `task_id` and the `SendMessage` call as `tool_use_id`; the agent's frames keep the first `Agent` call as `parent_tool_use_id` and name the agent in `agent_id`; `tool_use_result.resumedAgentId`; the closing `task_notification` names the `SendMessage` call | `translate.rs` | The resumed agent shows as a plain "Send message" row and its steps are dropped | Recording (`resume.ndjson`) |
| A finished agent's report: `task_notification.handback_report.text`, and in a foreground result `tool_use_result.handbackReport.text`, or the older `tool_use_result.content`; `summary` then only says the report went elsewhere | `translate.rs` | Agent cards show that notice instead of the report | Recording (`resume.ndjson`) for the notification; foreground shapes hand-written from 2.1.330 and `tools.ndjson` |
| `can_use_tool` fields `permission_suggestions`, `requires_user_interaction`, `suppress_always_allow_rule` | `mod.rs` | Wrong auto-approve, or "Always allow" missing | Recording |
| A background agent's `can_use_tool` carries `agent_id`, the `task_id` that `background_tasks_changed` lists; an ask from the main thread has none. An agent a background agent starts asks with its own `agent_id`, which that list leaves out; its `task_started` names the starter in `parent_task_id`. Either ask can arrive after the turn's `result`, and an answer sent then still reaches the agent | `mod.rs` | The approval card goes away when the turn ends while the agent keeps waiting | Recording (`background.ndjson`, `nested.ndjson`) |
| `interrupt` ends the foreground turn only: Claude Code sends `control_cancel_request` for that turn's asks and leaves a background agent running with its ask open. A foreground agent waiting on an ask is killed (`task_updated` `status: killed`) with its ask cancelled, not moved to the background; zz never sends `background_tasks`, the request that would move it | `mod.rs` | If interrupt starts stopping background agents without a `control_cancel_request` for their asks, Stop leaves a card open that nothing waits on; if it starts moving foreground agents to the background, their asks are not marked and end with the turn | Recording (`background.ndjson`); the foreground agent seen on 2.1.330 with `--model haiku`, not kept |
| `system/init.terminal_slash_commands` | `mod.rs` | A TUI-only command shows in chat and fails | Hand-written |
| Every launch passes the picked model's `value` (from `initialize` `models`) as `--model` and the picked effort as `--effort`, after the user's own arguments, so the last of each must win | `mod.rs` | A new chat, fork, rewind, switch or restart runs another model or effort than the picker shows | Fake; checked on 2.1.330 with `get_settings` (`applied.model`, `applied.effort`), no prompt |
| A model set by name (`/model fable`, a saved launch pick) is matched to its `initialize` `models` entry by `value`, by `resolvedModel` without a `[...]` suffix, or by `displayName` without case: the CLI takes `fable` and lists that model as `claude-fable-5-2[1m]` with `displayName` `Fable` | `mod.rs` | The picker shows the typed text instead of the model's name, checks no row, and offers the first listed model's efforts and modes | Fake; the list read from `initialize` on 2.1.331, no prompt |
| Every launch after the chat first showed a mode passes that mode as `--permission-mode`, after the user's own arguments: the last one wins, and `default` is taken although `--help` lists `manual` (which `initialize` reports as `default`). The first launch passes none, so `permissions.defaultMode` in the user's settings applies | `mod.rs` | A new chat, fork, rewind, switch or restart drops the mode and the picker jumps to the settings default | Fake; checked on 2.1.330 with `initialize` `current_permission_mode`, no prompt |
| Model ids start with `claude-`; `[…]` suffixes like `[1m]`; aliases `opus`, `sonnet`, `haiku`, `fable` | `mod.rs`, `cli_session.rs` | Wrong model badge, or flip opens the TUI on another model | Hand-written |
| A transcript's assistant entries carry `requestedModel`, the model the CLI asked for with its suffix (`claude-opus-5-5[1m]`), where `message.model` is the API's id without it; the flip from the TUI reads `requestedModel` first | `cli_session.rs` | The chat a TUI flips into runs the model without `[1m]`, the smaller context | Hand-written from 2.1.304 and 2.1.309 transcripts; 2.1.330 writes the field on every assistant entry (no `[1m]` session seen on it) |
| `claude auth status --json` keys | `daemon/accounts.rs` | Every account reads as signed out | Sample JSON |
| The stored login on macOS is the Keychain generic password of service `Claude Code-credentials` (`-<first 8 hex of sha256 of CLAUDE_CONFIG_DIR>` when that is set) and account `$USER`, written with `add-generic-password -U` (on the stdin of `security -i`, or as arguments when the line is over 4094 bytes) and read with `find-generic-password -w`; on Linux `<config>/.credentials.json`. The CLI reads it when it starts | `logins.rs` | A swap writes a login the CLI never reads, or reads back nothing and refuses | Hand-written; `security find-generic-password -s "Claude Code-credentials" -a "$USER"` (no `-w`) shows the item exists |
| A stored login carries `claudeAiOauth.accountUuid` and `claudeAiOauth.organizationUuid` (written at sign-in, kept across token renewals, added to an older login after the CLI fetches the profile), and `auth status --json` gives the cached profile's `orgId` | `logins.rs`, `daemon/accounts.rs` | Gone from the login: zz takes the cached name unchecked and no longer notices a login changed outside zz. `orgId` gone: the same. Renamed ids read as a login with none | Hand-written; a login signed in on 2.1.330 carried both, read from the Keychain on 2026-11-24, and the save path in the 2.1.331 binary keeps them |
| zz unsets `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` and the account id vars for a provider it manages | `accounts.rs`, `environment.rs` | A new override variable runs a chat on another account than the active slot | Hand-written |
| Child CLIs must not act as nested sessions: zz unsets `CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, `CLAUDE_CODE_MESSAGING_SOCKET` and the rest of the list in `lib.rs` | `lib.rs` | A new variable makes a pane's CLI think it runs inside another session | None; run `env` in a Bash tool call of a nested `claude` |
| Peer registry `<config>/sessions/<pid>.json` (`sessionId`, `status`, `messagingSocketPath`) and sockets in `/tmp/cc-socks` | `claude_peers.rs`, `cli_session.rs` | Flip from the TUI opens a fresh chat; zz panes vanish from Claude Code's peer list | Hand-written |
| `settings.json` `hooks.SessionStart` runs `zz tools --context` and its stdout becomes context; `permissions.allow` gets `Bash(zz:*)` | `agent_hooks.rs` | Terminal-pane Claude loses zz's context, or asks before every `zz` command. The 2.1.292 recording already suggests rules as `Bash(cmd *)`, so the `:*` form may be on its way out | Hand-written |
| Text markers: `[Request interrupted`, `<command-`, `<local-command-`, `<task-notification>`, `<pasted_content id=…>`, `[Image #N]`, first line `Exit code N` | `translate.rs`, `sessions.rs` | Internal text shows in the chat, or cancelled turns read as finished | Hand-written |

## Codex

Driver: `agent/codex/` (`mod.rs` requests, `translate.rs` items), with `agent/codex_queue.rs` and
`agent/visualize.rs`. On the shared path zz talks to the app server daemon the TUI uses, a separate
binary under `~/.codex/packages/app-server-daemon/`, so its version is the one that counts.

| What zz relies on | Where | If it changes | Pinned |
|---|---|---|---|
| `codex app-server daemon start` and the socket `$CODEX_HOME/app-server-control/app-server-control.sock` | `codex/control.rs`, `mod.rs` | Each pane waits 20 s, then runs its own server: chat and TUI stop sharing a thread live | None |
| `codex app-server daemon restart` makes the shared app server read `auth.json` again; the login lives in `$CODEX_HOME/auth.json` (file store) | `codex/mod.rs`, `logins.rs` | After an account swap or sign-in, chats on the shared server keep the old account | None; checked `--help` on codex-cli 0.169 |
| `config/read` returns `config.developer_instructions`; zz prepends it to its context and sends both as `developerInstructions` on `thread/start` | `mod.rs` | On an error zz sends no instructions at all; if `developerInstructions` stops replacing the config's, the user's instructions arrive twice | Fake |
| `turn/start` `clientUserMessageId` comes back as `userMessage.clientId` | `mod.rs`, `translate.rs` | /rewind says the prompt is no longer in the conversation | Fake |
| `turn/steer` with `expectedTurnId` answers an error when that turn is no longer the running one (`no active turn to steer`, ``expected active turn id `X` but found `Y` ``), and `turn/start` while a turn runs joins that turn and answers its id. zz holds a steer until it knows the turn id and sends a refused one as the next prompt with `turn/start` | `mod.rs` | A steer that Codex took is sent again, or one it dropped is never sent | Fake; the errors and the join seen on 0.171.0 with a `thread/shellCommand` turn on a private app server |
| A turn keeps running when the connection that started it closes. `thread/resume` and `thread/read` give `thread.status.type` (`active` while a turn runs) and each turn's `status` (`inProgress` for the running one); a connection that resumes mid-turn gets no `turn/started`, only the turn's later events, and can `turn/interrupt` it | `mod.rs` | A chat flipped from a TUI mid-turn shows idle: no Stop, and the next prompt joins the old turn | Fake; seen on 0.171.0 with a `thread/shellCommand` turn resumed from a second connection after the first closed |
| `thread/fork` `beforeTurnId` | `mod.rs` | /rewind keeps the later turns | Fake |
| `turn/completed` (`turn.status`), `thread/status/changed` (`status.type == "active"`) | `mod.rs` | The turn never settles | Recording |
| A connection keeps getting a thread's events and requests until it sends `thread/unsubscribe`, and every request carries `threadId`. After a new chat, switch, fork or rewind zz unsubscribes the thread it left and its subagent threads, drops their events and answers their approvals with `cancel` | `mod.rs` | Without `threadId` the left thread's approvals show in the new chat; if unsubscribe stops working, zz cancels every later approval on that thread, a TUI's on the shared server included | Fake |
| Item types `userMessage`, `agentMessage`, `reasoning`, `commandExecution`, `fileChange`, `mcpToolCall`, `dynamicToolCall`, `webSearch`, `collabAgentToolCall`, `subAgentActivity` and their fields | `translate.rs` | An unknown or renamed type drops its row | Recording for most; `mcpToolCall`, `dynamicToolCall`, `webSearch` fake only |
| `thread/list` `{useStateDbOnly}` returns `data[].{id, cwd, name, updatedAt}`; `thread/read` returns turns with items | `mod.rs` | Chats vanish from the resume list; reopened chats are empty | Fake |
| `model/list` fields `model`, `displayName`, `isDefault`, `supportedReasoningEfforts` | `mod.rs` | No model or effort picker | Fake |
| Async question replies go back as a `<send_user_message_question_reply>` block | `translate.rs` | The model never sees the answers | Recording (0.166.0) |
| Rate limits: `limitId` absent or `codex`, windows of 300 and 10080 minutes | `translate.rs` | Quota bars disappear | Recorded JSON |
| `account/read` returns `account.{email, workspaceName, planType}` | `daemon/accounts.rs` | Accounts show signed out | Fake |
| Rollouts `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-*-<thread>.jsonl`: first line `session_meta`, then `turn_context` and `thread_settings_applied` | `cli_session.rs`, `codex/rollout.rs` | Flip from the TUI loses the thread, model or busy state | Fake |
| TUI argv: `resume <id>` positional, and the flags that make a TUI run its own server (`--no-daemon`, `--remote`, `--profile`, most `-c`) | `cli_session.rs` | Flip reads the wrong thread or cuts a running turn. A flag that takes a value has to be in `CODEX_VALUE_FLAGS`, or the id after `resume` is lost (`-s`, `-a` and the rest of the 0.171.0 list were added 2026-11-20) | Fake |
| `codex queue --thread --message` prints `Queued message <id> for thread <thread>.`; the TUI's pane title is `<spinner> <thread name> \| …` | `codex_queue.rs` | Peer messages get pasted into the terminal instead of queued, or report a failure after queueing | Fake |
| `$CODEX_HOME/rules/zz.rules` `prefix_rule(pattern = ["zz"], decision = "allow")`; `hooks.json` `SessionStart` | `agent_hooks.rs` | `zz` commands ask for approval or run sandboxed; terminal-pane Codex loses zz's context | Fake |
| Account homes link `sessions`, `session_index.jsonl`, `config.toml`, `skills`, `hooks.json`, the `*.sqlite` state files and the rest of the list in `accounts.rs`; zz unsets `CODEX_ACCESS_TOKEN`, `OPENAI_FEDERATION_RULE_ID`, `OPENAI_IDENTITY_TOKEN_FILE` | `accounts.rs` | Threads go missing in other accounts; a new override variable signs a pane into the wrong account. `~/.codex/sqlite/` is new and not linked | Fake; diff `ls -A ~/.codex` against the list |
| Visualize references `\u{E200}visualize\u{E202}{"path":…}\u{E201}` in agent text | `visualize.rs` | The raw characters show in the chat | Fake |
| The title helper picks a model whose id contains `luna`, `nano`, `mini` or `small` | `mod.rs` | Titles are generated on the chat's own, slower model | None |

Recordings label direction differently: in `tools.ndjson` and `subagent.ndjson` `"dir":"in"` is zz
to Codex, in `app-server-live.jsonl` `"out"` is.

## Antigravity

Driver: `agent/agy.rs` on the shared ACP runtime (`dialect.rs`, `runtime.rs`).

| What zz relies on | Where | If it changes | Pinned |
|---|---|---|---|
| Config option id `thought_level` is effort | `agy.rs` | No effort picker; flip drops `--effort` | Test uses its own copy |
| Option id `model` | `dialect.rs`, `zz-client/src/agent_settings.rs` | No model and mode lens at all | None |
| `session_info_update.title` | `dialect.rs` | Untitled chats (agy has no stored title zz reads) | Grok-shaped test |
| A user-facing page write: `tool_call` with `rawInput.ArtifactMetadata.UserFacing: true` and `rawInput.Path` (else `locations[0].path`). | `fanout.rs` | The page pane no longer opens by itself; the chat row still works | Test |
| Mode ids `default`, `accept-edits`, `auto`, `plan` | `agent_settings.rs` | Wrong risk badge on the mode | None |
| The TUI keeps `…/conversations/<uuid>.db` open | `cli_session.rs` | Flip from the TUI opens a fresh chat | Test |
| The signed-out error starts with `Authentication required` | `dialect.rs` | The raw error shows instead of the sign-in hint. Not confirmed that agy sends this text | Grok-shaped test |
| `agy acp` `session/list` answers `-32000` (`Authentication required`) when signed out and a result when signed in; `agy acp --login` with `NO_BROWSER=1` prints the Google OAuth URL on stdout, reads the pasted code or redirect URL on stdin, and exits 0 on success | `daemon/accounts.rs`, `daemon/agent_login.rs` | Account status reads signed out or login stops parsing the link | Fake CLI test; checked on 1.5.14 |
| The latest release is `version` in `<updater>/manifests/<os>_<arch>.json` (`darwin`, `linux`, `windows`; `amd64`, `arm64`), the manifest agy's updater and install script read | `daemon/providers.rs` | Settings shows the installed version with no check and no Update | Test |

## Grok Build

Driver: `agent/grok.rs` on the shared ACP runtime.

| What zz relies on | Where | If it changes | Pinned |
|---|---|---|---|
| `session/new` `_meta.rules` is appended to the system prompt; zz puts its workspace context there. | `grok.rs`, `dialect.rs` | Grok no longer knows `zz html` or the rest of the context | None; ask a new chat what its rules say |
| Config option id `reasoning_effort` is effort | `grok.rs` | No effort picker | Test uses its own copy |
| `_x.ai/billing` answers `config.creditUsagePercent`, `config.currentPeriod.{type,end}` | `grok.rs` | No quota; if Grok stops answering, each turn's command loop waits 30 s | Shape tested, method name not |
| `$GROK_HOME/sessions/*/<uuid>/summary.json` keys `generated_title`, `session_summary`, `current_model_id`, `reasoning_effort` | `grok.rs`, `cli_session.rs` | Untitled resumed chats; flip opens on the wrong model | None |
| `$GROK_HOME/active_sessions.json`, an array of `{session_id, pid}` | `cli_session.rs` | Flip from the TUI opens a fresh chat | Test |
| The signed-out error starts with `Authentication required` | `dialect.rs` | Raw error instead of the sign-in hint | Test |
| `grok agent stdio` `_x.ai/billing` answers `-32000` (`Authentication required`) when signed out and `subscription_tier` + `config` when signed in; `grok login --oauth` prints the OAuth URL on stderr and opens the browser itself; `grok login --device-auth` prints the device URL (`?user_code=…`) and code on stderr and exits 0 on success | `daemon/accounts.rs`, `daemon/agent_login.rs` | Account status reads signed out or login stops parsing the link or device code | Fake CLI test; checked on 1.0.92 |
| `grok update --check --json` prints `latestVersion` | `daemon/providers.rs` | Settings shows the installed version with no check and no Update | Test |

## Cursor

Driver: `agent/cursor.rs` on the shared ACP runtime. Cursor has no flip.

| What zz relies on | Where | If it changes | Pinned |
|---|---|---|---|
| `NO_OPEN_BROWSER=1` makes a signed-out `authenticate` fail at once | `cursor.rs` | A browser opens on every start, then a 30 s wait | None |
| Model values are `slug[key=value,…]` with `effort`, `reasoning_effort` or `reasoning` and `fast` | `cursor.rs`, `dialect.rs` | Raw model names in the picker; a bare `--model` may be refused | Test |
| Mode option `mode` with `agent`, `plan`, `ask`; `~/.cursor/cli-config.json` `approvalMode` | `cursor.rs`, `dialect.rs` | No "Run everything" choice, wrong badges | Mechanism tested, values not |
| `~/.cursor/acp-sessions/<id>/meta.json` key `title` | `cursor.rs` | Untitled resumed chats | None |
| `cursor/ask_question` and `cursor/create_plan` get "method not found" | `runtime.rs` | Unknown: what Cursor does then has not been seen | None |
| `cursor-agent about --format json` prints `latestVersion` | `daemon/providers.rs` | Settings shows the installed version with no check and no Update | Test |
| `cursor-agent status --format json` (with `CURSOR_AGENT_CLI_ASSUME_SSH=0`, optionally preceded by a tip line before `{`) prints `isAuthenticated` and `userInfo.email`; `cursor-agent login` with `NO_OPEN_BROWSER=1` and `CURSOR_AGENT_CLI_ASSUME_SSH=0` prints the `loginDeepControl` URL and exits 0 on success | `daemon/accounts.rs`, `daemon/agent_login.rs` | Account status reads signed out or login stops parsing the link | Fake CLI test; checked on 2026.11.19 |
| `CURSOR_AGENT_CLI_ASSUME_SSH=0` skips the keychain probe every command runs first when `SSH_CONNECTION`, `SSH_CLIENT` or `SSH_TTY` is set | `daemon/providers.rs` | From a daemon started over ssh with the keychain locked, the version, the latest check and Update all exit 1 with "Your macOS login keychain is locked" | Test fakes the probe |

## Every ACP provider

zz speaks ACP v1 through `agent-client-protocol` 5.0.0 with schema 1.15.1 (`Cargo.lock`).

| What zz relies on | Where | If it changes |
|---|---|---|
| `agentCapabilities.loadSession` | `runtime.rs` | Reopening a chat starts a fresh agent session and replays zz's journal, so the agent forgets the conversation |
| `session/load` failing falls back to `session/new` | `runtime.rs` | Same as above, with a notice in the chat |
| `session/list` entries carry an absolute `cwd` that exists | `runtime.rs` | Entries are dropped from the resume list |
| After `session/cancel` the agent sends what is left of the turn, then answers the pending `session/prompt` | `runtime.rs` | Stop leaves the chat stopping for 5 s (`CANCEL_GRACE`) before the turn ends, and updates sent after that show in the next turn. Checked 2026-11-21 with one short prompt each, cancelled 0.05 to 0.33 s after it was sent, before any reply text: agy 1.5.13 (`gemini-3.8-flash`) answered `cancelled` 0.17 s after `session/cancel`, grok 1.0.92 (`grok-4.7-build-fast`) 0.008 s, cursor-agent 2026.11.19 (`gpt-5.4-nano`) 0.006 s. A cancel during a running tool call was not tried |
| `sessionUpdate` kinds and shapes match schema 1.15.1 | `chat_bridge.rs`, `acp_chat.rs` | A new kind or a reshaped one fails to parse and the whole update is dropped. Bump the crate when a provider moves to a newer schema |
| Tool call fields `toolCallId`, `title`, `kind`, `status`, `content[].content.text`, `locations[0].path`, `rawInput.{command,pattern,url,query,prompt}` | `chat_bridge.rs` | Rows lose their target or text |

# Known gaps

* The tape recorder (`ZZ_AGENT_TAPE_DIR`, `bench/agents/tapes/`) records any provider, but its
  rig only drives the ACP providers so far. Only two of the
  nine steps in `app-server-live.jsonl` feed a test, and the `rollout-lens-stress` files feed none.
* Only `resume.ndjson` carries the two `initialize` fields above and `CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES`
  output; no recording has `get_settings`/`get_usage` replies.
* The ACP effort option ids are copied into the dialect tests, so changing `agy.rs` or `grok.rs`
  breaks no test.
* Signed-out detection for the ACP providers reads error text, not the error code.
