---
type: Playbook
title: Running the TUI parity campaign
description: Read or repeat a raw-TUI proof and record a new obligation against the correct tmux pin.
resource: compat/tui/campaign.json
tags: [tui, tmux, compatibility, campaign, playbook]
timestamp: 2026-11-24T00:00:00-03:00
---

# Read a proof

The campaign closed on 2026-09-20 with twelve baseline and six added obligations verified at `d77c9dc6`. `compat/tui/campaign.json` records the full pin, individual zz revisions, acceptance clauses, commands, environments, artifacts and independent reviews. `compat/tui/evidence/<ID>/<attempt>/` holds the captures. The [report](/tmux/tui-parity.md) is generated. The current oracle moved to 3.8 in October. `pin.tui-fixtures` in `compat/catchup/ledger.json` records the rerun at that pin.

```sh
python3 compat/tui/tracker.py check
python3 compat/tui/tracker.py ready
jq '.items[] | select(.id == "TUI-005") | {acceptance, proof}' compat/tui/campaign.json
python3 compat/tui/verify-claims.py
```

`check` validates the ledger, references, dependencies, proof fields and report freshness. A fully verified campaign can retain its historical pin. Reopening makes the tracker require the current oracle pin, and every proof must match its campaign pin. `verify-claims.py` also checks proof ancestry, mapped fixtures and case ownership. Run the fixture itself with the current zz build and fetched tmux.

```sh
cargo build -p zz-cli
compat/fetch-tmux.sh
python3 compat/tui/verify-claims.py --run TUI-005 --zz target/debug/zz_cli
```

# Proof surfaces

The [contract](/designs/tui-parity.md) compares CLI stdout, stderr and exit status, and attached clients' decoded cells, style, cursor, geometry and interaction. The outer pinned tmux gives both sides PTYs and decodes their screens. `capture-pane -e` preserves named, indexed and RGB colour classes even when the input escapes have different spellings. Isolate HOME, config and sockets, fix the outer size and TERM, and wait for observable state with deadlines.

Use the `FIXTURES` map in `compat/tui/verify-claims.py` to find the script for an obligation. For example, `compat/tui-screen-diff.sh` tests screen comparison and `compat/tui-copy-mode.sh` exercises selection through real input. The comparison's self-check must detect a one-sided change. A shared fixture charges each recorded case to its owner. Unattributed cases count against all mapped obligations. Accepted registry differences and skipped checks cannot satisfy an obligation's acceptance clause.

# The cycle

1. Reopen an obligation for contrary evidence or add one for new scope. Keep the baseline IDs fixed and record dependencies, affected tmux gaps, sources, measured behavior and next action. `tracker.py ready` orders work whose dependencies are verified.
2. Use `compat/tui/README.md` for setup and `compat/tui/HANDOFF.md` for the gate and resume procedure. The [compatibility cohorts](/playbooks/tmux-compat-cohorts.md) share its board. Before launching a new runner, apply `python3 compat/tui/lint-runner.py compat/tui/run-N.js` to that runner. Give each lane the source and fixture paths its obligation requires.
3. The worker measures the change, saves environment, binary hashes, captures and fixture output, and updates its evidence note. A reviewer independently reruns the clauses at the candidate tip and exercises a failing comparison. A timeout, waived case or incomplete dependency stays visible.
4. Run gates in the declared merge order. Remeasure the branch after rebase, resolve any cases waiting for a sibling, and record the revision that passed. Only the gate sets `verified` and records the proof and review. A completed proof waiting on another obligation stays in `review` until the dependency lands and the fixture passes at the new revision.
5. Regenerate with `python3 compat/tui/tracker.py write-report`. Run both trackers, the affected fixtures and their self-checks. The last gate runs all campaign fixtures, the [full attached corpus](/playbooks/compat-harness.md) and the crate checks prescribed in the handoff. Store the proof in the ledger once.

An interrupted parallel run can resume from completed worker and review results in its journal. `compat/tui/run-6b.js` shows how to feed those results into a fresh runner. `run-9b.js` shows how to continue a partially completed lane from its own branch.
