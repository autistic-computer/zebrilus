---
type: Reference
title: Agent CLI contracts
description: What zz relies on from the agent CLIs it drives (Claude Code, Codex, Antigravity, Grok Build, Cursor) that the vendor can change without notice, the version each was last checked at, which changes break zz quietly, and the commands that recheck them after an update.
resource: crates/zz-daemon/src/agent/
tags: [agent, claude-code, codex, antigravity, grok, cursor, acp, compatibility, reference]
timestamp: 2026-11-20T00:00:00-03:00
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
| Claude Code (`claude`) | 2.1.329 | 2.1.292, 2026-10-07 | Full: the fixtures in `agent/claude/fixtures` were recorded on it. 2.1.329 passed the flag probe, the binary grep and the `auth status` keys below, nothing live |
| Codex (`codex`) | 0.171.0 (shared daemon 0.171.0) | 0.159.0, 2026-10-07 | Full: `tools.ndjson` and `subagent.ndjson` in `agent/codex/fixtures`; `app-server-live.jsonl` is 0.166.0. 0.171.0 had the schema check and a binary grep, all names present |
| Antigravity (`agy`) | 1.5.13 | 1.5.13, 2026-11-20 | Live turns through a private daemon, including the artifact signal |
| Grok Build (`grok`) | 1.0.89 | 1.0.89, 2026-11-20 | Live turns through a private daemon, including `_meta.rules` |
| Cursor (`cursor-agent`) | 2026.11.19 | 2026.09.18, 2026-11-20 | Live turns on 2026.09.18. 2026.11.19 (it updated itself that morning) passed the initialize probe, nothing live |

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
names the one that went away. Add `--fork-session`, `--session-id`, `--append-system-prompt` and
`--effort` the same way when touching those paths.

**Claude Code control requests and env vars.** The binary carries the names it handles; a count of
0 means the name is gone:

```bash
B="$(readlink -f "$(command -v claude)")"; for s in can_use_tool command_lifecycle session_state_changed get_settings get_usage generate_session_title side_question apply_flag_settings stop_task agentProgressSummaries forwardSubagentText terminal_slash_commands CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES claude_authenticate claude_oauth_callback claude_oauth_wait_for_completion; do printf '%s=%s\n' "$s" "$(grep -a -c "$s" "$B")"; done
```

**Claude Code accounts.** `claude auth status --json | jq -c keys` must still list `loggedIn`,
`email`, `orgName`, `planTier` (or `subscriptionType`) and `configDirectory`.

**Codex protocol.** Codex prints its own app-server schema. Check the names zz sends and reads
against it (`--experimental`, because zz asks for the experimental API):

```bash
D=$(mktemp -d); codex app-server generate-json-schema --experimental --out "$D"; for s in clientUserMessageId beforeTurnId developerInstructions useStateDbOnly excludeTurns subAgentActivity collabAgentToolCall thread/backgroundTerminals/list item/tool/requestUserInput item/permissions/requestApproval turn/steer review/start thread/compact/start account/rateLimits/read; do printf '%s=%s\n' "$s" "$(rg -l -F "$s" "$D" | wc -l)"; done
```

`jq '.properties | keys' "$D/v2/ThreadForkParams.json"` (or any other `*Params.json`) shows a
request's fields. `codex app-server daemon version` prints the shared daemon's version, which can
differ from `codex --version`. `codex --help`, `codex resume --help` and `codex queue --help` show
new flags; a new flag that takes a value has to join the list `cli_session.rs` skips.

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

**ACP TUI flags for flip-agent.** `agy --help` must list `--conversation`, `--model`, `--effort`;
`grok --help` must list `--resume`, `--model`, `--effort`.

# Live checks, signed in

Run these when a cheap check shows a change, before a release, or after a version jump of more than
a few releases.

