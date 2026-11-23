---
type: Design Plan
title: Daemon update . a zz-owned headless copy and zz update
description: "Plan to update the headless zz (CLI and daemon) on any machine without touching the package that installed the desktop app. Each machine keeps one zz-owned copy of the headless binary at a fixed path under its data directory, a packaged zz hands over to it when it is newer, and `zz update` installs it on this machine or on named hosts over the connection zz already has: the remote CLI for ssh hosts, a daemon command for tailnet hosts. Restart stays a separate, explicit step, and an app never restarts a daemon newer than itself. Built, not merged."
status: Approved 2026-11-23; steps 1 to 6 and 8 built on the daemon-update branch, not merged; step 2 and step 7 landed on main; checked on one Mac with a loopback ssh host, not yet on xps, alienware or win-desktop
tags:
- update
- daemon
- cli
- fleet
- design-plan
timestamp: 2026-11-23T00:00:00-03:00
---

# Overview

[Split updates](/designs/split-updates.md) let a client update without restarting the daemon. This
plan covers the other half: moving a daemon to a newer build from another machine, with no package
manager, no sudo and no desktop app involved.

Two parts:

1. **Owned copy.** Each machine keeps one zz-owned copy of the headless binary, outside the package
   that installed the app. A packaged zz that starts hands over to it when it is newer.
2. **`zz update`.** One CLI verb installs an owned copy on this machine or on named hosts, over ssh
   or tailnet, and restarts the daemon only when asked.

Not in this plan: restart when idle, session restore, keeping processes across a restart, automatic
updates, pushing a local build to a host, a desktop button for any of it, macOS bundle installs,
and keeping a tailnet daemon up after its last session closes. A restart still ends every session
on that machine.

# Today

Checked on 2026-11-23 from a Mac on 0.30.0 (epoch 125, revision 9).

| Host | Transport | Install | Daemon binary | `zz fleet install` |
| --- | --- | --- | --- | --- |
| xps | ssh | deb 0.29.0 | `~/.local/bin/zz`, a link to `/usr/lib/zz/cli` | refuses: the link target is not one it owns |
| alienware | tailnet | pacman `zz-bin` 0.29.0 | `/usr/lib/zz/cli`, started by the app | not offered: tailnet hosts are filtered out |
| win-desktop | tailnet | a build folder, nothing installed | `zz.exe` itself, with the `daemon` verb | not offered |

- The daemon binary belongs to the package that installed the app. `daemon_executable()`
  (`crates/zz-cli/src/lib.rs`, one caller: `spawn_daemon`) runs the `cli` beside the running
  executable, or the executable itself. The Windows bundle ships no `cli.exe`, so `zz.exe` is the
  daemon there.
- `zz fleet install` and the desktop's "Update zz on {host}?" run `remote_install_script`
  (`crates/zz-daemon-client/src/endpoint.rs`) over an ssh shell. It pipes `install.sh` and exits
  with `zz-install-elsewhere` for any `~/.local/bin/zz` it does not recognise. Tailnet endpoints
  have no shell.
- `--host` with a CLI-local verb (`fleet`, `list-hosts`, `tools`, `protocol-version`, `--version`)
  runs the remote `zz` over ssh (`run_host_cli`, `run_remote_cli`). A tailnet host answers "this
  command runs on the host over ssh". Mux commands go to the daemon as `Exec`, over ssh or tailnet
  alike (`events` has a route of its own).
- Nothing on the wire carries the zz version. `ServerHello` and `Welcome` carry `revision` and
  `platform`. `zz list-hosts` prints the installed CLI's version for ssh hosts and nothing for
  tailnet hosts. `#{version}` is `3.8-zz`.
- The headless binary has no HTTP client. `stage_headless` in `scripts/ci.sh` fails when `ureq` is
  in zz-cli's tree, with "zz-cli links the update checker's TLS stack"; the in-app installer
  (`zz_config::update`, feature `update`) is only on in `crates/zz`. `rustls`, `hyper`, `flate2`
  and `sha2` are in zz-cli's tree all the same, so the rule no longer keeps TLS out.
- The desktop offers "Restart the zz daemon?" for any epoch mismatch, whichever side is newer
  (`StaleDaemonInfo::from_error` in `crates/zz/src/mux/client.rs` takes every
  `IncompatibleDaemon`), and `auto-restart-stale-daemon` does it without asking. A local app that
  loses its daemon to `ProtocolMismatch` or `ClientTooOld` keeps reconnecting
  (`finish_connect_failure`).
