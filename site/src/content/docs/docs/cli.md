---
title: CLI
description: One binary for the GUI, the daemon, and the command line.
---

The `zz` binary is the GUI, the daemon, and the CLI. Any supported tmux
command works from any shell against the running daemon:

```sh
zz list-panes
zz send-keys -t %3 'make test' Enter
zz split-window -h
zz new-window -n logs
zz detach-client
```

Targets use tmux syntax: `$session`, `@window`, `%pane`, all stable for a
pane's lifetime, visible in `choose-tree` and `display-panes`.

`zz --help` lists every command and the global flags; `zz <verb> --help`
shows one command. Neither needs a daemon.

## Other machines

`--host` goes before the verb and runs it on another machine over ssh:

```sh
zz list-hosts                            # the hosts in zz/config
zz --host desktop list-panes -a
zz --host desktop send-keys -t %3 'make test' Enter
zz --host you@box:2222 list-sessions     # any ssh destination
zz --host all inspect -a --json          # read-only verbs, every host
zz --host desktop                        # attach there
```

Pane IDs belong to their host, so `send-keys`, `send-text`, `run-pane`,
`paste-buffer` and `agent-send` need `-t` over `--host`. With `all`, text
lines start with `NAME: ` and JSON lines carry `host_name`.

| Command | What it does |
| --- | --- |
| `zz list-hosts [-F FORMAT] [--json]` | list hosts with their state and version; a `-F` without `host_state`, `host_version`, `host_protocol` or `host_error` skips the ssh probe |
| `zz fleet add NAME DEST` / `fleet remove NAME` | write or delete a `host-NAME` line |
| `zz fleet install [--restart-daemon] (-a \| NAME...)` | install or update zz there, in `~/.local/bin` |
| `zz fleet sync` | make every listed machine trust the others' zz keys and know their host lines |
| `zz fleet key` | print this machine's zz ssh key |

## Beyond tmux

| Command | What it does |
| --- | --- |
| `zz new-window --kind browser [--profile NAME] [URL]` | new window with a browser pane |
| `zz split-window --kind browser [--profile NAME] [URL]` | split into a browser pane |
| `zz set-browser-url -t %N URL` | navigate a browser pane |
| `zz capture-browser -t %N -o /tmp/out.png` | screenshot a browser pane (absolute path) |
| `zz split-window --kind picker [-h\|-v]` | split into an empty pane and pick its type |
| `zz new-window --kind agent [--provider codex\|claude-code]` | new window with an agent pane |
| `zz tools` | the command catalog written for agents |

Two more verbs, `zz agent-send` and `zz send-last-output`, talk to an Agent
pane. They need `experimental-agent-pane` on and a pane to aim at; see
[agent panes](/docs/agents/).

## Sessions

The daemon starts on first launch and outlives every window. Closing a
window detaches. PTYs, layouts, scrollback, and browser URLs and profiles all
survive and come back on reattach. Persistence is daemon-lifetime: a reboot
starts fresh.
