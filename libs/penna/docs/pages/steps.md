# Steps

A compact horizontal step strip for short setup or onboarding flows. Completed steps turn into checkmarks, the current step highlights on `SurfaceRole::STEP`, and upcoming steps stay muted.

## States

Pass the zero-based `current` index to `Steps::new(labels).current(index)`. Passing `labels.len()` marks every step complete.

```figure uses=steps
<div class="docs-show" style="flex-direction: column; align-items: flex-start; gap: 16px; width: 100%">
  <div class="penna-steps" role="list" aria-label="Pairing setup" style="flex-wrap: wrap">
    <div class="penna-step" role="listitem" data-place="done">
      <span class="penna-step-dot" data-penna="step" data-tone="primary"><i data-icon="check"></i></span>
      <span class="penna-step-label" data-penna-text>Allow</span>
    </div>
    <div class="penna-step" role="listitem" data-place="current" aria-current="step">
      <span class="penna-step-line" data-penna="step" data-tone="primary"></span>
      <span class="penna-step-dot" data-penna="step" data-tone="primary" data-state="selected"><span data-penna-text>2</span></span>
      <span class="penna-step-label" data-penna-text>Find</span>
    </div>
    <div class="penna-step" role="listitem" data-place="later">
      <span class="penna-step-line" data-penna="step"></span>
      <span class="penna-step-dot" data-penna="step"><span data-penna-text>3</span></span>
      <span class="penna-step-label" data-penna-text>Place</span>
    </div>
  </div>
  <div class="penna-steps" role="list" aria-label="Completed setup" style="flex-wrap: wrap">
    <div class="penna-step" role="listitem" data-place="done">
      <span class="penna-step-dot" data-penna="step" data-tone="primary"><i data-icon="check"></i></span>
      <span class="penna-step-label" data-penna-text>Allow</span>
    </div>
    <div class="penna-step" role="listitem" data-place="done">
      <span class="penna-step-line" data-penna="step" data-tone="primary"></span>
      <span class="penna-step-dot" data-penna="step" data-tone="primary"><i data-icon="check"></i></span>
      <span class="penna-step-label" data-penna-text>Find</span>
    </div>
    <div class="penna-step" role="listitem" data-place="done">
      <span class="penna-step-line" data-penna="step" data-tone="primary"></span>
      <span class="penna-step-dot" data-penna="step" data-tone="primary"><i data-icon="check"></i></span>
      <span class="penna-step-label" data-penna-text>Place</span>
    </div>
  </div>
</div>
```

```rust
Steps::new(["Allow", "Find", "Place"])
    .id("setup-steps")
    .current(1)
```

- **Short verb labels:** keep each step to one word so the strip fits comfortably above a dialog or setup sheet without wrapping.
- **Continuous work:** use [Progress](progress.html) for downloads or indeterminate work; use `Steps` only for discrete named stages.

## API

| Rust (`penna::Steps`) | HTML (`uses=steps`) | Role |
| --- | --- | --- |
| `Steps::new(labels)` | `.penna-steps[role="list"]` | Horizontal step indicator |
| `.id(...)` | `role="listitem"` per `.penna-step` | Accessible list semantics |
| `.current(index)` | `data-place="done" \| "current" \| "later"` | Step progression state |
| — | `.penna-step-line[data-penna="step"]` | 28 × 2 px connector (`data-tone="primary"` when reached) |
| — | `.penna-step-dot[data-penna="step"]` | 20 px badge (`check` icon when done) |
