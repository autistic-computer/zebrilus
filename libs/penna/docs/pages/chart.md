# Chart

Plots up to four numeric series against shared labels, as bars or lines. For one number with a name use a card; for exact rows use a table.

## Kinds

```figure uses=chart
<div class="docs-show" data-gap="wide" style="width: 100%">
  <div class="docs-cell" style="flex: 1; min-width: 260px">
    <div class="penna-chart">
      <div class="penna-chart-legend">
        <span class="penna-chart-key"><i class="penna-chart-swatch" style="--series: var(--penna-series-0)"></i>Requests</span>
        <span class="penna-chart-key"><i class="penna-chart-swatch" style="--series: var(--penna-series-1)"></i>Cached</span>
      </div>
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
            <div class="penna-chart-band"><span class="penna-chart-bar" style="height: 58%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 36%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band"><span class="penna-chart-bar" style="height: 76%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 48%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band"><span class="penna-chart-bar" style="height: 64%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 41%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band"><span class="penna-chart-bar" style="height: 92%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 60%; --series: var(--penna-series-1)"></span></div>
            <div class="penna-chart-band"><span class="penna-chart-bar" style="height: 70%; --series: var(--penna-series-0)"></span><span class="penna-chart-bar" style="height: 44%; --series: var(--penna-series-1)"></span></div>
          </div>
        </div>
      </div>
      <div class="penna-chart-axis"><span>Mon</span><span>Tue</span><span>Wed</span><span>Thu</span><span>Fri</span></div>
    </div>
    <span class="docs-note">Bar</span>
  </div>
  <div class="docs-cell" style="flex: 1; min-width: 260px">
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
          <svg class="penna-chart-svg" viewBox="0 0 240 160" preserveAspectRatio="none">
            <polyline fill="none" stroke="var(--penna-series-0)" stroke-width="2" points="20,96 70,58 120,74 170,28 220,44" />
            <circle cx="20" cy="96" r="4" fill="var(--penna-series-0)" />
            <circle cx="70" cy="58" r="4" fill="var(--penna-series-0)" />
            <circle cx="120" cy="74" r="4" fill="var(--penna-series-0)" />
            <circle cx="170" cy="28" r="4" fill="var(--penna-series-0)" />
            <circle cx="220" cy="44" r="4" fill="var(--penna-series-0)" />
          </svg>
        </div>
      </div>
      <div class="penna-chart-axis"><span>10:00</span><span>10:15</span><span>10:30</span><span>10:45</span><span>11:00</span></div>
    </div>
    <span class="docs-note">Line</span>
  </div>
</div>
```

- **Bar**: grouped bars, up to 24 wide, 2 apart, 4 radius on the far end. Negative values drop below the zero line.
- **Line**: a 2 stroke joining the points with an 8 dot on each point. Non-finite values leave a gap.

```rust
Chart::new(
    "traffic",
    ChartKind::Bar,
    ["Mon", "Tue", "Wed", "Thu", "Fri"].map(Into::into),
    [
        Series::new("Requests", [11_600.0, 15_200.0, 12_800.0, 18_400.0, 14_000.0]),
        Series::new("Cached", [7_200.0, 9_600.0, 8_200.0, 12_000.0, 8_800.0]),
    ],
)
```

## Anatomy

1. **Legend**: shown only when there are two or more series, an 8 dot in each series' color beside its name.
2. **Gutter**: 40 wide, ticks formatted with `k`, `M` and `B` suffixes in tabular figures, aligned to horizontal hairlines across the plot.
3. **Plot**: 160 tall. Hovering a band washes it and opens a tooltip with the label and every series value for that index.
4. **Axis**: up to 12 labels on desktop, or 6 under `.touch(true)`, evenly thinned when more are given.

Series colors are fixed by position (`blue`, `pink`, `yellow`, `purple`) so every chart uses the same accessible palette in both modes. Series past `MAX_SERIES` (4) are ignored.

## API

| Rust | Does |
| --- | --- |
| `Chart::new(id, kind, labels, series)` | Bar or line chart, up to 4 series |
| `ChartKind::Bar`, `ChartKind::Line` | Plot style |
| `Series::new(name, values)` | One named series of `f64` |
| `.touch(true)` | Uses `body` labels and caps the axis at 6 ticks |
| `MAX_SERIES` | 4 |
