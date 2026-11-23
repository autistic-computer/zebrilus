# Resource use with four Ladybird configurations

2026-11-23T19:22:16Z · 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz · NVIDIA GeForce RTX 3070 Laptop GPU, 620.73.04, 8192 MiB · kwin 6.8.3.

4 measured blocks and one discarded warm-up per viewport. 0 completed blocks rejected. Table entries are medians. Paired differences include the range across blocks.

## 786 × 608 CSS pixels

| Arm | Initial canvas at 1.5 s | Canvas after animation | Scrolled page |
| --- | --- | --- | --- |
| zz CPU | Visible | Verified | Verified |
| zz GPU + bitmap | Marker missing | Verified | Verified |
| zz DMA-BUF | Marker missing | Verified | Verified |
| Stock Qt GPU | Visible | Verified | Verified |

### Idle

| Arm | App PSS MiB | Pane added PSS MiB | Host PSS MiB | Engine PSS MiB | Driver GPU MiB | CPU % of a core |
| --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 401.6 | 198.7 | 185.7 | 190.5 | 58.0 | 0.50 |
| zz GPU + bitmap | 466.6 | 263.4 | 161.3 | 280.2 | 81.0 | 0.00 |
| zz DMA-BUF | 458.5 | 255.3 | 160.4 | 272.9 | 81.0 | 0.12 |
| Stock Qt GPU | 394.1 | — | 108.2 | 285.8 | 31.0 | 0.12 |

| Paired difference | App PSS MiB | Added pane PSS MiB | Driver GPU MiB |
| --- | --- | --- | --- |
| DMA-BUF minus zz CPU | +56.7 (+55.1 to +60.3) | +57.4 (+53.7 to +58.7) | +23.0 (+23.0 to +23.0) |
| DMA-BUF minus zz GPU + bitmap | -7.7 (-10.0 to -7.2) | -8.4 (-10.2 to -6.5) | +0.0 (+0.0 to +0.0) |
| DMA-BUF minus Stock Qt GPU | +64.7 (+63.0 to +65.5) | — | +50.0 (+50.0 to +50.0) |

### Canvas

| Arm | App CPU s | Host CPU s | Engine CPU s | KWin CPU s | Host reads MiB | Callbacks/s | Driver GPU MiB |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 2.00 | 1.25 | 0.75 | 0.41 | 437.52 | 60.00 | — |
| zz GPU + bitmap | 2.19 | 1.19 | 0.99 | 0.37 | 433.88 | 60.00 | — |
| zz DMA-BUF | 1.42 | 0.76 | 0.66 | 0.38 | 0.00 | 59.50 | — |
| Stock Qt GPU | 0.85 | 0.13 | 0.72 | 0.30 | 0.00 | 59.50 | — |

| Paired difference | App CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| DMA-BUF minus zz CPU | -0.60 (-0.73 to -0.50) | -0.51 (-0.58 to -0.42) | -0.09 (-0.16 to -0.08) |
| DMA-BUF minus zz GPU + bitmap | -0.79 (-0.83 to -0.73) | -0.43 (-0.46 to -0.41) | -0.33 (-0.41 to -0.32) |
| DMA-BUF minus Stock Qt GPU | +0.56 (+0.44 to +0.60) | +0.62 (+0.54 to +0.67) | -0.06 (-0.11 to -0.05) |

### Scroll

| Arm | App CPU s | Host CPU s | Engine CPU s | KWin CPU s | Host reads MiB | Callbacks/s | Driver GPU MiB |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 2.99 | 0.92 | 2.07 | 0.30 | 437.52 | 60.00 | 58.0 |
| zz GPU + bitmap | 2.47 | 1.18 | 1.29 | 0.37 | 437.52 | 60.00 | 86.0 |
| zz DMA-BUF | 1.55 | 0.71 | 0.83 | 0.36 | 0.00 | 60.00 | 86.0 |
| Stock Qt GPU | 1.08 | 0.14 | 0.95 | 0.30 | 0.00 | 60.00 | 36.0 |

