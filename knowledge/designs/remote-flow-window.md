---
type: Design Plan
title: "Adaptive in-flight window for remote attach"
description: "The daemon limits unacknowledged output so fresh terminal frames can replace queued frames before they enter the network."
status: "Implemented, verified against source 2026-11-24"
tags:
- remote
- ssh
- latency
- flow-control
- protocol
- daemon
- design-plan
resource: crates/zz-daemon/src/daemon/flow.rs
timestamp: 2026-11-24T00:00:00-03:00
---

Output floods can delay a keystroke echo after the bytes have entered an ordered network stream. With the flow window negotiated, the daemon sends pings in the output stream. Acknowledgments tell it how many bytes arrived and how long delivery took. While the window is full, new terminal frames can coalesce in the mailbox.

`FlowWindow` in `crates/zz-daemon/src/daemon/flow.rs` starts at 64 KiB. It estimates the window as 5/4 of delivery rate times the minimum observed RTT, bounded to 16 KiB through 8 MiB. When blocked, it uses the best of the last 40 limited rate samples. Unblocked traffic can increase the estimate. `flow_window_wanted` in `crates/zz-daemon-client/src/client.rs` controls whether an interactive client requests the capability. `ZZ_FLOW_WINDOW=on|off` overrides the endpoint default.

# Measured behavior

In the 2026-10-18 two-machine SSH test, a 240×70 colored flood on the 60 ms / 0.1% loss profile gave echo p50 of 101–108 ms with pacing and 151–250 ms without. On the 85 ms / 1% loss profile, the corresponding figures were 233–242 ms and 759–1,142 ms. The more dramatic multi-second delays in the original transport experiment came from an unframed byte flood.

The test used `bench/perf/remote_echo.py`. Historical throughput figures included warmup bytes in the counter and require a fresh measurement. RTT and separately timed echo samples were unaffected. Outbound pacing does not bound client uploads or browser egress sharing the SSH connection. See [remote egress](/designs/remote-browser-egress.md) and [terminal lanes](/protocol/terminal-lanes.md).
