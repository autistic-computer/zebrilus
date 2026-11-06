# Retained tree reducer

`tree_reducer.bend` implements `TreeDelta::apply`, `TreeDelta::apply_overlay`, and `ClientView::apply` from `crates/zz-protocol/src/tree_delta.rs`. All nineteen `TreeOp` variants execute against the retained session, window, and pane graph. The terminal cell planes stay outside this graph boundary.

## Source behavior

An ordinary delta accepts only an exactly equal base generation, then assigns the exact supplied target generation. A lower target generation is valid. Overlays preserve both generation and focus; client views preserve generation and assign the supplied focus after all operations succeed. Any missing target rejects the complete batch and returns the original graph, including its original ordering and metadata references.

An empty batch preserves the complete original order. A nonempty successful batch sorts sessions by their full unsigned 64-bit IDs and windows stably by `(index, id)` exactly once, after every operation has executed. Duplicate session and window IDs are preserved. Lookups and replacements address the first match in the current vector order. Session and window removals remove all matches within the addressed collection, including the source behavior that removing an absent item succeeds. Pane maps retain unique ascending map keys. A pane map key can differ from the pane payload ID; incoming pane upserts derive their map key from the incoming payload ID.

Assignments to active-window, active-pane, focus, and layout references do not add referential-integrity requirements. No early sorting, monotonic-generation rule, or additional graph validation changes these source outcomes.

## Opaque metadata arena

The reducer carries unsigned 32-bit arena references for immutable metadata groups. The caller retains and validates the arena, then resolves result references into the corresponding typed fields. Arena references from different semantic groups are distinct. The groups are:

| Group | Retained fields |
|---|---|
| session-name | name |
| session-viewers | viewers |
| window-name | name |
| window-layout | layout, zoomed_pane, layout_dump, visible_layout_dump, pane_order, pane_z_order, floating, modal, sx, sy |
| window-flags | automatic_rename, activity, silence, pane_base_index |
| window-presentation | status_label, pane_border_status, pane_border_lines, pane_border_indicators |
| pane-title | title |
| pane-state | kind, synchronized_input, bell, dead, dead_status |
| pane-presentation | border_colour, active_border_colour, border_status_text, mode |
| pane-status | status |
| pane-summary | summary |

The source oracle interns the exact encoded bytes for each semantic group and retains the encoded bytes of each constituent field. The JavaScript differential resolves every output reference, reconstructs the complete postcard snapshot, and compares every resulting byte against the actual Rust reducer. An arena reference is therefore not treated as evidence that its underlying metadata matches: the fixture checks that separately.

## Binary boundary

`tree_bridge.bend` is an independent worker, and its `reply_args` function also serves the shared dispatcher in `bridge.bend`. It accepts `tree delta CURRENT UPDATE OUTPUT`, `tree overlay CURRENT UPDATE OUTPUT`, or `tree view CURRENT UPDATE OUTPUT`. Paths contain no spaces. `zz-bend-tree serve` reads one such command per line from a real stdin pipe. Its lines are limited to 16,384 bytes; the shared dispatcher permits up to 2 MiB per command line.

Each file contains raw little-endian unsigned 32-bit words. IDs are always represented by high and low words, in that order. Optional IDs are a word containing zero or one, followed by the two ID words only when present. Every vector has an explicit element count before its elements.

| Record | Words in order |
|---|---|
| snapshot | generation ID, optional focus ID, session count, sessions |
| session | ID, name reference, active-window ID, viewers reference, window count, windows |
| window | ID, index, name reference, active-pane ID, layout reference, flags reference, presentation reference, pane count, panes |
| pane entry | map-key ID, payload ID, title reference, state reference, presentation reference, status reference, summary reference |
| ordinary delta | base ID, version ID, operation count, operations |
| overlay | operation count, operations |
| client view | optional focus ID, operation count, operations |

Operation tags are zero through eighteen in the `TreeOp` declaration order. Target IDs and replacement metadata follow the corresponding `tree_data.bend` constructor field order. A session or window upsert includes its complete record. A pane upsert includes its payload ID and five references, without a separate map key.

The output starts with one status word, any error ID words, then the complete result snapshot:

| Status | Meaning | Additional error words |
|---|---|---|
| 0 | success | none |
| 1 | base mismatch | expected generation ID, received base ID |
| 2 | missing session | requested session ID |
| 3 | missing window | requested window ID |
| 4 | missing pane | requested pane ID |

Semantic rejection includes the exact original snapshot in the output. The worker prints the matching status only after closing a complete output file. Malformed input exits with status 65 and leaves the output unopened. Invalid commands exit with status 64.

The combined normalized input budget is 67,108,800 bytes, reserving 64 bytes below 64 MiB. Both input files are checked against the remaining budget before reading their contents. The output budget is 64 MiB and is checked before opening the output. Reads and writes use chunks of at most 65,536 bytes. Collection counts are checked against remaining words and the minimum record width before conversion to an iteration count or collection allocation. Invalid tags, partial words, trailing words, and unsorted or duplicate pane-map keys reject the packet.

