---
type: Research
title: What the 2026-10-16 visual changes cost on macOS
description: Frame, GPU and memory A/B of the Rex-sized chrome, text fade and glass-over-pages commit against the tree before it, the Primitive size fix that pays the regression back, the Metal glass backdrop blit that cost 170 MB of driver memory, and the follow-ups.
resource: crates/zz-gpui/src/scene.rs
tags:
- performance
- macos
- gpu
- memory
- gpui
- glass
timestamp: 2026-10-16T23:00:00Z
---

# Question

`d6b52ad11` reworked the chrome (Rex-sized pane headers, labels that fade instead of ending
in an ellipsis, glass that blurs native browser pages, picked outlines and shadows). Did it make
frames slower, and what is cheap to win back? Linux PR 17 was rebased on it and measured in the
same runs.

# Setup

MacBook Pro M4 Max, 48 GB, macOS 27.0.1. Every window opened on the Dell P2725QE (100 Hz,
scale 2), which no other window covered: a covered zz window stops drawing, so runs on the main
display read as idle whenever another app sat on top. 1280x900 window, `bench/gpui/frames.py`,
three alternating repeats, 15 to 20 s windows after a 5 s warmup, 60 fps in every terminal
scenario. Medians below.

| Label | Tree |
| --- | --- |
| base | `32ee3ab37` (the tree before the visual commit, same as `0d729eb36`) |
| main | `0e53ca4e0` (visual commit `d6b52ad11`) |
| pr | PR 17 rebased on main (`f0f806ca8`) |
| c1 | pr plus the `Primitive` change below |

All four were built alike: `cargo xtask bundle-cef --profile profiling` with
`CARGO_PROFILE_PROFILING_LTO=thin` and 16 codegen units, so absolute numbers sit a little above
a fat-LTO release. GPU time came from a local probe that wrote `GPUEndTime - GPUStartTime` of
each Metal command buffer; it is not in the tree.

Two configs: plain (a fresh config) and glass (`interface-theme = liquid-glass`,
`pane-gaps = true`, `window-background-blur = true`).

# Results

Main-thread CPU (% of a core) and draw time per frame (us):

| Scenario | base | main | c1 |
| --- | ---: | ---: | ---: |
| stream, plain | 10.66 / 1350 | 10.73 / 1330 | 9.60 / 1239 |
| grid (9 panes), plain | 14.30 / 2020 | 14.04 / 1918 | 13.19 / 1753 |
| scroll, plain | 1.86 / 593 | 1.92 / 619 | 1.79 / 569 |
| long sidebar names, plain | 11.46 / 1482 | 12.72 / 1591 | 11.00 / 1332 |
| palette, plain | 19.01 / 2701 | 19.80 / 2866 | 19.46 / 2702 |
| stream, glass | 11.32 / 1291 | 11.52 / 1312 | 10.19 / 1132 |
| grid, glass | 14.51 / 1827 | 14.76 / 1858 | 13.90 / 1766 |
| long sidebar names, glass | 11.99 / 1400 | 13.04 / 1562 | 11.32 / 1312 |

- Terminal scenarios did not regress: stream, grid, scroll and idle stay within 3% of base in
  both configs, on CPU and GPU.
- Labels that fade did. With 24 sessions whose names overflow the sidebar, main draws 29.5% more
  sprites (4369 to 5660), paint takes 28% longer and the main thread 11% more. The palette
  scenario draws 24% more sprites and costs 4% more main thread and 4.5% more GPU (70.9 to
  74.1 ms/s). `Window::insert_glyph_sprite` splits every glyph in a fade into one sprite per
  device pixel column, which keeps the shaders unchanged.
- A browser page under no glass stays a native layer in every build: 0 GUI frames, about 5.5%
  GUI CPU. With the palette open over an animated page, main draws the page itself at the page's
  rate so the glass can blur it: 0 to 30 fps, GUI CPU 5.5% to 18.6%, GPU 0 to 57 ms/s, footprint
  420 to 648 MiB. That is the feature working, paid for while glass covers a moving page.
- PR 17 on Metal: within 3% of main on stream, grid, scroll and the palette in the glass config.
  Its inactive-window cap works: a window that loses key status draws at about 24 fps instead
  of 60.

# The Primitive fix

A Metal sample of the stream scenario put 32% of draw time in
`Scene::insert_primitive::<MonochromeSprite>` and 18% of the main thread's busy time in
`memmove` under it. `Primitive` was 464 bytes because `Glass` (eight shapes and a full material)
sat in it inline, while a glyph sprite is 112. Every glyph converted into that enum, cloned it
for `place_primitive`, and pushed it again into `paint_operations`. `Primitive::Glass` now holds a
`Box<Glass>` (the enum is 176 bytes, the size of `Quad`), `place_primitive` takes it by
reference so nothing clones it, and a const assertion keeps it from growing back.

