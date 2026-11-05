# Field

Frames a form control with a label above it and help under it, and shows an error in place of the help. In settings, a row's title names its control instead.

## Anatomy

```figure uses=field,input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: 260px"><div class="penna-field"><span class="penna-field-label" data-penna-text>Host name</span><label class="penna-input" data-penna="field"><input value="studio" /></label><span class="penna-field-help" data-penna-text>Other computers see this name.</span></div></div>
</div>
```

1. **Label**: `body` at 500, in the ink.
2. **Control**: an input, select, checkbox or radio group, as you built it.
3. **Help**: `small`, muted. Optional.

The parts sit `gap-tight` 4 apart. Help wraps at the field's width.

```rust
Field::new()
    .label("Host name")
    .help("Other computers see this name.")
    .child(Input::new(&host))
```

## Error

```figure uses=field,input,select
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 260px"><div class="penna-field"><span class="penna-field-label" data-penna-text>Host name</span><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Use letters, numbers and dashes.</span></div></div></div>
  <div class="docs-cell" style="width: 260px"><div class="penna-field"><span class="penna-field-label" data-penna-text>Language</span><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-placeholder="Language" data-tone="danger"></div><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Pick a language for code blocks.</span></div></div></div>
</div>
```

The error replaces the help, after a `circle-x`, in the danger color. Paint the control with `.tone(Tone::Danger)`: only its edge turns. Show it once the person leaves the control or sends the form, never while they type, and clear it with the edit that fixes it.

## Words

The label is a short noun in sentence case, no colon. An error says what is wrong and how to fix it.

```figure uses=field,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-field" style="width: 220px"><span class="penna-field-label" data-penna-text>Host name</span><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Use letters, numbers and dashes.</span></div></div></div><figcaption>Say how to fix it</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-field" style="width: 220px"><span class="penna-field-label" data-penna-text>Host name:</span><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Invalid.</span></div></div></div><figcaption>Don't only say it's wrong</figcaption></figure>
</div>
```

## Placement

Fields in a form sit `gap-group` 16 apart. Two short fields can share a line, `gap-related` 8 apart.

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Field::new()` | `.penna-field` | The frame |
| `.label(text)` | `.penna-field-label` | Names the control |
| `.child(control)` | the control | What it frames |
| `.help(text)` | `.penna-field-help` | What the value is for |
| `.error(text)` | `.penna-field-error` | Replaces the help |
