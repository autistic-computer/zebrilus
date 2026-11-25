# Snippet

A compact, copyable command, address, or token strip on `card.code` with an inline copy button that flips to a check mark on copy.

## Copyable commands

Click the trailing copy button to copy the command cleanly without selecting the prefix symbol.

```figure uses=snippet,button,icon
<div style="display: flex; flex-direction: column; gap: 10px; width: 100%; max-width: 460px">
  <div class="penna-snippet" data-penna="card.code" data-ground>
    <span class="penna-snippet-prefix" aria-hidden="true">$</span>
    <code class="penna-snippet-code">cargo run --example docs --features tree-sitter -- --check</code>
    <button class="penna-button penna-snippet-copy" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Copy command" data-snippet-copy><i data-icon="copy"></i></button>
  </div>
  <div class="penna-snippet" data-penna="card.code" data-ground>
    <code class="penna-snippet-code">zz://studio.tailnet.ts.net:7443/work</code>
    <button class="penna-button penna-snippet-copy" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Copy address" data-snippet-copy><i data-icon="copy"></i></button>
  </div>
</div>
```

```rust
Snippet::new("docs-cmd", "cargo run --example docs -- --check")
    .prefix(Some("$"))
```

## API (`penna::Snippet`)

| Rust | HTML (`uses=snippet`) | Role |
| --- | --- | --- |
| `Snippet::new(id, code)` | `.penna-snippet[data-penna="card.code"]` | Monospace command or token strip |
| `.prefix(Option<str>)` | `.penna-snippet-prefix` | Non-copied leading prompt (`$` by default) |
| `.copyable(bool)` | `button[data-snippet-copy]` | Trailing quiet icon button writing to clipboard |
