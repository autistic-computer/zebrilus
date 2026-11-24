---
type: Research
title: Linux browser pane pacing and dma-buf import cost
description: Measured CEF frame delivery on Linux with CEF's own BeginFrame timer against zz-driven external BeginFrames at 60 and 120 Hz, on the readback and GPU tiers, plus what each dma-buf import costs; external BeginFrames stay opt-in.
resource: libs/ala-cef/src/cef_runtime.rs
timestamp: 2026-10-28T18:44:35-03:00
tags:
- linux
- browser
- performance
- nvidia
---

# Frame delivery on the tested NVIDIA host

CEF's internal timer kept the page at the display refresh rate. zz-driven external BeginFrames sometimes
stalled. Page animation, CEF delivery and frames consumed by the UI were measured separately. The
desktop still selects external frames on Linux only when `ZZ_BROWSER_EXTERNAL_BEGIN_FRAME=1`, in
`crates/zz/src/browser/environment.rs` (`external_begin_frame`).

**Fixture.** zz `0fc63e82b` plus a resize-cover change, debug build with dependencies at optimization
level 2, on a CachyOS 7.2.11 host with RTX 3070 Laptop and NVIDIA 615.78.08. Headless gamescope 3.17 ran
at 1,600 × 1,000 and 60 or 120 Hz with private socket, profile and HOME. The browser viewport was 393 ×
580 beside a terminal. The host was on AC, governor powersave, with intermittent builds (load average
7–10). The local page animated a gradient each rAF and reported its own rate/gaps in its title.
Diagnostics counted CEF callbacks and successful mailbox takes. The first fifth of each run was
excluded.

On this host the normal shared-texture attempt timed out into readback. A temporary Vulkan feature
switch supplied DMA-BUFs for the GPU-tier measurements. Both the failure and the switch are
configuration-specific. [NVIDIA OSR study](/research/2026-08-07-nvidia-cef-accelerated-osr.md)

## Pacing results

Each cell gives delivered/consumed FPS, p95 consumed frame gap and median page rAF. Each configuration ran twice.

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

During the busy first 60 Hz readback run, the internal timer delivered paints in bursts of four to six
after message-pump gaps of 50–80 ms. The UI consumed 18 FPS while the page maintained 60 rAF. With the
external clock, page rAF fell to 7–9 by the end. In the second 120 Hz external GPU run there were no
paints for eight seconds, then 6–16 rAF. Build overlap was a possible contributor, not an established
cause. Mesa AMD/Intel, real desktop compositors, release builds and rates above 120 Hz remained
unmeasured. [CEF runtime](/browser/cef-runtime.md) · [OSR rendering](/browser/osr-rendering.md)

## DMA-BUF import cost

The GPU path delivered 861 frames at 60 Hz and 393 × 580 device pixels.

| Step | p50 | p95 | p99 | max |
| --- | --- | --- | --- | --- |
| Import (`vkCreateImage`, memory import, bind, wgpu wrap) | 338 µs | 556 µs | 736 µs | 996 µs |
| Copy into the destination, submit, error scope | 110 µs | 219 µs | 293 µs | 709 µs |

CEF rotated three buffers with three callbacks between reuses. Median import cost was about 2% of one
main thread at 60 Hz or 4% at 120 Hz if paid once per frame. A cache by device, inode, offset and size
was proposed, but cache hit rate and end-to-end savings were never measured with a cache installed.
Reuse would also have to respect ownership and synchronization. Current diagnostic target
`ala_cef::diagnostics::accelerated_copy` in `libs/ala-cef/src/cef_runtime.rs` reports `import_us` and
`copy_us`. The importer and explicit ownership transfers live in `libs/ala-cef/src/linux_dmabuf.rs`.
[Linux stack study](/research/2026-10-16-linux-performance.md)
