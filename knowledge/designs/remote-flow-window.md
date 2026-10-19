---
type: Design Plan
title: Adaptive in-flight window for remote attach
description: Keystroke echo no longer waits behind seconds of screen data over ssh - the daemon pings inside its own output stream, the client answers, and the daemon keeps unanswered bytes under measured rate times RTT, so frames coalesce in the mailbox instead of queueing in sshd and TCP; local unix clients keep the old path. Built on protocol v110 with two-machine measurements; in review as a PR.
status: "In review 2026-10-19 on branch flow-window, protocol v110: done-when met over ssh, local perf gate green; to be tested before merging to main"
tags:
- remote
- ssh
- latency
- flow-control
- protocol
- daemon
- design-plan
resource: crates/zz-daemon/src/daemon.rs
timestamp: 2026-10-18T00:00:00-03:00
---

# Overview

On 2026-10-18 we measured zz's remote link (bare `ssh -L` with zz's flags) against QUIC between
two machines, with `tc netem` playing cellular. On good networks ssh is not the problem: idle
round trip and throughput matched QUIC. The lag is buffering. Everything rides one ordered stream
and the daemon hands frames to its socket as fast as the socket takes them, so during an output
flood a keystroke echo waits behind seconds of screen data that is already in the pipe.

Keystroke echo p50 while the server floods output on the same stream:

| link | ssh | QUIC | either, ≤32 KB in flight |
|---|---|---|---|
| 85 ms RTT, 1% loss, 20/5 Mbit | 28.5 s | 7.5 s | ~117-120 ms |
| 60 ms RTT, 0.1% loss, 20/5 Mbit | 2.6 s | 2.9 s | ~67 ms (the bare RTT) |

Capping unacknowledged bytes fixes it on both transports, and a fixed 128 KB cap still lagged
about 0.7 s on the harsh link and 120 ms on the LTE one. The cap has to follow the link. This
design adds that cap: the daemon puts a small ping into its own output stream, the client echoes
it back, and the daemon stops handing bytes to the socket once the unanswered ones reach the
measured throughput times RTT, plus a quarter. Frames that cannot go out yet wait in the mailbox
that already keeps only the newest frame per pane.

# How frames flow today

Read at `323283d16` (2026-10-18).

**Daemon.** A pane's frames reach a client's `OutboundMailbox` (`crates/zz-daemon/src/daemon.rs`)
through `PaneSink::deliver` (`daemon/shard_sink.rs`) or, off the fast path, through
`publish_loop_view` and `Shared::publish_terminal_for_pane`. `OutboundState.terminals` holds one
pending frame per pane. A new frame for a pane that already has one gets `NeedsFull` from
`enqueue_terminal_with`, and the caller swaps the slot for a full viewport of the newest state
(`replace_terminal_viewport`): newest wins. `pop_ready_frame` drains, in order, the reliable deque,
the command-output slot, agent frames one pane per turn, then terminal slots one pane per turn. The
event loop's writer, `Connection::write_ready` (`daemon/event_loop.rs`), pulls up to 256 KiB with
`try_recv_batch` and writes until the socket says `WouldBlock`. When the mailbox is empty, an
interactive client's frames skip the loop and go straight to the socket
(`write_direct_terminal`).

**Why coalescing never starts over ssh.** The newest-wins slot only fills while the writer is
blocked, and the writer only blocks when its socket refuses bytes. Over ssh the daemon's peer is
sshd, which reads the unix socket as long as the ssh channel window (about 2 MiB) and its TCP send
buffer (`tcp_wmem` max 4 MiB here) have room. At the 0.15 MiB/s a 1% loss link sustains, that much
buffer is tens of seconds of screen, which is the table's first row.

**Clients.** Nothing goes back per frame. The only feedback is `RequestFull` after a patch fails to
apply and `Resync` after an undecodable frame. All native clients read through
`InteractiveClient::recv` / `try_recv` (`crates/zz-daemon-client/src/client.rs`): the desktop's
`zz-mux-reader` thread feeds a depth-1 channel (`crates/zz/src/mux/client.rs`), the TUI drains
the socket into a newest-per-pane `FrameInbox` (`crates/zz-tui/src/app.rs`), and the iOS app
reads on its own thread (`clients/app/src/transport.rs`). `InteractiveClient` learns at connect
time whether its endpoint is `Endpoint::Local` or `Endpoint::Ssh`; the Unix desktop forwards a
socket with `ssh -L`, Windows and iOS run `zz proxy` over an ssh exec channel. No RTT is measured
anywhere ([zz crate](/crates/zz.md) notes predictive echo would need an application ping).