| Paired difference | App CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| DMA-BUF minus zz CPU | -1.49 (-2.37 to -1.26) | -0.22 (-0.68 to -0.14) | -1.29 (-1.69 to -1.08) |
| DMA-BUF minus zz GPU + bitmap | -0.97 (-1.92 to -0.47) | -0.44 (-0.95 to -0.26) | -0.54 (-0.97 to -0.21) |
| DMA-BUF minus Stock Qt GPU | +0.46 (-0.50 to +0.60) | +0.58 (+0.11 to +0.63) | -0.12 (-0.61 to -0.04) |

## 1152 × 768 CSS pixels

| Arm | Initial canvas at 1.5 s | Canvas after animation | Scrolled page |
| --- | --- | --- | --- |
| zz CPU | Visible | Verified | Verified |
| zz GPU + bitmap | Marker missing | Verified | Verified |
| zz DMA-BUF | Marker missing | Verified | Verified |
| Stock Qt GPU | Marker missing | Verified | Verified |

### Idle

| Arm | App PSS MiB | Pane added PSS MiB | Host PSS MiB | Engine PSS MiB | Driver GPU MiB | CPU % of a core |
| --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 408.0 | 205.1 | 189.0 | 193.7 | 80.0 | 0.75 |
| zz GPU + bitmap | 468.9 | 265.9 | 162.2 | 281.1 | 93.0 | 0.00 |
| zz DMA-BUF | 459.0 | 254.7 | 161.4 | 272.1 | 93.0 | 0.25 |
| Stock Qt GPU | 400.0 | — | 113.4 | 285.9 | 37.0 | 0.00 |

| Paired difference | App PSS MiB | Added pane PSS MiB | Driver GPU MiB |
| --- | --- | --- | --- |
| DMA-BUF minus zz CPU | +50.9 (+49.9 to +51.9) | +49.5 (+47.7 to +51.9) | +13.0 (+13.0 to +13.0) |
| DMA-BUF minus zz GPU + bitmap | -9.9 (-10.9 to -9.7) | -11.1 (-12.6 to -9.0) | +0.0 (+0.0 to +0.0) |
| DMA-BUF minus Stock Qt GPU | +58.9 (+57.7 to +61.5) | — | +56.0 (+53.0 to +56.0) |

### Canvas

| Arm | App CPU s | Host CPU s | Engine CPU s | KWin CPU s | Host reads MiB | Callbacks/s | Driver GPU MiB |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 2.49 | 1.69 | 0.80 | 0.38 | 810.00 | 60.00 | — |
| zz GPU + bitmap | 2.50 | 1.47 | 1.04 | 0.38 | 803.25 | 60.00 | — |
| zz DMA-BUF | 1.43 | 0.77 | 0.66 | 0.39 | 0.00 | 59.75 | — |
| Stock Qt GPU | 0.85 | 0.14 | 0.72 | 0.29 | 0.00 | 59.50 | — |

| Paired difference | App CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| DMA-BUF minus zz CPU | -1.07 (-1.13 to -1.03) | -0.92 (-0.95 to -0.90) | -0.15 (-0.18 to -0.13) |
| DMA-BUF minus zz GPU + bitmap | -1.09 (-1.27 to -0.88) | -0.71 (-0.78 to -0.56) | -0.39 (-0.49 to -0.32) |
| DMA-BUF minus Stock Qt GPU | +0.57 (+0.50 to +0.60) | +0.63 (+0.60 to +0.65) | -0.06 (-0.10 to -0.04) |

### Scroll

| Arm | App CPU s | Host CPU s | Engine CPU s | KWin CPU s | Host reads MiB | Callbacks/s | Driver GPU MiB |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 3.22 | 1.10 | 2.12 | 0.36 | 810.00 | 60.00 | 80.0 |
| zz GPU + bitmap | 3.04 | 1.52 | 1.52 | 0.41 | 810.00 | 60.00 | 117.0 |
| zz DMA-BUF | 1.72 | 0.77 | 0.95 | 0.40 | 0.00 | 60.00 | 101.0 |
| Stock Qt GPU | 1.12 | 0.13 | 1.00 | 0.29 | 0.00 | 60.00 | 45.0 |

