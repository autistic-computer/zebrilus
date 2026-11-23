# Input

Takes typed text. To pick from a fixed list use a [select](select.html); for on or off, a [switch](switch.html).

## Kinds

```figure uses=input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 184px)"><label class="penna-input" data-penna="field"><input placeholder="Search sessions" aria-label="Field input" /></label><span class="docs-note">Field</span></div>
  <div class="docs-cell" style="width: min(100%, 184px)"><label class="penna-input" data-penna="field.outline"><input placeholder="Search sessions" aria-label="Outline input" /></label><span class="docs-note">Outline</span></div>
  <div class="docs-cell" style="width: min(100%, 184px)"><div class="penna-number" data-penna="field" role="spinbutton" data-step="1" data-min="0"><button class="penna-button" data-penna="button.quiet" data-compact type="button" tabindex="-1" data-step-by="-1" aria-label="Decrease"><i data-icon="minus"></i></button><label class="penna-input" data-penna="field" data-align="center" data-bare><input value="4" inputmode="decimal" aria-label="Count" /></label><button class="penna-button" data-penna="button.quiet" data-compact type="button" tabindex="-1" data-step-by="1" aria-label="Increase"><i data-icon="plus"></i></button></div><span class="docs-note">Number</span></div>
  <div class="docs-cell" style="width: min(100%, 184px)"><label class="penna-input" data-penna="field"><input type="password" value="correct horse" aria-label="Password input" /></label><span class="docs-note">Password</span></div>
  <div class="docs-cell" style="width: min(100%, 384px)"><label class="penna-input" data-penna="field.multiline" data-multiline><textarea rows="2" data-min-rows="2" data-max-rows="6" aria-label="Notes">Panes split along the long edge.
New windows open in the current directory.</textarea></label><span class="docs-note">Multi-line</span></div>
</div>
```

- **Field**: the default. Soft fill, no edge.
- **Outline**: an edge with no fill at rest, for grounds with their own fill.
- **Number**: steps with its buttons or Up and Down; slide on the value to scrub it, a step per 4 points.
- **Multi-line**: grows from `min` to `max` rows, then scrolls.
- **Password**: masks typed characters with `InputContentType::Password`.
- **Bare** (`.appearance(false)`) and **Borderless** (`.bordered(false)`): strip chrome or edge inside a container that draws its own.

```rust
let search = cx.new(|cx| {
    InputState::new(window, cx).placeholder("Search sessions")
});
Input::new(&search).cleanable(true)
```

## Anatomy

```figure uses=input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 240px)"><label class="penna-input" data-penna="field" data-size="xlarge"><span class="penna-input-prefix"><i data-icon="search"></i></span><input placeholder="Jump to a pane" aria-label="Jump to a pane" /><span class="penna-input-suffix"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></span></label><span class="docs-note">Prefix, placeholder, suffix</span></div>
  <div class="docs-cell" style="width: min(100%, 240px)"><label class="penna-input" data-penna="field" data-size="xlarge"><input value="build logs" aria-label="Filter logs" /><span class="penna-input-suffix"><button class="penna-button penna-input-clear" data-penna="button.quiet" data-shape="icon" data-size="large" type="button" tabindex="-1" aria-label="Clear"><i data-icon="circle-x"></i></button></span></label><span class="docs-note">Text, clear button</span></div>
</div>
```

1. **Container**: a wash and corners, a deeper one while focused. The size sets its height and side padding.
2. **Prefix**: optional, before the text, at most 75% of the width. Icons are muted.
3. **Text**: the value, or a faint placeholder.
4. **Suffix**: optional. Holds the spinner while loading, the clear button when `.cleanable(true)`, and caller accessories.

## Sizes

```figure uses=input
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span><label class="penna-input" data-penna="field" data-size="xsmall" style="width: 200px"><input value="git log --stat" aria-label="xsmall input" /></label></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span><label class="penna-input" data-penna="field" data-size="small" style="width: 200px"><input value="git log --stat" aria-label="small input" /></label></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span><label class="penna-input" data-penna="field" style="width: 200px"><input value="git log --stat" aria-label="medium input" /></label></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span><label class="penna-input" data-penna="field" data-size="large" style="width: 200px"><input value="git log --stat" aria-label="large input" /></label></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span><label class="penna-input" data-penna="field" data-size="xlarge" style="width: 200px"><input value="git log --stat" aria-label="xlarge input" /></label></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span><label class="penna-input" data-penna="field" data-size="touch" style="width: 200px"><input value="git log --stat" aria-label="touch input" /></label></span>
  <span>touch-large</span><span><code>56</code></span><span><code>touch-body</code></span><span><label class="penna-input" data-penna="field" data-size="touch-large" style="width: 200px"><input value="git log --stat" aria-label="touch-large input" /></label></span>
</div>
```

Medium is the default and follows density. Same size as the controls beside it.

## States