**Leftovers of [scene streaming](/designs/scene-streaming-remote.md).** `RequestFull`, the
history ring and reconnect are there and stay as they are. The deleted QUIC per-frame streams with
RESET supersession were the earlier answer to this same problem. This design does not need loss
handling: the stream stays reliable and ordered, only the daemon's pace changes.

# Design

## Wire (v110)

v109 shipped in zz 0.18.0, so this takes v110. Two `ProtocolMessage` variants, appended at the end
of the enum, on the control lane:

| message | direction | fields |
|---|---|---|
| `Ping` | daemon → client | `token: u64`, `rtt_us: u32` |
| `Pong` | client → daemon | `token: u64` |

- `token` is the stream offset just past the ping. A pong for token *t* means the client has every
  byte before *t*. The client only echoes it, so it never counts bytes.
- `rtt_us` is the daemon's current minimum RTT (0 before the first sample). Nothing reads it in
  v1. A predictive echo gets the link RTT from it with no extra round trips.
- A client hello capability, `flow-window-v1`, next to `TTY_INPUT_CAPABILITY`. A client that sends
  it must answer every ping.
- About 20 bytes each way per ping.

## Who opts in

- **Client.** `InteractiveClient`'s hello carries `flow-window-v1` when it connected through
  `Endpoint::Ssh`. Local endpoints never send it. `ZZ_FLOW_WINDOW=on|off` overrides the choice, so
  tests can force it on a unix socket and the remote bench can run before and after on one build.
- **Daemon.** A hello with the capability gives the connection's mailbox a `FlowWindow` and leaves
  its direct socket unset, so the event loop's writer is the only writer. Without the capability no
  new code runs: no pings, no gate, same direct writes. Local clients keep today's exact path, and
  `just perf gate` checks that.

## Client: answer pings

`InteractiveClient::recv` and `try_recv` answer a `Ping` with a `Pong` through their own sender as
soon as they decode it, and don't hand the ping to the caller. That one place covers the desktop,
the TUI and iOS. The pong leaves when the reader thread decodes the ping. The desktop reader
blocks on its depth-1 channel while the UI catches up, so a slow UI also holds the window shut.
That is the behaviour we want: the daemon coalesces instead of the kernel queueing.

## Daemon: the gate

`OutboundState` gets `flow: Option<FlowWindow>`.

- **In flight** = bytes written to the socket plus bytes already batched, minus the last acked
  offset.
- **Gate.** `try_recv_batch` stops popping once in flight reaches the window, but only while a
  ping is out: with every ping answered there is nothing left to wait for, so the writer sends,
  and that batch carries a new ping. The window can therefore never get stuck, whatever it shrinks
  to. A frame that starts under the window still goes out whole.
- **What it gates.** The whole outbound stream: reliable, command output, agent, terminal. It only
  pauses popping, so it reorders nothing. Terminal slots keep coalescing through the existing
  `NeedsFull` → `replace_terminal_viewport` path. Agent frames queue under their existing per-pane
  share and `AgentLagged` replay. Reliable messages wait in the mailbox deque instead of sshd's
  buffer.
