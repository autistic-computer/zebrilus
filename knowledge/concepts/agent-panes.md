---
type: Concept
title: Agent panes and the flip
description: How a pane runs Claude Code or Codex as a chat (PaneKind::Agent over the vendor drivers), how a chat and its window are named after the CLI's session title, how flip-agent switches one pane between the CLI's own TUI and that chat on the same session (busy rules, --now, --when-idle and the armed flag, finding the TUI's session, waiting for the old process), forks and side chats, accounts (a CLI login per pane through CLAUDE_CONFIG_DIR or CODEX_HOME, agent-account, and agent-accounts to list who is signed in and add one, the 5-hour and weekly rate limits each account reports, and the composer's usage pill that shows the context and those limits and switches the account), Codex on its shared app server, the chat's slash list, agent-send and agent-respond, what the chat draws, the preview cards and the floating view pane they open, and how its stream reaches clients.
resource: crates/zz-daemon/src/daemon/agent_flip.rs
tags: [agent, agent-pane, flip, chat, claude-code, codex, driver, stream]
timestamp: 2026-11-11T00:00:00-03:00
---

# Overview

A pane runs Claude Code or Codex one of two ways, and one button switches between them:

- **The CLI.** An ordinary terminal pane running the vendor's own TUI. zz draws the terminal and
  types nothing into it.
- **The chat.** An agent pane (`PaneKind::Agent`) whose daemon runs the CLI's headless protocol
  through the native drivers (Claude Code stream-json, the Codex app server) and streams the
  conversation to clients, which draw it as a chat.

`flip-agent` turns one into the other on the same session: it stops the running process and
starts the other side resuming that session, in the same pane id and layout place. The CLIs' own
screen-only commands (`/config`, `/status`, `/mcp` and so on) are not offered in the chat; to use
one, flip to the CLI.

The 2026-10-23 [agent lens](/designs/agent-lens.md), a chat drawn over a terminal running the TUI,
was replaced by this on 2026-11-01 and its runtime deleted. The drivers, the
[turn machine](/designs/agent-turn-machine.md), the chat machine and the chat UI it used carried
over.

# Agent panes

`PaneKindSnapshot::Agent(AgentDescriptor)` (wire 118) is the pane kind:

| Field | Meaning |
|---|---|
| `provider` | `ClaudeCode` or `Codex` |
| `cwd` | the folder the driver runs in; filled from `-c`, else the terminal or donor pane's folder, else the daemon's |
| `session_id` | the vendor session to resume; the driver's session goes back into it, so a `/clear` or a fork shows here |
| `model`, `effort` | what the driver is set to once its session is ready (`SetConfigOption` after `SessionReady`) |
| `account` | the [account](#accounts) the pane runs on (wire 122); `None` is the CLI's own login |

**Opening one.** `select-pane-kind [-t pane] [-c dir] [--resume id] [--model m] [--effort e]
[--account id] agent <claude|codex>` (`--resume id` may also follow the provider) turns a picker, or a terminal,
into an agent pane. A terminal keeps its pane id and place: its process is stopped and the driver
starts once it has exited (`AGENT_CLI_EXIT_GRACE`, 5 s). `select-pane-kind [-c dir] terminal
[command ...]` turns an agent pane back into a terminal (the mux emits `PaneRespawned`), closing
its driver. Both conversions are `MuxState::convert_pane`; a pinned title survives them.

**Running it.** The daemon opens the pane's runtime (`Shared::open_agent_pane`, `agent_pane_spec`)
with the descriptor's provider, folder, session, model and effort and the pane's workspace
environment (`ZZ_PANE`, `TMUX`, `TMUX_PANE`). A resumed Claude session replays its transcript
before the driver says it is ready; a resumed Codex thread comes back through `thread/resume`.
Closing the pane closes the driver. Agent messages (`AgentReplay`, `AgentPrompt`, approvals,
settings) are accepted only for agent panes, and `AgentState` and the stream follow
[How the stream reaches a client](#how-the-stream-reaches-a-client).

**Forks and side chats.** `--fork` (with `--resume`) opens the pane on a copy of the session, and
`--at <message>` copies it only through that message's turn (`crate::agent_fork`, which
`zz tools --fork-chat` uses too): a Claude Code fork copies the transcript under a new session id,
a Codex fork asks the app server for `thread/fork` with `lastTurnId`. The copy's id becomes the
pane's session. Clients open a fork or side chat in a new pane with one command line, since a new
window's or float's pane becomes the target of the next command:

- Fork from a message: `new-window -c <dir> ; select-pane-kind --resume <session> --fork --at
  <message> [--account <id>] agent <claude|codex>`
- Side chat in a PiP: `new-pane -c <dir> ; select-pane-kind --resume <session> --fork
  [--account <id>] agent <claude|codex>`

Both carry the source pane's account (`zz_client::agent_settings::pane_account`), so the copy runs
on the same login.

# Chat titles

A chat is named after its session, the name each CLI gives its own sessions.

- **Pane and window.** The pane title is the session title, and `agent` until one is known. The
  driver reports it as `AgentStreamPayload::TitleChanged`, which the host keeps out of the stream:
  it lands in `AgentPaneState.title`, goes out as `AgentPaneWire.title`, and the daemon sets the
  pane title from it (`apply_agent_title`, `MuxState::update_agent_title`). A title from the CLI
  replaces one set with `select-pane -T`, as a program's OSC title does, and `allow-set-title off`
  keeps the pane's own. With the default `automatic-rename-format`, a window whose active pane is a
  chat takes the chat's title as its name (`refresh_automatic_window_name_for_pane`), so
  `list-windows`, the status bar, the chat sidebar, the session tree and the phone overview all read
  it; a window the user renamed keeps its name. `pane-title-changed` and `window-renamed` fire. No
  wire change: the pane title and `AgentPaneWire.title` were already on the wire.
- **Claude Code.** On start or resume the driver reads the transcript's last `customTitle` (a
  rename), else its last `aiTitle` (`claude::sessions::title`, the head and tail reads the session
  list uses); a fork or rewind takes the source session's. After the first prompt of an untitled
  session (not a `/` command) it sends the `generate_session_title` control request with the prompt
  as the description and `persist: true`, so the CLI saves the title in the session as well. The
  turn does not wait on it. A `/rename` comes back as `system` / `session_title_changed`, and a
  `/clear` starts untitled.
- **Codex.** The thread's `name` from the `thread/start`, `thread/resume` or `thread/fork` answer; a
  fork or rewind keeps the chat's title when the copy has none. After the first prompt of an unnamed
  thread the driver starts an ephemeral thread (`ephemeral: true`, read-only sandbox, approvals
  never) on the first listed model whose id contains luna, nano, mini or small (else the chat's
  model), at low effort when that model offers it, asks it for a short title for the prompt, saves it
  with `thread/name/set` so the user's other Codex clients show it, and unsubscribes from the helper.
  An ephemeral thread is never saved, so the helper stays out of thread lists, though the shared app
  server announces it with `thread/started` to connected clients while it runs.
  `thread/name/updated` follows renames from any client, the TUI's `/rename` included.
- **Flips** keep the title: the chat resumes the session and reads its saved name, and the CLI shows
  its own.

# The flip

`flip-agent [-t pane] [--now | --when-idle | --cancel]` (`crates/zz-daemon/src/daemon/agent_flip.rs`).

