---
type: Concept
title: Agent panes and the flip
description: How a pane runs Claude Code or Codex as a chat (PaneKind::Agent over the vendor drivers), how a chat and its window are named after the CLI's session title, how flip-agent switches one pane between the CLI's own TUI and that chat on the same session (busy rules, --now, --when-idle and the armed flag, finding the TUI's session, waiting for the old process), forks and side chats, accounts (a CLI login per pane through CLAUDE_CONFIG_DIR or CODEX_HOME, and agent-account), Codex on its shared app server, the chat's slash list, agent-send and agent-respond, what the chat draws, the preview cards and the floating view pane they open, and how its stream reaches clients.
resource: crates/zz-daemon/src/daemon/agent_flip.rs
tags: [agent, agent-pane, flip, chat, claude-code, codex, driver, stream]
timestamp: 2026-11-10T00:00:00-03:00
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
- **Flips.** Chat to CLI runs the CLI as `CLAUDE_CONFIG_DIR=<dir> claude --resume ...` (or
  `CODEX_HOME=<dir> codex resume ...`) inside the user's interactive shell
  (`cli_session::interactive_shell`), so the variable reaches the TUI after the shell's startup
  files. CLI to chat reads the variable from the TUI's process (`process_info::environment`:
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
- **Not built yet.** Listing accounts with who is signed in, adding one (the add command will call
  `accounts::create`), the composer's account chip, and removing an account.

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
  `zz_client::agent_flip` (`AgentFlips`) keeps that state per pane for every client.
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

zz keeps no chats of its own. A chat with nothing in it lists what its CLI already has, and
picking one resumes it in the same pane.

- **The lists.** While the pane is ready and its chat is empty, the desktop asks the pane's driver
  for two lists with `AgentSessionOp::List` (`zz_client::agent_resume::Resume::start`): the chats
  in the pane's folder, and the newest chats in any folder. Claude Code's come from
  `~/.claude/projects`, Codex's from `thread/list`. The reply is `EventPayload::AgentSessions`,
  which `MuxClient` hands to `AgentController::apply_sessions`. `Resume::rows` turns them into
  Recent here (three chats, then More chats here) and Projects (the other folders the newest chats
  are in, five, then More projects, which also asks for the next page). The pane's own session is
  left out. A folder row shows its parent path and its last chat's age; there is no chat count,
  since the driver pages the list.
- **What is left out.** The daemon drops a chat whose folder no longer exists on its host
  (`zz-daemon/src/agent/runtime.rs`), since it cannot be resumed there: temp dirs, deleted
  worktrees, chats that ran on another machine. Projects also leaves out scratch folders (under a
  temp dir, `~/Library` or a dot folder; `scratch` in `agent_resume`), which the filter still
  finds. Because a page of the newest chats can then hold few folders, `Resume::wanted` asks for
  more pages, up to eight, until Projects has more than five.
- **The filter.** A field over the lists (`resume_filter`, the view's own `InputState`). With text
  in it `Resume::rows` returns one flat result instead: Folders whose path matches, then Chats
  whose title or folder name matches, newest first, each chat with its folder's name; the first
  row is picked. Inside an opened folder it narrows that folder's chats by title. While it has text
  `Resume::wanted` pages through the rest of the list, up to forty pages, so the match is not only
  the newest chats. Nothing matches is a row of its own.
- **Picking.** A chat sends `AgentSessionOp::Switch` with its session and folder: the driver
  restarts on that session, the pane moves to that folder and the transcript replays as it does on
  a restart. A folder asks for that folder's chats and shows them in place of the lists, with a
  back row; More chats asks for the next page. Up from an empty composer moves focus to the
  filter with the last row picked; from there typing filters, Up and Down move, Down past the last
  row returns to the composer, and Enter picks. Escape clears the filter, then drops the selection,
  then leaves a folder, then returns to the composer. The pointer selects the row under it. `zz_ui::agent::resume::resume_rows` draws the rows under
  `agent_resume_welcome`.
- **The footer.** The folder chip and the side chat button are gone: the header pill shows the
  folder, and `/side` opens a side chat. What is left is the branch summary on the left and two
  controls on the right (`zz_ui::agent::controls`):
  - `agent_mode_chip`: the driver's `mode` option, the permissions the CLI runs with. A click or
    Shift-Tab steps to the next mode (`LensSettings::next_mode`). `agent_settings::mode_risk` gives
    it a tone, and the send button takes the same one: none while the agent asks first (Claude's
    Ask, Codex's Auto), success when it cannot write (Plan, Read only), warning when it acts
    without asking (Accept edits, Claude's Auto), danger with nothing guarding it (Bypass
    permissions, Full access).
  - `agent_model_picker`: a popover over the footer. On top the models, a check on the current
    one: the first five the driver lists (`LensSettings::listed_models`) plus the current one when
    it is further down, and a More models row that adds the rest to the same list, which scrolls
    past eight rows. Under a rule the efforts as penna's `StepRail`, one horizontal rail with a dot
    per step that sets the effort when the knob is let go. Claude's first effort is `default`, so
    its first step is a ring (`effort_starts_unset`).
- **Where.** The desktop has all of it. Web, iPad and iPhone have the footer but not the lists yet,
  and the HTML client has neither. The storybook's Agent story shows the lists (Empty chat) and the
  controls (Composer controls).

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
- **Long replies.** A reply over 32 KB or 512 lines shows its start in a 420 px scroll box with
  Copy full message under it (`ChatOpen::previews`); Copy under the turn still takes all of it.
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