- **Pings.** Inside a batch, the writer puts a `Ping` after the frame that takes the stream
  max(window / 4, 4 KiB) past the last ping, so a full window carries about four. Pings sit between
  frames rather than at the end of the write: a small last segment waits in TCP for the ACK of
  everything before it (Nagle, plus the peer's delayed ACK), and a ping stuck there reads two RTTs
  too long. A batch that starts into an empty pipe also ends with a ping. That one gives a clean RTT
  sample: during a flood every ping has a window of bytes ahead of it and measures queue as well as
  distance, while a keystroke's small frame after a quiet moment goes out alone. A batch that pops
  nothing never pings.
- **Pongs.** `connection.rs` hands `Pong { token }` to `OutboundMailbox::acknowledge`, which moves
  the acked offset and updates the estimate below. The loop calls `write_ready` after it reads a
  connection, so a window that a pong opens flushes in the same turn. While the gate is shut the
  writer drops write interest, so a blocked connection costs no wakeups.

## Window estimate

All state lives in `FlowWindow`, a plain struct with no I/O, so it is tested on its own.

- **min_rtt**: the lowest (pong time − ping send time) on this connection. A lifetime minimum
  fails safe: if the path gets slower, the window comes out too small (less throughput), never too
  big (lag). A reconnect starts over.
- **Delivery rate per pong**: (token − the acked offset when that ping was sent) / (now − the time
  of that ack). This is BBR's ack-rate sample. Its interval spans at least one RTT, so pongs that
  arrive bunched together skew it far less than a pong-to-pong rate would.
- **max_rate**: the highest of the last 8 samples whose ping saw the gate shut while it was in
  flight, which is when the sender was limited by the window. Any other sample counts only if it
  raises the window, so idle typing never shrinks it. An earlier rule called a round limited when
  half a window was in flight; on wifi that caught ordinary traffic during RTT spikes, the samples
  read the spike as low capacity, and the window sank until it gated a link that had room (measured
  2026-10-18: +15-40 ms echo, -30% frames).
- **window** = clamp(max_rate × min_rtt × 5/4, 16 KiB, 8 MiB). It is 64 KiB before the first
  sample.

Why 5/4: while the link has spare capacity, each round's window is 1.25× the bytes the last round
delivered, so the window grows 25% per RTT. Once the link is full, the window sits 25% above the
bandwidth-delay product, a quarter RTT of queue. Echo then costs about one RTT, plus that quarter,
plus the time to send one frame, plus the gap between pings.

Measured over ssh, the window settled near 80 KB on both the LTE and the harsh profile: about
0.7-1 MB/s of zz bytes (ssh compresses them further) times a 66-88 ms minimum RTT. Gain 2, BBR's
congestion-window gain, was also measured; see Results for why 5/4 stays.

## What stays the same

- Local unix clients and command clients: no capability, no new code.
- Message order, `RequestFull`, `Resync`, the history ring, reconnect.
- The 72 MiB `MAX_OUTBOUND_BYTES` cap and "too far behind": a slow remote fills the mailbox a few
  MiB sooner than it does today, because sshd no longer holds that part.

## Not in v1

- **Browser egress on the same TCP.** `ssh -D` rides the same ssh process as `-L`, so a remote
  browser page load fills the shared TCP queue and the echo waits behind it. Fix: give egress its
  own ssh connection.
- **Upstream.** Nothing paces client → daemon traffic, so a paste upload can bloat the 5 Mbit
  uplink.
- **Web client.** zz-web passes frames through to a WebSocket. The WASM client would need to send
  the capability and answer pings.
- **Windows daemon.** Its writer uses `recv_batch` and ignores the capability.
- **Replacement frames are full viewports.** A coalesced slot always becomes a full frame. A patch
  against the last delivered viewport would be smaller. Measure before doing it.
- **Mailbox priority.** Terminal slots pop last, so a lot of agent or reliable traffic queued in
  the mailbox goes out ahead of an echo frame.
- **`ssh host zz attach`.** A TUI running on the remote host writes a plain tty stream that zz
  cannot pace.
- **A link with room but a jittery RTT.** With mono floods on the harsh profile, which fit the link,
  the 5/4 window still shut on RTT spikes and cost 20-30 ms of echo p50 (125-135 against 103-106
  ms). Pacing sends at the measured rate, with a larger window for slack, is BBR's answer; it needs a
  timer-driven writer.

# Measurement plan

- **Local.** `just perf gate` before (`323283d16`) and after; `cargo ci-test`; clippy, including
  the no-agent daemon lint.
- **Unit.** `FlowWindow` tests (`daemon/flow_tests.rs`): growth by 1.25× per round on a link with
  room, settling at 1.25× BDP, a rate drop, the 16 KiB floor, idle samples never shrinking the
  window. Mailbox tests: the gate holds frames until a ping inside the window is answered, a window
  with no ping out never waits, two frames enqueued while it is shut come out as one full frame, and
  a mailbox without flow neither pings nor waits.
- **Remote.** `bench/perf/pane/flood.py` redraws every row but the last as fast as it can, with
  `color` giving every cell a random 256-color foreground, and keeps a six-letter token for the
  last key on the last row. Consecutive tokens differ in every cell, so the client redraws them as
  one run of text even when frames coalesce. `bench/perf/remote_echo.py` attaches `zz_cli
  attach-session` in a pty with `ZZ_TUI_RELAY=1`, sends keys, and reports echo p50, p90 and max plus
  the tty bytes per second. The client dials a manual `ssh -N -L` with zz's own flags rather than
  `--host`, because `--host` gates on the protocol version of the zz on the remote login PATH, which
  is the installed release. "Before" is the same build with `ZZ_FLOW_WINDOW=off`.
- **Visibility.** The daemon logs `zz_daemon::flow` at debug once a second per flow client:
  min_rtt, the last RTT and rate, max_rate, window. Run it as `RUST_LOG=warn,zz_daemon::flow=debug
  zz_cli --verbose daemon`. Plain `--verbose` logs every frame and wrote 97 GB in seven minutes of
  flood.

# Done when

- Echo p50 during a flood is within ~1.5× the round trip on the 60 ms / 0.1% profile, over ssh.
- `just perf gate` is green.
- The Results section below holds the before/after numbers for both profiles.

# Results

## Loopback rehearsal (2026-10-18, alienware, one machine)

Daemon and client on alienware, joined by two `socat` relays over a TCP port that `tc netem` throttles
on `lo`. `bench/perf/remote_echo.py` drives `zz_cli attach-session` in a 120x40 pty with
`ZZ_TUI_RELAY=1`, so keys travel over the socket, against a `bench/perf/pane/flood.py` pane. "Off" is
the same build with `ZZ_FLOW_WINDOW=off`. This is not the two-machine run: loopback's 64 KiB MSS
turns every 8 KiB relay write into a small segment, so Nagle plus delayed ACK added about 125 ms to
every round trip until the relays got `nodelay`. Numbers below are with `nodelay`.

| link | RTT (measured min) | flow off p50 / p90 | flow on p50 / p90 | window settled at |
|---|---|---|---|---|
| none | 0.1 ms | 41 / 42 ms | 9.8 / 10.2 ms | 64 KiB |
| 30 ms each way, 0.1% loss, 20/5 Mbit | 62 ms | 65 / 65 ms | 65 / 80 ms | ~25 KiB |
| 40 ms each way, 1% loss, 1.5/5 Mbit | 107 ms | 16 481 / 17 180 ms | 179 / 180 ms | ~25 KiB |

The 20 Mbit row shows no bloat with the window off because this flood makes about 300 KB/s of
frames, under the link. The 1.5 Mbit row is the case the design is for: the frames outrun the link,
and the window turns a 16 second echo into 1.7x the RTT, with one 120x40 frame's transfer time as
the floor.

## Local gate

`just perf gate` on alienware, 2026-10-19, on the final build: green, 3 pass, 0 fail, 0 regressed,
0 drifted (460 s). Detached ASCII 108.7 MB/s against a 109.2 MB/s pty ceiling, Unicode 87.2 MB/s,
attached ASCII 1300 ms, `echo.wire_bytes` 33 / 149 B as before.

A first run on 2026-10-18 failed three throughput rules against W0. The host was slow, not the
change: the gate's own pty ceiling (`cat` through a cooked pty, no zz) read 54 MB/s that day
against 109-131 MB/s on other days, with the CPU at 1.6 GHz. An A/B against `323283d16` built the
same way found no regression: an interleaved main, flow, flow, main run of the cli, chatty, mem,
echo and control groups put every row within noise, and 300 alternating `list-panes` per build gave
2.66 / 3.24 ms on main against 2.57 / 2.74 ms on flow.

## Two machines over ssh (2026-10-18)

The daemon ran on the macbook (192.168.1.173) and the client on alienware, over a forwarded socket
from `ssh -N -L` with zz's flags (`Compression=yes`, `ControlMaster=auto`, keepalives). `tc netem`
on alienware's `wlan0` shaped only traffic to and from the Mac: frames inbound at 20 Mbit, keys
outbound at 5 Mbit, the delay and loss both ways. Wifi power save was off during the runs. Bare
ping through the throttle: 64-67 ms minimum on LTE, 85-87 ms on harsh, with wifi jitter on top
(averages 74-112 ms). 20 keys per row, two rows per cell, each pair given as both values.

Colored flood, where frames outrun the link:

| profile, pane | flow off p50 / p90 | flow on (5/4) p50 / p90 | gain 2 p50 / p90 |
|---|---|---|---|
| LTE 240x70 | 151, 250 / 231, 729 ms | 108, 101 / 239, 346 ms | 190, 130 / 308, 191 ms |
| LTE 120x40 | 83, 85 / 158, 125 ms | 86, 81 / 215, 137 ms | 83, 85 / 211, 171 ms |
| harsh 240x70 | 1142, 759 / 1426, 1927 ms | 242, 233 / 438, 545 ms | 315, 294 / 513, 524 ms |
| harsh 120x40 | 328, 402 / 1172, 1534 ms | 136, 223 / 604, 297 ms | 169, 229 / 505, 414 ms |

Mono flood (digits and symbols, no color):

| profile, pane | flow off p50 / p90 | flow on (5/4) p50 / p90 | gain 2 p50 / p90 |
|---|---|---|---|
| harsh 240x70 | 115, 103 / 226, 187 ms | 137, 139 / 203, 258 ms | 164, 121 / 232, 238 ms |
| harsh 120x40 | 103, 106 / 187, 262 ms | 135, 125 / 231, 236 ms | 95, 109 / 144, 281 ms |
| LTE 240x70, earlier half-window rule | 81, 96 / 157, 235 ms | 97, 92 / 155, 181 ms | not run |

Findings:

- **The done-when holds.** On the LTE profile with a flood that fills the link (colored 240x70),
  flow on gives an echo p50 of 101-108 ms, 1.3-1.6x the RTT; flow off gives 151-250 ms with a p90
  up to 729 ms. On harsh, flow on cuts p50 from 0.76-1.14 s to 233-242 ms and max from 1.6-2.2 s
  to under 0.85 s.
- **zz rarely bloats the way a raw byte flood does.** The daemon already coalesces to its frame
  clock, and ssh compresses the frames, so a mono 120x40 or 240x70 flood fits a 20 Mbit link even
  at 1% loss (0.4-1.6 MiB/s of tty output). The 28.5 s and 2.6 s rows in the overview came from a
  raw stream; with zz frames the bloat shows when frames are large and incompressible (color, big
  panes, several flooding panes) or the link is slow.
- **5/4 beats gain 2 whenever the link is full**, by 6-80 ms at p50. Gain 2 only helped the mono
  harsh 120x40 case, where the link had room. 5/4 stays; the cost it leaves is under Not in v1.
- **The window costs frames, not freshness.** With flow on, the client draws fewer intermediate
  frames during a flood (tty output up to a third lower on saturated rows); each one it draws is
  the newest.
- **No Nagle stall on real ssh.** Idle pings measured the bare RTT (66.6 ms on LTE, 87.6 ms on
  harsh) and pongs never lagged by a delayed-ACK multiple. On loopback the same code needed
  `nodelay` relays, which was the 64 KiB MSS, not ssh.

# Decisions

Taken at approval on 2026-10-18, as proposed: the window paces the whole outbound stream, `Ping`
carries `rtt_us`, and `ZZ_FLOW_WINDOW=on|off` stays as an override.

# Related

- [Scene-streaming remote attach](/designs/scene-streaming-remote.md): the earlier remote plan
  whose QUIC half was deleted
- [Fleet attach](/designs/fleet-attach.md): the ssh endpoints this paces
- [Remote browser egress](/designs/remote-browser-egress.md): the `ssh -D` traffic that shares the
  link
- [zz wire protocol](/protocol/wire-protocol.md): the version history v110 joins
- [Session persistence](/concepts/session-persistence.md): the newest-wins mailbox
