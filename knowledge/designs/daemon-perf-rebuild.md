---
type: Design Plan
title: "Daemon performance rebuild"
description: "The completed 2026 daemon performance campaign, its measured exit and the original assignments used by the brief generator."
status: "Waves 0 through 4 completed 2026-10-04"
resource: crates/zz-daemon/src/daemon.rs
tags:
- performance
- daemon
- protocol
- tmux
- benchmark
- campaign
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

# Campaign status

The five waves closed on 2026-10-04. The daemon batches publication, expands formats on demand and serves command invocations without a full interactive attach. Terminal frames can travel from PTY shards to subscribed clients. A separate daemon executable was tried and dropped. The proposed removal of the mux mutex was deferred.

# Baseline

The tracked wave-zero runs are `bench/perf/results/baseline-{macbook,alienware}-17e17115.json`. The strict final runs are `wave4-alienware-3057f095-{1,2,3}.json` and `wave4-macbook-3057f095-r2-{1,2,3}.json` in the same directory. Each JSON includes the executable checksum, reference executable, machine, metric samples and verdicts. The campaign judged each ratio by its median across three runs on each host. Individual runs contain failed rows.

# Targets

`python3 bench/perf/run.py --targets` prints the thresholds from `bench/perf/thresholds.json`. Run `just perf gate final --strict` to measure a current release build. The fixtures, isolation, result schema and noise policy live in `bench/perf/README.md`.

# Architecture

The mux event loop is in `crates/zz-daemon/src/daemon/`. PTY scheduling and pane event handling are in `crates/zz-terminal/src/session/{shard,pane_actor}.rs`. See [terminal lanes](/protocol/terminal-lanes.md) for frame delivery and [wire protocol](/protocol/wire-protocol.md) for compatibility rules.

# Historical lane briefs

The following sections retain the original work assignments for the campaign's brief generator. Their proposed APIs, rollout switches and checks describe the 2026 campaign. The delivery notes, later fixes and open follow-ups are recorded in `bench/perf/campaign/HANDOFF.md`. The complete pre-compaction plan, including per-lane measurements and omitted follow-up notes, is available with `git show 4fa976578:knowledge/designs/daemon-perf-rebuild.md`.


## W0-GATE: permanent zz-vs-tmux gate (effort M)

Built in `bench/perf/` (`run.py`, `gate.py`, `isolate.py`, `probe.py`, `timing.py`, `ptyclient.py`,
`sockproxy.py`, `fixtures.py`, `groups/`, `pane/`, `thresholds.json`, `test_gate.py`), Python 3
stdlib + ctypes, headless. `bench/perf/README.md` is the reference for flags, probes, ids and the
noise policy; this section records what the lanes rely on.

- `just perf gate [stage] [args]` builds `zz_cli` and runs `run.py --zz target/release/zz_cli
  --stage <stage>`. Flags: `--json`, `--baseline <previous merge>`, `--w0 <W0 JSON>` (found
  automatically), `--quick`, `--only`, `--strict`, `--keep`, `--rescore`, `--targets`.
- **tmux is a release build**: without `--tmux` the gate tries `/opt/homebrew/bin/tmux`,
  `/usr/local/bin/tmux`, `/usr/bin/tmux`, then PATH, resolves symlinks, and refuses scripts, the
  zz binary, any `-V` containing `-zz` (a zz pane puts the zz tmux wrapper first on PATH), and
  AddressSanitizer builds (`otool -L` / `ldd`, `__asan_init` via `nm`). The pinned `compat/.cache`
  tmux is an ASan debug build of next-3.8 and stays for compat behaviour only. `meta` records the
  resolved path, version, linked libraries, and the zz binary's sha256 and mtime.
