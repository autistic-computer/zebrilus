# September tmux compatibility campaign handoff

Cycle 18 integrated on Ubuntu on 2026-09-07, with records written after midnight UTC on
September 8. For the current registry, pin, corpus and exit criteria, use the [campaign tracker](../../TMUX_COMPAT_TRACKER.md).
The [TUI campaign](../tui/README.md) and [3.8 catch-up campaign](../catchup/README.md) have
their own ledgers, runners and close-outs.

## Cycle 18 close-out

The [cycle-18 runner](opus-compat-run-18.js) assigned two worktrees, one independent reviewer per
lane and a single integration gate. The client lane landed at `a4369c39`, the mux lane at
`808dcc7e`. Both used protocol 99. The gate changed the if-shell fixture in `05decabf` after the
pin itself produced more than one valid ordering. It recorded the full run in `5a15ae8e`.
[Cycle-18 integration log](CAMPAIGN-LOG.md#2026-09-07-cycle-18-integration)

| Evidence | Result at close |
| --- | --- |
| Frozen scope | 304/304 items in 65/65 groups. Of 46 active groups with 446 items, 3 were open, 0 blocked and 43 accepted. There were 199 closed records. |
| Client | Fixed the stdin stall under PTY backpressure. The writer discarded pending paint on detach. Below 109 columns the detach hint left the status row. The desktop clipboard mirror read the v99 producer field. |
| Mux | Matched missing-buffer errors per verb and applied background if-shell/run-shell insertions in queue order. Two feature tests racing to bind one key remained an accepted difference. |
| Full differential | 254 scenarios, 3,086 steps, no unexpected differences. The fifth attempt passed the attached fixture. SHA-256 `fe76abeafaa9a14fb4190dbd199e7c780ea82df14b9cad94888c0c74d9e758e6`. [Stored cycle-18 summary](https://github.com/autistic-computer/zebrilus/blob/5a15ae8eeb88f22a8912b14d4114292dcae7118d/compat/results/summary.md) |

The stored footer literally names `04d28cb19e73`. The corresponding fixture commit in the
rewritten history is `05decabfb90a`. Their `crates/` and `compat/` trees agree. The four known
rows below come from the [cycle-18 registry](https://github.com/autistic-computer/zebrilus/blob/5a15ae8eeb88f22a8912b14d4114292dcae7118d/compat/tmux-gaps.json).

| Scenario under `known/` | Differences |
| --- | --- |
| `known-main-preset-two-panes` | 1 GEO |
| `known-pane-scrollbar-columns` | 2 GEO, 1 OUT |
| `known-spread-mixed` | 1 GEO |
| `known-terminal-runtime` | 6 FMT |

Three items remained when the gate closed. Their subsequent dispositions are in the
[gap registry](../tmux-gaps.json).

- `desktop.overlay-consumers` recorded motion and wheel events reaching the pane beneath an open
  desktop menu. The later catch-up ledger deferred `later.menu-input`.
- `tui.client-output-queue-budget` recorded a 4 MiB paint queue. After 45 s with an unread terminal,
  the pin answered a key in 0.050 s and zz missed the 12 s deadline. The group closed on September 9
  with a [backpressure fixture](../tui-output-backpressure.sh).
- `tui.status-row` recorded missing per-client theme colors. The group closed on September 10 with
  indexed colors preserved and all 14 [status-row comparisons](../status-row.sh) asserted.

## Resuming on another machine

For a new board cycle, fetch the published base and inspect `git worktree list`, the current
[registry](../tmux-gaps.json) and issue #7 before choosing an item. Cycle 18's front names,
worktree locations, host settings and leases describe the September run.

1. Load the repo toolchain. Populate the pin with `compat/fetch-tmux.sh` and plugins with
   `compat/fetch-corpus.sh` once per cache. Share the resulting paths with the workers. From the
   repo root, run the formats scenario to check the setup.

   ```sh
   ZZ_COMPAT_TMUX="$PWD/compat/.cache/tmux-src/tmux" \
   ZZ_COMPAT_CORPUS="$PWD/compat/.cache/plugins" compat/run.sh --strict-geometry formats
   ```
2. Check GitHub access on the machine running the cycle with `gh api user` and
   `python3 compat/board.py status`. Supply a stable `ZZ_BOARD_HOLDER` for the run. Identify live
   user sessions and sockets before starting fixtures. Give tests their own HOME and config dir,
   keep PATH for tools such as nvim, and use per-side files when a scenario writes then reads
   through the [shared fixture HOME](../diff-scenario.sh).
3. The unattended September setup used [guard-rm-home.py](guard-rm-home.py) as a Bash PreToolUse
   hook with a 10 s timeout. Its [original setup and machine checklist](https://github.com/autistic-computer/zebrilus/blob/4fa9765780e2b17ffdc6db035e5806fef42e8c90/compat/orchestration/HANDOFF.md#resuming-on-another-machine)
   includes the hook reload and keeping the host awake through the gate.
4. Freeze disjoint work, claim the fronts, review each final tip and integrate serially under MAIN.
   Build the binary for the worktree under test before running fixtures. Keep reviewer probes on
   a branch so they survive a machine move. See the [board client](../board.py) and
   [cohort playbook](../../knowledge/playbooks/tmux-compat-cohorts.md).

### Board tool quirks

`--holder` precedes the subcommand. Use a lease with units, such as `--lease 6h`. Renewal counts
from the renewal comment and can shorten a lease. Cycle 18 claimed its two lane locks for 14h and
left MAIN for the gate. Overlapping zones block a second claim even by the same holder.

Use TRIAGE to define or withdraw fronts, MAIN to record integration, repair or rejection.
`--priority` takes an integer. `release` and `withdraw` require `--reason`. A records-only push
is ledgered with `integrated MAIN --merge <sha>`. The board's exact flags are available through
`python3 compat/board.py <command> --help`. Its [fold tests](../board_test.py) cover how comments
change the board.

## If the gate dies mid-cycle

1. Inspect `origin/main` and every gate worktree. Record which lanes landed and where the fix commits are.
2. Preserve the rebased tip as `campaign/<name>-gated` on origin before further runs or cleanup.
3. Verify the reviewer's required fixes in the code and registry at that tip. Re-run unfinished
   stages and any stage whose inputs changed. Re-run the reviewer's failing probe.
4. Complete the gate, update the ledger from the merged registry and release the locks. The
   [cycle log](CAMPAIGN-LOG.md) records the cycle-11 recovery. The [cycle-17 reports](cycle-17-reports.json)
   and [continuation runner](opus-compat-run-17b.js) show how a gate resumed on a different machine.

If a rebase conflicts in the registry, `gaps-merge.py BASE OURS THEIRS OUT` in this directory
merges by ID and exits 2 on a conflicting record. Resolve the disputed record, regenerate the gap
report and check the manifest counts. The [merge script](gaps-merge.py) defines the input order.

## Stamping a full run

From a clean checkpoint tree, run `just compat run --strict-geometry --attached-client`, then
`compat/run.sh --check-summary`. Cycle 18 budgeted 9,000 seconds for the full run after an earlier
48-minute timeout. The [harness](../run.sh) retries each failed scenario once alone after the
corpus and retries a failing attached fixture once. The summary records retries. A second failure
or a skip leaves the previous artifact in place.

The stored-summary check compares the inventory, step counts and registered channels with the
current corpus. It requires a PASS stamped at a clean ancestor with an unchanged attached fixture.
Later `crates/` commits produce a drift warning. A fixture or behavior claim at a later revision
needs proof from that revision. Cycle 18 also ran [status-row](../status-row.sh) and
[pane geometry](../tui-pane-geometry.sh) at the merged tip.

## Pages that carry live checkpoint numbers

Gate step 7 in the archived runners names this section. `tmux-tracker.py write-report` generates
[the gap report](../../knowledge/tmux/gaps.md) from the registry. At a new checkpoint, check the
[root tracker](../../TMUX_COMPAT_TRACKER.md) and any present-tense counts still quoted in these pages.

- [Compatibility contract](../../knowledge/tmux/tmux-compat.md), [status line](../../knowledge/tmux/status-line.md), [key tables](../../knowledge/tmux/key-tables.md), [copy mode](../../knowledge/tmux/copy-mode.md), [commands](../../knowledge/tmux/commands.md) and [divergences](../../knowledge/tmux/divergences.md).
- [Harness playbook](../../knowledge/playbooks/compat-harness.md) and [cohort playbook](../../knowledge/playbooks/tmux-compat-cohorts.md).

Dated measurements belong to their tested revision. The [original handoff](https://github.com/autistic-computer/zebrilus/blob/4fa9765780e2b17ffdc6db035e5806fef42e8c90/compat/orchestration/HANDOFF.md)
retains per-cycle launch and resume recipes, all measured thresholds, machine notes, historical
test failures and gate decisions. Read it offline with
`git show 4fa976578:compat/orchestration/HANDOFF.md`. The [cycle-13 review](CAMPAIGN-REVIEW.md)
cites [line 63 in the handoff it reviewed](https://github.com/autistic-computer/zebrilus/blob/3aa2aede56227d60642e7d2a4aca5642b8ad674e/compat/orchestration/HANDOFF.md#L63).
