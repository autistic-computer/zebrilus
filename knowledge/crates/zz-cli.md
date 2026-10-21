---
type: Rust Crate
title: zz-cli crate
description: Headless CLI, daemon entrypoint, and terminal attach client shared with the desktop app.
resource: crates/zz-cli/src/lib.rs
tags: [crate, cli, daemon, tui, headless, packaging]
timestamp: 2026-10-20T18:00:00-03:00
---

# Overview

`zz-cli` provides the CLI verbs, `zz daemon`, `zz proxy`, `zz fleet`, and `zz attach`.
The `zz-cli` package builds the headless `zz_cli` binary; the `zz` package builds the
GPUI desktop binary `zz` and links the shared `zz-cli` library for CLI dispatch,
daemon spawning, and terminal attach through the `zz-tui` library.

Desktop bundles install `zz_cli` as `cli`: `lib/zz/cli` on Linux and
`Contents/MacOS/cli` on macOS. The `zz` symlink on PATH points to that `cli` file.
Headless archives instead ship the same binary as `zz`, alongside the MIT and Apache licenses.
The release workflow (`.github/workflows/release.yml`) builds the Linux headless binary with
`cargo zigbuild --target <arch>-unknown-linux-gnu.2.28`, so it needs glibc 2.28 or newer
(Debian 10, RHEL 8) and only `libc`, `libm`, `libpthread`, `libdl` and `librt`. The desktop
tarball, AppImage and deb are still built natively on Ubuntu 24.04.

The `daemon_executable` rule makes a GUI spawn the sibling `cli` and the headless
binary spawn itself. Sessions survive when the desktop client quits.
The spawner hands the daemon's `Child` to a `zz-daemon-reaper` thread that waits on it,
so a daemon that stops while the GUI or an attach client is still running does not stay
behind as a zombie.
`zz app` opens the desktop app beside the headless binary, or explains that the
app is not installed on a headless host.

The headless dependency tree contains no GUI crates. CI's "Headless binary stays
GUI-free" step checks the normal dependencies with `cargo tree -p zz-cli`.
Install it on Linux or macOS with `install.sh --headless`, or run
`curl -fsSL https://zzmux.sh/install.sh | sh -s -- --headless`.
The installer puts `zz` in `~/.local/bin`; `--prefix <dir>` selects another prefix.

# Global flags and other machines

`application_arguments` reads zz's own flags only before the verb: `--host NAME` / `--host=NAME`
there, and `--socket` there or after `daemon`, `proxy` and `app` (the launcher and the remote
start scripts pass it in that spot). Nothing after `--` is a zz flag, so a nested
`zz run-pane -t %3 -- zz --host xps inspect -a` or a `-- --host 0.0.0.0` payload stays intact;
`diagnostics::application_args_from` and `verbose_requested` stop at that `--` too, so a
`-- cargo test --verbose` payload keeps its word and leaves verbose logging off, and a verb's
`--help` counts only before it, so `run-pane -t %3 -- ls --help` runs `ls --help`.
`zz --help` lists the global flags and every catalogued verb; `zz -h` stays the pinned tmux banner.

`configured_host_endpoint` resolves `--host` through `zz_daemon_client::resolve_fleet_host`, which
the raw TUI shares: a `host-NAME` line wins, otherwise the value is an ssh destination, and a value
starting with `-` is refused. `local` and `all` are reserved. `--host all` re-runs the CLI once per
configured host in parallel for a chain whose every command passes `!spec.mutates` (`inspect`,
`list-hosts`, `show-last-output`, `protocol-version` and `--version` count as read-only). The parent checks the
chain once first, so an unknown verb or a usage error prints a single time with its own exit
status, and `events`, `wait-pane`, `fleet` and `tools` are refused with what they do and
`run it per host with --host NAME`. Each child gets `ZZ_SSH_BATCH=1`, so every ssh it starts runs with
`BatchMode=yes`, no security-key identities and a 3 s connect timeout, and on unix it also runs in its own session with
no controlling terminal; a host that needs a password, a touch or a host-key answer fails with the
hint below. Text lines get a `NAME: ` prefix, JSON lines under
`--json` get `host_name` first (`from_host` when the row has its own `host_name`, as `list-hosts`
rows do), and the exit is 1 when any host failed. Bare `zz --host NAME` runs `new-session -A`
there.

Over `--host`, `untargeted_host_input` exits 2 for any command without `-t` whose spec mutates and
whose `-t` names a pane, window or session (input verbs, `kill-*`, `respawn-*`, `split-window`,
`pipe-pane`, `html`, `set-option -p`/`-w` and the rest), since nothing there is the caller's; the
message asks for `-t %N`, `-t @N` or `-t NAME` by that kind. `join-pane`, `move-pane`, `break-pane`,
`move-window` and `swap-window` also need `-s`, because their source defaults to the current pane or
window. `new-window`, `attach`, `run-shell`, `if-shell`, `source-file` and `-g` forms stay allowed:
their `-t` is a destination or only a format context. A chain stops being checked at `new-session`
or at `new-window` without `-d`, since the commands after it land in what it made. The check reads
`--name=value` as the daemon does, and when the arguments do not parse it refuses unless a `-t`
comes before `--`.

`host_connect_failure` turns endpoint errors into the fix, from the cold connect and from the first
exec that verifies a kept forward alike (the chain, `--kill-server`, `events`, `-C`, `--progress`,
the host-local passthrough and the raw TUI): `zz fleet install NAME` for a missing zz,
`--restart-daemon` for a host on an older protocol, "update zz on this machine" for a newer one,
`ssh DEST` once or `zz fleet sync` for a refused login, `ssh DEST` once for an untrusted host key,
else `can't reach NAME: <reason>`, with `; known hosts: a, b` when NAME has no host line. DEST is
the login zz tried (`user@box`, or `ssh://user@box:PORT` with a port). OpenSSH folds every failure
into exit 255, so login and host-key refusals are read from the reason the endpoint built from
ssh's stderr.

`zz tools` in every form is answered by the CLI from the guide text in `zz-daemon`, so it needs no
daemon and never prints an older daemon's guide. `just tools skill` writes
`.agents/skills/zz-workspace/SKILL.md` from `zz_cli tools --skill`.
