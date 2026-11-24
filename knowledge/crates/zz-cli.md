---
type: Rust Crate
title: zz-cli crate
description: Headless executable, CLI dispatch and daemon startup shared with the desktop app, remote host targeting and owned updates.
resource: crates/zz-cli/src/lib.rs
tags: [crate, cli, daemon, tui, headless, packaging]
timestamp: 2026-11-24T00:00:00-03:00
---

# Startup and commands

`crates/zz-cli/src/main.rs` builds the headless `zz_cli` launcher.
`crates/zz-cli/src/lib.rs` provides `run_startup` for both that executable and the desktop
app. The launcher handles local verbs, starts the daemon when needed, executes command chains,
or enters [the TUI](/designs/tui-client.md) through `zz-tui`. `control_mode.rs` implements
the tmux-style `-C` and `-CC` stdio front end.

macOS and Linux desktop bundles install the headless launcher as `cli` next to the GUI
executable. Headless archives ship it as `zz`. The GUI can spawn its sibling launcher as the
daemon, while a headless installation uses its own binary. `zz app` opens the desktop
application where installed. See [desktop packaging](/crates/zz-xtask.md).

```sh
zz --help
zz tools --skill
zz protocol-version
zz build-info
```

## Selecting a machine

Global `--host NAME` goes before the verb. A configured host name selects its endpoint.
An unconfigured name can be an SSH destination. Arguments after `--` belong to the child
command. For example, `zz run-pane -t %3 -- ls --help` runs the requested help in that pane.

```sh
zz fleet add build user@buildbox
zz --host build list-sessions
zz --host all list-panes -a --json
zz --host build run-pane -t %3 -- pwd
```

For a remote mutation whose target would default to the caller's pane, window or session,
supply `-t`. Moves with a default source may also require `-s`. Creating a session or an
attached window establishes a target for the rest of that command chain.
`--host all` accepts eligible read-only chains, runs hosts in parallel with unattended SSH,
identifies each host in the output and reports failure if any host failed.
`untargeted_host_input` and `SINGLE_HOST_VERBS` in `lib.rs` define the exceptions.
For a remote installation or login problem, see [client routes](/crates/zz-daemon-client.md)
and [remote installation](/crates/zz-daemon.md#installing-zz-on-a-remote).

## zz update and the owned copy

`zz update --check` reports the installed and daemon versions. `zz update` installs the
headless release in the application's `owned/` data directory. `--version V` selects an
explicit release, `--beta` selects the beta channel and `--remove` removes the owned copy.
Named hosts and `-a` select remote and fleet updates. In zz Dev the update command is disabled.
The macOS app bundle has its own updater.

`crates/zz-config/src/owned.rs` takes an installation lock, verifies the archive checksum,
runs the staged binary's `build-info`, swaps the binary and writes the metadata last.
A newer owned copy can take over a packaged Unix launch. Bootstrapping a daemon also
checks both peers' compatibility. `ZZ_OWNED_COPY=0` disables handover.
`zz update --restart` additionally requests a daemon restart, which ends its sessions.
See `crates/zz-cli/src/update.rs`, [process model](/architecture/process-model.md) and
[wire epochs](/protocol/wire-protocol.md#wire-epochs).

`cargo tree -p zz-cli -e normal` shows what the headless binary links.
`install.sh --headless` selects a headless installation.