- Isolation: fresh HOME / XDG_* / ZZ_DATA_DIR / ZZ_LOG_DIR, `ZZ_SOCKET=/tmp/zzpf-<pid>.sock`,
  `ZZ_TRAY=0`, `ZZ_UPDATE_CHECK=0`, `SHELL=/bin/bash` with no rc files; TMUX, TMUX_PANE, ZZ_PANE,
  ZZ_SESSION unset; groups run with that HOME as the working directory, so panes start there
  wherever the gate is launched from; both servers start with `-f <root>/perf.conf` holding
  `set -g history-limit 10000` (zz's default; tmux defaults to 2000), checked per group; both
  servers and every descendant killed after each group and on exit; zz and tmux runs alternate.
- Probes: CPU (macOS `RUSAGE_INFO_V4` user+system x timebase; Linux `clock_getcpuclockid` in ns,
  falling back to `/proc/<pid>/task/*/schedstat`), instructions (macOS `ri_instructions`, for every
  CPU metric; Linux user-space `perf_event_open` counters when available), footprint (`ri_phys_footprint`; Linux
  `Pss_Anon + Pss_Shmem + SwapPss`) and RSS as info, threads (`PROC_PIDTASKINFO`; `/proc` Threads), thread spawns (`PROC_PIDLISTTHREADIDS` sampled
  every ~2 ms; Linux task ids), wakeups (`ri_interrupt_wkups + ri_pkg_idle_wkups`; ctxt switches).
- `sockproxy.py`: Unix-socket relay counting bytes, u32-prefixed frames and connections (zz only;
  tmux passes the tty fd).
- Groups: `cli` (verbs at p1 and p20, list verbs at 20 sessions), `spawn`, `cold` (+ infocmp
  forks), `config`, `chatty` (steady, flip, hidden, visible with TUI CPU and tty bytes), `idle`,
  `mem` (p1, p20, tui20, scroll180, scroll80 and first copy-mode entry at 10k x 180), `attach` (ttfc, total tty bytes until quiet, CPU,
  wire), `echo` (latency idle and busy30, wire bytes), `throughput` (detached ASCII and unicode,
  attached), `control` (latency, burst rate, %output), `statusjob` (`#()` jobs).
- Thresholds per metric per stage in `thresholds.json`, the single source of the Targets table
  (`run.py --targets` prints it). Hard: CPU, bytes, footprint, threads, counts, throughput. Wall
  time fails on a quiet host (1-minute load per CPU <= 0.5 at start and end) and with `--strict`,
  and warns otherwise. Regressions against the previous merge and W0 follow the `tolerance` block.
  `report_only` names the metrics that never gate. Every metric records median, p10, p90, p99,
  min, max and n.
- JSON schema 2: `meta` (git_sha, zz_bin, zz_sha256, zz_mtime, zz_version, tmux_bin, tmux_version,
  tmux_libs, os, arch, ncpu, loadavg_start/end, noisy, stage, w0, history_limit, cpu_source,
  warnings, started_at), `metrics[]` (id, group, unit, kind, zz, tmux, ratio, vs_baseline, vs_w0,
  threshold, checks, verdict), `summary`.

Not built yet (TODO, also listed in `bench/perf/README.md`):

- TODO: the GUI-like mode of `crates/zz-client/examples/perf_client.rs`: all sessions, history
  backfill and 8 visible panes, with its `gui` group and attached footprint. W2-TERM built the
  frame-sink client and `throughput.headless.ascii_ms`; the latter needs a reference baseline.
- Built in W2-COPY: first copy-mode entry on a 10k x 180 pane, included in `mem` with tmux twins
  and wave2/final footprint, CPU and wall limits.
- TODO: injected RTT in `sockproxy.py` and the report-only `remote` group; the report-only `agent`
  group (fixture ACP provider).
- Done on 2026-09-29: the gate runs on Linux. Its W0 is
  `bench/perf/results/baseline-alienware-e9a5ad99.json` and its quick twin.

Gate (met at e9a5ad99 on macOS, see Baseline): `--stage baseline` passes; `--stage wave1` and
`--stage final` fail on today's binary; full run <= 12 min (about 7), `--quick` <= 3 min (about 2.5); the
committed W0 JSONs are the Baseline table below; `test_gate.py` covers threshold evaluation, the
noise policy, the regression rules, the tmux choice and that every gated metric has a final rule.
Linux baselines and the wave-1 exit are recorded in `bench/perf/campaign/HANDOFF.md`.

## W1-FOOTPRINT: process facts, local time, allocator (effort M)

Scope:
1. `terminal_current_command`: `terminal.foreground_process_id()` then one `KERN_PROCARGS2`
   sysctl and the exec-path basename, which is what sysinfo 0.39.6 returns today. Not
   `pbsi_comm`: a `claude` symlink to `versions/2.1.99` reads `2.1.99` there (measured), which
   breaks the default `@agent-progress-commands claude` match in `synchronize_pane_progress` and
   renames windows to the version. Linux: `/proc/<pgrp>/stat` comm, what sysinfo returns today
   (tmux uses `/proc/<pgrp>/cmdline` argv0; not adopted). Record the divergence from tmux
   `osdep_get_name` (osdep-darwin.c:36-51, pbsi_comm at 48-49) in the tmux-drop-in ledger.
2. `terminal_working_directory`: `PROC_PIDVNODEPATHINFO`; Linux `/proc/<pid>/cwd`.
3. `lifecycle.rs`: `IdentityRecord::current` start time via `proc_pidinfo` `PROC_PIDTBSDINFO` or
   `/proc/<pid>/stat`; liveness via `kill(pid, 0)`; remaining sysinfo helpers. `client.rs`
   `short_device_name`: `gethostname`. `zz-cli/src/diagnostics.rs`. Drop sysinfo from zz-daemon
   and zz-cli manifests.
4. `crates/zz-mux/src/localtime.rs`: `localtime_r` -> `tm_gmtoff` -> `DateTime<FixedOffset>`,
   offset cached 1 s. Replace `chrono::Local` in status.rs (`render` now, `DaemonFormatHooks`,
   `command_with_optional_variables`, strftime helpers) and formats.rs (`CommandHooks::strftime`,
   `format_time_value`, `pretty_time`, `relative_time`, `format_datetime`). Removes the
   `/etc/localtime` re-read per new thread (3.5% of the split loop).
5. CoreFoundation: `cargo tree -p zz-cli -i core-foundation-sys` shows chrono -> iana-time-zone as
   the only path, but xtask `build_linux_binaries` and `build_macos_binaries` build
   `-p zz -p zz-cli` in one invocation, and ala, zlog and crates/zz (`agent/view.rs` uses
   `Local`) enable chrono's clock, so features unify back. CF leaves `zz_cli` only if `zz_cli`
   builds in its own cargo invocation (xtask, deb, pacman, headless recipes) or links with
   `-Wl,-dead_strip_dylibs` for that target alone. Do it only if the dual build measures a gain;
   spawn is already equal to tmux, so no wall saving is claimed.
6. `purge_freed_memory_promptly`: mimalloc `os_tag` 241; purge delay chosen by the memory gate
   (start at 10 ms). `libghostty-vt-sys` `zig_optimize_mode`: ReleaseSafe only at OPT_LEVEL 0.

Write zone: workspace/zz-daemon/zz-cli/zz-mux `Cargo.toml` (sysinfo, chrono entries); daemon.rs
`terminal_working_directory`, `terminal_current_command`; lifecycle.rs sysinfo helpers and
`IdentityRecord::current` (not `write_identity_atomically`); client.rs `short_device_name`;
status.rs time sites; formats.rs time functions and chrono import; new `localtime.rs`;
`diagnostics.rs`; zz-cli `purge_freed_memory_promptly`; xtask build functions if item 5 goes
ahead; `zig_optimize_mode`.

Gate vs W0: `chatty.cpu_pct.flip` >= 15% lower; `mem.footprint.p1` >= 0.4 MB
lower; `mem.footprint.p20` no worse after the purge change; `cli.wall.version.p1` <= tmux + 0.3
ms; if item 5 lands, `otool -L` of the bundled binary in `dist/` lists only libSystem (+ libiconv)
and dyld image count is recorded (tmux 87, today 704). Tests: symlinked-binary fixture, compat
`pane_current_command` / `pane_current_path` rows, `compat/tui-choosers.sh`, lifecycle identity
and incompatible-daemon tests, agent peer tests, strftime tests, `cargo check` for Windows.

Expected: process lookup 37.5 us -> microseconds per burst and no longer grows with system
process count; madvise 5-7% of per-command CPU and 6.6% of config replay.

## W1-PUBLISH: change-driven publication (effort M)

Scope:
1. `KeyTables` generation from a process-global `AtomicU64` (whole-object replacement counts),
   bumped in `bind`, `unbind`, `update_binding_metadata`, `remove_table`, `ensure_table` (insert
   only), `set_prefix`, `set_prefix2`. Command name resolved at bind time so `snapshot()` does no
   catalog or prefix resolution. `publish_key_tables_if_changed` keeps the cheap reset pass and
   snapshots/compares only when the generation moved.
2. `refresh_status_filtered`: filter subscribers first; return before `set_format_now`,
   `state.snapshot()`, `format_hook_facts`, `format_option_snapshot` when none match.
3. Split `publish_mux_snapshots`: bookkeeping that always runs (`window_latest_clients.retain`,
   `release_chooser_zooms`, `terminal_appearance_updates` -> `set_appearance`, and
   `last_published_mux_generation`), then snapshot build and `stamp_snapshot_for_client` only when
   `subscribers` is non-empty.
4. `set_pane_runtime_facts_with_hooks` (zz-mux command.rs) bumps the tree generation only when a
   field of the unstamped snapshot changes (the automatic window name); a separate runtime-facts
   counter serves daemon readers. On a runtime-facts change, re-stamp each subscriber's labels
   (`expand_window_status_labels`, `stamp_pane_border_chrome`, `stamp_pane_border_colours`) and
   send only when the stamped bytes differ from the last send to that client (per-client hash).
   Optional shortcut: skip the re-stamp when no window-status, border or status template
   references runtime facts (scan cached per options generation). Re-stamp also on the
   status-interval tick. No "every bump changed the body" assert.
5. `refresh_automatic_window_name_for_pane`: `NAME_INTERVAL` 500 ms throttle with a Rename
   deadline for the remainder.
6. `synchronize_pane_runtime`, `synchronize_pane_title`, `rename_window_from_pane`: `mark_dirty`
   + leading-edge flush + at most one trailing flush per 16 ms (PublishFlush deadline). The
   command path still publishes at the end of each command.
7. One `zz-daemon-timers` thread with one deadline heap replaces
   `start_display_panes_deadline_dispatcher`, `start_key_table_deadline_dispatcher`,
   `start_silence_deadline_dispatcher`, `start_client_message_deadline_dispatcher`; schedule APIs
   and channel types unchanged. **The timer thread only posts expirations**: hook and command work
   (`expire_window_silence` -> `run_event_hooks` alert-silence, which can run a synchronous
   `run-shell`) runs on a worker or the command path, so a hook cannot stall repeat-time, message
   clears, display-panes, Rename or PublishFlush.
8. `start_status_sampler`: skip the tick when no status-interval client, control subscription or
   format monitor exists. `sync_claude_peer_states` runs only while some registry record's pid is
   in a pane's process tree, and detects change by per-file mtime (N stats per tick) or kqueue
   `EVFILT_VNODE` / inotify on the directory and files. Directory mtime alone misses in-place
   status rewrites.

Keep signatures of `publish_snapshot`, `publish_snapshot_state`, `publish_key_tables_if_changed`.

Write zone: zz-protocol key.rs `KeyTables`; zz-mux model.rs generation helpers; command.rs
`set_pane_runtime_facts_with_hooks`, `refresh_automatic_window_name_for_pane`; daemon.rs
`initialize_with_mux_config_files`, the four `start_*_deadline_dispatcher`,
`expire_window_silence` call path, `refresh_status_filtered`, `start_status_sampler`,
`sync_claude_peer_states`, `publish_key_tables_if_changed`, `synchronize_pane_title`,
`rename_window_from_pane`, `synchronize_pane_runtime`, `publish_snapshot`,
`publish_snapshot_state`, `publish_mux_snapshots`; new `daemon/timers.rs`; agent/claude_peers.rs
scan entry; `compat/wire-version.py` `NOT_WIRE`.

Gate vs FOOTPRINT JSON: `config.wall.source_1000` <= 90 ms; `chatty.cpu_pct.flip` <= 8%;
`cli.cpu.display` >= 11% lower; fixed service threads 9 -> 6; 4-pane 300-line TUI workload <= 12
Snapshot and <= 10 StatusChanged per 3 s (today 244 and 55). Tests: key-table tests,
`reload_user_config_resets_key_tables_to_the_file`, status / display-panes / key-table / silence /
client-message timing tests (re-run alone on failure), hook order, runtime-fact label rows, detached
window-style, timer-stall, in-place peer status. Compat: `compat/run.sh` (automatic-rename,
pane_current_command, hooks), `compat/status-row.sh`, `compat/tui-indicators.sh`,
`compat/tui-choosers.sh`.

Expected: key tables 11-21% of every command and 59% of config replay; status/snapshots with no
subscriber 52-53% with 10 panes printing; config 272 -> ~70 ms alone; flip CPU 24% -> 6-8%.

## W1-FORMAT: lazy universe, option index (effort M)

Scope:
1. `tmux_options.rs`: `LazyLock` index `HashMap<&'static str, {TmuxOption, is_consumer}>` from
   `tmux_option_names()` and aliases; `exact_tmux_option`, `exact_tmux_option_name`,
   `build_tmux_option` read it; `parse_format_option` does one probe;
   `extend_format_option_values` iterates a per-scope consumer list.
2. **Universe as `Built(Arc<FormatUniverse>) | Deferred(&'e MuxEngine)`**, deferred only inside a
   lock scope. Parts (loop items, option rows per scope, environment + window user options) fill
   on first read. `StatusContext.format_universe` changes type from the owned Arc. `detach()`
   builds the parts a template scan needs at **every escape point**. Any part not built and read
   off-lock renders empty with no error, which is the bug this rule prevents. No existing lazy
   path to reuse: `FormatContextSnapshot` builds on the first `status_context()` call.
   As built, the engine borrow is the `'e` of `StatusContext<'e>`, so the compiler finds every
   escape: only the status request and the mode requests leave the lock. Border presentations,
   chooser rows, control subscriptions, format monitors and hook bodies expand inside it on one
   deferred universe per client per call.
3. `expand_format_inner`, `expand_format_with_hooks`: return input unchanged when it has no `#`
   (and no `%` when time expansion is on); lazy `option_fallback`.
4. `list_keys`, `list_commands` resolve once; `list_sessions`, `list_windows`, `list_panes`
   (incl. `-f`, `--json`) share one lazy universe per command.
5. `build_status_context`: no `WindowOptions` clone in `window_knobs`; known `WindowId` for
   history-limit and synchronize-panes lookups; layout dumps once per window per batch.
6. `match_value`: 64-entry regex cache keyed by pattern and case flag.
7. daemon.rs: `pane_format_geometry` helper for `mouse_pane_cell`, `mouse_format_variables`,
   `popup_position_variables`, `client_viewport_facts`. `client_feature_mask` /
   `client_colour_count_with` cache terminfo values per client **keyed on TERM, client features,
   `terminal-features` and `terminal-overrides`** (or an options generation).
   **`format_hook_facts` keeps terminal handles**: `inner.terminals` becomes an `Arc<BTreeMap>`
   updated copy-on-write, and facts take a refcount. `border_presentations`, `mode_request`,
   `refresh_control_subscriptions`, `run_format_monitors`, chooser `client_chooser_rows` share one
   lazy universe per client per call.
8. `seed_session_environment`, `apply_client_environment_update`: compile update-environment
   patterns once per pattern-set change.

This lane owns `status.rs` facts readers and `daemon/chooser_presentation.rs` in wave 1. Keep
signatures of `status_request`, `format_hook_facts`, `format_hook_facts_for_client`,
`client_format_facts`.

Write zone: formats.rs (except the time functions FOOTPRINT already changed); tmux_options.rs
lookup functions + index; command.rs `parse_format_option`, `StatusRowVariables::lookup`,
`format_option_snapshot`, `extend_format_option_values`, `format_option_value`,
`format_option_target`, `window_knobs`, `window_size`, `history_limit_for_pane`, `list_*`,
`format_option_rows`, `seed_session_environment`, `apply_client_environment_update`; daemon.rs
`refresh_control_subscriptions`, `run_format_monitors`, `client_feature_mask`,
`client_colour_count_with`, `client_format_facts`, `mouse_format_variables`,
`client_viewport_facts`, `status_request`, `border_presentations`, `mode_request`,
`popup_position_variables`, `mouse_pane_cell`, `format_hook_facts`, `format_hook_facts_for_client`,
`ServerState.terminals` type; status.rs facts readers; chooser_presentation.rs.

Gate vs PUBLISH JSON: `cli.*.list_keys.*` wall <= tmux at p1 and p20 with CPU <= 3 / 5 ms;
`cli.wall.list_panes_a.s20` <= tmux; `cli.cpu.list_windows_a.s20`, `cli.cpu.list_sessions.s20` CPU
<= 1.2x; `config.wall.source_1000` <= 25 ms. Tests: zz-mux format tests incl.
`snapshot_contexts_share_universe_and_match_independent_formats`,
`name_checks_and_nested_scope_loops_use_the_engine_universe`, the 198-entry vocabulary pin;
index-vs-scan equality for every name and alias; lazy-vs-eager differential over direct expansion
**and every escape point** (1 pane, 20 panes, several sessions with marked, zoomed and dead panes);
terminal-overrides refresh; `compat_manifest_tests.rs`, `format_modifier_client_loop.rs`,
`compat/run.sh`, `compat/tui-mouse.sh`, `compat/tui-pane-geometry.sh`.

Expected: universe 95% of list-keys CPU; list-keys 17 -> ~1.6 ms (1 pane), 78 -> ~4 ms (20);
list-* linear, not quadratic; option lookups 470 of 569 `format_option_snapshot` samples; regex
52 of 344 in the 20-pane status render.

## W1-PANE: frames for watchers, compression, threads, spawn (effort L)

Scope:
1. No streamed view: `publish_active_views` builds no fallback snapshot, refreshes only the
   metadata of the stored latest viewport (title, OSC 7 cwd, status, size, modes, mouse, cursor,
   progress), keeps the old cell plane. `latest_viewport()` keeps its signature.
2. `Command::SetViewStream(view, Foreground|Preview|Off)` from `refresh_terminal_visibility`.
   **The streamed set includes Preview for every pane shown in a choose-tree/choose-client
   preview, display-panes, popup, GUI sidebar preview, and the pane a `#{C:}` template reads**
   (the active pane of each window in an attached status line), so `chooser_presentation.rs`,
   status.rs `pane_search` and the popup first frame read fresh cells without a round trip. A full
   frame is published when a view starts streaming. Copy-mode facts keep updating for
   non-streamed views in copy mode. `watch_terminal`, `publish_terminal_for_pane` skip diffs for
   non-streamed views.
3. `TerminalSession::fresh_viewport()` (actor round trip) exists for first frames and tests only.
   **Forbidden while holding `inner` or the status mutex**; debug-assert through a thread-local
   marker set around command execution and status render.
4. Last view deactivates: drop `RenderState`, row/cell iterators, cell pools; recreate on
   activation.
5. Idle compression in `run_terminal`'s wait block and `output_view_worker`: `compress_due` 1 s
   after `compression_activity()` changes, then `Terminal::compress(Incremental)` in <= 2 ms steps;
   re-arm after `capture-pane -S -`, HistoryChunk, `ModeRevision::capture`, search snapshots;
   `Unsupported` is a no-op. Time one page restore with the probe and record it.
   **Line-count pruning stays the only history limit**: `set_scrollback_max_bytes` (today `None`
   in `new_terminal`) is set only as a backstop at 4x the estimate history-limit x cols x 10 B,
   capped at 256 MiB.
6. Threads: `SearchWorker` spawned on first search. Child exit via kqueue `EVFILT_PROC NOTE_EXIT`
   in `wait_for_wake` (Linux: pidfd in the poll set, thread fallback before 5.3). **Register
   before the first wait; on `ESRCH` go straight to `waitpid(WNOHANG)`; remove portable_pty's
   `Child::wait` path so exactly one owner reaps.** The dead notice still follows the final output
   drain. Linux `zz-pty-gather` stays.
7. Spawn: `openpty` + `ptsname_r` / `TIOCPTYGNAME`, not portable_pty's `ttyname_r` /dev scan
   (690 vs 8.5 us). Word separators, allow-passthrough, wrap-search go into `TerminalSpawn` and a
   settings slot, not blocking sends on the capacity-1 command channel
   (`MAX_PENDING_ACTOR_COMMANDS = 1`). View setup uses `try_send` with a coalescing slot so
   `DeferredTerminalCommand::run` never parks the command thread.
8. Identity: pid and tty published through a one-shot Condvar after spawn; empty and surface
   sessions resolve immediately; `wait_for_terminal_identity` waits on it (2 s cap kept).
9. Views: `TerminalSession` records views the actor has seen; `release_view` / `detach_view` for
   an unknown view send nothing.
10. `is_current_terminal` reads a per-pane epoch without `inner`.
11. `shell_integration.rs`: `resource_root` in a `OnceLock` keyed by cache root, content-hash
    directory, no `sync_all`. `ActorWake::notify` does not log ERROR while the actor terminates.
12. **Echo fast path**: in `run_terminal`, publish at once, bypassing `CONTENT_PUBLISH_STALENESS`
    (16 ms), when PTY input was written in the last ~50 ms.

Write zone: zz-terminal session.rs `PublishedViewports`, `TerminalSession::spawn*`, identity fns,
`latest_viewport*`, view API, `Command`, `command_channel`, `CommandSender`, `ActorWake::notify`,
`new_terminal`, `run_terminal` (not the retained-pane tail), activate/deactivate/release view actor
fns, `SearchWorker`, `wait_for_wake`, `publish_active_views`, `snapshot`, `output_view_worker`;
shell_integration.rs `configure_default_shell`, `resource_root`, `materialize_resources`,
`write_resource`; daemon.rs `send_full`, `watch_terminal`, `is_current_terminal`,
`refresh_terminal_visibility`, `publish_terminal_for_pane`, `DeferredTerminalCommand::run`,
`wait_for_terminal_identity`. Not chooser_presentation.rs or status.rs (FORMAT owns them).

Gate vs FORMAT JSON: `spawn.wall.split_empty_P` <= 6 ms; `spawn.wall.split_shell` <= 6.5 ms;
`chatty.cpu_pct.steady` <= 3%; `mem.footprint.scroll180` / `scroll80` <= 1.0x / 1.15x tmux;
`mem.threads.p20` <= 51; `mem.footprint.p20` <= 45 MB; `echo.p99.busy30` <= 1.5x
tmux; throughput rows; `attach.ttfc.p1` no worse. Tests: zz-terminal history, capture, copy-mode,
snapshot tests; daemon exit / remain-on-exit / `-P` / pane_pid / pane_tty / word-separators /
allow-passthrough / wrap-search tests; exec client that ran copy-mode and capture-pane leaves no
view; styled wide lines fill history-limit exactly; `split-window 'true'` exit status; chooser
preview of a hidden printing pane is current. Compat: `compat/run.sh`, `tui-copy-mode.sh`,
`tui-screen-diff.sh`, `tui-output-backpressure.sh`, `tui-pane-geometry.sh`, `tui-choosers.sh`;
`bench/run.sh`.

Expected: snapshot builds 71% of detached-pane CPU; RenderState 0.72 MB per 180x50 pane;
`ttyname_r` 0.86 ms per spawn; 2.6 ms command-thread block per split; 2 s identity poll; threads 4
-> 2 per pane; pages 288 MB -> 13-24 MB for 20x12k lines.

## W1-EXEC: one-frame commands, fast cold start (effort L)

Scope:
1. zz-protocol message.rs (append only, version stays 107): `Exec(ExecRequest)` with
   `protocol_version` first, flags (UTF8, HAS_TTY, STDIN_AVAILABLE, NESTED, READ_ONLY),
   `client_instance_id`, origin pane, cwd, tty, size, TERM, features, `startup_reentry`,
   `spawned_server_id`, **`expect_server_id: Option<u64>`**, environment as one NUL-separated blob
   parsed lazily, `commands: Vec<CommandInvocation>`. `ExecOutput{stream, bytes}`.
   `ExecExit{code, server_id, resume: Option<ExecResume>}`; `ExecResume` carries exactly the
   attach/TUI classification `PreparedCommandList` carries today plus the unexecuted tail.
   Codec arms and round-trip tests in terminal_codec.rs. `PrepareCommandList` stays.
2. `handle_connection`'s first read: **envelope version check first** (the existing
   VersionMismatch branch), **then** dispatch `ClientHello` to today's path unchanged and `Exec`
   to `serve_exec`.
