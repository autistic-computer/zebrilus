---
type: Concept
title: Agent lens
description: How a terminal pane running Claude Code or Codex shows an agent timeline. Covers the header toggle and chat mode, the slash commands and skills it lists, the terminal PiP for the CLI's own dialogs, references to panes, chats and files as links, copy and fork on messages, how the chat draws pastes, live turns, subagents and diagrams, the model, effort and folder controls, side chats and forks, pasted images, the daemon lens runtime that tails the CLI's own transcript, the agent stream that carries it to clients, Claude permission prompts, and what agent-send and agent-respond do with it.
resource: crates/zz-daemon/src/agent/lens.rs
tags: [agent, lens, claude-code, codex, transcript, terminal, pane]
timestamp: 2026-10-31T20:30:00-03:00
---

# Overview

zz has no agent pane kind. Claude Code and Codex run as their own TUIs in ordinary terminal panes,
and a terminal whose foreground program is `claude` or `codex` gets a lens button in its header.
With the lens on, the pane body shows that agent's timeline and a composer in place of the cell
grid; clicking again shows the terminal. The terminal process stays the source of truth: the
timeline is read from the transcript the CLI already writes, and the composer types into the TUI.

The rulings, the full deletion map and the history of the earlier attempt are in the
[agent lens design](/designs/agent-lens.md). Both halves are on main (merged in `69ceca016`).

Until 2026-10-23 zz had a separate Agent pane (`PaneKindSnapshot::Agent`) that spawned an ACP
adapter or a native driver per pane, with project, history, provider, model and effort pickers.
That pane, its pickers, its text projection terminal, its tree status record and the subagent PiP
feeds are gone. The [daemon-owned agent runtime](/designs/agent-daemon-runtime.md) and the
[native drivers](/designs/native-agent-drivers.md) record how it worked.

# Which panes get a lens

- **Client.** `zz_client::window_summary::pane_agent` maps `pane_program` (the terminal's
  `PaneSummary.process`, wire v112) to `claude` or `codex`. The toggle is per client and not
  synced, so one client can show the lens while another shows the terminal. The header button is
  `zz_ui::pane::pane_lens_button` (`PaneLens`, sent as `TerminalPaneAction::Lens`), an eye (Solar's
  `eye`) for either agent, drawn in the
  `pane.header.lens` role, which tints it while the lens is open. Like the split buttons beside it,
  it shows on the focused pane and on the others only while the pointer is over the header.
- **Phone.** In the compact shell (iPhone) the lens opens by default for a terminal running an
  agent, since typing into a TUI with a phone keyboard is slow. Closing it with the toggle keeps that
  pane on its terminal until the agent exits (`CompactState::lens_dismissed` in
  `clients/app/src/app/compact.rs`). With the desktop sidebar in chat mode every terminal running an
  agent in the attached session opens with its lens on (`AppView::open_chat_lenses` in
  `crates/zz/src/workspace/view.rs`); turning it off with the toggle keeps that pane on its
  terminal until the agent exits (`lens_dismissed`). In raw mode, and on web and iPad, it opens
  only from the toggle.
- **Daemon.** `lens_provider` in `crates/zz-daemon/src/daemon/lens.rs` looks for a `claude` or
  `codex` basename in the pane's process list (case-insensitive, `.exe` stripped), and
  `terminal_agent` only answers for `PaneKind::Terminal`.

# Opening the lens runtime

`Shared::open_lens(pane)` runs when a client sends `AgentReplay` for a terminal's own `%pane`, and
when `agent-send --wait`, `--notify`, `--final` or `--on-block allow|deny` targets one. It adds the
pane to `lens_panes` and opens the agent runtime with an `AgentPaneSpec` whose `lens` field carries
the pane, the provider and a `TerminalLens` (the pane's pid, a paste-and-submit path and a key
path). `AgentHost::open` sees the `LensSpec` and runs `agent::lens::run_lens` on the pane's
`zz-agent-{n}` thread instead of starting a process.

From there the path is the one the old Agent pane used: host pump, fanout lane, `AgentUpdates` and
`AgentState` events, and the client reducers. The runtime lives until the pane closes
(`close_agent_panes` drops it from `lens_panes`). Agent messages for any pane that is not a
terminal are refused (`is not a terminal pane`), and a replay for a terminal running neither CLI
answers `is not running Claude Code or Codex`. The lens is Unix only; elsewhere `open_lens` returns
false.

