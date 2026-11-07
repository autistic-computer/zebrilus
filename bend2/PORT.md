# Bend core

The executable port targets Bend 2.0.40. Its independent proof check uses the BendTT kernel compiled with Lean 4.34.0. `toolchain.json` pins official release archives and their SHA-256 digests for Linux and macOS on x64 and arm64. `python3 bend2/toolchain.py` installs missing pinned tools under `~/.cache/zz-port` and prints their paths; `ZZ_PORT_TOOLCHAINS` selects another cache. Native compilation also needs clang. Archive extraction needs tar with gzip and zstd support.

The source baseline is zebrilus `27b789d56`. The port keeps the existing Rust implementation as the executable compatibility oracle.

| Bend source | Source contract | Ported behavior |
|---|---|---|
| `core.bend` | `crates/zz-protocol/src/framing.rs` | Protocol 118 envelope validation, reserved flags, both lanes, the 64 MiB limit, and exact error precedence |
| `core.bend` | `crates/zz-protocol/src/layout.rs`, `snapshot.rs` | Stable 64-bit IDs, layout pane enumeration, membership, parallel swap, sibling promotion on removal, split metadata preservation |
| `layout_edit.bend` | `crates/zz-protocol/src/layout.rs` `joined_layout` | Remove then insert, first matching branch, before/after ordering, axis and IEEE-f32 split ratio behavior |
| `core.bend` | `crates/zz-client/src/core.rs` | Agent attention status and attention edges |
| `replay.bend` | `crates/zz-client/src/agent_lens.rs` `AgentCursor::apply` | Sequence ordering, overlap suppression, malformed positional advancement, restoring resets, gap replay requests, journal sequence/size projection, saturating byte accounting and pending replay state |
| `terminal.bend` | `crates/zz-terminal/src/model.rs` `TerminalViewport::apply_patch` | Generation and dictionary alignment, dimensions, append count/byte bounds, strict span order, exact changed-cell budget, span bounds, scroll range and exposed-row replacement; atomic structural plans |
| `terminal_cells.bend` | `crates/zz-terminal/src/model.rs` `TerminalViewport::apply_patch` | Retained packed-cell shifting, span payload replacement and zero-filled clear tails, with structural and payload-count rejection before mutation |
| `input_router.bend`, `input_router_claims.bend` | `crates/zz-client/src/input.rs` `InputRouter`, `PrefixClaim` | Pending and observed focus, overlays, pane lifecycle, ordered press/release claims, capture and key routing precedence, exact effect queues and plain-key eligibility |

Each ID is two U32 words, preserving the complete u64 range without passing through a floating-point number. Split ratios stay as raw f32 bits. Independent layout branches use Bend parallel lets. Cursor transitions remain sequential because their order is observable.

The replay boundary receives normalized JSON decoding results. It records journal sequence and byte length; the caller retains the actual payload. Capability and available-command reduction, JSON parsing, history trimming and transcript rendering remain outside this port. The original cursor's temporary `seq - 1` on a restoring reset is eliminated: the reset bypasses the gap gate, then advances directly to its sequence, preserving the resulting cursor and journal.

# Formal evidence

Run from the repository root:

```bash
python3 scripts/port/guard.py bend bend2/PROOF.bend
python3 scripts/port/guard.py bend bend2/PROOF.bend --verdict
```

Both gates pass. The second independently checks the translation with the Lean-verified BendTT kernel. The dependency closure contains no foreign effects, `@unsafe`, open laws or proof placeholders. There are 71 public laws in `LAWS.bend` and its imported law modules, and 229 supporting lemmas in the proof modules. The retained tree reducer, its rollback and ordering laws, and its complete metadata boundary are described in [TREE_PORT.md](TREE_PORT.md). The input router's exact opaque boundary, 20 public laws and executable source traces are described in [INPUT_ROUTER_PORT.md](INPUT_ROUTER_PORT.md).

The public laws prove, for every input of their stated types:

