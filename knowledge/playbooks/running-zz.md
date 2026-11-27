---
type: Playbook
title: Building and running zz
description: Launch an isolated desktop or thin client, exercise a native browser pane and collect logs and profiles.
resource: scripts/run.sh
tags: [running, cargo, cef-download, daemon, browser-fixture, profiling, instruments]
timestamp: 2026-11-27T00:00:00Z
---

## Launch

Use just 1.52 or newer. `just --list --list-submodules` lists the full command tree. For
a first desktop build install the [native prerequisites](/playbooks/prerequisites.md).

| Target | Command | Behavior |
| --- | --- | --- |
| macOS dev | `just run mac` | Builds and launches `dist/zz-dev/zz Dev.app`. |
| Linux dev | `just run linux` | Builds the GUI and CLI, launches `zz-dev app` from the Cargo debug output. |
| Dev rebuild loop | `just watch mac` or `just watch linux` | Replaces the client after a successful build. A compatible daemon retains the sessions. |
| Linux hot reload | `just hot linux` | Subsecond via Dioxus CLI 0.7.10. Needs `ld.lld` and a staged Linux CEF runtime (`just run linux` or `CEF_PATH`). |
| Installed macOS/Linux GUI | `zz app` | Connects to the existing daemon or starts one. A new first session uses the shell's directory, a Dock/Finder launch uses home. |
| Linux raw Cargo GUI | `cargo run -p zz -- app` | Runs from Cargo's build tree. For dev isolation use the dev recipe. |
| Windows desktop | From native PowerShell, `./scripts/build-windows.ps1`, then `.\dist\zz\zz.exe` | The script selects the x64 Visual Studio tools and calls `just build windows` with Git Bash available. |
| Raw terminal | `zz`, `zz attach -t NAME`, `zz new -s NAME` | Bare `zz` runs `new-session -A` and can create session 0. Explicit attach requires an existing session. |

`just run` and `just watch` accept `--verbose` and `--features LIST`.
For release bundle paths, packaging and local signing, see [building a CEF bundle](/playbooks/build-cef-bundle.md).
On macOS `just install mac` quits the installed GUI, replaces `/Applications/zz.app`,
reopens it and attempts to put its `cli` launcher on PATH. A running daemon keeps serving
the previous build until it restarts. `zz update --check` also reports the packaged and
owned headless copies. Set `ZZ_OWNED_COPY=0` to disable handover when checking a packaged binary.

## Daemon and dev isolation

`scripts/run.sh` sets `ZZ_DEV_BUILD=1` at compile time and clears inherited socket, pane,
tmux and startup context. The dev identity uses `zz-dev` for config, socket, browser
profile, journal and preferences. Copy individual settings into the dev config if needed.
The normal Linux dev socket is `$XDG_RUNTIME_DIR/zz-dev/default.sock`; the temporary-directory
fallback is `zz-dev-$USER/default.sock`. On Unix set `ZZ_SOCKET` or use `-S PATH` for an
explicit endpoint. Keep test sockets directly under `/tmp` to fit the Unix socket path limit.
`TMUX` supplies pane metadata. Jobs also get the selected zz socket and a private `tmux`
launcher on PATH.

Dev launchers link `~/.local/bin/zz-dev` to the headless dev CLI. They refuse to replace
a regular file there. Run the dev recipe on a remote dev host before connecting.
From inside an installed pane, `env -u ZZ_SOCKET ~/.local/bin/zz-dev list-sessions`
queries the local dev daemon. To test a daemon change, restart the dev daemon only after
its sessions can be stopped. Restarting the GUI alone leaves a compatible daemon intact.

## Thin clients

| Client | Commands | Endpoint |
| --- | --- | --- |
| Web | `just web setup`, then `just web run` | Dev gateway on loopback port 8081. [Gateway, host setup and tests](/playbooks/browser-client.md) |
| iOS | `just ios rig`, then `just ios run [iPhone\|iPad]` | `scripts/ios.sh` checks `ZZ_ALA_ENDPOINT`, `ZZ_DEV_SOCKET`, then `ZZ_SOCKET` before probing the rig and dev sockets. `ZZ_ALA_DEMO=terminal` runs the terminal example. |
| Android | `just android setup`, `just android rig`, then `just android run` | Uses `ZZ_ALA_ENDPOINT` when set, otherwise maps the rig through `adb reverse` to `@zz-rig`. |