3. **No executor pool in wave 1: one thread per Exec connection.** That thread reads the Exec,
   runs it, writes, and reads the next frame. **An Exec connection accepts repeated Exec frames,
   one `ExecExit` each, and closes on client EOF.** Before running, the daemon checks
   `expect_server_id` and answers a mismatch with an `ExecExit` error. **A zero-command Exec is the
   liveness/mismatch probe.** Parking inside a command (hooks, `source-file`, `run-shell`,
   `if-shell`, `wait-for`, pane waits: 16 `report_command_queue_park` call sites, some reached
   only dynamically) blocks only that connection's thread; on the first park or
   `ClientFileRequest` from the connection, spawn today's reader so stdin, file responses and
   client EOF keep flowing. No static `may_park` flag.
4. **Startup**: a non-reentry Exec that arrives before startup completes is parked in a pending
   list (no thread held) and resumed by `finish_startup`; `startup_reentry` Execs (config job
   children) run at once. Test: two concurrent cold CLI commands against a config whose
   `run-shell` child calls the CLI; also `source-file` of a TPM-style config and an
   `after-select-pane` run-shell hook that calls the CLI.
5. `register_exec` inserts only the client facts formats read (origin, tty, size, pid, cwd,
   utf8, features, environment). No `ServerHello`, key snapshot, option snapshot, mux snapshot,
   status render, `published_mux_options`, `client_status_rows`, or `warm_terminfo_entries`. The
   chain goes through `prepare_command_list_with_engine` and
   `execute_command_request_with_prepared` with <= 2 `inner.lock()` takes per command; alias
   freezing and error semantics unchanged. `unregister_exec` releases views (cheap after PANE),
   skips detach hook captures and `refresh_control_output_taps` for a client that never
   attached. `spawned_server_id` replaces the `__zz-cold-start-prepare-abort` pseudo-command.