`open_lens` is the only production caller of the agent runtime. The native Claude and Codex
drivers, the ACP runtime and the [turn machine](/designs/agent-turn-machine.md) stay in the source,
but no pane opens them now.

# Where the timeline comes from

`agent/lens.rs` reads files; it never starts a process.

- **Claude Code.** The peer record `~/.claude/sessions/<pid>.json` (read by `claude_peers.rs`) maps
  the pane to its Claude process, session id, cwd and status. The transcript is
  `~/.claude/projects/<sanitized cwd>/<session>.jsonl` (`claude/sessions.rs::session_file`). On open
  the runtime reads the last 8 MiB, walks the `parentUuid` chain from the newest leaf
  (`sessions::chain_tail`) and feeds the entries through `claude::translate::Translator::history_entry`,
  the translator the native driver uses for resume. New lines are tailed every 250 ms. A new session
  id in the record (`/clear`, `/resume`) becomes a session switch with a fresh replay, and a prompt
  whose parent is not the last line seen (`/rewind`) reloads the chain. A `system` /
  `turn_duration` line marks a finished turn.
- **Codex.** The runtime looks for the rollout the pane's `codex` process holds open, through
  `/proc/<pid>/fd` on Linux or `lsof -p` on macOS, and takes the one whose `session_meta` has no
  parent. Codex 0.164 on macOS leaves the file to its shared app-server (`~/.codex/packages`), so the
  runtime falls back to the newest parentless rollout in the last 7 day folders of
  `$CODEX_HOME/sessions` whose cwd matches the pane and that was modified since the process started
  (`newest_rollout`); two Codex panes in one folder can then pick each other's session.
  `codex/rollout.rs` (`RolloutMeta`, `v2_item`) reshapes `item_completed` items into the app
  server's shapes and `codex::translate::Translator::history_item` translates them:
  - A command recorded as `[bash|zsh|sh|dash, -c|-lc, script]` shows only its script, and a
    command of several parts is titled by the whole command, not its first part.
  - A file change's map of paths becomes the list of changes with the paths untouched (adds and
    deletes as one hunk of `+` or `-` lines, updates as their diff). Only lines before a diff's
    first `@@` count as `---`/`+++` file headers.
  - Items recorded without a status (context compaction, image views, extensions) are done; an
    image path loses its `file://`.
  - Extensions: `web.search` becomes a web row (`Open <url>`, `Find "<pattern>"` or
    `Search "<query>"`, its results as output) and `clock.sleep` a "Waited 1m" step; other kinds
    are dropped.
  - Agent tools: only a spawned agent and `SubAgentActivity` are subagent rows. `wait` is a
    "Waited for agents" step, interrupting, resuming or messaging an agent are plain steps, and
    `list_agents` and `close_agent` are not shown.
  - Subagents: a started `SubAgentActivity` names the child's thread. The runtime finds the child
    rollout `rollout-*-<thread>.jsonl` next to the root, or in the last 7 day folders when the
    root sits in `$CODEX_HOME/sessions` (it keeps looking for 30 s), reads it before each root
    line and feeds it through `Translator::child`: the child's tools nest under the agent row and
    its last message becomes the row's result. Its children's children work the same way. With
    no child file, the row waits for the root's `FINAL_ANSWER` agent message and takes the text
    after `Payload:` as its result (or ends with the turn).
  - Times: each line's `timestamp` stamps its rows, and `completed_at_ms - started_at_ms` (or a
    command's `duration`) goes in `_meta.zz.ms`, so a step starts that long before its record.
  - `token_count` lines fill the context ring (`usage_update`).

  `task_started` marks the pane busy; `task_complete` and `turn_aborted` mark it idle. A
  `task_complete` that carries an `error` (an API error, such as a model the login does not allow)
  fails the lens turn with Codex's message, so `agent-send --wait` exits 1. While a lens turn
  waits on a pane whose rollout is not found yet, the runtime looks on every tick instead of every
  5 s, and a rollout found after its turn already ended settles that turn from its last
  `task_complete`.

Claude's session record carries `status` (`busy`, `idle`, `waiting`). The runtime turns it into
`Activity { busy }` events, so the pane's phase follows the TUI whether the turn came from the
lens, from typing in the terminal, or from a peer message.

# Input and approvals

