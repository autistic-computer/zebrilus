---
type: Research
title: Linux browser pane pacing and dma-buf import cost
description: Measured CEF frame delivery on Linux with CEF's own BeginFrame timer against zz-driven external BeginFrames at 60 and 120 Hz, on the readback and GPU tiers, plus what each dma-buf import costs; external BeginFrames stay opt-in.
resource: crates/zz-browser/src/cef_runtime.rs
tags:
- linux
- browser
- performance
- nvidia
---

# Question

Two ideas came out of comparing zz's browser pane with another app's WPE
backend: let GPUI's display clock drive Chromium's BeginFrames on Linux, and
cache dma-buf imports by buffer identity instead of importing each frame.
Neither was worth building without numbers. This note has the numbers.

# Setup

- alienware: RTX 3070 Laptop, NVIDIA 615.78.08, CachyOS 7.2.11, on AC with the
  `powersave` governor. Another session's cargo builds ran on and off during the
  runs (load average 7 to 10), which is the noise in rep 1.
- zz at `0fc63e82b` plus the resize-cover change, debug build (dependencies at
  `opt-level = 2`, so wgpu, ash and the driver run optimized).
- Headless gamescope 3.17 at 1600x1000, `-r 60` or `-r 120`, isolated socket,
  XDG dirs and `HOME`. One terminal pane and one browser pane split side by side
  (browser viewport 393x580 logical).
- The page fills a canvas with a new gradient every `requestAnimationFrame` and
  writes its own rAF rate and frame gaps into `document.title`, sampled once a
  second through `list-panes -F '#{pane_title}'`.
- Delivered: CEF paint callbacks (`zz_browser::diagnostics::cef_paint` for
  readback, `zz_browser::diagnostics::accelerated_copy` for the GPU tier).
  Consumed: `frame_mailbox take` lines that returned a frame. First fifth of each
  run dropped as warmup.
- Readback tier: the default on this NVIDIA box, where the GPU tier gets no first
  frame and the two-second guard recreates the pane in readback mode
  ([NVIDIA OSR](/research/2026-08-07-nvidia-cef-accelerated-osr.md)).
- GPU tier: a temporary, uncommitted `enable-features=Vulkan` switch, which makes
  CEF deliver dma-bufs on this driver. Not a production setting; it only made the
  GPU path measurable here.

# Pacing results

Delivered / consumed frames per second (p95 consumed gap) and the page's rAF
median, two reps each. `int` is CEF's internal timer (the Linux default), `ext`
is `ZZ_BROWSER_EXTERNAL_BEGIN_FRAME=1`.

| Tier | Hz | BeginFrames | Rep 1 | Rep 2 |
| --- | --- | --- | --- | --- |
| readback | 60 | internal | 48 / 18 (170 ms), rAF 60 | 60 / 60 (19 ms), rAF 60 |
| readback | 60 | external | 36 / 37 (57 ms), rAF 42 | 60 / 60 (19 ms), rAF 60 |
| readback | 120 | internal | 120 / 120 (9 ms), rAF 120 | 120 / 119 (11 ms), rAF 120 |
| readback | 120 | external | 117 / 117 (11 ms), rAF 118 | 118 / 118 (10 ms), rAF 118 |
| GPU | 60 | internal | 60 / 60 (21 ms), rAF 60 | 60 / 59 (18 ms), rAF 60 |
| GPU | 60 | external | 59 / 59 (22 ms), rAF 60 | 60 / 60 (18 ms), rAF 60 |
| GPU | 120 | internal | 120 / 120 (10 ms), rAF 120 | 119 / 119 (10 ms), rAF 120 |
| GPU | 120 | external | 114 / 113 (13 ms), rAF 116 | 10 / 9 (380 ms), rAF 8 |

Rep 1's 60 Hz readback rows ran while the other session's build was compiling.
With CEF's timer, `on_paint` calls landed in bursts of four to six after 50 to
80 ms gaps in the message pump, so GPUI took 18 of the 48 frames CEF painted,
but the page itself kept its 60 rAF. With the external clock the page fell to
7 to 9 rAF by the end of the run. Rep 2's GPU external run at 120 Hz painted
nothing for its first eight seconds and then crawled at 6 to 16 rAF. Under
CEF's own timer the page's rAF never dropped below the display rate in any run.

# Pacing conclusion

External BeginFrames buy nothing on Linux here and are fragile. With CEF's
internal timer every configuration holds the display rate (60 at 60 Hz, 120 at
120 Hz), and the external clock is at best the same, slightly behind at 120 Hz,
and in two of eight runs it collapsed to single-digit rates, which is the blank
or stalled pane the opt-in default was guarding against
([CEF runtime](/browser/cef-runtime.md)). They stay opt-in on Linux/FreeBSD.
Why the external clock collapses is not understood yet; both collapses may
have overlapped the other session's builds.
The "let GPUI's vblank drive Chromium" variant was already ruled out on
macOS: riding `on_next_frame` halved the browser frame rate
([OSR rendering](/browser/osr-rendering.md)).

Unmeasured: AMD and Intel (Mesa), a real KDE or GNOME session instead of
headless gamescope, displays above 120 Hz, and release builds.

# dma-buf import cost

The GPU tier imports CEF's dma-buf as a new Vulkan image and memory every
callback, then copies it into a zz-owned texture. `AcceleratedFrameProducer::produce`
now logs both halves under `zz_browser::diagnostics::accelerated_copy`
(`import_us`, `copy_us`; enable with `RUST_LOG=zz_browser::diagnostics=trace`).

861 frames at 60 Hz, 393x580 device pixels, NVIDIA with the Vulkan switch:

| Step | p50 | p95 | p99 | max |
| --- | --- | --- | --- | --- |
| Import (`vkCreateImage`, memory import, bind, wgpu wrap) | 338 µs | 556 µs | 736 µs | 996 µs |
| Copy into the destination, submit, error scope | 110 µs | 219 µs | 293 µs | 709 µs |

CEF rotated three buffers (`unique_handles=3`, `reuse_gap_callbacks=3`). With
zz's external message pump this callback runs on the main thread, so the import
is about 2% of the main thread at 60 Hz and 4% at 120 Hz, or 4 to 7% of a
120 Hz frame budget. A three-entry cache keyed by the identity zz already
computes (device, inode, offset and size per plane) would remove nearly all of
it. It was not built: the GPU tier is not the default on NVIDIA, Mesa's import
cost is unmeasured, and a cached image needs explicit acquire barriers that a
fresh import currently gets by accident.

# Related

* [OSR rendering](/browser/osr-rendering.md)
* [CEF runtime](/browser/cef-runtime.md)
* [Linux stack performance](/research/2026-10-16-linux-performance.md)
* [NVIDIA OSR](/research/2026-08-07-nvidia-cef-accelerated-osr.md)
