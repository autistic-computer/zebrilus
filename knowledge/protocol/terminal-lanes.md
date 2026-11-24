---
type: Protocol
title: Packed terminal lane
description: Terminal frame kinds, row encoding, limits, per-pane delivery and repair.
resource: crates/zz-protocol/src/pane_frame.rs
tags: [protocol, terminal, wire, packing, fanout]
timestamp: 2026-11-24T00:00:00-03:00
---

# Frame kinds

Lane 1 of the [wire protocol](/protocol/wire-protocol.md) carries grid data. The payload
starts with a one-byte kind, then the pane ID and sequence as unsigned varints. The
router is `encode_protocol_message_into_inner` in `crates/zz-protocol/src/terminal_codec.rs`.
The layout is implemented in `crates/zz-protocol/src/pane_frame.rs`.

| Kind | Body | Decoded event |
| --- | --- | --- |
| 0 | Full viewport | `TerminalViewport` |
| 1 | Patch against a named base | `TerminalPatch` |
| 2 | Nonzero output ID followed by a full viewport | `CommandOutput { viewport: Some(..) }` |
| 3 | Start, total, offset, columns, row count, dictionary, row records | `HistoryChunk` |

An empty command-output close goes over Control. A viewport embedded in a Control message,
such as a chooser preview, uses the compact viewport body inside a postcard byte string.

## Viewport and patch layout

A full viewport carries the view generation, its wrapping difference from the terminal
generation, the dictionary generation, columns, rows, a field mask, metadata, style and
grapheme dictionaries, then the row records. Omitted metadata takes its default. Omitted
cells are `PackedCell::EMPTY`.

A patch carries the base view generation, its difference from the base terminal generation,
the view and terminal generation changes, dictionary generation, dimensions, and field mask.
Wrapping differences and signed shifts use zigzag varints. A patch can shift rows, replace
metadata, append to the dictionary and replace a span in each changed row. Omitted metadata
keeps its prior value. Dictionary appends carry base counts. Newly exposed rows after a
scroll must be covered or cleared.

`TerminalPatchFields` in `crates/zz-terminal/src/model.rs` defines the 15 field bits.
`encode_patch_head` and `encode_patch_tail` in `pane_frame.rs` specify their wire order.
A cursor move has a short position-only form. A complete cursor record takes precedence.
Kitty placement updates include a scrollbar so placement rows can be validated.

## Rows and runs

A row or span stores a positive row step and a header `(start_column << 1) | clear`.
Zero ends the row list. Runs have a varint header `(count << 3) | (has_style << 2) | kind`.
A style change is followed by a style ID. Each row begins in style zero. A zero run header
ends the row. A patch's clear bit fills the unencoded tail with empty cells.

| Run kind | Payload |
| --- | --- |
| 0, text | One UTF-8 scalar per narrow cell |
| 1, wide | One scalar per wide head and matching spacer-tail pair |
| 2, repeat | One glyph code and flags for the run |
| 3, raw | A glyph code and flags for each cell |

The low bit of a glyph code selects a grapheme index (1) or scalar (0). The encoder can use
repeat runs for six or more identical cells. Each style stores colors plus the optional
underline, attributes and color classes indicated by its flags. Grapheme lengths precede a
contiguous byte arena. See `encode_runs`, `encode_styles` and `encode_full_graphemes`.

## Bounds

A sparse frame can describe a grid much larger than its payload. The codec caps a grid at
`MAX_FRAME_BYTES / 8` cells (8,388,608) and a history reply at 512 rows. History rows must
match the advertised width. Other bounds include 65,536 styles, 1,048,576 graphemes, 16 MiB
of grapheme bytes, 64 KiB of title and 16 KiB each for the working directory and hovered URI.
The decoder rejects unknown field bits, invalid dictionary references, malformed or
out-of-order spans, invalid shifts and trailing bytes.

## Delivery and repair

The daemon can replace queued terminal frames for the same pane with newer output.
`PaneFrameFanout::enqueue` reuses encoded patches for matching base/current transitions.
`TerminalFrames::full` caches full frames by pane, generation and cell storage.
Foreground and preview delivery have different budgets. Preview subscriptions can show
terminals beyond the focused window. Input, geometry and on-demand history use the
requester's visible scope. See [daemon publication](/crates/zz-daemon.md).

A receiver that cannot apply a patch sends `RequestFull { pane }`. The response supplies its
new base. To fetch older rows it sends `HistoryRequest { pane, start, count }`. The ring and
its request pacing live in [zz-client](/crates/zz-client.md). The `history-trickle` option
bounds background backfill (default 2,000 rows, maximum 10,000, zero disables backfill).
See [terminal frames](/concepts/terminal-frame.md) for retained cell and dictionary ownership.

`cargo test -p zz-protocol pane_frame` exercises encoding, round trips and malformed frames.
Changes to packing require a [wire epoch change](/protocol/wire-protocol.md#wire-epochs).
