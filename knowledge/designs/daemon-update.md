---
type: Design Plan
title: "Updating the headless CLI and daemon"
description: "zz update installs a verified headless binary and restarts the running daemon only when requested."
status: "Released in 0.31.0, checked on three hosts 2026-11-24"
resource: crates/zz-cli/src/update.rs
tags:
- update
- daemon
- cli
- fleet
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

`zz update [--check | --remove] [--version V | --beta] [--restart [-y]] [-a | host...]` manages the copy under the zz data directory. The package manager still owns the packaged installation. Installing the copy leaves existing sessions running. A daemon restart ends sessions on that machine.

| Target | How it updates |
| --- | --- |
| Local machine | The CLI downloads the release and checksum, stages and verifies the executable with `build-info`, then swaps the owned copy under an install lock. |
| SSH host | The local CLI invokes the remote CLI. An SSH shell remains available across a daemon wire epoch change. |
| Tailnet host | The remote daemon handles `update-server --version V` or `--remove`. The existing connection must support the command. |
| WSL distribution | `zz update` directs the caller to `zz fleet install NAME`. |

`-a` includes this machine and hosts returned by `fleet_hosts_with_tailnet`. A multi-machine restart requires `-y`. Local updates refuse zz Dev and an executable inside a macOS app bundle. `zz update --check` reports the target, owned version and daemon state when available.

`owned::hands_over` chooses a newer copy for ordinary invocations. Local bootstrap also checks the wire epoch and revision floors. Pinning, prior handover or `ZZ_OWNED_COPY=0` prevents another handover. Unix passes the held start lock to the replacement daemon with `--start-lock-fd`. Windows starts the successor with `--retry-bind`.

Source: `crates/zz-cli/src/update.rs` (routing and confirmation), `crates/zz-config/src/owned.rs` (install and handover), `crates/zz-daemon/src/daemon/update_server.rs` (remote install), and `crates/zz-daemon/src/daemon/restart.rs` (restart). For compatibility policy see [split updates](/designs/split-updates.md). The [desktop host update design](/designs/host-update-ui.md) tracks the UI work.

# Recorded checks

Checked on real machines on 2026-11-24 with zz 0.31.0, the first release that carries the headless
assets, from a Mac on 0.30.0 against three machines whose packaged zz was a 0.30.0 build with this
feature:

| Machine | Route | Install | Restart |
| --- | --- | --- | --- |
| xps (Ubuntu) | ssh, the remote CLI | owned 0.31.0 installed | daemon came back on 0.31.0 |
| alienware (CachyOS) | tailnet, `update-server` | owned 0.31.0 installed | `restart-server` kept the pid, the daemon runs `owned/zz daemon --start-lock-fd 3`, and the open desktop app stayed attached |
| win-desktop (Windows 11) | tailnet, `update-server` | owned `zz.exe` 0.31.0 installed | `restart-server` started `owned\zz.exe ... daemon --retry-bind` in the user's session, and the open app attached again |

- The three installs together took 4.1 s, the two Linux restarts 2.1 s, the Windows restart 3.6 s.
- On both Linux machines `zz --version` printed 0.31.0 after the install and 0.30.0 with
  `ZZ_OWNED_COPY=0`.
- Every machine answered `#{zz_version}` with 0.31.0 afterwards with nobody touching it.
- One command sent to win-desktop three seconds after its restart failed once with a misleading
  error and worked on the next try. Not looked into.

Still never run: an update across an epoch change, the cross-epoch warning, a restart with an agent
turn in flight, and `zz update -a` from a machine that also updates itself.

Whether a signed headless asset can be used by a packaged macOS app is still open.
