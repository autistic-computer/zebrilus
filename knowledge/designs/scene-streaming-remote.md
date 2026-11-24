---
type: Design Plan
title: "Remote terminal frames and history"
description: "The retained parts of the 2026 scene-streaming design are local rendering, history backfill and per-pane frame repair."
status: "Original QUIC transport and predictor retired 2026-08-01"
resource: crates/zz-protocol/src/terminal_codec.rs
tags:
- remote
- streaming
- quic
- mosh
- prediction
- roaming
- design-plan
timestamp: 2026-11-24T00:00:00-03:00
---

Terminal clients receive screen state and draw it locally. A receiver missing its base can request a full frame for one pane with `RequestFull`. History backfill uses `HistoryRequest` and `HistoryChunk` independently of that repair. The packed viewport and history records live in `zz-protocol/src/pane_frame.rs`. See [terminal lanes](/protocol/terminal-lanes.md).

The original July 2026 transport used per-frame QUIC streams, zstd negotiation, direct device pairing and speculative local echo. Those experiments ended on 2026-08-01. For current transports see [fleet attach](/designs/fleet-attach.md) and [tailnet nodes](/designs/tailnet-node.md). For interaction latency see the [remote flow window](/designs/remote-flow-window.md). The terminal attach client has its [own design](/designs/tui-client.md).
