# Verified input router kernel

`input_router.bend` and `input_router_claims.bend` port the pure focus, press-claim and key-routing behavior of `crates/zz-client/src/input.rs`. The executable observer calls the actual Rust `InputRouter` and `ChromeKeymap`; it does not contain a second routing implementation. Production Rust behavior is unchanged. Its private-state observer and public fixture entry point exist only with the `input-router-fixtures` feature.

The complete retained state contains the keymap reference, actual owner, pending owner, observed owner, restore owner, active pane/surface, capture pane, ordered held claims, ordered locally suppressed releases and ordered effect queue. Pane IDs retain both 32-bit words of every source u64. `Owner` distinguishes terminal/browser/other panes, sidebar, overlay, native editor and no owner.

The opaque boundary is explicit. `KeyToken`, `Payload` and `ChromeAction` are distinct typed wrappers around full-width identity references. Each input contains its original key reference, optional unshifted key reference, press/repeat/release action and complete original payload reference. The caller must intern key identities by actual `KeyCode` equality, and an unshifted code point must use the same identity as that character's key. The retained keymap reference identifies a caller-owned immutable map version. `Lookups` supplies the actual UI, sidebar, terminal and browser resolution results for that original input. Plain-key eligibility receives the actual UI/terminal table-presence results. Keymap decoding, modifier matching, `unbind_action`, raw key decoding and action execution remain caller responsibilities. Forwarded effects retain the complete original input rather than substituting its normalized key.

The observer interns actual Rust keys, full `KeyInput` values and actions, includes their reference dictionaries, and derives every lookup from the actual map. Inputs include text containing NUL, non-ASCII characters, all modifier combinations, unshifted characters and distinct high/low ID words. The Bend checker folds its own state continuously from `new`; it never replaces that state with a later Rust snapshot. It compares every complete before/after state, effective owner, disposition, drained queue and plain-key result.

# Source behavior preserved

All ten source events are represented. Effective focus prefers pending focus. A stale native focus observation updates `observed` without replacing a different pending owner. Activating a pane while an overlay is open changes `active` and `restore` without requesting focus. Overlay close restores the effective owner saved on open and appends its focus request. Pane removal uses the source's exact field-specific rules, including clearing the actual owner when the removed pane matches a pending pane. Neither pane removal nor detach clears capture; detach also preserves the existing effect queue. Window deactivation only clears claims.

Release handling precedes overlay, capture and keymap handling. A locally suppressed release also removes its held claim; another held release returns to its recorded pane with its original input. A non-release clears matching local suppression before testing the overlay. Capture precedes UI bindings; UI bindings precede a claimed active pane; owner-specific bindings follow those. An armed, claimed repeat is consumed without forwarding or replacing a held claim. Capture forwards repeats using the source's non-autorepeat claim path. Claim removal uses exact vector `swap_remove` ordering. Pane removal uses stable filtering and removes local suppressions using the original held claims. Plain-key eligibility intentionally ignores held claims.

# Universal proof scope

The aggregate `LAWS.bend` imports 18 kernel laws filled by `input_router_proof.bend` and two transaction equations filled by `input_router_transaction_proof.bend`. The four kernel proof modules add 47 supporting lemmas: 4 in the main proof, 18 for event capture preservation, 18 for complete focus-context preservation and 7 for generic vector operations. The transaction equations state that `resolve` with no worker candidate evaluates the original complete request and that a second drain returns an empty effect list for every initial state. The aggregate now has 71 public laws and 229 supporting lemmas in 43 dependency modules.

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

The isolated boundary is implemented by `input_router_bridge.bend` and the Mojo modules `verified_input_router`, `input_router_codec`, `input_router_storage` and `input_router_gate`. The existing combined worker and application input path are unchanged. Source-compatible lookup resolution, focus projection and ordered timeline integration remain separate work.


# Isolated native and Mojo boundary

`zz-bend-input serve` accepts `input REQUEST_PATH RESULT_PATH` and returns `0` only after writing the complete result. The same arguments can be supplied once without `serve`. Requests are stateless, so worker restart cannot lose retained claims or replay an old effect queue. A malformed packet exits 65 before opening the output. Reads and writes are at most 64 KiB each. The Mojo gate uses two reusable anonymous Linux files, or private close-on-exec temporary files on other platforms, with distinct request/result storage. It clears the result before every request.

Packets contain little-endian U32 words. Each starts with magic `1230131266` and version 1. IDs always use high word then low word. A Boolean or optional tag is exactly 0 or 1. A list starts with a U32 element count, checked against both its limit and the remaining words before constructing its elements. Unknown tags, incomplete words and trailing words are rejected.

The request contains the complete state followed by one operation. The state order is map reference, owner, optional pending owner, observed owner, restore owner, optional active pane/surface, optional capture pane, held claims, local claims and effects. Owner tags are none=0, pane=1, sidebar=2, overlay=3 and native editor=4; pane owners append an ID and surface tag (terminal=0, browser=1, other=2). Active panes omit the owner tag. Held entries contain key ID and pane ID. Input fields are key ID, optional unshifted key ID, action (press=0, repeat=1, release=2) and payload ID. Effects are forward=0 followed by pane/input, chrome=1 followed by action ID, or focus=2 followed by owner.

| Operation tag | Body |
|---|---|
| 0 | Event tag and its source fields; the ten event tags follow their order in `RouterEvent` |
| 1 | Optional capture pane |
| 2 | Key identity to suppress |
| 3 | Complete input, armed/claimed Booleans, then optional UI/sidebar/terminal/browser actions |
| 4 | Drain, no body |
| 5 | Releases/UI-table/terminal-table presence Booleans |

