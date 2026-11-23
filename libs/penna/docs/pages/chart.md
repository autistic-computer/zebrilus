# Chart

Plots up to four numeric series against shared labels, as bars or lines. For one number with a name use a [UI block](ui-block.html); for how far one task has come, a [progress bar](progress.html); for exact rows, a table.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail">Not built yet: a keyboard path to step through bands and read values.</span>
    </div>
  </div>
</div>
```

## Kinds

```figure uses=chart
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Bar</span>
      <span class="docs-lk-mono">Two series · legend below</span>
    </div>
    <div class="penna-chart">
      <div class="penna-chart-body">
        <div class="penna-chart-gutter">
          <span class="penna-chart-tick" style="top: 0%">20k</span>
          <span class="penna-chart-tick" style="top: 50%">10k</span>
          <span class="penna-chart-tick" style="top: 100%">0</span>
        </div>
        <div class="penna-chart-plot">
          <div class="penna-chart-rule" style="top: 0%"></div>
          <div class="penna-chart-rule" style="top: 50%"></div>
          <div class="penna-chart-rule" style="top: 100%"></div>
          <div class="penna-chart-bars">
            <div class="penna-chart-band" data-tooltip="Mon · Requests 11.6k · Cached 7.2k"><span class="penna-chart-bar" style="height: 58%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 36%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band" data-tooltip="Tue · Requests 15.2k · Cached 9.6k"><span class="penna-chart-bar" style="height: 76%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 48%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band" data-tooltip="Wed · Requests 12.8k · Cached 8.2k"><span class="penna-chart-bar" style="height: 64%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 41%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band" data-tooltip="Thu · Requests 18.4k · Cached 12k"><span class="penna-chart-bar" style="height: 92%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 60%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band" data-tooltip="Fri · Requests 14k · Cached 8.8k"><span class="penna-chart-bar" style="height: 70%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 44%; --series: var(--penna-series-1)"></span></div>
          </div>
        </div>
      </div>
      <div class="penna-chart-axis"><span>Mon</span><span>Tue</span><span>Wed</span><span>Thu</span><span>Fri</span></div>
      <div class="penna-chart-legend">
        <span class="penna-chart-key"><i class="penna-chart-swatch" style="--series: var(--penna-series-0)"></i>Requests</span>
        <span class="penna-chart-key"><i class="penna-chart-swatch" style="--series: var(--penna-series-1)"></i>Cached</span>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Line</span>
      <span class="docs-lk-mono">One series · no legend</span>
    </div>
    <div class="penna-chart">
      <div class="penna-chart-body">
        <div class="penna-chart-gutter">
          <span class="penna-chart-tick" style="top: 0%">80</span>
          <span class="penna-chart-tick" style="top: 50%">40</span>
          <span class="penna-chart-tick" style="top: 100%">0</span>
        </div>
        <div class="penna-chart-plot">
          <div class="penna-chart-rule" style="top: 0%"></div>
          <div class="penna-chart-rule" style="top: 50%"></div>
          <div class="penna-chart-rule" style="top: 100%"></div>
          <div class="penna-chart-bars">
            <div class="penna-chart-band" data-tooltip="10:00: 32"></div>
            <div class="penna-chart-band" data-tooltip="10:15: 51"></div>
            <div class="penna-chart-band" data-tooltip="10:30: 43"></div>
            <div class="penna-chart-band" data-tooltip="10:45: 66"></div>
            <div class="penna-chart-band" data-tooltip="11:00: 58"></div>
          </div>
          <svg class="penna-chart-svg" data-min="0" data-max="80" data-count="5" data-series="[[32, 51, 43, 66, 58]]"></svg>
        </div>
      </div>
      <div class="penna-chart-axis"><span>10:00</span><span>10:15</span><span>10:30</span><span>10:45</span><span>11:00</span></div>
    </div>
  </div>
</div>
```

- **Bar**: grouped bars, up to 24 wide, 2 apart, 4 radius on the far end and square on the zero rule. Negative values hang below zero.
- **Line**: a 2 stroke joining the middle of each band. A missing value breaks the line, and a value with no neighbor draws an 8 dot.

```rust
let requests = Series::new(
    "Requests",
    [11_600.0, 15_200.0, 12_800.0, 18_400.0, 14_000.0],
);
let cached = Series::new(
    "Cached",
    [7_200.0, 9_600.0, 8_200.0, 12_000.0, 8_800.0],
);

