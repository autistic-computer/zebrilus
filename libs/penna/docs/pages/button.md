# Button

A pressable control. Pick a look, give it a label or an icon, and handle the click.

```rust example
Button::new("primary").label("Primary").primary()
Button::new("secondary").label("Secondary")
Button::new("ghost").label("Ghost").ghost()
Button::new("danger").label("Danger").danger()
```

```html example
<button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Primary</span></button>
<button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Secondary</span></button>
<button class="penna-button" data-penna="button.quiet"><span class="penna-label" data-penna-text>Ghost</span></button>
<button class="penna-button" data-penna="button" data-tone="danger"><span class="penna-label" data-penna-text>Danger</span></button>
```

## Icons and sizes

```rust example
Button::new("copy").icon(IconName::Copy).label("Copy")
Button::new("small").label("Small").small()
Button::new("add").icon(IconName::Plus).ghost()
```

```html example
<button class="penna-button" data-penna="button"><i data-icon="copy"></i><span class="penna-label" data-penna-text>Copy</span></button>
<button class="penna-button" data-penna="button" data-size="small"><span class="penna-label" data-penna-text>Small</span></button>
<button class="penna-button" data-penna="button.quiet" data-shape="icon" aria-label="Add"><i data-icon="plus"></i></button>
```

## API

| Rust | HTML | What it does |
| --- | --- | --- |
| `.label(text)` | `.penna-label` | Text inside the button |
| `.icon(name)` | `<i data-icon>` | An icon before the label |
| `.primary()`, `.danger()`, `.ghost()` | `data-tone`, `data-penna="button.quiet"` | The look |
| `.small()` | `data-size="small"` | The compact size |
| `.selected(true)` | `data-state="current"` | Draws it picked |
| `.tooltip(text)` | `data-tooltip` | A tooltip on hover |
| `.on_click(handler)` | a `click` listener | Runs on press |
