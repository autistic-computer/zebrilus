# Attributing browser resource use

[Measured results](baseline-results.md) · [Raw trials and environment](../../bench/ladybird/baseline-samples.json.gz) · [Original CEF comparison](spike.md) · [GPU-sharing follow-up](gpu.md)

## Findings

Opening a first browser pane in a running zz cost a median **184 MiB at 786 × 608** and **194 MiB at 1152 × 768**. The Ladybird processes accounted for 180 and 185 MiB respectively. The zz desktop itself grew by 6 and 10 MiB. Across all accepted trials, the whole-pane cost ranged from 180 to 216 MiB. The two blocks per size with the least system swapping gave whole-pane medians of 208 and 204 MiB. The first-pane delta is more sensitive to system conditions than the small increase in the zz host.

At 1152 × 768, the headless engine occupied 184 MiB idle. Stock Ladybird occupied 332 MiB including Qt, and the complete zz session occupied 446 MiB, including its daemon and shell. A terminal-only zz session occupied 254 MiB at that window size.

Over 240 scroll steps, the zz frontend spent **0.63 extra CPU seconds** versus the stock frontend at the smaller size and **0.87 extra CPU seconds** at the larger one. The host difference was positive in all 12 matched comparisons. At the larger size its paired range was 0.77–0.94 seconds, while the difference charged to the engine ranged from −0.07 to +0.30 seconds. The zz process read 437.5 and 810.0 MiB respectively, within 4 KiB of one complete BGRA viewport per step in every accepted scroll trial. Stock Qt issued roughly 6 KiB of reads in each. This matches the spike's full-frame read before texture upload. The counters locate the extra CPU in the host, and leave its division between reading, upload and drawing open. [Bitmap read path](../../crates/zz-browser-runtime/src/ladybird/hub.rs)

The integer and synchronous-layout controls put nearly all measured CPU in WebContent. At 1152 × 768, median layout time was 179 ms headless, 180 ms in Qt and 182 ms in zz. The zz host recorded a median 0.00 CPU seconds during that task at 10 ms accounting resolution. Scrolling reached 60 JavaScript callbacks per second in all three arms.

## What the controls measure

| Control | What runs | What the comparison answers |
| --- | --- | --- |
| zz terminal only | The release desktop, private daemon and one idle shell | How much of the desktop is already resident when a user opens their first browser pane? |
| Headless Ladybird | The official Hub in manual headless mode and its normal helpers | What work does the engine perform on this page without an external graphical host? |
| Stock Ladybird | The official Qt browser and its Hub tree | What does a graphical browser spend presenting the same page? |
| Ladybird in zz | The same terminal session, plus a browser in a new mux window | What does a user pay to add a pane, and which processes receive that charge? |

The most useful RAM comparison for a zz user is the change **within the same desktop process** when the first browser opens. The most useful presentation comparison is zz versus stock Qt at the same visible viewport. The kernel charges CPU directly to the host, Hub and its helpers. Proportional set size (PSS) apportions shared resident pages among processes mapping them, so individual roles can gain or lose PSS as other processes open mappings. The two graphical hosts can also induce different work in the helpers. The earlier CEF comparison excluded the daemon and shell and used a different fixture. The new totals include both support processes, and the terminal-to-browser delta uses the same census at both ends.

## Experimental controls

- All three browser modes use the pinned Linux x86-64 Ladybird 2026.47.7 release with CPU painting. The harness verifies each embedded helper and engine library against the stock package by SHA-256. zz uses its release build from the spike with the dev identity. Its normal GPU-backed desktop presents the CPU-painted browser surface.
- One private 1600 × 1000 KWin Wayland display hosts the suite. The harness calibrates each graphical window until the page reports the exact requested CSS viewport at device-pixel ratio 1. The outer windows differ because the hosts have different chrome. Each trial gets a fresh home, browser profile and app configuration. The zz daemon uses its own socket and starts one clean `/bin/sh`. The stock profile marks the first-run welcome screen complete.
- Each size gets one excluded warm-up triplet. The six measured blocks per size contain every ordering of headless, Qt and zz once. The order of the two sizes alternates between blocks. Trials run serially against a loopback HTTP server with a warm filesystem cache. Both graphical hosts must display the expected canvas pixel and the scrolled content at the same screen coordinate in the warm-up. All trials must report a visible page, matching geometry and completed work.
- The quiet gate requires less than 40% system-wide CPU and memory PSI `some avg10` below 5% before each run. If the app processes' combined proportional swap exceeds 1 MiB at a checkpoint, the runner records and repeats the entire matched block. System-wide swap activity and CPU and memory stall counters are recorded separately. After the first swap-related rejection we added a sensitivity table. It recomputes the main paired differences for the lower-swap half of the accepted blocks and for blocks in which each arm saw at most 16 MiB of system-wide swap traffic. Ranking uses the busiest arm in each block. The archive retains the environment, warm-ups, exclusions, role inventories, checkpoints, counters, checksums and individual animation callback intervals.