- **Prompts.** The composer sends `AgentPrompt` as before. The runtime delivers it with
  `TerminalLens::submit`, the bracketed paste plus Enter that waits for the text to echo before
  pressing Enter (`paste_and_submit`). Pasted images are saved in the daemon's paste folder and
  their paths pasted in first, without Enter (`TerminalLens::attach`); both CLIs turn a pasted
  image path into an attachment, and `[Image` counts as the paste having landed. Slash commands
  are the CLI's own and are typed through (see Slash commands below). The composer's
  placeholder names the agent ("Message Claude Code…"), and before the agent has written a
  transcript the lens shows `presentation::lens_welcome`: the agent's mark, "Claude Code is waiting
  for its first prompt" and "What you send here is typed into the terminal."
- **Steer.** `AgentSteer` types text into a running turn without opening a lens turn, so the
  running one keeps its tracking. No client sends it now; the daemon still takes it. The pane host
  sends the runtime `/steer <text>` and the lens pastes `<text>` like a prompt. Claude Code reads text typed mid-turn
  as a message for the running turn, and Codex's Enter steers. With no turn running, a permission
  waiting, or images, the host takes it as `AgentPrompt`. Claude Code records the message once the
  turn takes it in, at its next tool boundary, as an `attachment` line of type `queued_command`
  (`commandMode: "prompt"`, `origin.kind: "human"`); the translator turns that into the user's
  message, and other origins (peers, task notifications) stay hidden. Codex 0.164 records it as an
  `item_completed` `UserMessage` inside the same turn. Until then the timeline shows nothing for it.
- **Stop.** `AgentCancel` sends Escape to the TUI.
- **Claude permission prompts.** While the dialog is up, the record reads `"status":"waiting"` with
  `"waitingFor":"permission prompt"`, and the pending `tool_use` is the last assistant block with no
  `tool_result`. The runtime raises a permission request for that tool with Allow (sends `1`) and
  Deny (sends Escape). Any other `waitingFor` (a plan approval, a question) raises a request with no
  options, titled "Waiting for a ... in the terminal". Clients draw it with
  `zz_ui::agent::presentation::terminal_wait_card`, whose one action, Show terminal, opens the
  terminal PiP (below) so the user answers there.
- **Codex approvals.** The rollout records nothing while Codex's dialog waits, so the lens shows no
  card; the user answers in the terminal.
- **Codex questions.** `request_user_input_async` records an `AgentMessage` with `questions` and
  `delivery: "async"`. It becomes a question row marked as background from the start, so it does not
  hold its turn open: the call does not block the turn and the question can outlive it. While it is
  open the runtime reports `waiting`: one question with 1 to 9 options raises a question card
  answered with the option's digit (the TUI's overlay takes digits), anything else a card that
  points at the terminal. The answer comes back as a `UserMessage` wrapped in
  `<send_user_message_question_reply>`, often as the first line of a new turn, and closes the row
  with an answered record instead of a user bubble. A prompt typed in the terminal, `turn_aborted`,
  or Stop while a turn runs drop an unanswered question (recorded with no answer); the composer
  offers no Stop once the turn is over. While it is open the lens refuses prompts and steers, so
  they cannot land in the overlay as option picks, and holds a model or effort pick until the
  question is answered. A question recorded before the running Codex process started is ignored,
  since its overlay died with the old process. A plain `send_user_message_async` message stays
  text.

# Slash commands

The lens runtime publishes the CLI's commands as an ACP `available_commands_update` after every
session reset and whenever Claude's skill list changes, so `/` in the composer opens the same
suggestion list the native drivers fill (`agent/lens_commands.rs`). It is read from files, like the
rest of the lens:

- **Built-ins**, a fixed list per CLI. The ones whose own UI is a dialog in the TUI carry
  `_meta.zz.terminal` and show "opens terminal": picking one types it into the CLI and opens the
  terminal PiP (`agent_completion::terminal_command`). Claude Code: `plan`, `status`, `config`,
  `context`, `usage`, `permissions`, `mcp`, `hooks`, `memory`, `resume`, `rewind`, `plugin`,
  `skills`, `btw`, `doctor`, `theme`, `login`, `logout`, `help`. Codex (0.166): `side`, `btw`,
  `permissions`, `status`, `usage`, `mcp`, `skills`, `resume`, `fork`, `mention`, `raw`, `logout`.
  The rest (`clear`, `compact`, `goal`, `subtask`, `cd`, `add-dir`, `export`, `copy`,
  `reload-skills`, `exit`; Codex `new`, `clear`, `compact`, `recap`, `goal`, `plan`, `review`,
  `init`, `diff`, `cd`, `pwd`, `rename`, `export`, `copy`, `quit`) run from the lens, and what they
  do shows in the timeline.
