# Mojo client

The client speaks the current daemon protocol directly over a Unix socket. It owns the postcard command codec, modern interactive Hello/Welcome/Batch stream, typed snapshots and atomic tree deltas, stable 64-bit identifiers, focus reconciliation, packed terminal decoding and painting, input routing, and a native mjui workspace. The existing Rust daemon continues to own PTYs and mux state.

From the zebrilus checkout:

```sh
just port build
target/port/zz-mojo --socket /tmp/zz.sock list-sessions
pixi run --manifest-path mojo/pixi.toml build-app
pixi run --manifest-path mojo/pixi.toml run --socket /tmp/zz.sock
pixi run --manifest-path mojo/pixi.toml shot /tmp/zz.sock /tmp/workspace.png
```

The native workspace uses the adjacent mjui checkout and its portable archives, SDL host, shaders, and fonts. Run its commands from the supplied script so resources resolve. The app has session navigation, window and pane selection, session creation, pane splitting, terminal text input and named keys, a reconnect action, and native terminal cells, colors, graphemes, overlays and cursor. A native worker serializes socket operations and input actions; the UI receives completed model snapshots without waiting for the daemon. Local selection changes carry an epoch so an older worker result cannot replace a newer selection. Pending input has a 4 MiB aggregate bound and a 4,096-action bound.

Each connection owns a persistent native Bend worker. Every daemon frame passes its envelope check before decoding. Tree deltas and client-view overlays use the same native worker within a 2 MiB normalized input bound; larger graphs retain the typed Mojo reducer. The adapter resolves compact, semantically typed metadata references before committing a successful complete tree. Rejections preserve the old tree and request a resynchronization. Mojo validates terminal payloads before Bend performs retained-cell mutation; Mojo commits the returned cells, dictionaries and metadata after a successful verdict. Linux exchanges cells through three reusable anonymous memory files, with a separate result buffer; other targets use private temporary files. Cell mutation is bounded to 64 MiB of retained cells. Metadata-only patches use the proved identity path, and larger retained grids use the validated Mojo mutation path with Bend structural preflight. `ZZ_BEND` overrides the worker executable; otherwise the client finds `zz-bend` next to its own executable. Protocol size bounds are checked before allocation. Bend proofs cover the named pure-core contracts, not binary file I/O, the Mojo transport, UI, or operating system.

The stream decodes full and patch frames, command-output viewports and history chunks from the packed terminal lane. Native grids use ABI-checked records; row caches include cell, dictionary and style changes. The color resolver preserves the reference's faint rounding, adaptive contrast and decorative Unicode exemptions. All visible split panes render at their measured grid sizes, independently of the daemon's normalized retained dimensions. Row, cursor and overlay projection follow the source placement rules. Pane-size reports carry the layout generation. Session/window/pane data comes from the typed interactive snapshot stream. Linux uses one-shot socket readiness watches while idle, with worker completion checked during active work; the portable fallback polls. Native key routing preserves press ownership, prefix claims and associated committed text without duplication. Pointer routing uses actual painted grid geometry, preserves captured pane ownership, forwards native mouse packets, and handles selection, wheel fractions, hover handoff and timed selection autoscroll. Status, notification, terminal-negotiation and command-output watermark reduction retain typed state. Command-output and history presentation, complete selection, browser/agent/editor renderers, settings, command palette, SSH transport, clipboard exchange and desktop integration remain in progress. Other event payloads are retained for their remaining reducers. The command client does not yet implement prepared command lists, stdin/exec handoffs, or environment/cwd publication.

Mojo 1.1.0 is resolved in `pixi.lock`. The language skill describes newer `match` and `if case` syntax that this compiler rejects; this target uses the compiler's supported conditional dispatch and guarded Optional access.

Build and test tasks use the shared build guard in `scripts/port/guard.py`. Linux builds run with a maximum 4 GiB memory budget and at most 256 MiB of swap; test suites run sequentially. The guard reserves 2 GiB of available device memory at startup and stops its own scope if available memory falls below 1 GiB. The native wrapper selects system Clang when available; `MJ_CC` and `MJ_CXX` select compatible compiler executables explicitly.

Native rendering builds use `--fp-mode=contract=off` to preserve the reference's floating-point operation boundaries. The native foreground color resolver is checked with exact RGB values rather than a pixel tolerance.

Verification:

```sh
pixi run --manifest-path mojo/pixi.toml test
pixi run --manifest-path mojo/pixi.toml test-tree-codec
pixi run --manifest-path mojo/pixi.toml differential-tree
pixi run --manifest-path mojo/pixi.toml differential-protocol
pixi run --manifest-path mojo/pixi.toml differential-control
pixi run --manifest-path mojo/pixi.toml differential-terminal
pixi run --manifest-path mojo/pixi.toml differential-colors
pixi run --manifest-path mojo/pixi.toml differential-layout
pixi run --manifest-path mojo/pixi.toml differential-input
pixi run --manifest-path mojo/pixi.toml differential-client-events
pixi run --manifest-path mojo/pixi.toml differential-terminal-projection
pixi run --manifest-path mojo/pixi.toml test-keyboard
pixi run --manifest-path mojo/pixi.toml keyboard-smoke
pixi run --manifest-path mojo/pixi.toml test-pointer
pixi run --manifest-path mojo/pixi.toml pointer-smoke
pixi run --manifest-path mojo/pixi.toml test-painter
pixi run --manifest-path mojo/pixi.toml test-readiness
pixi run --manifest-path mojo/pixi.toml smoke
pixi run --manifest-path mojo/pixi.toml stream-smoke
pixi run --manifest-path mojo/pixi.toml state-smoke
pixi run --manifest-path mojo/pixi.toml ui-smoke
pixi run --manifest-path mojo/pixi.toml build-app
pixi run --manifest-path mojo/pixi.toml native-smoke
```

Protocol differential checks regenerate vectors with the actual Rust encoder and reject stale checked-in fixtures. They cover native hello and command bytes, maximum 64-bit identifiers, Unicode and escaped arguments, raw non-UTF-8 output, stdout ownership, and server errors. Smoke checks start and stop their own daemon on a private `/tmp` socket, compare command stdout/stderr/exit codes against the Rust CLI, and execute an actual split. UI smoke drives real buttons and text input through mjui's headless driver, checks daemon changes, and saves `target/port/smoke-workspace.png`. Terminal interaction helper differential checks live in `scripts/port/terminal_differential.py`.
