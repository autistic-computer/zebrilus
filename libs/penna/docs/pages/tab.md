# Tabs

Switches between open things in one place: windows, pages, tasks. To pick a view of one thing use a segmented control.

## Looks

```figure uses=tab
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="docs-show" role="tablist"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>logs</span></span></div></div><span class="docs-note">Window</span></div>
  <div class="docs-cell"><div class="docs-show" role="tablist"><div class="penna-tab" role="tab" data-penna="tab.page" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>Docs</span></span></div><div class="penna-tab" role="tab" data-penna="tab.page" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>Issues</span></span></div></div><span class="docs-note">Page</span></div>
  <div class="docs-cell"><div class="docs-show" role="tablist"><div class="penna-tab" role="tab" data-penna="tab.tray" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>build</span></span></div><div class="penna-tab" role="tab" data-penna="tab.tray" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>tests</span></span></div></div><span class="docs-note">Tray</span></div>
  <div class="docs-cell"><div class="penna-pane-header" data-penna="bar.pane"><div class="penna-tab penna-title-tab" role="tab" data-penna="button.title" data-size="small" data-still><span class="penna-tab-path"><span class="penna-text" data-penna-text>~/dev/zebrilus</span></span></div></div><span class="docs-note">Title</span></div>
</div>
```

- **Window**: windows in the title bar.
- **Page**: pages in a browser.
- **Tray**: tasks over a composer.
- **Title**: a pane's title in its header.

```rust
let state = if open { PaintState::Selected } else { PaintState::Rest };
Tab::new(("tab", ix), TabLook::Window)
    .state(state)
    .px(Space::S10)
    .child(Label::new(TextStyle::Small, title))
```

## Anatomy

```figure uses=tab
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-size="xlarge" data-state="selected" aria-selected="true" data-still><i data-icon="terminal"></i><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span><span class="penna-tab-aside"><span class="penna-text" data-penna-text>zz</span></span></div><span class="docs-note">Icon, label, aside</span></div>
</div>
```

1. **Container**: the size sets its height, the look its fill and corners.
2. **Icon**: optional, 12 beside a `small` label.
3. **Label**: `small`, short.
4. **Aside**: optional, muted: a count or the program.

Inset and content are yours; the stories use 10 at the sides. A tab is as wide as its content; give tabs one width to make them equal.

## Strip

```figure uses=tab
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><div class="penna-tab-strip" data-penna="tab.strip" role="tablist" style="--penna-around-rows: 28px"><div class="penna-tab" role="tab" data-penna="tab.window" data-size="medium" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-size="medium" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>notes</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-size="medium" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>build</span></span></div></div></div>
</div>
```

Window tabs sit in one strip: a wash with no edge, 2 inside and 2 between tabs. Its corner is the tabs' corner plus 2, so the open tab sits concentric in it.

```rust
let on = TabStrip::ground(bar, cx);
TabStrip::new("windows", TabLook::Window)
    .on(bar)
    .with_size(px(28.0))
    .children(windows.iter().map(|w| Tab::new(w.id, TabLook::Window).on(on).with_size(px(28.0))))
```

## Sizes

```figure uses=tab
<div class="docs-tokens">
  <span class="docs-head">Size</span><span class="docs-head">Height</span><span class="docs-head">Token</span><span class="docs-head">Example</span>
  <span>xsmall</span><span><code>20</code></span><span><code>control-xs</code></span><span><div class="penna-tab" role="tab" data-penna="tab.window" data-size="xsmall" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div></span>
  <span>small</span><span><code>24</code></span><span><code>control-sm</code></span><span><div class="penna-tab" role="tab" data-penna="tab.window" data-size="small" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div></span>
  <span>medium</span><span><code>28</code></span><span><code>control-md</code></span><span><div class="penna-tab" role="tab" data-penna="tab.window" data-size="medium" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div></span>
  <span>large</span><span><code>32</code></span><span><code>control-lg</code></span><span><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div></span>
  <span>xlarge</span><span><code>40</code></span><span><code>control-xl</code></span><span><div class="penna-tab" role="tab" data-penna="tab.window" data-size="xlarge" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div></span>
  <span>touch</span><span><code>44</code></span><span><code>control-touch</code></span><span><div class="penna-tab" role="tab" data-penna="tab.window" data-size="touch" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div></span>
</div>
```

Large is the default. Only the height changes. One size per bar.

## States

```figure uses=tab
<div class="docs-show" data-gap="wide" role="tablist">
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="hover" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="hover active" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Selected</span></div>
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="focused" tabindex="0"><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected focused" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Selected, focused</span></div>
  <div class="docs-cell"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="disabled" aria-disabled="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><span class="docs-note">Off</span></div>
</div>
```

- The app sets the state; a tab lights on hover and press at rest.
- Focus is a ring, never a fill. The bar owns focus and the arrow keys.
- In HTML, Left, Right, Home and End move along a tab list.

## Selection

One open tab per bar.

```figure uses=tab
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>logs</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>build</span></span></div></div><figcaption>One selected</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>main</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>logs</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-still><span class="penna-tab-label"><span class="penna-text" data-penna-text>build</span></span></div></div><figcaption>Don't light two</figcaption></figure>
</div>
```

## API

| Rust | HTML | Does |
| --- | --- | --- |
| `Tab::new(id, look)` | `.penna-tab[role="tab"][data-penna]` | Tab in a look |
| `.state(state)` | `data-state`, `aria-selected`, `aria-disabled` | State from the app |
| `.focused(true)` | `data-state="focused"` | Focus ring over any state |
| `.with_size(size)`, `.small()` … `.touch()` | `data-size` | Height; large by default |
| `.interactive(false)` | `data-still` | State only |
| `.on(ground)` | none | Paints on a ground |
| `.on_click(handler)` | `click` | Switches |
| none | `role="tablist"`, `tabindex` | Arrow, Home and End keys |