- **`/model` and `/effort`**, for both CLIs, are the footer menu's setters (below). Typed with a
  value (`/model sonnet`, `/effort high`) the client sends `AgentSetConfigOption` instead of a prompt
  (`agent_settings::setter`); typed bare, the command stays in the composer as `/model ` and the
  suggestion list shows the values from the session's config options, the current one marked
  (`agent_completion::setting_completions`). Picking a value sets it. A prompt that still arrives as
  `/model <value>` or `/effort <value>` (the HTML client, `zz agent-send`) goes to the same setter in
  the runtime (`lens_settings::typed`) and finishes at once, failed if the setter refused it.
- **Claude Code skills and commands** from `~/.claude` (or `CLAUDE_CONFIG_DIR`) and every
  `.claude` from the pane's folder up to its git root: `skills/*/SKILL.md` (frontmatter `name`,
  `description`, `argument-hint`), `commands/**/*.md` (a subfolder becomes `dir:name`), and the
  skills and commands of the plugins `enabledPlugins` turns on, named `plugin:name`. The
  transcript's `skill_listing` attachment adds the skills Claude Code itself loaded (bundled ones
  included) once it has written one.
- **Codex skills** from `$CODEX_HOME/skills` and `.codex/skills` up to the git root, listed as
  `$name` and inserted that way, and prompts from `$CODEX_HOME/prompts` as `prompts:name`.

Skills carry `_meta.zz.skill` and show "skill". Old clients skip the `_meta` field and show the
plain list.

# The terminal PiP

On the desktop a command that only the CLI's own screen can show does not flip the pane. The lens
draws the pane's live `TerminalView` in a wide PiP card over the chat, with no scrim
(`zz_ui::agent::presentation::terminal_pip`, `crates/zz/src/agent/view/harness.rs`). Its header
shows the CLI's mark, "/config · Claude Code" and Done, and keys go to the CLI. The `TerminalView`
keeps the pane's own size, so the card shows its bottom rows, where the CLI draws its dialog: the
lens measures its own height and pins the terminal's bottom edge to the card's. The PiP closes when:

- Claude Code logs the dialog closing. Every `local-command-stdout` line ("Set model to …",
  "Disabled auto-compact") makes the runtime send `ConfigOptionsChanged` with `option_id` `dialog`
  and value `closed`; clients count those (`AgentPaneState::dialogs_closed`).
- Claude Code's input box is back on screen. Closing /status, /usage, /context, /mcp or an
  unchanged /config logs nothing, so the runtime checks the screen for the `❯` line between two
  rules (`lens_settings::claude_composer_shown`) and sends the same `dialog` signal.
- Done is pressed, which also sends Escape to the CLI. For Codex, Esc closes it too.

Codex logs nothing for its dialogs, so its PiP closes on Esc or Done. The web, iPad and iPhone
clients still flip to the terminal for these commands, since closing the lens there also closes its
stream.

# Model, effort and folder

The runtime reads what the CLI is set to from its own files (`agent/lens_settings.rs`): Claude Code
assistant lines carry `message.model`, `effort` and `cwd`, and "Set model to …" / "Set effort level
to …" lines update them at once. Codex `turn_context` lines and `event_msg` `thread_settings_applied`
lines carry `model`, the effort (`effort` or `reasoning_effort`) and `cwd`. It sends them as the
session's ACP config options (`model`, `effort` and `cwd`, in `SessionReady`, `SessionSwitched` and
`ConfigOptionsChanged`), so no wire change was needed and old clients ignore them. Clients parse them
with `zz_client::agent_settings::LensSettings`.

Until the transcript names them, the footer shows what the CLI was started with
(`Facts::claude_launch`, `Facts::codex_launch`): the agent process's own arguments (Claude Code
`--model`, `--effort`; Codex `-m`/`--model`, `-c model=…`, `-c model_reasoning_effort=…`), read by
pid (`process_info::record`), over its config (Claude Code `settings.json` under
`$CLAUDE_CONFIG_DIR` or `~/.claude`, then the folder's `.claude/settings.json` and
`settings.local.json`, keys `model` and `effortLevel`; Codex top-level `model` and
`model_reasoning_effort` in `$CODEX_HOME/config.toml`). The transcript's values win once they arrive.
Claude Code writes its `sessions/<pid>.json` record only after the first prompt, so until then the
lens takes the first process in the pane whose first or second argument is named `claude`.

