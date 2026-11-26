use super::*;

pub fn workspace_tools(args: &[RawText]) -> Result<String, ServerError> {
    let section = match args {
        [] => return Ok(WORKSPACE_TOOLS.to_owned()),
        [section] if section == "agent" => WORKSPACE_TOOLS_AGENT,
        [section] if section == "terminal" => WORKSPACE_TOOLS_TERMINAL,
        [section] if section == "browser" => WORKSPACE_TOOLS_BROWSER,
        [section] if section == "advanced" => WORKSPACE_TOOLS_ADVANCED,
        [flag] if flag == "--skill" => {
            return Ok(format!(
                "{WORKSPACE_TOOLS_FRONTMATTER}{WORKSPACE_TOOLS_HEAD}{WORKSPACE_TOOLS_AGENT}{WORKSPACE_TOOLS_ETIQUETTE}"
            ));
        }
        [flag] if WORKSPACE_TOOLS_LOCAL_FLAGS.contains(&flag.as_str()) => {
            return Err(ServerError::CommandParse(format!(
                "tools {flag} runs in the local zz CLI"
            )));
        }
        _ => {
            return Err(ServerError::CommandParse(
                "usage: tools [agent|terminal|browser|advanced | --skill | --context | --install-hooks | --remove-hooks]".to_owned(),
            ));
        }
    };
    Ok(format!("{WORKSPACE_TOOLS_SECTION_HEAD}{section}"))
}

const WORKSPACE_TOOLS_SECTION_HEAD: &str =
    "Run `zz tools` for targets, remote hosts, exit codes and every section.\n\n";

pub(super) const WORKSPACE_TOOLS_FRONTMATTER: &str = r"---
name: zz-workspace
description: Drive the surrounding zz workspace from inside a zz pane using the `zz` CLI to send text or piped output to an agent running in another pane, read a terminal pane's scrollback, screenshot a browser pane, navigate a browser, and route a failing command's output to an agent. Use whenever you are running inside zz (the `ZZ_PANE` environment variable is set) and the task involves another pane, a terminal's output, a browser page, or another machine.
---

";

pub const WORKSPACE_STUB_SKILL: &str = r#"---
name: zz-workspace
description: Drive the surrounding zz workspace (panes, windows, terminal output, agents, browser panes, other machines) with the `zz` CLI. Use whenever you are running inside zz (the `ZZ_PANE` environment variable is set) and the task involves another pane, a terminal's output, a browser page, or another machine.
metadata:
  zz-stub: "1"
---

Run `zz tools --skill` and follow the guide it prints. The guide comes from the installed zz, so it describes the commands this version has.
"#;

pub(super) const WORKSPACE_TOOLS_LOCAL_FLAGS: [&str; 1] = ["--context"];

macro_rules! workspace_tools_sections {
    ($($name:ident = $text:literal;)+) => {
        $(pub(super) const $name: &str = $text;)+
        const WORKSPACE_TOOLS: &str = concat!($($text),+);
    };
}