- On Windows a program that listens gets a firewall prompt per executable path
  ([tailnet node](/designs/tailnet-node.md)), and `prepare_socket` takes no start lock there.
- Headless release assets exist for Linux x86_64 and aarch64 and macOS arm64
  (`zz-{v}-headless-{os}-{arch}.tar.gz` plus `.sha256`, holding `zz`). Windows has none. The macOS
  one carries only the linker's ad-hoc signature.
- A tailnet host is reachable only while its daemon runs. `should_shutdown_if_empty`
  (`crates/zz-daemon/src/daemon.rs`) decides when a daemon with no subscribers exits. By default it
  exits only after it has held a session and lost its last one (`exit_empty_armed`), so a daemon
  that never had a session stays up. `exit-empty off` keeps an empty one up; `exit-unattached on`
  stops one with sessions and nobody attached.

# Decisions

## 1. One owned copy at a fixed path

```
<data dir>/zz/owned/
  zz            zz.exe on Windows
  build-info    version, epoch, revision, min-client-revision, min-daemon-revision
  .lock
```

- `<data dir>` is `app_identity::platform_data_dir()`, so zz Dev keeps its own tree. The cache
  directory is wrong for it: a cleaner would delete a daemon's binary.
- The path never changes, so a Windows firewall rule for the daemon is granted once.
- `build-info` is what the copy prints for a new CLI-local verb, `zz build-info`. It is written
  last and removed first: when it is there, the copy beside it is complete.
- A release older than the verb cannot become an owned copy.

## 2. A packaged zz hands over to a newer owned copy

On Unix the headless binary reads `build-info` before it acts. If the owned copy is newer than
itself, it replaces itself with it (`exec`, same arguments), for every verb. The process id and
inherited descriptors stay, so `--bootstrap-ready-fd` and the ssh start script work unchanged, and
`zz --version` and `zz protocol-version` answer for the copy that would become the daemon, which is
what the ssh probe asks. A command client is then never older than the daemon it talks to.

It does not hand over when:

- it runs as a helper (`--zz-pty-exec` and the other entry points `main` handles before
  `run_startup`), which stay ahead of the check;
- it is the daemon's pinned build: `ZZ_TMUX_EXECUTABLE` is set, which covers panes and the shell
  jobs the daemon starts (`tmux_shim_environment`, `configure_shell_job_environment`);
- it was started by a local client (`--bootstrap-server-id` is present, which the ssh start script
  never passes) and the owned copy is on another epoch, or its revision is below this build's
  `MIN_DAEMON_REVISION`, or its `min-client-revision` is above this build's revision. The local
  app then gets a daemon it can attach to. This check runs before the daemon adopts the bootstrap
  descriptor;
- `ZZ_OWNED_COPY=0` is set;
- it is itself the result of a handover. The marker for that is read and removed from the
  environment at startup, so it never reaches a child;
- the `exec` fails. It logs the error and carries on as itself.

On Windows there is no `exec` and the app's own executable is the daemon. `daemon_executable()`
applies the same rule there and returns the owned `zz.exe` when it qualifies.

The cost is one small file read per `zz` invocation, and one `exec` when the owned copy is newer.

## 3. The machine being updated downloads for itself

`install(version)`:

1. Take an OS file lock on `owned/.lock` (`File::try_lock`), which the system drops if the process
   dies. A second install fails with "an update is already running".
2. Delete what an earlier run left: `.staging-*` and `zz.old-*`. A file that cannot be deleted (a
   running `zz.exe` on Windows) stays for the next run.
3. Refuse a version that is not newer than the binary doing the install: "already zz {v}", or
   "zz {v} is older than this zz; `zz update --remove` goes back to the packaged one".
4. Fetch `https://github.com/autistic-computer/zebrilus/releases/download/v{v}/{asset}` and its
   `.sha256`, hash while streaming and compare. A mismatch deletes the download and fails. A
   release with no asset for this machine fails with "zz {v} has no headless build for
   {os}-{arch}".
5. Unpack into `owned/.staging-<pid>/` and run the unpacked binary with `build-info` and
   `ZZ_OWNED_COPY=0`. A copy that cannot start here, or reports another version, is discarded.
6. Swap: delete `build-info`, rename `zz` aside to `zz.old-<n>`, rename the staged binary to `zz`,
   write the new `build-info` by rename.