6. `Shared::unregister`, `detach_with_event_hooks`: `refresh_control_output_taps` once;
   `detach_client_state` skips `MuxHookSnapshot`, focus probe and copy-mode captures for clients
   that never attached.
7. client.rs `CommandClient`: speaks Exec. API: `connect` opens the socket; `execute` sends one
   Exec; `server_id()` is lazy (from the last `ExecExit`, else a zero-command probe);
   `server_hello()` is removed from `CommandClient`. Users to update: crates/zz
   `config/settings/file_options.rs` `execute_file_command` (passes `expect_server_id`, runs two
   commands on one client); zz-cli `connect_command_client` and
   `connect_command_client_with_spawn_provenance` (provenance from `ExecExit.server_id` vs
   `spawned_server_id`), `stored_default_client_command`, the kill-server path; crates/zz
   `config/mod.rs` `local_command_client`; daemon, zz-client simulator and `agent_soak.rs` tests
   that run many commands per connection (still one client identity per connection).
   (`agent_soak.rs` was deleted on 2026-10-23 with the Agent pane.)
8. zz-cli lib.rs: one Exec per invocation; delete the prepare round trip and per-command
   `CommandRequest` loop (`prepare_cli_command_chain`, `execute_prepared_command`,
   `execute_command_chain`). `ExecResume` drives the attach/TUI decision. Cold start:
   `spawn_daemon` passes `--bootstrap-ready-fd N` (CLOEXEC before panes spawn); the daemon writes
   one byte right after `LocalTransport::bind`; the CLI blocks on it (6 s deadline) and connects
   once; delete the redundant failing connect in `connect_or_spawn_daemon_with_provenance`.
   Windows keeps a 0.5/1/2/5 ms backoff. Foreign-version replies keep
   `classify_local_connect_error`; same-version decode failure retries once over the legacy path.
9. lifecycle.rs `write_identity_atomically`: write + rename, no `sync_all` (3.8 ms
   `F_FULLFSYNC`).
10. `run_foreground_listener`: `claude_peers::sweep_stale_records` after readiness; prompt history
    loads lazily behind a `OnceLock` on first access (not "after readiness", which loses an early
    entry). The in-pane `tmux` wrapper installs lazily on first pane spawn, pinned to a
    daemon-owned clone of the executable (Protocol policy).

Write zone: message.rs (append), terminal_codec.rs control-lane arms; client.rs `CommandClient`,
`connect_stream_with_startup_owner` Command branch (not `short_device_name`); daemon.rs
`run_foreground_with_ready`, `run_foreground_listener`, `accept_connections`,
`install_tmux_shim`, `finish_startup`, `wait_for_startup`, `Shared::unregister`,
`execute_command_request`, `execute_command_request_with_prepared`, `prepare_command_request`,
`prepare_command_list_for_request`, `prepare_command_list_with_engine`, `detach`,
`detach_with_event_hooks`, `detach_client_state`, `handle_connection` first-frame dispatch,
`prepare_socket`, new `serve_exec` / `register_exec` / `unregister_exec`; lifecycle.rs
`write_identity_atomically`; zz-daemon lib.rs cold-start pseudo-command; zz-cli lib.rs bootstrap
args, command-mode chain, `spawn_daemon`, `detach_daemon_session`, connect fns,
`stored_default_client_command`; crates/zz `file_options.rs` `execute_file_command`,
`config/mod.rs` `local_command_client`.

Gate vs PANE JSON: `cli.cpu.display.p1` <= 0.25 ms, `.p20` <= 0.40 ms; `cli.wall.display.p1` <=
1.10x tmux; sockproxy: 1 frame up, 1 write down per single `display-message` (<= 6 KB up, <= 200 B
down); thread count back to baseline after 500 CLI commands; `cold.wall.new_session` <= 22 ms;
`spawn.cpu.kill_pane` <= 0.8 ms; throughput unchanged. Tests: workspace tests,
`crates/zz-daemon/tests/{clients,overlay,signal_shutdown,agent_soak}.rs` (`agent_soak.rs` deleted
2026-10-23), zz-cli cold-start
provenance, daemon-upgrade mismatch tests, the concurrency tests in item 4, repeated-Exec and
`expect_server_id` tests. Compat: `compat/run.sh`, `packaged-cli.sh`, `startup-diagnostics.sh`,
`tui-client-commands.sh`, `tui-command-streams.sh`, control-mode fixtures.

Expected: removes the hello build (47-51%), unregister wakeups (8%), thread churn (5-10%), 2 of 3
round trips, 14.6 ms of retry slack and 3.8 ms of fsync on cold start.

## W1-ATTACH: attach path and TUI paint (effort M)

Scope:
1. `send_resync_inner`: on a fresh attach, do not resend the Snapshot `Attached` carries; send
   overlay events only for overlays that exist. Error-path Resync and RequestFull unchanged.