workspace_tools_sections! {
    WORKSPACE_TOOLS_HEAD = r"# Workspace tools

Drive terminal and browser panes, and the agents in them, through the `zz` CLI. Run `zz tools`
for the full catalog or `zz tools agent` for agent verbs. Run `zz tools --skill`
for the agent guide with skill frontmatter. Regenerate it with `just tools skill`.
Run `zz tools terminal` for terminal verbs, `zz tools browser` for browser verbs,
and `zz tools advanced` for state subscriptions and terminal-agent integration.
This guide is long. If your tool shortened it, the middle is what went missing:
read that part on its own with one of those section commands.
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
| `ZZ_AGENT` | Set in a chat: the agent it runs (`claude`, `codex`, `agy`, `grok`, `cursor-agent`). |
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

`#{pane_kind}` is `terminal`, `browser`, `editor`, `picker`, `agent`, or `view`.
`#{pane_last_command_status}` is the last completed command's exit code, or empty
when unknown; terminal panes report it from OSC 133 marks.
`#{@name}` reads a user option from pane, window, session, then global scope.

## Placement

A pane goes where its work belongs. The user reads sessions, windows and panes as
a tree of what is going on, so a pane in the wrong place hides your work or
clutters someone else's.

- What serves your own task stays in your own window. A page or a preview goes
  beside you: `zz split-window -d -h -t %3 --kind browser URL`. A helper terminal
  (a server, a watcher, a log) is a small split,
  `zz split-window -d -v -l 6 -t %3 'COMMAND'`, or a background task of your own
  when nobody needs to watch it. Do not open a window for either.
- A chat you open for the user (a hand-off, a fork, work they watch) is a float
  in your window, which `new-pane --kind agent` gives you. Background work of
  your own is not a chat: see Agents.
- Work that is not part of the window you would put it in gets a session of its
  own, named for the task: `zz new-session -d -P -F '#{pane_id}' -s TASK -c DIR`
  prints its first pane. Never add a pane to a window that is open for something
  else, on this machine or another.
- A pane that landed in the wrong place moves with
  `zz join-pane -d -h -s %7 -t %3`, running program and all.

## Remote hosts

`zz list-hosts` lists the machines in `zz/config` with their state and version.
`zz --host NAME VERB` runs a verb there (NAME may be an ssh destination; a name that
`zz/config` does not list and that is this machine's hostname runs here), and
`zz --host all VERB` runs a read-only verb on every host, prefixing text lines with
`NAME: ` and adding `host_name` to JSON lines. Pane IDs are per host and none is
yours there: pass `-t %N` from `list-panes -a` or your own
`new-window -d -P -F '#{pane_id}'` (`new-session -d -P -F '#{pane_id}'` on a host
with no session); a verb that acts on a pane, window or session exits 2 without
`-t`, and a move or swap without `-s`. A pane from `list-panes -a` is a target for
acting on that pane, not a place to start new work: a task you start on a host
gets a session of its own. `zz --host NAME new-session -d -P -F '#{pane_id}' -s TASK -c DIR`
prints a terminal `%N`,
`zz --host NAME split-window -d -t %N -c DIR --kind agent claude 'BRIEF'` opens the
chat beside it, and `zz --host NAME kill-pane -t %N` closes the terminal, which
leaves the chat alone in its session. `zz --host NAME run-shell 'COMMAND'` runs a
command line there with no pane, prints its output and exits with its status.
Hints and errors that come back from a host name their commands with
`--host NAME` already: run them as printed. Paths and pane environments are that
host's; `events` and `agent-send --progress` stream from it. Logins use zz’s own
key after the first, and `--host all` never prompts. A call that changes state
and stalls 15 s exits 1 and may have run there: check before retrying. When a
machine joins, run `zz fleet add NAME DEST`, then `zz fleet sync`; run
`zz fleet install NAME` for a missing zz. `zz update NAME` installs a newer build
there beside its package, with no sudo, and `--restart -y` moves its daemon onto
it, which ends that host's sessions; `zz update --check -a` prints what every
machine runs, and `zz build-info` prints this build's version and wire floors.
A tailnet host gets `zz update-server` and `zz restart-server` from it, sent to
its daemon.

`zz copy-file [HOST:]SOURCE [HOST:]DESTINATION` copies one file to a host, from a host or
between two hosts, and prints the destination and its size. A path on a host is absolute or
starts with `~/`; a destination that is a folder here, or ends with `/` there, keeps the
file's name. It copies files only: pack a folder first, as in
`zz --host NAME run-shell 'tar czf /tmp/x.tgz -C ~/dir .'`, then `zz copy-file NAME:/tmp/x.tgz .`.
Each host needs `read-file`, so update an older one first.

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
`pane_kind`, `agent_state`, `agent_pending_permission`, `agent_parent`, `browser_url`,
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

";
    WORKSPACE_TOOLS_AGENT = r#"## Agents

Claude Code, Codex, Antigravity and Grok run two ways in a pane, and a pane flips between them on the same session. Cursor runs as a chat only (`agent cursor`): its CLI and its chats keep separate conversations, so it does not flip. In a terminal pane the CLI runs as its own TUI, started like any program (`zz split-window -h claude`). In an agent pane the daemon runs the CLI headless and the clients draw the conversation as a chat: `zz select-pane-kind -t %N [-c DIR] [--resume ID] [--model M] [--effort E] [--account ID] agent claude|codex|agy|grok|cursor` turns a picker or a terminal into one.

A chat is a pane in the user's sidebar, with its own transcript, model and approvals, and it stays there until the user closes it. Open one for work the user watches or steers: a task they asked you to hand to another agent, work they want to see or answer, a fork of a conversation, a task for another machine. Do not open chats as workers for your own task (searches, reading, edits you split up and collect): use your own subagents for that, or do the work yourself.

### `zz new-pane [-d] [-P -F FORMAT] [-c DIR] [-t TARGET] --kind agent claude|codex|agy|grok|cursor [--model M] [--effort E] [--account ID] [--resume ID] [BRIEF]`

Open a new chat as a float. `split-window` and `new-window` take the same `--kind agent` words and give the chat a split beside the target or a window of its own. `--model`, `--effort`, `--account` and `--resume` go before the provider or right after it. The words after them are the brief, sent as the chat's first prompt once its session is up, with the same checks and 1 MiB cap as `agent-send` TEXT; no brief opens an empty chat. Without `-c` the chat starts in the folder a terminal made the same way would. `-P -F '#{pane_id}'` prints the new pane, which takes `agent-send` at once: `zz new-pane -d -P -F '#{pane_id}' --kind agent codex 'review the diff in this folder'`. A missing or unknown provider, or a bad flag value, exits 2. Every create prints one line on stderr, `new-pane: opened %N (codex)` (with the verb's own name); with a brief it goes on `; its reply comes back to you as a message; end your turn now` when run from a chat, and `; read its reply with: zz agent-send -t %N --wait ''` when run from anywhere else.

Run from a chat, the new chat is yours: `#{agent_parent}` names your pane, its reply to the brief comes back to you as a message once its turn ends, and it approves its own tool calls unless `agent-auto-approve` is `off`, so only a question it asks comes to you, with the `agent-respond` lines that answer it. Do not wait or poll: while your turn is still running, Claude Code and Codex get these messages inside it, and the other agents get them as their next prompt. The chat you open gets a note with its brief, kept out of its transcript, saying which chat opened it and that its final message is its reply. It cannot open chats of its own (it is told to do the work itself or say what it needs), and closing your pane closes it and every pane it opened. If the user closes it and opens it again, `zz agent-send -t %N` still reaches it by the id you were given; while it stays closed, the send fails with `%N was closed; open a new chat with: zz new-pane -d --kind agent codex 'BRIEF'`. A message reads `[zz] %N (codex) finished:` or `stopped (REASON):`, then the reply and `If this answers the user, answer the user now and send nothing to %N. Only if something is missing: zz agent-send -t %N 'TEXT'` (inside a running turn that line says to carry on with what you were doing instead); `failed: MESSAGE` and `Tell the user, or try again with: zz agent-send -t %N 'TEXT'`; or `was closed before it replied. Tell the user; do not open it again.`; an ask reads `[zz] %N (codex) asks: KIND TITLE` and lists the `zz agent-respond` commands that answer it, one `--option ID REQUEST_ID  # LABEL` line per choice of a question and `--answer 'TEXT' REQUEST_ID` for one that wants typed text; each line ends with the id of the request it answers, so once that request is answered or gone it fails with `has no pending request` instead of answering another one.

A prompt the user wrote that names an agent with `@+claude`, `@+codex`, `@+agy`, `@+grok`, `@+cursor` or `@+codex/MODEL`, or a running chat with `@HOST:%N`, comes with a note, kept out of the transcript, that tells you to hand the task over with the exact command (`zz new-pane -d --kind agent codex 'BRIEF'`, or `zz agent-send -t %N 'TEXT'` for a chat) and end your turn. For a chat on another machine the command is `zz --host HOST agent-send -t %N 'TEXT'`. Run from a chat, its reply comes back to you as a message too: stderr says `agent-send: HOST:%N's reply comes back to you as a message; end your turn now`, and the message reads `[zz] HOST:%N (codex) finished:` with `zz --host HOST agent-send -t %N 'TEXT'` as its follow-up. That chat's permission asks stay with it and are answered there. When the reply cannot be routed (the command runs from a terminal, or this machine cannot log in to HOST without asking), stderr says `agent-send: read %N's reply with: zz --host HOST agent-send -t %N --wait ''`, which waits for the turn and prints the reply.

A chat answers the same reads as a terminal. `#{pane_current_command}` is `claude`, `codex`, `agy`, `grok` or `cursor-agent`. `#{agent_state}` is `starting`, `idle`, `working`, `needs-approval`, `needs-input` or `failed`, and `#{agent_pending_permission}` is the pending request ID or empty, in `list-panes -F`, `display-message -p` and `inspect`. `capture-pane -p -t %N` prints the chat's last turn as text: `> ` and the prompt, one line per tool call (its kind and title), then the reply; `-S -` prints the whole chat, and an empty chat prints nothing. While a turn runs, stderr says `capture-pane: %N's turn is still running`. `wait-pane` on a chat fails with `%N is an agent chat; wait for its turn with: zz agent-send -t %N --wait ''`, and `show-last-output` with `%N is an agent chat; read it with: zz capture-pane -p -t %N`. Typing into a chat fails and names the verb to use: `send-keys`, `send-text`, `paste-buffer` and `run-pane` fail with `%N is an agent chat; send with: zz agent-send -t %N TEXT`, and `pipe-pane` with `%N is an agent chat; read it with: zz capture-pane -p -t %N`.

### `zz flip-agent [-t %N] [--now | --when-idle | --cancel]`

Flip a terminal running Claude Code, Codex, Antigravity or Grok into a chat on the same session, or a chat into a terminal running the CLI with that session resumed, through the user's interactive shell. The pane keeps its id and place. When something runs (a turn, background shells or subagents, queued prompts) the flip fails with an error that starts with `busy:` and says what runs; `--now` flips anyway and stops it, `--when-idle` flips once nothing runs, and `--cancel` drops a waiting flip. Codex on its shared app server never refuses: the turn lives in the server and the other side picks it up.

### `zz agent-account [-t %N] [--now | --when-idle] ID|default`

Switch the machine's login of the chat's CLI (Claude Code or Codex) to another account and resume every chat of that CLI on it. The CLI keeps its one login in its own place and the other accounts' logins are held aside, so the CLI switches too when opened outside zz. IDs are the slots `zz agent-accounts` lists (`a1`, `a2`, ...); `default`, or the account already in use, changes nothing. While any chat of that CLI runs something, a Codex turn included, it fails with an error that starts with `busy:`; `--now` switches anyway and stops it, and `--when-idle` switches once none runs, unless another `agent-account` for that CLI comes first.

`zz select-pane-kind --resume ID --fork [--at MESSAGE] agent claude|codex` opens a chat on a copy of a session, through that message's turn when `--at` names one.

### `zz agent-send [-t %N] [--submit | --notify | --wait [--progress] [--timeout SECS] [--on-block wait|fail|allow|deny] [--json | --final]] [--context PATH[:START[-END]]] [TEXT]`

Send TEXT as a prompt to an agent pane (a chat), or to another agent in a terminal pane, typed into its input with Enter only when `--submit` is given. A terminal running Claude Code, Codex, Antigravity or Grok refuses it: flip the pane to chat first. Read stdin when TEXT is omitted: `git diff | zz agent-send -t %3`. `--context` adds a file/line header and fences the payload; text is capped at 1 MiB.

A chat gets the prompt under a first line that names who sent it: `[zz] %N (claude) sent this, not the user:` from a chat, `[zz] a command in %N sent this:` from a terminal pane and `[zz] a command sent this:` from anywhere else. A sender on another machine is written `HOST:%N`, or `a command on HOST` when it has no pane. The chat's clients draw it as a message from that sender, not as the user's prompt. A brief given to `new-pane` gets the same line; a send or a brief the app itself runs, from a key binding or its command prompt, is the user's own and gets none.

`--submit` sends now and prints the chosen pane. `--wait`, `--notify`, `--final`, `--json` and `--on-block` follow the chat's turn. `--notify` runs from inside another terminal pane and, when the target's turn ends, posts its reply back into the calling pane; from a chat it is a plain send, since the reply comes back anyway. `--wait` waits for the turn and prints its reply on stdout (pane ID on stderr). A terminal pane falls back to waiting on `@agent_state`, and `--notify`, `--final` and `--on-block allow|deny` exit 2 there. Failure, cancellation, hand-back, or timeout exits non-zero. The timeout defaults to 600 seconds; `0` waits forever. A timeout leaves the turn running. `--json` prints one object with turn facts, `final_text`, and `transcript`. `--final` prints only the text after the last tool call or tool update. Both require `--wait`; combining them is a usage error. `--on-block wait` waits for permission. `--on-block fail` prints the pending permission JSON and exits 3 while the turn continues. `--on-block allow` approves the pending permission and `--on-block deny` rejects it.

Turn facts: `final_text` contains message text after the last tool call or tool update. `tool_calls` counts the turn's tool calls; permission counters count the chat's requests. Each buffer keeps its tail when capped: 1 MiB for the transcript and 256 KiB for final text. The JSON object has `pane`, `stop_reason`, `duration_ms`, `tool_calls`, `permissions` (`requested`, `allowed`, `denied`), `truncated`, `final_text`, and `transcript`. A blocked reply also has a nested `permission` object. Exit 0 means `end_turn`; other stop reasons exit 1 with the reply still printed, except cancellation, which keeps its existing error output. A blocked wait exits 3 and a timeout exits 124. Stderr carries the pane ID and on-block audit lines. `--progress` requires `--wait` and an explicit `-t %N`. It prints tool calls, state changes, and a heartbeat after 60 seconds without a line to stderr. Titles in progress lines stop at 120 characters.

A send without `--wait` prints the pane ID. From a chat, the reply comes back to you as a message when that turn ends, the chat's permission asks come to you as they open, and stderr says `agent-send: %N's reply comes back to you as a message; end your turn now`; from anywhere else stderr says `agent-send: read %N's reply with: zz agent-send -t %N --wait ''`. A chat that already has 4 prompts waiting refuses a send with exit 1 and `%N has 4 prompts waiting; wait for its turn with: zz agent-send -t %N --wait ''`. With `--wait` from a chat, `--on-block` defaults to `fail`, and a blocked wait ends its output with the `agent-respond` and `agent-send --wait ''` lines to run next. A chat cannot send to the chat that opened it: its reply reaches that chat when its turn ends. A chat sends at most 8 messages to one chat (a brief counts as the first) between two prompts the user gives it; the next fails with `%N has had 8 messages from you since the user last wrote; answer the user now`. `--wait ''` on a chat waits for the turn that is running and prints its reply; with no turn running it exits 1 with `%N has no running turn; read its last reply with: zz capture-pane -p -t %N`.

### `zz agent-respond [-t %N] (--allow | --deny | --option ID | --answer TEXT) [REQUEST_ID]`

Answer the named or oldest pending permission in an agent pane. Read the pending request with `zz inspect -t %N --json | jq .permission`.
Print the chosen option ID. `--allow` approves once and `--deny` rejects. `--answer TEXT` types the answer to a request that asks one question and prints `answered request ID`. Other panes exit 1 with `is not an agent chat`, and a chat with nothing pending exits 1 with `%N has no pending request (already answered, or none was asked)`.

### Permissions

Terminal agents keep their own permission settings: Codex `approval_policy`, Claude Code permission rules. `zz tools --install-hooks` adds a rule that lets both run `zz` without asking.

### `zz inspect [-a | -t %N] [--json]`

Describe a pane's kind, process, geometry, exit status, progress, agent facts, and browser URL.
Read one `key: value` line per field, or use `--json` for one object with string facts and string arrays for `verbs` and `events`. Text output lists use spaces and skip empty facts; JSON keeps every key, with empty strings for unavailable facts. `agent_pending_permission` is the pending request ID, or empty. `agent_provider` (`claude`, `codex`, `agy`, `grok` or `cursor-agent`), `agent_model` (the model the chat was started with, empty for the default) and `agent_session_id` are set for agent panes, and `agent_parent` names the chat that opened this one. `verbs` lists what applies to this pane's kind; agent panes list `agent-send`, `capture-pane`, `flip-agent`, `agent-account` and `agent-respond`, a terminal running Claude Code, Codex, Antigravity or Grok lists `flip-agent`, and another terminal with a nonempty `agent_state` lists `agent-send`. `events` names what `zz events` can report for the pane. Use `-a` for every pane in every session, ordered by session ID, window index, and pane index, with a blank line between text blocks or one JSON object per line; `-a` and `-t` cannot be combined. With neither, inspect the current pane.

```sh
zz inspect -a --json | jq -c 'select(.agent_state!="") | {pane_id,agent_state}'
```

The keys, in text output order, are `session_id`, `session_name`, `window_id`, `window_index`, `window_name`, `window_width`, `window_height`, `window_size`, `pane_id`, `pane_index`, `pane_active`, `pane_kind`, `pane_pid`, `pane_current_command`, `pane_current_path`, `pane_title`, `pane_width`, `pane_height`, `pane_dead`, `pane_dead_status`, `pane_dead_signal`, `pane_last_command_status`, `pane_pb_state`, `pane_pb_progress`, `pane_status`, `pane_status_kind`, `pane_status_progress`, `pane_status_app`, `pane_status_title`, `pane_status_message`, `pane_status_raw_title`, `pane_status_raw_message`, `pane_status_reported`, `agent_state`, `agent_pending_permission`, `agent_provider`, `agent_model`, `agent_session_id`, `agent_parent`, `permission`, `browser_url`, `verbs`, `events`.

`permission` is a nested `{"request_id":7,"tool_call":{...},"options":[...]}` object or `null` in JSON output; text output prints compact JSON on the `permission:` line, or an empty value.

### `zz events [-t %N]`

Stream hook events.
Print JSON lines, flushed per line: `{"seq":1,"event":"agent-state-changed","time":1750000000000,"hook_pane":"%3","agent_state":"working",...}`. Each line includes the hook's string variables. `time` is Unix milliseconds. Wait for the first line, `{"seq":0,"event":"ready","time":...}`, before starting work. On subscriber overflow, `gap` consumes the next sequence number; the client reconnects and continues counting without another `ready` line. Use `-t %N` for a pane, `-t @N` for a window, `-t '$N'` for a session ID, or `-t name` for a session name. Filters match exact hook fields, and a session filter also passes the events of that session's windows and panes; `ready` and `gap` always print. Omit `-t` to stream without filtering. `agent-state-changed` fires when a terminal's `@agent_state` is written and when a chat's state or pending permission changes, and carries `agent_state` and `agent_pending_permission` (the permission ID or an empty string). Agent panes also emit `agent-tool-call` for new calls and completed or failed status changes. Each event carries `tool_call_id`, `tool_title`, `tool_kind`, and `tool_status`; titles collapse whitespace and stop at 200 characters: `{"seq":2,"event":"agent-tool-call","time":1750000000000,"hook_pane":"%3","tool_call_id":"call-1","tool_title":"cargo test","tool_kind":"execute","tool_status":"in_progress"}`. These agent events are stream-only; `set-hook` cannot bind them. Other `@option-changed` firings are not streamed; use a hook for those. Invalid arguments exit 2. Connection and daemon errors, or any disconnect other than overflow, exit 1, including server shutdown.

"#;
    WORKSPACE_TOOLS_TERMINAL = r#"## Terminal panes

Run `zz tools terminal` for terminal verbs.

### `zz split-window [-h | -v] [-t %N] [-d] [-P] [-F FORMAT] [-e KEY=VAL] [-c DIR] [COMMAND...]`

Split a terminal pane; `-t %N` chooses the pane to split. Execute COMMAND as argv without a shell.
Print nothing unless `-P` requests `session:window.pane`; add `-F '#{pane_id}'` for the pane ID. Use `-d` to keep focus, `-e` for environment values, and `-c` for the working directory.

Open a picker with `zz split-window --kind picker -v -P -F '#{pane_id}'`.

### `zz run-pane [-t %N] [--timeout SECS] [--] COMMAND...`

Run a command in a terminal pane's POSIX shell without shell integration.
One COMMAND word is a shell command line, run as written: use it for pipes, `&&`
and variables the pane's shell should expand. Several words are a program and its
arguments, each quoted so the pane's shell gets them as you wrote them. With no
pane to run in, `zz run-shell 'COMMAND'` runs a command line in a shell of the
daemon's, prints its output and exits with its status; with `--host NAME` that is
the way to run a command on another machine without making a pane there. It prints
stdout only, so end the command with `2>&1` to see its errors; it expands formats, so
write `##{` for a literal `#{`; and on Windows the shell is `cmd`. `run-pane` needs a
POSIX shell in the pane: in PowerShell, cmd, fish or nushell it refuses, and the way
there is `send-text`, `send-keys Enter`, then `wait-pane --until TEXT`.
Type it, verify the echo, then press Enter. Print the output between
unique markers and return the child's exit code. Capture includes scrollback,
capped to the last 10,000 logical lines; when the start scrolled past the pane's
`history-limit`, stderr says `run-pane: output truncated to its last N lines`, so
send big output to a file. One run-pane runs per pane; a second waits its turn.
Exit 3 means nothing ran, so a retry is safe: the pane stayed busy past the timeout,
its foreground program is not a POSIX shell (`run-pane: %N is running vim, not a shell`),
the shell was waiting for more input such as an unclosed quote (run-pane sent C-c),
the line is longer than that shell takes (put a long command in a script file),
or the echo never appeared (`nothing was submitted`).
The timeout covers waiting, echo and execution; it defaults to 120 seconds, accepts
fractions, and 0 waits forever. When the command was typed and still runs at the
timeout, print partial output to stdout, write `run-pane: timed out after <N>s on %N; the command keeps running`
to stderr, and exit 125. A child exit of 125 differs from a timeout only by that
stderr line. Use a command or Control client.

```sh
zz run-pane -t %3 -- "sh -c 'echo hi; exit 7'"
zz wait-pane -t %3 --until 'ready' --timeout 2.5
```

### `zz wait-pane [-t %N] [--idle MS | --until TEXT | --regex RE | --exit] [--timeout SECS] [--tail N]`

Choose one condition; the default is `--idle 500`, measured from this call.
Text and regex matches search logical lines on screen at the call or written since,
including scrollback, capped to the last 10,000 logical lines. Join wrapped lines
and print the matching line. Prompt and typed-command rows never match in panes with
OSC 133 marks; elsewhere anchor the pattern (`--regex '^DONE$'`) so the echo cannot. `--tail N` narrows the search to the last N logical
lines after skipping trailing blanks. Idle success prints nothing.
With `--exit`, wait for the PTY child and mirror its exit status, with no output.
A retained dead pane returns its status immediately; killing or respawning the
pane releases the wait with status 0. `--tail` with `--exit` is a usage error.
The timeout defaults to 60 seconds, accepts fractions, and 0 waits forever.
Timeout exits 124 and names the condition. Invalid regex syntax exits 2.
Use a command or Control client.

### `zz send-text -t %N [--no-enter] [--timeout SECS] [TEXT]`

Paste into a terminal TUI, wait for the text to appear, then press Enter. Read
stdin when TEXT is omitted. `--no-enter` drafts. The timeout defaults to 2 seconds,
accepts fractions such as 0.5, and 0 waits forever. Only the target pane gets the text,
even with `synchronize-panes` on.
Print nothing on success. If the text never appears, exit non-zero without submitting.
A paste that Claude Code or Codex folds into a placeholder (`[Pasted text #1 ...]`,
`[Pasted Content N chars]`) counts as appeared.

### `zz send-keys -t %N 'text' Enter`

Send raw keys to a terminal; `-l` sends literal text. Use `send-text` for composers
that swallow an Enter sent before the paste appears.

Print nothing on success.

### `zz capture-pane -p -t %N [-S -] [-E -] [-J]`

Print a terminal pane's text to stdout. `-S -`/`-E -` include the whole scrollback;
`-J` rejoins soft-wrapped lines.

### `zz show-last-output -t %N`

Print that last command and output under a `%N $ command` header, with the same
OSC 133 requirement and caps, soft-wrapped lines joined. When known, an `exit: <n>` line follows the header, or `exit: running`
while it runs; after a run-pane the header shows the command it ran.

### `zz send-last-output -t %N`

Type the last completed command and output into the input of the window's most
recently focused terminal running Claude Code or Codex, without pressing Enter.
The default binding is `<prefix> e`. Requires OSC 133 prompt marks;
the bundled Bash/zsh integration emits them, with best-effort PowerShell support.
Output is capped at 200 lines or 256 KiB with a truncation note.

Print `sent the last command from %N to %M` with the source and recipient pane IDs.

### `zz pipe-pane [-o] -t %N [COMMAND]`

Pipe pane output to COMMAND. `-o` starts a pipe only when none exists.
Print nothing; omit COMMAND to close the pipe.

```sh
zz pipe-pane -o -t %5 'cat >> /tmp/agent-output.log'
```

### `zz run-shell -b -d SECS COMMAND`

Schedule a background command after SECS seconds.
Return without printing or waiting; show command output in the most recent pane's command-output view.

```sh
zz run-shell -b -d 300 'zz send-text -t %5 continue'
```

"#;
    WORKSPACE_TOOLS_BROWSER = r"## Browser panes

Run `zz tools browser` for browser verbs.

### `zz split-window --kind browser [-h | -v] [-t %N] [--profile NAME] [-P] [-F FORMAT] [URL]`

Split a pane to open a browser; `-t %N` chooses the pane to split.
Use `zz new-window --kind browser [--profile NAME] [URL]` to open a browser window.
Both default to `about:blank`. Print nothing unless `-P` requests the tmux target;
add `-F '#{pane_id}'` to print the new pane ID.

A browser pane renders only while a desktop app window shows its session. With none,
`set-browser-url` and `html` just record the page and `capture-browser` fails; over
`--host`, open that host's session in the app first.

### `zz set-browser-url -t %N URL`

Navigate a browser pane. Print nothing on success. `#{browser_url}` reports the active tab's URL.
It returns before the page loads.

### `zz load-browser-url -t %N [--timeout SECS] URL`

Navigate a browser pane and return once the page has loaded, so a `capture-browser` after it
shows the new page. The window showing the pane reports the load: `#{@browser_loading}` is `1`
while a page loads and `#{@browser_loaded}` counts the loads that finished. A navigation inside
the page, which loads nothing, returns after 1.5 seconds. The timeout is 30 seconds by default
and `0` waits forever; a page still loading then exits 124 and keeps loading. A load counts
once the pane has stayed loaded for 0.2 seconds, so one that ends as the navigation starts is
not taken for the new page. With no window on
the pane's session, or an app that does not report loads, it sets the URL and fails.

### `zz html [--float] [--reply] [-t %N] PATH|-`

Show a page you wrote beside your pane: a file, a directory with `index.html`, or `-` to read
HTML from stdin. The pane reloads each time a file the page loaded changes, so rewrite the file
and look again. A file serves only itself; a directory also serves the files under it, except
names that start with `.`. Pages can load scripts from CDNs. `--float` opens a floating pane.
A pane keeps one page pane: your next `zz html` shows its page there instead of splitting again.
`-t %N` shows the page in that pane if `zz html` opened it, otherwise beside it. With
`--reply` the page can call `zz.reply(text)`, which sends the text to your pane. Print the page
URL and the new pane. Over `--host`, PATH
is read on that host; send a local page with `zz --host NAME html -t %N - < page.html`.

### `zz capture-browser -t %N -o /absolute/out.png`

Save the latest browser frame as a PNG and print its path. Use an absolute path on the daemon's
host: the window showing the pane sends the picture and the daemon writes it there, wherever
that window runs. A desktop app talking to an older daemon writes the file on its own machine
instead. A capture straight after `set-browser-url` can show the old page; navigate with
`load-browser-url` to wait for the new one. On Linux and Windows, restart the app with `ZZ_BROWSER_SHARED_TEXTURE=0` if the GPU
path reports that readback is unavailable.

### `zz capture-simulator -t %N -o /absolute/out.png`

Save an iOS simulator pane's screen as an upright PNG and print its path. The desktop app on
the Mac that shows the pane sends the picture and the daemon writes it on this host.

### `zz tap-simulator -t %N X Y`

Tap the simulator at a point of that PNG, in its pixels. `swipe-simulator -t %N X1 Y1 X2 Y2`
drags between two points, `type-simulator -t %N text ...` types with the simulator's keyboard
(ASCII only), and `press-simulator -t %N home|lock` presses a button. `--kind simulator UDID`
on `new-pane` opens a pane on a device; `xcrun simctl list devices booted` lists them.

### Browser through CDP

CDP is off by default. Set `browser-remote-debugging-port = 9222` in `zz/config`,
or `ZZ_BROWSER_REMOTE_DEBUGGING_PORT=9222` in the client environment (which takes
precedence). Use `0` to disable it; enabled ports range from 1024 to 65535.
Open a browser pane to initialize CEF. Restart the client to change the port after
initialization. The loopback endpoint grants local processes access to your browser
session; enable it while you intend to grant that access.

Replace PORT with the configured port in one of these attach recipes:

```sh
agent-browser --cdp PORT snapshot -i
npx @playwright/mcp@latest --cdp-endpoint http://127.0.0.1:PORT
npx chrome-devtools-mcp@latest --browserUrl http://127.0.0.1:PORT
```

Correlate pane URLs with CDP targets:

```sh
zz list-panes -a -F '#{pane_id} #{pane_kind} #{browser_url}'
zz display-message -p -t %5 '#{browser_url}'
curl http://127.0.0.1:PORT/json/list
```

`#{browser_url}` gives the active tab URL, or an empty string for other pane kinds.
Compare it with each target's `url`; duplicate URLs need more context, and CDP may
list background tabs too. For `Page.captureScreenshot` with a clip, use `clip.scale`
of `1.0`; CEF off-screen rendering does not support custom capture scale.

Match the pane to its CDP target before attaching. Never create pages (`Target.createTarget`,
Playwright `newPage`, or tools that open a fresh tab on connect): CEF hosts those
as native Chromium windows outside zz. Install agent-browser instead of invoking
`npx` per call. Prefer `wait --load load` or `wait --text` over `networkidle`.

```sh
agent-browser --cdp 9222 snapshot -i
agent-browser --cdp 9222 click @e2
agent-browser --cdp 9222 wait --load load
```

";
    WORKSPACE_TOOLS_ADVANCED = r##"## Advanced state

Run `zz tools advanced` for state subscriptions and terminal-agent integration.

### `zz read-file [-s OFFSET] [-n BYTES] PATH` and `zz write-file [-a] PATH HEX`

The two steps `copy-file` is made of, on the daemon's host. `read-file` prints up to 262144
bytes of a file from OFFSET as lowercase hex; fewer bytes than asked means the file ended.
`write-file` replaces the file with the bytes HEX spells, or appends them with `-a`. PATH is
absolute or starts with `~/`.

### State and waiting

Read agent state through formats.
A terminal pane whose program reports OSC 7501 program status gets its most urgent
record mapped into `@agent_state`: `working`, `blocked`, `failed` for `error`, and
`idle` for `idle`, `done`, or no record. Once a pane has reported, the two heuristics
below stop writing `@agent_state` for it until a full reset.
Terminal panes running a listed agent CLI get `working`/`idle` from the OSC 9;4
progress bar through `#{agent_state}` and `@agent_state`. `@agent-progress-commands`
sets the whitespace- or comma-separated list of command basenames (default `claude`).
A terminal pane running Claude Code gets `working`/`idle` from Claude Code's own
session status in its peer registry once per second, and `blocked` while Claude Code
waits on a permission prompt or a question. Set `@agent-peer-state off`
at pane, window, session, or global scope to disable these updates.

```sh
zz list-panes -F '#{pane_id} #{agent_state} #{agent_pending_permission}'
zz agent-send -t %5 --wait "..."
```

A completed turn or permission request also rings
the pane bell; use `zz set-hook -g alert-bell 'display-message "agent needs attention"'`.

For foreign agents, read `@agent_state` with `zz show-options -p -t %5 -v @agent_state`.
Use `zz agent-send -t %5 --wait "..."` to send and wait for a turn, and use the
lifecycle hooks below to write the state. For transitions you did not cause,
`zz wait-for '@agent_state@%5'` observes a terminal's option writes; read the state
after waking.
`zz set-hook -g @option-changed` observes user-option writes.

The sticky channel is level-triggered like tmux's `wait-for`: a signal that happened
before the wait wakes it at once. A read-then-wait loop started right after a send
can see the pre-send idle and return early. Use `zz agent-send -t %N --wait "..."`
to wait for a turn on any pane kind. Use the channel loop to observe transitions
you did not cause.
`zz events -t %N` streams `agent-state-changed` lines for any pane kind.
`zz wait-for -E -F '#{==:#{pane},%N}' agent-state-changed` blocks until that pane's
next state change; add `-v` to print its `agent_state`.

The `zz refresh-client` verb supports `-B` subscriptions in control mode. Send this
on the control connection and read `%subscription-changed`:

```text
refresh-client -B 'agent:%5:#{agent_state}'
```

### Sending to terminal agents

A terminal pane running Claude Code or Codex takes no prompts from `agent-send`:
flip it to a chat with `zz flip-agent -t %N` and send there.

For a terminal pane running another agent, `--wait` requires a non-empty `@agent_state` and types the text. It waits for a
non-idle state followed by `idle`, and prints nothing on success in text mode.
`--json` prints turn facts with empty reply text. There, `--final`, `--notify` and
`--on-block allow|deny` exit 2 before sending. A pane that
starts idle must leave idle within 15 seconds; otherwise it exits 124. The overall
`--timeout` also exits 124. `failed` exits 1; `blocked` waits unless `--on-block fail`
requests exit 3. A missing `@agent_state` exits 1 before sending.

Terminal waits report their known duration and reply text with zero counters.

The inbox wait also ends when the pane's turn ends (`#{agent_state}` leaves `idle` and
comes back), printing the SendMessage reply if one came and nothing if the session
answered only in its pane; `blocked` exits 3 under `--on-block fail` there too.

`zz agent-send -t %N --wait ''` sends nothing, on any terminal agent: it returns at
once when the pane is idle and otherwise waits for the running turn to end, printing
nothing (read the pane), or a SendMessage reply that reached the inbox after an
earlier inbox wait timed out or its caller went away; the daemon keeps those for an
hour. It takes `--timeout`, `--json` and `--on-block wait|fail`. Use it to wait again
after a timeout, or after a restart, without nudging the agent.

### Peers

Use `zz agent-send -t %N --wait "..."` to send and wait for a turn. Peers are per
machine: for an agent on another host, use `zz --host NAME agent-send -t %N --wait "..."`.

A terminal pane becomes a peer when you set its pane `@name` option:
`zz set-option -p -t %3 @name codex-1`. A Codex pane with a session name receives
peer messages through the Codex queue described above. Other terminal peers,
and Codex sessions without a name yet, receive pasted and submitted text, so do
not name a bare shell pane. A queue lookup or delivery failure leaves the message
unsubmitted and logs the reason. The daemon hosts no vendor's server. Claude Code
terminal sessions keep their own registration.

### Foreign agents in terminal panes

A terminal agent other than Claude Code or Codex has no chat. Configure its lifecycle hooks (Stop,
Notification, or equivalent) to write a pane option:

```sh
zz set-option -p -t "$TMUX_PANE" @agent_state idle
zz set-option -p -t "$TMUX_PANE" @agent_state needs-approval
zz agent-send -t %5 --wait "..."
zz set-hook -g @option-changed 'run-shell "notify-send zz \"#{hook_target} #{hook_option}\""'
```

Each write signals the sticky channel `<option>@<pane>` and runs `@option-changed`.
Use `#{hook_target}` and `#{hook_option}` to identify the write.

"##;
    WORKSPACE_TOOLS_ETIQUETTE = r"## Etiquette

- Send to another person's agent only for an authorized hand-off.
- Do not send to `$ZZ_PANE`.
- Trim piped logs to the part the recipient needs.

### `zz debug-marker [NOTE]`

Write a `user_marker` line to the daemon log for later diagnostics.
Print nothing on success.
";
}
