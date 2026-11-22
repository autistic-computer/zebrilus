# Key field

Records a single shortcut and shows it as a keycap. For displaying a shortcut that isn't edited, use [Kbd](kbd.html).

## Anatomy

```figure uses=kbd,key_field
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="ctrl-b"></div><span class="docs-note">Recorded key</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="cmd-shift-p"></div><span class="docs-note">Chord</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-placeholder="Unbound"></div><span class="docs-note">Empty</span></div>
</div>
```

1. **Field**: a standard field surface, centered when holding a keycap or placeholder, left-aligned while recording.
2. **Value**: the recorded keystroke rendered with `Kbd` (`Kbd::format`), or a faint placeholder ("None" by default) when unset.
3. **Recording**: clicking, or pressing Enter or Space while focused, captures the next non-modifier keystroke. Escape cancels without changing the binding; clicking away cancels too.

```rust
let prefix = cx.new(|cx| KeyFieldState::new(window, cx).value(Keystroke::parse("ctrl-b").ok()));
KeyField::new("prefix", &prefix).placeholder("Unbound")
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
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-state="focused recording"><span class="penna-key-field-text">Press a key</span></div><span class="docs-note">Recording</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="ctrl-b" aria-disabled="true"></div><span class="docs-note">Off</span></div>
  <div class="docs-cell" style="width: 140px"><div class="penna-key-field" data-penna="field" data-key="ctrl-b" data-tone="danger"></div><span class="docs-note">Conflict</span></div>
</div>
```

- Modifier-only presses stay in recording until a key lands.
- Shifted punctuation (like `"` or `%`) strips the redundant Shift modifier when recorded.
- `.tone(Tone::Danger)` paints the danger edge, paired with an error message from its [Field](field.html).

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `KeyFieldState::new(window, cx)` | `.penna-key-field[data-penna="field"]` | Holds the keystroke and recording state |
| `.value(keystroke)`, `.set_value(keystroke, cx)` | `data-key` | Current shortcut |
| `.record(window, cx)`, `.cancel(cx)` | `data-state="focused recording"` | Starts or stops listening |
| `KeyField::new(id, &state)` | `.penna-key-field` | Field view |
| `.placeholder(text)` | `data-placeholder` | Text when unbound ("None" by default) |
| `.xsmall()` … `.touch()` | `data-size` | Size; medium by default |
| `.tone(tone)` | `data-tone` | Edge tone |
| `.disabled(true)` | `aria-disabled="true"` | Off |
| `.on(ground)` | inside `[data-ground]` | Paints on a ground |
| `KeyFieldEvent::Record(keystroke)` | `change` | Fires when a key is captured |