| Paired difference | App CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| DMA-BUF minus zz CPU | -1.50 (-1.53 to -1.44) | -0.33 (-0.34 to -0.31) | -1.17 (-1.19 to -1.13) |
| DMA-BUF minus zz GPU + bitmap | -1.34 (-1.40 to -1.00) | -0.76 (-0.86 to -0.62) | -0.53 (-0.61 to -0.39) |
| DMA-BUF minus Stock Qt GPU | +0.61 (+0.57 to +0.69) | +0.64 (+0.63 to +0.66) | -0.03 (-0.07 to +0.03) |

---

# Composited scrolling with four Ladybird configurations

2026-11-23T19:37:47Z · 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz · NVIDIA GeForce RTX 3070 Laptop GPU, 620.73.04, 8192 MiB · kwin 6.8.3.

4 measured blocks and one discarded warm-up per viewport. 0 completed blocks rejected. Table entries are medians. Paired differences include the range across blocks.

## 786 × 608 CSS pixels

| Arm | Page callbacks/s | Distinct composited frames/s | Callbacks seen (%) | P95 gap (ms) | Longest gap (ms) | Interior grey samples |
| --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 60.00 | 56.92 | 94.86 | 25.00 | 33.33 | 0.00 |
| zz GPU + bitmap | 60.00 | 59.83 | 99.72 | 16.67 | 25.00 | 0.00 |
| zz DMA-BUF | 60.00 | 60.00 | 100.00 | 16.67 | 16.67 | 0.00 |
| Stock Qt GPU | 60.00 | 60.00 | 100.00 | 16.67 | 16.67 | 0.00 |

| Paired difference | Distinct composited frames/s | P95 gap (ms) |
| --- | --- | --- |
| DMA-BUF minus zz CPU | +2.58 (+0.00 to +4.50) | -8.33 (-16.67 to +0.00) |
| DMA-BUF minus zz GPU + bitmap | +0.17 (-1.00 to +0.50) | +0.00 (+0.00 to +0.00) |
| DMA-BUF minus Stock Qt GPU | +0.00 (-1.00 to +0.33) | +0.00 (+0.00 to +0.00) |

## 1152 × 768 CSS pixels

| Arm | Page callbacks/s | Distinct composited frames/s | Callbacks seen (%) | P95 gap (ms) | Longest gap (ms) | Interior grey samples |
| --- | --- | --- | --- | --- | --- | --- |
| zz CPU | 60.00 | 59.33 | 98.89 | 16.67 | 25.00 | 0.00 |
| zz GPU + bitmap | 60.00 | 59.58 | 99.31 | 16.67 | 25.00 | 0.00 |
| zz DMA-BUF | 60.00 | 60.00 | 100.00 | 16.67 | 16.67 | 0.00 |
| Stock Qt GPU | 60.00 | 60.00 | 100.00 | 16.67 | 16.67 | 0.00 |

| Paired difference | Distinct composited frames/s | P95 gap (ms) |
| --- | --- | --- |
| DMA-BUF minus zz CPU | +0.58 (-4.83 to +2.33) | +0.00 (+0.00 to +16.67) |
| DMA-BUF minus zz GPU + bitmap | +0.42 (-5.00 to +4.00) | +0.00 (-16.67 to +16.67) |
| DMA-BUF minus Stock Qt GPU | +0.00 (-5.00 to +0.33) | +0.00 (+0.00 to +16.67) |

---

# Resource use in the same zz desktop with CEF and Ladybird

2026-11-23T19:45:32Z · 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz · NVIDIA GeForce RTX 3070 Laptop GPU, 620.73.04, 8192 MiB · kwin 6.8.3.

4 measured blocks and one discarded warm-up per viewport. 0 completed blocks rejected. Table entries are medians. Paired differences include the range across blocks.

## 1152 × 768 CSS pixels

