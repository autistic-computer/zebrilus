# Shared GPU frames in a zz browser pane

[Full measurement tables](gpu-results.md) · [Raw trials, frame IDs and binary hashes](../../bench/ladybird/gpu-samples.json.gz)

**Keep Ladybird as an opt-in Linux backend.** In four matched release pairs at 1152 × 768, opening a GPU-backed Ladybird pane added 255 MiB of PSS to a terminal session, compared with 438 MiB for CEF. Across individual pairs the saving was 182–185 MiB. Ladybird used 1.67 CPU seconds for the scroll workload against CEF’s 2.09, a 20% reduction in the medians. The page signalled readiness in 109 ms versus 353 ms. All four pairs favored Ladybird on these measures. On the separate six-second scrolling capture, Ladybird and CEF showed medians of 59.83 and 59.42 distinct frames/s. The paired difference ranged from −4.00 to +0.99 frames/s, so this campaign establishes a CPU saving with comparable observed cadence.

CEF had a substantial advantage once the work moved into the page. The 96-million-iteration integer checksum took 852 ms in Ladybird and 85.5 ms in CEF. Forty-eight synchronous layouts took 175 ms and 74 ms respectively. Both engines produced the required checksums. These are reasons to keep CEF available for interactive web apps, alongside the static-canvas issue below.

## What changed

At 1152 × 768, the release desktop consumed a median 3.22 CPU seconds for 240 scroll steps with CPU painting, 3.04 with GPU painting plus bitmap delivery, and 1.72 with GPU painting plus DMA-BUF. Every paired block favored sharing. Its paired saving over GPU painting plus bitmap delivery was 1.34 CPU seconds, with a range of 1.00 to 1.40. Host reads fell from 810 MiB to roughly 4 KiB. KWin CPU is additional.

The remaining cost is concentrated in the frontend. With shared frames, zz spent 0.64 more host CPU seconds than stock Qt per 240 large-viewport scroll steps. The paired engine CPU difference was −0.03 seconds with a range of −0.07 to +0.03. Switching to GPU painting also increased the first-pane resident-memory increment, from 205 to 255 MiB at the larger size. These figures compare Ladybird configurations on one NVIDIA laptop.

The median distinct frame rate was 60.00/s with DMA-BUF at both sizes. Six of eight measured trials showed every generated scroll step. The other two showed 354 and 330 of 360 steps, yielding 59 and 55 distinct frames/s. The page reported 60 callbacks/s in all eight. CPU painting showed medians of 56.92 and 59.33 distinct frames/s at the smaller and larger sizes respectively.

The GPU warm-ups found a correctness issue. After the initial draw, both zz GPU configurations showed the page background where the canvas marker should have been, at both sizes. Stock GPU Qt also missed that marker at 1152 × 768. CPU-painted zz showed it at both sizes. Once the canvas animated, every arm showed the expected marker and the correct page pixels after scrolling.

## Running the spike

The Linux x86-64 worktree stages the pinned 2026.47.7 alpha. Run the experimental path in a fresh spike desktop with `ZZ_LADYBIRD_GPU=1 just ladybird run <url>`. The default invocation uses CPU painting. To keep GPU painting and force bitmap delivery, also set `ZZ_BROWSER_SHARED_TEXTURE=0`. The desktop's `capture-browser` command works with bitmap delivery. The shared path can be captured at the compositor.

The zz desktop gives the Hub its Vulkan device and driver UUIDs. The Compositor exports BGRA DMA-BUFs, which zz imports once per backing store. For each presented frame zz copies the visible rectangle into an owned GPU texture, waits for that submission on the IPC thread, then signals that the producer can reuse the previous backing store. The mailbox hands the owned texture to ala. The copy and the wait both count toward the measured cost. The pinned adapter handles single-object, single-plane AR24. A Vulkan import error requests bitmap presentation for that view.

## Measurements

The four configurations isolate two changes. Comparing zz CPU painting with zz GPU painting plus bitmap delivery measures the effect of moving painting to the GPU while continuing to copy pixels through the host. Comparing the two GPU-painted zz runs isolates shared presentation. Stock Qt runs the same engine helpers and libraries. A separate paired campaign runs both release engines inside zz at 1152 × 768, using the staged CEF 156 release desktop from the original spike at zz base `e6d337080`. The archive records the executable and library hashes.

Each resource trial starts with a fresh profile on a private 1600 × 1000 Wayland display. The CEF control uses a disposable desktop configuration with its wallet disabled to avoid a first-use prompt. The page must report its exact viewport at scale 1. The runner uses the original baseline's 240 canvas steps, 240 scroll steps, integer checksum and synchronous layout checksum. It distinguishes the desktop, engine helpers, daemon and shell. Whole-app CPU includes all four. KWin is recorded separately. The CEF browser process embeds engine code in the zz desktop, so the whole-app figure is the comparable CPU metric across engines.

Each four-arm campaign covers all four positions per arm at each size. The direct comparison alternates order. Warm-ups validate canvas and scrolled-page pixels from compositor captures. The initial one-shot canvas draw has a separate recorded result. A measured resource block requires at most 1 MiB of sampled app-tree proportional swap across all its arms. Page readiness runs from issuing the open command to the page script reporting its geometry after two animation callbacks. The screenshot separately checks visible content. Startup times and preflight system activity are available in the raw data.

Host reads are Linux `rchar`, including IPC and reads from bitmap file descriptors. PSS measures resident CPU-addressable pages proportionally across processes. The NVIDIA driver also reports per-process GPU memory at idle and after the scroll, outside the timed intervals. Shared allocations can be attributed to multiple clients. The two accounting systems measure different things.

The presentation run scrolls the same rows by 16 CSS pixels per callback for six seconds. A 12-pixel-high fixed strip encodes the current frame index. A private PipeWire stream samples eight rows of KWin's composited output, with a negotiated maximum of 60 frames/s. The decoder counts distinct, ordered indices and preserves each stream timestamp, repeats and skipped indices. It validates idle boundaries, page visibility and the final scroll position. These counts measure frames observed at the compositor. Physical scanout, input latency and refresh rates above 60 Hz remain unmeasured. Capture runs separately from resource sampling. One incomplete resource block overlapped a development smoke test and was restarted before admission. The first CEF presentation warm-up failed the pattern validator and was repeated. Its log retained the beginning and end of the stream and did not identify the individual bad sample. The runner now saves the full trace on this failure.

## Reproduction

Install the baseline dependencies plus PipeWire, WirePlumber, GStreamer with `pipewiresrc`, Python GObject bindings, a C compiler, Wayland development files and plasma-wayland-protocols. Set `ZZ_SCREENCAST_XML` when the KDE screencast protocol XML is installed outside the usual system directory. NVIDIA process-memory sampling uses `nvidia-smi` when available.

```sh
just ladybird gpu-baseline target/gpu-comparison

ZZ_DEV_BUILD=1 cargo xtask bundle-cef --release --output target/cef-comparison
python3 bench/ladybird/baseline.py --stock target/ladybird-stock \
  --bundle target/ladybird-app --cef-desktop target/cef-comparison/zz \
  --size 1152x768 --blocks 4 --output target/cef-resources
python3 bench/ladybird/baseline.py --stock target/ladybird-stock \
  --bundle target/ladybird-app --cef-desktop target/cef-comparison/zz \
  --size 1152x768 --blocks 4 --presented-seconds 6 --output target/cef-presented

python3 bench/ladybird/gpu_report.py bench/ladybird/gpu-samples.json.gz
```
