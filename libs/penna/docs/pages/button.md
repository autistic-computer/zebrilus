# Button

A pressable control. Pick a look, give it a label or an icon, and handle the click.

```rust example story=buttons/variants
Button::new("primary").label("Primary").primary()
Button::new("secondary").label("Secondary")
Button::new("ghost").label("Ghost").ghost()
Button::new("danger").label("Danger").danger()
Button::new("link").label("Link").link()
```

## Sizes and icons

```rust example story=buttons/sizes
Button::new("small").label("Small").small()
Button::new("medium").label("Medium")
Button::new("large").label("Large").large()
Button::new("bell").icon(IconName::Bell).tooltip("Notifications")
Button::compact_icon("more", IconName::Ellipsis).tooltip("More")
```

## States

```rust example story=buttons/states
Button::new("save").label("Save").disabled(true)
Button::new("tab").label("Ghost").ghost().selected(true)
Button::new("saving").label("Saving").loading(true)
```

## API

| Rust | HTML | What it does |
| --- | --- | --- |
| `.label(text)` | `.penna-label` | Text inside the button |
| `.icon(name)` | `<i data-icon>` | An icon before the label |
| `.primary()`, `.danger()`, `.ghost()` | `data-tone`, `data-penna="button.quiet"` | The look |
| `.xsmall()`, `.small()`, `.large()`, `.xlarge()`, `.touch()` | `data-size="xsmall"`, `"small"`, `"large"`, `"xlarge"`, `"touch"` | A step on the size ladder; medium is the default |
| `.selected(true)` | `data-state="current"` | Draws it picked |
| `.tooltip(text)` | `data-tooltip` | A tooltip on hover |
| `.on_click(handler)` | a `click` listener | Runs on press |