## Workloads and counters

1. The zz desktop idles with its terminal visible for three seconds. The runner opens a browser pane through the normal CLI, leaving the terminal and daemon alive. The headless and Qt clocks begin just before fresh profile setup and process launch with the fixture as the initial URL. The page marks ready after two animation callbacks, drawing its initial canvas, and preparing 180 rows of content. The report labels each opening clock's starting event. It also reports time from the document's navigation start on its own clock.
2. After a 1.5-second settling period, the visible page idles for four seconds. Canvas then paints 240 updates, followed by 240 scroll steps of 16 CSS pixels each. Every step waits for an animation callback. The runner validates the callback count and final scroll offset. Each trial contributes its own mean callback rate, nearest-rank p95 and p99 interval, and count of intervals above 25 ms.
3. The page runs 96 million integer hash updates. It then creates a hidden 1,200-cell grid and changes its column count 48 times, reading the last cell's offset after each change to force layout. Both workloads return expected checksums. The page times each synchronous task and restores the grid within the same task. This gives an engine-heavy comparison with little graphical host work. [Fixture](../../bench/ladybird/baseline-page.html)
4. CPU is the change in user plus system time per PID, including its threads, in 10 ms ticks on this host. The runner checks PID and creation time at both ends of each phase. It counts the Hub and all its descendants as engine work, and the daemon and its shell separately from the desktop client. It also measures the private KWin process. The Python driver and desktop portals are outside the totals. A process born and exited entirely between boundary reads can escape CPU accounting.
5. Memory uses Linux `smaps_rollup` PSS and SwapPss at both boundaries and approximately once a second in between. Each trial contributes the median checkpoint and the highest sampled checkpoint. Table values are medians across trials. Paired differences are calculated before taking the median. Independent columns can differ slightly from the sum of their displayed medians. The OS selected CPU frequency dynamically. Host reads use `/proc/PID/io` `rchar`, the bytes returned by read-family calls across every file descriptor, including shared memory files. Access through memory mappings and GPU transfers are outside this counter. [Process accounting](../../bench/ladybird/baseline_accounting.py)

The measurements cover this machine, one page and one browser pane. The compositor screenshots prove that content reached the display surface at two points. The callback timings measure JavaScript cadence. Physical display cadence, input-to-photon latency, GPU time, VRAM and cold-cache startup require other measurements.

## Reproduce

On Linux x86-64, install the normal zz build dependencies, KWin Wayland with its virtual backend, Spectacle, a session D-Bus, Python 3.11 or newer, Pillow and psutil. Give each run a new output directory. The recipe builds the spike, verifies and stages the stock package, collects the trials and writes a Markdown report and a compressed archive.

```sh
just ladybird baseline target/baseline-run-1
```

Regenerate the tables from the checked-in archive without launching a desktop or installing Python packages.

```sh
python3 bench/ladybird/baseline_report.py bench/ladybird/baseline-samples.json.gz
```

An interrupted run can continue with the original runner arguments and `--resume`. The runner checks the host, executable hashes and benchmark settings, opens both graphical hosts on the fresh compositor to recheck each pending viewport, preserves prior attempts, and finishes the outstanding blocks.

For a longer campaign, use the staged binaries and a multiple of six blocks per size to retain balanced order.

```sh
python3 bench/ladybird/baseline.py --stock target/ladybird-stock \
  --bundle target/ladybird-app --blocks 12 --output target/baseline-run-12
python3 bench/ladybird/baseline_report.py target/baseline-run-12 \
  --archive target/baseline-run-12/samples.json.gz --output target/baseline-run-12/report.md
```
