---
type: Rust Crate
title: zz-cli crate
description: Headless executable, CLI dispatch and daemon startup shared with the desktop app, remote host targeting and owned updates.
resource: crates/zz-cli/src/lib.rs
tags: [crate, cli, daemon, tui, headless, packaging]
timestamp: 2026-11-27T00:00:00-03:00
---

# Startup and commands

`crates/zz-cli/src/main.rs` builds the headless `zz_cli` launcher.
`run_startup` in `crates/zz-cli/src/startup.rs`, re-exported from the crate root, serves both
that executable and the desktop app. The launcher handles local verbs, starts the daemon when
needed, executes command chains, or enters [the TUI](/designs/tui-client.md) through `zz-tui`.
`control_mode.rs` implements the tmux-style `-C` and `-CC` stdio front end.
See [source files](#source-files) for where each part lives.

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
`untargeted_host_input` and `SINGLE_HOST_VERBS` in `host_cli.rs` define the exceptions.
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

## Source files

`lib.rs` is the crate root. It declares the modules, glob-imports the ones split out of it so
they share one namespace, and re-exports their public functions so `zz_cli::run_startup` and
the other public paths still resolve. The types several modules or the tests build field by
field stay in it.

| Source under `crates/zz-cli/src` | Responsibility |
| --- | --- |
| `lib.rs` | Module declarations and re-exports, the shared constants, `StartupOptions`, `Startup`, `ApplicationArguments`, `SocketSelectionSource`, the daemon bootstrap and native attach argument types, `ChainRouting` and the command host thread-local |
| `startup.rs` | `run_startup`, the owned copy handover, the application working directory, the allocator options, the parent console and askpass entry points, and the application argument parser with the tmux label socket paths |
| `command_mode.rs` | `run_command_mode`, the local, host and TUI chain runners, the agent-send relay, the tmux shell command, the chain splitter and routing predicates |
| `help.rs` | Top-level and per-command help |
| `native_attach.rs` | Native `attach`: its argument parser, usage errors and the attach-session command it builds |
| `output.rs` | The chain output writer, stdout and stderr printers, the exit code contract and the error formatters |
| `kill_server.rs` | Local and host kill-server and the recovery after a failed kill |
| `daemon_spawn.rs` | Bootstrap arguments, spawning and restarting the daemon, the readiness pipe, session detach and inherited descriptors on Unix, connecting to a spawned daemon and finding the daemon executable |
| `launcher.rs` | The default client command, the Launch Services check, the startup directory and launching the desktop app |
| `host_cli.rs` | `--host` chains, fan-out to every host, untargeted verb checks, host connect failures and their hints |
| `control_mode.rs` | The `-C` and `-CC` front end; its tests are in `control_mode/input_tests.rs`, `frames_tests.rs`, `exit_tests.rs` and `notifications_tests.rs` |
| `*_tests.rs` | Tests for the crate root modules, one file per subject: `startup`, `command_mode`, `native_attach`, `output`, `kill_server`, `daemon_spawn`, `launcher`, `host_cli` |

The other files (`update.rs`, `diagnostics.rs`, `events.rs`, `fleet.rs`, `list_hosts.rs`,
`copy_file.rs`, `tailnet.rs`, `agent_hooks.rs`, `agent_login.rs`, `fork_chat.rs`) each own the
verb or subsystem they are named for.

The integration tests that run the built binary are one test binary, `tests/cli_binary/`.

| Source under `crates/zz-cli/tests/cli_binary` | Tests |
| --- | --- |
| `main.rs` | Help, usage and host tests that need no daemon |
| `daemon_autostart.rs` | The `Fixture`, pty and capture helpers shared below, and the tests that autostart a daemon: events, JSON output, startup config, new-session and native attach |
| `daemon_autostart/source_file.rs` | `source-file`, control config and `reload-config` |
| `daemon_autostart/aliases.rs` | Command aliases, caller stream alias groups and the cold prepared chain |
| `daemon_autostart/tui_status.rs` | Status rows of an attached TUI |
| `daemon_autostart/control_mode.rs` | The control mode stream parser and process helpers, and its startup, framing, signal, config and source tests |
| `daemon_autostart/control_mode/exit_status.rs` | Return codes, detach, end of file and wait-exit |
| `daemon_autostart/control_mode/inserted_commands.rs` | Background jobs, hooks and shutdown |
| `daemon_autostart/control_mode/notifications.rs` | Live notifications, refresh-client and pause-after |