- Layout swaps preserve pane count, the complete split shape and metadata, and are involutive. Equal source/target IDs and duplicate pane IDs require no exclusions.
- Envelope gate acceptance is exactly the conjunction of all validity gates. Short-prefix, oversized-length, lane and version errors have the specified precedence.
- Pending permissions demand attention and unchanged attention states produce no edge.
- Every replay step preserves or advances the last sequence. Pending replay requests are suppressed, a gap stops all remaining items in the batch, and a duplicate sequence leaves the cursor and journal unchanged.
- Rejected terminal structural plans preserve the complete retained structural state. Accepted plans take their exact output generations from the patch and require both full-width base generations to equal the retained ones. Every accepted span has a bounded row, start and length; every exposed scroll row has an actual matching span starting at zero whose length equals the row width or whose clear flag is true. Accepted dictionary additions fit the remaining style, grapheme and byte capacities. These are data-dependent certificates, including exact U32 and U64 equalities, derived from the executable validator's decisions.
- Retained-cell reduction preserves every cell on structural rejection or an invalid input/payload count, and empty edits preserve the complete grid. The implementation's preserve primitive leaves the grid unchanged; its replacement and clear primitives preserve exact arbitrary prefixes and suffixes, copy the complete incoming cell bits, and fill only the selected clear tail with zero cells. These laws apply to arbitrary opaque 64-bit cell records and arbitrary surrounding cells.
- Full row shifting in either direction and the complete ordered span sequence refine a source-index plan in `terminal_cells_origins.bend`. The plan depends on actual input lengths and patch/header values, without inspecting any cell bits. Every output entry is `Retained(index)`, `Incoming(index)`, or `Cleared`. Resolving the complete plan against arbitrary original and incoming cell lists gives exactly `Cells.mutate`; the complete `Cells.apply` also agrees with the plan's computed structural checks, payload-count checks, exact status and rollback. Every planned index has a structural certificate naming an existing cell in its stated source, and the executable numeric bounds checker universally returns true. These are full-list source-index refinement laws for the Bend mutation, not universal equivalence to every Rust execution.
- Every input-router key operation preserves its complete seven-field focus context, and every router event preserves capture. Focus acknowledgement and overlay restoration have exact full-state equations. Detach, window deactivation, effect draining, routing precedence and autorepeat have exact queue/claim equations. Generic vector laws preserve arbitrary prefixes and middles under the actual claim reducer's swap removal; no uniqueness assumption is needed for those list laws. Keymap resolution and raw input/action payloads enter through typed opaque references.

The supporting proofs establish structural equality reflection over both 32-bit ID words and unsigned 64-bit comparison/reflexivity/symmetry and maximum monotonicity. These are proofs over the implementation's actual word representations.

The proofs cover those equations. The terminal boundary receives raw generations, dimensions, dictionary counts and spans. UTF-8 and style payload validation, cell glyph and style references, and metadata merging remain in the Mojo client and are outside the terminal structural proof. The cell reducer preserves payload bits without interpreting their glyph/style validity. The proofs do not establish the complete postcard vocabulary, JSON parsing, I/O correctness, compiler code generation equivalence, floating-point join arithmetic, daemon behavior, terminal execution, or the complete application's correctness. `layout_edit.bend`, the `*_bridge.bend` modules, `terminal_cells_io.bend` and `bridge.bend` are outside the proof dependency closure; their behavior is exercised against executable evidence.

# Executable evidence

`crates/zz-client/examples/port_oracle.rs` calls the original Rust functions. It emits exact expected outputs for 5,811 envelope cases including a full 64 MiB boundary frame, 256 generated layouts with full-width IDs, 512 join cases, all 10 attention combinations, and 288 replay batch transitions including u64 saturation, malformed events, overlaps and restoring resets. The same oracle also emits terminal interaction vectors used by the Mojo port.

```bash
mkdir -p target/port
python3 scripts/port/guard.py bash -c 'cargo run --quiet -p zz-client --example port_oracle > target/port/oracle.json'
python3 scripts/port/guard.py bend bend2/core.bend -o target/port/core.mjs
python3 scripts/port/guard.py bend bend2/layout_edit.bend -o target/port/layout_edit.mjs
python3 scripts/port/guard.py bend bend2/replay.bend -o target/port/replay.mjs
python3 scripts/port/guard.py bend bend2/bridge.bend -o target/port/zz-bend
python3 scripts/port/guard.py node bend2/check.mjs
```