* **Claude Code and Codex: replay recordings.** Record a new session with the driver's frames
  (`{t,dir,msg}` per line, like the files in `agent/claude/fixtures` and `agent/codex/fixtures`),
  then run `cargo test -p zz-daemon agent::claude` (or `agent::codex`) and
  `ZZ_REPLAY_DIR=<dir> cargo test -p zz-daemon -- --ignored recordings_in_a_folder_drive_the_chat_machine`.
  The repo has no recorder yet; the 2026-10-07 recordings came from an outside harness.
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
| Each `assistant` frame carries the block just streamed by `stream_event`, keyed by message id and index | `translate.rs` | Text shows twice or goes missing | Recording |
| `initialize` fields `agentProgressSummaries`, `forwardSubagentText` | `mod.rs` | Subagent progress lines and text stop | Not recorded |
| `get_settings` (`applied.model`, `applied.effort`), `get_usage` (`rate_limits`), `generate_session_title` | `mod.rs` | Model badge, quota bars and titles stop | Fakes only; never confirmed against a real CLI |
| Tool names zz special-cases: `AskUserQuestion` (answers), `ExitPlanMode` (mode switch), `TodoWrite`, `TaskCreate`, `TaskUpdate` (plan panel), `Bash` (auto-approve of `zz` commands) | `translate.rs`, `mod.rs` | A renamed question or plan tool is approved with no answers or no mode switch; other renames lose their rows | Recording for `Agent`, `Edit`, `AskUserQuestion` |
| `can_use_tool` fields `permission_suggestions`, `requires_user_interaction`, `suppress_always_allow_rule` | `mod.rs` | Wrong auto-approve, or "Always allow" missing | Recording |
| `system/init.terminal_slash_commands` | `mod.rs` | A TUI-only command shows in chat and fails | Hand-written |
| Model ids start with `claude-`; `[…]` suffixes like `[1m]`; aliases `opus`, `sonnet`, `haiku`, `fable` | `mod.rs`, `cli_session.rs` | Wrong model badge, or flip opens the TUI on another model | Hand-written |
| `claude auth status --json` keys | `daemon/accounts.rs` | Every account reads as signed out | Sample JSON |
| `CLAUDE_CONFIG_DIR` isolates credentials (keychain entry included) and `.claude.json`; zz unsets `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN` and the account id vars | `accounts.rs`, `environment.rs` | A new override variable signs a pane into the wrong account | Hand-written |
| Account homes link these from `~/.claude`: `projects`, `settings.json`, `skills`, `agents`, `commands`, `plugins`, `sessions`, `plans`, `todos`, `tasks` and the rest of the list in `accounts.rs` | `accounts.rs` | New state Claude Code adds to `~/.claude` is missing in other accounts | Hand-written; diff `ls -A ~/.claude` against the list |
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
| `config/read` returns `config.developer_instructions`; zz prepends it to its context and sends both as `developerInstructions` on `thread/start` | `mod.rs` | On an error zz sends no instructions at all; if `developerInstructions` stops replacing the config's, the user's instructions arrive twice | Fake |
| `turn/start` `clientUserMessageId` comes back as `userMessage.clientId` | `mod.rs`, `translate.rs` | /rewind says the prompt is no longer in the conversation | Fake |
| `thread/fork` `beforeTurnId` | `mod.rs` | /rewind keeps the later turns | Fake |
| `turn/completed` (`turn.status`), `thread/status/changed` (`status.type == "active"`) | `mod.rs` | The turn never settles | Recording |
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

## Cursor

Driver: `agent/cursor.rs` on the shared ACP runtime. Cursor has no flip.

| What zz relies on | Where | If it changes | Pinned |
|---|---|---|---|
| `NO_OPEN_BROWSER=1` makes a signed-out `authenticate` fail at once | `cursor.rs` | A browser opens on every start, then a 30 s wait | None |
| Model values are `slug[key=value,…]` with `effort`, `reasoning_effort` or `reasoning` and `fast` | `cursor.rs`, `dialect.rs` | Raw model names in the picker; a bare `--model` may be refused | Test |
| Mode option `mode` with `agent`, `plan`, `ask`; `~/.cursor/cli-config.json` `approvalMode` | `cursor.rs`, `dialect.rs` | No "Run everything" choice, wrong badges | Mechanism tested, values not |
| `~/.cursor/acp-sessions/<id>/meta.json` key `title` | `cursor.rs` | Untitled resumed chats | None |
| `cursor/ask_question` and `cursor/create_plan` get "method not found" | `runtime.rs` | Unknown: what Cursor does then has not been seen | None |

## Every ACP provider

zz speaks ACP v1 through `agent-client-protocol` 5.0.0 with schema 1.15.1 (`Cargo.lock`).

| What zz relies on | Where | If it changes |
|---|---|---|
| `agentCapabilities.loadSession` | `runtime.rs` | Reopening a chat starts a fresh agent session and replays zz's journal, so the agent forgets the conversation |
| `session/load` failing falls back to `session/new` | `runtime.rs` | Same as above, with no error |
| `session/list` entries carry an absolute `cwd` that exists | `runtime.rs` | Entries are dropped from the resume list |
| `sessionUpdate` kinds and shapes match schema 1.15.1 | `chat_bridge.rs`, `acp_chat.rs` | A new kind or a reshaped one fails to parse and the whole update is dropped. Bump the crate when a provider moves to a newer schema |
| Tool call fields `toolCallId`, `title`, `kind`, `status`, `content[].content.text`, `locations[0].path`, `rawInput.{command,pattern,url,query,prompt}` | `chat_bridge.rs` | Rows lose their target or text |

# Known gaps

* No recorder for Claude Code or Codex sessions, so a full recheck needs an outside harness. Only
  two of the nine steps in `app-server-live.jsonl` feed a test, and the `rollout-lens-stress`
  files feed none.
* The Claude Code recordings predate four things the driver uses now: the two `initialize` fields
  above, `CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES` output, and `get_settings`/`get_usage` replies.
* The ACP effort option ids are copied into the dialect tests, so changing `agy.rs` or `grok.rs`
  breaks no test.
* Signed-out detection for the ACP providers reads error text, not the error code.
