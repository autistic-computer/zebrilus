# Update Log

## 2026-11-26
* **Creation**: Proposed the [English to command translator](/designs/english-command-translator.md): a small local model that turns an English sentence and the current session's state into zz commands and runs them with no preview. Nothing is built.

* The mux crate's `command.rs` now holds the engine's core types, the dispatch `match`, alias resolution and shared helpers; each command family moved into a subject file under `crates/zz-mux/src/command/`, and its tests into subject test files beside them. The [zz-mux](/crates/zz-mux.md) page lists the files, and the [command set](/tmux/commands.md) maps every command name, engine or daemon, to the file that runs it.
* The daemon's `daemon.rs` now holds only the core state, the `Daemon` entry point and shared helpers; the `Shared` methods moved into subject files under `daemon/`, and effect application split into per-subject helpers and named post-lock drain steps. The [zz-daemon](/crates/zz-daemon.md) page is now a map of the request flow, the `ServerState` layout and the file that owns each concern, and [daemon lock order](/architecture/daemon-locks.md) records which mutexes come before and after `inner`. Pages that named `daemon.rs` for moved code point at the new files.

## 2026-11-25

* The desktop app marks hosts on an older zz and updates them on request. An outdated host carries an arrow in a ring on its sidebar row, and Settings › Hosts is a table with each host's version and Update or Restart. The dialog that opened on connect is gone. See [host updates in the app](/designs/host-update-ui.md).
* Chat view lists every machine and puts New chat on a machine's header. On the desktop the New chat screen has a target machine and attaches nothing until an agent is picked, and a picker pane offers the agents and the pane kinds in both sidebar views. See [chat mode](/designs/chat-mode.md).

## 2026-11-24

* Compacted the knowledge bundle. The full daily log from July 14 through November 24 is available in the [archived log](https://github.com/autistic-computer/zebrilus/blob/1ce632dce46bc0a4aa6167111f5625f3a7abaefd/knowledge/log.md) or locally with `git show 1ce632dce:knowledge/log.md`.
* `zz copy-file` transfers a file between machines. `load-browser-url` waits for navigation to settle. Remote browser captures are written on the daemon's host. See the [command reference](/tmux/commands.md).
* Chat messages and briefs identify their sender. The daemon relays a remote chat's reply to the calling chat. See [agent panes](/concepts/agent-panes.md).
* A Claude Code login is checked against the account ids it carries: a cached name of another organization no longer labels it, and a login changed outside zz is noticed. See [account switching](/designs/agent-account-swap.md).

## 2026-11-23

* Agent settings group accounts by machine. Account changes swap the CLI's native login. Child chats stay in their parent's card until opened. See [agent panes](/concepts/agent-panes.md) and [account switching](/designs/agent-account-swap.md).
* zz reads peer status and identity from the running Tailscale service through its local API. See [tailnet hosts](/designs/tailnet-node.md).

## 2026-11-21

* During November 16 through 21, desktop and thin clients gained interactive chat controls. The daemon added drivers for Antigravity, Grok and Cursor. Phone and web clients subscribe to a chat when it becomes visible. See [agent panes](/concepts/agent-panes.md).
* The desktop added an iOS Simulator pane. The phone clients added first-launch onboarding. See [simulator panes](/designs/simulator-pane.md) and [application configuration](/configuration/app-config.md).

## 2026-09-20

* The [TUI parity campaign](/tmux/tui-parity.md) records its September closeout, including the baseline, additional obligations and the evidence for each. The [compatibility registry report](/tmux/gaps.md) tracks command and behavior differences separately.
* For a specific earlier change, search the archived log by date, symbol or command. Use `git log -p -- knowledge/<path>` to trace a particular concept.
