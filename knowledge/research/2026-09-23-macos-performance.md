---
type: Research
title: macOS CPU, GPU, and memory investigation
description: Measured macOS resource costs, proven improvements, and deferred protocol and renderer designs from the September 2026 performance investigation.
resource: scripts/profile-macos.sh
tags:
- performance
- macos
- memory
- cpu
- gpu
- protocol
timestamp: 2026-09-23T03:14:56Z
---

# What the September builds measured

On an M4 Max (Mac16,5, 16 cores, 48 GiB), phase one reduced daemon CPU per rename by 82–97% in
subscribed layouts. At 64 dormant panes, removing an unused native thread-local buffer saved about 71
MiB of daemon footprint. Neither change reduced idle GUI GPU time in the matched pair. A later
cursor-blink change did.

The baseline was `015bf7772`, phase one was `8342b81e2`, macOS was 27.0 (26A428), and Rust was 1.97.0.
The [full September record](https://github.com/autistic-computer/zebrilus/blob/4fa976578/knowledge/research/2026-09-23-macos-performance.md)
includes run IDs, binary hashes, excluded runs and individual results. The original local archive was
`target/performance/2026-09-23/`. Current capture commands are in `scripts/just/profile.just`, including
`just profile build mac` and `just profile metal mac`.

**Source check at `1ce632dce`.** The vendored `third_party/ghostty/src/lib_vt.zig` disables the C ABI
signal stack. The mux's `exact_tmux_option` now uses `OPTION_INDEX`, and `FormatContextSnapshot` still
shares format data across resolutions. `Shared::refresh_status_filtered` now selects recipients and
returns on an empty set before building status requests or advancing the format clock. The September
prototype advanced the clock first. Desktop Kitty cleanup is in `crates/zz/src/terminal/view.rs`
(`release_view`), Metal defers tile reuse in `libs/ala/crates/ala-platform/src/apple/metal_atlas.rs`,
and the macOS browser runtime loads and checks CEF on demand in `libs/ala-cef/src/cef_runtime.rs`
(`MacFrameworkLoader`). The measurements below belong to the historical builds.

## Desktop and GPU

The accepted desktop matrix used private daemon/config, AC power, an active visible 3,008 × 1,662-point
window at scale 2, ten seconds of warmup and 30 seconds of clean sampling. Three idle pairs and two
output pairs alternated build order. Browser and 16-window cases each had one pair. Each terminal in the
16-window case was 341 × 107 cells. CPU is percent of one core. Memory is the median of per-run medians.
A tooltip window, overlapping app launches and diagnostic samplers invalidated runs. Native tracing
followed clean measurement. Mach CPU counters used the measured 125/3 ns conversion, cross-checked
within 0.1%.

| Scenario | Process | CPU, baseline → final | RSS, baseline → final | Footprint, baseline → final |
| --- | --- | ---: | ---: | ---: |
| Idle terminal | gui | 1.045 → 1.054% | 134.67 → 131.84 MiB | 556.08 → 552.66 MiB |
| Idle terminal | daemon | 0.197 → 0.073% | 31.34 → 26.59 MiB | 18.02 → 15.17 MiB |
| Continuous output | gui | 15.132 → 15.301% | 155.91 → 153.27 MiB | 578.04 → 569.60 MiB |
| Continuous output | daemon | 9.370 → 9.421% | 34.16 → 28.77 MiB | 19.45 → 15.52 MiB |
| Browser + terminal | gui | 2.058 → 1.977% | 328.00 → 324.95 MiB | 378.33 → 376.69 MiB |
| Browser + terminal | daemon | 0.156 → 0.071% | 34.09 → 28.69 MiB | 21.50 → 16.02 MiB |
| 16 terminal windows | gui | 1.340 → 1.418% | 139.78 → 137.22 MiB | 564.83 → 561.05 MiB |
| 16 terminal windows | daemon | 0.429 → 0.091% | 126.39 → 103.84 MiB | 113.02 → 91.36 MiB |

Idle GUI CPU ranges overlapped (1.034–1.075% and 1.006–1.076%) at about 140 wakeups/s. The daemon fell
from 0.190–0.197% to 0.064–0.075% at about 11 wakeups/s. Browser helpers used 302.55 → 304.13 MiB
footprint and 489.03 → 489.83 MiB summed RSS. Helpers were outside the GUI Metal/CPU sampling. An
earlier 619 MiB peak included a 128 MiB diagnostic child.

Actual output was about 670–675 lines/s. Two additional reversed-order runs per build used 20 s warmup
and 60 s clean samples. Across all four pairs, median GUI CPU per line was 225.83 → 225.42 µs and daemon
CPU 141.13 → 138.59 µs, with overlapping ranges. Frame/notification counts were unavailable.

| Clean duration and run order | Delivered lines/s, baseline → final | GUI CPU / line, baseline → final | Daemon CPU / line, baseline → final |
| --- | ---: | ---: | ---: |
| 30 s, baseline/final | 674.03 → 671.56 | 221.96 → 229.20 µs | 136.17 → 141.12 µs |
| 30 s, final/baseline | 670.78 → 671.20 | 228.13 → 226.60 µs | 142.54 → 139.52 µs |
| 60 s, baseline/final | 669.98 → 672.75 | 228.30 → 219.74 µs | 141.91 → 136.91 µs |
| 60 s, final/baseline | 674.09 → 675.25 | 223.53 → 224.25 µs | 140.35 → 137.66 µs |

Each idle Metal trace recorded 39 presentations in 20 s (1.95/s). GPU interval union was 203.627 →
203.630 ms (1.018% of the capture), encoder CPU 3.038 → 2.938 ms. The denominator is the full trace
duration. The window's three 6,016 × 3,324 drawables used 77.4 MiB each (232.1 MiB total).

**Driver memory.** One 64 × 64 clear on a standalone device/queue raised footprint from 18.6 to 249.8
MiB with 28 resident 8 MiB driver blocks. Application allocations measured 983,040 bytes. An independent
Objective-C control measured 4.75 → 236.13 MiB at 64 × 64, 339.86 MiB at full size, and 6.22 MiB after
release. Metal 4 and Metal 3 queue limits of one/three buffers reproduced the same blocks. Keeping the
completed target and queue alive without submitting work reduced footprint from 235.97 to 6.36 MiB by 20
s, still 6.36 MiB at 60 s. An unused full-size path texture reported 77.4 MiB allocated without a
footprint increase. These are driver- and OS-specific observations.

In one process, hiding the blinking cursor for about 30 s took owned-unmapped graphics from 243.9 to
19.9 MiB. Showing it restored 243.9 MiB. Display drawables held at 232.1 MiB. That trace failed to
export submission counts. The browser run also had 224 MiB less GUI driver backing than terminal idle,
across different processes. A same-process terminal/browser/terminal trace would be needed to attribute
the transition.

**Steady visible cursor.** A subsequent same-process DECSCUSR trial recorded actual presentations. The
first frames after 45 s steady took 0.10 and 2.08 ms GPU time. CPU includes Instruments overhead.

| Phase | Presents/s | GPU ms/s | Footprint | Owned graphics resident | GUI CPU, 20 s window |
| --- | ---: | ---: | ---: | ---: | ---: |
| Blinking | 1.91 | 4.18 | 400.5 MiB | 228.8 MiB | 259.7 ms |
| Steady | 0 | 0 | 164.7 MiB | 4.8 MiB | 159.7 ms |
| Blinking again | 1.90 | 4.07 | 400.1 MiB | 228.8 MiB | 266.1 ms |

The matched idle pair below used no cursor escapes. The desktop leaves the cursor visible and stops
blinking ten seconds after the last key, mouse, IME or focus activity. Program output does not rearm it.
See `CURSOR_BLINK_TIMEOUT` in `crates/zz-client/src/terminal_view.rs` and the timer in
`crates/zz/src/terminal/view.rs`. The display link still woke about 120 times/s in every phase.

| Bundle | Presents/s | GPU ms/s | Footprint | GUI CPU, 20 s window |
| --- | ---: | ---: | ---: | ---: |
| Baseline | 1.93 | 4.21 | 399.1 MiB | 245.6 ms |
| Blink timeout | 0.02 | 0.04 | 154.1 MiB | 166.4 ms |

## Daemon and terminal

Phase one used matched full profiling bundles with native ReleaseFast, fresh sleeping PTYs, native
subscribers and 100 acknowledged renames per leg. One pair per case, with order reversed between cases.
All 1,000 renames were acknowledged. Driving-client snapshot event counts matched at 102, 100, 100, 0
and 103, including startup events. Eight serialized fixtures separately covered content equality. The
cost below is process CPU per rename.

| Windows | Subscribers | Baseline CPU / rename | Final CPU / rename | Reduction |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 1 | 10.627 ms | 1.938 ms | 81.8% |
| 16 | 1 | 49.142 ms | 3.210 ms | 93.5% |
| 64 | 1 | 201.932 ms | 7.142 ms | 96.5% |
| 64 | 0 | 143.644 ms | 3.494 ms | 97.6% |
| 64 | 4 | 384.189 ms | 20.921 ms | 94.6% |

The same legs observed 30 s idle, spanning two default status periods. No per-refresh count was
captured.

| Windows / subscribers | Idle CPU, baseline → final | RSS, baseline → final | Footprint, baseline → final |
| --- | ---: | ---: | ---: |
| 1 / 1 | 55.631 → 21.140 ms | 23.44 → 19.53 MiB | 12.95 → 9.08 MiB |
| 16 / 1 | 154.875 → 23.319 ms | 74.16 → 53.95 MiB | 63.69 → 43.45 MiB |
| 64 / 1 | 391.407 → 37.955 ms | 270.69 → 207.06 MiB | 259.89 → 196.44 MiB |
| 64 / 0 | 15.071 → 16.125 ms | 220.88 → 150.59 MiB | 210.92 → 140.78 MiB |
| 64 / 4 | 484.550 → 69.389 ms | 284.91 → 221.59 MiB | 273.78 → 211.28 MiB |

**Isolating lookup and formatting.** Selecting names before building descriptors preserved all 248
entries and aliases. An optimized dual-engine fixture alternated six rounds. The table gives option-snapshot times in milliseconds.

| Sessions × windows per session | Baseline | Candidate |
| --- | ---: | ---: |
| 1 × 1 | 10.552 ms | 0.153 ms |
| 4 × 4 | 59.994 ms | 0.792 ms |
| 8 × 8 | 210.397 ms | 2.777 ms |

Lookup alone in a quiet 100-rename native-daemon matrix yielded the CPU seconds below. Cargo's bitcode
settings differed for the standalone candidate CLI. The full-bundle matrix above controls for that
difference.

| Windows | Subscribers | Baseline | Lookup candidate |
| ---: | ---: | ---: | ---: |
| 1 | 1 | 1.033 | 0.217 |
| 16 | 1 | 4.949 | 0.581 |
| 64 | 1 | 20.422 | 4.664 |
| 64 | 0 | 14.535 | 0.336 |
| 64 | 4 | 38.970 | 18.620 |

At 64 windows, 1,662 of 2,146 active command samples included repeated format-universe construction.
Sharing a borrowed universe per client update took snapshot preparation from 40.43 to 1.51 ms,
allocations from 361,894 to 16,654 and requested bytes from 34.06 to 1.65 MB. Four clients took 171.67 →
13.66 ms. Borrowing uid/user directly then took 1.470 → 0.848 ms (one client) and 13.350 → 3.408 ms
(four). Dropping the batch before stamping pane modes took peak requested bytes from 691,538 to 420,768.
These in-process timings overlapped compilation. After the full changes, 1,472 of 2,184 command samples
still included status refresh.

**Native thread-local storage.** At 64 panes, the original daemon had 267 threads. Of about 82.5 MiB in
live native malloc, 70.7 MiB were 266 dyld TLS blocks containing Zig's unused 256 KiB signal stack. A
matched 256-thread Rust fixture measured 75.064 → 6.188 MiB footprint. The executable TLS section shrank
by 262,144 bytes. Rust's 128 KiB alternate stack, signal handlers, masks and flags matched. The C host
had no alternate stack in either build. Eleven wrapper tests linked the changed archive, with all 780
exports preserved. The native Debug test runner uses its own options.

Two interleaved daemon runs per variant isolated this change with matching ReleaseFast and sleeping
PTYs. At 64 panes RSS fell 275.97 → 204.65 MiB, live native malloc 82.476 → 13.119 MiB. Nine quiet
native timing pairs varied by about 1% or less across empty/short updates and
create/write/resize/render/free. A ReleaseSafe build was excluded.

| State | Baseline footprint | Patched footprint |
| --- | ---: | ---: |
| No panes | 6.56 MiB | 4.20 MiB |
| 1 pane | 12.75 MiB | 8.79 MiB |
| 16 panes | 63.20 MiB | 43.46 MiB |
| 64 panes | 265.91 MiB | 194.63 MiB |
| Close back to 1 pane | 149.91 MiB | 79.55 MiB |

At 64 panes the patched daemon reserved 546.2 MiB of virtual stacks with 8.34 MiB resident. Each
additional pane had four workers, about 128 KiB resident stack and 20 KiB remaining native TLS.
Returning to one pane restored the original 15 threads. Row builders accounted for about 10.5 MiB live,
while mimalloc dirty pages (161.2 MiB) also included reusable storage. Thread pooling needs fairness,
cancellation and final-output ordering evidence.

**Inspect deadlock.** `Shared::inspect` held the engine lock while reading agent permissions and the
agent flusher held its lane lock while publishing to the engine. Collecting owned rows before reading
the runtime removed the cycle. Three stuck tests passed ten runs each plus five inspect output/error
tests. Temporary peak requested allocation rose about 99–123 KiB at 64 short rows and 1.85 MiB at 1,024.
This buffer is per request.

## Agent text and payloads

- Eight 4 MiB Write/Edit inputs in a mimalloc reducer fixture retained 48 → 12 MiB of structured diff
  capacity by copying only retained prefixes. Raw JSON stayed at 96 MiB. Total payload capacity was 144
  → 108 MiB with matching serialized transcripts.
- Borrowing untruncated Markdown for display took 4,096 × 4,000-byte entries from 31.25 to 15.625 MiB
  of string capacity and 34.27 to 18.22 MiB process footprint. Render state retained another copy. An
  append crossing the preview limit must still advance the display revision.
- Testing appearance allocation identity before deep equality measured about 150 → 1 ns for the same
  shared settings.
- Shrinking 32 truncated 4 MiB strings to 512 KiB yielded 128 → 16 MiB capacity and 130.11 → 25.74 MiB
  footprint in a construction fixture. Repeated replacement added 11.6% elapsed time (about 6 µs per
  oversized update). The September change copied the retained text/diff prefix without unconditional
  shrinking.

## Image retirement and first browser

**Kitty image ownership.** Dropping a 1,024-square RenderImage's last CPU reference left 4,227,072 GPU
bytes allocated. Twenty visible split-pane closes grew resident graphics from 12.6 to 89.2 MiB. An
initial cleanup reused a shared tile before an older GPU read completed, yielding replacement pixels
(153 instead of 17 per channel). A completion guard retired keys immediately and held shared rectangles
until every older reader finished. Cache view counts covered overlapping focused views.

Matched 0.13.0 builds at `68657361` opened and closed ten 1,024 × 1,024 RGBA image panes, verifying each
upload and one fresh upload afterward. One older baseline retained ten extra mappings (40.3125 MiB). The
primary baseline got a final zero-image paint on one close and retained nine mappings.

| Measured image-sized graphics storage | Baseline | Candidate |
| --- | ---: | ---: |
| 4,128 KiB mappings before / after | 2 / 11 | 1 / 1 |
| Additional mappings after ten closes | 9 | 0 |
| Additional mapped and resident bytes | 38,043,648 | 0 |
| Additional mapped and resident MiB | 36.28125 | 0 |

Four alternating renderer pairs gave median paired CPU differences of −0.66% for a normal scene, −0.05%
for one small retirement/frame, +1.50% for sixteen and +1.73% for a 4 MiB upload/frame, with reversals
between runs. Warmed frame tracking allocated nothing in 100,000 one-/three-frame cycles. Thirteen Metal
API-validation tests and four desktop lifecycle tests covered ordering and release. Reupload latency
after moving a pane away and back and temporary storage while GPU readers lag were unquantified. The app
pair established image-sized resident allocations only. Historical production-identity bundles shared
window-state storage despite private sockets/config. Rejected focus, resize and wrong-app captures
remain excluded.

**Lazy CEF loading.** An isolated warm-cache load took 19–24 ms CPU, adding about 57.6 MiB RSS and
taking footprint from about 1.3 to 16.8 MiB. The GUI experiment instead alternated three matched
terminal-only pairs, 1,505 × 1,662 points at scale 2, 153 × 107 cells, private profiles, 5 s warmup and
120 samples over 30 s.

| Pair | Baseline GUI RSS | Candidate GUI RSS | RSS reduction | GUI footprint reduction |
| --- | ---: | ---: | ---: | ---: |
| 1 | 132.41 MiB | 118.41 MiB | 14.00 MiB | 10.70 MiB |
| 2, reversed order | 132.06 MiB | 118.55 MiB | 13.52 MiB | 1.55 MiB |
| 3 | 131.45 MiB | 117.47 MiB | 13.98 MiB | 10.84 MiB |

Median paired savings before browser use were 13.98 MiB RSS and 10.70 MiB footprint, with daemon memory
within 0.13 MiB. GUI idle CPU overlapped. First browser RGB capture return was 263/189/205 → 218/271/272
ms, or −45 to +82 ms paired. Reopen was 90–98 → 98–100 ms. Readback, encoding and 50 ms polling make
these unsuitable for first-presentation latency. Input, pixels, navigation and storage persistence
passed in the native correctness pair. A restored-browser baseline logged a quit timeout at about 216 ms
against a 200 ms app quit window. The restored-browser runs proved rendering only. Precise
first-presentation latency and complete restored-runtime shutdown were unresolved in this experiment.

## Other September experiments

**Exceptional message buffers.** A Unix socket fixture with real encoder/decoder, mimalloc and both
endpoints held alive tried a 2 MiB cap on retained capacity after completed messages.

| Check | Baseline | Exceptional-capacity ceiling |
| --- | ---: | ---: |
| Retained sender + receiver capacity after a 6 MiB prompt | 18,874,407 bytes | 0 bytes |
| RSS after one prompt, unchanged through 3 seconds idle | 33.48 MiB | 33.48 MiB |
| Footprint after one prompt, unchanged through 3 seconds idle | 32.50 MiB | 32.50 MiB |
| Allocation + reallocation calls for four prompts, instrumented | 13 | 25 |
| Cumulative requested allocation bytes for those four prompts | 50,332,333 | 125,830,756 |
| Median process CPU for 32 prompts, five alternating-order pairs | 148.84 ms | 151.53 ms |
| Median elapsed time for 32 prompts | 134.92 ms | 134.83 ms |

After 32 prompts the ceiling left 52.06 versus 33.47 MiB RSS and 51.08 versus 32.48 MiB footprint. The
18.59 MiB excess persisted at 0, 1, 3 and 10 s idle. September rejected immediate release. Timed pool
decay and interrupted prefix/body ownership need separate tests.

**Config observation.** Six alternating macOS helper runs used 5 s warmup and 30 s idle. Both
implementations detected all 72 in-place and atomic edits with existing parent directories.

| Measurement | Polling | Native observation |
| --- | ---: | ---: |
| Process CPU, percent of one core | 0.01822% | 0.000154% |
| Kernel interrupt wakeups per second | 2.000 | 0.033 |
| RSS | 6.55 MiB | 7.80 MiB |
| Physical footprint | 2.38 MiB | 3.14 MiB |
| In-place edit delivery after write completion | 215 ms | 11 ms |
| Atomic replacement delivery after write completion | 325 ms | 11 ms |

Native in-place delivery ranged 2.5–222 ms. The helper added 1.25 MiB RSS and 0.77 MiB footprint.
Missing/recreated parents, symlinks, overflow recovery and whole-app cost were untested.

**Display-link demand.** Two reversed-order rounds on a 120 Hz display issued 24 requests per
mode/cadence per round, 500 ms fixed or 471–537 ms varied after the last callback.

| Request to main-thread callback | Continuous | Sole-subscriber restart | Restart with active peer |
| --- | ---: | ---: | ---: |
| Fixed cadence median, 48 samples/mode | 1.45 ms | 9.94 ms | 1.61 ms |
| Varied cadence median, 48 samples/mode | 4.05 ms | 10.45 ms | 4.32 ms |
| Varied cadence observed p95 | 7.90 ms | 14.38 ms | 8.16 ms |

Stopping the sole link reduced callbacks from about 120/s to 1.8/s, interrupt wakeups from about 124/s
to 5.3–5.6/s, and CPU from 0.47–0.60% to 0.062–0.092% of one core. Per-round median callback delay rose
6.93–8.50 ms. Keeping a peer active retained native ticks. The full-app scheduling prototype was never
run. Presentation, resize, display changes, sleep/wake and high-rate input remained untested.

**Status recipients.** Twenty paired fixture cases compared the complete refresh with a prototype that
still updated the clock on empty calls. It preserved recipient order and contents. A separate fixture counted allocation calls and requested bytes.

| Complete refresh, 64-window fixture | Baseline allocations / reallocations | Candidate | Baseline cumulative requested bytes | Candidate |
| --- | ---: | ---: | ---: | ---: |
| Empty subscriber map | 12,445 / 166 | 0 / 0 | 913,302 | 0 |
| Session filter matches nobody | 12,709 / 182 | 0 / 0 | 924,973 | 0 |
| Client filter matches nobody | 12,544 / 166 | 0 / 0 | 918,639 | 0 |
| Matching recipient, with or without explicit filter | 23,969 / 1,221 | Unchanged | 2,198,060 | Unchanged |

External Cargo jobs invalidated both native timing attempts. These allocation figures establish neither
CPU gains nor later implementation performance. Metadata-delta proposals still lacked measurements of
encoded bytes, serialization CPU and client render cost in this study. A protocol trial would need
personalized snapshots, explicit base/result revisions, full resync on mismatch, event-order barriers
and convergence across clients. Nested arbitrary formats can make unrelated labels depend on a rename.

**Watchdog.** A native 100 ms main-queue heartbeat, 100 ms monitor and strict age >500 ms distinguished
queue progress from native timer activity.

| Case | Main-queue heartbeats during phase | Native timer firings | Heartbeat detector | Activity observer |
| --- | ---: | ---: | --- | --- |
| Healthy, 936 ms | 9 | 47 | No report | No report |
| Blocked main task, 905 ms | 0 | 0 | Reports and recovers | Reports and recovers |
| Nested native loop, 904 ms | 0 | 45 | Reports and recovers | Misses the stall |

The nested loop triggered the heartbeat detector at 506.6 ms. The observer missed it. The helper omitted
production stack sampling and measured no CPU or wakeups. Later Linux deadline measurements are in the
[Linux performance study](/research/2026-10-16-linux-performance.md).

**Journal serialization.** A direct newline-delimited writer with 128-byte reserve passed 58 output
cases in six alternating mimalloc rounds. CPU fell 5.6–5.9% at 64-byte chunks, 3.5–4.3% at 4 KiB, and
4.6–5.2% for the 1 MiB tool record. Four growth-boundary cases were slower in all six rounds.

| Complete line | Leading LF | CPU change, median paired rounds | Baseline to candidate median CPU |
| --- | --- | ---: | ---: |
| 129 bytes | No | +6.31% | 90.78 to 96.03 ns |
| 129 bytes | Yes | +5.87% | 89.09 to 94.13 ns |
| Escaped, 257 bytes | No | +2.05% | 160.06 to 163.71 ns |
| Boundary, 257 bytes | No | +2.38% | 141.22 to 146.32 ns |

For 129 bytes both paths made two allocation calls, but returned capacity rose from 129/130 to 256. Full
append/coalescing/flush throughput remained unmeasured.

**Bounded JSON.** The selective candidate detected direct over-cap strings, preserved UTF-8 and error
behavior, and matched all 24 fixture outputs and 205 cap-boundary cases. Six alternating rounds shared
one scratch binary per measurement type. Four build dependencies differed from the app lockfile.

| Payload | Baseline to candidate CPU per call | Change |
| --- | ---: | ---: |
| Null | 6.99 to 8.56 ns | +22.4% |
| Boolean | 6.91 to 8.35 ns | +20.9% |
| Large Unicode object string | 298 to 686 µs | +130.1% |
| Large escape-heavy object string | 541 to 896 µs | +65.6% |
| Direct Write content, 4 MiB | 1.198 to 1.076 ms | -10.1% |
| Direct Edit strings, 4 MiB each | 2.350 to 1.099 ms | -53.2% |

Write/Edit result capacity fell from about 8 MiB to 512 KiB each. A broad earlier prototype reduced raw
input capacity from 96 to 4 MiB. The Unicode/escape-heavy CPU regressions prevented acceptance. App RSS
and reducer throughput were unmeasured.

Other controls found 14.625 MiB less reported allocation for a deferred 2,560 × 1,440 Metal path target,
with no equal physical-footprint saving, and about 31.9 MiB made reusable by explicit allocator
collection in a 32 MiB fixture, without evidence on future page faults or other threads.

## Historical validation

The original workspace run passed 4,121 tests with one OSC 7 hostname-dependent failure and five
ignored. The failed test passed alone, as did an earlier port-collision test. The daemon rerun passed
all 1,005 tests at four threads and strict workspace Clippy passed. At that revision, the daemon-only
configuration lacked required JSON dependencies and workspace formatting found unrelated iOS files.
These results and the measurements above were recorded in September. No benchmark was rerun for this
source check.
