---
type: Subsystem
title: libghostty-vt embedding
description: How zz-terminal embeds libghostty-vt over a pinned Ghostty snapshot, including line-counted scrollback, terminal color-query replies, and single-worker-thread ownership.
resource: crates/zz-terminal/src/session.rs
tags: [libghostty, ghostty, vt, zig, worker-thread, mode-revision, kitty-graphics]
timestamp: 2026-11-24T00:00:00Z
---

# Overview

`zz-terminal` enables its native VT engine through the `session` feature. The workspace
uses the safe wrapper at `third_party/rust/libghostty-vt` with `kitty-graphics` and
defaults disabled. Its adjacent sys crate builds the trimmed C/Zig source in
`third_party/ghostty`. The two crates' `UPSTREAM.md` files record the pins and local changes.
`just vendor ghostty <rev>` updates the native source. The workspace pins Zig 0.16.0 in `mise.toml`.
See [prerequisites](/playbooks/prerequisites.md) for setup.

The pane actor installs the thread-local PNG decoder before creating terminal state.
The wrapper exposes Kitty placements and bounded image storage. Frozen snapshots share
immutable history, copy active pages and decode compressed pages on demand. Live
terminal handles stay on the actor. See the [macOS measurements](/research/2026-09-23-macos-performance.md).

# What libghostty provides

The worker uses these libghostty facilities (imports in `session.rs` and `session/mode_revision.rs`):

| Facility | Types used | Used for |
| --- | --- | --- |
| Terminal state | `Terminal<'alloc,'callbacks>`, `Screen`, `Mode` | VT parsing of PTY bytes, grid + scrollback, primary/alternate screens. |
| Render extraction | `RenderState`, `RowIterator`, `CellIterator`, `CellsCopy`, `Clip`, `Dirty`, `CursorVisualStyle` | `RenderResources::new` starts with a 1×1 clip. `build_snapshot` clips to the requested area and copies rows with `copy_trimmed_into`, leaving trailing default cells to the [frame](/concepts/terminal-frame.md) fill. |
| Cell semantics | `CellWide`, `CellSemanticContent`, `RowSemanticPrompt`, `TrackedGridRef`, `PointCoordinate` | Wide-glyph spacers, OSC 133 prompt/input/output marks, stable scroll-safe references. |
| Key encoding | `key::Encoder`, `key::Event`, `key::Key`, `OptionAsAlt` | Encode [`KeyInput`](/terminal/interaction.md) to terminal bytes (Kitty keyboard aware). |
| Mouse encoding | `mouse::Encoder`, `mouse::Event`, `EncoderSize` | Application mouse reporting when the app requests tracking. |
| Selection | `Selection`, `SelectWordOptions`, `SelectLineOptions`, `FormatOptions` | Word/line boundary selection and copy formatting (soft-wrap unwrap, trim). |
| Kitty graphics | `Graphics`, `PlacementIterator`, `Image`, `DecodePng` | Extract placement geometry and bounded decoded image data without exposing raw terminal handles. |
| Styling / color | `RgbColor`, `StyleColor`, `Underline` | Resolve per-cell fg/bg/underline, inverse, explicit-RGB detection. |
| Formatting / misc | `fmt::Formatter`, `focus`, `ScrollViewport`, `SizeReportSize`, `ColorScheme` | `capture-pane` output, focus reporting, scrollback paging, size/scheme reports. |

The worker also registers libghostty callbacks: `on_pty_write` (terminal responses collected in
`PtyEffects` and drained to the PTY writer), `on_size`, `on_color_scheme`, `on_xtversion`, and
`on_clipboard_write`, which answers every write through the request's `reply` before returning.
Default colors and the full 256-color palette are pushed into every terminal via
`apply_terminal_appearance` before any PTY output is processed.

# Scrollback limit

A pane takes its `history-limit` at spawn. `new_terminal` caps it at 1,000,000 lines.
The byte budget is 128 bytes per cell times the capped line count times the column
count (at least one), bounded to 64 MiB through 1 GiB and recalculated on resize.
Command-output views request 100,000 lines. The startup diagnostics view requests
`64 * 1024 * 1024` lines and hits the one-million-line cap.