## Verification scope

`tree_laws.bend`, `tree_fields_laws.bend`, and `tree_lookup_laws.bend` declare twenty public universal laws, with sixty-six supporting lemmas. Their proofs check with Bend and the independent Lean verdict. They establish atomic rejection for overlays, ordinary deltas, and client views; exact generation and focus behavior; first-match lookup identity and complete zipper reconstruction; field preservation for every pane, window, and session edit; exact pane field assignment; and stopped batches after an error.

Ordering is a constructive certificate over actual most-significant-first bits of the unsigned 64-bit ID or 96-bit `(index, id)` key. Each output partition consists of actual zero-bit keys followed by actual one-bit keys, recursively through every key bit. A separate universal law preserves the complete ordered sequence of values for every full key, proving stable duplicate ordering and preservation of the associated payloads. The certificates do not depend on caller-supplied ordering Booleans.

The proofs concern the represented retained graph and exact arena references. They do not prove the postcard codec, file system, native compiler, or the caller's arena encoding and type validation. Executable source differentials cover the full snapshot codec and both generated execution paths.

The source-derived oracle has 662 cases: every operation individually, all 361 ordered operation pairs, 256 deterministic longer batches, and focused cases for base mismatch, rollback after earlier successful edits, duplicate IDs and unsorted window indices, pane map-key/payload-ID mismatch, dangling references, empty batches, and client focus. Both generated JavaScript and the native binary compare the complete result graph and exact error IDs. JavaScript additionally compares the fully resolved source snapshot bytes. Native malformed-packet cases assert that the existing output remains unchanged.

## Resource measurements

The native profile uses valid graphs with empty names and viewers, no windows, reverse session IDs, and a nonempty successful operation that forces the final sort. Its two distinct arena entries contain valid encodings of an empty name and empty viewers. The maximum graph's complete source snapshot remains below the protocol frame limit. Every resulting normalized output word was compared against its exact expected value.

| Sessions | Normalized current bytes | Seconds | Worker peak MiB |
|---:|---:|---:|---:|
| 1,000 | 28,016 | 0.031 | 5.1 |
| 16,000 | 448,016 | 0.503 | 15.2 |
| 65,535 | 1,834,996 | 3.253 | 34.4 |
| 262,144 | 7,340,048 | 18.277 | 109.0 |
| 1,048,576 | 29,360,144 | 79.214 | 410.2 |
| 2,396,741 | 67,108,764 | 179.591 | 923.6 |

The maximum graph adds a sixteen-byte update and fits the combined budget. Its native resident expansion is approximately 14.4 times the normalized current bytes, including parsing, retained records, sorting, and output encoding. The 64 MiB budget is an offline ceiling. An interactive caller should use a smaller bounded request threshold and fall back to its typed reducer beyond that threshold. A 2 MiB live threshold and eight-second timeout allow margin over this machine's 65,535-session measurement; these are integration recommendations, not source protocol restrictions.

The JavaScript stress case separately sorts 65,535 sessions and 65,535 windows with repeated full sort keys, distinct values, and unsigned high-word boundaries. It checks every exact key and complete value against the expected stable order. Both reductions completed in 2.343 seconds with a 407 MiB guarded process peak. The combined global proof verdict, native differential, eight malformed cases, and scoped Rust clippy completed with a 136 MiB guarded peak. These measurements are local observations, not universal resource guarantees.

## Checks

All commands below run through `scripts/port/guard.py`, which provides the shared single build slot and hard memory bound. The source oracle consumes `target/port/control-vectors.json`, generated by the existing `control_port_vectors` example. The native worker consumes the packed form produced by `scripts/port/tree_check.py`.

```
python3 scripts/port/guard.py /home/demfabris/.cache/zz-port/bend/bin/bend --verdict bend2/PROOF.bend
python3 scripts/port/guard.py bash -c 'cargo run --locked -p zz-protocol --example control_port_vectors > target/port/control-vectors.json'
python3 scripts/port/guard.py bash -c 'cargo run --locked -p zz-protocol --example tree_reducer_oracle > target/port/tree-reducer-oracle.json'
python3 scripts/port/guard.py /home/demfabris/.cache/zz-port/bend/bin/bend bend2/tree_reducer.bend -o target/port/tree_reducer.mjs
python3 scripts/port/guard.py /home/demfabris/.cache/zz-port/bend/bin/bend bend2/tree_bridge.bend -o target/port/zz-bend-tree
python3 scripts/port/guard.py node bend2/tree_check.mjs
python3 scripts/port/guard.py node bend2/tree_stress.mjs
python3 scripts/port/guard.py python3 scripts/port/tree_check.py
python3 scripts/port/guard.py python3 scripts/port/tree_check.py --profile --maximum-sessions 2396741
```