The checker compares results from Bend's generated JavaScript with the Rust oracle and samples the same envelope vectors through the native executable. It checks exact layout metadata, pane order, swap involution, join results, replay requests, journal entries and byte counters. These finite comparisons complement the universal laws.

`crates/zz-client/examples/terminal_plan_oracle.rs` calls the actual Rust `TerminalViewport::apply_patch` for 2,627 structural cases. Its vectors include full-width generation carries and mismatches, zero-sized grids, both scroll directions, missing/full/cleared exposed rows, duplicate and reversed spans, exact changed-cell budgets, dictionary append bases and style capacity edges. Two accepted cases contain the maximum 65,535 spans. `terminal_check.mjs` compares exact statuses and retained structural states through the generated JavaScript, then checks every vector through one persistent native worker. Span and exposed-edge validation use tail-recursive folds to keep the stack bounded on both backends.

`crates/zz-client/examples/terminal_cells_oracle.rs` calls the actual Rust mutation for 1,705 cases, producing 1,001,582 expected cells. The cases cover shift-only unaffected interiors, partial rows, complete rows, clear tails, zero-sized grids, malformed structural edits, full-width generation words and complete packed cell records with distinctive glyph, style and flag values. `terminal_cells_check.mjs` compares every resulting cell through the generated JavaScript and also checks truncated retained and changed payload rejection. `scripts/port/terminal_cells_check.py` performs the native binary-file comparisons and tests that rejected or malformed inputs do not touch the output file. Its optional `--profile` measures isolated native workers on increasing viewport sizes.

`terminal_cells_origins_check.mjs` runs the derived source-index plan against every one of those 1,705 source cases. It checks the exact status, every computed source-index bound, and all 1,001,582 resolved cell records, including row shifting, sequential replacement payload offsets and clear tails. The checker resolves large plans through constant-time host indexing into the source arrays; 1,660 small fixtures also run the proved Bend linked-list resolver directly. Another 274 truncated retained and incoming payload cases check the plan's exact rollback. The universal proof establishes the resolver relationship for arbitrary lists; these finite comparisons tie its executable plan to the source outcomes. The standalone independent provenance verdict peaked at 112 MiB, and its executable source differential peaked at 396 MiB under the shared guard.

```bash
python3 scripts/port/guard.py bend bend2/terminal_cells_origins.bend -o target/port/terminal_cells_origins.mjs
python3 scripts/port/guard.py node bend2/terminal_cells_origins_check.mjs
```

# Native boundary

The native executable accepts one envelope validation through arguments:

```text
zz-bend validate TOTAL DECLARED LANE FLAGS VERSION
```

`TOTAL` includes the four length-prefix bytes. `DECLARED` counts the envelope and payload, excluding that prefix. All arguments are unsigned 32-bit decimal integers. The result is one decimal status followed by a newline:

| Code | Meaning |
|---|---|
| 0 | Valid envelope |
| 1 | Truncated prefix or header |
| 2 | Declared length exceeds 64 MiB |
| 3 | Declared and actual lengths differ |
| 4 | Unsupported lane |
| 5 | Protocol version differs from 118 |
| 6 | Reserved flags are nonzero |

`zz-bend serve` reads the same `validate ...` command, one per line, from stdin and flushes one status per line to stdout. The Mojo transport keeps one worker alive and uses it for each incoming frame. It must connect stdin through a pipe: the Bend file effect opens `/dev/stdin`, which Linux cannot reopen when it is a socket. EOF closes the worker. Malformed commands exit 64; its structurally bounded loop permits 4,294,967,295 frames before exit 75. A fresh worker starts in approximately a millisecond on the development host; subsequent requests take tens to hundreds of microseconds.

The native gate's I/O and process boundary are runtime checked. Formal claims apply to the pure functions it calls.