```figure uses=input,spinner
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field"><input placeholder="Search sessions" aria-label="Rest input" /></label><span class="docs-note">Rest</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field" data-state="hover"><input placeholder="Search sessions" aria-label="Hover input" /></label><span class="docs-note">Hover</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field" data-state="focused"><input value="git switch -c build" aria-label="Focused input" /></label><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field.outline" data-state="focused"><input value="studio.local" aria-label="Outline focused input" /></label><span class="docs-note">Outline, focused</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field"><input value="deploy --target staging" data-selection="9,23" aria-label="Selection input" /></label><span class="docs-note">Selection</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field" aria-disabled="true" data-disabled><input value="~/.config/git/config" disabled aria-label="Disabled input" /></label><span class="docs-note">Off</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field"><input value="Resolving host" aria-label="Busy input" /><span class="penna-input-suffix"><span class="penna-spinner"><i data-icon="loader"></i></span></span></label><span class="docs-note">Busy</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" aria-label="Error input" /></label><span class="docs-note">Error</span></div>
  <div class="docs-cell" style="width: min(100%, 192px)"><label class="penna-input" data-penna="field" data-tone="danger" data-state="focused"><input value="studio mac" aria-label="Error focused input" /></label><span class="docs-note">Error, focused</span></div>
</div>
```

- Focused: the fill takes a selected row's highlight with no edge; the outline kind keeps its rest edge.
- Off: faint text and affixes, no editing.
- Busy: a spinner in the suffix; the text stays editable.
- Error: `.tone(Tone::Danger)` paints only the edge, at rest and focused. Its [Field](field.html) says what is wrong.

A search field and the picked row under it share one highlight.

```figure uses=input,list
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 240px)"><label class="penna-input" data-penna="field" data-state="focused"><span class="penna-input-prefix"><i data-icon="search"></i></span><input value="dev" aria-label="Search folders" /></label><div class="penna-list" role="listbox" style="width: 100%"><div class="penna-list-item" data-penna="menu-item" role="option" data-state="selected" aria-selected="true"><div class="penna-list-content"><i class="penna-list-icon" data-icon="folder"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>dev</span></span></div></div><div class="penna-list-item" data-penna="menu-item" role="option"><div class="penna-list-content"><i class="penna-list-icon" data-icon="folder"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>dev-notes</span></span></div></div></div><span class="docs-note">Focused, over the picked row</span></div>
</div>
```

## Labels

In a form, a [Field](field.html) puts the label above it. The placeholder only hints, and goes with the first key.

```figure uses=field,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-field" style="width: min(100%, 200px)"><span class="penna-field-label" data-penna-text>Host</span><label class="penna-input" data-penna="field"><input placeholder="studio.local" /></label></div></div><figcaption>Label above, hint inside</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-field" style="width: min(100%, 200px)"><label class="penna-input" data-penna="field"><input placeholder="Host" aria-label="Host" /></label></div></div><figcaption>Don't use the placeholder as the label</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Input::new(&state)`, `InputState::new(window, cx)` | `label.penna-input[data-penna="field"]` | Text field and its state |
| `.placeholder(text)`, `.default_value(text)` on state | `placeholder`, `value` | Hint and starting text |
| `.value()`, `.set_value(..)`, `.validate(..)` on state | `input.value` | Read, write, and filter edits |
| `.prefix(el)`, `.suffix(el)` | `.penna-input-prefix`, `.penna-input-suffix` | Before and after the text |
| `.emphasis(Emphasis::Outline)` | `data-penna="field.outline"` | Outline kind |
| `.auto_grow(min, max)`, `.set_max_rows(..)` on state | `field.multiline`, `data-min-rows`, `data-max-rows` | Multi-line growth |
| `NumberInput::new(&state)`, `.step()`, `.min()`, `.max()` on state | `.penna-number`, `data-step`, `data-min`, `data-max` | Number kind |
| `.xsmall()` … `.touch_large()`, `.with_size(size)` | `data-size` | Size; medium by default |
| `.cleanable(true)` | `.penna-input-clear` | Clear button |
| `.set_loading(true, ..)` on state | `.penna-spinner` in suffix | Busy |
| `.content_type(InputContentType::Password)` | `type="password"` | Masked text (`Url`, `EmailAddress` too) |
| `.text_align(align)` | `data-align` (`center`, `right`) | Text alignment |
| `.disabled(true)` | `data-disabled`, `aria-disabled="true"`, `disabled` | Off |
| `.tone(Tone::Danger)` | `data-tone="danger"` | Error edge |
| `.appearance(false)`, `.bordered(false)` | `data-bare`, `data-borderless` | Strip chrome or edge |
| `.on(ground)` | `data-ground` on parent | Paint for the ground under it |
| `InputEvent::PressEnter { shift }`, `Change`, `Focus`, `Blur` | `keydown`, `input`, `focus`, `blur` | State events |
