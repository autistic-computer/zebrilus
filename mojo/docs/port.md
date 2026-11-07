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

The native workspace uses the adjacent mjui checkout and its portable archives, SDL host, shaders, and fonts. The build wrapper installs generated font/shader links under `target/share/<binary>`; the application resolves that prefix and preserves its launch directory when run from another cwd. The generated links require the adjacent checkout resources. The app has session navigation, window and pane selection, session creation, pane splitting, terminal text input and named keys, a reconnect action, and native terminal cells, colors, graphemes, overlays and cursor. A native worker serializes socket operations and input actions; the UI receives completed model snapshots without waiting for the daemon. Local selection changes carry an epoch so an older worker result cannot replace a newer selection. Pending input has a 4 MiB aggregate bound and a 4,096-action bound.

Each connection owns a persistent native Bend worker. Every daemon frame passes its envelope check before decoding. Tree deltas and client-view overlays use the same native worker within a 2 MiB normalized input bound; larger graphs retain the typed Mojo reducer. The adapter resolves compact, semantically typed metadata references before committing a successful complete tree. Rejections preserve the old tree and request a resynchronization. Mojo validates terminal payloads before Bend performs retained-cell mutation; Mojo commits the returned cells, dictionaries and metadata after a successful verdict. Linux exchanges cells through three reusable anonymous memory files, with a separate result buffer; other targets use private temporary files. Cell mutation is bounded to 64 MiB of retained cells. Metadata-only patches use the proved identity path, and larger retained grids use the validated Mojo mutation path with Bend structural preflight. `ZZ_BEND` overrides the worker executable; otherwise the client finds `zz-bend` next to its own executable. Protocol size bounds are checked before allocation. Bend proofs cover the named pure-core contracts, not binary file I/O, the Mojo transport, UI, or operating system.

The stream decodes full and patch frames, command-output viewports and history chunks from the packed terminal lane. Native grids use ABI-checked records; row caches include cell, dictionary and style changes. The color resolver preserves the reference's faint rounding, adaptive contrast and decorative Unicode exemptions. All visible split panes render at their measured grid sizes, independently of the daemon's normalized retained dimensions. Row, cursor and overlay projection follow the source placement rules. Pane-size reports carry the layout generation. Session/window/pane data comes from the typed interactive snapshot stream. Linux uses one-shot socket readiness watches while idle, with worker completion checked during active work; the portable fallback polls. Native key routing preserves press ownership, prefix claims and associated committed text without duplication. Pointer routing uses actual painted grid geometry, preserves captured pane ownership, forwards native mouse packets, and handles selection, wheel fractions, hover handoff and timed selection autoscroll. Status, notification, terminal-negotiation and command-output watermark reduction retain typed state. Warm history renders from retained rows with fractional local scrolling, bounded backfill and server convergence; reconnects retire row caches before revision identifiers can repeat. Further selection/search chrome, browser/agent/editor renderers, settings, command palette, SSH transport and desktop integration remain in progress. Other event payloads are retained for their remaining reducers. Both command and interactive Hello publish bounded native cwd/environment context, preserving raw environment bytes and a valid logical `PWD`. The CLI uses modern execution and native file/stdin streams; the app retains legacy command RPC for its control actions.

Command output replaces the owning pane content and uses a separate widget identity for each daemon output ID. Closing or replacing output releases the old widget key and row cache. Its top-right mode indicator follows the source labels, detail text and geometry through the shared floating-chip paint adapter. The kit’s remaining glass-material and raster compatibility limits apply to this chrome. Its native resize reports target the output terminal, and its mouse, selection, scroll, focus and paste actions use `CommandOutputView`. Local output input focus does not select a daemon pane; input from another pane parks the output key table while the daemon retains the output, and closing output restores ordinary pane input. Snapshot-changing daemon commands can retire output when the client context leaves its owner. Both terminal surfaces preserve the source distinction between uncomposed multi-scalar or newline paste and composed IME text, including composition begun and committed in one host frame. Text is queued before wire encoding so a valid 4 MiB paste retains its full payload allowance. Copy-mode key bindings own dismissal. Output search controls and the remaining terminal actions are part of the broader terminal UI work.

Clipboard notices retain pane, request ID, target, producer and sequence. Nonempty CLIPBOARD writes always reach that clipboard; supported PRIMARY writes keep their target, and only server-produced selections mirror PRIMARY into CLIPBOARD when `set-clipboard` is enabled. Zero request IDs remain valid. Notices use a separate 64 MiB/4,096-event mailbox, and clipboard queries coalesce into one separately bounded 32 MiB reply. Replies and queued native input retain their connection identity so reconnects cannot replay them onto a new client. Native clipboard reads enforce byte limits before copying into Mojo; CLIPBOARD offers preserve embedded NUL bytes. Ctrl-Shift/Cmd copy, paste and select-all preserve prefix/modal claims and consume their associated text and host edit commands once; standalone host menu commands retain their own action. A bounded input timeline preserves the order of buffered commits, host commands and physical keys, including multibyte text. Supported PNG offers take priority over text for ordinary panes and use two bounded 6 MiB upload slots, native 1 MiB chunks and a 10 ms pacing deadline. Output surfaces retain text-paste behavior. Image upload follows the current native application path, which does not cancel local scroll when an upload begins. Other native image formats and image presentation remain broader terminal work.

Mojo 1.1.0 is resolved in `pixi.lock`. The language skill describes newer `match` and `if case` syntax that this compiler rejects; this target uses the compiler's supported conditional dispatch and guarded Optional access.

