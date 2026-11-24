---
type: Reference
title: "TUI parity campaign"
description: "TUI parity obligations, their proof status, and progress against the fixed baseline."
resource: compat/tui/campaign.json
tags: [tmux, tui, compatibility, campaign]
timestamp: 2026-09-20T00:00:00Z
---

# TUI parity campaign

**12/12 baseline** and **6/6 added obligations** verified against tmux `d77c9dc6aa021e4bc61f0da128c591af695e6466`.

The campaign compares CLI stdout, stderr and exit codes exactly. Attached-client proofs compare decoded cells, styles, cursor, geometry and interactions. Only verified obligations count toward completion. The [gap registry](../../compat/tmux-gaps.json) records accepted differences and closed gaps separately.

Campaign status (0 unmeasured, 0 different, 0 active, 0 review, 0 blocked, 18 verified).
No dependency-ready obligations.

For an obligation's criteria, environment, commands and artifacts, look up its ID in the [campaign ledger](../../compat/tui/campaign.json).

```sh
jq --arg id 'TUI-001' '.items[] | select(.id == $id)' compat/tui/campaign.json
```

## everyday: Everyday terminal use

| ID | Obligation | Scope | Status | Reviewed revision |
| --- | --- | --- | --- | --- |
| `TUI-001` | Reliable attached-client baseline | baseline | verified | [`8bb6bd10a70c`](../../compat/tui/evidence/TUI-001/attempt-01/review.md) |
| `TUI-002` | Complete screen and interaction comparison | baseline | verified | [`8bb6bd10a70c`](../../compat/tui/evidence/TUI-002/attempt-01/review.md) |
| `TUI-003` | Stock launcher, command and key behavior | baseline | verified | [`f5f8adad1e87`](../../compat/tui/evidence/TUI-003/attempt-01/review.md) |
| `TUI-004` | Default canvas, status and pane borders | baseline | verified | [`83a19c0efab3`](../../compat/tui/evidence/TUI-004/attempt-05/review.md) |
| `TUI-013` | Recorded macOS geometry-report timeout | added | verified | [`d6ed90e9b042`](../../compat/tui/evidence/TUI-013/attempt-01/review.md) |

## interactive: Interactive surfaces

| ID | Obligation | Scope | Status | Reviewed revision |
| --- | --- | --- | --- | --- |
| `TUI-005` | Pane copy mode and search | baseline | verified | [`b7dbcbf61949`](../../compat/tui/evidence/TUI-005/attempt-03/review.md) |
| `TUI-006` | Chooser and command-output presentation | baseline | verified | [`e15d4f36e096`](../../compat/tui/evidence/TUI-006/attempt-03/review.md) |
| `TUI-007` | Prompts, menus, popups and pane labels | baseline | verified | [`83a19c0efab3`](../../compat/tui/evidence/TUI-007/attempt-02/review.md) |
| `TUI-008` | Mouse, paste, focus and key ownership | baseline | verified | [`c1ab1061a762`](../../compat/tui/evidence/TUI-008/attempt-05/review.md) |

## breadth: Broader parity and command composition

| ID | Obligation | Scope | Status | Reviewed revision |
| --- | --- | --- | --- | --- |
| `TUI-009` | Outer-terminal capabilities and fidelity | baseline | verified | [`ee6529d71037`](../../compat/tui/evidence/TUI-009/attempt-05/review.md) |
| `TUI-010` | Slow output, recovery and client lifecycle | baseline | verified | [`f5f8adad1e87`](../../compat/tui/evidence/TUI-010/attempt-01/review.md) |
| `TUI-011` | Remaining stock client command inventory | baseline | verified | [`40e51aa92a1d`](../../compat/tui/evidence/TUI-011/gate-07/gate-note.md) |
| `TUI-012` | Superset commands beside tmux behavior | baseline | verified | [`d84ea7ecbbf3`](../../compat/tui/evidence/TUI-012/attempt-01/review.md) |
| `TUI-014` | Client mode tools in the raw TUI | added | verified | [`40e51aa92a1d`](../../compat/tui/evidence/TUI-014/attempt-20-modes-muse9/notes.md) |
| `TUI-015` | A lock surface a client can draw | added | verified | [`b01dc1e95cda`](../../compat/tui/evidence/TUI-017/attempt-10/review.md) |
| `TUI-016` | Server log and terminal introspection | added | verified | [`9211d1e1b299`](../../compat/tui/evidence/TUI-016/attempt-01/review.md) |
| `TUI-017` | Rich capture transports and the snapshot residues | added | verified | [`ff9a76ad73ad`](../../compat/tui/evidence/TUI-017/attempt-10/review.md) |
| `TUI-018` | Caller stream forms | added | verified | [`9fc69e2f5849`](../../compat/tui/evidence/TUI-018/attempt-12/gate-handoff.md) |

Regenerate this page with `python3 compat/tui/tracker.py write-report`.
