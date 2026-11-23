# Field

Frames a form control with a label above it and help under it, and shows an error in place of the help. In settings, a row's title names its control instead.

## Anatomy

```figure uses=field,input
<div class="docs-show" data-gap="wide">
  <div class="docs-cell" style="width: min(100%, 260px)"><div class="penna-field"><span class="penna-field-label" data-penna-text>Host name</span><label class="penna-input" data-penna="field"><input value="studio" /></label><span class="penna-field-help" data-penna-text>Other computers see this name.</span></div></div>
</div>
```

1. **Label**: optional, `body` at 500 in the ink.
2. **Control**: an [input](input.html), [select](select.html), [checkbox or radio group](check.html), as you built it. A desktop field never stretches its control.
3. **Help**: optional, `small`, muted.
4. **Error**: replaces the help when set, `small` in danger after a 12 `circle-x`.

The parts sit `gap-tight` 4 apart. Help and errors wrap at the field's width.

```rust
Field::new()
    .label("Host name")
    .help("Other computers see this name.")
    .child(Input::new(&host))
```

## States

```figure uses=field,input,select
<div class="docs-show" data-gap="wide" style="align-items: flex-start">
  <div class="docs-cell" style="width: min(100%, 220px)"><div class="penna-field"><span class="penna-field-label" data-penna-text>Host name</span><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Use letters, numbers and dashes.</span></div></div><span class="docs-note">Error on input</span></div>
  <div class="docs-cell" style="width: min(100%, 220px)"><div class="penna-field"><span class="penna-field-label" data-penna-text>Language</span><div class="penna-select" data-options="Rust, Go, Zig, Swift, OCaml" data-placeholder="Language" data-tone="danger"></div><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Pick a language for code blocks.</span></div></div><span class="docs-note">Error on select</span></div>
  <div class="docs-cell" style="width: min(100%, 220px)"><div class="penna-field"><span class="penna-field-label" data-penna-text>Port</span><label class="penna-input" data-penna="field" data-disabled aria-disabled="true"><input value="22" disabled /></label><span class="penna-field-help" data-penna-text>Managed by the host profile.</span></div><span class="docs-note">Disabled control</span></div>
</div>
```

- **Error**: replaces the help, after a `circle-x`, in danger ink. Paint the control with `.tone(Tone::Danger)`: only its edge turns. Show an error once the person leaves the control or sends the form, never while they type, and clear it with the edit that fixes it.
- **Disabled**: a disabled control keeps its label and help unchanged.

## Words

The label is a short noun in sentence case with no colon and no period. Help and errors are one short sentence with a period; an error says what is wrong and how to fix it.

```figure uses=field,input
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><div class="penna-field" style="width: min(100%, 220px)"><span class="penna-field-label" data-penna-text>Host name</span><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Use letters, numbers and dashes.</span></div></div></div><figcaption>Say how to fix it</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-penna="panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><div class="penna-field" style="width: min(100%, 220px)"><span class="penna-field-label" data-penna-text>Host name:</span><label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Invalid.</span></div></div></div><figcaption>Don't only say it's wrong</figcaption></figure>
</div>
```

## Placement

Fields in a form sit `gap-group` 16 apart. Two short fields can share a line `gap-related` 8 apart, each with its own label.

```figure uses=field,input
<div class="penna-stack" data-gap="group" style="width: min(100%, 340px)">
  <div class="penna-field"><span class="penna-field-label" data-penna-text>Host</span><label class="penna-input" data-penna="field"><input value="studio.local" /></label></div>
  <div class="penna-inline" data-gap="related" style="align-items: flex-start">
    <div class="penna-field" style="flex: 1; min-width: 0"><span class="penna-field-label" data-penna-text>User</span><label class="penna-input" data-penna="field"><input value="demfabris" /></label></div>
    <div class="penna-field" style="width: 96px; flex: none"><span class="penna-field-label" data-penna-text>Port</span><label class="penna-input" data-penna="field"><input value="22" /></label></div>
  </div>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Field::new()` | `.penna-field` | The frame |
| `.label(text)` | `.penna-field-label` | Names the control |
| `.child(control)`, `.children(iter)` | Child elements | What it frames |
| `.help(text)` | `.penna-field-help` | What the value is for |
| `.error(text)` | `.penna-field-error`, `<i data-icon="circle-x">` | Replaces the help |
| `.touch_large()`, `.with_size(Size::TouchLarge)` | `data-size="touch-large"` | Phone field: 56, notes `touch-small`, stretches control |

## Phone

```figure uses=field,input
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Accessories</span>
      <span class="docs-lk-mono">56 px · 44 px buttons</span>
    </div>
    <div class="penna-stack" data-gap="group">
      <div class="penna-field" data-size="touch-large"><span class="penna-field-label" data-penna-text>Address</span><label class="penna-input" data-penna="field" data-size="touch-large"><span class="penna-input-prefix"><i data-icon="server"></i></span><input placeholder="user@host" /><span class="penna-input-suffix"><button class="penna-button" data-penna="phone.button" data-size="touch" data-shape="icon" type="button" aria-label="Scan QR code"><i data-icon="qr-code"></i></button></span></label></div>
      <label class="penna-input" data-penna="field" data-size="touch-large" data-state="focused"><span class="penna-input-prefix"><i data-icon="server"></i></span><input value="fabrico@xps" aria-label="Address" /><span class="penna-input-suffix"><button class="penna-button" data-penna="phone.button" data-size="touch" data-shape="icon" type="button" aria-label="Clear"><i data-icon="circle-x"></i></button></span></label>
      <label class="penna-input" data-penna="field" data-size="touch-large"><input value="fabrico@xps" aria-label="Connect address" /><span class="penna-input-suffix"><button class="penna-button" data-penna="button" data-tone="primary" data-prominent data-size="touch" data-shape="icon" type="button" aria-label="Connect"><i data-icon="arrow-up"></i></button></span></label>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Error and off</span>
      <span class="docs-lk-mono">touch-small · 16 px icon</span>
    </div>
    <div class="penna-stack" data-gap="group">
      <div class="penna-field" data-size="touch-large"><span class="penna-field-label" data-penna-text>Address</span><label class="penna-input" data-penna="field" data-size="touch-large" data-tone="danger"><input value="fabrico@xps:99999" /></label><div class="penna-field-error"><i data-icon="circle-x"></i><span data-penna-text>Ports go up to 65535.</span></div></div>
      <label class="penna-input" data-penna="field" data-size="touch-large" data-disabled aria-disabled="true"><input placeholder="user@host" disabled aria-label="Disabled address" /></label>
    </div>
  </div>
</div>
```

- **Field**: `touch-large` 56, `touch-body`, 20 leading icon; stretches inputs and selects to full width.
- **Focused**: the highlight a selected row takes, with no edge; floating composer, find and address fields use `field.composer` glass and stay unchanged on focus.
- **Label, help, error**: `touch-small`, the label strong, the error after a 16 `circle-x`.
- **Accessories**: clear, scan and send are 44 icon buttons set 4 from the trailing edge, never a rung smaller.
