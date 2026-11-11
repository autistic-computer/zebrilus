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

## Phone

```figure uses=notice
<div class="penna-notice" data-penna="notice" data-tone="warning" style="width: 100%"><div class="penna-notice-tone" data-penna="chip" data-tone="warning"><span class="penna-notice-lead"><i data-icon="triangle-alert"></i></span><div class="penna-notice-body"><span class="penna-text penna-notice-title" data-penna-text>Not built natively yet</span><span class="penna-text penna-notice-detail" data-penna-text>The 56 rung is drawn with the HTML kit; phone code draws fields at the default size today.</span></div></div></div>
```

```figure uses=field,input
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: 370px; gap: 16px">
    <div class="penna-field" data-size="touch-large"><span class="penna-field-label" data-penna-text>Address</span><label class="penna-input" data-penna="field" data-size="touch-large"><i data-icon="server"></i><input placeholder="user@host" /></label><span class="penna-field-help" data-penna-text>Where zz runs, reached with your own ssh.</span></div>
    <label class="penna-input" data-penna="field" data-size="touch-large"><i data-icon="server"></i><input value="fabrico@xps" /><span class="penna-input-suffix"><button class="penna-button" data-penna="button.quiet" data-size="touch" data-shape="icon" aria-label="Clear"><i data-icon="circle-x"></i></button></span></label>
    <label class="penna-input" data-penna="field" data-size="touch-large"><input value="fabrico@xps" /><span class="penna-input-suffix"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch" data-shape="icon" aria-label="Connect"><i data-icon="arrow-up"></i></button></span></label>
    <span class="docs-note">Label and help; clear and send accessories, 44</span>
  </div>
  <div class="docs-cell" style="width: 370px; gap: 16px">
    <div class="penna-field" data-size="touch-large"><label class="penna-input" data-penna="field" data-size="touch-large" data-tone="danger"><input value="fabrico@xps:99999" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Ports go up to 65535.</span></div></div>
    <label class="penna-input" data-penna="field" data-size="touch-large" data-disabled aria-disabled="true"><input placeholder="user@host" disabled /></label>
    <span class="docs-note">Error, disabled</span>
  </div>
</div>
```

- **Field**: `touch-large` 56, `touch-body`, a 20 leading icon.
- **Label, help, error**: `touch-small`, the label strong, the error after a 16 `circle-x`.
- **Accessories**: clear, scan and send are 44 icon buttons inside the field, never a rung smaller.