Ghostty prunes whole pages. After 30,000 lines at 80×24, `history_limit_counts_retained_lines`
allows 9,000–10,000 retained rows for a 10,000-line limit and 1,000–2,000 for a
2,000-line limit. The September 2026 probe measured 9,883 and 1,609 respectively,
and observed 427 rows at small limits (0–1,000). Those counts depend on page geometry.

# Replies tmux does not give

At this pin libghostty answers some queries the pinned tmux answers differently or not at all, and
it offers no option to turn them off, so zz passes them through: XTGETTCAP (`DCS + q`) is answered from
Ghostty's own terminfo entry whenever a PTY write callback is installed, where tmux stays silent;
DECRQSS answers SGR, DECSTBM, DECSLRM, and DECSCUSR, where tmux answers only the cursor style and
reports every other request as invalid. ANSI DECRQM is answered by both, with tmux knowing only IRM.
Title reports (`CSI 21 t`) stay off, the libghostty default and the pin's behavior
(`title_report_queries_stay_unanswered_like_the_pinned_tmux`).

# Dynamic color queries

Terminal-aware TUIs query the effective foreground, background, and cursor colors with OSC 10, 11,
and 12 before deriving subtle surfaces. `libghostty-vt` answers those queries through `on_pty_write`
using the defaults installed by `apply_terminal_appearance`, and the actor drains the response back
to the child PTY in order. Codex derives its shaded composer row from those replies instead of
falling back to an unstyled prompt. The `terminal_reports_configured_colors_for_osc_10_and_11_queries`
test covers both the query replies and preservation of ST versus BEL terminators.

# The worker-thread ownership rule

libghostty render-state dirty tracking and viewport snapshots are **stateful and single-threaded**.
zz-terminal keeps each live terminal and render state on its pane actor. Owned frozen snapshots
have independent metadata and mutex-serialized access. Consequences:

- `CommandSender` routes control work and PTY-writing input through separate bounded lanes. The actor
  consumes both on one thread, pauses the input lane while `PtyWriter` has a backlog, and keeps
  draining PTY output so full-duplex children can make progress. The actor serializes PTY writes,
  key/mouse encoding, resize, focus, and snapshot extraction (see
  [pty-worker](/concepts/pty-worker.md)).
- Search runs on a separate thread using an owned `ScreenSnapshot` through a mutex. It
  reconstructs one wrapped logical line at a time and never borrows the live terminal.
- `capture()` blocks only the calling client thread and is answered by the actor; terminal state never
  crosses threads.
- `Frames` holds one optional `RenderResources` for the pane. Its `streams` and `areas` maps
  track the stream epoch and requested dimensions separately for each `TerminalViewId`.

# Mode revisions

Copy mode and read-only views freeze the native active screen through
`ModeRevision::capture` in `session/mode_revision.rs`. The owned `ScreenSnapshot` retains
history after source output, pruning, ED3 and source destruction. `CopyGrid` converts only
requested rows and caches at most 64, paired with a style/grapheme dictionary.
`ModeRevisionReader` holds one row and its matching dictionary while rendering, navigating,
capturing plain or VT output, and formatting selections. Dictionary compaction can clear the
cache; readers and published frames retain their own immutable storage.

`HistorySearchSnapshot` retains native backing and decodes logical wrapped lines on demand.
Search maps Unicode byte matches back to physical cells, checks cancellation between rows
and recompresses after 512-row batches and at completion. Empty and single-scalar graphemes
skip the general iterator. Captures append glyphs to the destination without per-cell strings.

Frozen resize reflows owned backing, maps the copy cursor, clears selection and rebuilds
search marks. Appearance updates recolor frozen content; an enabled refresh replaces it with
a new source snapshot. Retained dead panes keep the terminal actor and frozen search state
while releasing PTY and input resources.

# Related

- Owned and driven by [`zz-terminal`](/crates/zz-terminal.md); frames land in the [terminal frame](/concepts/terminal-frame.md) model.
- Encoders back the [interaction](/terminal/interaction.md) subsystem; resolved colors come from [appearance](/terminal/appearance.md).
- The native tmux copy/view modes on top of mode revisions are described in [copy-mode](/tmux/copy-mode.md).
- The [X11 color reference](/references/ghostty-color-reference.md) records the separate provenance of the named-color table.