- **CLI to chat.** On a terminal whose process list shows `claude` or `codex` (`cli_provider`), the
  daemon finds the session the TUI is on, its folder and its model and effort
  ([below](#finding-the-tuis-session)), and runs `select-pane-kind -t <pane> -c <dir> --resume
  <id> --model <m> --effort <e> agent <cli>`. The TUI is stopped and the driver starts once both
  the terminal and the CLI process have exited, so two processes never hold the session. A TUI
  that has not sent its first prompt has no session yet, and the chat starts a new one.
- **Chat to CLI.** On an agent pane, the daemon closes the driver and waits for it and its process
  to exit (`AgentRuntime::close_and_wait`), then runs `select-pane-kind -t <pane> -c <dir>
  terminal /bin/sh -c 'exec "${SHELL:-/bin/sh}" -ic ...'` with `claude --resume <id> --model <m>
  --effort <e>` or `codex resume <id>` (`cli_session::cli_command`, `interactive_shell`), through
  the user's interactive shell so their aliases apply. Model and effort come from the driver's
  current config options, else the descriptor. Codex takes its model and effort from the thread,
  and `-c` would cut the TUI off the shared app server, so `codex resume` gets no flags.
- **Running work.** Something running refuses the flip with an error that starts with `busy:` and
  says what runs, for example `busy: a turn and 2 background tasks`:
  - a chat: a running turn or permission (`phase`), background tasks and subagents (`tasks`), and
    queued prompts (`cli_session::chat_running`);
  - a Claude TUI: its session record's `status` `busy`, `shell` (a background shell) or `waiting`.
  - Codex on the shared app server never refuses, since the turn and its background terminals
    live in the server and the other side rejoins them. A Codex TUI running embedded (started with
    `-c` overrides, `--profile`, `--no-daemon` and the like, or holding its rollout open) refuses
    like Claude, reading its rollout's last `task_started` and `task_complete`.
- **`--now`** flips anyway: the turn and every background shell and subagent of a Claude session
  stop with the process.
- **`--when-idle`** arms the flip. The pane's `PaneSummary.flip_armed` turns on (wire 118, in the
  snapshot and `TreeOp::PaneSummary`), and a watcher thread checks every 250 ms and flips once
  nothing runs, then clears it. **`--cancel`** clears it. An armed pane that closes or stops running
  the CLI drops the flip.
- The pane keeps its id and place. `inspect` lists `flip-agent` among the verbs of an agent pane
  and of a terminal running either CLI.
- Both directions keep the pane's [account](#accounts).

# Accounts

A pane can run on an account: a directory zz keeps per provider,
`<data>/zz/accounts/claude/<id>` or `<data>/zz/accounts/codex/<id>` (`zz-dev` for dev builds;
`<data>` is `~/Library/Application Support` on macOS and `$XDG_DATA_HOME` or `~/.local/share` on
Linux), created 0700 (`crates/zz-daemon/src/agent/accounts.rs`). Ids are
`[a-z0-9][a-z0-9_-]{0,31}`. Everything the daemon runs for the pane gets `CLAUDE_CONFIG_DIR` or
`CODEX_HOME` set to it, and the CLI keeps that account's login there. A pane with no account
leaves both variables as the daemon has them, since `CLAUDE_CONFIG_DIR=~/.claude` set explicitly
picks a different Keychain item than leaving it unset. zz never reads or writes tokens.

- **Shared by link.** An account directory links everything but the login into the default
  directory (`~/.claude`, or `$CODEX_HOME` or `~/.codex`, as `accounts::default_home` resolves
  them), so sessions, settings, skills and the Codex state databases are one set for every
  account, and the daemon's readers (session lists, titles, transcript replay, the Claude peer
  registry, `agent_fork`) keep reading the default directory. Claude Code links `projects`,
  `settings.json`, `settings.local.json`, `skills`, `agents`, `commands`, `output-styles`,
  `plugins`, `history.jsonl`, `file-history`, `session-env`, `plans`, `paste-cache`, `uploads`,
  `todos`, `tasks`, `sessions`, `teams`, `dev-mods`, `scripts`, `CLAUDE.md` and
  `keybindings.json`. Codex links `sessions`, `archived_sessions`, `session_index.jsonl` and its
  `.lock`, `thread-writer-locks`, `rollout-migrations`, `.sqlite-maintenance.lock`, `config.toml`,
  `AGENTS.md`, `skills`, `agents`, `plugins`, `hooks.json`, `keybindings.json`, `history.jsonl`,
  `attachments` and `worktrees`, and every `state_*`, `thread_history_*`, `goals_*` and `queue_*`
  `.sqlite` file (never its `-wal`, `-shm` or `-journal`). Logins (`.credentials.json`,
  `.claude.json`, `auth.json`), caches, logs, memories and the Codex daemon files stay per account.
  `accounts::link` runs when an account is created and before every spawn on it, adds only the
  links that are missing, and never replaces or removes an entry.
- **Seed.** A new Claude Code account gets `.claude.json` from the global config
  (`~/.claude.json`, or `.claude.json` in the daemon's `CLAUDE_CONFIG_DIR`): `hasCompletedOnboarding`,
  `mcpServers` and each project's `hasTrustDialogAccepted`, nothing else, so the TUI skips
  onboarding and the trust dialog. A missing or broken global config seeds
  `{"hasCompletedOnboarding": true}`.
- **Spawn.** `agent_pane_spec` puts the descriptor's account (`accounts::Account`: the id and its
  resolved directory) on the pane's `AgentWorkspaceEnvironment`, outside `entries()`, which also
  feeds Codex's `thread_config`. Before spawning, `accounts::prepare` checks the directory and
  links it. The Claude Code driver sets `CLAUDE_CONFIG_DIR` beside the workspace entries, so a
  configured command's own environment still wins. The Codex driver starts the app server daemon
  for the account with `CODEX_HOME` set and connects to
  `<dir>/app-server-control/app-server-control.sock`, one daemon per home; its per-pane
  `codex app-server` fallback gets `CODEX_HOME` too. An ACP adapter gets the variable unless its
  command sets it. A named account whose directory is missing fails the pane with
  `Claude Code account a1 has no directory at <dir>; ...` and runs nothing on another login.
- **No login from the environment.** A credential in the environment comes before the login in
  the config directory, so a daemon started with `ANTHROPIC_API_KEY` set would run every account
  pane on that key. Everything that runs for an account (the driver, the app server daemon and its
  fallback, an ACP adapter, the identity probe, the flip's CLI and the login window) goes without
  `accounts::login_overrides`: for Claude Code `ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_API_KEY`,
  `CLAUDE_CODE_OAUTH_TOKEN`, and `CLAUDE_CODE_USER_EMAIL`, `CLAUDE_CODE_ACCOUNT_UUID` and
  `CLAUDE_CODE_ORGANIZATION_UUID`, which a daemon started inside a hosted session inherits and the
  CLI writes into a fresh account's `.claude.json`; for Codex `CODEX_ACCESS_TOKEN`,
  `OPENAI_FEDERATION_RULE_ID` and `OPENAI_IDENTITY_TOKEN_FILE`, each of which wins over
  `auth.json` (codex-cli 0.167: `login status` and the app server's `account/read` follow them),
  where `OPENAI_API_KEY` and `CODEX_API_KEY` do not. `accounts::run_on` removes them from a
  `Command` before the command's own environment applies, so a configured value still wins; an
  ACP adapter runs under `/usr/bin/env -u ...` for each one its command does not set; the shell
  lines start with `unset ...;`. The default account keeps the daemon's environment as it is.
- **Flips.** Chat to CLI runs the CLI as `unset ...; CLAUDE_CONFIG_DIR=<dir> claude --resume ...`
  (or `CODEX_HOME=<dir> codex resume ...`) inside the user's interactive shell
  (`cli_session::interactive_shell`), so the variable reaches the TUI after the shell's startup
  files and the user's aliases still apply. CLI to chat reads the variable from the TUI's process (`process_info::environment`:
  `/proc/<pid>/environ` on Linux, the strings after argv in `KERN_PROCARGS2` on macOS) and, when it
  names an account directory (`accounts::account_of`), adds `--account`. macOS shows no environment
  for Apple-signed binaries, which the CLIs are not. A chat whose account directory is missing
  refuses to flip to the CLI.
- **Switching a live chat.** `agent-account [-t pane] <id|default>`
  (`crates/zz-daemon/src/daemon/agent_account.rs`). Anything running fails it with an error that
  starts with `busy:`, using the flip's `chat_running`, which for Codex also counts a running turn,
  since that turn lives in the old home's app server. Otherwise the daemon closes the driver and
  waits (`AgentRuntime::close_for_reopen`), sets the account (`MuxState::update_agent`), and opens
  the pane again on its session. The reopened stream carries on from the old one's sequence
  numbers, so a client keeps its cursor and takes the resume as a session reset.
- **`default`** means no account everywhere: `agent-account default` and `select-pane-kind
  --account default` both run the CLI's own login, and `valid_agent_account` refuses `default`, so
  a directory of that name is never listed, created or flipped into.
- **In the chat.** The composer footer's usage pill shows the account's limits and switches the
  account (see [the footer](#the-empty-chat-and-the-footer)).
- **Rate limits.** What each account has used of its 5-hour and weekly limits; see
  [below](#rate-limits).
- **Not built yet.** Removing an account: a signed-out account directory is reused by the next add
  instead.

## Listing and adding accounts

`agent-accounts [--add claude|codex]` (`crates/zz-daemon/src/daemon/accounts.rs`, internal, like
`providers`) answers on the client event stream with `EventPayload::AgentAccounts(Vec<AgentAccount>)`
(wire 122), never on stdout.

- **The list.** One `AgentAccount { provider, id, signed_in, email, organization, plan }` per
  account of each CLI installed on the daemon's host (found as `providers` finds it: the login-shell
  PATH plus `~/.local/bin`). `id: None` is the CLI's own login and always comes first for its
  provider, signed in or not; after it come the accounts under `accounts/<cli>/` that are signed in,
  in id order (`accounts::ids`: a2 before a10). A signed-out account is left out, and a CLI that is
  not installed has no rows.
- **Who is signed in.** Each account is asked in its own thread with the account's variable set,
  and the default with the variable left as the daemon has it; zz reads no token. Claude Code runs
  `claude auth status --json` and reads its stdout only: `loggedIn`, `email`, `orgName`, and for
  `plan` the `planTier` (`max_20x`) when present, else `subscriptionType` (`max`); it exits 1 when
  signed out and still prints the JSON. Codex starts `codex app-server` on stdio, sends
  `initialize`, `initialized` and `account/read` with `refreshToken: false`
  (`agent_fork::codex_request`, the fork's handshake), and is signed in when `account` is not null:
  `email`, `workspaceName` as the organization and `planType` as the plan; an API key login is
  signed in with no email or plan. The server's stdin is closed, it gets 2 s to exit, and then its
  process group is killed. Both probes drop the parent Claude Code session variables and `ZZ_PANE`,
  `ZZ_SESSION`, `TMUX` and `TMUX_PANE` (and for an account the login overrides above), give up
  after 10 s, and count as signed out when they fail.
  An account directory is linked (`accounts::prepare`) before its probe, so a Codex home has its
  state databases before its first app server.
- **Watching.** A plain call adds the client to the watchers, sends the list the daemon already has
  (nothing before the first probe ends), and refreshes in a worker thread; a call during a refresh
  asks for one more pass. The whole list goes to every watcher still connected when it changes.
  Every plain call refreshes, which is how a login that finished shows up: the daemon does not
  follow the login window.
- **Adding.** `--add` subscribes the caller too, then in a worker thread asks that CLI's accounts,
  picks the first account that is not signed in, else the lowest `aN` not taken (`a1`, `a2`, ...),
  creates or completes it (`accounts::create`: 0700, links, Claude seed), and opens a new window
  named after the provider and id in the caller's session, running `claude auth login` or
  `codex login` through the user's interactive shell with `CLAUDE_CONFIG_DIR` or `CODEX_HOME` set
  (`cli_session::interactive_shell`). The login itself is the CLI's own; zz types nothing into it.
- **Clients.** zz-client's core keeps the last list (`ClientCore::agent_accounts`) and emits
  `CoreEvent::AgentAccountsChanged`. The desktop keeps one list per host beside the providers report
  (`MuxClient::host_agent_accounts`, `request_agent_accounts`, `add_agent_account`); zz-app reads the
  core (`Connection::request_agent_accounts`, `add_agent_account`, `switch_agent_account`). The
  composer's usage pill is their one reader.

## Rate limits

Quota belongs to an account, not a pane: up to two windows, the 5-hour one and the week, each a
used fraction and a reset time (`QuotaWindows`, `QuotaWindow { used, resets_at }` in
`zz_protocol::providers`). zz calls no usage endpoint and reads no token; it keeps what the CLIs
emit or answer, in the daemon's memory only, one record per provider and account (`None` is the
CLI's own login; `Accounts.quotas` in `daemon/accounts.rs`).

- **Claude Code** writes `rate_limit_event` on its stream once per turn, with
  `rate_limit_info.unifiedWindows.five_hour` and `seven_day`, each `utilization` (0 to 1) and
  `resetsAt` (unix seconds); the driver reads both (`claude::translate::rate_limits`). Nothing
  reports it while idle, so a Claude Code account's limits are known only after a turn on it since
  the daemon started.
- **Codex.** The account probe asks the same short app server for `account/rateLimits/read`
  (`excludeResetCreditDetails: true`) after `account/read`, so the limits are known while idle and
  come again with every `agent-accounts` call. A pane connection takes `account/rateLimits/updated`
  too, which carries no `threadId` and fills in only what changed. The `rateLimits` snapshot's
  `primary` and `secondary` windows are placed by `windowDurationMins`: 300 is the 5-hour window,
  10080 the week, and any other length is left out (`codex::translate::rate_limits`); a snapshot
  whose `limitId` is not `codex` is ignored. codex-cli 0.167.1 on a Pro login answered one
  10080-minute `primary` window and a null `secondary`
  (`agent/codex/fixtures/rate-limits-read.json`), so that account shows the week alone.
- **From a pane.** A driver sends `AgentStreamPayload::QuotaChanged`, which the host keeps out of
  the stream like `TitleChanged`: it merges it into the pane's state, the fanout hands a change to
  the daemon (`AgentPublisher::publish_agent_quota`), and the daemon merges it into the record of
  the pane's provider and account (`Shared::apply_agent_quota`). A window an update leaves out keeps
  its last value; a probe answer replaces the record.
- **To clients.** `EventPayload::AgentQuotas(Vec<AgentQuota { provider, account, windows }>)`,
  gated by `agent-quotas-v1` ([wire protocol](/protocol/wire-protocol.md#adding-a-message-inside-an-epoch)),
  goes to the `agent-accounts` watchers on every plain call and whenever a record changes, the
  whole list each time. zz-client keeps it (`ClientCore::agent_quotas`) and raises
  `CoreEvent::AgentAccountsChanged`; the desktop keeps one list per host beside the accounts
  (`MuxClient::host_agent_quotas`). An older daemon never sends it and an older client never gets
  it, so either side shows no limits. A client treats a window whose reset time has passed as
  unknown (`agent_settings::account_quota`, given the time), with no meter and no warning tint,
  since Claude Code can report one window a turn and leave the other's old value in the record.

# Finding the TUI's session

`crates/zz-daemon/src/agent/cli_session.rs` reads the CLIs' own files, never their screens.

- **Claude Code.** The record `sessions/<pid>.json` under `$CLAUDE_CONFIG_DIR` or `~/.claude` whose
  process is the pane's or below it (`claude_peers::record_in_pane_with`) names the session, the
  folder and the status. The CLI writes it at start and removes it at exit. Model and effort are
  the transcript's last assistant line (`message.model`, `effort`) and its "Set model to" and "Set
  effort level to" lines, over the CLI's arguments and settings files (`Facts::claude_launch`);
  a model name the CLI printed becomes its alias for `--model`.
- **Codex.** The thread is the one `codex resume <id>` names, else the parentless rollout the TUI
  holds open (embedded), else the one parentless rollout in the TUI's folder modified since it
  started; two such rollouts refuse the flip and ask for `codex resume <id>`. Model and effort
  come from the rollout's `turn_context` and `thread_settings_applied` lines over `config.toml`
  and the arguments.

# Codex on the shared app server

A Codex TUI runs its threads in the app server daemon it starts (`codex app-server daemon start`),
reached over a WebSocket on `$CODEX_HOME/app-server-control/app-server-control.sock`. When
`agent-command` is plain `codex`, the chat driver connects there too (`Process::connect`,
`codex::managed_daemon`) instead of starting an app server of its own, starting the daemon first
when its socket is missing (without the pane's `ZZ_PANE`, `ZZ_SESSION`, `TMUX` or `TMUX_PANE`).
A thread then outlives whichever side drove it: a flip kills the TUI and `thread/resume`s, or
closes the chat's connection and runs `codex resume`, mid-turn included, and the server unloads an
idle thread with no client 60 s later. A configured command with arguments keeps an app server of
its own per chat.

The server's environment is its starter's, so the chat hands each thread its pane's environment:
`thread/start`, `thread/resume` and `thread/fork` carry a `config` override that sets
`shell_environment_policy.set.<NAME>` for `ZZ_PANE`, `ZZ_SESSION`, `ZZ_SOCKET`, `TMUX`,
`TMUX_PANE`, `ZZ_TMUX_EXECUTABLE`, `ZZ_TMUX_SHIM_DIR`, `ZZ_AGENT_CONTEXT` and `PATH`
(`thread_config` in `agent/codex/mod.rs`). Commands and background terminals in a chat's turns
see the chat's pane, two chats in different panes on one server each see their own, and a flip to
the chat gets it too: the TUI is gone by then, and the server rebuilds a loaded thread nobody is
subscribed to when a resume brings different overrides. Where it falls short:

- The server ignores the overrides when another client is still subscribed to the thread, so a
  second chat resuming a thread a live chat holds runs its commands with the first chat's pane.
- The TUI's own turns run with the server's environment: no pane when zz started the server, or
  the pane of the TUI that started it. `zz` called from Codex in the TUI may name no pane or the
  wrong one.

# The chat's slash list

- **Claude Code.** The driver offers the commands the CLI's `init` frame lists (`slash_commands`),
  less its `terminal_slash_commands` and the ones it refuses over stream-json (`CHAT_REFUSED`:
  `/status`, `/help`, `/memory`, `/permissions`, `/export`, `/rewind`, `/resume`, `/tasks`, `/btw`,
  `/focus`, `/fast`, `/doctor`). Until the first turn sends `init`, it offers the `initialize`
  answer's list less those. Typing one of the refused commands fails the turn with "only works in
  Claude Code's own screen; flip the pane to the CLI", and `/doctor`, which goes through the
  user's real configuration, is never sent.
- **Codex.** `/compact` and `/review`.
- **zz's own verbs** (`/btw`, `/side`, `/steer`, `/fork`, `/rewind`) and the `/model` and `/effort`
  setters come from the client (`zz_client::agent_completion`).

The driver follows a `/clear`'s new session id (`follow_session`), and the empty `result` frames
the CLI writes while it resumes (`num_turns: 0`, before its first idle `session_state_changed`)
end no turn.

# From outside: the CLI

- `agent-send [-t %N] TEXT` to an agent pane submits a prompt. `--wait`, `--json`, `--final`,
  `--on-block`, `--notify` and `--progress` work on its turn as they did for the old agent pane.
- `agent-send` with text to a terminal running Claude Code or Codex fails with "runs Claude Code in
  its terminal; flip the pane to chat first". Nothing types prompts into a TUI or posts them to its
  peer socket. `--wait ''` still waits on such a terminal's `@agent_state`.
- `agent-respond` answers a pending permission in an agent pane.
- `flip-agent` and `select-pane-kind ... agent` as above.

# What the clients draw

- **The flip button.** A terminal running either CLI and an agent pane carry a flip button in their
  header (`ButtonLook::Toggle`, `zz_ui::pane::pane_flip_button`) that sends `flip-agent -t <pane>`.
  A `busy:` answer offers Switch when idle and Switch now; an armed flip shows with Cancel.
  `zz_client::agent_flip` (`AgentFlips`) keeps that state per pane for every client, and the
  composer's account switch (`agent-account`) rides the same requests and busy card.
- **The chat.** `crates/zz/src/agent` on the desktop and `clients/app/src/app/agent_pane.rs` on
  web, iPad and iPhone draw an agent pane with `zz_ui::agent::chat`: the composer, the model and
  effort menu, questions and approvals asked in the composer, subagent rows, images, references,
  Copy and Fork, and preview cards. Both open subagents, images and cards in the
  [view pane](#previews-and-the-view-pane); a host that gives the chat no view opener (the
  storybook) keeps the subagent PiP and draws diagrams and code inline. The storybook's Agent story
  draws them from fixed chat events. The HTML client (`clients/dom/src/agent.rs` and
  `web/js/agent.js`) draws its own timeline from `zz_client::agent_timeline` over the older
  transcript model, not from `chat_rows`, so its runs, steps and subagents follow other rules.

# The empty chat and the footer

zz keeps no chats of its own. A chat with nothing in it shows the welcome and, centered under it,
two buttons 6 px apart (`zz_ui::agent::resume::folder_button` and `resume_button`): the folder
the next chat starts in, and Resume, for a chat the CLI already has. Each opens a menu.

- **The listings.** While the pane is ready and its chat is empty, the desktop asks the pane's
  driver for two lists with `AgentSessionOp::List` (`zz_client::agent_resume::Resume::start`): the
  chats in the pane's folder, and the newest chats in any folder. Claude Code's come from
  `~/.claude/projects`: the driver reads the first and last 64 KiB of each transcript once and
  keeps that summary while the file's mtime and size stay the same (`sessions::list`), and keeps
  the whole sorted list per folder filter so a page after the first is a slice of it, about 12 ms,
  not a new walk. Codex's come from `thread/list` with `archived: false` and `useStateDbOnly:
  true`, so Codex answers from its state database instead of scanning its rollouts. The reply is
  `EventPayload::AgentSessions`, which `MuxClient` hands to `AgentController::apply_sessions`.
  The pane's own session is left out of both menus.
- **The menus.** One popover shape for both (`zz_ui::agent::resume::resume_menu`): 300 px wide,
  6 px in, a 30 px search field ("Search folders" or "Search chats"), then 32 px rows in a list
  at most eight rows tall that scrolls. Only one is open at a time, and neither drills into
  another. The model (`Resume`: which menu is open, the search text, the picked row and each
  menu's rows) is in `zz-client`; the view draws the rows and forwards keys.
  - **Folder menu.** The folders the CLI has chats in: the pane's folder first, with a check where
    the age would be, then the others newest first, each with its parent path (home-relative) and
    its last chat's age. There is no chat count, since the driver pages the list. Picking a folder
    sends `AgentSessionOp::New { cwd }` and starts a new chat there in this pane; picking the
    folder the chat is already in just closes the menu.
  - **Resume menu.** "In" and the folder's name, then the eight newest chats in the pane's folder,
    or "No chats here yet"; a search reaches the older ones; then "Other folders" and the eight newest chats elsewhere, each
    with its folder's name and age. Picking a chat sends `AgentSessionOp::Switch` with its session
    and folder: the driver restarts on that session and the transcript replays as it does on a
    restart.
- **Search.** `zz_client::fuzzy::fuzzy_match`, the palette's matcher. In the folder menu it
  matches a folder's name, else its path, name matches first. In Resume it gives one flat list of
  up to 40 chats whose title, else folder name, matches, title matches first and the newest among
  equals, each with its folder's name. Nothing matches is a row of its own.
- **Loading and paging.** Until the first listings arrive a menu is one `ResumeRow::Loading` row,
  a spinner and "Loading". While a menu is open, `Resume::wanted` asks for the next page of the
  all-folders listing, one request at a time, until it runs out or reaches 40 pages; with Resume
  open the pane-folder listing pages the same way. The spinner row ends the menu while a page is
  out. A `SessionListFailed` reply ends the wait.
- **What is left out.** The daemon drops a chat whose folder no longer exists on its host
  (`zz-daemon/src/agent/runtime.rs`), since it cannot be resumed there: temp dirs, deleted
  worktrees, chats that ran on another machine. The folder menu also leaves out scratch folders
  (under a temp dir, `~/Library` or a dot folder; `scratch` in `agent_resume`) unless the search
  matches them.
- **Archived chats.** Codex's listing asks for unarchived threads only. Claude Code keeps no
  archive or hidden mark of its own (none in its transcripts, `~/.claude.json`, `~/.claude/` or
  `claude --help`), so every Claude Code chat whose folder exists is listed.
- **Keys.** Up from the empty composer opens Resume. With a menu open, typing goes to its search
  field; Up and Down move the picked row past labels and the spinner and keep it in view; Enter
  picks; Escape clears the search, then closes the menu. The first pickable row is picked when
  nothing else is. Closing puts focus back in the composer; a press outside closes the menu too.
  The menus are penna `Popover`s the view opens and closes itself (`Popover::open` and
  `on_toggle`).
- **The pane's folder.** After a `New` or a `Switch`, the driver answers `SessionSwitched` with
  the session's folder. `AgentController` takes it as the pane's folder, which the folder button
  and the next listings use, and the pane header reads it for an agent pane
  (`AppView::pane_folder`). The daemon's fanout sends the same folder to
  `Shared::apply_agent_folder`, which writes it into the pane's `AgentDescriptor` and publishes
  the snapshot, so the sidebar, the palette and a restart of the pane use the folder the chat
  is in now, not the one the pane opened in. `AgentView` takes the descriptor's folder for its
  pane whenever the snapshot changes it (`AgentController::move_pane`). The daemon looks up an
  agent pane's summary folder, repo and branch from that folder too, so a chat opened in a
  terminal pane drops the terminal's folder.
- **The footer.** The folder chip and the side chat button are gone: the header pill shows the
  folder, and `/side` opens a side chat. What is left is the branch summary on the left and two
  controls on the right (`zz_ui::agent::controls`):
  - `agent_usage_pill`: penna's context ring at 16 px (`chat::context_ring`), centred on the cap
    height of the context's percentage beside it (`42%`, tabular figures), muted, in the same
    `chrome_button` as the picker. What it shows is a `zz_client::agent_timeline::ContextUsage`:
    known, the fill and its percentage; a chat with nothing in it yet (no turns), the empty ring
    and `0%`; a chat whose use is not known yet (a resumed chat before its first turn), the empty
    ring and `--` in the same muted text. When the
    5-hour window or the week of the pane's [account](#accounts) is at 80% or more
    (`QUOTA_WARNING`), the pill takes the warning soft paint and the ring's arc that paint's ink;
    the number stays the context's. Its tooltip is the context sentence ("84k of 200k context
    tokens used (42%)"), "No context used yet" in an empty chat, or "Context use shows after the
    next turn". It shows whenever the footer controls do, before the first
    `agent-accounts` list arrives and while a question is open; a chat asks for the list when it
    mounts if there is none, and the pill asks again every time it is clicked, which is how a
    finished login shows up. The input row no longer draws the ring (`Composer.usage` stays in
    penna and is left empty). Its popover is 264px wide, 6px in, with no headings:
    - Up to three meters from one builder (`usage_meter`): the label, a muted detail after a middle
      dot and the percentage at the end, over penna's small `Progress` bar. Context reads `84k of
      200k` when the use is known, shows no detail, 0% and an empty bar in an empty chat, and is
      left out while the use is unknown; 5-hour and Week read `resets in 2h 14m` (or `resets
      in 14m`) when the reset is under a day away, `resets Thu` (the local weekday) otherwise, and
      nothing when it is past or unknown (`zz_client::agent_timeline::quota_reset_detail`), and
      show when the account's [limits](#rate-limits) are known. A window at 80% or more draws its
      percentage in the warning ink. With no meter, the meters and their rule are left out.
    - Under a rule, the pane provider's accounts as 40px rows (penna's `suggestion_row`): the
      email over the plan and organization (`account_detail`: `max_20x` reads Max 20x), or Signed
      out for a default that is not signed in, the current one picked and checked, the theme's row
      gap apart. Picking another sends `agent-account -t %N <id|default>` through
      `zz_client::agent_flip::AgentFlips::switch_account`, the flip's own request tracking: a
      `busy:` answer shows the flip's busy card over the pane with "Wait for ..., then pick the
      account again." and only its close button (`Flip::AccountBusy`, `pane::pane_account_busy`),
      and any other error, the missing-directory one included, becomes a notification that starts
      with `agent-account:`.
    - Under a rule, Add account sends `agent-accounts --add` for the pane's provider. Both close
      the popover.
  - `agent_model_picker`: a popover over the footer, the model, its effort and the permissions in
    one menu. The button shows the permission mode's glyph in its tone (`mode_glyph`, centred on
    the label's cap height), then the model by its choice's display name
    (`LensSettings::model_label`, the raw value only when no choice matches), then the effort. The
    menu's sections, each under a rule: every model the driver lists, in its order, a check on the
    current one, in one list six rows tall that scrolls and opens with the current model in view;
    the efforts as penna's `StepRail`, one horizontal rail with a dot per step that sets the effort
    when the knob is let go (Claude's first effort is `default`, so its first step is a ring,
    `effort_starts_unset`); then Permissions, the driver's `mode` option, the permissions the CLI
    runs with, one row per mode with a check on the current one. Model and permission rows sit the
    theme's row gap apart (`Theme::row_gap`, 2px in an inset list), so two highlights never touch.
    Shift-Tab in the composer steps to the next mode (`LensSettings::next_mode`).
    The glyph is the mode's own (`mode_icon`, by provider and mode value): a raised hand for
    Claude's Ask, a pencil for Accept edits and Codex's Auto (it edits the workspace without
    asking), a folded map for Plan, a bolt for Claude's Auto (the agent decides when to ask), a
    struck-through shield for Bypass permissions and Full access, and an eye for Read only; a mode
    the table lacks falls back by its risk (hand, eye, bolt, struck-through shield).
    `agent_settings::mode_risk` gives the glyph its tone: none while the agent asks first
    (Claude's Ask, Codex's Auto), success when it cannot write (Plan, Read only), warning when it
    acts without asking (Accept edits, Claude's Auto), danger with nothing guarding it (Bypass
    permissions, Full access). The send button draws the same in every mode. Claude Code always offers Bypass
    permissions; the driver starts the CLI with `--allow-dangerously-skip-permissions` unless its
    command already has that flag or `--dangerously-skip-permissions`, since without one the CLI
    refuses to switch a running session to it.
- **Where.** The desktop has all of it. Web, iPad and iPhone have the footer, the usage pill
  included, but not the empty chat's menus yet, and the HTML client has neither. The storybook's
  Agent story shows the menus (Empty chat: at rest, each menu open, searching, loading and Nothing
  matches) and the controls (Composer controls: the pill in a new chat, at each context fill and before
  the use is known, a model pill per Claude Code mode and per Codex mode, then the footer as
  usual, in a new chat, before the use is known, with a 5-hour window at 92%,
  with an account that has no limits yet, with two accounts and the second picked, and with a
  signed-out default).

# References in the chat

zz references in a message show as named links (`zz_ui::agent::refs`, resolved by
`zz_ui::target::resolve_target` against the same host tree the `@` menu lists):

- An `@` token as the `@` menu writes it: `@host:session:window.%pane`, `@session:window`,
  `@host:%pane`, `@host`, `@~/path/`, `@host:/path/file.rs:12`.
- In inline code, the same token, a pane or session id (`%3`, `$0`, `$0:1.%2`), or a file path with
  an extension (`crates/zz/src/main.rs:40`), resolved against the chat's folder.

A pane shows as its program and window ("bash · zebrilus 0:1"), a chat as its title, a window as
"session › name", a file as its name and line, a folder with a trailing slash, and anything on
another host adds " · host". Tokens that resolve to nothing stay as written, and fenced code
blocks are left alone. Clicking a pane, window, session, chat or host switches to it like the
palette does; a file opens in a float running `$VISUAL` or `$EDITOR` at that line, started through
the user's interactive shell like the CLIs below, and a folder opens a shell float there
(`refs::open_path`). The assistant's
markdown links them after parsing: `ChatMarkdown` hands penna `refs::ref_links` as its inline-link
hook, which turns each token in a text run, or a code span that resolves, into a `zzref:<token>`
link that `RefLinks` resolves again on click. The pane keeps one `ChatMarkdown` and builds a new
one, so every reply parses again, only when the hosts or the working directory change. The user's
bubble draws refs as clickable ranges.

## In the composer

The composer's `@` menu is `zz_ui::target::TargetPicker`, made for a composer by
`zz_ui::command::mention_picker`. The desktop (`crates/zz/src/agent/view.rs`) and the thin
clients (`clients/app/src/app/agent_pane.rs`) build its button and its layer with the same two
calls:

- **The + button** (`target::mention_button`) toggles the menu. With the menu closed it writes `@`
  at the caret, with a space before it when the caret follows a word, unless the caret already sits
  after a bare `@` that starts a mention, which it opens again instead. With the menu open it
  closes it. Closing it this way, with Escape or with a press outside it takes back the `@` the
  button wrote when nothing was typed after it (`TargetPicker::unplant`); an `@` you typed stays.
  The press outside is the panel's `on_mouse_down_out`, deferred so the press lands first; the
  picker keeps where it was (`closed_at`), so the click on + that caused it does not open the menu
  again.
- **The menu floats.** penna's `ChatComposer` has an `over` slot: a deferred layer positioned on
  the composer box, its left and right edges, 8 px above its top edge and growing up
  (`target::mention_layer` puts the open picker in it). Opening and closing it changes nothing in
  the chat's layout or scroll. The list keeps at most eight rows and fewer when the window has no
  room above the composer (the picker measures where it sits and `fit`s to it). Typing goes on in
  the input, and `mention_keys` keeps Up, Down, Tab, Enter and Escape.
- **Picked targets show by name.** Picking a row writes `@` and the name `resolve_target` gives it,
  the name the chat shows for that reference, and the input draws it in the reference highlight
  (penna's `InputState::set_highlights`): a chat as `@fix the atlas tests`, a terminal pane as
  `@cargo watch · zz:1`, a browser pane as `@localhost:8097 · zz:2`, anything on another host with
  ` · host` (`@tune the lr schedule · gpu-box`). Two targets with the same name read `@name` and
  `@name (2)`, and the same target twice keeps one name. The input holds the name;
  `zz_ui::mention::Mentions` keeps each name's range and raw reference, moves the ranges as the
  text changes, and drops a name when an edit touches it, so an edited name goes out as the plain
  text it now reads. A name with text glued to its end goes out plain too.
- **What leaves.** Sending and queueing take `TargetPicker::raw_text`, which puts each name's raw
  reference back (`@local:%12`), so the agent gets exactly what the menu used to write. Text with
  raw references that enters the composer (a queued prompt returned to it, a reclaimed prompt,
  a paste) gets names for every reference that resolves; one that resolves to nothing stays as
  written. Undoing that naming brings the raw text back and leaves it raw. There is no draft
  saving or prompt history in the composer for the names to pass through.

# How the chat reads

`zz_ui::agent::chat` draws the folded chat (`chat_thread`):

- **Your messages.** Claude Code wraps a paste as `<pasted_content id="…">…</pasted_content
  id="…">` in its transcript; the runtime drops the wrapper (`claude::translate::prompt_text`). The
  bubble is `CardLook::Bubble` (`card.bubble`): a wash of the accent with the ground's ink. A
  message whose text runs past 224 px folds to 176 px, its last 64 px of text fading out
  (`FoldFade`, ala's vertical text fade, so it works on glass), and offers Show full message, then
  Show less. The bubble measures its own text as it draws and reports it through
  `ChatClick::Tall`; `ChatClick::Unfold` flips the fold (`ChatOpen::tall`, `ChatOpen::unfolded`).
- **Pasted images.** The translators put a prompt's images on its `user_message_chunk` as
  `_meta.zz.images` (`{ format, data }`, base64): Claude's base64 image blocks, minus the
  `[Image #N]` placeholders, and Codex's data URLs and local image files. The bridge
  (`chat_bridge::attach_images`) keeps a message's text and base64 together under 6 MiB so one
  stream item stays under the 9 MiB batch cap, and the chat item carries them as `ItemBody::images`;
  a user item is held to that cap rather than the 1 MiB one other chat items get
  (`runtime::validate_payload`). A prompt sent through the drivers gets the same treatment:
  `runtime::prompt_echo` puts its images on its first text chunk (an empty one for a prompt that is
  only images), and the image chunks after it stay for the timeline. Both drivers drop
  `_meta.zz.images` from the raw update once the bridge has read it,
  so the stream carries each image once; the driver's journal keeps it, so a restored session's
  chat shows it again. The fanout's chat log and every client's `ChatView` keep images on the
  last 8 items that have them (an item that loses its images gives up its place), so a live chat
  and a late client's replay both show older pastes as text only. The bubble draws them first as
  140 px tiles that open full size on click; `ChatPane::sync` decodes each item's images once and
  keeps them in `ChatOpen::images`. When an item leaves the view, loses its images, or the session
  changes, `ChatPane::release` drops the decoded pixels from ala's asset cache and the sprite
  atlas, unless a live chat pane still shows the same image (an app global lists each pane's
  shown images). The desktop agent view and the thin client's agent pane hook
  `ChatPane::close_on_release`, so closing the pane frees every image it held, through its window
  or, when the window closed with it, from the asset cache alone.
- **Live turns.** The last line of a running turn says what the agent is doing now, with a soft sweep
  of light across its text, a breathing sparkle and the time since the prompt (`chat_rows::live_line`). It takes the newest of:
  the CLI's own activity line (`ChatEvent::TurnActivity`, from Claude Code's `task_summary`), a
  running tool's own description (`ItemBody::activity`: Claude's Bash, PowerShell and Agent
  `description`, Codex's `wait` as Waiting for agents), else the running tool's verb and target
  ("Running cargo test"), and the latest reasoning heading. A message the agent writes after them
  clears them, and the line says Thinking only when nothing more specific is known. While a Codex
  async question is open and no tool runs, it says Waiting for your answer (`ChatView::asking`). While the
  newest run of tools is still going, the line is that run's header and opens to its steps. It
  hides while the chat waits on you. The transcript only gets a reply once it is whole, so a reply
  that lands while you watch types itself in over 220 to 900 ms (`ChatPane::sync` notes when it
  arrived; replies already there on the first look show at once). While a reply streams or types
  itself in it is drawn repaired (`mend`): an unclosed bold, code span or fence is closed and a
  link whose URL has not arrived is inert, so the text does not jump when the rest lands. Text that
  arrives fades in, each paragraph, table cell and code block keyed by where it starts in the source
  (penna's `StreamingVeil`); two that shared a key would restart each other's fade every frame and
  never show, which is why table cells carry their own source spans.
- **Long replies.** A reply over 32 KB or 512 lines shows its start at 420 px, clipped and fading
  into the pane's ground (penna's `bottom_fade` over a clip that does not scroll, so the wheel
  scrolls the chat), with Show full message and Copy under it, left on the text column. Only the
  first 8 KB or 64 lines are laid out while it is closed (`ChatOpen::previews`, mended so an
  unclosed bold or code span is closed; an unclosed fence runs to the end of the cut, as Markdown
  draws it). Show full message draws the whole reply in place, as any reply, and Show less
  closes it again (`ChatOpen::expanded`, per item while the pane lives); closing scrolls the chat
  up by what the reply lost, so the row stays under the pointer. Copy there takes this reply,
  where Copy under the turn takes every text of the turn. A reply that streams or types itself in
  past the limit shows the same closed start.
- **Runs.** Tool calls and thoughts in a row, with nothing else between them (no text, message,
  subagent, plan, notice or ask), are one run with one header (`chat_rows::turn_rows`). In a
  finished turn the fold joins runs that only a kept row (an answered question, your message, a
  live agent) parted, since that row shows under the fold. Opening a run lists its steps flat, in
  order, with its thoughts. A step reads its target, or its title when it has none
  (`chat_rows::step_label`), and a failed one turns red and says why in
  its faint detail: the first line of the tool's error, after `exit N` for a command with an exit
  code.
- **Batches.** Claude Code sends a sentence for each finished batch of tool calls
  (`tool_use_summary`, with the ids it covers). A run whose calls one sentence covers shows it
  instead of zz's count ("Counted lines in a.txt" for "Ran 2 commands"), with failures and denials
  still counted after it; a run that several sentences cover shows zz's count ("Edited plan.md, used
  4 tools"), and each call a sentence covers alone reads that sentence over its target, as in
  "Loaded plan mode tools  select:EnterPlanMode,ExitPlanMode". The chat matches a sentence by tool
  id (`ChatEvent::ItemsSummarized`), so one that lands after the reply, after the turn, or in a
  replay finds its rows. Codex, subagents and a session resumed from the CLI's transcript (which
  keeps no sentences) show zz's count. Claude Code's `post_turn_summary`, a line about the whole
  turn sent after it, is not drawn: the reply sits right under the fold and says the same thing.
- **Thinking.** A finished run whose reasoning is only headings (Codex records the headings of its
  summaries; the reasoning itself is encrypted) leaves no row, while the running one still shows
  its heading. Opening a run lists its reasoning, heading removed and muted, in order with its
  steps; a run with nothing to list has no disclosure. A heading is a line that is all bold or a
  `#` heading (one to six `#` and a space), so a thought that opens with inline bold or `#42`
  keeps its text (`chat_rows::reasoning_body`). The opened text is selectable and scrolls inside
  260 px. Claude's thinking reads the same way.
- **Subagents.** One row each, a flat `CardLook::Activity` card (`card.activity`) that washes
  deeper on hover; the plan card uses the same card. A row holds the CLI's tile in its program colors (`program_tile`, as pane
  stacks draw it) with a status dot on its corner (working, needs you, done, failed; a stopped agent
  has none) and the name; the time it took and the PiP mark show only while the pointer is on the
  row (always on touch). The tile's tooltip names the state ("Waiting for you"), and so do the
  tile and the row (a button named after the agent) to screen readers, so it does not rest on the
  dot's color. Clicking one opens its chat in the
  [view pane](#previews-and-the-view-pane): the prompt it was given, its own rows, its answer and a
  footer, live (`chat_agent_body`). An approval it waits on stays under its row. Without a view
  opener the chat opens it in a PiP docked beside the thread instead (`chat_agent_pip`,
  `chat_pip_frame`; on touch it covers the thread), with the same tile in its header; its close
  button or a second click closes it, and an approval shows in the PiP while it is open.
- **Asks.** A question, a tool approval or a plan approval morphs the composer instead of opening
  a card (`zz_ui::agent::ask`, penna's `Composer.ask`): an approval has a header with its glyph,
  title and a count when more wait, then the command with its folder; a question is its own header,
  the question wrapping beside the chat glyph with the pager or count on its first line (its short
  label stays for the chat's row and the answered record). Then numbered options, and the input
  row taking the answer's own field (a question's typed answer, or a reply that denies or keeps
  planning and is sent as the next message). In a pick-any question a picked answer's key lights
  (solid accent, no checkbox), and so does the typed answer's key once it is picked. Digits pick
  while the field is empty, Up and Down move the highlight, Left and Right page between questions,
  Enter sends, Escape skips or denies; hints in the footer say so, a pick-any question's leading
  with "Pick any" or how many are picked. A plan approval (Claude's `switch_mode` request, sent as
  `Ask::Plan`) shows the plan in a scrolling inset with a button that opens it in the view pane.
  The rules for titles, send labels and hints live in `zz_client::agent_ask`, shared by every
  client. The chat keeps one row per ask while it waits ("waiting for you"), and an answered
  question as a row per question with its answer.
- **Codex's async questions.** Codex can ask without stopping its turn
  (`request_user_input_async`); the app server sends that as an `agentMessage` with
  `delivery: "async"`, `questions` (`{ title, options? }`) and a markdown copy in `text`. The
  driver asks the questions in the composer, each taking an option or typed text,
  and drops the copy while keeping any other text (`codex::translate::async_questions`). The ask
  is a background request: not blocking, kept past the end of its turn by the driver, the chat and
  the host (`AgentPendingPermission::outlives_turn`). An answer goes back as a user message in the
  CLI's own reply format, `<send_user_message_question_reply>` around
  `[{questionItemId, question, answer}]` with the id `["request_user_input_async", call, index]`
  (`translate::question_reply`): `turn/steer` into the running turn, else `turn/start`, and a
  start when the steer misses the turn. That message closes the card as answered and never shows
  as a bubble, also when the TUI sent it. Any other message closes the card with no answer, Stop
  leaves it open, and a resumed thread shows the answered ones as records and reopens the one
  still waiting.
- **Callouts.** `> [!NOTE]` style alerts and `:::name title` blocks draw as a flat card washed in
  their tone with an icon and a title (penna's `BlockNode::Callout`).
- **Preview cards.** With a view opener, mermaid and SVG blocks, code blocks of six lines or more
  and markdown fences of six lines or more leave the reply as cards (`agent::preview`), flat
  `CardLook::Activity` cards that wash deeper on hover. A diagram is a thumbnail of the drawn diagram
  named by its kind (Flowchart, Sequence diagram, Gantt chart; the pop-out mark shows on hover). A
  code block is its language's tile (the program tile when zz knows the language: rust, python,
  go, typescript and the other program marks, plus 32 language marks from Devicon; a neutral tile
  with braces otherwise), its file name when the fence names one (`rust src/main.rs`,
  `rust:src/main.rs`, `title="..."`) or its language, and its length. A markdown fence is named by
  its first heading. Fences that follow each other share one row: the card plugin claims the next
  siblings with `MarkdownNode::absorbs`. Shorter code, `html preview` blocks, blocks over 64 KiB and
  mermaid types pele does not draw stay inline. Clicking a card opens it in the view pane.
- **Diagrams.** Without a view opener, or inside a markdown view, a mermaid or SVG block sits in a
  `CardLook::Figure` card (`card.figure`), which has no fill and no header: Copy source and Open
  full diagram sit over its top right corner while the pointer is on it (always on touch). pele
  (`libs/pele`) draws mermaid:
  flowchart, sequence, class, state, ER, gantt, pie and quadrant. Any other type stays an ordinary
  code block of its source, and a supported source that does not parse shows the error with its
  line and column. The diagram's canvas is transparent and its colors are worked out on the chat's
  ground (`MermaidThemeColors::from_theme`). Nodes take the accent wash the bubble takes (pele's
  `node`), clusters, frames and notes the `card.activity` wash (`penna::blocks::chat::diagram_paint`).
  Lifelines, frames and box edges take the faint foreground; nodes take the accent's outline when
  the theme's outline knob is on and a deeper accent wash when it is off. Every line is 1.25 px
  (pele's `Theme::stroke_width`). Corners follow the control radius up to 8 px. Edges and arrows
  take the muted foreground; text is the app font at
  13 px measured by the app's text system (ala's SVG renderer draws `system-ui` in the face that
  text system uses). Pie, gantt and quadrant series are the accent, success, warning and danger,
  then hues that fill the widest gaps between them, all at one Oklch lightness near the accent's
  (0.5 to 0.58 on a light chat, 0.68 to 0.78 on a dark one) so the ground-colored labels on slices
  and bars read on every fill (`MermaidThemeColors::series`). A flowchart, class, state or
  ER diagram that runs across and is wider than 640 px turns to run down when that fits. A figure
  draws at its own size, scaled down only to fit the transcript's width, so a diagram that turned or
  a long sequence keeps its 13 px labels however tall it gets. Answers use the same markdown style
  as code blocks and tables elsewhere (`agent_text_view`). Transcript text (answers, the user's
  bubble, thoughts, notices and side replies) is penna's `read-body`, 14 px on a 22 px line, and the
  markdown inside inherits it. The chat passes its pane as the owner
  (`ChatHooks::owner`), so on the desktop an HTML block's live page can reply to the agent, and two
  panes showing the same item keep their own preview toggles.

# Previews and the view pane

A chat opens what it previews in one floating `view` pane (`PaneKindSnapshot::View`), a pane with
no process. The chat sends `view-pane -t <chat> -T <title> -k diagram|code|markdown|image|subagent
[-l language] [-i item] [-n index] [-y rows] [-- text]` (`ViewRequest::command`). In the mux
(`view_pane` in `crates/zz-mux/src/command.rs`), if the chat's window already has a view whose
origin is that chat, the view takes the new content and is selected, so a chat has one view and
cards are one click from swapping it; otherwise a float opens 55% of the chat wide
(at least 48 columns, or the whole chat when narrower) on the bottom row of
[PiP places](/designs/pip-panes.md), in the column nearest the chat's right side, `-y` rows tall.
A client passes its `pip_fill_rows`, the tallest PiP its canvas holds; without `-y` the float is
the window's height. It is an ordinary float:
it moves and resizes, `join-pane` tiles it, it shows in the sidebar and the pane stacks with the
eye glyph, every client on the window sees the same content, and it survives detach. `#{pane_kind}`
is `view`, and `inspect` lists `view-pane`, `resize-pane` and `kill-pane` for it. It takes no input:
Escape closes it.

In the GUI clients the view grows out of what was clicked and flies back into it when it closes
([opening from what you clicked](/designs/pip-panes.md#opening-from-what-you-clicked)). The row,
card or tile is a `PipSource`: a subagent row's tile and name, and a code or markdown card's
language tile and heading, become the view's header icon and title on the way; a code card's lines
travel with the frame; a diagram's thumbnail and an image tile's picture scale up into the view's
picture. The source dims to 40% while its view is open.

Diagrams, code and markdown travel in the descriptor, at most 64 KiB. Images and subagents travel
as the chat item they come from and are drawn from the client's own copy of the origin's chat
(every GUI client holds every agent pane's chat): a user message's image (only its 8 newest kept,
as in the chat), or a subagent's live rows. When the chat no longer has the item, or its chat is
not open in this window, the view says so.

The desktop (`crates/zz/src/view_pane.rs`) and zz-app (`clients/app/src/app/view_pane.rs`) draw it
with `zz_ui::agent::view` right under the pane's own header: the diagram fit to the pane, the code
highlighted in a code block with its own Copy (an `html` block keeps its Preview toggle), the
markdown rendered with the chat's extensions, the image scaled down to fit, or the subagent's
chat. The view has no bar of its own. Its header is the one every pane wears, floating, tiled or
zoomed, so Tile, Zoom or Unzoom and Close are always there, and a pane kind can add its own
buttons to it (`PaneKindButton`, `TerminalPaneAction::Kind`): a diagram adds Show the source,
which turns into Show the diagram while the source shows (`view_buttons`). The terminal client
draws the kind and title with a note to open it in the app; the HTML client draws a placeholder.

# How the stream reaches a client

The full delivery contract is in the
[daemon-owned agent runtime](/designs/agent-daemon-runtime.md) and the byte bounds are in the
[wire protocol](/protocol/wire-protocol.md).

- **Coalescing.** The fanout gathers items into 25 ms windows (`BATCH_WINDOW` in
  `agent/fanout.rs`) on one shared flush thread that parks when nothing is gathered. Frames are
  capped at `MAX_AGENT_UPDATES_BYTES` (9 MiB).
- **Sequencing.** The fanout mints the per-pane sequence. A client keeps a cursor per pane, drops
  items at or below it, and asks for a replay when a batch starts past it.
- **A dedicated lane.** Agent frames have their own outbound slot, capped at
  `MAX_PENDING_AGENT_BYTES` (9 MiB + 64 KiB) per pane. Overflow drops the pane's queued frames and
  queues `AgentLagged`, so a slow client replays instead of being disconnected.
- **Replay.** Each pane keeps an 18 MiB ring of encoded items (`MAX_REPLAY_RING_BYTES`); a replay
  older than the ring takes the reset path the runtime design describes. A driver that resumes a
  session replays its history from the CLI's own file first.
- **Visibility.** The heavy stream goes to clients for whom the pane is an agent pane in their
  focused window, honoring zoom (`visible_agent_panes`), and to clients watching the pane.
  `AgentState` goes to every client attached to the session and to the pane's watchers, and a
  reattaching client gets it for every agent pane there (`send_agent_resync`, `session_agent_panes`).
- **Watchers.** A client that sends `AgentReplay` for a pane watches it from then on, attached or
  not (`watched_agents` on the client entry): it gets the pane's `AgentUpdates`, `AgentState` and
  `AgentLagged` until it disconnects or the pane closes, and the pane's `AgentState` right away when
  the watch starts. A client can follow an agent this way without attaching to its session, which
  would resize its panes, and an attached client keeps a chat live while its focused window shows
  something else. A client that both sees and watches a pane is
  sent each frame once, and a watched pane leaving the visible set keeps its queued frames. There is
  no unwatch message.

# Related

- [Agent lens design](/designs/agent-lens.md) . the design this replaced
- [Daemon-owned agent runtime](/designs/agent-daemon-runtime.md) . host, fanout, journal
- [Native agent drivers](/designs/native-agent-drivers.md) . the drivers agent panes run
- [Chat mode](/designs/chat-mode.md) . what a new pane offers in each sidebar view
- [Agent peer bus](/architecture/agent-peer-bus.md)
- [Wire protocol](/protocol/wire-protocol.md)
- [tmux command set](/tmux/commands.md)
- [zz](/crates/zz.md) and [zz-daemon](/crates/zz-daemon.md)