Build and test tasks use the shared build guard in `scripts/port/guard.py`. Linux builds run with a maximum 4 GiB memory budget and at most 256 MiB of swap; test suites run sequentially. The guard reserves 2 GiB of available device memory at startup and stops its own scope if available memory falls below 1 GiB. The native wrapper selects system Clang when available; `MJ_CC` and `MJ_CXX` select compatible compiler executables explicitly.

Native rendering builds use `--fp-mode=contract=off` to preserve the reference's floating-point operation boundaries. The native foreground color resolver is checked with exact RGB values rather than a pixel tolerance.

Verification:

```sh
pixi run --manifest-path mojo/pixi.toml test
pixi run --manifest-path mojo/pixi.toml test-tree-codec
pixi run --manifest-path mojo/pixi.toml differential-tree
pixi run --manifest-path mojo/pixi.toml differential-protocol
pixi run --manifest-path mojo/pixi.toml test-client-context
pixi run --manifest-path mojo/pixi.toml differential-client-context
pixi run --manifest-path mojo/pixi.toml differential-terminal-appearance
pixi run --manifest-path mojo/pixi.toml test-terminal-settings
pixi run --manifest-path mojo/pixi.toml differential-terminal-settings
pixi run --manifest-path mojo/pixi.toml differential-client-files
pixi run --manifest-path mojo/pixi.toml test-client-files
pixi run --manifest-path mojo/pixi.toml client-files-smoke
pixi run --manifest-path mojo/pixi.toml differential-exec
pixi run --manifest-path mojo/pixi.toml test-exec-context
pixi run --manifest-path mojo/pixi.toml exec-smoke
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
pixi run --manifest-path mojo/pixi.toml test-command-output
pixi run --manifest-path mojo/pixi.toml differential-command-output
pixi run --manifest-path mojo/pixi.toml command-output-smoke
pixi run --manifest-path mojo/pixi.toml test-ime-provenance
pixi run --manifest-path mojo/pixi.toml test-clipboard
pixi run --manifest-path mojo/pixi.toml differential-clipboard
pixi run --manifest-path mojo/pixi.toml clipboard-host-smoke
pixi run --manifest-path mojo/pixi.toml clipboard-workspace-smoke
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

The command-output smoke opens real output on its private interactive client, checks owner-pane substitution and output resize, parks and restores its input focus through rendered pointer surfaces, exercises native paste/IME and copy-mode dismissal, and verifies exact underlying PTY input bytes. It saves `target/port/command-output-workspace.png`. The SDL provenance test uses native text-editing/text-input events with the dummy video driver; the matching AppKit assertions live in mjui’s `test_textinput` suite and require macOS.

Clipboard differential tests execute the current Rust serde, chrome keymaps and private clipboard policy methods with isolated host adapters. Host tests use SDL's dummy video driver, so they do not read or change the user's clipboard. The private-daemon workspace test checks actual selection copy, query/reply ownership, application OSC 52 PRIMARY, exact bracketed-paste/IME PTY bytes, interleaved physical/menu paste and raw-key ordering, and a valid multi-chunk PNG upload. The AppKit assertions use a private pasteboard and require macOS.

The typed appearance codec retains every font, color, palette, cursor, padding, contrast and provenance field. Its source differential checks complete roundtrips, exact float bits and validation boundaries. Font localization and style requests match the native selection rules, including provenance filtering, ordered fallbacks and features, and weight/zoom bounds. Applying all appearance options and bounded font replacement to the workspace remains in progress.

Command clients answer native file read/write/append and whole/chunked stdin requests. The 128 source codec cases include every error variant's terminal message and exit status, malformed fields, and exact 32 MiB wire boundaries. Fifteen direct I/O groups check byte bounds, raw files, spent stdin, closed descriptors and signal restoration; 21 private-daemon groups compare the Rust CLI, including over 2 MiB of streamed input into an empty pane. Whole stdin is limited to 1 MiB; files above 32 MiB produce a bounded error locally. These runtime checks were run on Linux.

The CLI uses modern execution requests and preserves native command separators, raw arguments, streamed output/error ordering, stdout ownership and cumulative exit codes. Its 937 source codec cases cover all flags and optional facts, full invocation metadata, prepared resume results, process environments and malformed/truncated frames. Three environment tests check raw ordering/duplicates, entry/byte limits and locale precedence. Twenty-six private-daemon groups cover chains, source-file, stdin reuse, direct split streaming, logical cwd/environment, closed standard streams, blocked execution termination and server shutdown. Closed stdin is recorded before missing standard descriptor slots are reserved. Interactive resume launching, remote SSH and the full global CLI option parser remain in progress.

Terminal rendering now has an opt-in native appearance path: exact Float32 RGBA/HSLA colors, full-source rounded-selection corners retained across viewport clipping, link-hover fill and one-device-pixel underline, copy-mode row/cell fills, and native cursor shape/blink/composition behavior. The actual Rust helper differential covers 65,536 color conversions, 189 height adjustments, 1,512 grid metrics given measured font inputs, 1,952 corner cases, 1,536 overlay colors, 64 visibility combinations and 160 cursor paints. Local painter regressions cover clipped neighboring selection spans and fixed device geometry at 1x/2x. `differential-terminal-render`, `differential-terminal-overlays` and `test-terminal-render-state` run the bounded checks. Application font replacement/appearance activation, selection-foreground reshaping, terminal glyph positioning and full terminal pixel identity remain unfinished; protocol settings retained in the model do not establish rendering parity.
