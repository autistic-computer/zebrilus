# Browser resource baseline results

2026-11-23T18:30:50Z, 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz, 16 logical CPUs, 15.4 GiB RAM, Linux 7.3.5-2-cachyos-deckify, kwin 6.8.3.

6 accepted blocks per size, 2 warm-up blocks and 5 rejected blocks, with 1 campaign resumption. Each measured animation completes 240 steps. Values are medians across blocks. Paired differences show the median and the full range of individual differences. CPU is user plus system time across all threads. KWin runs separately and is excluded from app totals.

Preflight system CPU ranged from 0.3% to 36.8%. The highest sampled app-tree SwapPss was 52 KiB. The system swapped 194796 pages in or out during measured trials.

## 786 × 608 CSS pixels

### Idle memory and CPU

| Arm | Total MiB | Host MiB | Engine MiB | Daemon + PTY MiB | Sampled peak MiB | CPU % of one core |
| --- | --- | --- | --- | --- | --- | --- |
| zz terminal only | 255.1 | 228.4 | 0.0 | 26.9 | 255.1 | 0.50 |
| Headless Hub | 177.6 | 0.0 | 177.6 | 0.0 | 177.7 | 0.12 |
| Stock Qt | 324.1 | 150.3 | 177.3 | 0.0 | 329.2 | 0.12 |
| zz | 436.6 | 234.5 | 179.6 | 26.5 | 443.5 | 0.62 |

| Within-block difference | Total MiB | Host MiB | Engine MiB | Daemon + PTY MiB |
| --- | --- | --- | --- | --- |
| Open pane in zz | +184.2 (+179.9 to +208.1) | +6.2 (+4.3 to +6.5) | +179.6 (+173.5 to +201.8) | +0.0 (-0.9 to +0.0) |
| zz minus stock Qt | +114.4 (+109.7 to +134.2) | +86.1 (+81.6 to +96.3) | +3.9 (+0.0 to +6.6) | +26.5 (+25.6 to +32.2) |

### Opening the first page

| Arm | Start of clock | Median to ready ms | Range ms | Navigation to fixture ready ms |
| --- | --- | --- | --- | --- |
| Headless Hub | Profile setup and launch | 119 | 116–139 | 41 |
| Stock Qt | Profile setup and launch | 279 | 267–305 | 41 |
| zz | Pane request | 128 | 120–158 | 42 |

### CPU and reads per completed workload

| Work | Arm | Total CPU s | Host CPU s | Engine CPU s | Daemon + PTY CPU s | KWin CPU s | Host reads MiB | Sampled peak MiB |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| canvas | Headless Hub | 0.37 | 0.00 | 0.37 | 0.00 | 0.00 | 0.0 | 181.1 |
| canvas | Stock Qt | 0.81 | 0.32 | 0.49 | 0.00 | 0.27 | 0.0 | 327.9 |
| canvas | zz | 1.86 | 1.18 | 0.68 | 0.00 | 0.40 | 437.5 | 443.1 |
| scroll | Headless Hub | 1.44 | 0.00 | 1.44 | 0.00 | 0.00 | 0.0 | 187.4 |
| scroll | Stock Qt | 1.73 | 0.24 | 1.49 | 0.00 | 0.24 | 0.0 | 334.3 |
| scroll | zz | 2.51 | 0.90 | 1.62 | 0.00 | 0.27 | 437.5 | 449.7 |

| zz minus stock Qt | Total CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| canvas | +0.88 (+0.63 to +1.41) | +0.82 (+0.54 to +1.00) | +0.10 (+0.05 to +0.41) |
| scroll | +0.73 (+0.30 to +1.15) | +0.63 (+0.54 to +0.71) | +0.11 (-0.29 to +0.44) |

### JavaScript and synchronous layout

| Work | Arm | Page time ms | Page time range ms | Host CPU s | Engine CPU s | WebContent CPU s |
| --- | --- | --- | --- | --- | --- | --- |
| integer | Headless Hub | 874.8 | 833.3–1027.6 | 0.00 | 0.87 | 0.87 |
| integer | Stock Qt | 864.6 | 836.5–948.2 | 0.00 | 0.87 | 0.86 |
| integer | zz | 858.5 | 840.7–1027.7 | 0.00 | 0.88 | 0.86 |
| layout | Headless Hub | 183.0 | 170.7–211.3 | 0.00 | 0.20 | 0.20 |
| layout | Stock Qt | 178.4 | 169.9–205.4 | 0.00 | 0.19 | 0.19 |
| layout | zz | 177.4 | 169.1–210.8 | 0.00 | 0.19 | 0.18 |

