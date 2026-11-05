# Principles

Six rules every penna screen follows. Where the spec is silent, they decide.

## Keyboard first

Every action has a key, and the interface shows it.

```figure uses=menu
<div class="penna-popup" data-penna="popover" style="width: 220px"><div class="penna-popup-items" role="menu"><div class="penna-popup-row" data-penna="menu-item" role="menuitem"><span class="penna-popup-icon"><i data-icon="plus"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab</span><span class="penna-label penna-popup-key" data-penna-text>⌘T</span></span></div><div class="penna-popup-row" data-penna="menu-item" role="menuitem"><span class="penna-popup-icon"><i data-icon="panel-right"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span><span class="penna-label penna-popup-key" data-penna-text>⌘D</span></span></div><div class="penna-popup-row" data-penna="menu-item" role="menuitem"><span class="penna-popup-icon"><i data-icon="xmark"></i></span><span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span><span class="penna-label penna-popup-key" data-penna-text>⌘W</span></span></div></div></div>
```

Focus is a ring outside the edge, only from the keyboard, and never looks like selection.

```figure uses=switch,tab
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="focused"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Button</span></div>
  <div class="docs-cell"><button class="penna-toggle" data-state="focused" aria-checked="false" aria-label="Focused"></button><span class="docs-note">Switch</span></div>
  <div class="docs-cell"><div class="docs-show" role="tablist"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><i data-icon="terminal"></i><span class="penna-tab-label"><span class="penna-text" data-penna-text>Selected</span></span></div><div class="penna-tab" role="tab" data-penna="tab.window" data-state="focused" tabindex="0"><i data-icon="terminal"></i><span class="penna-tab-label"><span class="penna-text" data-penna-text>Focused</span></span></div></div><span class="docs-note">Tabs</span></div>
</div>
```

## Calm and dense

One text size does most of the work. Weight sets the hierarchy; color is kept for meaning.

```figure uses=label,kbd
<div class="penna-settings" style="padding: 0; width: 420px"><div><div data-penna="settings.card"><div class="penna-setting"><div class="penna-copy">Next pane<small>Move to the pane on the right.</small></div><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></span></div><div class="penna-setting"><div class="penna-copy">Previous pane<small>Move to the pane on the left.</small></div><span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="left"></kbd></span></div></div></div></div>
```

## One highlight

One color marks on, picked and focused. One main action per view.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Add host</span></button></div><figcaption>One main action, in the highlight</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-tone="primary" data-prominent><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="success"><span class="penna-label" data-penna-text>Add host</span></button></div><figcaption>Don't color actions for decoration</figcaption></figure>
</div>
```

## Content first

Chrome stays quiet so the work stands out.

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button.quiet" aria-label="Split"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button.quiet" aria-label="Search"><i data-icon="search"></i></button><button class="penna-button" data-penna="button.quiet" aria-label="More"><i data-icon="ellipsis"></i></button></div><figcaption>Ghost buttons in bars</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel"><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" aria-label="Split"><i data-icon="panel-right"></i></button><button class="penna-button" data-penna="button" aria-label="Search"><i data-icon="search"></i></button><button class="penna-button" data-penna="button" aria-label="More"><i data-icon="ellipsis"></i></button></div><figcaption>Don't fill every control in chrome</figcaption></figure>
</div>
```

## Every state shows

Each state has its own look. Motion is short and stops for reduced motion.

```figure uses=spinner
<div class="docs-show" data-gap="wide">
  <div class="docs-cell"><button class="penna-button" data-penna="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Rest</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Hover</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover active"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Pressed</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="focused"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Focused</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" disabled><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Off</span></div>
  <div class="docs-cell"><button class="penna-button" data-penna="button" data-loading><span class="penna-spinner" data-size="small"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Busy</span></div>
</div>
```

## One kit

The same components draw native windows, the phone and this site, in every look. A look changes materials, never sizes or behavior.