| Arm | Initial canvas at 1.5 s | Canvas after animation | Scrolled page |
| --- | --- | --- | --- |
| zz DMA-BUF | Marker missing | Verified | Verified |
| zz CEF | Visible | Verified | Verified |

### Idle

| Arm | App PSS MiB | Pane added PSS MiB | Host PSS MiB | Engine PSS MiB | Driver GPU MiB | CPU % of a core |
| --- | --- | --- | --- | --- | --- | --- |
| zz DMA-BUF | 457.5 | 254.9 | 159.9 | 271.2 | 93.0 | 0.00 |
| zz CEF | 639.8 | 437.6 | 289.7 | 324.2 | 104.0 | 3.12 |

| Paired difference | App PSS MiB | Added pane PSS MiB | Driver GPU MiB |
| --- | --- | --- | --- |
| DMA-BUF minus zz CEF | -182.9 (-184.0 to -180.2) | -182.4 (-185.2 to -182.3) | -11.0 (-12.0 to -11.0) |

### Canvas

| Arm | App CPU s | Host CPU s | Engine CPU s | KWin CPU s | Host reads MiB | Callbacks/s | Driver GPU MiB |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zz DMA-BUF | 1.46 | 0.77 | 0.68 | 0.39 | 0.00 | 59.50 | — |
| zz CEF | 2.32 | 1.14 | 1.17 | 0.35 | 0.02 | 60.00 | — |

| Paired difference | App CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| DMA-BUF minus zz CEF | -0.86 (-0.92 to +0.06) | -0.37 (-0.37 to +0.06) | -0.49 (-0.55 to +0.00) |

### Scroll

| Arm | App CPU s | Host CPU s | Engine CPU s | KWin CPU s | Host reads MiB | Callbacks/s | Driver GPU MiB |
| --- | --- | --- | --- | --- | --- | --- | --- |
| zz DMA-BUF | 1.67 | 0.74 | 0.92 | 0.39 | 0.00 | 60.00 | 101.0 |
| zz CEF | 2.09 | 1.14 | 0.95 | 0.36 | 0.02 | 60.00 | 119.0 |

| Paired difference | App CPU s | Host CPU s | Engine CPU s |
| --- | --- | --- | --- |
| DMA-BUF minus zz CEF | -0.38 (-0.53 to -0.35) | -0.38 (-0.46 to -0.35) | -0.02 (-0.07 to +0.03) |

### Page readiness and engine work

| Arm | Open to page ready (ms) | 96M integer iterations (ms) | 48 forced layouts (ms) |
| --- | --- | --- | --- |
| zz DMA-BUF | 109.1 | 852.1 | 174.5 |
| zz CEF | 352.5 | 85.5 | 74.1 |

| Paired difference | Ready ms | Integer ms | Layout ms |
| --- | --- | --- | --- |
| DMA-BUF minus zz CEF | -242.2 (-245.3 to -207.2) | +766.6 (+758.2 to +778.5) | +100.5 (+96.4 to +101.5) |

---

# Composited scrolling in the same zz desktop with CEF and Ladybird

2026-11-23T19:49:21Z · 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz · NVIDIA GeForce RTX 3070 Laptop GPU, 620.73.04, 8192 MiB · kwin 6.8.3.

4 measured blocks and one discarded warm-up per viewport. 0 completed blocks rejected. Table entries are medians. Paired differences include the range across blocks.

## 1152 × 768 CSS pixels

| Arm | Page callbacks/s | Distinct composited frames/s | Callbacks seen (%) | P95 gap (ms) | Longest gap (ms) | Interior grey samples |
| --- | --- | --- | --- | --- | --- | --- |
| zz DMA-BUF | 60.00 | 59.83 | 99.72 | 16.67 | 16.67 | 0.00 |
| zz CEF | 60.00 | 59.42 | 99.03 | 16.67 | 33.33 | 0.00 |

| Paired difference | Distinct composited frames/s | P95 gap (ms) |
| --- | --- | --- |
| DMA-BUF minus zz CEF | +0.41 (-4.00 to +0.99) | +0.00 (+0.00 to +16.67) |
