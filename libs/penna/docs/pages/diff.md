# Diff

Structured code diffs (`DiffView`) and compact five-block change ratios (`DiffStat`) built on penna's semantic `Success` and `Danger` washes.

## Diff view and diff stat

Switch between `Unified` and `Split` layouts in the header bar; `DiffStat` renders proportional green/red blocks alongside exact `+added` / `−removed` counts.

```figure uses=diff,icon,segmented
<div class="penna-diff" data-penna="card" data-ground data-layout="unified">
  <div class="penna-diff-head">
    <span class="penna-diff-path"><i data-icon="file"></i><span>src/foundation/motion.rs</span></span>
    <div class="penna-diff-tools">
      <span class="penna-diff-stat"><span class="penna-diff-add">+3</span><span class="penna-diff-rem">−1</span><span class="penna-diff-pips" aria-hidden="true"><i data-kind="add"></i><i data-kind="add"></i><i data-kind="add"></i><i data-kind="add"></i><i data-kind="rem"></i></span></span>
      <div class="penna-segmented" data-penna="field" data-size="small" data-value="0" role="radiogroup" aria-label="Diff layout" data-diff-layout>
        <div class="penna-segments" data-penna="button.quiet" data-state="current">
          <span class="penna-segment-pill"></span>
          <span class="penna-segment" role="radio" aria-checked="true"><span class="penna-label" data-penna-text>Unified</span></span>
          <span class="penna-segment" role="radio" aria-checked="false"><span class="penna-label" data-penna-text>Split</span></span>
        </div>
      </div>
    </div>
  </div>
  <div class="penna-diff-hunk">@@ -15,5 +15,7 @@ pub const FAST_ANIMATION_SPEED: f32 = 2.0;</div>
  <div class="penna-diff-body">
    <div class="penna-diff-lines">
      <div class="penna-diff-line" data-kind="ctx"><span class="penna-diff-num">15</span><span class="penna-diff-num">15</span><span class="penna-diff-mark"> </span><span class="penna-diff-code">pub const SURFACE_ENTER_DURATION: Duration = Duration::from_millis(160);</span></div>
      <div class="penna-diff-line" data-kind="rem"><span class="penna-diff-num">16</span><span class="penna-diff-num"></span><span class="penna-diff-mark">−</span><span class="penna-diff-code">pub const SURFACE_LEAVE_DURATION: Duration = Duration::from_millis(140);</span></div>
      <div class="penna-diff-line" data-kind="add"><span class="penna-diff-num"></span><span class="penna-diff-num">16</span><span class="penna-diff-mark">+</span><span class="penna-diff-code">pub const SURFACE_LEAVE_DURATION: Duration = Duration::from_millis(100);</span></div>
      <div class="penna-diff-line" data-kind="add"><span class="penna-diff-num"></span><span class="penna-diff-num">17</span><span class="penna-diff-mark">+</span><span class="penna-diff-code">pub const HOVER_CARD_OPEN_DELAY: Duration = Duration::from_millis(240);</span></div>
      <div class="penna-diff-line" data-kind="add"><span class="penna-diff-num"></span><span class="penna-diff-num">18</span><span class="penna-diff-mark">+</span><span class="penna-diff-code">pub const HOVER_CARD_CLOSE_DELAY: Duration = Duration::from_millis(140);</span></div>
      <div class="penna-diff-line" data-kind="ctx"><span class="penna-diff-num">17</span><span class="penna-diff-num">19</span><span class="penna-diff-mark"> </span><span class="penna-diff-code">pub const SURFACE_RISE: f32 = 6.0;</span></div>
    </div>
  </div>
</div>
```

```rust
DiffView::new("motion-diff", "src/foundation/motion.rs")
    .hunk(DiffHunk::new(
        "@@ -15,5 +15,7 @@",
        vec![
            DiffLine::context(15, 15, "pub const SURFACE_ENTER_DURATION = ...;"),
            DiffLine::removed(16, "pub const SURFACE_LEAVE_DURATION = ...;"),
            DiffLine::added(16, "pub const SURFACE_LEAVE_DURATION = ...;"),
        ],
    ))
```

## API (`penna::DiffView`, `penna::DiffStat`)

| Rust | HTML (`uses=diff`) | Role |
| --- | --- | --- |
| `DiffView::new(id, path)` | `.penna-diff[data-penna="card"]` | File diff card with header and line gutters |
| `.hunk(DiffHunk)` | `.penna-diff-hunk` + `.penna-diff-line` | Added, removed, and context lines |
| `.layout(Unified \| Split)` | `data-layout="unified \| split"` | Toggle inline unified vs side-by-side layout |
| `DiffStat::new(added, removed)` | `.penna-diff-stat` | Tabular `+N −M` counters and 5-block ratio bar |