The same persistent worker also accepts `terminal` followed by 29 unsigned decimal header fields and zero or more `ROW START LENGTH CLEAR` span quadruples:

```text
terminal C_GEN_HI C_GEN_LO C_VIEW_HI C_VIEW_LO C_DICT C_COLS C_ROWS C_STYLES C_GRAPHEMES C_BYTES P_BASE_HI P_BASE_LO P_BASE_VIEW_HI P_BASE_VIEW_LO P_GEN_HI P_GEN_LO P_VIEW_HI P_VIEW_LO P_DICT P_COLS P_ROWS P_STYLE_BASE P_APPEND_STYLES P_GRAPHEME_BASE P_APPEND_GRAPHEMES P_APPEND_BYTES SCROLL_POSITIVE SCROLL_MAGNITUDE P_CELL_COUNT [ROW START LENGTH CLEAR]...
```

`CLEAR` and `SCROLL_POSITIVE` are exactly zero or one. A zero magnitude is independent of its sign. The returned code is 0 for acceptance, 1 for a base-generation mismatch, 2 for dictionary misalignment or overflow, 3 for dimension mismatch and 4 for row, span, scroll or changed-cell budget errors. Dictionary epoch precedes dimensions; dictionary append checks follow dimensions, matching the source implementation's precedence. Target generations need not increase: the source only requires matching bases.

Every request is independent; rejected or partial requests cannot alter a later plan. Lines are limited to 2 MiB. Accepted source patches contain at most 65,535 spans because their rows must increase strictly, and each row/start/length is a u16. Their maximum decimal span representation occupies 1,310,700 bytes plus a small fixed header, below this limit. The transport uses real stdin pipes; the native differential harness creates them with Python because Node's child stdin socket cannot be reopened as `/dev/stdin` on Linux.

The experimental cell mutation command reuses the same header and spans:

```text
cells CURRENT_BINARY CHANGED_BINARY OUTPUT_BINARY HEADER SPANS
```

Each binary cell is exactly eight bytes: little-endian glyph U32 followed by the little-endian packed style/flags U32. Both inputs must have exactly the declared number of records. They are capped at 8,388,608 cells (64 MiB), matching the existing terminal wire limit, and every read/write chunk is at most 64 KiB. The worker opens the output only after structural and complete payload-count validation, writes the result, closes it and then emits status zero. Structural rejection emits its usual nonzero status without opening any file. Invalid binary lengths exit 65 before touching the output; an I/O failure may leave a partial output and never reports success, so callers consume a distinct output buffer only after status zero. Paths are whitespace-free private files or Linux `/proc/PID/fd/FD` paths to reusable anonymous files.

The Mojo stream calls immutable `TerminalViewport.validate_patch` before native cell mutation, retaining all UTF-8, payload-reference and metadata checks. Edits within the wire cell limit use `BendGate.cells` with separate retained, changed and result buffers; `apply_verified_cells` revalidates and checks the result's exact grid count before committing cells, dictionary additions, generations and metadata. The original `apply` remains available and shares the same validation and commit code. Empty edits bypass full-cell transport using the proved identity law.

The isolated native profile on the development host includes fresh-worker startup, full input/output file transport, a one-row shift and a clear tail. Peak worker RSS includes a fixed runtime overhead, so its ratio to payload size is largest for tiny grids. The largest case exercises the exact wire limit; the whole guarded test, including its Python fixture host, peaked at 962 MiB. These measurements describe the native list reducer, rather than the complete GUI process.

| Grid | Retained bytes | Time | Peak worker RSS |
|---|---:|---:|---:|
| 80 × 24 | 15,360 | 1.87 ms | 3.30 MB |
| 200 × 80 | 128,000 | 6.70 ms | 4.97 MB |
| 400 × 120 | 384,000 | 19.7 ms | 6.50 MB |
| 1,000 × 500 | 4,000,000 | 327 ms | 28.2 MB |
| 2,000 × 1,000 | 16,000,000 | 1.23 s | 100 MB |
| 4,096 × 2,048 | 67,108,864 | 6.09 s | 407 MB |
