# UI block

Renders a structured surface from JSON messages inside an agent chat or panel, built entirely from penna's own components and tokens.

## Anatomy

```figure uses=a2ui,card,badge,button,progress,label
<div class="penna-a2ui">
  <div class="penna-a2ui-grid" style="--cols: 3">
    <div class="penna-card penna-a2ui-stat" data-penna="card" data-ground>
      <span class="penna-a2ui-stat-label">Builds</span>
      <span class="penna-a2ui-stat-value">42</span>
      <span class="penna-a2ui-stat-detail">Last 24h</span>
    </div>
    <div class="penna-card penna-a2ui-stat" data-penna="card" data-ground>
      <span class="penna-a2ui-stat-label">Passing</span>
      <span class="penna-a2ui-stat-value">97.6%</span>
      <span class="penna-a2ui-stat-detail">1 flaky test</span>
    </div>
    <div class="penna-card penna-a2ui-stat" data-penna="card" data-ground>
      <span class="penna-a2ui-stat-label">Median</span>
      <span class="penna-a2ui-stat-value">1m 14s</span>
      <span class="penna-a2ui-stat-detail">-12s vs main</span>
    </div>
  </div>
  <div class="penna-card penna-a2ui-card" data-penna="card" data-ground>
    <div class="penna-a2ui-row" style="justify-content: space-between">
      <span class="penna-type" data-type="body" data-strong data-penna-text>Release v0.42.0</span>
      <span class="penna-badge" data-penna="badge" data-tone="warning"><span class="badge-text" data-penna-text>Waiting</span></span>
    </div>
    <div class="penna-progress" role="progressbar" aria-label="Checks" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.75" style="--penna-value: 75%"><div class="penna-progress-fill"></div></div>
    <div class="penna-a2ui-row" style="justify-content: flex-end">
      <button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Deploy</span></button>
    </div>
  </div>
</div>
```

A surface is a `Document` fed by `parse_messages` (standard A2UI JSONL) or `compile_express` (a compact shorthand). `A2uiSurface` owns the live inputs, selects, local state and data-model bindings, and emits `A2uiEvent::Action` when a button is pressed.

```rust
let mut doc = Document::new();
for message in parse_messages(json_lines)? {
    doc.apply(message)?;
}
let surface = cx.new(|_| A2uiSurface::new("deploy").with_document(doc));
```

## Catalog

| Group | Components |
| --- | --- |
| Layout | `Row`, `Column`, `Grid`, `Card`, `Divider`, `Tabs` |
| Display | `Text`, `Stat`, `Badge`, `Progress`, `Chart`, `Table` |
| Controls | `Button`, `TextField`, `CheckBox`, `Switch`, `Slider`, `ChoicePicker`, `Segmented` |

- On a phone (or under `CoarsePointer`), grids cap at 2 columns, buttons grow to at least 120 wide, and controls step up to touch sizes.
- `Mode::Held { pressed }` freezes a block after the user answers it, highlighting the button that was pressed so transcript history stays legible.
- Nesting stops at 32 levels deep so malformed trees can't loop.

## API

| Rust | Does |
| --- | --- |
| `parse_messages(text)` | Parses A2UI JSONL messages |
| `compile_express(text)` | Compiles the compact shorthand into messages |
| `Document::new()`, `.apply(message)` | Holds the component tree and data model |
| `A2uiSurface::new(id)` | Interactive view over a `Document` |
| `.set_document(doc, cx)` | Replaces the document in place |
| `.set_mode(Mode::Held { pressed }, cx)` | Freezes a completed surface in a transcript |
| `.touch(true)` | Forces touch layout |
| `A2uiEvent::Action(Action)` | Fires when a button submits an action and resolved context |
| `CATALOG`, `CATALOG_ID`, `EXPRESS` | Schema and prompt references for model callers |
