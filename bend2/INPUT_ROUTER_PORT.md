# Verified input router kernel

`input_router.bend` and `input_router_claims.bend` port the pure focus, press-claim and key-routing behavior of `crates/zz-client/src/input.rs`. The executable observer calls the actual Rust `InputRouter` and `ChromeKeymap`; it does not contain a second routing implementation. Production Rust behavior is unchanged. Its private-state observer and public fixture entry point exist only with the `input-router-fixtures` feature.

The complete retained state contains the keymap reference, actual owner, pending owner, observed owner, restore owner, active pane/surface, capture pane, ordered held claims, ordered locally suppressed releases and ordered effect queue. Pane IDs retain both 32-bit words of every source u64. `Owner` distinguishes terminal/browser/other panes, sidebar, overlay, native editor and no owner.

The opaque boundary is explicit. `KeyToken`, `Payload` and `ChromeAction` are distinct typed wrappers around full-width identity references. Each input contains its original key reference, optional unshifted key reference, press/repeat/release action and complete original payload reference. The caller must intern key identities by actual `KeyCode` equality, and an unshifted code point must use the same identity as that character's key. The retained keymap reference identifies a caller-owned immutable map version. `Lookups` supplies the actual UI, sidebar, terminal and browser resolution results for that original input. Plain-key eligibility receives the actual UI/terminal table-presence results. Keymap decoding, modifier matching, `unbind_action`, raw key decoding and action execution remain caller responsibilities. Forwarded effects retain the complete original input rather than substituting its normalized key.

The observer interns actual Rust keys, full `KeyInput` values and actions, includes their reference dictionaries, and derives every lookup from the actual map. Inputs include text containing NUL, non-ASCII characters, all modifier combinations, unshifted characters and distinct high/low ID words. The Bend checker folds its own state continuously from `new`; it never replaces that state with a later Rust snapshot. It compares every complete before/after state, effective owner, disposition, drained queue and plain-key result.

# Source behavior preserved

All ten source events are represented. Effective focus prefers pending focus. A stale native focus observation updates `observed` without replacing a different pending owner. Activating a pane while an overlay is open changes `active` and `restore` without requesting focus. Overlay close restores the effective owner saved on open and appends its focus request. Pane removal uses the source's exact field-specific rules, including clearing the actual owner when the removed pane matches a pending pane. Neither pane removal nor detach clears capture; detach also preserves the existing effect queue. Window deactivation only clears claims.

Release handling precedes overlay, capture and keymap handling. A locally suppressed release also removes its held claim; another held release returns to its recorded pane with its original input. A non-release clears matching local suppression before testing the overlay. Capture precedes UI bindings; UI bindings precede a claimed active pane; owner-specific bindings follow those. An armed, claimed repeat is consumed without forwarding or replacing a held claim. Capture forwards repeats using the source's non-autorepeat claim path. Claim removal uses exact vector `swap_remove` ordering. Pane removal uses stable filtering and removes local suppressions using the original held claims. Plain-key eligibility intentionally ignores held claims.

# Universal proof scope

The aggregate `LAWS.bend` imports 18 public router laws, all filled by `input_router_proof.bend`. Its four proof modules add 47 supporting lemmas: 4 in the main proof, 18 for event capture preservation, 18 for complete focus-context preservation and 7 for generic vector operations. The aggregate now has 69 public laws and 229 supporting lemmas.

The checked equations establish:

- Every key operation preserves the complete keymap/owner/pending/observed/restore/active/capture context for arbitrary states, inputs, prefix values and resolved bindings. Every router event preserves capture.
- Generic swap removal preserves arbitrary prefixes and middles, moves exactly the last element to the selected position, and removes exactly the selected final element when appropriate. These use the executable `pop_last`/`replace_at` implementation over arbitrary element types and lists.
- Unshifted key identity takes precedence for claim tracking.
- Detach, window deactivation, active selection, overlay activation, pending focus acknowledgement, stale sidebar observation, overlay round trips and queue draining have exact full-state results.
- Overlay press, armed claimed repeat, captured press, UI binding and a claimed release have exact state/effect equations establishing their stated precedence. The claimed-release equation permits arbitrary remaining held claims, effects, owner, pending owner and capture, and retains the original input payload.

No law accepts an input Boolean asserting that routing was correct. Preservation is proved through the actual event/key reducers, full context records and actual list transformations. Normalized lookup values remain the stated opaque seam. These proofs do not establish universal equivalence to all Rust executions, the codec/compiler/I/O boundary, keymap resolution or application input dispatch. The generic list helper is only source-compatible for a valid removal index; the router derives indices from actual list searches. Its total behavior on an invalid index is not a claim about Rust's panicking `Vec::swap_remove`.

# Executable evidence and limits

Run from the repository root:

```bash
python3 scripts/port/input_router.py
```

The wrapper acquires the shared 4 GiB build guard, checks the full aggregate proof normally and with the independent Lean-backed verdict, runs the feature-gated Rust observer, compiles the Bend kernel and typed data module to JavaScript, and compares the traces. `--skip-proof` is intended for the unified check immediately after its aggregate verdict. Results are written to `target/port/input-router-verification.json`.

The checked corpus has 122 traces and 21,116 operations: 9,073 events, 4,404 keys, 2,697 drains, 2,647 plain-key checks, 1,652 capture changes and 643 release suppressions. It contains 1,430 forwarded and 138 chrome effects. Five focused traces and 96 deterministic generated traces are complemented by 21 priority matrices spanning seven owner kinds, empty/owner-only/UI-plus-owner maps, all prefix flags, capture, active-pane presence and all key actions. The fixture's largest retained claim lists have five entries each; its largest effect queue has 27 entries. All complete state/effect/results match. The final full proof/differential wrapper peaked at 1,702 MiB under the shared guard.

The source trace file is 39,163,389 bytes. The checker refuses files above 64 MiB, more than 256 traces or more than 512 operations in one trace. These are harness resource limits, not source router semantics or universal bounds on claims/effects. The pure reducer has no new truncation, eviction or capacity policy. Its independent proof has no vector-length bound.

# Typed integration API

The pure interface is `new(keymap_ref)`, `event(state,event)`, `set_capture(state,pane)`, `suppress_release(state,key)`, `key(state,input,prefix,lookups)`, `drain_effects(state)` and `passes_plain_keys(state,releases,ui_binds,terminal_binds)`. A key result contains the complete next state and `Consumed` or `Native`; a drain result contains the next state and complete ordered effects. The caller resolves opaque payload/action references only after accepting the returned transition.

There is no live bridge change in this checkpoint. A future bridge can accept a complete normalized state and operation atomically and return the complete next state/effects. It must validate reference ownership and byte/count limits before allocation, reject malformed results before committing any Mojo state, and preserve a typed reducer fallback above its separate resource budget. Stateless requests avoid a restarted worker losing held claims or duplicating queued effects. Such transport and live integration require their own native conformance and memory/latency checks before enabling them.
