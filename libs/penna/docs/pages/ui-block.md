# UI block

Renders a structured surface from JSON messages or compact express syntax inside an agent chat or panel, built entirely from penna's own components and tokens.

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning">
  <div class="penna-notice-tone" data-penna="chip" data-tone="warning">
    <span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span>
    <div class="penna-notice-body">
      <span class="penna-text penna-notice-title" data-penna-text>Gap against the spec</span>
      <span class="penna-notice-detail">Built natively with the <code>ui-blocks</code> story. A full HTML/JS runtime twin, data-bound tones, and field validation checks are not built yet.</span>
    </div>
  </div>
</div>
```

## Anatomy

```figure uses=a2ui,card,badge,button,progress,label
<div class="penna-a2ui">
  <div class="penna-a2ui-grid" style="--cols: 3">
    <div class="penna-card penna-a2ui-stat" data-penna="card.activity" data-ground>
      <span class="penna-a2ui-stat-label" data-penna-text>Builds</span>
      <span class="penna-a2ui-stat-value" data-penna-text>42</span>
    </div>
    <div class="penna-card penna-a2ui-stat" data-penna="card.activity" data-ground>
      <span class="penna-a2ui-stat-label" data-penna-text>Passing</span>
      <span class="penna-a2ui-stat-value" data-penna-text>97.6%</span>
    </div>
    <div class="penna-card penna-a2ui-stat" data-penna="card.activity" data-ground>
      <span class="penna-a2ui-stat-label" data-penna-text>Median</span>
      <span class="penna-a2ui-stat-value" data-penna-text>1m 14s</span>
    </div>
  </div>
  <div class="penna-card penna-a2ui-card" data-penna="card.activity" data-ground>
    <div class="penna-a2ui-row" style="justify-content: space-between">
      <span class="penna-type" data-type="body" data-strong data-penna-text>Release v0.42.0</span>
      <span class="penna-badge" data-penna="badge" data-tone="warning"><span class="badge-text" data-penna-text>Waiting</span></span>
    </div>
    <div class="penna-progress" role="progressbar" aria-label="Checks" aria-valuemin="0" aria-valuemax="1" aria-valuenow="0.75" style="--penna-value: 75%"><div class="penna-progress-fill"></div></div>
    <div class="penna-a2ui-row" style="justify-content: flex-end">
      <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
      <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Deploy</span></button>
    </div>
  </div>
</div>
```

A surface holds a `Document` fed by `parse_messages` (A2UI JSONL) or `compile_express` (a compact shorthand that compiles to a single `Message::CreateSurface`). `A2uiSurface` owns the live inputs, selects, local state, and data-model bindings, and emits `A2uiEvent::Action` when a button is pressed.

```rust
use penna::blocks::a2ui::{
    compile_express, parse_messages, A2uiEvent, A2uiSurface, Mode,
};

let surface = cx.new(|cx| {
    let mut surface = A2uiSurface::new("deploy");
    for message in parse_messages(json_lines).unwrap_or_default() {
        surface.apply(message, cx);
    }
    surface
});

cx.subscribe(&surface, |_, surface, event: &A2uiEvent, cx| {
    let A2uiEvent::Action(action) = event;
    let pressed = Some(action.source_component_id.clone());
    surface.update(cx, |s, cx| s.set_mode(Mode::Held { pressed }, cx));
});

let message = compile_express(shorthand)?;
```

## States

Once the user presses an action button, the app puts the block in `Mode::Held { pressed }`: every control disables while keeping its value at full opacity, and the pressed button gains a `check` icon so transcript history stays legible.

```figure uses=a2ui,card,badge,button,progress,label
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Live</span>
      <span class="docs-lk-mono">Mode::Live</span>
    </div>
    <div class="penna-card penna-a2ui-card" data-penna="card.activity" data-ground>
      <div class="penna-a2ui-row" style="justify-content: space-between">
        <span class="penna-type" data-type="body" data-strong data-penna-text>Deploy to staging</span>
        <span class="penna-badge" data-penna="badge" data-tone="warning"><span class="badge-text" data-penna-text>Pending</span></span>
      </div>
      <div class="penna-a2ui-row" style="justify-content: flex-end">
        <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Skip</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Deploy</span></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Held</span>
      <span class="docs-lk-mono">Mode::Held</span>
    </div>
    <div class="penna-card penna-a2ui-card" data-penna="card.activity" data-ground>
      <div class="penna-a2ui-row" style="justify-content: space-between">
        <span class="penna-type" data-type="body" data-strong data-penna-text>Deploy to staging</span>
        <span class="penna-badge" data-penna="badge" data-tone="success"><span class="badge-text" data-penna-text>Answered</span></span>
      </div>
      <div class="penna-a2ui-row" style="justify-content: flex-end">
        <button class="penna-button" data-penna="button" type="button" disabled><span class="penna-label" data-penna-text>Skip</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button" disabled><i data-icon="check"></i><span class="penna-label" data-penna-text>Deploy</span></button>
      </div>
    </div>
  </div>
</div>
```

## Catalog

| Group | Components |
| --- | --- |
| Layout | `Column`, `Row`, `Grid`, `Card` |
| Display | `Text`, `Stat`, `Badge`, `Progress`, `Chart`, `Table` |
| Controls | `Button`, `TextField`, `Select`, `Segmented`, `Slider` |

- **Placement**: inside a chat, a block sits in `card.figure` with no inset and never starts text selection in the surrounding reply. `Card` components paint as `card.activity`.
- **Phone**: `.set_touch(true, cx)` caps grids at 2 columns, grows buttons to 44 px tall (at least 120 px wide), steps `TextField` and `Select` up to `touch-large` 56 px, and scales text to touch rungs (`Slider` also reads `CoarsePointer::get(cx)`).
- **Safety**: nesting stops at 32 levels deep; `Document::apply` collects malformed component warnings in `.problems()` and still renders the valid parts.

## API (`penna::blocks::a2ui`)

| Rust | HTML | Does |
| --- | --- | --- |
| `parse_messages(text)` | none | Parses A2UI JSONL into `Vec<Message>` |
| `compile_express(text)` | none | Compiles compact express syntax into a `Message::CreateSurface` |
| `Document::default()`, `.apply(message)` | none | Applies messages to component tree and data model; returns `Vec<String>` warnings |
| `Document::{model, is_empty, problems, buttons}` | none | Inspects bound data, parse warnings, and action buttons |
| `A2uiSurface::new(id)` | `.penna-a2ui` | Interactive view over a `Document` |
| `.apply(message, cx)`, `.document()` | none | Applies a message to the live surface or reads its `Document` |
| `.set_mode(Mode, cx)` | none | Switches between `Mode::Live` and `Mode::Held { pressed }` |
| `.set_touch(bool, cx)` | `@media (max-width: 760px)` | Enables phone touch layout on the surface |
| `A2uiEvent::Action(Action)` | none | Emitted on button press (`name`, `surface_id`, `source_component_id`, `context`) |
| `CATALOG`, `CATALOG_ID`, `EXPRESS`, `SpecError` | none | Schema, prompt references, and parse error type |