The models on offer come from a catalog per CLI (`lens_settings::Catalog`):

- **Claude Code.** The aliases `opus`, `sonnet`, `haiku` and `fable`, each with the levels `low` to
  `max`.
- **Codex.** `$CODEX_HOME/models_cache.json`, the list Codex itself fetches (read up to 8 MiB,
  reread when its mtime changes): the models with `visibility` `list`, by `priority`, named by
  `display_name`, each with its `supported_reasoning_levels` and `default_reasoning_level`. Without
  the file the current model stands alone with `low`, `medium`, `high` and `xhigh`.

The current model is always one of the choices, even when the catalog hides it. The `model` option's
entries carry `_meta.zz.efforts` and `_meta.zz.defaultEffort`; the `effort` option lists the current
model's levels, named from one table (`xhigh` is "Extra high").

The composer footer shows the folder on the left and, on the right, the model and effort button and
the side chat button (`zz_ui::agent::controls::{agent_directory_chip, agent_settings_menu,
agent_side_chat_button}`). The button opens the same menu for every CLI: the models, then the
current model's levels. Picking one sends `AgentSetConfigOption`, which the runtime types into the
TUI (`lens_settings::command`) and shows at once:

- **Claude Code.** `/model <alias>` or `/effort <level>`. Its "Set model to …" line confirms it.
- **Codex.** Codex has no `/effort`; its `/model` takes the level inline, so a model pick types
  `/model <slug> --reasoning=<level>`, keeping the current level when the new model takes it and the
  model's default otherwise, and a level pick types `/model <current slug> --reasoning=<level>`.
  Codex answers "Model changed to … for this session only" in the terminal, and its rollout names
  the settings in the `turn_context` it writes about a second after the next turn starts, so the
  lens lays the pick over what the rollout says, across a reload, until a rollout line names the
  settings again; a pick Codex refused shows until then. Codex refuses `/model` while a task runs,
  so a pick made mid-turn is held and typed when the turn ends; a later pick replaces a held one.
  A new Codex process in the pane drops the picks. A slug or level the catalog does not list fails
  with `SettingFailed` before anything is typed.
- **Folder.** On the local host the folder chip opens a folder picker and the runtime types
  `/cd <path>`. On a remote host it only shows the folder. Codex holds a mid-turn `/cd` the same way.

# Side chats and forks

- **Side chat.** The footer's chat-plus button opens a floating pane in the same window running a
  fork of the session: `claude --resume <session> --fork-session`, or `codex fork <session>`,
  through the user's interactive shell like a new chat (see [Starting a chat](#starting-a-chat)).
  The main chat never sees it. In chat mode the side chat gets its lens like any agent pane; in raw mode
  the header toggle opens it.
- **Copy and Fork from here.** Hovering a message shows Copy and a branch button under it; the last
  answer keeps them shown, and on touch they always show. Copy puts the message's text (for an
  answer, every text block of the turn) on the clipboard. Fork opens a new window after
  the chat's running `zz tools --fork-chat <claude|codex> <session> <message>`, which runs on the
  CLI's host through the pane's private `zz` (`crates/zz-cli/src/fork_chat.rs`):
  - Claude Code: it copies the transcript through the end of that message's turn under a new
    session id in the same project folder, then runs `claude --resume <new id>` through the user's
    interactive shell.
  - Codex: it finds the message's `turn_id` in the rollout, asks the app server for `thread/fork`
    with `lastTurnId` (through `codex app-server proxy`, or its own `codex app-server`), then runs
    `codex resume <new thread>` the same way.
  - The chat item id carries the source id (`u:<uuid>` for Claude, `u:<item id>` for Codex), and
    `zz_client::agent_settings::source_message` reads it back.

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
markdown carries them as `zzref:` links; the user's bubble draws them as clickable ranges.

# How the chat reads

`zz_ui::agent::chat` draws the folded chat (`chat_thread`):

- **Your messages.** Claude Code wraps a paste as `<pasted_content id="…">…</pasted_content
  id="…">` in its transcript; the runtime drops the wrapper (`claude::translate::prompt_text`). A
  long message stays inside a bubble capped at 260 px that scrolls.