A crash at any step leaves either the old copy whole or no `build-info`, and without `build-info`
the packaged zz runs.

- **Transport.** The plan reuses the in-app downloader instead of writing a second one: `parse_releases`, `Channel`, `ReleaseWire`, the checksum parsing, the streaming hash
  and `download` (`crates/zz-config/src/update.rs`, `update/install/download.rs`) move out from
  behind the `update` feature, and the headless asset names join them. That puts `ureq` in zz-cli's
  tree, so the `stage_headless` rule goes. Measured on macOS arm64 with `cargo build --release -p
  zz-cli`: 33,598,416 bytes before (93fda43ea), 34,228,032 bytes with the downloader, `tar`,
  `zz_config::owned` and `zz update`, a difference of 629,616 bytes (0.60 MiB). ureq runs on the
  rustls aws-lc-rs provider the tailnet node already links, without ureq's default `ring`: with
  both providers on, rustls has no default and the tailnet's `ClientConfig::builder()` panics.
  `stage_headless` now fails when anything turns on rustls's `ring` in zz-cli's tree.
  Unpacking uses `flate2`, already linked, and the `tar` crate. The other choice is the system's
  `curl` and `tar` behind the same function: no new dependency, a second download path to keep.
  The two reviews split on this; fabrico picked the shared downloader on 2026-11-23.
- The checksum comes from the same release as the archive. It catches a broken download, not a
  forged release, which is what `install.sh` and the in-app installer give today.
- `HTTPS_PROXY` is honored: the HTTP client reads `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and
  `NO_PROXY` by default (ureq 3.5 `Proxy::try_from_env`). A host with no route to the release
  fails, and its line says so.
- `ZZ_UPDATE_BASE_URL` replaces the download origin (`…/releases/download`) for tests and mirrors.
- The version is validated as semver before it reaches a URL. There is no `--url` and no file
  argument.
- The code is one module with no feature gate, used by both the CLI and the daemon. Proposed home:
  `zz_config::owned`.

## 4. `zz update` is one CLI verb with one route per transport

```
zz update [--check | --remove] [--version V | --beta] [--restart [-y]] [-a | <host>...]
```

| Target | Route |
| --- | --- |
| this machine | the CLI runs `install` in its own process; no daemon needed |
| ssh host | the remote CLI runs the same verb: `run_remote_cli(["update", ...])` |
| tailnet host | the daemon runs it: `update-server [--version V]` over `Exec` |

- `-a` is this machine and every configured host. Hosts run in parallel and print one line each, as
  `zz fleet install` does: `alienware: zz 0.31.0 installed; daemon still 0.30.0 with 3 sessions,
  restart with --restart`.
- `update-server` is a new daemon command: a name in `DAEMON_COMMAND_NAMES` and a spec in
  `DAEMON_COMMAND_SPECS`, a `DaemonCommandDispatch` variant, a `DAEMON_COMMAND_DISPATCHES` row and
  its match arm. It parks with `CommandWait` and `CommandState`, runs `install` on a job thread
  holding no daemon lock, resolves the command on every outcome, and sends its lines through
  `release_command_stdout` (`EventPayload::CommandStdout`). The CLI sets a stdout handler on its
  `CommandClient` to print them. It needs no wire change: a daemon that lacks it answers `unknown
  command: update-server`, which the CLI turns into "zz on {host} is too old to update itself;
  update it there once".
- The ssh route does not touch the daemon protocol, so it works across an epoch change. The tailnet
  route cannot cross one: `Exec` carries the epoch. A tailnet-only host has to be updated before
  the asking machine moves to the new epoch (`zz update -a --version V` from the old build), or by
  hand.
- The version: the newest release on the build's channel for this machine, and the asking
  machine's version for a host, so `zz update xps` means "bring xps to my version". `--version`
  names one. `--beta` is the newest beta, for this machine or a host. Finding the newest release
  happens on the asking machine, so a host is always sent a version.
- `--remove` deletes the owned copy under the same lock, so the packaged zz runs from the next
  start.
- `zz update` refuses on a zz Dev build, and on a macOS bundle install with "this Mac's zz updates
  through the app": privacy grants follow the app bundle, and the headless macOS asset is not
  signed with our identity. A headless install on macOS is allowed.
- `zz fleet install` stays for putting zz on a machine that has none.

## 5. Every daemon can say what it is

- New format variables `#{zz_version}` and `#{zz_revision}`. An older daemon expands them to
  nothing, which reads as "older than this feature".