### Engine CPU by helper

| Work | Arm | WebContent CPU s | Compositor CPU s | Hub CPU s | Other helpers CPU s |
| --- | --- | --- | --- | --- | --- |
| canvas | Headless Hub | 0.15 | 0.19 | 0.03 | 0.00 |
| canvas | Stock Qt | 0.18 | 0.24 | 0.07 | 0.00 |
| canvas | zz | 0.22 | 0.35 | 0.11 | 0.00 |
| scroll | Headless Hub | 0.18 | 1.23 | 0.03 | 0.00 |
| scroll | Stock Qt | 0.18 | 1.27 | 0.06 | 0.00 |
| scroll | zz | 0.19 | 1.33 | 0.09 | 0.00 |

### Page animation callbacks

| Work | Arm | Hz | p95 ms | p99 ms | Count >25 ms | Worst trial count >25 ms |
| --- | --- | --- | --- | --- | --- | --- |
| canvas | Headless Hub | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| canvas | Stock Qt | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| canvas | zz | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| scroll | Headless Hub | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| scroll | Stock Qt | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| scroll | zz | 60.0 | 16.7 | 16.7 | 0.0 | 0 |

### Sensitivity to background swapping

| Subset | Blocks | Max system swap MiB in one arm | Opening pane Δ MiB | Scroll host zz − Qt CPU s | Scroll engine zz − Qt CPU s |
| --- | --- | --- | --- | --- | --- |
| All accepted blocks | 6 | 60.5 | +184.2 (+179.9 to +208.1) | +0.63 (+0.54 to +0.71) | +0.11 (-0.29 to +0.44) |
| Lower-swap half | 3 | 16.6 | +207.7 (+182.1 to +208.1) | +0.67 (+0.54 to +0.67) | +0.11 (+0.06 to +0.11) |
| At most 16 MiB system swap traffic per arm | 2 | 7.0 | +207.9 (+207.7 to +208.1) | +0.67 (+0.67 to +0.67) | +0.08 (+0.06 to +0.11) |

## 1152 × 768 CSS pixels

### Idle memory and CPU

| Arm | Total MiB | Host MiB | Engine MiB | Daemon + PTY MiB | Sampled peak MiB | CPU % of one core |
| --- | --- | --- | --- | --- | --- | --- |
| zz terminal only | 253.8 | 228.3 | 0.0 | 26.6 | 253.8 | 0.50 |
| Headless Hub | 183.6 | 0.0 | 183.6 | 0.0 | 183.7 | 0.00 |
| Stock Qt | 332.4 | 154.0 | 181.6 | 0.0 | 340.6 | 0.37 |
| zz | 445.8 | 237.3 | 184.5 | 26.6 | 454.0 | 0.50 |

| Within-block difference | Total MiB | Host MiB | Engine MiB | Daemon + PTY MiB |
| --- | --- | --- | --- | --- |
| Open pane in zz | +193.7 (+190.3 to +216.3) | +9.6 (+8.8 to +10.9) | +184.5 (+180.5 to +205.4) | +0.0 (-0.3 to +0.0) |
| zz minus stock Qt | +116.4 (+109.1 to +136.4) | +85.1 (+81.9 to +98.0) | +5.7 (+0.9 to +7.4) | +26.6 (+25.3 to +32.5) |

### Opening the first page

| Arm | Start of clock | Median to ready ms | Range ms | Navigation to fixture ready ms |
| --- | --- | --- | --- | --- |
| Headless Hub | Profile setup and launch | 117 | 95–165 | 40 |
| Stock Qt | Profile setup and launch | 335 | 279–916 | 42 |
| zz | Pane request | 131 | 119–201 | 43 |

### CPU and reads per completed workload

