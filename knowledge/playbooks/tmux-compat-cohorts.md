---
type: Playbook
title: Running tmux compatibility cohorts
description: Select a bounded compatibility change, coordinate ownership and close it against the pinned behavior.
resource: compat/orchestration/HANDOFF.md
tags: [tmux, compatibility, campaign, workflow]
timestamp: 2026-11-24T00:00:00-03:00
---

# Select the work

Start with the workflow someone needs under the [tmux command contract](/tmux/tmux-compat.md) and name an observable acceptance check. Find the matching group and its dependencies in `compat/tmux-gaps.json`. Reserve the specific items the change will close. New findings get their own record and proof so a batch can finish.

| Need | Record or command |
| --- | --- |
| Current gaps, decisions and closures | `compat/tmux-gaps.json` and the [generated report](/tmux/gaps.md). |
| Original agreed scope | `python3 compat/progress.py` compares current items with `compat/progress-baseline.json`. It counts additions after the freeze separately. |
| Claims and integration status | `python3 compat/board.py status` replays the comments of the dispatch issue. `zones` lists ownership paths. |
| Previous batches, gate recovery and machine handoff | `compat/orchestration/HANDOFF.md`, `CAMPAIGN-LOG.md` and the cycle runners in that directory. |
| Work at the 3.8 pin | `python3 compat/catchup/ledger.py status`, `ready` and `show <id>`. `compat/catchup/README.md` gives the resume procedure and records the 2026-10-10 close-out. |
| Attached-screen obligations | `compat/tui/campaign.json` and the [TUI playbook](/playbooks/tui-parity-campaign.md). |

# Coordinate a batch

1. Identify the pinned tmux behavior, put the reproducible observation and expected result in the registry and choose a proof surface. Use the [harness playbook](/playbooks/compat-harness.md) for command, config and layout differences.
2. On the dispatch board, set `ZZ_BOARD_HOLDER` to a stable machine/session identity. Run `board.py pick` and claim the front before editing. Give each worker a worktree at the recorded base and enough paths to implement its obligation. The board rejects overlapping zone claims. Hold TRIAGE to define fronts and use `residual` for discoveries outside the claim.
3. The worker measures the difference on isolated sockets, implements the agreed change and reruns the focused comparison. Include the base, tip, command, pin, exact observed result, changed registry items and any unproved clauses in the handoff. Keep a still-open item when a fixture or dependency covers only part of the contract.
4. An independent reviewer checks each item against the pin, reruns the proof at the candidate revision and verifies the comparison can detect a one-sided failure. A source inventory entry establishes spelling or shape. State changes, output, screen rendering and GUI interaction each require a check that observes them.
5. Serialize integration under MAIN. Reconcile registry edits by stable ID, then regenerate the report. Rerun affected checks after rebasing. The board's `integrated`, `repair` and `rejected` transitions require the MAIN holder. Record the result and release the lease with a reason.

# Validation ladder

Start with the smallest named scenario and its log, then run the affected crate tests and the delta corpus for the commands touched. `just compat check` validates the inventory and ledger. For a full close-out run `just compat run --strict-geometry --attached-client` and inspect the stored summary, including retries, known tuples and the attached-client stamp. A GUI-specific finding needs a GUI observation.

Close an `adopt` item only when its acceptance clauses pass at the integrated revision. For an intentional difference, record the decision, the concrete behavior users see, and a reproducer. The original frozen-scope percentage and the raw-TUI proof count measure different things. Use the individual records when deciding whether a user's workflow is covered.