- `zz update --check [-a | <host>...]` changes nothing and prints, per machine: the running
  daemon's version and revision, the owned copy, the packaged version where the route can see it,
  the target version, and its session count.
- `zz list-hosts` fills `host_version` from the running daemon when the host answers.

## 6. Restart is asked for, and the daemon replaces itself

`--restart` runs after a successful install.

- It prints "This ends N sessions on {host}." and asks, unless `-y`. Without a terminal and without
  `-y` it refuses.
- New daemon command `restart-server`. In order: send the command's reply, stop sessions and jobs,
  stop the tailnet node and wait for its thread (`tailnet::spawn` drops the handle today; with
  `tailnet = own` its stop runs too, so the dial socket is cleaned), drop the listener, remove the
  socket, the identity file and the pin guards, then start the owned copy, directly and not
  through the handover rule. On Unix that is an `exec` of the copy; on Windows it starts the copy
  and exits.
- The successor gets the same socket and config arguments, a fresh server id and no bootstrap
  arguments. On Unix the start lock (`StartLock`, `prepare_socket`) is held from before the socket
  is removed until the successor has bound it, so a reconnecting app cannot start another binary
  in between. Windows has no start lock: the successor retries binding the pipe and the tailnet
  port for up to 10 s while the old process exits.
- Tailnet keys and the allow list stay on disk, so the host keeps its name and its peers. The web
  gateway is a separate process; its connections drop and it reconnects.
- The successor starts with no sessions. Under the default idle rule it stays up until it has held
  a session and lost it, so a tailnet host stays reachable with nobody at it. When the host sets
  `exit-unattached on` or `exit-empty on`, the host's line says the daemon will not stay up.
- A daemon that lacks `restart-server` gets `kill-server` on this machine and on ssh hosts, where
  the next connect starts the new build. A tailnet host never gets `kill-server` from `zz update`:
  nothing could start it again. Its line says to restart it there.
- When the target is on another epoch than the running daemon, the line before the question adds
  "The desktop app on {host} is on epoch N and cannot attach to it until it updates."

## 7. An app never restarts a daemon newer than itself

Without this, the app on a host undoes a cross-epoch update: it sees a mismatch, restarts the
daemon, and its bootstrap start cannot hand over across the epoch, so the packaged build comes
back and the machine that asked is locked out.

- `StaleDaemonInfo::from_error` returns a restart offer only when the daemon's epoch is below this
  build's, or unknown, or for `DaemonTooOld`. `auto-restart-stale-daemon` follows the same rule.
- A daemon on a newer epoch shows what `ClientTooOld` shows today: "This zz is too old for the
  running daemon", with Update when the update check knows a release. The raw terminal and the CLI
  print the message and do not offer `--restart-daemon`.
- A local app that loses its daemon to a newer epoch or to `ClientTooOld` stops reconnecting and
  shows that screen.

# Build order

1. `zz build-info`, `#{zz_version}`, `#{zz_revision}`. They ship first so later builds can be
   inspected and installed.
2. Decision 7 in the desktop app and the raw terminal. It stands alone and has to be on a host
   before a cross-epoch update reaches it.
3. The download code out from behind the feature, with the size measured. `zz_config::owned`:
   `install`, the self-check, `--remove`. `zz update` and `zz update --check` for this machine.
4. The handover on Unix and the `daemon_executable()` rule on Windows.
5. `restart-server` and `--restart` for this machine.
6. Hosts: the ssh route, `update-server` and the tailnet route, `-a`, per-host lines, `--check`
   across hosts, `host_version` in `list-hosts`.
7. Release: a Windows headless asset. `scripts/package-headless.sh` learns Windows and `zz.exe`,
   the Windows job in `.github/workflows/release.yml` builds `zz-cli`, signs it as it signs the
   bundle, and uploads `zz-{v}-headless-windows-x64.tar.gz` with its `.sha256`, named like the
   other Windows assets. `publish` does not wait on the Windows job today, so a release can lack
   the asset; step 4 of `install` covers that.
8. Pages: [process model](/architecture/process-model.md),
   [session persistence](/concepts/session-persistence.md), the zz-cli crate page, and the
   `zz tools --skill` text.

Each host needs one package update by hand to a release that has steps 1 to 6. After that its
daemon never needs the package again.

