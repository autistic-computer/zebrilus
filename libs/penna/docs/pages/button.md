# Button

A pressable control. Pick a look, give it a label or an icon, and handle the click.

```rust example story=buttons/variants
Button::new("primary").label("Primary").primary()
Button::new("secondary").label("Secondary")
Button::new("ghost").label("Ghost").ghost()
Button::new("danger").label("Danger").danger()
Button::new("link").label("Link").link()
```

## Anatomy

A button has a container and up to three parts inside it, in this order:

- **Container**: the shape, with its fill, outline and corners. The look and the theme draw it, and the size sets its height.
- **Icon**: optional, before the label. While the button is loading it turns into a spinner.
- **Label**: the text. The size of the button sets how big it is.
- **Caret**: optional, a small chevron at the far end, for a button that opens a menu.

When the button is reached from the keyboard, a focus ring appears just outside the container.

Each size sets the height, the text, the padding at the sides, the icon and the space between the icon and the label together, so two buttons of one size always match. A button with only an icon is square.

## Sizes and icons

```rust example story=buttons/sizes
Button::new("xsmall").label("Extra small").xsmall()
Button::new("small").label("Small").small()
Button::new("medium").label("Medium")
Button::new("large").label("Large").large()
Button::new("xlarge").label("Extra large").xlarge()
Button::new("touch").label("Touch").touch()
Button::new("bell").icon(IconName::Bell).tooltip("Notifications")
Button::compact_icon("more", IconName::Ellipsis).tooltip("More")
```

## States

```rust example story=buttons/states
Button::new("save").label("Save").disabled(true)
Button::new("tab").label("Ghost").ghost().selected(true)
Button::new("saving").label("Saving").loading(true)
```

Pointing at a button lifts it. Each picture below shows the state on the left, beside a resting copy.

```figure story=buttons/hovered
```

Pressing deepens the fill for as long as the button is held.

```figure story=buttons/pressed
```

Keyboard focus draws a ring outside the edge. Only the keyboard brings it up; a click does not.

```figure story=buttons/focused
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