2. **Duplicate-Full drop against the pending slot only**, in `enqueue_terminal_with` /
   `replace_terminal_with`. Never against `delivered_terminals`: clients wipe viewports on
   `Attached` and after a failed patch (zz-client core `viewports.clear()`, remove +
   `request_full`), and `delivered_terminals` is cleared only on cancel, suspend and close. If the
   gate shows delivered-side dedup is needed for the 69 KB, it requires clearing
   `delivered_terminals` in `send_attached`, `send_resync_inner`, on RequestFull/NeedsFull and on
   popup change, tagged with an attach epoch. `newer_terminal_delivered` keeps dropping only
   strictly older frames.
3. `attach`, `attach_with_event_hooks`, `attach_collect_event_hooks`, `send_attached`: for clients
   reporting a size (`client_size_fact`), apply it and the status reservation through the resize
   policy before streaming, so the first Full has the final geometry.
4. `write_outbound` / `OutboundMailbox::recv`: `writev` up to ~256 KB of **consecutive ready
   frames**. **The reliable lane stays strictly FIFO** (kitty image chunks must precede the frame
   that places them). If bulk must yield, it gets its own lane plus a rule that holds a terminal
   frame whose `kitty_placements` reference an image until that image's chunks are written. Keep
   the `take_preview_refresh` check after each frame.
5. zz-tui: drain queued core events, paint once; on SnapshotChanged repaint only chrome whose
   layout, window list or pane set changed; never paint an empty pane before its first frame;
   read extended-keys and focus-events without two `CommandClient` connections. (Not with a
   `CommandRequest` on the `InteractiveClient`: any command with output that an interactive
   client runs opens a command-output view over its session. As built, the hello carries them.)

Write zone: daemon.rs `newer_terminal_delivered`, `OutboundMailbox::enqueue_terminal_with`,
`replace_terminal_with`, `recv`, `send_attached`, `attach`, `attach_with_event_hooks`,
`attach_collect_event_hooks`, `send_resync`, `send_resync_inner`, `write_outbound`; zz-tui
app.rs attach setup, event loop, paint fns, `handle_core_event`; tty.rs option reads; lib.rs
preflight.

Gate vs EXEC JSON: `attach.tty_total.p1` <= 8 KB; `attach.ttfc.p1` <= 14 ms; `attach.conns.p4` <= 2;
`attach.wire_s2c.p4` <= 130 KB; `attach.cpu.*` <= 3x tmux; `chatty.tty_kibps.hidden` <= 2 KiB/s;
`echo.wire_bytes.idle` <= 1700 B. Tests: switch-client A -> B -> A with no output and re-attach
deliver a Full for every visible pane; RequestFull after a bad patch repaints; kitty image then
placement order under batched writes; attach-sequence daemon tests (updated only where they asserted
the duplicates); zz-tui tests; `python3 compat/tui/tracker.py check` and every compat/tui fixture
(`attached-client.sh`, `tui-overlays.sh`, `tui-launch-diff.sh`, `tui-screen-diff.sh`,
`tui-caps.sh`).

Expected per attach: 69 KB duplicate Full, a 2.2 KB Snapshot and 9 blank repaints (4 panes), one
72 KB wrong-size Full (new-session), two extra connections; 29 write syscalls -> ~2.

## W2-TERM: PaneFrame terminal lane (effort L)

Scope:
1. terminal_codec.rs `PaneFrame`: varint pane, per-pane stream sequence, base, generation; flags
   byte for metadata present (title, cwd, cursor, modes, scrollbar, search, size, dictionary
   append); rows as varint y, x0, then runs of (style id, attrs, length, UTF-8), trailing default
   blanks dropped, scroll-shift op. Full = base 0, all rows. CommandOutput, HistoryChunk and
   previews use the same rows. Delete the 8 B/cell Full and Patch encoders and decoders.
2. zz-terminal model.rs `diff_with_scratch`: changed column spans, not whole rows.
3. daemon.rs `publish_terminal_for_pane`: encode once per (pane, base), reuse for every subscriber
   at that base; per-pane stream sequence instead of `next_sequence`; `send_full`, `send_history`,
   `enqueue_terminal_viewport`, `enqueue_terminal_viewport_preview`, `replace_terminal_viewport`.
   Latest-wins and NeedsFull unchanged.
4. Clients decode into `PackedCell` planes: zz-client core apply/decode; zz-tui
   terminal_event.rs, render.rs; crates/zz mux terminal and HistoryChunk decode (keep
   `history_requests_pending` across tree changes); gpui-shared terminal pane.
5. `knowledge/protocol/terminal-lanes.md`.

Write zone: terminal_codec.rs terminal lane; message.rs `EventPayload` terminal variants;
model.rs diff and patch types; daemon.rs functions in item 3 and terminal sequence allocation;
zz-client core terminal fns; zz-tui terminal_event.rs, render.rs; crates/zz mux terminal decode;
gpui-shared terminal decode; terminal-lanes.md.

Gate vs wave-1 exit JSON: throughput rows; headless-client throughput (TODO in W0) >= 0.85x W0;
`echo.wire_bytes.idle` <= 64 B; terminal bytes in `attach.wire_s2c.p4` <= 15 KB; chatty CPU no
worse. Tests: codec round-trip and property tests (random grids, wide cells, graphemes, hyperlinks,
dictionary growth), zz-client, zz-tui, GUI mux client tests, `tui-screen-diff.sh`,
`tui-output-backpressure.sh`, `tui-copy-mode.sh`, `just web-build`, `just ios-ala iPad build`,
`bench/run.sh`.

Expected (re-encoded captures): patches 15x smaller, Full 10x, blank screens ~140x, echo ~30-40 B.

## W2-CTRL: control plane v2 (effort XL)

Scope:
- Wire (in 107): `Hello` with viewport, subscriptions {tree: None|Attached|All, status, options
  bitset, keys: None|Hash|Full, default pane stream}, optional Attach op, environment blob;
  `MuxOptionKey` appends ExtendedKeys, FocusEvents; `ServerHello` -> `Welcome {version,
  server_id, client_id, instance_id, caps u64}`; `Batch`, `TreeDelta{base, version, ops}` +
  `TreeSync` on base mismatch, `ClientView`, `KeyTablesChanged{hash, mouse bitset}`,
  `GetKeyTables`; in-place upgrade after `ExecExit{resume: Attach}`.
- **Key subscriptions**: GUI, iOS and web subscribe `keys=Full` and receive per-table deltas
  (bind-key touches one table); they read `prefix_bindings` on every input route. Hash-only (+
  mouse bitset) is for TUI. Command-only and control clients request no key state.
- **Control mode**: `%layout-change` and window notifications stay hook-driven (W2-HOOKS owns the
  source; `DEFERRED_CONTROL_NOTIFICATIONS` ordering after `%end` preserved). New: control lines go
  as one `ExecRequest` body over the interactive connection, answered by `CommandResponse` frames
  and one `ExecExit` (W1-EXEC built no `ExecOutput`), instead of `prepare_commands` + execute (1
  round trip per line).
- Daemon: `Shared::register` / `register_subscribed` build only `Welcome`; `send_attached` +
  `send_resync_inner` produce one `Batch` (tree, ClientView, status, options, each visible pane's
  Full at final size); `publish`, `publish_to_control_clients`, `enqueue_reliable`,
  `enqueue_encoded_reliable` encode once and push `Arc<[u8]>`; `publish_mux_snapshots` sends a
  per-scope TreeDelta and nothing on an empty diff; `stamp_snapshot_for_client` produces only a
  ClientView; status output joins the flush Batch; agent stream fanout encodes once.
- Clients: zz-daemon `InteractiveClient` (connect + attach in one round trip); zz-client core
  (Welcome, TreeDelta into `MuxSnapshot`, Batch as one event group, key tables by subscription);
  zz-tui lib.rs, app.rs (delete tty.rs option reads and the preflight); zz-cli `ExecResume`
  upgrade and control lines; crates/zz mux/client.rs, lib.rs, config/mod.rs caps,
  workspace/which_key.rs; gpui-shared connection.rs; zz-client-ffi hello and caps;
  web and iOS rebuild; `knowledge/protocol/wire-protocol.md`, `snapshots.md`.

Write zone: message.rs hello/Welcome/Snapshot/KeyTablesChanged/new messages/MuxOptionKey;
tree_delta.rs TreeDelta ops; daemon.rs functions above, `handle_connection` interactive branch,
`serve_exec` upgrade hand-off; client.rs `InteractiveClient`; zz-client core; zz-tui lib.rs,
app.rs, tty.rs; zz-cli lib.rs, control_mode.rs command submission (not notification building);
crates/zz mux/client.rs, lib.rs, config/mod.rs, workspace/which_key.rs, workspace/view.rs,
workspace/new_session.rs, command/palette.rs, config/settings/multiplexer.rs; gpui-shared
connection.rs, command_palette.rs; zz-client-ffi ffi.rs, settings.rs, settings/mobile.rs;
agent/fanout.rs encode path; protocol docs.

