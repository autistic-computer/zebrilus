---
type: Research
title: Linux stack performance and rendering validation
description: Matched Linux CPU and RAM measurements, resource lifetime checks, accepted optimizations, and cross-platform validation follow-ups for the terminal, browser, text, and rendering stack.
resource: libs/ala-cef/src/linux_dmabuf.rs
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

# Linux measurements

On AMD Wayland, three matched pairs streaming 512 MiB of mixed terminal output cut combined GUI/daemon
CPU by 35.81–36.71%. Three browser animation pairs sustained their cadence with lower submit cost.
Ordinary terminal streaming, ASCII throughput and a coalesced agent burst showed no repeatable app-level
gain. The original candidate used more PSS (proportional set size) at startup. Later rebuilds did not
reproduce that as a consistent anonymous-memory increase.

**What was measured.** Baseline `67ccece76` and candidate `253be803e` used Linux 7.3.0-9-generic, Ryzen
7 7800X3D, eight online cores, GNOME Wayland and AMD Vulkan. The final display was a Dell P2725QE at
3,840 × 2,160, 100 Hz, scale 1.5. The app window was 1,280 × 900 logical pixels with 12-point DejaVu
Sans Mono. An earlier 120 Hz scale-1.25 display was excluded from matched results. Every run had fresh
private processes, matching geometry/inputs and combined-package profiling features. The candidate GUI
predated the view-area fix, so its runs connected to the final CLI-owned daemon. CPU is percent of one
core or process CPU seconds as labeled. Clean timings excluded builds, traces and interception.