c1 against main: stream draw down 7 to 14% (paint down 30%), grid down 5 to 9%, the long sidebar
down 16%, which leaves it 6 to 10% under base despite the extra fade sprites. The palette goes
back to base. zz-gpui and the Metal platform tests pass.

# Memory

Launch footprint is 334 MiB for the plain config and for each glass knob alone, and 523 to
533 MiB with Liquid Glass, gaps and blur together, in base and main alike. The difference is in
`Owned physical footprint (unmapped) (graphics)` (227 to 407 MiB), as twenty more 8 MiB driver
chunks (48 against 28). The Metal device's own allocations do not explain it:
`currentAllocatedSize` ends at 84 MB, and the glass chain is 2048x1792 plus 1024x896 BGRA8,
about 18 MB.

Under that config the pane frames read their backdrop: their tint is translucent over the
translucent window plane, and the material keeps its saturation and brightness, so
`reads_backdrop` holds with no blur. Every frame then copies 2032x1784 of the drawable into the
chain's first level. Switches in a debug build split the cost:

| Glass drawing | Footprint | 8 MiB chunks |
| --- | ---: | ---: |
| as shipped | 524 to 533 MB | 48 |
| not drawn | 344 MB | 28 |
| drawn in the scene's encoder, nothing copied | 344 MB | 28 |
| encoders split and chain allocated, no copy | 361 MB | 28 |
| copy as a render pass sampling the drawable | 363 MB | 28 |
| plain config | 343 MB | 28 |

The blit encoder reading the drawable is what costs the 160 to 170 MB; the storage mode and
usage of the destination level change nothing. `MetalGlass` now copies with a render pass
(`fs_glass_copy` in `glass.wgsl`, a `textureLoad` per texel under each copy's scissor, with
`GlassBlurUniform::copy` carrying the offset). Streaming under that config drops from 533 to
362 MB. `bench_glass` keeps its times within 3% (full-window clear glass 0.721 to 0.703 ms at
5344x2964), and the glass pixel checks pass.

The wgpu renderer (Linux, web, iOS) still copies with `copy_texture_to_texture`, and on iOS
that costs nothing. On an iPhone 15 Pro Max (A17 Pro, release build, the phone attached to a
streaming session on this Mac, `phys_footprint` from `TASK_VM_INFO` every 2 s, medians of 17
samples):

| Phone | Footprint |
| --- | ---: |
| default theme | 140, 141 MB |
| Liquid Glass, as shipped | 191, 184, 183 MB |
| Liquid Glass, backdrop copy skipped | 190 MB |
| Liquid Glass, surface not readable (no glass drawn) | 136 MB |

The 45 to 50 MB Liquid Glass adds on the phone comes from reading the backdrop at all (a copyable
surface and the blur chain at 3x), not from the blit. The iOS simulator shows none of it: its
Metal runs through `SimMetalHost`. A render-copy port to wgpu passed every wgpu glass check on
Vulkan (RX 7900 XTX) but stays out of the tree, since it saves nothing on the phone and wgpu-hal
30 does not offer `TEXTURE_BINDING` on Metal surfaces anyway.

# Follow-ups

- Fade glyphs in the sprite shaders (one sprite per glyph, the span packed into the sprite's
  unused `pad`) instead of one sprite per column. Removes the 24 to 30% extra sprites and the GPU
  increase; touches the Metal, three WGSL and the HLSL shaders.
- On the phone, split the 45 to 50 MB of backdrop reading between the copyable surface and the
  blur chain, and see whether the pill can read less than it does.
- Whether pane frames with no blur need the backdrop at all: their backdrop is the flat window
  plane, so the result could be worked out without reading it.
- Decide whether a page under glass should redraw at the page's full rate.
- Grid frames are still mostly layout of about 115 uncached nodes, see
  [where a zz frame goes](/research/2026-10-06-surface-cost.md).
- Repeat this check on Linux, where the glass reading pipeline is wgpu.

# Harness notes

- Covered windows draw nothing. `frames.py --display-uuid` opens the window on another display.
- Profiling bundles are ad hoc signed and only debug builds pass `use-mock-keychain`, so the
  first browser pane asks the Keychain for Chromium's Safe Storage key. The prompt takes focus,
  and builds with PR 17's inactive-window cap then draw at about 24 fps in every later scenario.
  The browser scenario is opt-in for that reason.