| Work | Arm | Total CPU s | Host CPU s | Engine CPU s | Daemon + PTY CPU s | KWin CPU s | Host reads MiB | Sampled peak MiB |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| canvas | Headless Hub | 0.52 | 0.00 | 0.52 | 0.00 | 0.00 | 0.0 | 188.9 |
| canvas | Stock Qt | 1.01 | 0.46 | 0.56 | 0.00 | 0.34 | 0.0 | 338.2 |
| canvas | zz | 1.98 | 1.35 | 0.63 | 0.00 | 0.27 | 810.0 | 457.4 |
| scroll | Headless Hub | 1.85 | 0.00 | 1.85 | 0.00 | 0.00 | 0.0 | 196.8 |
| scroll | Stock Qt | 2.05 | 0.28 | 1.77 | 0.00 | 0.21 | 0.0 | 346.2 |
| scroll | zz | 2.94 | 1.12 | 1.82 | 0.00 | 0.23 | 810.0 | 465.5 |

| zz minus stock Qt | Total CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| canvas | +1.04 (+0.85 to +1.17) | +0.91 (+0.86 to +1.09) | +0.07 (-0.06 to +0.27) |
| scroll | +0.96 (+0.70 to +1.24) | +0.87 (+0.77 to +0.94) | +0.09 (-0.07 to +0.30) |

### JavaScript and synchronous layout

| Work | Arm | Page time ms | Page time range ms | Host CPU s | Engine CPU s | WebContent CPU s |
| --- | --- | --- | --- | --- | --- | --- |
| integer | Headless Hub | 858.7 | 836.1–1066.9 | 0.00 | 0.86 | 0.86 |
| integer | Stock Qt | 871.0 | 843.6–996.4 | 0.00 | 0.88 | 0.87 |
| integer | zz | 897.9 | 838.1–1027.3 | 0.00 | 0.91 | 0.90 |
| layout | Headless Hub | 178.7 | 170.7–225.5 | 0.00 | 0.18 | 0.18 |
| layout | Stock Qt | 179.6 | 167.6–236.1 | 0.00 | 0.20 | 0.20 |
| layout | zz | 181.7 | 168.1–451.5 | 0.00 | 0.20 | 0.20 |

### Engine CPU by helper

| Work | Arm | WebContent CPU s | Compositor CPU s | Hub CPU s | Other helpers CPU s |
| --- | --- | --- | --- | --- | --- |
| canvas | Headless Hub | 0.18 | 0.30 | 0.03 | 0.00 |
| canvas | Stock Qt | 0.17 | 0.32 | 0.07 | 0.00 |
| canvas | zz | 0.17 | 0.35 | 0.09 | 0.00 |
| scroll | Headless Hub | 0.17 | 1.66 | 0.03 | 0.00 |
| scroll | Stock Qt | 0.17 | 1.54 | 0.04 | 0.00 |
| scroll | zz | 0.17 | 1.57 | 0.09 | 0.00 |

### Page animation callbacks

| Work | Arm | Hz | p95 ms | p99 ms | Count >25 ms | Worst trial count >25 ms |
| --- | --- | --- | --- | --- | --- | --- |
| canvas | Headless Hub | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| canvas | Stock Qt | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| canvas | zz | 60.0 | 16.7 | 16.7 | 0.0 | 1 |
| scroll | Headless Hub | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| scroll | Stock Qt | 60.0 | 16.7 | 16.7 | 0.0 | 0 |
| scroll | zz | 60.0 | 16.7 | 16.7 | 0.0 | 0 |

### Sensitivity to background swapping

| Subset | Blocks | Max system swap MiB in one arm | Opening pane Δ MiB | Scroll host zz − Qt CPU s | Scroll engine zz − Qt CPU s |
| --- | --- | --- | --- | --- | --- |
| All accepted blocks | 6 | 114.0 | +193.7 (+190.3 to +216.3) | +0.87 (+0.77 to +0.94) | +0.09 (-0.07 to +0.30) |
| Lower-swap half | 3 | 64.3 | +214.9 (+193.7 to +216.3) | +0.79 (+0.77 to +0.88) | -0.05 (-0.07 to +0.06) |
| At most 16 MiB system swap traffic per arm | 2 | 7.3 | +204.3 (+193.7 to +214.9) | +0.83 (+0.79 to +0.88) | +0.00 (-0.05 to +0.06) |
