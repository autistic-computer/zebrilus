# tmux compatibility campaign tracker

The September campaign delivered all 304 items frozen on 2026-08-31. Cycle 17 finished the frozen
scope. Cycle 18 integrated the client and mux work that followed. For work today, start with the
[gap registry](compat/tmux-gaps.json) and the [accepted differential summary](compat/results/summary.md).
The [September handoff](compat/orchestration/HANDOFF.md) records what was measured then.

## Current checkpoint

### Campaign dashboard

Snapshot at `1ce632dce`. Recompute from the linked inputs when resuming.

| Signal | Evidence |
| --- | --- |
| Campaign delivery | 304/304 frozen items, 65/65 groups, 15/15 park dispositions. [Frozen baseline](compat/progress-baseline.json) |
| Live unresolved | 2 open + 0 blocked, 7 post-freeze items. Desktop menu mouse capture accounts for one, byte-preserving option and command paths for six. [Gap records](compat/tmux-gaps.json) |
| Registry counts | The [generated report](knowledge/tmux/gaps.md) lists current item totals, open work, accepted groups and closed records. |
| Ledger settlement | 272/274 groups (99.3%). Each group weighs one. The numerator counts closed records plus accepted groups, the denominator counts closed records plus all active groups. |
| Latest differential | 282 scenarios, 3,966 steps, 10 registered known rows. The attached-client fixture passed at `18d5b94f066e`. Later commits changed `crates/`. [Stored results](compat/results/summary.md) |
| Differential SHA-256 | `fd92ee7e22c7c1d4116e1d9483b1250582e1a7a0ad712586ba9ef193c151ebd0` |
| Pinned tmux | 3.8, `7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53`. September used `d77c9dc6`. [Pin and source map](third_party/tmux-reference/UPSTREAM.md) |

The later [TUI parity campaign](compat/tui/README.md) recorded its own attached-client proof.
The [catch-up campaign](compat/catchup/README.md) moved the pin to 3.8 and records its merged
and deferred items in [its ledger](compat/catchup/ledger.json).

## Resume contract

