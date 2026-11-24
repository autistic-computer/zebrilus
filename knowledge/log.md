# Update Log

## 2026-11-24

* Compacted the knowledge bundle. The full daily log from July 14 through November 24 is available in the [archived log](https://github.com/autistic-computer/zebrilus/blob/1ce632dce46bc0a4aa6167111f5625f3a7abaefd/knowledge/log.md) or locally with `git show 1ce632dce:knowledge/log.md`.
* `zz copy-file` transfers a file between machines. `load-browser-url` waits for navigation to settle. Remote browser captures are written on the daemon's host. See the [command reference](/tmux/commands.md).
* Chat messages and briefs identify their sender. The daemon relays a remote chat's reply to the calling chat. See [agent panes](/concepts/agent-panes.md).

## 2026-11-23

* Agent settings group accounts by machine. Account changes swap the CLI's native login. Child chats stay in their parent's card until opened. See [agent panes](/concepts/agent-panes.md) and [account switching](/designs/agent-account-swap.md).
* zz reads peer status and identity from the running Tailscale service through its local API. See [tailnet hosts](/designs/tailnet-node.md).

## 2026-11-21

* During November 16 through 21, desktop and thin clients gained interactive chat controls. The daemon added drivers for Antigravity, Grok and Cursor. Phone and web clients subscribe to a chat when it becomes visible. See [agent panes](/concepts/agent-panes.md).
* The desktop added an iOS Simulator pane. The phone clients added first-launch onboarding. See [simulator panes](/designs/simulator-pane.md) and [application configuration](/configuration/app-config.md).

## 2026-09-20

* The [TUI parity campaign](/tmux/tui-parity.md) records its September closeout, including the baseline, additional obligations and the evidence for each. The [compatibility registry report](/tmux/gaps.md) tracks command and behavior differences separately.
* For a specific earlier change, search the archived log by date, symbol or command. Use `git log -p -- knowledge/<path>` to trace a particular concept.