Gate vs TERM JSON: `attach.ttfc.p1` <= 1.1x tmux; `attach.conns.p4` = 1; `attach.cpu.*` <= 1.25x
tmux; `attach.wire_s2c.p4` <= 8 KB; bind-key <= 64 B for hash subscribers; rename in an unattached
session <= 100 B per client; `control.latency` <= 1.2x, `control.burst_cmds_per_s` >= 0.8x tmux;
`chatty.client_cpu_pct.visible` <= 3x + 1% (1% on the Mac where tmux reads zero),
`chatty.tty_kibps.visible` <= 1.5x; cli and throughput no
regression. Tests: zz-client simulator convergence, zz-client-ffi + C integration client, daemon
clients and overlay tests, `cargo test -p zz`, every compat/tui fixture, control-mode fixtures
(`%begin`/`%end`, `%layout-change`), `compat/run.sh`, `just web-build`, `just ios-ala iPad build`.

Expected: attach ~10 round trips -> 1; hello 28.3 KB -> ~40 B + subscribed state;
KeyTablesChanged 26 KB -> ~20 B for hash subscribers; tree change 20-60 B, encoded once;
attach CPU 26 -> <= 5 ms.

## W2-HOOKS: read-only skip, then change journal (effort L)

Two commits, the first mergeable alone.

1. `mutates` is **zz's own predicate over arguments**, not tmux `CMD_READONLY` (tmux.h:1999 is a
   read-only-client permission flag, set on attach-session, send-keys and others that mutate).
   `capture-pane` without `-p` (it writes a paste buffer, `-b` or a new one, as
   cmd-capture-pane.c does), `display-message -I`/`-d` count as mutating. Read-only commands skip
   `MuxHookSnapshot::capture`, `capture_pane_focus_probe`, copy-mode / active window / pane /
   bell / focused-window captures and the after-diff (debug-assert generation unchanged).
   **`after-<cmd>` hooks still fire for read-only commands** (pinned tmux `CMD_AFTERHOOK`,
   cmd-queue.c:653, covers list-keys, list-panes, show-options, capture-pane, display-message).
   `format_hook_facts_for_client` is built only when a hook body or format needs client facts.
2. zz-mux `MuxState.journal: Vec<Change>` pushed by every mutating method (SessionCreated,
   Renamed, Closed, WindowLinked, Unlinked, Renamed, LayoutChanged, ActivePaneChanged, PaneFocus,
   PaneTitle, PaneExited, OptionChanged{scope, name}, KeyTablesChanged, EnvironmentChanged,
   BellRaised, ClientSessionChanged). **Each Change carries the names, ids and indexes hook
   formats need, captured at emission** (hooks for closed sessions read what
   `MuxHookSnapshot` / `HookSessionState` hold today). Route every direct `get_mut` on the pub
   `sessions` / `windows` fields through methods; `drain_journal()`. Mutating commands and
   `synchronize_pane_runtime`, `synchronize_pane_title`, `close_exited_terminal`,
   `rename_window_from_pane` take hook events from the journal in tmux `notify_*` order. Then
   delete `MuxHookSnapshot` and the diff in `mux_hook_events`. This lane owns the source of
   control-mode notifications.

Write zone: catalog.rs `CommandSpec.mutates`; zz-mux model.rs journal and mutators; command.rs
mutation sites (not list or format fns); daemon.rs `MuxHookSnapshot`, `mux_hook_events`,
`execute_with_mux_source_inner`, `capture_pane_focus_probe`, `close_exited_terminal`,
`synchronize_pane_title`, `rename_window_from_pane`, `synchronize_pane_runtime`,
`format_hook_facts_for_client`.

Gate vs CTRL JSON: `config.wall.source_1000` <= 1.1x tmux; `cli.cpu.display.p20` 5-15% lower;
`spawn.cpu.kill_pane` <= 0.4 ms; oh-my-tmux cold server CPU <= 30 ms (tmux 20). Tests: hook-order
tests (`splice_pane_focus_events`, window-layout-changed order, client-detached on session loss),
after-list-keys and other read-only after-hooks, debug assert "generation moved implies journal
entry" over the whole daemon suite, control-mode `%begin`/`%end` ordering fixtures,
`compat/run.sh` hooks and source-file scenarios.

## W2-COPY: copy mode without flat clones (effort L)

Scope: replace `ModeRevision`'s flat clone (PackedCell + semantics + search text + 12 B offsets
per cell, 128 MiB cap) while keeping tmux clone semantics. tmux clones backing at entry and on
an enabled refresh (`window_copy_clone_screen` in window-copy.c). Source pruning and ED3 do not
change frozen content. Resize reflows the frozen backing, maps the logical cursor, clears
selection and recalculates existing search marks. The `clear-history` command exits copy mode;
ED3 leaves it open. Clone covered pages copy-on-write, keeping compressed pages compressed, and
render only visible rows per frame. `HistorySearchSnapshot` searches frozen rows lazily, joining
physical wraps into logical lines. An enabled refresh takes another page snapshot only when
there is unseen output and no selection; it retains tmux's follow and suppression rules.
Retained dead panes keep their compressed `Terminal` instead of flat `FrozenHistory`
(`publish_frozen_history`, the `run_terminal` retained-pane tail). Remove `ModeRevisionTooLarge`.

Write zone: `session/mode_revision.rs`, `session/copy_grid.rs`; session.rs `FrozenHistory`,
`publish_frozen_history`, `run_terminal` retained-pane tail, copy-mode refresh,
`HistorySearchSnapshot` and `copy_mode_snapshot`; the native page snapshot and safe row APIs.
The retained copy completion path also needs `Shared::watch_terminal`; see the as-built notes.

Gate against lane base `f341009d`: first copy-mode entry on a 10k x 180 pane is included in `mem`,
with bounds of 1 MiB incremental footprint and 5 ms daemon CPU and control reply wall time.
Throughput must remain unchanged. Tests: tmux differentials for copy mode under continuous
output past history-limit and resize in copy mode; `compat/tui-copy-mode.sh`, `compat/run.sh`
copy-mode rows, `tracker.py check`, terminal copy/search tests and daemon copy-session tests.

## W3-SHARDS: PTY shard threads inside zz-terminal (effort XL)

Scope, keeping the `TerminalSession` public API: K shard threads (K = min(available_parallelism,
4), `ZZ_PTY_SHARDS`, chosen by the gate) own PTY fds and terminals, never migrating; each polls
PTY fds, child exit (`EVFILT_PROC` / pidfd) and a wake fd; never `waitpid(-1)` (run-shell,
pipe-pane, ssh children are reaped elsewhere). The per-pane actor becomes a state machine
(`on_readable`, `on_command`, `on_deadline`) keeping 64 KiB reads, 256 KiB / 1 ms turn caps,
16 ms time-sampled frames and the echo fast path; `PTY_BRIDGE_SPIN_MAX` spins only while the pane
is its shard's only ready pane. One lazy search thread per daemon. Fold Linux `zz-pty-gather` only
if `bench/run.sh` on Linux matches. Own fork: allocation-free child with prebuilt argv/envp, then
setsid, `TIOCSCTTY`, dup2, closefrom, chdir, execve. One live frame per pane per publish shared
by all live views; per-view frames only for copy mode or scrollback. Windows ConPTY keeps a
reader thread per pane feeding its shard. Shard -> loop channels are unbounded or coalescing, so
a shard never blocks on the loop.

Write zone: session.rs spawn fns, `run_terminal`, `wait_for_wake`, wake-pipe drain, child wait,
pty gather, `SearchWorker`, `publish_active_views`, `output_view_worker`, view activation; new
`session/shard.rs`; shell_integration.rs spawn command building; portable-pty removal in
manifests.

Gate vs W2 exit JSON: zz-terminal threads = K; `mem.footprint.p20` <= 20 MB; one flood >= 4x tmux
and >= 0.85x W0, and 4 concurrent floods >= base aggregate; `echo.p99.busy30` p99 in an idle pane
while 4 floods run <= 1.5x tmux; `throughput.attached.ascii_ms` <= 1.18x W0. Tests: zz-terminal
tests, daemon exit / remain-on-exit / respawn / job control (Ctrl-Z, SIGWINCH, foreground pgid),
`compat/run.sh`, compat/tui fixtures, `bench/run.sh` on macOS and Linux.

## W3-LOOP: single-owner mux loop (effort XL)

- (e0) Move the per-request thread_locals into a per-cmdq-item context:
  `CLIENT_KEY_INJECTION_DEPTH`, `DEFERRED_CONTROL_NOTIFICATIONS`, `COMMAND_QUEUE_PARK`. List every
  synchronous actor round trip reachable from command or lock-holding paths (`capture_screen` ->
  `terminal.capture`, `fresh_viewport`, identity waits, search) in the PR; each becomes a
  continuation.