Chart::new(
    "traffic",
    ChartKind::Bar,
    ["Mon", "Tue", "Wed", "Thu", "Fri"].map(Into::into),
    [requests, cached],
)
```

## Edge cases

```figure uses=chart
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Under zero</span>
      <span class="docs-lk-mono">Hangs from zero rule</span>
    </div>
    <div class="penna-chart">
      <div class="penna-chart-body">
        <div class="penna-chart-gutter">
          <span class="penna-chart-tick" style="top: 0%">10</span>
          <span class="penna-chart-tick" style="top: 33.33%">5</span>
          <span class="penna-chart-tick" style="top: 66.67%">0</span>
          <span class="penna-chart-tick" style="top: 100%">-5</span>
        </div>
        <div class="penna-chart-plot">
          <div class="penna-chart-rule" style="top: 0%"></div>
          <div class="penna-chart-rule" style="top: 33.33%"></div>
          <div class="penna-chart-rule" style="top: 66.67%"></div>
          <div class="penna-chart-rule" style="top: 100%"></div>
          <div class="penna-chart-bars">
            <div class="penna-chart-band" data-tooltip="Mon: 4"><div class="penna-chart-slot"><span class="penna-chart-bar" style="top: 40%; height: 26.67%"></span></div></div>
            <div class="penna-chart-band" data-tooltip="Tue: -2"><div class="penna-chart-slot"><span class="penna-chart-bar" data-below style="top: 66.67%; height: 13.33%"></span></div></div>
            <div class="penna-chart-band" data-tooltip="Wed: 7"><div class="penna-chart-slot"><span class="penna-chart-bar" style="top: 20%; height: 46.67%"></span></div></div>
            <div class="penna-chart-band" data-tooltip="Thu: -5"><div class="penna-chart-slot"><span class="penna-chart-bar" data-below style="top: 66.67%; height: 33.33%"></span></div></div>
            <div class="penna-chart-band" data-tooltip="Fri: 3"><div class="penna-chart-slot"><span class="penna-chart-bar" style="top: 46.67%; height: 20%"></span></div></div>
          </div>
        </div>
      </div>
      <div class="penna-chart-axis"><span>Mon</span><span>Tue</span><span>Wed</span><span>Thu</span><span>Fri</span></div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Missing values</span>
      <span class="docs-lk-mono">Gap + isolated 8 px dot</span>
    </div>
    <div class="penna-chart">
      <div class="penna-chart-body">
        <div class="penna-chart-gutter">
          <span class="penna-chart-tick" style="top: 0%">60</span>
          <span class="penna-chart-tick" style="top: 50%">30</span>
          <span class="penna-chart-tick" style="top: 100%">0</span>
        </div>
        <div class="penna-chart-plot">
          <div class="penna-chart-rule" style="top: 0%"></div>
          <div class="penna-chart-rule" style="top: 50%"></div>
          <div class="penna-chart-rule" style="top: 100%"></div>
          <div class="penna-chart-bars">
            <div class="penna-chart-band" data-tooltip="Mon: 41"></div>
            <div class="penna-chart-band" data-tooltip="Tue: 38"></div>
            <div class="penna-chart-band" data-tooltip="Wed: none"></div>
            <div class="penna-chart-band" data-tooltip="Thu: 52"></div>
            <div class="penna-chart-band" data-tooltip="Fri: none"></div>
            <div class="penna-chart-band" data-tooltip="Sat: 40"></div>
            <div class="penna-chart-band" data-tooltip="Sun: 39"></div>
          </div>
          <svg class="penna-chart-svg" data-min="0" data-max="60" data-count="7" data-series="[[41, 38, null, 52, null, 40, 39]]"></svg>
        </div>
      </div>
      <div class="penna-chart-axis"><span>Mon</span><span>Tue</span><span>Wed</span><span>Thu</span><span>Fri</span><span>Sat</span><span>Sun</span></div>
    </div>
  </div>
</div>
```

With no data, the rules and labels stay and the axis runs from 0 to 1.

## Anatomy

1. **Gutter**: 40 wide, ticks formatted with `k`, `M` and `B` suffixes in tabular figures, right-aligned `space-8` from the plot on horizontal hairlines through zero.
2. **Plot**: 160 tall. Hovering a band washes it with a 4 radius and opens a tooltip with the label and every series value for that band.
3. **Axis**: `gap-tight` 4 below the plot, up to 12 labels on desktop or 6 under `.touch(true)`, evenly thinned when more are given.
4. **Legend**: shown only with two or more series, `space-8` under the labels, starting where the plot starts: an 8 swatch rounded by 2, `gap-tight` 4, then the series name in `small` in the ink.

Series colors are fixed by position (`blue`, `pink`, `yellow`, `purple`) regardless of the theme tint, so neighbors stay distinct in light and dark. Series past `MAX_SERIES` 4 are ignored.

## Phone

```figure uses=chart
<div style="width: min(100%, 358px)">
  <div class="penna-chart" data-touch>
    <div class="penna-chart-body">
      <div class="penna-chart-gutter">
        <span class="penna-chart-tick" style="top: 0%">250</span>
        <span class="penna-chart-tick" style="top: 50%">125</span>
        <span class="penna-chart-tick" style="top: 100%">0</span>
      </div>
      <div class="penna-chart-plot">
        <div class="penna-chart-rule" style="top: 0%"></div>
        <div class="penna-chart-rule" style="top: 50%"></div>
        <div class="penna-chart-rule" style="top: 100%"></div>
        <svg class="penna-chart-svg" data-min="0" data-max="250" data-count="5" data-series="[[41, 38, 44, 52, 47], [120, 96, 140, 210, 180]]"></svg>
      </div>
    </div>
    <div class="penna-chart-axis"><span>Mon</span><span>Tue</span><span>Wed</span><span>Thu</span><span>Fri</span></div>
    <div class="penna-chart-legend">
      <span class="penna-chart-key"><i class="penna-chart-swatch" style="--series: var(--penna-series-0)"></i>p50 ms</span>
      <span class="penna-chart-key"><i class="penna-chart-swatch" style="--series: var(--penna-series-1)"></i>p99 ms</span>
    </div>
  </div>
</div>
```

Under `.touch(true)`, ticks and axis labels step up to `body`, the legend to `touch-small`, and at most 6 labels are written.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Chart::new(id, kind, labels, series)` | `.penna-chart` | Bar or line chart, up to 4 series |
| `ChartKind::Bar`, `ChartKind::Line` | `.penna-chart-bars`, `svg.penna-chart-svg` | Plot style |
| `Series::new(name, values)` | `--penna-series-0..3`, `data-series` | One named series of `f64` |
| `.touch(true)` | `data-touch` | `body` ticks and labels, `touch-small` legend, 6-label cap |
| `MAX_SERIES` | 4 | Maximum series drawn |
