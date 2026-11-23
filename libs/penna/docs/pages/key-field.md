# Key field

Records a single shortcut and shows it as a keycap. For displaying a shortcut that isn't edited, use [Keycap](kbd.html); to type text, an [input](input.html).

## Anatomy

```figure uses=kbd,key_field
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="ctrl-b"></div><span class="docs-note">Recorded key</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="cmd-shift-p"></div><span class="docs-note">Chord</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-placeholder="Unbound"></div><span class="docs-note">Placeholder</span></div>
</div>
```

1. **Container**: a field surface with height and side inset from its rung, as wide as its container.
2. **Value**: the recorded keystroke drawn as a centered `Kbd` keycap (`sm` at `xs` and `sm`, `md` from `md` up).
3. **Placeholder**: with no keystroke, centered in the field's faint ink ("None" by default).
4. **Prompt**: while recording, "Press a key" left-aligned in the faint ink in place of the value.

```rust
let prefix = cx.new(|cx| {
    KeyFieldState::new(window, cx).value(Keystroke::parse("ctrl-b").ok())
});
KeyField::new(&prefix).id("prefix").placeholder("Unbound")
```

## Sizes

```figure uses=kbd,key_field
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span style="width: 128px"><div class="penna-key-field" data-penna="field" data-size="xsmall" data-key="ctrl-a"></div></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span style="width: 128px"><div class="penna-key-field" data-penna="field" data-size="small" data-key="ctrl-a"></div></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span style="width: 128px"><div class="penna-key-field" data-penna="field" data-key="ctrl-a"></div></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span style="width: 128px"><div class="penna-key-field" data-penna="field" data-size="large" data-key="ctrl-a"></div></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span style="width: 128px"><div class="penna-key-field" data-penna="field" data-size="xlarge" data-key="ctrl-a"></div></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span style="width: 128px"><div class="penna-key-field" data-penna="field" data-size="touch" data-key="ctrl-a"></div></span>
</div>
```

Medium is the default and follows density.

## States

```figure uses=kbd,key_field
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="cmd-shift-p"></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="cmd-shift-p" data-state="hover" data-still></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="cmd-shift-p" data-state="focused" data-still></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-state="focused recording" data-still></div><span class="docs-note">Recording</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="ctrl-b" aria-disabled="true"></div><span class="docs-note">Off</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="ctrl-b" data-tone="danger"></div><span class="docs-note">Error</span></div>
</div>
```

- A press focuses the field and starts recording; Tab only focuses it, and Enter or Space then starts recording.
- Modifier-only presses stay in recording until a key lands. Escape with no modifier cancels, and leaving the field cancels too.
- Shifted punctuation like `"` or `%` records as the symbol without a redundant Shift; Shift stays on letters and named keys.
- `.tone(Tone::Danger)` paints the danger edge, paired with an error message from its [Field](field.html).

## Placement

At the end of a settings row on the card's ground, sized to fit a chord (96 px holds three modifiers and a key at medium), or in a [Field](field.html) in a form. Use sentence-case placeholders ("None", "Unbound").

```figure uses=kbd,key_field,card,label
<div class="penna-card penna-inline" data-penna="settings.card" data-ground data-gap="group" style="width: min(100%, 360px); justify-content: space-between">
  <span class="penna-stack" data-gap="tight">
    <span class="penna-type" data-type="body" data-strong data-penna-text>Prefix key</span>
    <span class="penna-type" data-type="small" data-muted data-penna-text>Pressed before window commands.</span>
  </span>
  <div style="width: 96px; flex: none"><div class="penna-key-field" data-penna="field" data-key="ctrl-b"></div></div>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `KeyFieldState::new(window, cx)` | `.penna-key-field[data-penna="field"]` | Holds the keystroke and recording state |
| `.value(key)`, `.set_value(key, cx)`, `.keystroke()` | `data-key` | Initial, updated, and current shortcut |
| `.record(window, cx)`, `.cancel(cx)`, `.is_recording()` | `data-state="focused recording"` | Starts, stops, or checks recording |
| `.focus(window, cx)` | `.focus()` | Focuses without starting recording |
| `KeyField::new(&state)` | `.penna-key-field` | Field view |
| `.id(id)` | `id`, `data-id` | Author id for accessibility |
| `.placeholder(text)` | `data-placeholder` | Text when unbound ("None" by default) |
| `.xsmall()` … `.touch()`, `.with_size(size)` | `data-size` | Size; medium by default |
| `.tone(tone)` | `data-tone` | Edge tone |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.on(ground)` | `data-ground` on parent | Paints the field and cap on a ground |
| `KeyFieldEvent::Record(keystroke)` | `change` | Fires when a key is captured |