- (a) `BTreeMap<ClientId, Client>` replaces the ClientId-keyed maps in `ServerState`.
- (b) One `zz-mux` thread on mio: listener, nonblocking client sockets (per-client read buffer,
  `writev` on writable), Waker, signal self-pipe (signal-hook replaces async-signal, async-io,
  futures-lite, `DaemonSignalGuard`), timer heap absorbing the W1 timer thread and the status
  sampler. The status tick runs only while a shown status needs it; control subscriptions and
  format monitors get their own timers. Per-connection threads go away; the Mutex stays, taken
  only on the loop.
- (c) Pane watcher threads post `ViewportReady` and events through a channel + Waker.
- (d) Delete the Mutex; `inner.lock()` sites become `&mut`.
- (e) tmux-style cmdq with continuations for the 16 park points and sleep loops: wait-for,
  run-shell and if-shell jobs (pipes and child exit on the loop), confirm-before, command-prompt,
  menus, popups, pane `-W` waits, client stdin and file reads, `agent-send --wait`, identity waits.
  **`#()` status jobs join the loop**: job pipe and child exit registered with it; the
  `zz-status-job` reader thread and the terminate-on-drop thread in status.rs go away. A small
  helper pool only for items not yet converted, listed in the PR.

Write zone: all production code in daemon.rs (no other lane edits it in wave 3), `daemon/*.rs`,
transport.rs, status.rs, `agent/*` posting, zz-daemon `Cargo.toml`.

Gate vs SHARDS JSON: `cli.cpu.display` <= max(1.2x, tmux + 0.03 ms) at p1 and p20;
`spawn.cpu.split_shell` <= 1.2x; `spawn.cpu.kill_pane` <= tmux + 0.1 ms; `idle.wakeups_per_s.p20` <=
0.2/s with no client timers; fixed daemon threads = 1 loop + agent threads;
`statusjob.threads_per_s` <= 0.5 and `statusjob.cpu_pct` <= 1.5x + 0.1 pt; chatty no worse; 0
`lock_slow` samples under chatty; 20-pane `list-keys` and a 20-pane `capture-pane -S -` on large
history delay another pane's echo p99 no more than tmux does. Tests: workspace (flaky daemon tests
re-run alone), `compat/run.sh`, compat/tui, control-mode fixtures, `cargo check` Windows, full bench
at `wave3`.

## W4-DELIVER: frames straight from shards (effort L)

Scope: the loop pushes per-pane subscriber sinks (queue handle, stream kind, delivered base,
frozen flag) to the owning shard on subscription change. The shard encodes each `PaneFrame` once
per (pane, base) as `Arc<[u8]>`, enqueues on every matching sink (latest-wins, NeedsFull kept),
and wakes the loop only when a queue goes empty -> non-empty. Delete `watch_terminal`, the
`zz-pane-N` threads and per-frame lock traffic in `publish_terminal_for_pane`. Edge events post to
the loop (title, OSC 7 cwd, bell, exit, clipboard, OSC rename, copy results, kitty images,
placeholder binds, activity edge, foreground change, checked at most every 500 ms per pane with
output). `#{pane_current_command}` / `#{pane_current_path}` resolve at expansion through the W1
direct calls. Activity and silence use an atomic last-output timestamp and loop timers.
Control-mode `%output` becomes a shard-delivered view kind; delete `refresh_control_output_taps`,
`run_control_output_tap` and the `zz-control-output-N` threads. **Per-pane stream sequence
barriers**: every loop notification about a pane (`%exit`, `%window-close`, mode changes, pane
removal) carries "after seq N" and the client queue holds it until that pane's frames and
`%output` up to N are queued, so final output precedes exit (`control-mode.exit-pane-output`,
wire-protocol.md).

Write zone: daemon.rs `watch_terminal`, `close_exited_terminal`, `publish_terminal_for_pane`,
`synchronize_pane_runtime`, `refresh_control_output_taps`, `run_control_output_tap`,
`OutboundState` terminal slots, subscription handlers; session.rs sink API, `EventQueueState`,
shard publish path; formats.rs current command/path callbacks.

Gate `--stage final`: `mem.threads.p20` <= 12; `mem.footprint.p20` <= 12 MB, `.p1` <= 5 MB;
a control client adds 0 threads (today +23); `chatty.*` <= 1.2x tmux + 0.5 pt; 1 encode per frame
with 2 clients on a pane (counter); throughput rows. Tests: `compat/run.sh` (monitor-activity,
monitor-silence, automatic-rename, pane_current_command), compat/tui, control-mode `%output` and
exit-ordering fixtures, workspace tests.

## W2-FMT: tmux format.c model, no materialized context (effort L)

Reinstated under the no-compromise rule. W1-FORMAT makes the universe lazy but still resolves
through `StatusContext`, a ~100-field struct of owned strings built per item.

Scope: resolve through a tree of borrowed engine handles and typed client, session, window and
pane ids. Variable lookup goes options first, then the sorted static callback table, then
per-command variables, then the environment. Loops (`#{S:}`, `#{W:}`,
`#{P:}`) push a borrowed child tree. Parsed template operations are cached by source text;
they hold syntax, never option values. Options generation keys the option snapshots, needs
scan and indirect reference closure instead. Command facts borrow daemon maps and build
derived maps only on demand. Status and mode requests leave the engine lock through
`detach_with_references()`, which captures referenced values and reachable loop contexts.
Owned facts remain necessary for those requests. Unknown dynamic job output uses the full
capture path. The old `StatusValues` object is boxed and initialized only for explicit legacy
field access or rollback.

Write zone: formats.rs (all but the time helpers), the `StatusContext` users in status.rs and
command.rs `StatusRowVariables`, daemon.rs `format_hook_facts*`.

Gate `--stage wave2`: format code under 3% of samples in `cli.list_keys.p20`, status render and
`config.source_1000` profiles; `cli.cpu.list_keys.*` and `cli.cpu.list_*_a.s20` <= tmux CPU;
`chatty.cpu_pct.visible` improves on the W2-HOOKS merge. Tests: the format differential rows in
`compat/run.sh` (formats, formats-values, status rows, choosers) with exact output, plus a
property test that the compiled path and the W1 lazy path agree on every pinned format name.

Handed over from W1-FORMAT (see its As-built block): per-row template parsing is 25-30% of
`list-keys` p20; `list_windows` costs about 2.2 Minstr at s20 against tmux's 2.9 for the whole
command; the status template scan and `format_option_snapshot` run on every refresh because
nothing tells them options changed, so this lane keys the option and dependency caches by an
options generation; the per-command `FormatHookFacts` snapshot makes every `set @x` clone
that scope's user option map.

## W4-ROWS: bulk row extraction from libghostty (effort M)

Reinstated under the no-compromise rule. Frame build reads cells through per-cell calls into
libghostty-vt.

Scope: first profile `throughput.attached.*`, `chatty.cpu_pct.visible` and
`chatty.client_cpu_pct.visible` after W4-DELIVER. If row extraction shows in any of them, add a
fork API that copies a row range into packed cells in one call (styles and graphemes as dictionary
indexes), carried in the Ghostty fork pinned in
`third_party/rust/libghostty-vt-sys/build.rs` and the libghostty-rs fork pinned by `rev` in the
workspace `Cargo.toml`. If it does not show, record the profile in this doc and close the lane.

Gate: attached throughput and visible chatty CPU improve on the W4-DELIVER merge; both fork
commits recorded with a rebase note in `third_party/rust/libghostty-vt-sys/UPSTREAM.md`.

## W4-BINARY: daemon-only executable (closed 2026-10-02, not merged)

The plan: run the daemon from its own `zz-daemon` executable instead of the multi-role `zz_cli`,
on the guess that about 9.6 MB of daemon RSS was file-backed pages of CLI code it never runs.
Gate `--stage final`: `mem.rss.p1` <= 2x tmux, cold start not slower, remote start working.

Measured, the guess is wrong. A lane built the split (`perf/binary` `b08a3565`: a `[[bin]]` in
`crates/zz-daemon` whose graph is 171 crates against 218 and has no rustls, ureq, http,
`zz-tui`, `zz-config` or `zz-client`; `zz daemon` and cold start exec it with an in-process
fallback; a source-hash build id so a stale `target/debug/zz-daemon` never runs; packaging in
every recipe) and measured it on the M4 Max against the wave-3 binary:

| metric | `zz_cli` | `zz-daemon` | tmux |
| --- | ---: | ---: | ---: |
| `mem.rss.p1` MiB | 12.20 | 11.72 | 4.09 |
| `mem.footprint.p1` MiB | 4.70 | 4.66 | 2.69 |
| `mem.rss.p20` MiB | 19.14 | 18.66 | 4.22 |
| executable bytes | 18,153,120 | 16,580,624 | |
| idle resident text | 7216K of 16.7M | 6848K of 15.2M | |
| dyld images | 81 | 81 | |

