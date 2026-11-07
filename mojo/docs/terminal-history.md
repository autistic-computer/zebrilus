# Retained terminal history and local scrolling

`zzmojo.terminal_history` ports the retained row reducer from
`crates/zz-client/src/scrollback.rs`. `zzmojo.local_scroll` ports the pure local
scroll state machine from `crates/zz-client/src/local_scroll.rs`.
`zzmojo.history_pacer` ports history range selection, request merging, retry,
stale completion handling, and quiet backfill.

The source-derived oracle executes the actual client functions. The Mojo
differential compares 2,371 cases and 7,077,844 bytes of exact results. The 12
retained-history traces contain 500 complete state snapshots: live and retained
packed cells, every style and grapheme byte, dictionary-plane sharing groups,
row revisions, generation and scrollbar fields, ring limits, mutation and
invalidation counters, and the global revision allocator. They include accepted
and rejected patches, counter wrap, full replacement, both scroll directions,
reanchors, gaps beyond the below cache, and independent style/grapheme compaction
above the source's 1,024-byte thresholds. Other traces check Float32 bits,
effect order, coalesced request floors, stale timer generations, prefetch minima,
the exact three-second retry boundary, stale history completions, and the
history-trickle wire option.

This is differential evidence for the history and scrolling port. The existing
Bend proofs cover the terminal mutation and source-index refinement; these new
history state machines do not have universal formal proofs.

## Runtime boundary

The live `InteractiveConnection` prepares retained rows before terminal patch
validation, runs the existing Bend cell boundary, and commits history only after
the resulting viewport is accepted. This preserves the source quirk that a
rejected patch can consume revision IDs while leaving retained state unchanged.
A prepared update carries its state revision and expected resulting viewport
header; stale preparations fail before cache mutation.

Each retained row owns shared cell and dictionary planes. Snapshot exports copy
row handles and revision metadata, and compaction replaces the row's cell plane
and dictionary together. Exported previous versions remain consistent. No
terminal plane is copied into every dictionary group. `AppModel.histories`
contains these frozen exports, and `history_invalidations` also covers panes
whose sidecars were evicted.

The source limits remain 10,000 history rows and 512 below rows, with requests
of at most 512 rows. The live transport additionally reduces a request's row
count for very wide terminals to stay within the decoder's eight-million-cell
limit, preserving the requested range's end. History-trickle defaults to 2,000,
merges actual mux option events, and uses the source's unsigned parsing rules.

`TerminalHistoryCache` bounds all pane sidecars together to 64 MiB and also
fits them within the existing 128 MiB terminal-data budget after live and command
output planes. It accounts for allocated list capacity, shared plane payloads,
unique dictionary/plane metadata, ring storage, revisions, and snapshot metadata.
Old UI-owned snapshot versions are separate from the cache's ownership count.
They share planes until those rows change and the application already bounds
its in-flight model handoff.

Pressure trims older history before below rows, shrinks allocated ring storage,
forgets pending requests, advances invalidation stamps, and lowers that ring's
capacity to avoid an immediate refill loop. A pane sidecar can be evicted when
its remaining metadata cannot fit. Local presentation then retires and requests
its target from the actual daemon. It never displays rows with stale generation
headers. A later full viewport restores the configured row limits.

With 10,000 rows, the four-pane profile records 9,098,148 cache-owned bytes for
80×24; adding 200×80 reaches 27,797,192 bytes; adding 400×120 reaches 62,496,876
bytes. A fourth 400×120 pane trims/evicts to 67,052,172 bytes across two sidecars,
below 64 MiB. The guarded compile/run peaked at 318 MiB and the native cache
exercise took 0.324 seconds on this machine. These are bounded cache exercises,
not a claim about total application RSS.

## UI adapter

`TerminalScrollController` consumes only a coherent typed `AppModel`. It applies
the source effects in order, schedules synchronization after 120 ms, ignores
stale generations, and retires a request after the two-second source timeout.
Connection changes and pane retirement remove their local timers. Ordinary
typing cancels local scroll; selection initiation flushes the target before
pointer input. Command-output routing retains its own ownership and focus rules.

The retained painter resolves each displayed row from history, the current
viewport, or below rows. Each row uses its own dictionary and stable revision. A new server/client
identity clears the terminal row caches before revision IDs can be reused.
Fractional scrolling paints the preceding row with the corresponding physical
pixel offset. Cursor and overlay rows use the live-row projection. Stale snapshot
headers fall back to the current viewport. Reconnect restores terminal focus;
ordinary rendering leaves sidebar or modal focus alone.

## Checks

Run these through the shared guarded task runner:

- `pixi run --manifest-path mojo/pixi.toml differential-history`
- `pixi run --manifest-path mojo/pixi.toml test-terminal-history`
- `pixi run --manifest-path mojo/pixi.toml test-local-scroll`
- `pixi run --manifest-path mojo/pixi.toml test-history-painter`
- `pixi run --manifest-path mojo/pixi.toml history-smoke`
- `pixi run --manifest-path mojo/pixi.toml history-workspace-smoke`
- `pixi run --manifest-path mojo/pixi.toml profile-history`

The private-daemon component smoke requests 512 real history rows, checks row
identities and payloads, scrolls fractionally through retained rows, and waits for
the server target to converge. The focused painter check asserts exact rows,
historical dictionary colors, fractional pixels, identity reuse, and stale
fallback. The workspace smoke exercises the actual local-scroll interception,
debounce, selection flush, input cancellation, retained terminal focus, and
reconnect focus. Native `Workspace.setup` sets initial terminal focus before
the first frame; ordinary frames do not force focus. Its image is written to `target/port/history-workspace.png`.

The complete native workspace smoke passed with a 1,443 MiB guarded peak. Its
reconnect check exposed a row-cache reset bug: old display-row indices survived
a cache clear. `mjui` now clears both previous and displayed row references. The
focused regression reuses the same revision IDs with a different cell payload
and dictionary background, and asserts the new exact pixels after reset; it
passed with a 556 MiB guarded peak.