The iOS group also has `build`, `device`, `bench`, `testflight` and `rig stop`.
Android's `emulator`, `build`, `device`, `logs` and `rig stop` are in
`scripts/just/android.just`. Both mobile rigs use `scripts/ios-rig.sh`.
Device pairing, signing and endpoint options are in `clients/ios/README.md` and
`clients/android/README.md`.

## First-build CEF download

Set a shared cache before the first build to avoid downloading the same distribution in
each worktree:

```sh
export CEF_PATH="$HOME/.cache/cef"
```

The downloader verifies the minimal archive against CEF's published SHA-1, extracts it,
and the build compiles the wrapper. CI keys the cache by CEF version, OS and architecture.
[CEF pin](/references/cef-artifacts.md) · [Updating CEF](/playbooks/updating-cef.md)

## Exercising the browser pane: the loopback fixture

In one terminal, start the loopback server. In a pane of the running desktop instance,
open a browser split:

```sh
cargo run -p zz --bin zz_browser_fixture
```

```sh
zz split-window -h --kind browser http://127.0.0.1:9324/
```

Check the red/green/blue stripes, typed text, changed title, navigation, scroll target
and persistent cookie/count after a reload. The page also links a popup fixture.
`crates/zz/src/bin/zz_browser_fixture.rs` documents the optional port, popup and IME
regressions, and Linux shared-texture probe.

Use one override at a time when diagnosing rendering. `crates/zz/src/browser/environment.rs`
parses them and `controller.rs` sets the timing and fallback policy.

| Setting | Effect |
| --- | --- |
| `ZZ_BROWSER_FPS=1..240` | Override the fastest attached display's ceiling (60 FPS fallback). An unfocused visible pane normally caps at 30 FPS, with a one-second scroll boost. |
| `ZZ_BROWSER_SHARED_TEXTURE=0` | Force readback while keeping GPU acceleration. A visible Linux shared-texture pane with no first frame for two seconds falls back independently. |
| `ZZ_BROWSER_GPU=0` | Force software rendering and readback. |
| `ZZ_BROWSER_EXTERNAL_BEGIN_FRAME` | On macOS `0` restores the internal timer. Elsewhere `1` opts into external pacing. |
| `ZZ_BROWSER_BF_ADAPTIVE=1` | Enable experimental delivery-based throttling on the external frame clock. |

## Logs and live diagnostics

`just run` writes to the repo's ignored `logs/` directory. `ZZ_LOG_DIR` overrides direct
launches. Installed defaults are `~/Library/Logs/zz` on macOS, `$XDG_STATE_HOME/zz/logs`
(or `~/.local/state/zz/logs`) on Linux and `%LOCALAPPDATA%\zz\logs` on Windows.
The GUI and daemon write `zz.app.log` and `zz.daemon.log` with an 8 MiB live file and one
rotated generation. CEF warnings go to `cef.log`. Add `--verbose` to record terminal I/O,
browser URLs/input and process environment for a reproduction.

Mark an incident with `zz debug-marker "freeze during scroll"` or Command-Shift-M
(Control-Shift-M on Linux/Windows). Search for `user_marker`, `main_thread_stall` and
`main_thread_stall_recovered`. The stall watchdog threshold is 500 ms. On macOS the
marker or watchdog can capture two seconds of all-thread stacks in `zz.stall-*.sample.txt`,
at most once per minute, keeping eight. For crashes, pair the ring logs with the
`.ips` report in `~/Library/Logs/DiagnosticReports`. To resolve a CEF frame, take its
`imageOffset` and use `atos -o PATH_TO_CEF_DWARF -offset OFFSET` with the matching
`cef_binary_<version>_<platform>_release_symbols.tar.bz2`. Take the exact version
from `third_party/cef/ARTIFACTS.md`.

## Glass lab