The CLI-only code is 1.3 MB of `__text`. The rest of the 6.8 MB the idle daemon keeps resident
is daemon, mux and terminal code that startup touches, spread over about 45% of the 16 KB text
pages; `MADV_RANDOM`, `MADV_DONTNEED` and `MADV_FREE` on `__TEXT` change nothing. Cold start was
flat within noise and the daemon retired 1.4% fewer instructions to idle.

Decision: not merged. The split costs about 1,000 net lines across 39 files (two executables in
every package recipe, a build-id handshake, a Linux `/proc/<pid>/fd` pin for the in-pane `tmux`
wrapper) for 0.48 MiB of clean, evictable text pages, while `mem.footprint.*`, the gated memory
rows, already pass. `mem.rss.*` stays informational. If RSS ever matters, the lever is code
layout, not the split: an order file from an lldb trace (startup, one `new-session -d`, 5 s idle:
1448 functions, 1.94 MB over 390 pages) relinked with `-Wl,-order_file` idled at 8.3 MB against
12.1, and the quick mem gate read `mem.rss.p1` 8.64 MiB (2.10x tmux). It needs the list
regenerated by every release build (symbol names carry crate hashes that move with the version,
toolchain and lockfile) and a trace that covers the gate's mem phase.

## W4-CONTROL: control client stdio handoff (as built 2026-10-02)

A relay in front of `tmux -C` that does no work still costs 6.9 us and 47 kinstr per command, so
the lane took the stdio variant. `zz_cli -C` passes its stdin and stdout to the daemon with
SCM_RIGHTS (`ControlStdio`, `crates/zz-daemon/src/daemon/control_stdio.rs`). While the client is
idle and caught up, the daemon runs plain lines itself and writes `%begin`/`%end` and `%output`
to that stdout. Hooks, tree changes, errors, multi-step or waiting commands, blank lines and EOF
go back to the client, which renders them with its own code and returns the bytes in
`ControlWrite`; `ControlStdioSync` carries the next frame number across the switch. The client
keeps its own stdio with `ZZ_CONTROL_RELAY=1`, for a tty, file or named FIFO stdin (kqueue misses
FIFO EOF on macOS), on the thread connection path, and against a daemon whose `Welcome` lacks the
`control-stdio-v1` capability bit (an older protocol-107 daemon).

Review fixes: removing a connection never blocks on its stdout (it drops what is left, as a dead
relay client does); a client socket EOF closes the handed stdin and stdout and the outbound queue
at once instead of draining into a dead client's pipe; WRITABLE on stdout is dropped once a flush
drains; the client restores the flags of fds 0, 1 and 2 when it takes stdout back or exits, waits
for the close ack with no deadline while the socket lives, and hands stdout back before printing
an input error so stderr keeps its place. Control-mode return capture keeps a second Return or
EOF queued instead of dropping it, so `refresh-client -f wait-exit`, a blank line and EOF exits 0
in both modes. Tests: `control_stdio_tests.rs` and the cli_binary control section.

## W4-SPAWN: Linux panes start through a shared-memory clone (as built 2026-10-02)

`fork()` from the multithreaded daemon copied its page tables and write-protected every anonymous
page, so the shard thread then took about 250 copy-on-write faults per new pane on the pages it
touched next (thread start, terminal setup, the first publish); after the change it takes 4. On
Linux the pane child now starts with `clone(CLONE_VM | CLONE_VFORK)` (`spawn_shared` in
`crates/zz-terminal/src/session/unix_pty.rs`) and runs the same `exec_child` setup before exec.
The parent stays suspended until the child execs or exits, so the exec fence pipe and its poll
are gone on Linux. The child runs on its own 64 KiB mapping with a `PROT_NONE` guard page below
it, unmapped as soon as `clone` returns. The PTY master opens with `O_CLOEXEC` and the slave comes
from `TIOCGPTPEER`, with the path as fallback. Empty panes skip building a spawn environment they
never use. macOS keeps the `posix_spawn` helper, with `spawn_forked` as its fallback.

Pane programs inherit the daemon's transparent huge page setting: on Linux a zz pane starts with
THP disabled when the daemon disabled it, which `zz_cli` does at start (W1 Linux follow-up
above). tmux leaves THP alone; this is a recorded difference. Before this lane a pane got THP back
only from a daemon that had turned it off itself, and a daemon started by the CLI or over ssh
never does (it inherits the CLI's setting), so the common case is unchanged. The child can no
longer clear the flag: on a shared mm that gave the whole daemon a window with THP on, and with
20 panes in one command list the review saw 2048 kB of daemon `AnonHugePages` in 2 of 8 runs.

`ZZ_PTY_FORK=1`, read once at the first spawn, puts Linux back on `spawn_forked` and the exec
fence.

The shard thread now waits for the child's exec with no limit, where the fence wait gave up after
`PANE_EXEC_WAIT` (500 ms). An exec that stalls after the path checks pass (a binary on a hung FUSE
or network file system) holds every pane on that shard until it returns. glibc's `posix_spawn`
waits the same way; accepted.

Review fix: the first version took the child stack from mimalloc (`Vec::with_capacity`). The
freed 64 KiB block came back as a pane's PTY read buffer, so each live pane kept about 235 kB more
resident (`mem.footprint.p20` 6.3-6.8 -> 11.1 MiB on alienware); the stack is now its own
mapping.

Measured on alienware against perf/wave4 `b9e272a9a`, five alternating quick runs each
(`--only spawn,mem`, load 2-8). DL6 is in the base, so it no longer starts a reader thread per
pane. `spawn.cpu.split_shell` 1.47-2.11 -> 1.15-1.44 ms (median -29%, tmux 0.83-1.17),
`spawn.cpu.new_window` 1.33-1.95 -> 0.87-1.47 ms (-29%, tmux 0.64-1.04), `split_empty_P` -9%,
`kill_pane` unchanged. User instructions do not move (`spawn.instr.split_shell` 0.60-0.65 Minstr
on both), so the saving is kernel time. `mem.footprint.p1` 2.36 -> 2.37 MiB; `.p20` medians 7.57
-> 7.72 MiB (runs spread 7.2-8.7 on the base and 7.2-11.5 on the lane), 7.70 -> 7.63 MiB over
five more alternating `--only mem` pairs, and the review's footprint probe (20 default shells)
reads 5.23-5.24 MiB on the base and 5.24-5.25 on the lane. With 20 panes in one
command list, all 20 start with THP off and the daemon holds no `AnonHugePages`, in 8 of 8 runs and
in 3 of 3 with `ZZ_PTY_FORK=1`. On the Mac only the empty-pane environment skip reaches this code,
and three alternating runs show the spawn rows equal to the base.

## W4-TTYIN: the raw TUI hands its tty input to the daemon (as built 2026-10-04)

tmux's client gives the server its tty, so a key wakes the server, the pane and the server again
before the bench sees the echo. zz's relay client added a client wake and a socket hop on the way
in. `zz_cli attach` now passes its stdin to the daemon with SCM_RIGHTS after the hello
(`TtyInput`, behind the `tty-input-v1` Welcome bit and the same hello capability; protocol stays
107). The daemon reopens the terminal by name with its own nonblocking open file description, so
it never changes the flags of the shell's terminal, and reads it on the mux loop
(`crates/zz-daemon/src/daemon/tty_input.rs`). A key that means the same whatever follows it
(`zz_protocol::tty_input_key`: every 7-bit byte except ESC, a complete UTF-8 scalar that is not a
control, and a complete cursor, Home, End, Insert, Delete, page or function key sequence, bare or
with an xterm modifier; checked against zz-tui's parser for all 256 bytes, sampled scalars and
every candidate sequence) goes straight to the pane through the EM5 plain-key path while the
client has reported that it is caught up and that a plain key would reach that pane unchanged,
the daemon agrees that pane is the client's active pane, the connection has nothing queued and
the key tables pass it. Anything else (Alt keys, other escape sequences, mouse, paste, focus and
terminal replies, a sequence cut off at the end of a read, the prefix and bound keys, anything
while a daemon-side mode is open) goes back to the client in `TtyInputBytes`, which feeds it to
its existing parser. Once anything is forwarded, later bytes follow it until the client sends
`TtyInputReady` with the number of chunks it has processed and the pane, which it only does with
an empty event queue, an idle parser, no pending escape and no client-side route open
(`input::plain_key_pane`, `InputRouter::passes_plain_keys`); it reports no pane only to withdraw
one it reported for the same count. Output stays on the client. A kitty graphics probe makes the
parser busy: the client withdraws its pane before it writes the probe and reports it again once
the primary device attributes reply that fences the probe arrives, or after a second. Only in that
window does the parser read text starting with `Gi=` as a reply that a relay stripped of its
`ESC _`; outside it `Gi=` is typed text on both sides, so the daemon has no special case for it.
