---
type: Concept
title: Agent panes and the flip
description: How a pane runs Claude Code, Codex, Antigravity, Grok or Cursor as a chat (PaneKind::Agent over the vendor drivers, Antigravity, Grok and Cursor over their own ACP servers), how a chat and its window are named after the CLI's session title, how flip-agent switches one pane between the CLI's own TUI and that chat on the same session (busy rules, --now, --when-idle and the armed flag, finding the TUI's session, waiting for the old process), forks and side chats, accounts (a CLI login per pane through CLAUDE_CONFIG_DIR or CODEX_HOME, agent-account, and agent-accounts to list who is signed in and sign one in, either by the daemon driving the CLI's own login and the chat showing its steps in a card above the composer or in a terminal window, the 5-hour and weekly rate limits each account reports, and the composer's usage pill that shows the context and those limits and switches the account), Codex on its shared app server, the chat's slash list, agent-send and agent-respond, reopening a chat another chat opened on its session (select-child-chat), what the chat draws, the preview cards and the floating view pane they open, and how its stream reaches clients.
resource: crates/zz-daemon/src/daemon/agent_flip.rs
tags: [agent, agent-pane, flip, chat, claude-code, codex, antigravity, grok, cursor, driver, stream]
timestamp: 2026-11-21T00:00:00-03:00
---

# Overview

A pane runs Claude Code, Codex, Antigravity or Grok one of two ways, and one button switches between them (Cursor is a chat only, see [Cursor](#cursor)):

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
| `provider` | `ClaudeCode`, `Codex` or `Antigravity` |
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

**Creating one.** `new-pane|split-window|new-window [-d] [-P -F fmt] [-c dir] [-t target] --kind
agent <claude|codex> [--model m] [--effort e] [--account id] [--resume id] [BRIEF ...]` makes the
pane an agent pane from the start (`agent_spawn` in `crates/zz-mux/src/command.rs`), so the
daemon opens its runtime inside the same command (the `PaneCreated` arm, then
`Shared::open_agent_pane`) with no exit grace, and `-P` prints a pane that already takes
`agent-send`. The four flags may sit before the provider or right after it; `--fork` and `--at`
are not taken here. The words after them are the brief: joined with single spaces, checked like
`agent-send` TEXT (`zz_mux::check_plain_text`, 1 MiB), carried to the daemon as
`MuxEffect::AgentBrief` and queued with `AgentRuntime::prompt` once the runtime is open, so the
driver sends it when its session is ready. Without `-c` the folder comes from the same donor a
terminal would use (`spawn_cwd_source`, `MuxState::cwd_donor`; a chat target gives its own
folder). `new-pane` without `-x`/`-y` floats at `AGENT_FLOAT_WIDTH` by `AGENT_FLOAT_HEIGHT`
(46% by 72%), the size the fork float below uses. A missing or unknown provider, a bad flag value,
or an agent flag without `--kind agent` exits 2.

**Chats that open chats.** A caller is a chat caller when its client's origin pane
(`ClientHello.origin`, from `ZZ_PANE`) is a chat other than the target (`Shared::chat_caller`).
A chat a chat caller creates gets that chat as its parent, kept daemon-side on the engine
(`MuxEngine::set_agent_parent`, no wire field) and read as `#{agent_parent}` and `inspect`'s
`agent_parent`. A chat with a parent cannot open one (`refuse_grandchild_chat`: `%C was opened by
%P; a chat opened by another chat cannot open one. Do the work yourself, or say in your reply what
you need`, exit 1, checked before the mux runs) and
cannot `agent-send` to its parent. The brief of such a chat, and a chat caller's plain
`agent-send` to a chat, carry a waiter (`Shared::chat_report_waiter`) that reports the turn into
the caller as a prompt the daemon makes (`AgentRuntime::inject`): `[zz] %N (codex) finished:` with
the final text (or the full text when the final part is empty, last 64 KiB), `stopped (reason):`
(or `stopped (cancelled):` with no text), each followed by a blank line and `If this answers the
user, answer the user now and send nothing to %N. Only if something is missing: zz agent-send -t
%N 'TEXT'`; `failed: message` followed by `Tell the user, or try again with: zz
agent-send -t %N 'TEXT'`; or `was closed before it replied. Tell the user; do not open it again.`
`AgentReport::parse` strips those trailing lines, so a client gets only the reply text. While that turn waits on a permission
or a question, the waiter's `asks` sink sends one `[zz] %N (codex) asks: kind title` message per
request id with the `zz agent-respond` lines that answer it. Every string here, the stderr lines
included, is built and parsed by `zz_protocol::agent_mention` (`AgentReport`, `AgentAsk`,
`ReplyNotice`, `AgentRespond`), so clients read them with the same code. Deliveries to one caller
go out in the order they were made, from one queue per caller (`Shared::deliver_to_chat`), and an
ask whose request is answered by the time its turn comes is dropped. These prompts are
`injected`: they travel the pane's inbox rather than its 4-slot prompt channel, do not count
against `MAX_AGENT_QUEUED_PROMPTS`, keep the byte cap (a prompt over it is logged and dropped), are
never handed back to a composer when a queue is reclaimed (`/clear`, unqueue, close), and are
dropped with a log line when the caller is gone. A caller flipped to a terminal gets them typed
into its terminal the way `--notify` does. Every pane a chat with a parent creates, a terminal from
`split-window` included, gets that chat as its parent too (`adopt_child_chat_panes`). When a pane
goes away, the chats and panes it opened are killed
right after the command that removed it, on a thread (`close_orphaned_chats`); flipping a parent
to a terminal closes nothing. From a chat caller `agent-send --wait` defaults to `--on-block fail`,
and `--wait ''` on a chat joins the running turn (`HostCommand::Join`) or fails with `%N has no
running turn; read its last reply with: zz capture-pane -p -t %N`.

**What the verbs tell an agent.** Every create with `--kind agent` prints one stderr line
(`ReplyNotice::Opened`): `<verb>: opened %N (NAME)`, followed by `; its reply comes back to you as
a message; end your turn now` for a chat caller with a brief or `; read its reply with: zz agent-send -t %N --wait ''`
for any other caller with a brief. A plain `agent-send` prints the pane on stdout and on stderr
either `agent-send: %N's reply comes back to you as a message; end your turn now` (chat caller) or `agent-send: read
%N's reply with: zz agent-send -t %N --wait ''`. `--notify` from a chat is a plain send. A chat caller sends at most `MAX_SENDS_SINCE_USER` (8)
prompts to one chat, a brief counting as the first, until it receives a prompt the daemon did not
inject (from the user or any client); then the next send fails with `%N has had 8 messages from
you since the user last wrote; answer the user now` and nothing is sent. The counts live in
`AgentRuntime::sends`, keyed by caller and target, and a non-injected prompt to the caller clears
the caller's counts. Sends from terminals are not counted. A chat
whose queue already holds `MAX_AGENT_QUEUED_PROMPTS` prompts refuses a send at once
(`refuse_full_chat`: `%N has 4 prompts waiting; wait for its turn with: ...`), and prompts the
daemon queues for `agent-send` or a brief (`PromptSource::Command`) are never handed back to a
composer if a queue is reclaimed; one the host cannot take at all fails with `%N's agent is not
running (agent_state STATE); the prompt was not sent`. `wait-pane` and `show-last-output` on a chat name
`agent-send --wait ''` and `capture-pane -p`; `capture-pane` adds `capture-pane: %N's turn is
still running` on stderr during a turn. `agent-respond --answer TEXT` answers a request that asks
one question with typed text (`AgentAnswerQuestion`'s host path), and nothing pending reads `%N has
no pending request (already answered, or none was asked)`.

**Reopening a child.** A child chat is its session, not its pane. For every chat a chat opens,
the daemon keeps a `ChildRecord` under the opener's provider and session
(`AgentRuntime::update_child`): the child's original pane id, provider, session id once its
session starts, folder, the model and effort it last showed (`note_child_descriptor`), account,
the pane that holds it now, and every pane it was reopened in (`reopened`). The records are saved next to the opener's
journal as `<key>.children.json`, so they outlive a daemon restart and the opener being resumed in
another pane; they go when the opener's session is deleted (`AgentJournal::remove_for`), not when
its pane closes. `select-child-chat [-t HOST] %N` (internal, for the card a host draws) takes any
id the child has had, selects the child's window and pane and prints the pane: the recorded pane
while it still runs that session, else any live chat on that session, else a new float in the
host's window resumed on the session with the same provider, folder, model, effort and account,
no brief, and the host as parent. A child closed before its session started fails with `%N was
closed before its session started; it cannot be opened again`, and a pane the host did not open
with `%N is not a chat %H opened`. `agent-send -t %N` from the host to any id the child has had
reaches the pane that holds the child now (`Shared::moved_child`) and counts toward one set of 8
messages per child (`child_key`); with no pane holding it, it fails with `%N was closed; open a new
chat with: zz new-pane -d --kind agent NAME 'BRIEF'`.

**The mention note.** A prompt whose text holds an agent token (`@+claude`, `@+codex`, either with
`/MODEL`) or a chat token (`@HOST:%N`, the shape only the composer's Chats rows write) reaches the
agent with an instruction built from those tokens (`zz_protocol::agent_mention::mention_note`,
one sentence per distinct token): for `@+codex`, `The user tagged codex: codex does this task, not
you. Write it a brief that stands on its own, run zz new-pane -d --kind agent codex 'BRIEF' and end
your turn. Do not do the task yourself. Its reply and any permission asks come back to you as
messages.` (with `--model MODEL` when the token names one), and for `@macbook:%2`, `@macbook:%2 is
another chat, not you: run zz agent-send -t %2 'TEXT' and end your turn. Its reply comes back to
you as a message.` A chat opened by a chat gets a second note with its brief
(`child_note`): `%P (NAME) opened this chat. Your final message is sent to it as your reply, so
finish with the answer. You cannot open other agents or message %P; if you need something, say so
in your reply.`, and for a Codex child `If a command fails with "Operation not permitted", rerun it
with escalated permissions even if your brief says to stop on failure; %P is asked to approve it.` The daemon carries that note on the queued
prompt (`QueuedPrompt::note`, `RuntimeCommand::Prompt`'s `note`), never in the text.
`hidden_note` joins the two. Neither note joins the user's text: Claude
Code gets it as a second content block wrapped in `<system-reminder>`, Codex as a second text item
in `turn/start`'s `input`, and other ACP agents as a second text block in the prompt request. The
stream's own echo of the prompt has only the user's text, and the transcript readers drop the
note block when they replay a session (`claude::translate::message_text`, `codex::translate::user_text`),
recognising it with `is_hidden_note`.

**Codex chats run `zz` outside the sandbox.** Before it starts Codex for a chat, the driver writes
`rules/zz.rules` (`prefix_rule(pattern = ["zz"], decision = "allow")`, the file `zz tools
--install-hooks` writes) into the Codex home the chat runs on, the account's or `$CODEX_HOME`,
when that file is missing (`agent_hooks::ensure_codex_rule`); an existing file, edited or not, is
left alone. Codex then runs a command made only of `zz` segments outside its sandbox without asking,
so the first `zz` reaches the daemon socket. Codex reads rules when it starts, so a shared app
server that was already running picks the rule up on its next start.
Daemons from `REVISION` 6 on do this.

**Running it.** The daemon opens the pane's runtime (`Shared::open_agent_pane`, `agent_pane_spec`)
with the descriptor's provider, folder, session, model and effort and the pane's workspace
environment (`ZZ_PANE`, `TMUX`, `TMUX_PANE`). The descriptor's model and effort follow the chat:
each `AgentState` whose config options hold a `model` or `effort` option writes its current value
there (none for `default`), so a reopen after `agent-account` or a sign-in starts on what the chat
last showed, not on what it was created with. The descriptor holds no permission mode, so such a
reopen starts on the command's mode or the CLI's default; within one Claude Code driver, a new
chat, switch, fork, rewind or restart passes the mode the chat showed as `--permission-mode`. A resumed Claude session replays its transcript
before the driver says it is ready; a resumed Codex thread comes back through `thread/resume`.
Closing the pane closes the driver. Agent messages (`AgentReplay`, `AgentPrompt`, approvals,
settings) are accepted only for agent panes, and `AgentState` and the stream follow
[How the stream reaches a client](#how-the-stream-reaches-a-client).

**Forks and side chats.** `--fork` (with `--resume`) opens the pane on a copy of the session, and
`--at <message>` copies it only through that message's turn (`crate::agent_fork`, which
`zz tools --fork-chat` uses too): a Claude Code fork copies the transcript under a new session id,
a Codex fork asks the app server for `thread/fork` with `lastTurnId`. The copy's id becomes the
pane's session. Forking from a message opens the copy as a PiP over the chat it came from, in the
same window, with one command line, since a new float's pane becomes the target of the next
command (`zz_client::agent_settings::fork_chat_command`, sent by the desktop and the thin clients):

- `new-pane -t %N -c <dir> -x 46% -y 72% ; select-pane-kind --resume <session> --fork [--at
  <message>] [--model <m>] [--effort <e>] [--account <id>] agent <claude|codex>`

The float takes no title of its own: the fork's session title names it, as for any chat. It
carries the source pane's account (`zz_client::agent_settings::pane_account`), so the copy runs on
the same login.

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
  since that turn lives in the old home's app server. `--now` moves it anyway (the reopen stops
  what runs), and `--when-idle` waits in the daemon, checking every 250 ms as an armed flip does,
  and moves it once nothing runs; another `agent-account` for the pane, or the pane no longer
  being a chat, drops the wait, which no client shows. Otherwise the daemon closes the driver and
  waits (`AgentRuntime::close_for_reopen`), sets the account (`MuxState::update_agent`), and opens
  the pane again on its session. The reopened stream carries on from the old one's sequence
  numbers, so a client keeps its cursor and takes the resume as a session reset.
- **`default`** means no account everywhere: `agent-account default` and `select-pane-kind
  --account default` both run the CLI's own login, and `valid_agent_account` refuses `default`, so
  a directory of that name is never listed, created or flipped into.
- **In the chat.** The composer footer's usage pill shows the account's limits, switches the
  account and signs one in (see [the footer](#the-empty-chat-and-the-footer) and
  [signing in](#in-the-chat)).
- **Rate limits.** What each account has used of its 5-hour and weekly limits; see
  [below](#rate-limits).
- **Not built yet.** Removing an account. A signed-out account directory is reused by the next add,
  and a sign-in the daemon drives removes the directory it created when it fails or is cancelled.

## Listing and adding accounts

`agent-accounts [--add claude|codex] [--account id|default] [--pane target-pane]
[--browser local|elsewhere] [--terminal]` (`crates/zz-daemon/src/daemon/accounts.rs`, internal, like
`providers`) answers on the client event stream with `EventPayload::AgentAccounts(Vec<AgentAccount>)`
(wire 122) and, for a sign-in, `EventPayload::AgentLogin`, never on stdout. `--account`, `--pane`,
`--browser` and `--terminal` need `--add`.

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
  `ZZ_SESSION`, `TMUX` and `TMUX_PANE` (and for an account the login overrides above) and give up
  after 10 s. A probe that fails, times out or gets no `loggedIn` from Claude Code leaves the
  status unknown (`identify` gives `None`): the list shows it as signed out, and a sign-in never
  picks it.
  An account directory is linked (`accounts::prepare`) before its probe, so a Codex home has its
  state databases before its first app server.
- **Watching.** A plain call adds the client to the watchers, sends the list the daemon already has
  (nothing before the first probe ends), and refreshes in a worker thread; a call during a refresh
  asks for one more pass. The whole list goes to every watcher still connected when it changes.
  Every plain call refreshes, which is how a login that finished shows up: the daemon does not
  follow the login window.
- **Which account.** `--add` subscribes the caller too. `--account default` signs in the CLI's own
  login, `--account <id>` an existing account directory (signed out or not). Without `--account`
  the daemon asks that CLI's accounts, picks the first account it read as signed out, else the
  lowest `aN` not taken (`a1`, `a2`, ...), and creates or completes it (`accounts::create`: 0700,
  links, Claude seed; `accounts::pick_account`).
- **Two ways to sign in.** A caller that declared `agent-login-v1` in its hello gets a sign-in the
  daemon drives ([below](#signing-in-without-a-terminal)). A caller without it, a one-shot CLI
  connection (it opens with `Exec` and declares nothing), and any `--terminal` call get the
  terminal window: a new window named after the provider and id (`default` for the CLI's own
  login) in the caller's session, running `claude auth login` or `codex login` through the user's
  interactive shell with `CLAUDE_CONFIG_DIR` or `CODEX_HOME` set for a named account
  (`cli_session::interactive_shell`). The login there is the CLI's own; zz types nothing into it
  and does not follow it.
- **Clients.** zz-client's core keeps the last list (`ClientCore::agent_accounts`) and emits
  `CoreEvent::AgentAccountsChanged`. The desktop keeps one list per host beside the providers report
  (`MuxClient::host_agent_accounts`, `request_agent_accounts`, `start_agent_login`); zz-app reads the
  core (`Connection::request_agent_accounts`, `start_agent_login`, `switch_agent_account`). The
  composer's usage pill is their one reader.

## Signing in without a terminal

The daemon runs the CLI's own login on a pipe nobody sees and reports each step to the clients
(`crates/zz-daemon/src/daemon/agent_login.rs`). zz has no OAuth client of its own: the unmodified
CLI opens the flow, exchanges the code and writes the credentials into the account's directory
(or the CLI's own place for `default`). zz never reads, stores or logs a token.

- **Browser.** `--browser local` says the client's browser runs on the daemon's machine (the
  desktop on its local daemon), so the CLI's callback on `localhost` can catch the redirect.
  `--browser elsewhere` (the default) is for a remote host, an iPad or the web: Codex uses its
  device code, Claude Code its manual page that shows a code for the user to paste back.
- **Claude Code.** `claude -p --output-format stream-json --input-format stream-json --verbose`,
  then the stream-json control requests the SDK uses: `initialize`, then `claude_authenticate`
  with `loginWithClaudeAi: true`, which answers `{manualUrl, automaticUrl}` and opens no browser.
  Local: the step shows `automaticUrl` and the daemon sends `claude_oauth_wait_for_completion`.
  Elsewhere: the step shows `manualUrl`; the pasted code (`code#state`, or the code alone, when
  the daemon takes `state` from `manualUrl`) goes back as `claude_oauth_callback`
  `{authorizationCode, state}`. Both answer with the account once the CLI has stored the login;
  an error answer is shown as the CLI wrote it. Checked on Claude Code 2.1.325.
- **Codex.** `codex app-server` on stdio: `initialize`, `initialized`, then `account/login/start`
  with `type: "chatgpt"` (local: `{loginId, authUrl}`, the callback listening on
  `127.0.0.1:1455` on the daemon's machine) or `type: "chatgptDeviceCode"` (elsewhere:
  `{loginId, verificationUrl, userCode}`). The notification `account/login/completed {loginId,
  success, error}` ends it. An error that says device code login is not enabled becomes
  `DeviceCodeDisabled`, one that says it timed out `TimedOut`. Checked on codex-cli 0.169.0.
- **Steps.** `AgentLoginStep` in `zz_protocol::providers`: `Starting`, `OpenUrl { url }` (local
  browser, waiting), `DeviceCode { url, code }`, `PasteCode { url }`, `Verifying` (a pasted code is
  being exchanged, or the account is being probed), `Done { email, organization, plan, pane }`,
  `Failed(AgentLoginFailure)` (`NotInstalled`, `DeviceCodeDisabled`, `TimedOut`, or `Cli(text)`;
  `AgentLoginFailure::message` words each) and `Cancelled`.
- **The machine.** `agent::login::LoginMachine` on zz-machine decides; a driver thread per login
  owns the CLI process, feeds it facts (the CLI's answers, the client's code, cancel or restart, a
  tick) and runs its effects (launch, stop the CLI, send the code, probe, switch the pane, remove
  the directory). Starting has 1 minute to show a link and exchanging a pasted code 1 minute to
  finish; a link waits 15 minutes. A step is published when it or the browser changes; a final one
  only after its effects ran.
- **One per provider.** A daemon runs one sign-in per provider. Another `--add` for that provider
  while it runs restarts it in the other browser mode when `--browser` differs (same login id,
  account and pane, a new CLI process) and otherwise sends the requester the current step.
- **Done.** The daemon probes that account again as the list does, puts the answer in the list and
  publishes `AgentAccounts`. With `--pane` naming an agent chat of the same provider, an idle chat
  moves to the account as `agent-account -t` moves it (`switch_agent_account`, which reopens it
  even when it already was on that account, so a chat that failed for want of a login picks it
  up) and `pane` is `Switched`; a busy one is left alone and `pane` is `Busy`.
- **Failed or cancelled.** The CLI is killed with its process group. An account directory this
  sign-in created is removed; one it reused, or `default`, is kept.
- **The pasted code.** It travels as `ProtocolMessage::AgentLogin(AgentLoginRequest::Code)`, never
  as a command argument, so it stays out of command history. `AgentLoginCode` prints as
  `AgentLoginCode(..)` in `Debug`, so the client's and daemon's trace logs never show it. The driver
  holds it until it writes the control request to the CLI's stdin, then drops it.
- **Events.** Each step goes to every client that declared `agent-login-v1`, as
  `EventPayload::AgentLogin(AgentLogin { id, provider, account, pane, browser, step })`
  ([wire protocol](/protocol/wire-protocol.md#adding-a-message-inside-an-epoch)); a plain
  `agent-accounts` call sends it the logins still running.
- **Client core.** `ClientCore::agent_logins()`, `agent_login(provider)` and
  `dismiss_agent_login(id)` (drops a finished one) with `CoreEvent::AgentLoginChanged { provider }`;
  `zz_client::agent_login` builds the requests: `start(provider, LoginAccount, pane, browser)` and
  `restart(login, browser)` (commands), `submit_code(login, code)` and `cancel(login)` (messages),
  and `daemon_signs_in(capabilities)` reads the daemon's hello.
- **Who declares it.** `agent-login-v1` sits in `OPT_IN_CLIENT_CAPABILITY_NAMES`, which the daemon
  accepts and no hello sends by itself. The clients that draw the card name it: the desktop app in
  both of its interactive connections (`connect_interactive_client` in `crates/zz/src/lib.rs` for the
  local daemon, `MuxClient`'s host connect for every other host), and zz-app in the native
  transport (iOS, Android: `clients/app/src/transport.rs`) and the browser hello
  (`browser::hello` in `clients/app/src/connection.rs`). The TUI, the HTML client and one-shot CLI
  connections do not, so they keep the terminal window.
- **Trying it.** `zz tools --agent-login <claude|codex> [--browser local|elsewhere]
  [--account id|default] [--pane %N]` connects with the capability, starts a sign-in and prints
  each step; a line on stdin is the pasted code, `local` or `elsewhere` restarts in that mode, and
  `cancel` or the end of input cancels.

## In the chat

Every entry point names the chat's pane, so the steps land in that chat
(`zz_client::agent_login::entry`). Against a daemon without `agent-login-v1` it sends the plain
`agent-accounts --add <provider>` as before, the pill shows no Sign in, and the terminal window
opens.

- **Entry points.** In the usage pill's menu Add account starts a sign-in for a new account, and a
  signed-out account's row reads Sign in at its end; picking it signs that account in, the default
  one too (`LoginAccount::of`). On iPhone the same rows sit on the plus sheet's Account page.
- **Browser.** `--browser local` only from the desktop on its own machine
  (`agent_login::browser(host == HostId::LOCAL)` in `MuxClient::start_agent_login`); the desktop on
  a remote host, iPad, iPhone, Android and the web send `elsewhere`. On `OpenUrl` with a local
  browser the desktop opens the link in the system browser once per login id
  (`MuxClient::observe_agent_login`).
- **The card.** `zz_client::agent_login::card` turns a step into what to show (title, glyph, line,
  link, code, field, actions, and whether X cancels or closes); `zz_ui::agent::login::login_card`
  draws it for the desktop (`AgentView::render_login`, `crates/zz/src/agent/view/harness.rs`) and
  zz-app (`AgentPane::render_login`, `clients/app/src/app/agent_pane/login.rs`) in the composer's
  prefix, right above the field, in the chat whose pane the sign-in names, wherever it was started.
  It has the busy card's look (`ChipLook::Floating`, 10 px in, a 13 px glyph 7 px from a 13/16
  title) and leaves the chat usable.
  - Starting and Verifying: "Sign in to Claude" (or ChatGPT) with the spinner.
  - `OpenUrl`: "Sign in to Claude in your browser", the link row (a 32 px well: globe, the URL
    without its scheme in mono, Copy link, Open again) and Use a code instead, which restarts the
    sign-in as `elsewhere`.
  - `PasteCode`: the link row, a field for the code and Continue; Enter sends too. The field is an
    `InputState` of its own (`CodeField`), made for that login id and dropped when the step moves
    on; `CodeField::take` hands its text to `submit_code` alone and empties it, which also clears
    its undo history. Nothing else keeps the code: not the composer draft, the view state, a log
    or a preference.
  - `DeviceCode`: the link row and the code in a 64 px well, 26 px mono, with Copy code.
  - `Failed`: "Couldn't sign in to ..." and one line (`failure_line`: not installed, device code
    sign-in is off for this account, the sign-in timed out, or the CLI's own text), Sign in in a
    terminal (`--terminal`) and Retry, both with the same account and browser; when the account the
    failed sign-in made is gone from the list they start a new one (`agent_login::retry`).
  - `Done` with the chat moved, or no chat: the client dismisses the login, the card goes and the
    usage pill's menu opens on the new account (`UsagePill::open`, a one-time `Popover::open`).
    `Done` with the chat busy: "Added <email>", "Switching now stops what is running.", Switch when
    idle and Switch now, which send `agent-account --when-idle` or `--now` through
    `AgentFlips::switch_account_when`.
  - `Cancelled`: the card goes.

  X cancels a running sign-in (`cancel`) and closes a finished card.
- **iPhone.** The plus sheet gets a Sign in page after Account (`PlusPage::SignIn`,
  `PlusSheet::sign_in` in `crates/zz-ui/src/agent/phone.rs`), titled like the card ("Couldn't sign
  in" when it failed), with back and X in the header: back returns to Account and leaves the
  sign-in running, X cancels. The link is a whole touch row (Open sign-in page, the site under it)
  that opens the page, the device code a tile that copies on tap, the field touch-large, and the
  actions tall full-width buttons. When it is done the sheet goes back to Account with the new
  account checked; a busy chat gets the Switch page without back. A sign-in started on another
  client for the chat opens the page when no sheet is up.
- **Storybook.** Agent > Sign in draws every card state from fixed steps
  (`clients/storybook/src/stories/agent/login.rs`), Agent > Composer controls the menu with Sign in,
  and Phone > Plus sheet the phone pages.

## Rate limits

Quota belongs to an account, not a pane: up to two windows, the 5-hour one and the week, each a
used fraction and a reset time (`QuotaWindows`, `QuotaWindow { used, resets_at }` in
`zz_protocol::providers`). zz calls no usage endpoint and reads no token; it keeps what the CLIs
emit or answer, in the daemon's memory only, one record per provider and account (`None` is the
CLI's own login; `Accounts.quotas` in `daemon/accounts.rs`).

- **Claude Code** writes `rate_limit_event` on its stream once per turn, with
  `rate_limit_info.unifiedWindows.five_hour` and `seven_day`, each `utilization` (0 to 1) and
  `resetsAt` (unix seconds); the driver reads both (`claude::translate::rate_limits`). Without a
  turn, the driver asks the CLI itself: once a chat's CLI answers `initialize`, it sends the control
  request `{"subtype":"get_usage","skip_behaviors":true}`, which the CLI answers from the plan's
  usage with no model call (`skip_behaviors` skips its scan of local transcripts). The answer's
  `rate_limits.five_hour` and `seven_day` carry `utilization` (0 to 100) and `resets_at` (ISO 8601)
  (`claude::translate::usage_rate_limits`); a window the plan has not started is `null` and is left
  out. So every chat start fills its account's limits about a second after the CLI is up: a new
  chat, a resumed one, a fork, and the reopen that follows a switch or a finished sign-in. Claude
  Code 2.1.325 answers it; an older CLI refuses it, the error is dropped, and the limits wait for a
  turn as before. A sign-in with no chat to move (`zz tools --agent-login` without `--pane`) leaves
  the new account without limits until a chat opens on it: `claude auth status --json` and the
  `initialize` answer carry none.
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
  its last value; a probe answer replaces the record. Records are never dropped while the daemon
  runs, and clients get every account's, so a chat moved to an account used before shows its last
  figures at once, before its own `get_usage` answer comes in.
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
  the transcript's last assistant line (`requestedModel`, which keeps a suffix like `[1m]`, else
  `message.model`, which drops it; and `effort`) and its "Set model to" and "Set
  effort level to" lines, over the CLI's arguments and settings files (`Facts::claude_launch`);
  a model name the CLI printed becomes its alias for `--model`.
- **Codex.** The thread is the parentless rollout the TUI holds open (embedded), else the one
  `codex resume <id>` names, else the one parentless rollout in the TUI's folder modified since it
  started; two such rollouts refuse the flip and ask for `codex resume <id>`. A newer rollout in
  the folder may be another pane's, so it never beats the named thread. A TUI on the shared app
  server holds no rollout, so after `/new` or `/resume` inside it the flip still opens the thread
  its command line names. Model and effort come from the rollout's `turn_context` and
  `thread_settings_applied` lines over `config.toml` and the arguments.
- **Antigravity.** The conversation is the one whose `conversations/<uuid>.db` file the `agy`
  process holds open, else the one `--conversation <id>` names (`cli_session::agy_session`). Model
  and effort come from its `--model` and `--effort` arguments only.

# Antigravity

Antigravity (`AgentProvider::Antigravity`; `agent agy` or `agent antigravity`, `@+agy`, `(agy)` in
reports) is the third provider. Its CLI has an ACP server, `agy acp`, so its driver
(`crates/zz-daemon/src/agent/agy.rs`) is the generic ACP runtime (`runtime::run_acp_agent`) behind
`agent/dialect.rs`, which Grok shares, with four changes:

- **Which agy.** The driver runs the first `agy` on the agent PATH whose `agy acp --help` names the
  subcommand. An install from before the ACP server (1.1.x) prints the general usage there and is
  skipped. When every `agy` found is that old the pane fails with `Antigravity at <path> is too old
  for chats: it has no agy acp. Update it with agy update.`, and with none it fails with the not
  installed line.
- **Effort.** agy calls its effort option `thought_level`. The driver renames it to `effort` in
  every option list it sends on (`SessionReady`, `SessionSwitched`, `ConfigOptionsChanged`,
  `config_option_update`) and back on `SetConfigOption`, so the chat, `--effort` and the flip read
  it as they do the other providers'.
- **Title.** agy sends `session_info_update` with the conversation's title after the first prompt
  and on load. The driver turns it into `TitleChanged`.
- **Session list.** agy's `session/list` ignores the folder it is asked for, so the driver keeps
  only that folder's sessions. A conversation with no recorded folder comes back under the folder
  the server runs in.

The modes are `default`, `accept-edits`, `auto` and `plan` (`agent_settings::mode_risk`: `plan` is
read-only, `accept-edits` and `auto` act unasked).

**The flip.** Chat to CLI runs `agy --conversation <id> --model <m> --effort <e>`. CLI to chat
resumes the conversation found above, and a TUI with no conversation yet starts a new chat.
Nothing on disk says an agy TUI is busy, so it never refuses the flip: a turn running in the TUI
stops with it.

**Not there.** Accounts: agy documents no variable for its home, so `--account`, `agent-account`
and `agent-accounts --add agy` refuse, and agy has no rows in the account list or the limits. The
sign-in the daemon drives: sign in by running `agy`. Forks: `--fork` logs and starts a new chat.
The command is not configurable: there is no `agent-antigravity-command`. Settings reads the latest
version from the manifest agy's own updater reads, Update runs `agy update` and Install runs the
vendor's install script.

# Grok

Grok (`AgentProvider::Grok`; `agent grok`, `@+grok`, `(grok)` in reports) is the fourth provider.
Its CLI has an ACP server, `grok agent stdio`, so its driver (`crates/zz-daemon/src/agent/grok.rs`)
is the same runtime behind `agent/dialect.rs` that Antigravity uses. `dialect::run` takes a
`Dialect`, the provider's name for the effort option and its signed-out line, and does the rest
the same for both:

- **The transcript.** The clients draw a chat from `Chat` events only, and the ACP runtime sends
  none. `agent/acp_chat.rs` (`Transcript`) feeds what the runtime sends through the chat bridge the
  other two drivers use: a prompt's echo (`messageId` `zz-prompt-<turn>-…`) or `PromptAccepted`
  starts the turn, `PromptFinished` ends it as completed, cancelled or failed, a permission answer
  is recorded as the answer, and history replayed by `session/load` gets a turn for each user
  message. A text chunk with no `messageId` (grok sends none) is given one per run of chunks, so
  the text before a tool call and the text after it are two items in that order. A `SessionReset`
  after the session went live (a sign-in that opens the session again, a switch) starts the
  transcript over and sends `ChatEvent::Restart`, so the history the agent replays on load shows
  once. The `Chat` events follow the payload that caused them.
- **Effort, title, session list.** The effort rename, `session_info_update` to `TitleChanged`, and
  the session list kept to the folder asked for. Folders are compared resolved, so `/tmp/x` and
  `/private/tmp/x` are one, and a kept session carries the folder as it was asked for. A new
  session (`SessionReset` that restores nothing) clears the title. A new chat or a switch
  (`SessionSwitched`) takes the stored title of the session it lands on, or clears it, unless the
  agent named that session while it loaded.
- **A failed start.** `run_acp_agent` reports a `session/new` that fails as `PaneFailed` and keeps
  running, so the pane can still be told to sign in or to open again.
- **Stop.** The runtime sends `session/cancel`, answers the turn's open asks as cancelled, and ends
  the turn as cancelled when the agent answers the cancelled `session/prompt`, so what the agent
  sends while it stops stays in that turn. An agent that has not answered after 5 s
  (`CANCEL_GRACE`) has its turn ended anyway. The queue is handed back at Stop, and a prompt
  queued while the turn is stopping is handed back the same way when the turn ends.

- **Which grok.** The first `grok` on the agent PATH. With none the pane fails with the not
  installed line.
- **Effort.** grok calls its effort option `reasoning_effort`; the chat reads it as `effort`.
- **Signed out.** grok answers `session/new` with `Authentication required`. The dialect turns a
  pane failure that starts with those words into the provider's line, `Grok is signed out. Sign in
  by running grok login in a terminal.`, and Antigravity's the same way.

- **Usage.** grok answers the extension request `_x.ai/billing` with the plan's credit use and
  its period. A dialect with `quota` asks it (`RuntimeCommand::Ask`) when a session is ready and
  after each turn, and a weekly period becomes the week window of `QuotaChanged`.
- **Title on load.** grok sends `session_info_update` when it titles a new conversation and not
  when one is loaded, so a loaded session with no title yet takes `generated_title` (else
  `session_summary`) from its `summary.json` (`Dialect::stored_title`).

grok reports no modes. Whether it asks before a tool runs is its own setting (`permission_mode` in
grok's config), so a chat on an always-approve config never asks.

**The flip.** Chat to CLI runs `grok --resume <id> --model <m> --effort <e>`. CLI to chat reads
the TUI's session from `active_sessions.json` in grok's home (the daemon's `GROK_HOME`, else
`~/.grok`): the row whose `pid` is the TUI's names its `session_id`. A TUI started with another
`GROK_HOME` refuses the flip, since the chat would not find its session. With
no row it takes a session id given to `--resume` or `--session-id`, and with neither starts a new
chat. The model and effort come from that session's `summary.json` (`current_model_id`,
`reasoning_effort`), then from `--model` and `--effort`. Nothing read says a grok TUI is busy, so
it never refuses the flip.

**Not there.** Context use (grok sends no `usage_update`), accounts (`--account`, `agent-account`,
`agent-accounts --add grok` refuse, though `GROK_HOME` would allow them; the clients offer no
Add account for a provider where `AgentProvider::accounts` is false), the sign-in the daemon drives (run `grok login`), and forks.
The command is not configurable. Settings reads the latest version from `grok update --check
--json`, Update runs `grok update` and Install the vendor's script.

**Seen working** (2026-11-20, grok 1.0.89): a turn with tool calls, the title, `--model` and
`--effort`, chat to TUI, a TUI with an open session to a chat, a TUI given `--resume` to a chat
with its history, and a resumed session's title. Not seen: a permission ask, since the config
tested approves everything.

**Known gaps in the shared runtime.** A `session/load` that fails falls back to a new session
with the old transcript replayed over it. A load that replays more than 4,096 updates or 18 MiB
opens on the newest and drops the oldest, with a log line only: nothing in the chat says older
history was left out. The latest title among the dropped updates is still applied. A pane with two TUIs of one provider flips the first
one found. A client that recovers from the journal sees the agent's own name for the effort
option until the recovery metadata that follows replaces the list.

# Cursor

Cursor (`AgentProvider::Cursor`; `agent cursor`, `@+cursor`, `(cursor-agent)` in reports) is the
fifth provider, and a chat only. Its CLI has an ACP server, `cursor-agent acp`, and its driver
(`crates/zz-daemon/src/agent/cursor.rs`) is the third on `agent/dialect.rs`.

- **Which CLI.** The first `cursor-agent` on the agent PATH, else an `agent` there that resolves
  into a `cursor-agent` folder (the vendor installs both names).
- **Sign-in first.** Cursor refuses `session/new` until the client has called `authenticate` with
  `cursor_login`, signed in or not. A dialect with `sign_in` holds the pane's first `Open`, sends
  `Authenticate` in its place, and sends the held `Open` when `Authenticated` comes back, which it
  keeps from the host so the host does not open a second time. `AuthenticationFailed` reaches the
  pane as the signed-out line, `Cursor is signed out. Sign in by running cursor-agent login in a
  terminal.`
- **No browser.** The ACP process runs with `NO_OPEN_BROWSER=1`, so a signed-out `authenticate`
  fails at once with the vendor's login link, where without it the CLI opens a browser and waits.
- **Models.** Cursor's model values carry their parameters, `composer-2.5[fast=true]`, it refuses
  a bare name, and it names each choice by its slug. A dialect with `model_name` remembers the
  values last offered, turns a bare `--model composer-2.5` into the offered value that starts with
  `composer-2.5[`, and renames each choice from its value: the slug in words, then the effort and
  `Fast` when the parameters carry them (`grok-4.6[effort=high,fast=true]` is `Grok 4.6 High
  Fast`). Effort is one of those parameters, so there is no effort option.
- **Modes.** `agent`, `plan` and `ask` come as the `mode` config option, which is where the
  clients read modes; `plan` and `ask` are read-only (`agent_settings::mode_risk`).
- **Run everything.** Cursor has no mode that stops asking: that is `approvalMode` in its
  `cli-config.json`, `allowlist` (ask for a shell command not on the list) or `unrestricted`. A
  dialect with `unguarded` adds a `full-access` choice named Run everything to the mode option.
  Under `unrestricted` it stands in for `agent`, which never asks there. Under `allowlist` it sits
  beside `agent`, and choosing it keeps Cursor in `agent` and raises this chat's auto-approve tier
  to all until another mode is chosen.
- **Title on load.** Like grok, Cursor titles a new chat and says nothing on load, so a loaded
  session takes `title` from `acp-sessions/<id>/meta.json` in Cursor's folder (`CURSOR_CONFIG_DIR`,
  else `~/.cursor`).

A prompt sent to a chat that failed to sign in waits in the queue, since the process is still
there to sign in to.

**No flip.** Cursor keeps ACP sessions in `acp-sessions/` and its CLI's chats in `chats/`. The
CLI started with a chat's id opens an empty conversation under that id, and the ACP server
answers `Session not found` for a CLI chat. So `flip-agent` refuses a Cursor chat
(`AgentProvider::flips`), the clients offer no switch for one, and a terminal running
`cursor-agent` is not taken for an agent.

**Not there.** Usage (the CLI gives the plan's name and no numbers, and Cursor sends no
`usage_update`), accounts, the sign-in the daemon drives, forks, and answers to
Cursor's own blocking requests (`cursor/ask_question`, `cursor/create_plan`), which the runtime
refuses as unknown methods. Settings reads the latest version from `cursor-agent about --format
json`, Update runs `cursor-agent update` and Install the vendor's script. The daemon runs those
with `CURSOR_AGENT_CLI_ASSUME_SSH=0`: a daemon started over ssh carries `SSH_CONNECTION`, and with
it set every `cursor-agent` command first probes the login keychain and exits 1 when it is locked,
`--version` included.

**Seen working** (2026-11-20, CLI 2026.09.18): sign-in first, a turn with tool calls, the title,
a bare `--model`, the mode and model options set over ACP, a resumed session with its history and
title, the flip refused, and in the app the renamed model and Run everything under an
`unrestricted` config. Not seen: Run everything chosen under `allowlist`, and the two blocking
requests.

# Codex on the shared app server

A Codex TUI runs its threads in the app server daemon it starts (`codex app-server daemon start`),
reached over a WebSocket on `$CODEX_HOME/app-server-control/app-server-control.sock`. When
`agent-command` is plain `codex`, the chat driver connects there too (`Process::connect`,
`codex::managed_daemon`) instead of starting an app server of its own, starting the daemon first
when its socket is missing (without the pane's `ZZ_PANE`, `ZZ_SESSION`, `TMUX` or `TMUX_PANE`).
It takes frames up to 64 MiB there, the line size it takes from an app server of its own
(`MAX_FRAME_BYTES` in `agent/child.rs`), so a long `thread/read` arrives whole; a larger frame
closes the connection, where the line would be dropped. A thread then outlives whichever side drove it: a flip kills the TUI and `thread/resume`s, or
closes the chat's connection and runs `codex resume`, mid-turn included, and the server unloads an
idle thread with no client 60 s later. The server sends no `turn/started` to a connection that
joins mid-turn, so a chat that opens a thread whose `thread/read` says `status.type` `active` with
its last turn `inProgress` starts that turn as running itself: Stop interrupts it, and its
`turn/completed` settles the chat. A configured command with arguments keeps an app server of
its own per chat. A chat that moves to another thread (new chat, switch, fork, rewind) sends
`thread/unsubscribe` for the thread it left and the subagent threads it saw that thread spawn,
drops their events and answers a request still coming from them with `cancel`, so the new chat
never shows them; a TUI on the old thread keeps getting them.

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
- **Steer.** `/steer <text>` (and `AgentSteer`) adds text to the running turn; with no turn running
  the host sends the text as a prompt. Codex takes it as `turn/steer` on the turn's id. The driver
  holds a steer until Codex names the turn, and a steer Codex refuses, or one that finds the turn
  already over, goes out as the next prompt: a plain user message, ahead of anything queued. A held
  steer goes back to the composer when the turn is stopped or fails, as the queue does. Claude Code
  takes a steer as a user message with `priority: "now"`; one that reaches its driver after the turn
  ended goes out as the next prompt the same way, and one that arrives while the turn is stopping or
  the CLI is down goes back to the composer.

The driver follows a `/clear`'s new session id (`follow_session`), and the empty `result` frames
the CLI writes while it resumes (`num_turns: 0`, before its first idle `session_state_changed`)
end no turn.

# From outside: the CLI

- `agent-send [-t %N] TEXT` to an agent pane submits a prompt. `--wait`, `--json`, `--final`,
  `--on-block`, `--notify` and `--progress` work on its turn as they did for the old agent pane.
- `agent-send` with text to a terminal running Claude Code, Codex, Antigravity or Grok fails with "runs Claude Code in
  its terminal; flip the pane to chat first". Nothing types prompts into a TUI or posts them to its
  peer socket. `--wait ''` still waits on such a terminal's `@agent_state`.
- `agent-respond` answers a pending permission in an agent pane.
- `flip-agent` and `select-pane-kind ... agent` as above.
- Formats read a chat like a terminal. `#{pane_current_command}` is the provider's command name
  (`claude`, `codex`, `AgentProvider::command_name`). `#{agent_state}` maps the chat's published
  phase (`agent_chat_facts`, `crates/zz-daemon/src/daemon.rs`): `starting`, `idle`, `working`,
  `needs-approval` for a pending permission, `needs-input` for a pending question, `failed`.
  `#{agent_pending_permission}` is the pending request id. The daemon keeps both on the engine
  (`MuxEngine::set_agent_chat`) when the runtime publishes a state, so `list-panes -F`,
  `display-message -p` and `inspect` read the same values, and fires `agent-state-changed` with
  both when either changes. `inspect` adds `agent_provider`, `agent_model` (the model the chat was
  started with) and `agent_session_id`.
- `send-keys`, `send-text`, `paste-buffer` and `run-pane` on a chat fail with
  `%N is an agent chat; send with: zz agent-send -t %N TEXT`, and `pipe-pane` with
  `%N is an agent chat; read it with: zz capture-pane -p -t %N`. A `synchronize-panes` send
  whose window holds a chat fails with "pane has exited".
- `capture-pane -p -t %N` on a chat prints its last turn as text, and `-S` (any value, so the
  context's `-S -`) the whole chat, capped at the last 64 KiB (`agent::capture`). A prompt prints
  as `> ` and its text, later lines indented two spaces; a tool call as its kind word and title
  on one line (`execute cargo test`); a reply as its text; a blank line follows the prompt and
  separates turns. Reasoning, plans, notices, errors and subagent steps are left out, and so is
  the mention note, which never joins the user's text. An injected `[zz] ...` prompt prints like
  any prompt. An empty chat prints nothing. The text comes from the chat state the daemon keeps
  per pane for every client (`PaneLane::chat`, the `ChatLog` a client's replay recovers from):
  the Claude Code and Codex drivers fill it, it is complete rather than byte-capped, and a
  restored chat refills it when the driver replays its session, so it needs no client. A chat
  on another ACP agent has no chat state, so it is read from the pane's retained stream
  instead, which holds only the recent part. Without `-p` the text goes into a paste buffer, as
  for a terminal; `-J`, `-e`, `-N`, `-T`, `-a`, `-q`, `-C`, `-E` and the value of `-S` change
  nothing.

# What the clients draw

- **The flip button.** A terminal running either CLI and an agent pane carry a flip button in their
  header that sends `flip-agent -t <pane>`. It is one of the pane's controls
  (`zz_ui::pane::PaneActions::flip`, `pane_flip_button`): the same bar button as split and close,
  just before them, hidden with them on a pane that is not focused until the header is hovered,
  and a row of the More menu when the header is too narrow.
  A `busy:` answer offers Switch when idle and Switch now; an armed flip shows with Cancel.
  `zz_client::agent_flip` (`AgentFlips`) keeps that state per pane for every client, and the
  composer's account switch (`agent-account`) rides the same requests and busy card.
- **The chat.** `crates/zz/src/agent` on the desktop and `clients/app/src/app/agent_pane.rs` on
  web, iPad and iPhone draw an agent pane with `zz_ui::agent::chat`: the composer, the model and
  effort menu, questions and approvals asked in the composer, subagent rows, images, references,
  Copy and Fork, and preview cards. A send the agent cannot take says why above the composer, in
  the shared words of `zz_client::agent_lens` (`prompt_not_ready`, `AGENT_QUEUE_FULL` at
  `MAX_AGENT_QUEUED_PROMPTS`, `AGENT_REFUSES_IMAGES` when the driver's `ready` capabilities lack
  `images`), and an empty chat reads Resuming the agent's session while a chat reopens and
  Cancelling the current turn after Stop (`agent_empty` with `LensTranscript::settling`, read
  from the stream's `sessionReset` `restoring` flag). A pane whose driver's `ready` capabilities
  set `watch` shows the chat with no composer, only what sits above it (queue, plan, errors), and
  an empty chat with the welcome alone, no folder button or Resume, on both
  (`zz_ui::agent::controls::watch_composer`). On iPhone the plan tray is a whole-row button over
  the composer that unfolds the plan above it, scrolling once it passes half the space between
  the top inset and the dock (`zz_ui::agent::tasks::phone_plan_tray`), and the plus sheet's first
  page ends with the chat's Git summary. A code block's Preview, Open and Copy take a 44pt touch
  area on iPhone, spaced so the areas do not overlap. The web client's composer has an
  image button beside the plus (an Images row in the plus sheet on a narrow window) that opens the
  browser's file picker (`attachments::choose_images`), shown when the agent takes images; iOS has
  no picker yet. Both open subagents, images and cards in the
  [view pane](#previews-and-the-view-pane); a host that gives the chat no view opener (the
  storybook) keeps the subagent PiP and draws diagrams and code inline. The storybook's Agent story
  draws them from fixed chat events. The HTML client (`clients/dom/src/agent.rs` and
  `web/js/agent.js`) draws its own timeline from `zz_client::agent_timeline` over the older
  transcript model, not from `chat_rows`, so its runs, steps and subagents follow other rules.

# The empty chat and the footer

zz keeps no chats of its own. A chat with nothing in it shows the welcome and, centered under it,
two buttons 6 px apart (`zz_ui::agent::resume::folder_button` and `resume_button`): the folder
the next chat starts in, and Resume, for a chat the CLI already has. Each opens a menu.

- **From a chat that has messages.** The two buttons are gone once the chat shows, so on the
  desktop the pane header gets a Chats button (a clock, `AgentView::header_buttons`), a pane
  control like the flip beside it. It asks for the listings afresh and opens the same popover
  under the header's right end (`zz_ui::agent::resume::HeaderMenu`, `Menu::Switch`): the Resume
  rows, then New chat (`AgentSessionOp::New` in the pane's folder, which lands on the empty chat)
  and Change folder…, which turns the popover into the folder menu. A search hides the two
  actions. The daemon refuses a switch while a turn runs, and the chat shows its message. The
  thin clients do not draw this button yet.
- **The listings.** While the pane is ready and its chat is empty, the client asks the pane's
  driver for two lists with `AgentSessionOp::List` (`zz_client::agent_resume::Resume::start`): the
  chats in the pane's folder, and the newest chats in any folder. Claude Code's come from
  `~/.claude/projects`: the driver reads the first and last 64 KiB of each transcript once and
  keeps that summary while the file's mtime and size stay the same (`sessions::list`), and keeps
  the whole sorted list per folder filter so a page after the first is a slice of it, about 12 ms,
  not a new walk. Codex's come from `thread/list` with `archived: false` and `useStateDbOnly:
  true`, so Codex answers from its state database instead of scanning its rollouts. The reply is
  `EventPayload::AgentSessions`, which `MuxClient` hands to `AgentController::apply_sessions`
  on the desktop and `CoreEvent::AgentSessions` hands to the thin client's agent pane
  (`clients/app/src/app/agent_pane/resume.rs`), each keeping its own `Resume`.
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
  `on_toggle`). The thin client's popovers take Up, Down and Enter the same way, but Up from
  the composer does not open Resume there; while a menu is open the pane's focus target is
  its search field.
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
    - A signed-out account's row reads Sign in at its end, in the muted small style, instead of the
      check, and picking it signs that account in, when the daemon signs in itself
      (`UsagePill::signs_in`).
    - Under a rule, Add account starts a sign-in for a new account of the pane's provider
      ([in the chat](#in-the-chat)). Both close the popover, which opens again on the new account
      once the sign-in is done.
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
    Shift-Tab in the composer steps to the next mode (`LensSettings::next_mode`), on the desktop
    and in the thin client with a keyboard, except while the `@` menu is open.
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
- **Where.** The desktop has all of it. The empty chat (the sky behind the welcome, the two
  buttons and their popovers) is one widget, `zz_ui::agent::resume::EmptyChat`, which the
  desktop view and the thin client's wide layout (iPad and the web, 640 pt and up) both draw.
  On iPhone the two are rows under the title that open sheets (`zz_ui::agent::phone::start_rows`
  and `resume_sheet`), with no sky. Web, iPad and iPhone have the footer, the usage pill
  included, and the HTML client has neither. The storybook's
  Agent story shows the menus (Empty chat: at rest, each menu open, searching, loading and Nothing
  matches) and the controls (Composer controls: the pill in a new chat, at each context fill and before
  the use is known, a model pill per Claude Code mode and per Codex mode, then the footer as
  usual, in a new chat, before the use is known, with a 5-hour window at 92%,
  with an account that has no limits yet, with two accounts and the second picked, with a
  signed-out account to sign in, and with a signed-out default).

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
  the input, and `mention_keys` keeps Up, Down, Ctrl-N, Ctrl-P, Tab, Shift-Tab, Backspace at a
  level's edge, Enter, Alt-Enter and Escape, the palette's key model
  ([command palette](/concepts/command-palette.md#chips-and-keys)).
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
  `runtime::prompt_updates` sends a prompt over 128 KiB as several text chunks of one message; every
  chunk but the last carries `_meta.zz.more`, and the bridge keeps the user item open until the
  last one, so the prompt is one bubble live and after a replay.
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
- **Live turns.** The last line of a running turn says what the agent is doing now, in a muted label
  with no glyph, and the time since the prompt (`chat_rows::live_line`); each new label comes into
  focus from left to right (penna's `ActivityMark.live`). It takes the newest of:
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
- **Claude Code's background agents.** An agent started in the background keeps running after
  the turn that launched it ends, and can still ask for approval then. Its `can_use_tool` request
  names it in `agent_id`, the task id `background_tasks_changed` lists; an agent it starts asks
  with its own id, which `task_started` ties to it through `parent_task_id`. The driver marks an ask
  from either with `_meta.zz.backgroundAgent`, so the host keeps it past the end of the turn
  (`AgentPendingPermission::outlives_turn`) and both clients keep its card. Unlike Codex's
  questions it stays blocking: the chat says it waits for you, and the ask sits under the
  agent's row. Stop interrupts the foreground turn only, and Claude Code leaves a background
  agent running, so Stop denies the turn's own asks and leaves the agent's ask open.
- **Callouts.** `> [!NOTE]` style alerts and `:::name title` blocks draw as a flat card washed in
  their tone with an icon and a title (penna's `BlockNode::Callout`).
- **Preview cards.** With a view opener, mermaid and SVG blocks, code blocks of six lines or more
  and markdown fences of six lines or more leave the reply as cards (`agent::preview`), flat
  `CardLook::Activity` cards that wash deeper on hover. A diagram is a thumbnail of the drawn diagram
  named by its kind (Flowchart, Sequence diagram, Gantt chart; the pop-out mark shows on hover,
  always on touch). A
  code block is its language's tile (the program tile when zz knows the language: rust, python,
  go, typescript and the other program marks, plus 32 language marks from Devicon; a neutral tile
  with braces otherwise), its file name when the fence names one (`rust src/main.rs`,
  `rust:src/main.rs`, `title="..."`) or its language, and its length. A markdown fence is named by
  its first heading. Fences that follow each other share one row: the card plugin claims the next
  siblings with `MarkdownNode::absorbs`. Shorter code, blocks over 64 KiB and
  mermaid types pele does not draw stay inline. On the desktop, iOS and the web an `html preview` block is a visual
  drawn bare on the chat's ground with penna's HTML kit, and an `html` block holding a whole
  document is a page frame ([artifacts](/designs/userpanes.md#inline-previews-in-the-agent-pane)). Clicking a card opens it in the view pane.
- **UI blocks.** On the desktop a fenced `ui` block in a reply is drawn in the chat's own window
  with penna components, with no page and no browser process (`agent::ui`, on penna's
  `blocks::a2ui`). The block holds the compact lines penna's `express.md` describes, which the
  daemon appends to the agent pane context, or the same thing as A2UI JSON. It sits bare on the
  chat's ground like a visual, with no controls of its own. A button
  sends at once as the user's message, one line: `ui action NAME {...}` with the values the action
  listed, through `controller.prompt`, so a press mid-turn queues like a typed message. The bubble
  shows the button's label and those values instead of the line. A block is live until the user
  sends anything after it; from then on its controls are disabled and the pressed button keeps a
  check. That is read from the order of the chat (`ChatPane::sync_ui`), so nothing is stored and a
  resumed chat draws old blocks held. A block with no button is never held. While the fence is
  still open in a streaming reply the block is the visual's wash. A block that does not compile
  shows the first problem with Ask for a fix, which sends `ui problems [...]`; zz sends nothing on
  its own. A press inside a block keeps the chat's text selection from starting, so its fields
  take focus. The desktop and the thin clients both set `AgentUiSender`: the thin client sends the
  line as `ProtocolMessage::AgentPrompt` through `AgentPane::send_text`, which echoes it locally
  and counts it as queued mid-turn, like a typed message. On a phone (`ChatLook::touch`) a block
  is drawn for a thumb (`A2uiSurface::set_touch`): 44 buttons sharing their row, 56 fields as
  wide as the block, text on the touch ladder. A `Chart` in a block draws bars or lines for up to
  four series (penna's `components::chart`), so the context no longer points agents at
  `html preview` fragments; a fragment an agent writes anyway is still drawn. The bubble's values
  are in the muted ink of the bubble's ground. A client with no `AgentUiSender` (the HTML client)
  shows the fence as code.
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
  (`ChatHooks::owner`), so on the desktop, iPad and the web an HTML block's live page can draft a
  reply into the composer, and two
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
  older than the ring takes the reset path the runtime design describes. When the journal cannot
  vouch for the session either (it stops at 4,096 records or 18 MiB), only the client that asked
  gets a rebuild: a restoring reset, the closing metadata and the newest part of the chat that
  fits in 4.5 MiB (`MAX_REBUILD_BYTES`), on numbers it skips, so no other viewer is sent anything.
  Events that refer to the part left out count in `ChatView.skipped`. A driver that resumes a
  session replays its history from the CLI's own file first. A pane's numbers never start over
  within a daemon process, so a client's position stays good across reconnects to the same
  daemon (`server_id` in the hello).
- **Visibility.** The heavy stream goes to clients for whom the pane is an agent pane in their
  focused window, honoring zoom (`visible_agent_panes`), and to clients watching the pane.
  `AgentState` goes to every client attached to the session and to the pane's watchers, and a
  reattaching client gets it for every agent pane there (`send_agent_resync`, `session_agent_panes`).
- **The phone and web clients** (`clients/app/src/connection.rs`) keep a chat's buffer and
  position for as long as the pane stays a chat, across host and session switches. A chat is live
  while its connection is the host on screen, the pane is in the attached session and the shell
  drew its chat view in the last frame (`AppShell::drawn_agents`, handed over with
  `Connection::show_agents`; a view pane's origin chat counts as drawn): one chat on the phone,
  every chat in the window on wider layouts. Any other chat is paused: its updates are dropped
  and its position stays where it was. So an attach asks only for the chats on screen, and
  another chat of the session is fetched the first time it shows. Until its first items land the
  chat reads Resuming the agent's session (`Connection::agent_loading`), not the empty new chat.
  A chat that was never shown still gets its `AgentState` as part of the attached session, so its
  face, marks, unseen dot and chimes do not need the replay. When it turns live
  again the client sends `AgentReplay` from its position plus one (`AgentCursor::resume_from`), so
  the daemon sends only what it missed. A daemon with `agent-unwatch-v1` is told with
  `AgentUnwatch` when a chat pauses, and for every chat of the attached session that is not on
  screen, and again for every chat it keeps after a new hello, so the daemon stops sending their
  updates while `AgentState` keeps coming: the overview's working and waiting marks, the unseen
  dot and the done chime all read `AgentState`, not the updates. A chat with no position yet and
  one that stopped being a chat start over from 0. After a new hello the client keeps every position when the hello's `server_id` matches
  the one the positions came from and the daemon's revision is 8 or more; a daemon with the same
  `server_id` and an older revision keeps the list of chats it watches but not the positions, and a
  different `server_id` starts everything over. Each buffer carries a stream number
  (`Connection::agent_stream`); a chat view that sees a different number starts its transcript
  over. The buffers are capped by bytes only (`MAX_AGENT_HISTORY_BYTES`, 32 MiB per chat and per
  connection), and `AgentCursor::trimmed` records the newest item the cap dropped: a view that had
  not applied it yet asks for the whole chat again (`Connection::refetch_agent`) instead of drawing
  a chat with its start missing.
- **Watchers.** A client that sends `AgentReplay` for a pane watches it from then on, attached or
  not (`watched_agents` on the client entry): it gets the pane's `AgentUpdates`, `AgentState` and
  `AgentLagged` until it disconnects or the pane closes, and the pane's `AgentState` right away when
  the watch starts. A client can follow an agent this way without attaching to its session, which
  would resize its panes, and an attached client keeps a chat live while its focused window shows
  something else. A client that both sees and watches a pane is
  sent each frame once, and a watched pane leaving the visible set keeps its queued frames.
  `AgentUnwatch` (`agent-unwatch-v1`) mutes a pane's updates for its sender, seen or watched, and
  makes it a watcher of the pane's `AgentState`; the next `AgentReplay` unmutes it
  (`muted_agents` on the client entry).

# Related

- [Agent lens design](/designs/agent-lens.md) . the design this replaced
- [Daemon-owned agent runtime](/designs/agent-daemon-runtime.md) . host, fanout, journal
- [Native agent drivers](/designs/native-agent-drivers.md) . the drivers agent panes run
- [Chat mode](/designs/chat-mode.md) . what a new pane offers in each sidebar view
- [Agent peer bus](/architecture/agent-peer-bus.md)
- [Wire protocol](/protocol/wire-protocol.md)
- [tmux command set](/tmux/commands.md)
- [zz](/crates/zz.md) and [zz-daemon](/crates/zz-daemon.md)
