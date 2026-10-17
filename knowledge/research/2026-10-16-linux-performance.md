---
type: Research
title: Linux stack performance and rendering validation
description: Matched Linux CPU and RAM measurements, resource lifetime checks, accepted optimizations, and cross-platform validation follow-ups for the terminal, browser, text, and rendering stack.
resource: crates/zz-browser/src/linux_dmabuf.rs
tags:
- linux
- performance
- rendering
- cpu
- gpu
- memory
- terminal
- browser
- agent
timestamp: 2026-10-16T19:15:35Z
---

# Measurement contract

This investigation starts from `0d729eb36f6247f445b331503a5e7a05fdd1d6f2`.
The host is Linux 7.3.0-9-generic on an AMD Ryzen 7 7800X3D with eight online
cores, GNOME Wayland, and an AMD Vulkan GPU. The initial investigation used
a Dell P2725QE at 3840 × 2160, 100 Hz, scale 1.5. The display configuration
subsequently changed to a Samsung Odyssey G80SD at 3840 × 2160, 120 Hz,
scale 1.25. The display returned to the Dell at 100 Hz and scale 1.5 before the final
app matrix. Final baseline/candidate pairs use that configuration, a
1280 × 900 logical-pixel window, and DejaVu Sans Mono at 12 points. Each
run records the actual display state at both ends; mismatched geometry or
focus invalidates a pair.

CPU percentages are fractions of one core. Native terminal and shaping
experiments isolate mechanisms; their improvements are not whole-application
CPU claims. Clean timing excludes builds, tracing, frame diagnostics, and
Vulkan interception. Correctness and resource lifetime runs are separate.
The initial mixed-GUI perf capture had mostly unresolved call chains, so
its inclusive function percentages are not used to attribute savings. Native
probes, equivalent-output tests, operation counts, and app pairs establish
the accepted results.

The original optimized baseline GUI SHA-256 is
`309d7dd4f989d140b781d8fe729d8922c5f732222d6bee3f6f5d87f7eab1c9d4`;
the CLI is
`b6fa9a8be3570cc88b3b9baca8649da757f7823e54e7ba8ccbf267012c57a90e`.
Every app run gets a private daemon socket, config, data, cache, browser
profile, and launch receipt. Baseline and candidate processes are restarted
between runs and use identical workload bytes and window geometry.

Both variants use the `profiling` optimization profile and combined GUI/CLI
package feature selection. A standalone CLI rebuild changed shared dependency
features; its interrupted comparison was recorded under
`excluded-first-matrix/` and excluded from results. The final CLI is rebuilt
with `cargo build --profile profiling -p zz -p zz-cli --bin zz_cli`.
The measured GUI contains the final GUI changes but predates the four-line
daemon view-area repair. Every candidate clean app comparison starts the final CLI-owned daemon
first, so it does not use the GUI's embedded daemon implementation. Optimized
import ownership audits use the preserved baseline CLI and validate the
candidate GUI importer independently.
All final source changes are covered by the complete test and lint gates.

The original investigation stored raw evidence in the ignored directories
`target/profiles/linux-stack-20261016/` and
`target/profiles/linux-efficiency-20261016/`. The RAM follow-up on October 16
found that the build directory and original working checkout had been removed.
The original raw profiles, comparison binaries, and runner are no longer
available there. The follow-up recovered the baseline CLI through a still-running
process's executable mapping and verified the exact SHA-256 recorded above;
the original GUI and candidate binaries remain unavailable.
The measurements below remain the committed campaign record;
artifact names identify the original receipts, not currently available files.
Fresh RAM probes use `target/profiles/linux-ram-20261016/` in the main checkout.
They record their own source revisions, binary hashes, workloads, and results.

