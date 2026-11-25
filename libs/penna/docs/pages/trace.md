# Trace

A collapsible tool-call and execution timeline for agents, build pipelines, and diagnostics — pairing live spinner/status glyphs, tabular durations, and expandable output wells.

## Agent execution timeline

Click any step row with a disclosure chevron to inspect its captured tool output.

```figure uses=trace,icon,spinner
<div class="penna-trace" data-penna="card" data-ground>
  <div class="penna-trace-head">
    <span class="penna-trace-title">Verify Wave 3 widgets</span>
    <span class="penna-trace-meta">3 steps · 1.26 s</span>
  </div>
  <div class="penna-trace-step">
    <button class="penna-trace-row" type="button" aria-expanded="false">
      <i data-icon="circle-check" data-tone="success"></i>
      <span class="penna-trace-tool">cargo clippy</span>
      <span class="penna-trace-target">--lib --features tree-sitter -- -D warnings</span>
      <span class="penna-trace-ms">260 ms</span>
      <i class="penna-trace-chev" data-icon="chevron-right"></i>
    </button>
    <div class="penna-trace-detail" hidden>
      <pre class="penna-trace-code" data-penna="card.code">Finished `dev` profile [unoptimized + debuginfo] in 0.26s — 0 warnings</pre>
    </div>
  </div>
  <div class="penna-trace-step">
    <button class="penna-trace-row" type="button" aria-expanded="true">
      <i data-icon="circle-check" data-tone="success"></i>
      <span class="penna-trace-tool">cargo test</span>
      <span class="penna-trace-target">--lib (474 unit tests)</span>
      <span class="penna-trace-ms">980 ms</span>
      <i class="penna-trace-chev" data-icon="chevron-right"></i>
    </button>
    <div class="penna-trace-detail">
      <pre class="penna-trace-code" data-penna="card.code">test blocks::table::tests::table_sort_cycles_and_orders_numeric_and_text_columns ... ok
test blocks::diff::tests::diff_stat_computes_counts_and_five_block_buckets ... ok
test result: ok. 474 passed; 0 failed</pre>
    </div>
  </div>
  <div class="penna-trace-step">
    <button class="penna-trace-row" type="button" aria-expanded="false">
      <span class="penna-spinner" data-size="xsmall" data-color="accent"><i data-icon="loader"></i></span>
      <span class="penna-trace-tool">docs --check</span>
      <span class="penna-trace-target">verifying 82 pages &amp; 107 icons</span>
      <span class="penna-trace-ms">20 ms</span>
      <i class="penna-trace-chev" data-icon="chevron-right"></i>
    </button>
    <div class="penna-trace-detail" hidden>
      <pre class="penna-trace-code" data-penna="card.code">docs check ok: 82 pages across 4 groups</pre>
    </div>
  </div>
</div>
```

```rust
Trace::new(
    "wave3-trace",
    vec![
        TraceStep::new("cargo clippy", "--lib -- -D warnings")
            .status(TraceStatus::Done)
            .duration_ms(260)
            .detail("0 warnings"),
        TraceStep::new("cargo test", "--lib")
            .status(TraceStatus::Running)
            .duration_ms(980),
    ],
)
.title("Verify Wave 3 widgets")
```

## API (`penna::Trace`)

| Rust | HTML (`uses=trace`) | Role |
| --- | --- | --- |
| `Trace::new(id, steps)` | `.penna-trace[data-penna="card"]` | Collapsible timeline container |
| `.title(text)` | `.penna-trace-head` | Optional header bar with step count and total duration |
| `TraceStep::new(tool, target)` | `.penna-trace-row` | Monospace tool name + truncated target summary |
| `.status(Running \| Done \| Error)` | spinner / check / xmark glyph | Live execution indicator |
| `.duration_ms(u64)` / `.detail(str)` | `.penna-trace-ms` / `.penna-trace-detail` | Tabular timing and expandable output well |