- **Pasted images.** The translators put a prompt's images on its `user_message_chunk` as
  `_meta.zz.images` (`{ format, data }`, base64): Claude's base64 image blocks, minus the
  `[Image #N]` placeholders, and Codex's data URLs and local image files. The bridge
  (`chat_bridge::attach_images`) keeps a message's text and base64 together under 6 MiB so one
  stream item stays under the 9 MiB batch cap, and the chat item carries them as `ItemBody::images`;
  a user item is held to that cap rather than the 1 MiB one other chat items get
  (`runtime::validate_payload`). The bubble draws them first as 140 px tiles that open full size on
  click; `ChatPane::sync` decodes each item's images once and keeps them in `ChatOpen::images`.
- **Live turns.** The last line of a running turn says Thinking, or Working while a tool runs, with
  a shimmer across the word, a turning sparkle and the time since the prompt. It hides while the
  chat waits on you. The transcript only gets a reply once it is whole, so a reply that lands while
  you watch types itself in over 220 to 900 ms (`ChatPane::sync` notes when it arrived; replies
  already there on the first look show at once).
- **Thinking.** A finished run whose reasoning is only headings (Codex records the headings of its
  summaries; the reasoning itself is encrypted) leaves no row, while the running one still shows
  its heading. Opening a run lists its reasoning, heading removed and muted, in order with its
  steps; a run with nothing to list has no disclosure. Claude's thinking reads the same way.
- **Subagents.** One row each: the CLI's mark, the name, a status glyph (spinning, Needs you, done,
  failed, stopped) and the time it took. Clicking one opens its chat in a PiP over the thread
  (`chat_agent_pip`): the prompt it was given, its own rows, its answer and a footer; its close
  button or a second click closes it. An approval it waits on sits under its row.
- **Answered questions.** The question, with the answer under it.
- **Diagrams.** A mermaid or SVG block sits in a card on the `code` role, like a code block, whose
  header holds its label, Copy source and Open full diagram. The diagram's canvas is transparent, so
  it shows the card through it; edge and relation labels keep a chip in the node fill. Answers use
  the same markdown style as code blocks and tables elsewhere (`agent_text_view`).

# Starting a chat

zz starts every agent CLI (new chat, side chat, fork) through the user's interactive shell, so
aliases and shell functions such as `alias claude='claude --dangerously-skip-permissions'` apply.
The pane runs `/bin/sh -c` with `exec "${SHELL:-/bin/sh}" -ic '<command>'`
(`zz_client::agent_launch::interactive_shell`); `/bin/sh` parses that line whatever the pane's
default shell is, and the user's shell only sees a plain command line. Bash and zsh exec the
expanded command, so the pane's process is still `claude` or `codex` and the lens finds it.

A new chat starts with the model and effort last picked for that provider: Claude Code
`--model <alias> --effort <level>`, after the alias's own flags so they win, and Codex
`-c model="<model>" -c model_reasoning_effort="<level>"` (Codex refuses a second `-m`). The desktop
remembers them in `agent-launch.json` in its data folder (`crates/zz/src/agent/launch.rs`) when the
footer menu sets one, or when the lens settings change model or effort within a chat (the CLI's own
`/model` and `/effort`). The web, iPad, iPhone and HTML clients start chats without them.

# How the stream reaches a client

The delivery model is unchanged from the agent pane; the full contract is in the
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
  older than the ring takes the reset path the runtime design describes. When the lens runtime
  opens, it rebuilds the history from the CLI's own file.
- **Visibility.** The heavy stream goes to clients for whom the pane is a lens pane in their
  focused window, honoring zoom (`visible_agent_panes`), and to clients watching the pane.
  `AgentState` goes to every client attached to the session and to the pane's watchers, and a
  reattaching client gets it for every lens pane there (`send_agent_resync`, `session_agent_panes`).
- **Watchers.** A client that sends `AgentReplay` for a pane watches it from then on, attached or
  not (`watched_agents` on the client entry): it gets the pane's `AgentUpdates`, `AgentState` and
  `AgentLagged` until it disconnects or the pane closes, and the pane's `AgentState` right away when
  the watch starts. A client can follow an agent this way without attaching to its session, which
  would resize its panes, and an attached client keeps a lens live while its focused window shows
  something else. A client that both sees and watches a pane is
  sent each frame once, and a watched pane leaving the visible set keeps its queued frames. There is
  no unwatch message.