1. Check the published base, worktrees, unrelated edits and existing claims. For the September
   workflow, [issue #7](https://github.com/autistic-computer/zebrilus/issues/7) owns claims and
   transitions. The [orchestration handoff](compat/orchestration/HANDOFF.md) covers machine setup,
   locks and recovery.
2. Read the group's items, decision, dependencies, evidence and acceptance clause in the registry.
   Measure zz and the pinned tmux before freezing one bounded change. When the registry and a
   narrative disagree, inspect the source, tests and measured behavior. The
   [knowledge index](knowledge/index.md) locates subsystem owners.
3. Run the preflight in a dedicated worktree from verified `origin/main`. Re-rank open and blocked
   items before selecting work. The progress script compares unresolved item slugs with the frozen
   baseline. New items are reported separately.

```sh
python3 compat/tmux-tracker.py check
compat/run.sh --check-summary
python3 compat/progress.py
just compat check
```

## Validation and closure gates

Run the focused tests and named differential or attached probe for the changed behavior first.
A closure needs an independent review of the final revision. Record a new finding in the registry,
including any evidence that invalidates an earlier acceptance. The [cohort playbook](knowledge/playbooks/tmux-compat-cohorts.md)
sets the slice and checkpoint gates. The [harness playbook](knowledge/playbooks/compat-harness.md)
covers scenario selection, individual channels and fixture operation.

For a normal September slice, the close included `cargo fmt --all -- --check`, workspace clippy
with all targets, all features and warnings denied, `cargo test --workspace --all-features`,
`bash -n` on changed shell fixtures, OKF validation and `git diff --check`. Record both the loaded
and exact-solo results when a daemon test shows the timing or headless behavior in [AGENTS.md](AGENTS.md).

When a checkpoint changes the accepted corpus or invalidates its proof, run the following on the
clean checkpoint tree. A skip, reduced inventory, interrupted run or headless run leaves the
previous accepted summary in place.

```sh
just compat run --strict-geometry --attached-client
compat/run.sh --check-summary
```

## Practical exit gate

Keep issue #7 open until all five conditions hold.

- Daily session, window, pane, config, plugin, Control and attached-TUI workflows have no known silent semantic mismatch.
- The config and pinned plugin corpus runs without unexpected skips.
- The strict differential and the attached-client fixture pass at full counts.
- The persisted summary matches the current corpus. The fixture stamp identifies the tested revision.
- Every remaining gap has a recorded `native`, `park` or `never` decision, or fails loudly without corrupting state.

Tmux spellings must preserve tmux behavior or fail loudly. Zz additions use zz-only commands.
Linked sessions, the private tmux socket protocol and multi-user socket ACLs fall outside this
campaign. See the [compatibility contract](knowledge/tmux/tmux-compat.md) and [deliberate divergences](knowledge/tmux/divergences.md).

## Updating this file

After a registry edit run `python3 compat/tmux-tracker.py write-report`, then
`python3 compat/tmux-tracker.py check`. The generator owns [gaps.md](knowledge/tmux/gaps.md).
For a new checkpoint record the frozen cohort, discoveries, change in unresolved work, exact
closure commits, artifact counts, known rows, stamp, digest and any incomplete proof. Record the
next bounded slice or pause. Update affected bundle pages and the [milestone log](knowledge/log.md).
The September gate published under MAIN and mirrored its result to issue #7 after the commit existed.

## Shipped history

| Period | Original evidence |
| --- | --- |
| Phases 0 through 8 | [Phase index](knowledge/designs/tmux-drop-in.md#phases) and [original phase-by-phase acceptance record](https://github.com/autistic-computer/zebrilus/blob/4fa9765780e2b17ffdc6db035e5806fef42e8c90/knowledge/designs/tmux-drop-in.md#phases). The [CLI audit](knowledge/research/2026-08-22-tmux-cli-compatibility-audit.md) gives the baseline. |
| Alert cohort through 10ag | [Milestones and commits](https://github.com/autistic-computer/zebrilus/blob/b402f90d893ee9ef2ca0a1cb7baa63558356b866/TMUX_COMPAT_TRACKER.md#shipped-history), [10ag proof and qualified test results](https://github.com/autistic-computer/zebrilus/blob/b402f90d893ee9ef2ca0a1cb7baa63558356b866/TMUX_COMPAT_TRACKER.md#persisted-acceptance-evidence-for-10ag), [closed registry history](knowledge/tmux/gaps.md#closed-history). |
| 10ah, 10ai and the parallel waves | [Original contracts, exclusions, evidence and queue](https://github.com/autistic-computer/zebrilus/blob/b402f90d893ee9ef2ca0a1cb7baa63558356b866/TMUX_COMPAT_TRACKER.md#candidate-queue-after-wave-2) and [superset roadmap](knowledge/designs/tmux-superset-roadmap.md). |
| Orchestrated cycles 1 through 18 | [Per-cycle log](compat/orchestration/CAMPAIGN-LOG.md), [review through cycle 13](compat/orchestration/CAMPAIGN-REVIEW.md) and [runners and reports](compat/orchestration/). |

### 2026-09-07 cycle-18 integration checkpoint

| Evidence | Result at the checkpoint |
| --- | --- |
| Merges and review | Client `a4369c39`, mux `808dcc7e`, then the fixture change `05decabf`. Both lanes integrated under protocol 99. |
| Ledger | 199 closed, 43 accepted, 3 open, 0 blocked. The three post-freeze items kept the practical exit gate open. |
| Differential | 254 scenarios, 3,086 steps, four registered known rows and an attached-client PASS. The [handoff](compat/orchestration/HANDOFF.md) lists the four rows, exact stamp and digest. |

### 2026-09-07 cycle-17 integration checkpoint

All four lanes integrated and the frozen meter reached 304/304. The recorded fixture PASS still
belonged to cycle 16. The [cycle-17 table](https://github.com/autistic-computer/zebrilus/blob/b402f90d893ee9ef2ca0a1cb7baa63558356b866/TMUX_COMPAT_TRACKER.md#2026-09-07-cycle-17-integration-checkpoint)
gives the platform qualifications and review findings. Read all earlier tables offline with
`git show b402f90d8:TMUX_COMPAT_TRACKER.md`.