Launch `ZZ_GLASS_LAB=1 just run mac` or open **Glass lab** from the dev app's palette.
Tune the active mode, switch mode for the other look, then use **Copy** to export the shown
values. Per-row reset restores that value, **Reset** restores both looks. Changes persist
in `glass-lab.toml` in the dev data directory. The panel takes clicks without taking key
focus on macOS. The implementation and file format are in `crates/zz/src/window/glass_lab.rs`.

## Profiling the macOS bundle

Build once with `just profile build mac`. The profiling app has release optimization
and full DWARF. Each capture starts it on a short private socket, records the owned process
tree and saves the result under `target/profiles/<run>/`. Keep window size, display scale,
visibility, pane geometry and workload matched. The private socket isolates daemon sessions.
The ordinary profiling bundle still shares GUI preferences and browser roots with installed zz.

| Question | Capture | Readout |
| --- | --- | --- |
| Which code uses CPU? | `just profile cpu mac gui 20s` (also `daemon`, `all`) | `just profile summary cpu RUN_DIR` filters to owned PIDs. |
| Where do threads wait? | `just profile system mac 20s` | System Trace in the run directory. |
| Which GPU work reaches the display? | `just profile metal mac 20s` | `just profile summary metal RUN_DIR` attributes command buffers and presents to the GUI. |
| When is the first frame displayed? | `just profile startup mac 8s` | `startup-summary.json` reports the first displayed surface attributable to the launched GUI PID, when available. |
| Why are rows repainted? | `just profile terminal mac 20s` | `just profile summary terminal RUN_DIR` reads row-cache and prepaint diagnostics. |
| How much memory is charged? | `just profile memory mac 60s` | `memory.csv` and `memory-summary.txt` give footprint and RSS every 250 ms. |

`ZZ_PROFILE_APP=/Applications/zz.app` works for startup and memory without dSYMs.
For existing processes, `python3 scripts/sample-macos-memory.py --pid GUI_PID --daemon-pid DAEMON_PID --duration 60s --output RUN_DIR`
leaves them running. Startup launches under Instruments with no warmup. Attached captures
and memory sampling wait for daemon readiness and a five-second default warmup
(`ZZ_PROFILE_WARMUP_SECONDS`). For steady-state browser work allow time for helper startup.
Keep verbose and terminal diagnostic logging out of performance comparisons. A nonzero
exit with `cleanup-escalations.txt` identifies a capture that required forced shutdown.

**Recorded macOS baselines, 2026-09-05.** Three uninstrumented pairs reduced median first
workspace render from 169 to 123 ms. Two alternating Metal launch pairs measured about
225 and 231 ms to first displayed frame. Holding the handshake for 1.5 s still allowed the
new workspace to render at 123 ms. In a 1→9→1 terminal sweep, Ghostty page mappings
returned from 18 to 2 and live libc allocations returned to 8.52 MiB. Dirty libc pages
settled at 21.7 MiB versus 9.34 MiB initially and repeat cycles plateaued. Check allocator
tags before attributing memory to GPU work: that build's mimalloc tag 100 appeared as
`IOAccelerator` in vmmap.

## Linux allocator memory

The desktop and CLI enable mimalloc's `no_thp` feature. The Linux CLI also calls
`PR_SET_THP_DISABLE`, which child programs inherit. The GNU/Linux GUI schedules
`malloc_trim(0)` on its background executor every ten seconds. The daemon defaults its
mimalloc purge delay to zero unless `MIMALLOC_PURGE_DELAY` is set. See `start_heap_trim`
in `crates/zz/src/lib.rs` and `purge_freed_memory_promptly` in `crates/zz-cli/src/startup.rs`.

Reproduce the retention check by closing 63 of 64 terminals, then measure idle footprint
and churn CPU separately. The recorded comparison was 199.9 versus 76.7 MiB with a zero
purge delay and about 2.6% more daemon CPU on 96-pane churn. A separate 2026-09-22 dhat
profile traced 20 MiB to tray pixmap serialization.

## Checks

`scripts/ci.sh` lists the stages for each platform. `cargo ci-test` runs all features in
the root workspace. The browser client's separate checks are in
[running the browser client](/playbooks/browser-client.md#checks). For ala-cef's
renderer-independent domain tests, run
`cargo test --manifest-path libs/ala-cef/Cargo.toml --no-default-features`.