# From outside: the CLI

The daemon's `zz tools agent` text (`WORKSPACE_TOOLS_AGENT` in `crates/zz-daemon/src/daemon.rs`) is
the reference; the [command set](/tmux/commands.md) has the rows.

- `agent-send [-t %N] [TEXT]` sends to the agent in a terminal: Claude Code through its peer socket
  ([agent peer bus](/architecture/agent-peer-bus.md)), Codex through `codex queue`, otherwise typed
  into its input.
- `--wait`, `--notify`, `--final`, `--json` and `--on-block` go through the pane's lens: the text is
  typed into the CLI and the transcript says when the turn ends. A terminal running neither CLI
  falls back to waiting on `@agent_state`, and `--notify`, `--final` and `--on-block allow|deny`
  exit 2 there. `--on-block allow|deny` answers a Claude permission prompt with the keys above.
  `--wait ''` (no text) skips the lens: it waits on `@agent_state` for the running turn and prints
  nothing, so a caller can wait again after a timeout without typing anything.
- `agent-respond` answers a pending permission only in a pane whose lens is open (in a client or
  through a running `agent-send --wait`); other panes exit 1 with `has no agent lens open`.
- `inspect` lists `agent-respond` in `verbs` for a pane with an open lens, and its `permission`
  object comes from the lens runtime. `events` emits `agent-tool-call` for terminals with an open
  lens.
- `send-last-output -t %N` types a terminal's last command and output into the window's most
  recently focused terminal running Claude Code or Codex (`recent_agent_terminal`), without Enter.
- `#{agent_state}` reads a terminal's `@agent_state` (fed by OSC 7501, OSC 9;4 progress and the
  Claude peer sampler); it is empty for other pane kinds. `#{agent_pending_permission}` is always
  empty now. The lens writes no `@agent_state` and no tree status record.

# What the clients draw

The lens draws the chat (`crates/zz-ui/src/agent/chat.rs`): `chat_thread` draws the rows
`zz_client::chat_rows` makes from the `ChatView`, inside `chat_column`, the column both apps share,
and `chat_agent_pip` draws a subagent's chat in its PiP. The permission and question cards,
`AgentComposer` and the task tray live in `crates/zz-ui/src/agent/`. Both apps still build
`AgentTimeline` (`crates/zz-ui/src/agent.rs`) and draw it only while the chat view is empty, as
against a daemon older than the chat machine or before the first chat turn arrives.
`AgentComposer` holds the `Composer` pill: `+`, which starts an `@` mention with the palette's
target picker (`zz_ui::target::TargetPicker`, inserted with its host), the field, the context ring
and the round send button; the cards over it and the branch under it are the lens's own. The
timeline keeps its `TimelineStick` / `StickSpring` follow-the-tail pin (`agent.rs`) for that case;
the display-only Markdown repair for streaming text (`zz-client/src/mend.rs`), the streamed-text
fade (`penna/src/text/veil.rs`) and the shared leased animation clock (`penna/src/pulse.rs`)
carried over unchanged.

- **Desktop.** `crates/zz/src/agent` (controller and view) behind the `agent-pane` cargo feature,
  which stays on by default; without it a stub stands in.
- **Web and iPad.** `clients/app/src/app/agent_lens.rs`.
- **HTML client.** `clients/dom/src/agent.rs` builds the lens view from the
  shared `zz-client` reducers as JSON; `clients/dom/web/js/agent.js` draws them with the design kit.
- **Storybook.** One Agent story (`clients/storybook/src/stories/agent`) draws every chat state
  from fixed chat events with the same calls, `chat_column` included, and asks the way the
  desktop draws them.

The lens has no project picker and no provider picker; the old Agent pane's pickers were deleted,
not hidden. Model, effort and folder are the composer footer's controls (see
[Model, effort and folder](#model-effort-and-folder)).

# Related

- [Agent lens design](/designs/agent-lens.md) . rulings, deletion map, follow-ups
- [Daemon-owned agent runtime](/designs/agent-daemon-runtime.md) . host, fanout, journal
- [Native agent drivers](/designs/native-agent-drivers.md) . the translators the lens reads with
- [Agent peer bus](/architecture/agent-peer-bus.md)
- [Wire protocol](/protocol/wire-protocol.md)
- [tmux command set](/tmux/commands.md)
- [zz](/crates/zz.md) and [zz-daemon](/crates/zz-daemon.md)