A result contains the complete next state and an answer: none=0; key=1 followed by 0 for consumed or 1 for native; drained=2 followed by the ordered effects; or plain-key=3 followed by its Boolean.

`RouterMapArena` owns one immutable map-version/action-body snapshot. `RouterPayloadArena` owns a source-scope/key/input/body snapshot. Constructors reject duplicate identities and excessive counts/bytes; every input key and optional unshifted key must belong to the key registry. A session takes ownership of both snapshots. Callers must allocate stable key identities by actual key equality and unique source scopes/map versions; arenas do not implement key decoding or keymap resolution. Opaque bodies can contain NUL. Body access returns a separate copy. Normalized requests and comparison buffers contain references only, so checking execution does not clone raw payload bodies.

`InputRouterSession.prepare(gate, operation)` keeps the current state unchanged. The gate computes the independent typed Mojo result from that exact state, then requests native execution when eligible. It validates every returned key, action, payload and complete forwarded-input identity against the owned arenas, verifies the answer kind, and compares the complete canonical state, answer and ordered effects against typed execution. Only an exact match accepts the Bend result. This checked execution mode evaluates both implementations and incurs their bounded normalized-data overhead. An absent, malformed, foreign, semantically incorrect or timed-out worker is killed/reaped and disabled; the already computed typed result remains the candidate.

A prepared transition captures the exact original normalized request, session generation, source scope and map version. `commit` rechecks those values, all references and delivery capacity before changing state. It rejects a plan for another pre-state even when its generation/scope/map happen to match. Each successful commit advances the generation; a second commit of the same plan is stale. Only an explicit drain moves effects to the delivery mailbox. `take_effects` moves that mailbox out once. Callers execute effects after commit and resolve their bodies through the owning session. These Mojo/transport properties are covered by executable tests; the Bend transaction equations do not prove universal equivalence of Mojo, native I/O or compiler output.

The session owns immutable arena snapshots. `install_map` now requires empty retained effects and delivery, preserves all claims/focus/capture and changes only the map identity. `replace_payloads` requires the same source scope and an empty delivery mailbox, validates every retained key/input and preserves the exact raw bytes of pending inputs. Both advance the existing monotonic generation and invalidate prepared plans; neither rebuilds a generation-zero session per key. The complete source keymap resolver and these lifecycle checks are described in [the Mojo keymap port](../mojo/docs/chrome-keymap.md). A future ordered adapter must supply actual desktop caller projection and atomic effect delivery. No application enablement is claimed here.

# Separate resource policy

| Resource | Limit |
|---|---:|
| Native normalized request/result | 64 KiB each |
| Native decoded held/local vector capacity | 256 each |
| Native decoded effect capacity | 512 |
| Native request admission with append headroom | 255 held, 255 local, 511 effects |
| Outer normalized request/result | 1 MiB each |
| Outer held/local/effects and delivery queue | 4,096 each |
| Key identities, inputs, actions per arena | 4,096 each |
| Retained raw payload bodies | 4 MiB total |
| Retained action bodies | 256 KiB total |

Admission conservatively reserves one extra claim/effect slot for transitions which can append; native requests require fewer than 256 held/local entries and fewer than 512 effects before reduction. Other valid source states use the bounded typed path. Outer admission similarly rejects an operation without its conservative append capacity. Refusal is explicit and leaves the complete session state and delivery queue unchanged. These limits do not redefine source behavior or evict claims. Generation exhaustion also refuses atomically. A worker deadline is 2 seconds per pipe wait; the timeout fault test includes a child that ignores SIGTERM and verifies forced termination before fallback.

# Boundary conformance

Run `python3 scripts/port/input_router_boundary.py` from the repository root. It self-guards all builds/runs, checks the aggregate proof normally and independently, refreshes the actual source oracle, builds the isolated worker and two focused Mojo probes, and writes `target/port/input-router-boundary.json`. The unified check may use `--skip-proof --skip-reference` immediately after the existing aggregate/source stages.

All 122 traces and 21,116 operations compare exactly in both typed Mojo and native Bend, including 2,946,620 canonical result bytes in each phase. Healthy execution accepts all 21,116 native results with no fallback. Ten fault modes each replay the entire corpus through exact fallback: foreign map/key/action/payload references, altered input metadata, a structurally valid in-scope semantic change, truncation, excessive output bytes, an exited worker and a timeout. The full corpus therefore checks 253,392 transitions across twelve execution modes, with 35,359,440 canonical result bytes compared.

Focused assertions check unchanged state after prepare, stale and foreign-prestate commit rejection, drain delivery exactly once, raw NUL preservation and body-copy isolation, native-limit typed fallback, outer claim/effect/delivery refusal, and excessive payload/duplicate map rejection. A resource stress accepts 255 held claims, 255 local claims and 511 effects natively; the 256/256/512 case takes the exact typed path. These synthetic vector shapes test adapter capacity rather than source reachability. Five direct malformed native packets preserve an existing sentinel output. The complete proof/source-refresh/build/boundary suite peaked at 1,222 MiB; the final rebuild and all boundary checks without repeating those unchanged proof/source stages peaked at 850 MiB. Focused runtime probes including outer-budget refusals used 17,548 KiB in typed mode and 17,680 KiB with the native worker; healthy corpus runs took 0.38 seconds in typed mode and 2.89 seconds in checked native mode on this host. These timings include the full corpus, arena validation and reference transport, rather than only a reducer call. Current timing/memory results are recorded in the generated report. These are finite conformance and fault checks, alongside the universal pure Bend equations.