Cross-platform runtime validation is tracked in
[macOS #11](https://github.com/demfabris/zz/issues/11),
[Windows/WSL #12](https://github.com/demfabris/zz/issues/12),
[web #13](https://github.com/demfabris/zz/issues/13),
[physical iOS #14](https://github.com/demfabris/zz/issues/14),
[Intel/NVIDIA Linux #15](https://github.com/demfabris/zz/issues/15), and
[experimental FreeBSD #16](https://github.com/demfabris/zz/issues/16).
Each issue pins baseline `0d729eb36f6247f445b331503a5e7a05fdd1d6f2` and
candidate `cda21ce33feb350be7918e8af135c4119478bf60`, lists the applicable
changes and breakage checks, and requires matched runtime evidence.

# Native terminal results

Five rotated ReleaseFast trials use the same 64 MiB input, terminal geometry,
and formatted-state hash. A row-local hyperlink mapping memo avoids repeated
URI hashing and destination lookup while preserving reference counts.

| Workload | Baseline CPU | Memo CPU | Change |
| --- | ---: | ---: | ---: |
| Mixed output | 4.7290 s | 3.6912 s | −21.94% |
| Repeated OSC 8 hyperlink spans | 4.6879 s | 2.2801 s | −51.36% |
| ASCII | 0.09885 s | 0.09869 s | No measurable difference |

All variants produce identical formatted terminal state. Narrow-page,
reference-count, map-capacity retry, and allocation-failure tests exercise
the mapping lifecycle. Source: `third_party/ghostty/src/terminal/page.zig`,
`clonePartialRowFrom`.

Reducing the initial raw-page preheat from four pages to one cuts CPU for
10,000 terminal create/free cycles from 0.16699 to 0.06105 seconds (−63.44%).
For 1,000 cycles, `strace` counts fall from 4,008 to 1,008 `mmap` calls and
4,000 to 1,000 `munmap` calls. The existing demand-allocation/reuse path stays
in place; no shared pool or lifetime coupling is added. Sustained 512 MiB
ASCII CPU remains flat (0.79062 versus 0.78914 seconds).

The native harness's inherited maximum RSS is not usable as a memory result.
Allocation call counts establish less allocation churn, not resident-byte
savings. Source: `third_party/ghostty/src/terminal/PageList.zig`, `init`.

# Copy-mode extraction

Copy mode now bounds packed cells and overlays to the existing consumer-derived
viewport bound. The frozen terminal, selection contents, search coordinates, and
scrollbar range retain their full extent. The cache key includes the requested
extent, so a later larger consumer gets the missing columns and rows.

An optimized extraction probe alternates fresh frozen revisions with
2000 × 2000 versus 120 × 40 output. Median extraction falls from 92.54 to
0.1167 milliseconds, and packed output falls from 32,000,000 to 38,400 bytes.
Both cases use the candidate implementation; requesting the full extent
reproduces the previous extraction behavior. This measures extraction and
output allocation, not the memory occupied by the complete frozen snapshot.
Source: `crates/zz-terminal/src/session.rs`, `copy_mode_snapshot`, and
`session/mode_revision.rs`, `viewport_cells`.

The broader daemon suite caught a related integration defect: after a focus
change wrote back layout geometry, the native terminal reached 41 × 24 but
the cached client area still clipped its published frame to 39 × 24. The daemon
now refreshes existing view areas after geometry settles. The existing focus
test and neighboring geometry, resize, and preview tests pass. One thousand
unchanged visibility refreshes preserve all four frame identities in a
two-pane/two-client fixture, with zero new published frames. This path runs
on layout/stream snapshot updates, not terminal frame publication.
Source: `crates/zz-daemon/src/daemon.rs`, `refresh_terminal_visibility`.

# Text shaping and painting

The global six-entry shaping-plan FIFO repeatedly evicted plans while trying
fallback faces. The vendored cosmic-text 0.19 implementation keeps six plans
per font face, with the existing full plan-key comparison. Mutating the font
database clears the plans. This avoids changing font coverage, normalization,
fallback, or glyph selection.

Eight alternating runs of 1,024 mixed lines reduce median shaping time from
0.9948 to 0.3367 seconds (−66.15%) and process user CPU from 0.97 to 0.33
seconds. Plan constructions fall from 246,122 to 2,125. The tradeoff is about
2.5 MiB more retained allocator storage and 2,638 KiB more median maximum RSS
in this 425-face workload. The bound is six plans per shaped face; it is not a
constant global memory bound.

Equivalence checks cover 1,464 records with three cache orders, Arabic,
Hebrew, Indic, combining marks, ligatures, emoji, feature sets, and static and
variable fonts. All 39,924 glyphs and 2,160 missing-glyph results match.
The shaping probe excludes ANSI processing and app rendering.
Sources: `third_party/rust/cosmic-text/src/shape.rs`, `ShapeBuffer`, and
`src/font/system.rs`, `FontSystem::db_mut`.

Terminal rows also paint freshly computed raster data immediately, removing
the first-paint glyph walk that preceded the same raster computation. The
slow path remains available. Eighty-eight paint executions across five scale
factors, clipping, opacity, synthetic styles, and fractional origins produce
identical sprites while reducing glyph-cache lookups from 1,408 to 880
(−37.5%). This is an operation-count result. Source:
`crates/zz-ui/src/terminal.rs`, `RowRenderCache`.

# Browser resource ownership and readback

The Linux DMA-BUF importer transfers both the Vulkan image and its dedicated
memory allocation to wgpu. Previously imported memory was left outside the
texture's destruction path. Temporary image/memory ownership and the duplicated
file descriptor now have explicit cleanup paths through allocation and bind
failures. The local adapter preserves CEF's modifier-plane layouts when the planes
share one DMA-BUF allocation; disjoint plane allocations remain unsupported.
wgpu-hal 30's existing helper only accepts one modifier plane.

An instrumented debug app run balances 1,385 imports with 1,385 frees and
peaks at two live imports. The final optimized app balances 1,405 imports
with 1,405 frees, with zero remaining after browser close and GPU drain. Allocation
and bind fault injection verify descriptor closure, image-before-memory
destruction, successful readback fallback, and no remaining private processes.
Import counts establish lifetime balance, not unique VRAM backing bytes.
Source: `crates/zz-browser/src/linux_dmabuf.rs`.

Readback frames now update a persistent GPU texture. Consecutive frames upload
the clipped damage rectangle; new sessions, resized frames, unknown damage,
and generation gaps require a full upload. Oversized textures fall back before
wgpu validation. The complete CPU frame remains available to screenshot and
terminal consumers.

Three alternating test-profile pairs of 120 uploads from a 1920 × 1080 source
compare full frames with 256 × 128 damage. Median elapsed upload time falls from 48,734 to
4,293 microseconds (−91.19%); pixel bytes fall from 995,328,000 to 15,728,640
(−98.42%). Both cases reuse the same texture and include queue submission and
periodic completion waits. Pixel equality is checked after every case.
This excludes CEF's full CPU copy and final compositing. GPU tests also cover
alpha-byte preservation, generation gaps, resize, session replacement, and
device size limits. Current CEF output has an opaque background; these tests
do not establish hypothetical transparent-window compositing parity.
Source: `crates/zz-browser/src/readback_texture.rs`.

# Agent and markdown updates

Tool payloads are grouped by tool entry ID. Updating one tool truncates and
updates its own slots instead of scanning every retained tool payload. Live
text handles still receive updates, and resetting the timeline releases the
cache. Six alternating trials of the actual old and new adapters show replay
times of 13.62 versus 1.10 milliseconds for 1,000 tools and 1,167.30 versus
10.42 milliseconds for 10,000 tools. One thousand unchanged updates at 10,000
retained tools fall from 255.05 to 0.675 milliseconds. This adapter probe uses
the test profile, not the optimized GUI. Source:
`crates/zz/src/agent/view.rs`, `ui_entry_with_markdown`.

Parsed markdown shares its block vector through `Arc`; appending uses copy on
write, preserving snapshots held by renderers. For 2,000 snapshot clone/drop
operations, eight paragraphs fall from 12.59 milliseconds to 21.58
microseconds and 512 paragraphs from 870.79 milliseconds to 21.32
microseconds. This test-profile probe isolates document cloning, not parsing or total app
CPU.
Retained-snapshot tests and the text suite verify append behavior.
Sources: `crates/zz-gpui-kit/src/text/document.rs` and `text/state.rs`.

# Scheduling decisions

Dirty notifications now participate in the existing inactive-window frame
cap, alongside explicit rendering and animation callbacks. Required
presentation and idle checks preserve their bypass behavior. Deterministic
tests verify dirty work survives throttling, required presentation proceeds,
and activation resumes drawing. A live optimized Wayland probe verifies
focus enter/leave events: streaming draws at 64.2 FPS while focused, 20.7 FPS
while inactive, and 63.6 FPS after reactivation. A single pending update
arrives in 35.0 milliseconds while inactive. This establishes cap and delivery
behavior; the older baseline focus probe used a different display scale and
build load, so it is excluded from performance comparisons.
Source: `crates/zz-gpui/src/window.rs`.

The main-thread watchdog observer sleeps until the earliest expiry of its
last observed heartbeat while the app is healthy. It retains 100-millisecond
polling during a stall and the foreground heartbeat itself remains unchanged.
Tests exercise 190 freeze offsets, detection within the threshold plus
100 milliseconds, recovery within 100 milliseconds, and at most 25 healthy
observer checks per ten seconds versus the previous 100.
Source: `crates/zz/src/diagnostics/mod.rs`.

The terminal's global 16-millisecond publication interval remains unchanged.
Thirty bounded trials use a 120 Hz producer and simulated 60/120 Hz display
consumers. A 10-millisecond interval at 60 Hz increases process CPU by 11.6%
for 0.28% more displayed updates. At 120 Hz it increases CPU by 10.8% for
58.8% more displayed updates; that excludes encoding and GUI cost. Starting
the 16-millisecond interval at batch start instead of completion is effectively
flat. A global faster default fails the efficiency requirement at 60 Hz.

Display-aware publication remains a separate feature: the protocol currently
has no display-rate demand, and preview consumers also request the normal
watched cadence. It needs consumer-rate aggregation, delivery policy, and wire
compatibility work before it can replace the global interval safely.
Sources: `crates/zz-terminal/src/session/pane_actor.rs`, publication deadlines,
and `crates/zz-protocol/src/message.rs`, terminal request variants.

# Measured proposals not adopted

| Proposal | Measurement | Decision |
| --- | --- | --- |
| Cache external texture views and bind groups | Creating a pair costs a median 1.289 microseconds; cloning costs 10 nanoseconds. Maximum saving at 60 Hz is about 0.077 milliseconds of CPU per second per surface. | Keep the current ownership model; another cache must track rotating textures and release closed browsers. |
| Retain copy-flash scratch resources between animations | 120 shader frames take 21.86 milliseconds versus 4.24 for empty scenes. Input/target textures already survive all 120 frames and are released afterward. | Keep reuse within each animation; retaining two 1080p targets between animations costs about 15.8 MiB per depth. |
| Remove resize completion waits | A 120-draw resize loop takes 231.76 milliseconds. Across resize, setup, and recovery, 129 device-idle calls take 0.734 milliseconds and 385 timeline waits take 3.15 milliseconds. | Preserve completion before destruction/reconfiguration; the waits are a small measured fraction and configure still requires synchronization. |
| Disable post-input scene replay globally | 120 clean frame requests during sustain produce 120 full draws; opting out produces zero. | Preserve the existing VRR underclocking mitigation. The backend lacks a reliable actual-VRR-active signal for a targeted fixed-refresh policy. |

Resource timing uses an AMD Vulkan GPU and isolated headless probes; the
resize probe uses small 128/129-pixel Wayland windows and eight successful
error recoveries. These results do not bound larger scenes or other drivers.
The replay count is measured, but actual VRR hardware behavior was not
validated. Sources: `crates/zz-gpui-platform/src/wgpu/wgpu_renderer.rs` and
`crates/zz-gpui/src/window.rs`.

# Final app comparisons and validation

Three fresh alternating terminal pairs use the same 512 MiB mixed fixture and
107 × 43 terminal geometry. The table gives each variant's median; percentage
changes are medians of the three paired changes.

| Mixed-output metric | Baseline | Candidate | Paired change |
| --- | ---: | ---: | ---: |
| Workload wall time | 38.716 s | 30.486 s | −21.05% |
| GUI CPU | 29.78 s | 12.45 s | −58.46% |
| Daemon CPU | 39.37 s | 31.55 s | −19.86% |
| GUI + daemon CPU | 69.15 s | 43.96 s | −36.49% |

Wall-time improvements range from 20.83% to 21.61%, and combined CPU
improvements from 35.81% to 36.71%. This is the combined candidate result;
the isolated probes above establish which mechanisms improve, but do not
additively assign the app's total saving to individual changes.

Ordinary streaming and grid CPU are effectively unchanged. The 150 MiB ASCII
workload has essentially equal median wall time, about 0.323 seconds, with
a 1.105-second baseline outlier during 74.4% host CPU load; it does not
establish a throughput improvement. Live watchdog observer voluntary context
switches fall from 10.0 to 2.5 per second in all four steady terminal phases.

The original matrix measured higher candidate memory use. Combined GUI/daemon median PSS is
144.91 versus 160.13 MiB at initial idle, 169.15 versus 183.33 MiB after
32 idle windows, and 297.33 versus 314.67 MiB after mixed output. Most of
the offset already exists at initial idle; growth from initial idle through
mixed output is 152.41 versus 155.02 MiB. These are sequential workload
endpoints, not independent steady-state allocations. The app measurements
do not assign that offset to one component.

A separate frame-recorded terminal pair preserves the same geometry. Heavy
mixed-output draw wall time falls from a median 23.009 to 10.077 milliseconds
and p95 from 30.948 to 13.960 milliseconds. Prepaint accounts for most of
that difference: 21.548 to 8.675 milliseconds median. Median shaped glyphs
per frame are similar, 3,128 versus 3,117. Recorded FPS rises from 36.34 to
39.90; ordinary stream/grid FPS remains about 63 in both variants. This is
one diagnostic pair with frame instrumentation, not a process-CPU comparison.
Receipt: `renderer/terminal-frame-summary.json`.

Browser animation intervals require CDP receipts before and after timing:
the expected document must be visible, focused, painted, and advancing, with
the same 1498 × 1138 canvas at device scale 1.5. Canvas PNG readback runs
outside the CPU interval. It may affect subsequent caches or allocations;
both variants use the same collector and debugging-port setting. Native
focus, display geometry, and complete process cleanup are also required.

Earlier browser attempts without these activity receipts were excluded. A
subsequent baseline attempt directly exposed a hidden document with an
untouched transparent canvas. The replacement third pair explicitly switches
from the worker window to the exact browser window/pane before its fixed
warmup; both variants use that sequence and record actual GUI selection.
The first two accepted pairs independently pass the same CDP activity checks.
The hidden-startup cause remains unresolved; activation is a harness step,
not an established product fix. Failed receipts remain in the excluded
artifact directories.

Three accepted browser pairs show sustained GUI/daemon CPU savings. Each
row is a successive ten-second animation interval; percentages use one
core as 100%. Variant columns are medians, and changes are medians of paired
changes. Daemon CPU is zero at the accounting resolution.

| Animation interval | Baseline GUI + daemon | Candidate GUI + daemon | Paired change |
| --- | ---: | ---: | ---: |
| First | 20.90% | 11.50% | −43.90% |
| Second | 36.40% | 11.70% | −68.21% |
| Third | 50.90% | 11.30% | −78.00% |

CEF child CPU remains about 10% in both variants. Including CEF, final-interval
CPU is a median 61.30% versus 21.40% of one core. Baseline GUI system CPU
grows from 1.36 to 4.36 seconds per interval; the candidate stays between
0.29 and 0.34 seconds. All three pairs show this pattern, with overlapping
host-load ranges. Static GUI CPU overlaps at about 2.1–2.2%, and after-close
CPU is below 1% in both.

GUI/daemon PSS is 239.37 versus 251.54 MiB before animation, 249.89 versus
260.98 MiB at the final interval, and 256.61 versus 268.47 MiB after close.
CEF children retain 229.95 versus 232.73 MiB after close; the runtime keeps
seven child processes until app shutdown. Most candidate memory offset
already exists before animation. PSS does not measure retained imported GPU
handles or assign that offset to the importer. The separate lifetime audit
establishes balanced resource ownership without a VRAM-byte claim.
Receipt: `renderer/browser-app-summary.json`.

The GUI's discrete-GPU DRM client reports median total/resident VRAM of
138.27 → 151.23 → 157.52 MiB across the baseline animation intervals,
versus 110.23 → 110.23 → 103.55 MiB in the candidate. After close it reports
141.73 versus 87.55 MiB. Candidate animation endpoints stay within
103.52–110.23 MiB across the three runs. This establishes lower retained
buffers over the observed interval, not an all-time bound or unique global
VRAM saving: shared buffers can appear in multiple clients' counters, so GUI
and CEF counters are not summed. See the [DRM accounting contract](https://www.kernel.org/doc/html/latest/gpu/drm-usage-stats.html)
and `renderer/browser-drm-summary.json`.

A separate recorded browser pair sustains 99.4–99.9 FPS in both variants.
Present/submit wall medians grow from 0.881 to 2.954 milliseconds across the
baseline intervals, versus 0.178 to 0.186 milliseconds in the candidate;
final-interval p95 is 3.487 versus 0.490 milliseconds. Draw-only medians stay
around 0.3 milliseconds, with higher candidate p95 tails in this one recording
(0.523–0.571 versus 0.305–0.421 milliseconds). The result establishes retained
cadence and reduced submit cost, not an improvement in every frame phase.
These are CPU-side wall phases, not GPU execution or process CPU measurements.
Receipt: `renderer/browser-frame-summary.json`.

The unpaced, coalesced agent burst does not establish a whole-app speedup.
Across three pairs, GUI/daemon CPU during the 20-second window is a median
0.560 versus 0.580 seconds, with overlapping ranges. Empty and settled CPU
also overlap. The separate actual-adapter and document-clone probes establish
their narrower improvements. Offline journal checks verify all 4,000 text chunks and 1,000 tools in
exact order and content in all six runs, with contiguous persisted sequences.
This proves daemon persistence, not that the GUI applied every mock update;
the app timing is not a full-row completion benchmark.

A separate recorded agent pair waits for the explicit `end_turn` response.
Both runs return all 1,000 tools and the identical, untruncated 499,112-character
transcript from 4,000 text events. Reported completion takes 491 versus
489 milliseconds. Both draw 60 frames during the 20-second capture; median draw wall
time is 0.959 versus 0.983 milliseconds and p95 is 1.593 versus 1.996
milliseconds. Neither draws during the settled interval. This confirms
completion for the recorded pair and establishes no rendering improvement;
it does not retroactively prove completion in the clean timing runs or that
every intermediate mock update was individually presented. Frame percentiles
include the idle tail after the roughly half-second burst.
Receipt: `renderer/agent-frame-summary.json`.

Background desktop services kept running; host load is recorded for every
sample. Builds and tracing were excluded. Workload generators and CLI probes
are excluded from GUI/daemon CPU; browser child processes are reported
separately. Full app receipts are in `app/` and the terminal aggregation is
`renderer/terminal-app-summary.json`.

The final `cargo ci-test` run passes 6,426 tests, with 21 ignored. The native
Ghostty Debug suite passes 6,517 tests with 68 skipped. Separate validation
passes 142 platform tests (including GPU tests), five iOS host tests, the explicit Vulkan pixel
check, and the Wayland resize/recovery probes. Workspace all-target/all-feature
clippy passes with warnings denied. The full web build, including syntax
highlighting, passes using task-local wasm-bindgen 0.2.128 and the installed
LLVM 22 tools; it reports existing nightly deprecation warnings in the kit
highlighter. macOS, Windows, physical iOS, and other GPU drivers were not run.

Two existing fixture assumptions were repaired while completing these gates.
The native compressed-clone test now deliberately leaves a partially used
last page instead of accidentally filling it exactly. The process-detection
test now uses a shell named `codex`; the host's uutils `sleep` refuses that
renamed executable. The original size-reduction and process-group assertions
remain meaningful. The copy-mode focus regression described above was
introduced by exposing stale bounds and was fixed before final acceptance.

The final matrix accepts 18 clean runs and six separate frame-recorded runs,
with no missing runs or receipt failures. Eleven changes are retained with
the evidence and limits above. Five evaluated proposals remain unchanged:
the global terminal cadence, external texture binding cache, retained
copy-flash resources, resize wait removal, and unconditional post-input replay
removal. The matrix summary is `app-summary.json`; per-finding decisions are
in `ledger.json`.

# RAM follow-up

The follow-up rebuilds the original baseline and shipped candidate with the
same combined-package command, profiling settings, dependency features, and
toolchain. New artifacts live under `target/profiles/linux-ram-20261016/`.
The fresh CLI hashes are
`527114d7777d78b022f913572d69206115ec4270e0cf4c5bca20a4f3dd5a7d31`
(baseline) and
`6a6b4da2544bbd44d445e5eb89d5413458d2b7a58b5c8f1a63044e292b04f5fd`
(candidate). Their loadable file segments differ by 288 bytes; loadable memory
segments differ by minus 32 bytes. Binary growth does not explain a multi-MiB
startup difference in these rebuilds.

Three alternating daemon pairs use fresh private sockets and identical
107 × 43 terminals. A common TUI subscriber uses a separate executable inode,
so attaching it does not divide either daemon's executable-page PSS. Each
endpoint takes three samples. All samples have zero `SwapPss` and
`AnonHugePages`; cleanup leaves no private processes running. At least
23.71 GiB of host memory remains available throughout the runs.

| Daemon phase | Baseline PSS | Candidate PSS | Baseline anonymous PSS | Candidate anonymous PSS |
| --- | ---: | ---: | ---: | ---: |
| Empty | 17.17 MiB | 16.54 MiB | 1.703 MiB | 1.695 MiB |
| One unwatched terminal | 20.76 MiB | 20.42 MiB | 2.609 MiB | 2.598 MiB |
| One watched terminal | 22.52 MiB | 22.05 MiB | 3.328 MiB | 3.324 MiB |
| 32 idle terminals, one watched | 30.62 MiB | 30.68 MiB | 11.371 MiB | 11.906 MiB |

The table reports independent medians, so differences between its medians
need not equal median paired differences. The original roughly 3.9 MiB daemon
startup offset does not reproduce. The 32-terminal phase does retain more
anonymous memory in all three candidate runs: 592, 92, and 948 KiB. These
measurements do not identify the responsible allocations. Both revisions use
the same allocator policy; the native row memo lives on the stack and the
page-preheat change reduces reservations. Neither observation assigns the
remaining difference to a particular change.

A separate sharing control starts a second private daemon from the same
executable inode. The first daemon's PSS falls from 19.99 to 11.70 MiB;
executable-mapping PSS falls from 18.10 to 9.80 MiB while anonymous memory
stays at 2.402 MiB. Stopping the second process restores the first result.
Starting a byte-identical executable from another inode does not produce
that drop. This demonstrates PSS sensitivity to other live processes, as
described in the [Linux proc accounting documentation](https://cdn.kernel.org/doc/html/latest/filesystems/proc.html).
It does not establish the cause of the original matrix's offset: those raw
maps are unavailable, and the surviving historical daemon has since paged out.

Receipts: `daemon-memory-summary.json`, `matched-build-comparison.json`,
`daemon-runs/`, and `executable-sharing/result.json` under the fresh artifact
directory. These daemon-only runs use a TUI subscriber and do not replace the
historical GUI workload.

## Fresh GUI memory comparison

Three alternating pairs compare the original baseline with shipped candidate
`cda21ce33`, before either RAM follow-up patch. The rebuilt GUI hashes are
`3fa9c2749a031d3a5b0ff00ac6fd98d6abac111d62887aa33bc141ca81b0b91e`
and `89e306211daae31b9971899d7c576e7c53898eefe5935ccc76e1c6c725d884c5`.
Both use the same full-debug profiling build and matching preserved CLI.
Build receipts and debug files are in `baseline-bin/` and `candidate-bin/`.

This is a new controlled fixture: explicit DejaVu UI/terminal fonts, a quiet
terminal, private application/GPU cache directories, a 1280 × 900 logical
window, a 1920 × 1350 native buffer, and a 104 × 41 pane. It does not recreate
the unavailable original harness. System page cache remains warm between
runs. Equal Wayland logging establishes a real main-surface buffer commit
and frame callback, focus, and fresh presentation after each text phase.
These are instrumented memory runs and establish no application CPU gain.

All six runs and 48 checkpoints pass, with no rejection or retry. Each checks
geometry, focus, compiler absence, and zero process `SwapPss`; cleanup leaves
no private process running. At least 21.8 GiB of host RAM remains available.
Neither GUI loads CEF or starts browser children in this terminal-only fixture.

| Checkpoint | Baseline GUI + daemon PSS | Candidate GUI + daemon PSS | Baseline anonymous PSS | Candidate anonymous PSS |
| --- | ---: | ---: | ---: | ---: |
| Startup, 3 s after first presentation | 141.36 MiB | 143.41 MiB | 26.840 MiB | 26.836 MiB |
| Startup, 22 s | 140.83 MiB | 142.84 MiB | 26.246 MiB | 26.238 MiB |
| ASCII, 12 s after update | 142.45 MiB | 144.16 MiB | 27.711 MiB | 27.684 MiB |
| Unsupported-character fallback, 12 s | 200.15 MiB | 203.26 MiB | 36.008 MiB | 36.875 MiB |

These are independent medians. Total PSS varies substantially with file-backed
pages, especially the first pair, and is not evidence for a stable allocation
increase. Settled startup GUI anonymous paired differences are +0.156, +0.125,
and −0.141 MiB. The original large startup offset does not reproduce as a
consistent anonymous, GPU, or CEF cost in this fixture. Its original cause
remains unknown.

The first pair's settled ASCII checkpoint has 20.180 MiB less candidate
self-executable PSS. Its executable code segment has 5.164 MiB resident,
versus 23.812 MiB in the baseline. PSS nearly equals RSS for these mappings,
so this is residency variation, not the proportional-sharing effect isolated
in the daemon control. All 48 checkpoints contain exactly the four expected
ELF load mappings, with no whole-file executable or debug-file mapping.
Why the clean executable pages became nonresident remains unproven.

The unsupported-character phase does produce a consistent GUI anonymous
increase: +0.973, +0.957, and +0.758 MiB. GUI anonymous medians are 30.906 and
31.777 MiB. Font-file resident medians rise from about 2.67 MiB at startup to
52.80 and 53.26 MiB after fallback; font-file PSS is 51.82 and 52.31 MiB.
This confirms a workload-dependent cost, but mapping names alone do not
assign anonymous allocations to shaping plans, font objects, driver state,
or other caches. The isolated font probes below establish the narrower
container-storage saving; a complete GUI containing that later patch was not
rebuilt for this comparison.

AMD driver accounting has the same resident-VRAM medians in both variants:
87.42 MiB at startup and after ASCII, and 145.11 MiB after fallback. Resident
GTT stays at 2.004 MiB. Duplicate descriptors are deduplicated by DRM client
and device. These counters include shared buffers; they are not unique
physical VRAM and must not be added to process PSS. The fallback-related
driver-memory growth is a shared workload cost, not a candidate regression.

Receipts: `gui-memory-summary.json`, `gui-file-pss-attribution.json`,
`gui_memory.py`, and the per-run records under the fresh artifact directory.
The earlier functional pilot is retained under `gui-pilots-excluded/` and
excluded from these results.

## Terminal resize buffers

The live snapshot cache retains up to two reusable cell planes. A plane only
works for its exact cell count, but the shipped code can retain a large old
plane indefinitely while a smaller terminal stays watched. The follow-up
keeps reuse for matching lengths and removes incompatible cached planes when
a live cell plane commits with a different cell count. Later frame-building
operations can still fail; this is not a completed-frame publication gate.
Existing snapshots held by other consumers remain valid through their `Arc`;
their allocations remain alive until those consumers release them. Extraction
retries keep their existing path. Frozen copy-mode history bypasses this live-frame commit path
and is outside the change's scope.

A same-binary probe copies the production pool methods and uses the daemon's
mimalloc version, `no_thp`, and `MIMALLOC_PURGE_DELAY=0`. Growing from 107 × 43
to 1,000 × 500 cells and then shrinking, followed by 100 ordinary updates,
leaves 4,036,808 bytes in the shipped pool versus 36,808 bytes with pruning:
exactly 4,000,000 bytes of obsolete cell payload are released. The logical
plane-creation count stays at four in both variants, and output checksums
match. This is retained payload, not an application RSS claim.
The saving scales with the old geometry: one obsolete 240 × 80 plane contains
150 KiB of cells. The 4 MB result uses a deliberately enormous terminal.

Seven alternating timing repetitions ran after the optimized GUI build
finished and before the next build started. Median isolated method costs are:

| Workload | Shipped | Pruned | Meaning |
| --- | ---: | ---: | --- |
| Fixed 120 × 40 cells | 0.291 µs/update | 0.286 µs/update | Same two plane creations |
| Alternate 120 × 40 and 160 × 50 | 0.381 µs/update | 0.397 µs/update | About 0.015 µs added per update |
| Monotonic resize through 2,000 updates | 8.615 µs/update | 4.789 µs/update | Same creation count; fewer minor faults |
| Alternate 107 × 43 and 1,000 × 500 | 27.3 µs/update | 664.7 µs/update | About 0.64 ms added per extreme resize |

Pruning trades cross-size reuse for releasing obsolete cell buffers. The extreme
alternating case creates 100 planes instead of two and takes 48,850 minor
faults instead of 977; neither variant takes major faults. This cost matters
for repeated huge geometry changes and is not represented by the ordinary
resize result. Logical plane creations are not a count of OS allocations;
mimalloc reuses pages in the ordinary alternating case. The loop includes
plane acquisition, updates, and commit but excludes final cache destruction.
Live publication computes one shared maximum frame bound before iterating
attached views, so clients with different sizes do not by themselves cause
the extreme alternating-size case.

An earlier application probe observed anonymous memory falling from 8.31 to
3.58 MiB after detaching the subscriber from a resized terminal. That probe
overlapped compilation and was used only to find the retained buffer. The
roughly 4.7 MiB change also includes other released state; it is not assigned
entirely to this pool.

The subsequent clean daemon comparison uses three alternating pairs of the
preserved shipped CLI and a profiling CLI built with the pool change. All six
runs and 42 checkpoints pass, with matching captured terminal output, zero
swap, no compiler activity, and complete private-process cleanup. After
1,000 × 500 cells shrink to 107 × 43 and remain watched, median anonymous
memory is 8.3555 MiB shipped versus 4.5391 MiB fixed. Relative to each run's
initial small terminal, growth falls from 5,378,048 to 1,376,256 bytes:
exactly 4,001,792 bytes (3.8164 MiB) less in every pair. This agrees with the
separate 4,000,000-byte cell-payload measurement. Initial, large-terminal,
and detached memory match; detaching and reattaching also converge. The
saving is conditional on retaining obsolete resize buffers, not a startup
reduction. The fixed CLI SHA-256 is
`50eed1e8e191cf15c9325a3eeffac5b054b62f618f4f92718e71227cd0c1037b`.

Receipts: `pool-probe/counts.jsonl`, `pool-probe/timing-summary.json`, and
`resize-retention/result.json` for the discovery and isolated probes;
`resize-comparison-summary.json`, `pool-fixed-bin/build-receipt.json`, and
`pool-fixed-build-source-receipt.json` for the clean application comparison.
Source:
`crates/zz-terminal/src/session.rs`, `ViewportDictionary::commit_cell_plane`.

The change applies to native daemon hosts, including Linux, macOS and the
experimental Windows path. Remote and thin clients benefit when their daemon
is updated. It changes no wire format. The full terminal suite passes 463
tests with three ignored, including dirty-snapshot reuse and the new resize
ownership/retry checks. The retry test exercises retaining and clearing an
uncommitted resized plane; it does not inject a native extraction failure.
Terminal all-target/all-feature clippy passes with warnings denied.

## Font-plan storage

The shipped cache keeps at most six plans per face, but normal `VecDeque`
growth reserves eight slots after the fifth insertion. The follow-up adds
`reserve_exact(1)` before insertion. It preserves the six-plan
FIFO, key matching, database invalidation, and font selection. It changes only
container growth; it does not reduce the number of cached plans.

The new standalone probes use installed fonts and four fixed corpora. They
are separate from the deleted original shaping fixture. The fallback corpora
deliberately include U+10FFFF and U+10FFFE, which are noncharacters, and the
unassigned U+0378. Their unresolved glyphs drive an exhaustive font search;
these are missing-glyph stress cases, not representative ordinary multilingual
text. Per-scalar allocation attribution was not measured. Counting the bytes
released when dropping only the plan cache gives:

| Corpus | Shipped cache | Exact-growth cache | Saving |
| --- | ---: | ---: | ---: |
| ASCII or ordinary UI text, one loaded face | 2,324 B | 1,620 B | 704 B |
| Fallback with default features, 425 faces / 437 plans | 887,008 B | 441,024 B | 445,984 B (50.3%) |
| Fallback with five feature sets, 425 faces / 2,132 plans | 2,439,248 B | 1,992,912 B | 446,336 B (18.3%) |

Preallocating six slots per face saves less in the five-feature case and adds
297,792 bytes in the default-feature case. The follow-up rejects that policy.
Exact growth adds 11 cold allocation calls to the default-feature fixture
and 1,282 to the five-feature fixture. Plan constructions and hits remain
identical. All variants match complete shaped-output hashes, glyph counts,
and missing-glyph counts across both the system allocator and mimalloc.

Separate binaries initially showed about 3% more CPU for warm ordinary text.
A control puts both growth policies in the same binary, selects the policy
only on cache misses, and alternates 15 pairs with mimalloc 0.1.52/no_thp.
The apparent regression does not survive that control. Median paired warm
CPU changes are −0.48% for ASCII, −0.82% for UI text, −0.30% for default
fallback, and −0.88% for five-feature fallback; every range spans zero.
The ordinary-text difference is −0.450 milliseconds per 8,192 lines, with
a range of −2.985 to +0.759 milliseconds. Cold construction also shows no
consistent penalty. This establishes no detected CPU regression in these
fixtures, not a new shaping-speed claim.

In the same-binary runs, first-corpus anonymous memory falls from a median
10,788 to 10,296 KiB for default fallback and 13,096 to 12,372 KiB for
five-feature fallback. Ordinary UI text shows no measurable resident saving.
Both fallback fixtures load 425 faces from 399 font mappings, about 52 MiB
of resident file pages. Dropping plans leaves approximately 8.6 MB of requested
live allocations in either policy, including font objects, metadata, matching,
and scratch storage. Codepoint-support caches remain empty in these fixtures.
The ordinary fixture's 704-byte saving does not explain the historical GUI
startup offset.

Requested allocation sizes exclude allocator slack, C allocations, and font
file mappings. Releasing plans does not immediately return all their bytes
to the OS. Full-smaps parsing happens outside CPU timing and can retain
temporary allocator pages; the resident figures above precede that parsing.
The evidence supports a small reduction in fallback-heavy workloads, not a
fixed RAM saving for every application window.

The same-binary SHA-256 is
`9f8270f5be2a9c07204d81c631e21d557f167153768dc48058569f13a4bc144a`.
Sources: `third_party/rust/cosmic-text/src/shape.rs`, `shape_fallback`, and
`src/font/system.rs`, `FontSystem`. Receipts and reproducible probe sources:
`font-probe/decision.json`, `findings.json`, `policy-findings.json`, `probe.rs`,
and `setup.py` under the fresh artifact directory.

Validation of the changed production source passes all six cosmic-text unit
tests, native `no_std`, WebAssembly default and `no_std` compilation, and
identical cold/warm output hashes for all four corpora. The `no_std` checks
emit two unused-import warnings in unchanged upstream imports. This is
compilation coverage, not runtime validation on another platform.
Receipt: `font-validation/validation-standalone.json`.
All 28 GPUI platform text tests also pass against the changed production
source, including bidirectional text, combining characters, ligatures and
font changes. Receipt: `font-validation/validation-platform.json`.

The changed cosmic-text path serves Linux, FreeBSD, WebAssembly and iOS.
Native macOS uses CoreText and native Windows uses DirectWrite, so this
storage change does not affect their text backends. Runtime evidence here is
Linux-only. The increased allocation count on cold cache misses is the main
tradeoff; glyph output and the six-plan eviction policy remain unchanged.

The larger retained font mappings remain an investigation candidate. A missing
cluster keeps `shape_fallback` searching; on Unix the final fallback stage can
visit all matched faces. `FontSystem::get_font` retains `Arc<Font>` values,
fontdb retains shared file mappings, and GPUI retains fonts selected for
output. Clearing one owner cannot release the mappings. fontdb exposes
`make_face_data_unshared`, but cosmic-text has no public selective font-cache
eviction API. A standalone full-reset probe could measure a reclamation bound
while preserving the database and IDs; it would not establish a safe app idle
policy or avoid the CPU cost of rebuilding fallback plans. No font eviction
policy is introduced here. The approximately 52 MiB of resident font-file
pages are clean and file-backed, so the OS can reclaim them under memory
pressure. They are not 52 MiB of unreclaimable private heap; a future policy
must distinguish mapped-file residency from live private allocations.
