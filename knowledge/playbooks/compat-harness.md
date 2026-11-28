---
type: Playbook
title: Running the tmux compatibility harness
description: Run a differential against the pinned tmux, interpret its five result channels and register an exception.
resource: compat/run.sh
tags: [tmux, compatibility, differential-testing, geometry, playbook]
timestamp: 2026-11-27T00:00:00-03:00
---

# Running the corpus

Run from the repository root with modern Bash on PATH. On macOS with Homebrew Bash installed, `export PATH="$(brew --prefix bash)/bin:$PATH"` also selects it for subprocesses that call `bash`. The harness builds `zz_cli` and fetches tmux 3.8 at `7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53`. It checks the source checkout, binary and build stamp. `ZZ_COMPAT_TMUX` can select an existing build from that recipe. `ZZ_COMPAT_ZZ` selects a prebuilt zz executable. A scenario marked `corpus: required` also needs the pinned plugin checkouts from `compat/fetch-corpus.sh` or its verified `ZZ_COMPAT_CORPUS` directory.

`compat/diff-scenario.sh` runs both sides under one locale whatever the caller's is: `LANG` and `LC_CTYPE` are `C.UTF-8`, or `en_US.UTF-8` where that is not installed, and time, numbers, messages, money and collation are `C`. Both sides therefore see a UTF-8 client and print English month names. A scenario that needs another locale sets it in its own fixture.

| What to check | Command |
| --- | --- |
| Available scenarios | `compat/run.sh --list` |
| One behavior | `compat/run.sh --strict-geometry windows panes` |
| Scenarios for a change, including config fixtures and command matches | `compat/run.sh --list --delta origin/main...HEAD --commands split-window,select-pane`. Drop `--list` to execute. |
| Source inventory, tracker and mux checks | `just compat check` |
| Freshness of the stored summary | `compat/run.sh --check-summary` |
| Full differential and real attached-client fixture | `just compat run --strict-geometry --attached-client` |

The full run writes `compat/results/summary.md` after the attached fixture passes and the summary validates. A named subset or headless-only run leaves the stored report in place. A failed row gets one isolated retry. A missing required plugin corpus shows up as a skip and makes the run incomplete. The freshness check compares scenario paths, step counts, result cells and the attached fixture's pass stamp without executing either binary.

# Reading results

Open `compat/results/<scenario>.log` for each side's exit status, stdout, stderr and per-step diff. Ordinary command lines compare success versus failure. Use explicit output assertions when the payload matters.

| Channel | What the runner compares |
| --- | --- |
| TOPO | Sessions, windows, panes, their names, indexes and active selections. |
| GEO | Cell dimensions and complete window layout, including pane IDs. `--strict-geometry` makes a difference fail an ordinary row. |
| FMT | Bytes printed by a shared `fmt:` query. Both sides must succeed. |
| OUT | Command stdout for `out:`, exact exit status and both streams for `err:`, and ordinary command output in the config fixture tier. |
| WARN | Each side's expected config diagnostics and the result of its `source-file` control block. |

Use `compat/attached-client.sh target/debug/zz_cli compat/.cache/tmux-src/tmux` for interactive input and attach behavior. `compat/startup-diagnostics.sh` takes the same two binary paths for cold-start config causes. On macOS, `just build mac` followed by `compat/packaged-cli.sh dist/zz/zz.app` checks the installed-style launcher under a path containing spaces. For the rendered terminal screen use the [TUI proof fixtures](/playbooks/tui-parity-campaign.md).

# Adding a scenario

1. Create `compat/scenarios/<name>.txt`, with one command per line. The runner starts isolated servers, creates session `w` on both sides, and takes topology and geometry readings after each step. Leave that session alive, name windows explicitly for stable names and target known indexes.
2. Use `fmt: #{pane_width}` for a format, `out: show-options -gv @name` for query output, or `err:` for a failing command's diagnostic. A `fmt:` payload is one argument. The other query payloads split on whitespace. These directives reject quotes, dollar signs, backticks and `#(`. Ordinary commands allow quoted words but reject shell metacharacters. Use `zz-only:` and `tmux-only:` for setup specific to one side.
3. For config or plugin behavior, use the existing `compat/scenarios/smoke/` tier. Declare `corpus: none` or `corpus: required`. Use `stage:` for files copied into the scratch home, `conf:` to source a fixture, `expect-warn:` to pin each side's config diagnostics, and `keys: <table> <key>` to compare one binding. See `compat/diff-scenario.sh` for the supported directives and path rules.
4. Run the new scenario by name and inspect the log. If it represents an accepted mismatch, move it to `known/`, give it one `# gap: <id>` metadata line and add its exact TOPO, GEO, FMT, OUT and WARN counts to `known_differentials` in `compat/tmux-gaps.json`. The runner checks both the counts and expected exit. When fixed, move the row back to the ordinary corpus and remove its registered tuple.

# Tracker and generated report

Register a newly measured gap in `compat/tmux-gaps.json` with a stable ID, individual affected items, owner, dependency IDs, decision, evidence and acceptance checks. Keep partial work under the same ID. On closure record the date, resolution and proof in `closed`. The source inventory is `compat/tmux-oracle.json`. `compat/check.sh` reconciles it with the pinned checkout and zz's command, format, option, key and hook inventories.

```sh
python3 compat/tmux-tracker.py write-report
python3 compat/tmux-tracker.py check
python3 compat/tmux-tracker.py known-tuple known/known-spread-mixed.txt
```

The [gap report](/tmux/gaps.md) is generated from the registry. The [divergence map](/tmux/divergences.md) explains the product decisions. For a different upstream commit, use the [pin update procedure](/playbooks/updating-tmux-reference.md).