# Done when

1. From the Mac, `zz update alienware` installs the target version with no sudo and no change to
   the pacman package, and its sessions keep running.
2. `zz update alienware --restart -y` leaves alienware answering
   `zz --host alienware display-message -p '#{zz_version}'` with the target version within 30 s,
   with nobody touching alienware.
3. `zz update xps` works the same over ssh, including when xps is on an older epoch.
4. `zz update --check -a` prints a running version for every host, tailnet ones included.
5. A download whose checksum does not match changes nothing on the host, exits non-zero, and the
   host's line says why.
6. After the host's package moves past its owned copy, the next daemon start runs the packaged
   binary.
7. On win-desktop, steps 1 and 2 pass with an installed zz, and the second update raises no
   firewall prompt.
8. With the desktop app open on a host, a restart onto another epoch leaves that app showing "This
   zz is too old for the running daemon", and the new daemon keeps running.
9. `zz update --remove` on a host, then a daemon start, runs the packaged zz.

# Tests

- The handover rule, as a table of exact picks: owned newer on the same epoch; owned newer on
  another epoch when started by a local client; owned newer but below this build's daemon floor;
  owned older; `ZZ_TMUX_EXECUTABLE` set; `ZZ_OWNED_COPY=0`.
- The asset name and URL for each OS and architecture, as exact strings.
- `install` with a wrong checksum leaves `owned/` as it was.
- `install` stopped between the two renames of the swap leaves no `build-info`.
- `StaleDaemonInfo::from_error` gives no restart offer for a daemon on a newer epoch.

Everything else is checked on xps, alienware and win-desktop against the list above.

# Review

Two agents reviewed this on 2026-11-23 against the source. Both picked the same answer for these,
and the plan follows them:

- Hand over for every verb, with helpers and pinned execution left out.
- ssh hosts keep the remote CLI route, because it crosses an epoch and needs no daemon.
- No epoch-independent way into a tailnet daemon. "Update before you cross" is enough for now.
- macOS bundle installs stay out.
- The data directory, not the cache.
- A restarted daemon starts empty; no made-up session.

# Open

1. Whether a signed headless asset would let a macOS bundle install use an owned copy. Not checked.

# Citations

- `crates/zz-cli/src/lib.rs` `daemon_executable`, `daemon_executable_from`, `spawn_daemon`,
  `run_command_mode`, `run_host_cli`
- `crates/zz-cli/src/fleet.rs` `fleet_install`, `install_targets`
- `crates/zz-cli/src/list_hosts.rs` `probe_endpoint`
- `crates/zz-daemon-client/src/endpoint.rs` `remote_install_script`, `remote_path_fallback`,
  `remote_daemon_start_body`, `run_remote_cli`
- `crates/zz-daemon-client/src/lifecycle.rs` `terminate_incompatible_daemon`
- `crates/zz-daemon/src/daemon.rs` `pinned_executable`, `DaemonCommandDispatch`,
  `should_shutdown_if_empty`, `StartLock`, `prepare_socket`, `release_command_stdout`
- `crates/zz-daemon/src/lib.rs` `tmux_shim_environment`, `configure_shell_job_environment`
- `crates/zz-daemon/src/daemon/terminal_requests.rs` `CommandWait`, `CommandState`
- `crates/zz-protocol/src/catalog.rs` `DAEMON_COMMAND_NAMES`, `DAEMON_COMMAND_SPECS`
- `crates/zz-protocol/src/message.rs` `PROTOCOL_VERSION`, `REVISION`
- `crates/zz-protocol/src/app_identity.rs` `platform_data_dir`, `DIRECTORY`
- `crates/zz-config/src/update.rs` `parse_releases`, `Channel`, `ReleaseWire`, `checks_enabled`
- `crates/zz-config/src/update/install/download.rs` `expected_checksum`, `verify`
- `crates/zz/src/mux/client.rs` `StaleDaemonInfo::from_error`, `finish_connect_failure`
- `crates/zz/src/workspace/view.rs` `prompt_stale_daemon`
- `crates/zz-daemon/src/tailnet.rs` `spawn`
- `scripts/ci.sh` `stage_headless`
- `scripts/package-headless.sh`, `.github/workflows/release.yml`, `install.sh` `install_headless`
- [split updates](/designs/split-updates.md), [process model](/architecture/process-model.md),
  [tailnet node](/designs/tailnet-node.md)