The original checkout and ignored profiles were removed. Only the baseline CLI was recovered from a
running process and verified by SHA-256. The original GUI, candidate binaries and runner were
unavailable for the RAM follow-up. The committed run IDs, full hashes, receipt names, original checks
and per-run qualifications are in the
[full October record](https://github.com/autistic-computer/zebrilus/blob/4fa976578/knowledge/research/2026-10-16-linux-performance.md).
The fresh RAM runs used their own fixtures and `target/profiles/linux-ram-20261016/`.

**Source check at `1ce632dce`.** The DMA-BUF importer and readback uploader now live in
`libs/ala-cef/src/linux_dmabuf.rs` (`import_texture`) and `readback_texture.rs`
(`ReadbackTexture::upload`). The vendored `third_party/ghostty/src/terminal/page.zig` still memoizes
row-local hyperlinks in `clonePartialRowFrom`, and `PageList.zig` sets `page_preheat` to one. The live
cell-plane cache and per-face shaping FIFO are described below. These checks verify the mechanisms in
source. All performance figures here belong to the October builds.

## Mixed terminal output

Three fresh pairs used 107 × 43 cells. The change column gives the median paired change, which can
differ from the ratio of the two independently reported medians.

| Mixed-output metric | Baseline | Candidate | Paired change |
| --- | ---: | ---: | ---: |
| Workload wall time | 38.716 s | 30.486 s | −21.05% |
| GUI CPU | 29.78 s | 12.45 s | −58.46% |
| Daemon CPU | 39.37 s | 31.55 s | −19.86% |
| GUI + daemon CPU | 69.15 s | 43.96 s | −36.49% |

A separate instrumented pair measured draw wall median 23.009 → 10.077 ms, p95 30.948 → 13.960 ms, and
prepaint median 21.548 → 8.675 ms. Shaped glyphs were similar (3,128 → 3,117/frame), and FPS rose 36.34
→ 39.90. Ordinary stream/grid stayed near 63 FPS. At 150 MiB ASCII the wall median was about 0.323 s in
both variants with one 1.105 s baseline outlier under 74.4% host load.

Five rotated native ReleaseFast trials used identical 64 MiB inputs, terminal geometry and
formatted-state hashes. Row-local hyperlink memoization avoids repeat URI hashing and destination
lookup.

| Workload | Baseline CPU | Memo CPU | Change |
| --- | ---: | ---: | ---: |
| Mixed output | 4.7290 s | 3.6912 s | −21.94% |
| Repeated OSC 8 hyperlink spans | 4.6879 s | 2.2801 s | −51.36% |
| ASCII | 0.09885 s | 0.09869 s | No measurable difference |

Reducing page preheat from four pages to one took 10,000 create/free cycles from 0.16699 to 0.06105 s
CPU. For 1,000 cycles, mmap calls fell 4,008 → 1,008 and munmap 4,000 → 1,000. Sustained 512 MiB ASCII
CPU held at 0.79062 → 0.78914 s. Inherited maximum RSS was unusable. The counts measure allocation
churn.

- **Copy mode.** On fresh frozen revisions, extracting 120 × 40 cells instead of the full 2,000 × 2,000
  took 0.1167 versus 92.54 ms and 38,400 versus 32,000,000 packed bytes. Both sides used the candidate.
  Selection and search retained the full underlying frozen history. The first integration clipped 41 ×
  24 to a stale 39 × 24 after focus changes. Refreshing existing view areas fixed it. With two panes/two
  clients, 1,000 unchanged visibility refreshes preserved all four frame identities. See
  `copy_mode_snapshot` and `Shared::refresh_terminal_visibility`.
- **Text shaping.** A six-entry FIFO per font face took 1,024 mixed lines from 0.9948 to 0.3367 s over
  eight alternating runs, user CPU from 0.97 to 0.33 s, and constructions from 246,122 to 2,125. The
  425-face fixture retained about 2.5 MiB more allocator storage and 2,638 KiB more median maximum RSS.
  Across 1,464 records/three cache orders, 39,924 glyphs and 2,160 missing-glyph results matched.
  Shaping excludes ANSI processing and app rendering.
- **Row paint.** Reusing freshly computed raster data in `crates/zz-ui/src/terminal.rs`
  (`RowRenderCache`) produced identical sprites in 88 paints across five scales with clipping, opacity,
  synthetic styles and fractional origins. Glyph-cache lookups fell 1,408 → 880. This is an operation
  count.

## Browser animation and ownership

Each accepted pair had a visible, focused document painting an advancing 1,498 × 1,138 canvas at device
scale 1.5. CDP receipts bracketed the timing. PNG readback ran outside it. A hidden startup with a
transparent canvas invalidated an earlier run. Explicit pane activation enabled the third pair to pass.
The cause of hidden startup was unresolved. Each row is a successive 10 s interval, with medians over
three pairs.

| Animation interval | Baseline GUI + daemon | Candidate GUI + daemon | Paired change |
| --- | ---: | ---: | ---: |
| First | 20.90% | 11.50% | −43.90% |
| Second | 36.40% | 11.70% | −68.21% |
| Third | 50.90% | 11.30% | −78.00% |

CEF child CPU stayed near 10%. Including children, the final interval was 61.30 → 21.40% of one core.
Baseline GUI system CPU grew 1.36 → 4.36 s per interval while the candidate stayed at 0.29–0.34 s.
Static CPU overlapped at 2.1–2.2%, after-close below 1%. GUI/daemon PSS was 239.37 → 251.54 MiB before
animation, 249.89 → 260.98 MiB in the final interval and 256.61 → 268.47 MiB after close. Seven CEF
children persisted until shutdown, with 229.95 → 232.73 MiB PSS after close.

A separate recorded pair ran at 99.4–99.9 FPS on both builds. Median CPU-side present/submit time grew
0.881 → 2.954 ms across the baseline intervals versus 0.178 → 0.186 ms in the candidate. Final p95 was
3.487 versus 0.490 ms. Draw medians stayed near 0.3 ms, with *higher* candidate draw p95 (0.523–0.571
versus 0.305–0.421 ms). This measured submit wall time, excluding GPU execution.

- The import transfers the Vulkan image and dedicated allocation to wgpu, including cleanup on
  allocation/bind failure. Multiple modifier planes must share one DMA-BUF allocation. Debug
  imports/frees balanced at 1,385/1,385 (peak two live). The optimized app balanced 1,405/1,405, zero
  after close and GPU drain. Fault injection covered descriptor closure, destruction order and fallback.
  Counts alone cannot identify unique VRAM bytes.
- DRM GUI-client total/resident VRAM medians across animation were 138.27/151.23/157.52 MiB versus
  110.23/110.23/103.55 MiB. After close they were 141.73 versus 87.55 MiB. Shared buffer accounting
  prevents summing GUI and CEF usage.
  [Kernel DRM accounting](https://www.kernel.org/doc/html/latest/gpu/drm-usage-stats.html)
- For readback, three alternating test-profile pairs compared 120 uploads of a 1,920 × 1,080 frame with
  uploads of 256 × 128 damage into the same texture. Elapsed time fell 48,734 → 4,293 µs and pixel bytes
  995,328,000 → 15,728,640, with identical pixels. This excludes the full CEF CPU copy and final
  compositing. New sessions, size changes, generation gaps and unknown damage required full upload.
  Transparent-window composition was outside the opaque CEF fixture.

## Agent and scheduling

Six alternating adapter trials grouped tool payloads by tool ID. Replaying 1,000 tools fell 13.62 → 1.10
ms and 10,000 tools 1,167.30 → 10.42 ms. At 10,000 retained tools, 1,000 unchanged updates fell 255.05 →
0.675 ms. In the test profile, 2,000 Markdown document clone/drop operations took 12.59 ms → 21.58 µs
for eight paragraphs and 870.79 ms → 21.32 µs for 512. Penna's `components/text/document.rs` and
`state.rs` share blocks by Arc and copy on append.

In three unpaced app pairs the 20 s GUI/daemon CPU medians overlapped (0.560 → 0.580 s). All six
journals preserved 4,000 chunks and 1,000 tools in order. A *separate* recorded pair waited for
end-of-turn, returning the same 499,112-character transcript in 491 → 489 ms. It drew 60 frames on each
side, median 0.959 → 0.983 ms, p95 1.593 → 1.996 ms, with an idle tail and no settled draws. Persistence
in the clean runs does not prove GUI completion there.

A Wayland probe measured 64.2 FPS focused, 20.7 inactive, 63.6 after reactivation and 35.0 ms for one
pending inactive update. The older focus run used a different scale and load. The inactive cap in
`libs/ala/crates/ala/src/window.rs` preserves required presentation. `watchdog_check_interval` in
`crates/zz/src/diagnostics/mod.rs` reduced healthy observer checks from 100 to at most 25 per 10 s in
the original timing model. Tests covered 190 freeze offsets, detection within threshold + 100 ms and
recovery within 100 ms. Live voluntary context switches fell from 10.0 to 2.5/s in four steady phases.

Thirty simulated 60/120 Hz consumer trials with a 120 Hz producer compared 10 ms to 16 ms publication.
At 60 Hz it added 11.6% CPU for 0.28% more displayed updates. At 120 Hz it added 10.8% CPU for 58.8%
more updates, excluding encoder/GUI cost. Starting the timer at batch start was flat. The study retained
the default. Display-aware cadence needed a per-consumer demand and delivery design.

| Proposal | Measurement | Decision |
| --- | --- | --- |
| Cache external texture views and bind groups | Creating a pair costs a median 1.289 microseconds; cloning costs 10 nanoseconds. Maximum saving at 60 Hz is about 0.077 milliseconds of CPU per second per surface. | Keep the current ownership model; another cache must track rotating textures and release closed browsers. |
| Retain copy-flash scratch resources between animations | 120 shader frames take 21.86 milliseconds versus 4.24 for empty scenes. Input/target textures already survive all 120 frames and are released afterward. | Keep reuse within each animation; retaining two 1080p targets between animations costs about 15.8 MiB per depth. |
| Remove resize completion waits | A 120-draw resize loop takes 231.76 milliseconds. Across resize, setup, and recovery, 129 device-idle calls take 0.734 milliseconds and 385 timeline waits take 3.15 milliseconds. | Preserve completion before destruction/reconfiguration; the waits are a small measured fraction and configure still requires synchronization. |
| Disable post-input scene replay globally | 120 clean frame requests during sustain produce 120 full draws; opting out produces zero. | Preserve the existing VRR underclocking mitigation. The backend lacks a reliable actual-VRR-active signal for a targeted fixed-refresh policy. |

The resize probe used small 128/129-pixel Wayland windows and eight recoveries on AMD Vulkan. The replay
count did not establish behavior on actual VRR hardware.

## RAM follow-up

The original app matrix reported GUI/daemon PSS rising 144.91 → 160.13 MiB at startup, 169.15 → 183.33
MiB at 32 idle windows and 297.33 → 314.67 MiB after mixed output. These were successive workload
endpoints. Growth through mixed output was 152.41 versus 155.02 MiB. The raw maps were lost, so the
reason for the initial offset remains unknown.

Rebuilt CLIs with the same feature set differed by 288 bytes of loadable file segments and −32 bytes of
loadable memory segments. Three alternating daemon pairs used identical 107 × 43 terminals and a TUI
subscriber from another executable inode. Each checkpoint had three samples, zero SwapPss and
AnonHugePages.

| Daemon phase | Baseline PSS | Candidate PSS | Baseline anonymous PSS | Candidate anonymous PSS |
| --- | ---: | ---: | ---: | ---: |
| Empty | 17.17 MiB | 16.54 MiB | 1.703 MiB | 1.695 MiB |
| One unwatched terminal | 20.76 MiB | 20.42 MiB | 2.609 MiB | 2.598 MiB |
| One watched terminal | 22.52 MiB | 22.05 MiB | 3.328 MiB | 3.324 MiB |
| 32 idle terminals, one watched | 30.62 MiB | 30.68 MiB | 11.371 MiB | 11.906 MiB |

The roughly 3.9 MiB daemon startup offset did not recur. At 32 terminals candidate anonymous memory was
592, 92 and 948 KiB higher. Starting a second daemon from the same inode reduced the first's PSS from
19.99 to 11.70 MiB, while anonymous memory stayed 2.402 MiB. Releasing it reversed the effect. An
identical executable on another inode had no such effect. This control establishes sensitivity to
sharing. [Linux proc accounting](https://cdn.kernel.org/doc/html/latest/filesystems/proc.html)

Three alternating GUI pairs compared rebuilt baseline and shipped `253be803e`, before either RAM
follow-up. The new fixture used explicit DejaVu fonts, 1,280 × 900 logical / 1,920 × 1,350 native, 104 ×
41 cells and warm OS page cache. Wayland commits and callbacks confirmed presentation/focus. All 48
checkpoints passed with zero process swap and no CEF children. These instrumented runs measured memory
only.

| Checkpoint | Baseline GUI + daemon PSS | Candidate GUI + daemon PSS | Baseline anonymous PSS | Candidate anonymous PSS |
| --- | ---: | ---: | ---: | ---: |
| Startup, 3 s after first presentation | 141.36 MiB | 143.41 MiB | 26.840 MiB | 26.836 MiB |
| Startup, 22 s | 140.83 MiB | 142.84 MiB | 26.246 MiB | 26.238 MiB |
| ASCII, 12 s after update | 142.45 MiB | 144.16 MiB | 27.711 MiB | 27.684 MiB |
| Unsupported-character fallback, 12 s | 200.15 MiB | 203.26 MiB | 36.008 MiB | 36.875 MiB |

Settled startup GUI anonymous differences (+0.156, +0.125, −0.141 MiB) changed direction. A 20.180 MiB
difference in executable PSS in one ASCII run reflected different page residency. Its cause was unknown.
The missing-character phase consistently increased candidate GUI anonymous memory (+0.973, +0.957,
+0.758 MiB). Font-file resident medians reached 52.80/53.26 MiB from about 2.67 MiB at startup. Both
builds had 87.42 MiB resident VRAM at startup/ASCII, 145.11 MiB after fallback and 2.004 MiB GTT. These
shared resources cannot be added to PSS. Mapping names do not identify the owner of anonymous
allocations.

### Terminal resize buffers

`ViewportDictionary::commit_cell_plane` in `crates/zz-terminal/src/session.rs` prunes old lengths on
live cell-plane commit. Consumer-held Arcs stay valid. Frozen copy history bypasses this path. On
mimalloc/no_thp with zero purge delay, growing 107 × 43 → 1,000 × 500 → 107 × 43 then applying 100
updates cut pooled payload 4,036,808 → 36,808 bytes with equal checksums and four logical plane
creations. A single obsolete 240 × 80 plane is 150 KiB. Seven alternating method repetitions measured how pruning affected each resize pattern.

| Workload | Shipped | Pruned | Meaning |
| --- | ---: | ---: | --- |
| Fixed 120 × 40 cells | 0.291 µs/update | 0.286 µs/update | Same two plane creations |
| Alternate 120 × 40 and 160 × 50 | 0.381 µs/update | 0.397 µs/update | About 0.015 µs added per update |
| Monotonic resize through 2,000 updates | 8.615 µs/update | 4.789 µs/update | Same creation count; fewer minor faults |
| Alternate 107 × 43 and 1,000 × 500 | 27.3 µs/update | 664.7 µs/update | About 0.64 ms added per extreme resize |

Extreme alternation needed 100 versus two planes and 48,850 versus 977 minor faults, with zero major
faults. Different clients share a maximum view bound, so their sizes alone do not force alternation. A
clean daemon comparison used three alternating pairs and 42 checkpoints. After shrinking while watched,
anonymous memory fell 8.3555 → 4.5391 MiB. Growth from initial small geometry fell 5,378,048 → 1,376,256
bytes, exactly 4,001,792 fewer in *each* pair. Initial, large and detached memory matched. The earlier
discovery run overlapped compilation. Its 8.31 → 3.58 MiB detach change included other state.

### Font-plan storage

In `libs/ala/third_party/cosmic-text/src/shape.rs`, `shape_fallback` reserves one additional slot before
insertion into a six-plan per-face FIFO. `FontSystem::db_mut` clears it. The earlier VecDeque allocated
eight slots after the fifth insertion. Separate installed-font corpora used U+10FFFF, U+10FFFE and
U+0378 to drive exhaustive missing-glyph fallback. These are stress fixtures.

| Corpus | Shipped cache | Exact-growth cache | Saving |
| --- | ---: | ---: | ---: |
| ASCII or ordinary UI text, one loaded face | 2,324 B | 1,620 B | 704 B |
| Fallback with default features, 425 faces / 437 plans | 887,008 B | 441,024 B | 445,984 B (50.3%) |
| Fallback with five feature sets, 425 faces / 2,132 plans | 2,439,248 B | 1,992,912 B | 446,336 B (18.3%) |

Preallocating six slots added 297,792 bytes in default fallback. Exact growth added 11 and 1,282 cold
allocation calls for the two fallback corpora, with unchanged constructions, hits, glyphs and
missing-glyph counts. An apparent 3% regression across separate binaries disappeared in 15 alternating
same-binary pairs with mimalloc/no_thp. Median paired warm CPU changes for ASCII/UI/default/five-feature
fallback were −0.48/−0.82/−0.30/−0.88%, all ranges spanning zero. UI text differed by −0.450 ms per
8,192 lines (−2.985 to +0.759 ms).

First-corpus anonymous memory fell 10,788 → 10,296 KiB and 13,096 → 12,372 KiB for fallback. Ordinary UI
showed no measurable resident saving. Both stress corpora loaded 425 faces from 399 font mappings, about
52 MiB of clean, reclaimable file pages. Dropping just the plans left ~8.6 MB requested live
allocations. The fixture cannot establish a safe eviction policy, its rebuild cost, or a general
window/startup saving. The fresh GUI matrix predated this change.

## Validation at the measured revisions

The October campaign accepted 18 clean and six frame-recorded app runs. It recorded 6,426 workspace
tests passed/21 ignored, 6,517 native Debug tests passed/68 skipped, 142 platform tests, five iOS host
tests, Vulkan pixel and Wayland recovery checks, strict workspace Clippy and a web build. The resize
follow-up recorded 463 terminal tests/three ignored. The font follow-up recorded six cosmic-text and 28
platform text tests, equivalent output hashes and native/WASM compile checks including no_std. iOS host
tests and WASM compilation provide no device/browser runtime measurement.

The campaign filed matched-runtime follow-ups for
[macOS #11](https://github.com/autistic-computer/zebrilus/issues/11),
[Windows/WSL #12](https://github.com/autistic-computer/zebrilus/issues/12),
[web #13](https://github.com/autistic-computer/zebrilus/issues/13),
[physical iOS #14](https://github.com/autistic-computer/zebrilus/issues/14),
[Intel/NVIDIA Linux #15](https://github.com/autistic-computer/zebrilus/issues/15) and
[FreeBSD #16](https://github.com/autistic-computer/zebrilus/issues/16). The results above cover AMD
Linux.
