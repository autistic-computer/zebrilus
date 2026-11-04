# Input

Takes typed text. To pick from a list use a select, for on or off a switch.

```figure uses=notice
<div class="docs-full penna-notice" data-penna="notice" data-tone="warning"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Being reworked</span><span class="penna-text penna-notice-detail" data-penna-text>An error only tints the field, with no message, and has no HTML look yet. There is no helper text.</span></div></div></div>
```

## Kinds

```figure uses=input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field"><input placeholder="Search sessions" /></label><span class="docs-note">Field</span></div>
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field.outline"><input placeholder="Search sessions" /></label><span class="docs-note">Outline</span></div>
  <div class="docs-cell" style="width: 192px"><div class="penna-number" data-penna="field" role="spinbutton" data-step="1" data-min="0"><button class="penna-button" data-penna="button.quiet" data-compact type="button" tabindex="-1" data-step-by="-1"><i data-icon="minus"></i></button><label class="penna-input" data-penna="field" data-align="center" data-bare><input value="4" inputmode="decimal" /></label><button class="penna-button" data-penna="button.quiet" data-compact type="button" tabindex="-1" data-step-by="1"><i data-icon="plus"></i></button></div><span class="docs-note">Number</span></div>
  <div class="docs-cell" style="width: 408px"><label class="penna-input" data-penna="field.multiline" data-multiline><textarea rows="2" data-min-rows="2" data-max-rows="6">Panes split along the long edge.
New windows open in the current directory.</textarea></label><span class="docs-note">Multi-line</span></div>
</div>
```

- **Field**: the default.
- **Outline**: on a ground with its own fill.
- **Number**: steps with its buttons or Up and Down.
- **Multi-line**: grows with its text, then scrolls.

```rust
Input::new(&search).cleanable(true)
```

## Anatomy

```figure uses=input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 240px"><label class="penna-input" data-penna="field" data-size="xlarge"><span class="penna-input-prefix"><i data-icon="search"></i></span><input placeholder="Jump to a pane" /><span class="penna-input-suffix"><kbd class="penna-kbd" data-penna="keycap" data-kbd="cmd-k"></kbd></span></label><span class="docs-note">Prefix, placeholder, suffix</span></div>
  <div class="docs-cell" style="width: 240px"><label class="penna-input" data-penna="field" data-size="xlarge"><input value="build logs" /><span class="penna-input-suffix"><button class="penna-button penna-input-clear" data-penna="button.quiet" data-shape="icon" data-size="large" type="button" tabindex="-1" aria-label="Clear"><i data-icon="circle-x"></i></button></span></label><span class="docs-note">Text, clear button</span></div>
</div>
```

1. **Container**: fill, edge, corners. The size sets its height and side padding.
2. **Prefix**: optional, before the text. Icons are muted.
3. **Text**: the value, or a faint placeholder.
4. **Suffix**: optional. Also holds the spinner and the clear button.

## Sizes

```figure uses=input
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Text</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>caption</code></span><span><label class="penna-input" data-penna="field" data-size="xsmall" style="width: 200px"><input value="git log --stat" /></label></span>
  <span>small</span><span><code>24</code></span><span><code>small</code></span><span><label class="penna-input" data-penna="field" data-size="small" style="width: 200px"><input value="git log --stat" /></label></span>
  <span>medium</span><span><code>28</code></span><span><code>body</code></span><span><label class="penna-input" data-penna="field" style="width: 200px"><input value="git log --stat" /></label></span>
  <span>large</span><span><code>32</code></span><span><code>body</code></span><span><label class="penna-input" data-penna="field" data-size="large" style="width: 200px"><input value="git log --stat" /></label></span>
  <span>xlarge</span><span><code>40</code></span><span><code>lead</code></span><span><label class="penna-input" data-penna="field" data-size="xlarge" style="width: 200px"><input value="git log --stat" /></label></span>
  <span>touch</span><span><code>44</code></span><span><code>touch-body</code></span><span><label class="penna-input" data-penna="field" data-size="touch" style="width: 200px"><input value="git log --stat" /></label></span>
</div>
```

Medium is the default and follows density. Same size as the controls beside it.

## States

```figure uses=input,spinner
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field"><input placeholder="Search sessions" /></label><span class="docs-note">Rest</span></div>
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field" data-state="hover"><input placeholder="Search sessions" /></label><span class="docs-note">Hover</span></div>
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field" data-state="focused"><input value="git switch -c build" /></label><span class="docs-note">Focused</span></div>
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field"><input value="deploy --target staging" data-selection="9,23" /></label><span class="docs-note">Selection</span></div>
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field" aria-disabled="true" data-disabled><input value="~/.config/git/config" disabled /></label><span class="docs-note">Off</span></div>
  <div class="docs-cell" style="width: 192px"><label class="penna-input" data-penna="field"><input value="Resolving host" /><span class="penna-input-suffix"><span class="penna-spinner"><i data-icon="loader"></i></span></span></label><span class="docs-note">Busy</span></div>
</div>
```

- Focused: the edge turns to the ink, the fill stays.
- Off: faint text, no editing.
- Busy: a spinner at the end; the text stays editable.

## Labels

A label above names the field. The placeholder only hints, and goes with the first key.

```figure uses=label,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="docs-form" data-spacing="grouped" style="width: 200px"><div class="docs-field"><span class="penna-type" data-type="small" data-strong data-penna-text>Host</span><label class="penna-input" data-penna="field"><input placeholder="studio.local" /></label></div></div></div><figcaption>Label above, hint inside</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="docs-form" data-spacing="grouped" style="width: 200px"><div class="docs-field"><label class="penna-input" data-penna="field"><input placeholder="Host" /></label></div></div></div><figcaption>Don't use the placeholder as the label</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `.placeholder(text)` on the state | `placeholder` | Hint while empty |
| `.default_value(text)` on the state | `value` | Starting text |
| `.prefix(el)`, `.suffix(el)` | `.penna-input-prefix`, `.penna-input-suffix` | Before and after the text |
| `.emphasis(Emphasis::Outline)` | `data-penna="field.outline"` | Outline kind |
| `.auto_grow(min, max)` on the state | `field.multiline`, `data-min-rows`, `data-max-rows` | Multi-line |
| `NumberInput::new(&state)` | `.penna-number`, `data-step`, `data-min`, `data-max` | Number kind |
| `.xsmall()` … `.touch()` | `data-size` | Size; medium by default |
| `.cleanable(true)` | `.penna-input-clear` | Clear button |
| `.set_loading(true, ..)` on the state | `.penna-spinner` in the suffix | Busy |
| `.content_type(Password)` | `type="password"` | Masked text |
| `.disabled(true)` | `data-disabled`, `disabled` | Off |
| `.appearance(false)` | `data-bare` | No chrome |
| `.bordered(false)` | `data-borderless` | No edge |
| `InputEvent::PressEnter` | `keydown` | Enter pressed |
