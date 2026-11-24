---
type: Research Report
title: Codebase Audit for Code Smells, Rust Antipatterns, and Performance Issues
description: Revalidation at 9c8891c found nine confirmed issues, four qualified or latent findings, one intentional ABI contract, and one overstated impact claim.
tags:
  - research
  - audit
  - rust
  - concurrency
  - performance
  - ffi
timestamp: 2026-08-17T14:45:00-03:00
git_commit: 9c8891c61520954bba57a711eba389b8e9e40982
---

# August audit and disposition

Of 34 initial claims, the check at `2f8fbd99` dropped 19 after following callers, timeouts, platform
guards and existing containment. The second check at `9c8891c61` retained nine confirmed findings, four
conditional or latent ones, one intentional ABI contract and one overstated impact claim. The repair
commit is `59d853045`. The table preserves the original finding IDs and the limits on what each
established.

| ID | Observation at the audited revision | Disposition |
| ---: | --- | --- |
| 1 | On Unix, an indefinite wait for a full PTY write buffer blocked resize, shutdown and producers on the terminal actor. | Bounded whole-command input admission at 256 commands/64 MiB, with nonblocking writes retried alongside reads and control work. |
| 2 | An active pane with an armed prefix could consume a dialog's Enter/Escape before dispatch. Delivery to the PTY was unproven. | Tokenized prefix cancellation gates ordinary workspace keys through daemon acknowledgement or connection reset. |
| 3 | Stored Alt-Ctrl and live Control-Alt names disagreed for the same chord. | Stored and live paths order Control before Alt. |
| 4 | Overlapping Git captures spawned seven children per scan of a repository with HEAD, eight for an unborn repository, with two reader threads per command. User-visible cost lacked a production profile. | A single-flight gate coalesces pending refreshes. Any cheaper capture must still represent tracked/untracked files, deletions and renames. |
| 5 | Copy counts up to 9,999 eagerly cloned the invocation and two arguments for each stock cursor-down command. | Bounded cost at this revision. A performance change needed a profile. |
| 6 | At 250 rows, shift scoring used 62,500 fingerprint comparisons plus 124,500 nonzero-loop predicate visits. | No production frame-time attribution. |
| 7 | Five Swift getters per pane each walked the entire layout, giving 5N traversals of an N-pane window. | No app profile attributed cost. The audited FFI crate has since been removed. |
| 8 | Across 44 Unix C exports, a panic leaving the boundary would abort. Poisoned locks already recovered. | No reachable panic found on a valid normal call. The audited crate has since been removed. |
| 9 | Connect queued HELLO before waking the event descriptor. Both audited consumers happened to drain eagerly. | Connect was changed to wake poll-first consumers. |
| 10 | The C contract returned bytes with an explicit length. C used a length-bound comparison and Swift used the buffer length. | Intentional. Copy before another call invalidates the backing snapshot. |
| 11 | The editor reducer used process cwd as a placeholder. The daemon replaced it with the donor's live cwd under the same lock before publication. | The live-cwd integration test passed. Direct reducer use and cwd lookup failure remained edge cases. |
| 12 | Copy-pipe capped selection at 32 MiB before spawning, staged stdin in a tempfile, and started its 30 s timeout after staging/spawn. | No tempfile-related failure established. The timeout fix terminated the shell's process group. |
| 13 | On Windows cookie import, the DPAPI-owned output remained in the OS allocation after copying to protected storage. | Wipe that allocation before freeing it. |
| 14 | The AppKit send-event flag restored its outer state on normal return, but a foreign unwind could skip restoration. | A drop guard restores the state. No crash was demonstrated. |
| 15 | A BSD wake drained at most 64 bytes, allowing extra idle actor cycles. Channel checks on either side of the drain still serviced queued commands. | Both pipe ends became nonblocking, interrupted writes retried and draining continued to EAGAIN. |

At `1ce632dce`, `crates/zz/src/workspace/view.rs` retains the dialog acknowledgement/reset test, and
`crates/zz-daemon/src/agent/host.rs` gates `start_git_refresh` through a request before spawning.
`libs/ala-cef/src/chrome_import/cookie.rs` wipes DPAPI output in `dpapi_unprotect`. Its
`src/mac_app_protocol.rs` uses `SendEventRestore`. The checkout has no `crates/zz-client-ffi`. Findings
7–10 describe that retired boundary. For original file paths, symbols and per-claim reasoning, use
`git show 4fa976578:knowledge/research/2026-08-17-codebase-audit.md`.
