# Which-key

A passive shortcut cheat sheet that appears after a prefix chord to show the next available bindings grouped by topic. It never steals focus or intercepts keys — the app's own key dispatcher handles the next stroke.

## Grouped sheet and user bindings

`WhichKeyView` sits on `SurfaceRole::SHEET`, shows the active prefix chord via [Keycap](kbd.html), groups bindings into balanced columns via `.fit(available)`, highlights user-defined bindings (`yours: true`) in the accent ink, and appends `↻` on repeatable bindings.

```figure uses=which_key
<div class="penna-which-key" data-penna="sheet" role="group" aria-label="prefix" style="max-width: 100%">
  <div class="penna-which-key-head">
    <kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b" data-kbd-lowercase></kbd>
    <span class="penna-which-key-text penna-which-key-name" data-penna-text>prefix</span>
    <span class="penna-which-key-more"><span class="penna-which-key-cap" data-penna-text>?</span><span class="penna-which-key-text" data-penna-text>all keys</span></span>
  </div>
  <div class="penna-which-key-body">
    <div class="penna-which-key-row" data-group="Windows"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text>c</span></span><span class="penna-which-key-label">New window</span></div>
    <div class="penna-which-key-row" data-group="Windows"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text>0–9</span></span><span class="penna-which-key-label">Select window</span></div>
    <div class="penna-which-key-row" data-group="Windows"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text>,</span></span><span class="penna-which-key-label">Rename window</span></div>
    <div class="penna-which-key-row" data-group="Panes"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text>%</span></span><span class="penna-which-key-label">Split left and right</span></div>
    <div class="penna-which-key-row" data-group="Panes"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text>"</span></span><span class="penna-which-key-label">Split top and bottom</span></div>
    <div class="penna-which-key-row" data-group="Panes"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text data-mac="←↓↑→">left down up right</span></span><span class="penna-which-key-label">Select pane</span><span class="penna-which-key-repeat" data-penna-text>↻</span></div>
    <div class="penna-which-key-row" data-group="Yours"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text data-yours>g</span></span><span class="penna-which-key-label">Open git status</span></div>
    <div class="penna-which-key-row" data-group="Yours"><span class="penna-which-key-caps"><span class="penna-which-key-cap" data-penna-text data-yours>t</span></span><span class="penna-which-key-label">Toggle tests</span></div>
  </div>
</div>
```

```rust
let header = WhichKeyHeader {
    table: "prefix".into(),
    prefix: Keystroke::parse("ctrl-b").ok(),
    prefix_raw: "^B".into(),
    more: Some(WhichKeyCap { keys: vec![], raw: "?".into(), yours: false }),
};

WhichKeyView::new(header, rows).fit(window.viewport_size())
```

## Packing and phone companion

- **Automatic column packing:** `.fit(size)` (`data-fit="W H"` in HTML) packs small groups into shared columns and splits tall groups across columns to stay inside the viewport without scrolling unless necessary.
- **Digit folding:** consecutive digit keys fold into compact ranges (`0–9`), keeping the sheet scannable.
- **Phone sheet:** on touch devices, `blocks::touch::WhichKeyList` renders the same `Arc<[WhichKeyRow]>` as a scrollable tap-to-run list inside a [Sheet](sheet.html).

## API

| Rust (`penna::blocks::which_key`) | HTML (`uses=which_key`) | Role |
| --- | --- | --- |
| `WhichKeyView::new(header, rows)` | `.penna-which-key[data-penna="sheet"][role="group"]` | Passive shortcut overlay |
| `.fit(Size<Pixels>)` | `data-fit="W H"` | Column packing budget |
| `WhichKeyHeader` | `.penna-which-key-head` | Prefix `Kbd`, table name, `? all keys` |
| `WhichKeyRow` (`group`, `repeat`) | `.penna-which-key-row[data-group]` | Binding row + optional `↻` repeat mark |
| `WhichKeyCap` (`yours`) | `.penna-which-key-cap[data-yours][data-mac]` | Keycap text (accent ink when user-defined) |
| `blocks::touch::WhichKeyList` | — | Tappable touch list companion (native) |
