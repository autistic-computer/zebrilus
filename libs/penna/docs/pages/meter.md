# Meter

A bounded scalar gauge for CPU, memory, disk, or context-window usage. For ongoing task completion, use [Progress](progress.html).

## Threshold tones

By default `Meter` transitions from `Success` to `Warning` at 75% and `Danger` at 90% so capacity alerts read at a glance without manual tone logic.

```figure uses=meter
<div style="display: flex; flex-direction: column; gap: 16px; width: 100%; max-width: 420px">
  <div class="penna-meter" role="meter" aria-label="CPU usage" aria-valuemin="0" aria-valuemax="100" aria-valuenow="42">
    <div class="penna-meter-head"><span class="penna-meter-label">CPU</span><span class="penna-meter-value">42%</span></div>
    <div class="penna-meter-track"><div class="penna-meter-fill" style="--penna-value: 42%"></div></div>
  </div>
  <div class="penna-meter" data-tone="warning" role="meter" aria-label="Context window" aria-valuemin="0" aria-valuemax="200" aria-valuenow="158">
    <div class="penna-meter-head"><span class="penna-meter-label">Context window</span><span class="penna-meter-value">158k / 200k</span></div>
    <div class="penna-meter-track"><div class="penna-meter-fill" style="--penna-value: 79%"></div></div>
  </div>
  <div class="penna-meter" data-tone="danger" role="meter" aria-label="Disk quota" aria-valuemin="0" aria-valuemax="100" aria-valuenow="94">
    <div class="penna-meter-head"><span class="penna-meter-label">Disk quota</span><span class="penna-meter-value">94%</span></div>
    <div class="penna-meter-track"><div class="penna-meter-fill" style="--penna-value: 94%"></div></div>
  </div>
</div>
```

```rust
Meter::new("context-meter", 158.0)
    .range(0.0, 200.0)
    .label("Context window")
    .value_label("158k / 200k")
```

## API (`penna::Meter`)

| Rust | HTML (`uses=meter`) | Role |
| --- | --- | --- |
| `Meter::new(id, value)` | `.penna-meter` (`role="meter"`) | Bounded scalar gauge |
| `.range(min, max)` | `aria-valuemin` / `aria-valuemax` | Value bounds (default `0.0..1.0`) |
| `.thresholds(warn, danger)` | `data-tone` | Ratio thresholds (default `0.75`, `0.90`) |
| `.label(text)` / `.value_label(text)` | `.penna-meter-head` | Leading title and tabular value readout |
| `.tone(Tone)` | `data-tone` (`accent` \| `warning` \| `danger` \| `neutral`) | Explicit fill tone override |
