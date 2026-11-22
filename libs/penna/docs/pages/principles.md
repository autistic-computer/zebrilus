# Principles

Six rules every penna screen follows. Where the spec is silent, they decide.

## Keyboard first

Every action has a keyboard path, and the interface shows it: shortcuts sit beside menu rows, and focus is a ring outside the edge that appears only from the keyboard and never looks like selection.

```figure uses=menu,kbd
<div class="docs-pr-kbd" data-docs-principles data-principles-menu>
  <div class="penna-popup docs-pr-menu" data-penna="popover">
    <div class="penna-popup-items" role="menu" tabindex="0" aria-label="Pane commands">
      <div class="penna-popup-row" data-penna="menu-item" role="menuitem" data-action="New tab" data-shortcut="cmd-t">
        <span class="penna-popup-icon"><i data-icon="plus"></i></span>
        <span class="penna-popup-main"><span class="penna-label" data-penna-text>New tab</span><span class="penna-label penna-popup-key" data-penna-text data-kbd-text="cmd-t">⌘T</span></span>
      </div>
      <div class="penna-popup-row" data-penna="menu-item" role="menuitem" data-selected data-state="selected" data-action="Split right" data-shortcut="cmd-d">
        <span class="penna-popup-icon"><i data-icon="panel-right"></i></span>
        <span class="penna-popup-main"><span class="penna-label" data-penna-text>Split right</span><span class="penna-label penna-popup-key" data-penna-text data-kbd-text="cmd-d">⌘D</span></span>
      </div>
      <div class="penna-popup-row" data-penna="menu-item" role="menuitem" data-action="Split down" data-shortcut="cmd-shift-d">
        <span class="penna-popup-icon"><i data-icon="panel-bottom"></i></span>
        <span class="penna-popup-main"><span class="penna-label" data-penna-text>Split down</span><span class="penna-label penna-popup-key" data-penna-text data-kbd-text="cmd-shift-d">⇧⌘D</span></span>
      </div>
      <div class="penna-popup-separator" role="separator"></div>
      <div class="penna-popup-row" data-penna="menu-item" role="menuitem" data-action="Close pane" data-shortcut="cmd-w">
        <span class="penna-popup-icon"><i data-icon="xmark"></i></span>
        <span class="penna-popup-main"><span class="penna-label" data-penna-text>Close pane</span><span class="penna-label penna-popup-key" data-penna-text data-kbd-text="cmd-w">⌘W</span></span>
      </div>
    </div>
  </div>
  <div class="docs-pr-kbd-aside">
    <div class="docs-pr-kbd-readout">
      <span data-principles-menu-status data-penna-text>Focus the menu and step with arrow keys.</span>
      <kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="cmd-d" data-principles-menu-key hidden></kbd>
    </div>
    <div class="docs-pr-kbd-hints">
      <span class="docs-pr-kbd-hint"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="up"></kbd><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="down"></kbd><span>Move</span></span>
      <span class="docs-pr-kbd-hint"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="enter"></kbd><span>Run</span></span>
      <span class="docs-pr-kbd-hint"><kbd class="penna-kbd" data-penna="keycap" data-size="small" data-kbd="escape"></kbd><span>Clear</span></span>
    </div>
  </div>
</div>
```

```figure uses=switch,tab
<div class="docs-pr-focus">
  <div class="docs-cell">
    <div class="docs-pr-slot"><button class="penna-button docs-pr-static" data-penna="button" data-state="focused" tabindex="-1"><span class="penna-label" data-penna-text>Save</span></button></div>
    <span class="docs-note">Button · focused</span>
  </div>
  <div class="docs-cell">
    <div class="docs-pr-slot"><button class="penna-toggle docs-pr-static" data-state="focused" aria-checked="false" aria-label="Focused switch" tabindex="-1"></button></div>
    <span class="docs-note">Switch · focused</span>
  </div>
  <div class="docs-pr-focus-pair" role="tablist">
    <div class="docs-cell">
      <div class="docs-pr-slot"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="selected" aria-selected="true" data-still><i data-icon="terminal"></i><span class="penna-tab-label"><span class="penna-text" data-penna-text>Selected</span></span></div></div>
      <span class="docs-note">Selected · wash</span>
    </div>
    <div class="docs-cell">
      <div class="docs-pr-slot"><div class="penna-tab" role="tab" data-penna="tab.window" data-state="focused" data-still><i data-icon="terminal"></i><span class="penna-tab-label"><span class="penna-text" data-penna-text>Focused</span></span></div></div>
      <span class="docs-note">Focused · ring</span>
    </div>
  </div>
</div>
```

## Calm and dense

Most text on a screen is one size, `body`; weight sets the hierarchy and color is kept for meaning. Spacing follows relationships, so dense screens group cleanly without padding them out.

```figure uses=badge,label,kbd
<div class="penna-settings docs-pr-calm">
  <div>
    <h2>Session</h2>
    <div data-penna="settings.card" data-ground>
      <div class="penna-setting">
        <div class="penna-copy">Host<small>studio.local · 192.168.1.14</small></div>
        <span class="penna-badge" data-penna="badge" data-tone="success"><i data-icon="circle-check"></i><span class="badge-text" data-penna-text>Connected</span></span>
      </div>
      <div class="penna-setting">
        <div class="penna-copy">Transport<small>Encrypted peer channel on port 4433.</small></div>
        <span class="penna-type penna-mono docs-pr-calm-val" data-type="small" data-penna-text>QUIC · 12 ms</span>
      </div>
      <div class="penna-setting">
        <div class="penna-copy">Next pane<small>Move focus to the pane on the right.</small></div>
        <span class="penna-inline" data-gap="tight"><kbd class="penna-kbd" data-penna="keycap" data-kbd="ctrl-b"></kbd><kbd class="penna-kbd" data-penna="keycap" data-kbd="right"></kbd></span>
      </div>
    </div>
  </div>
</div>
```

## One highlight

Selection, focus and the main action stay in the ink itself, whatever the tint. The accent only colors details: an on switch, a slider fill, a checked box, a link. At most one main action per view.

```figure uses=list,switch,slider,check
<div class="docs-pr-split">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Ink</span>
      <span class="docs-pr-col-meta">Selection, focus, primary</span>
    </div>
    <div class="docs-pr-card" data-penna="panel" data-ground>
      <div class="docs-pr-list" role="listbox" aria-label="Panes" data-principles-list>
        <div class="penna-list-item" data-penna="menu-item" role="option" tabindex="0" data-state="selected" aria-selected="true" data-principles-row>
          <span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>work</span></span></span>
        </div>
        <div class="penna-list-item" data-penna="menu-item" role="option" tabindex="0" aria-selected="false" data-principles-row>
          <span class="penna-list-content"><i class="penna-list-icon" data-icon="square-terminal"></i><span class="penna-list-label"><span class="penna-text" data-penna-text>logs</span></span></span>
        </div>
      </div>
      <div class="docs-pr-btn-row">
        <button class="penna-button docs-pr-static" data-penna="button" data-state="focused" tabindex="-1"><span class="penna-label" data-penna-text>Cancel</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Add host</span></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Accent</span>
      <span class="docs-pr-col-meta">Details only</span>
    </div>
    <div class="docs-pr-card" data-penna="panel" data-ground>
      <div class="docs-pr-rows">
        <div class="docs-pr-detail-row">
          <span>Auto-reconnect</span>
          <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Auto-reconnect" type="button"></button>
        </div>
        <div class="docs-pr-detail-row">
          <span>Opacity</span>
          <div class="penna-slider" data-size="small" data-min="0" data-max="100" data-value="68" aria-label="Opacity">
            <div class="penna-rail"><div class="penna-fill"></div><div class="penna-knob"></div></div>
          </div>
        </div>
        <div class="docs-pr-detail-row">
          <label class="penna-checkbox" aria-checked="true"><span class="penna-check"></span><span class="penna-label" data-penna-text>Restore panes</span></label>
          <a class="docs-pr-link" href="color.html">Color tokens</a>
        </div>
      </div>
    </div>
  </div>
</div>
```

```figure
<div class="docs-dodont">
  <figure data-verdict="do"><div class="docs-dodont-panel" data-ground><i class="docs-mark" data-icon="circle-check"></i><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Add host</span></button></div><figcaption>One main action, in the ink</figcaption></figure>
  <figure data-verdict="dont"><div class="docs-dodont-panel" data-ground><i class="docs-mark" data-icon="circle-x"></i><button class="penna-button" data-penna="button" data-tone="danger" type="button"><span class="penna-label" data-penna-text>Cancel</span></button><button class="penna-button" data-penna="button" data-tone="success" data-prominent type="button"><span class="penna-label" data-penna-text>Add host</span></button></div><figcaption>Don't color actions for decoration</figcaption></figure>
</div>
```

## Content first

Bars, headers and toolbars use quiet controls with no fill at rest and muted labels, so chrome recedes and the work leads.

```figure
<div class="docs-dodont">
  <figure data-verdict="do">
    <div class="docs-dodont-panel docs-pr-pane-host" data-ground>
      <i class="docs-mark" data-icon="circle-check"></i>
      <div class="docs-pr-pane">
        <div class="docs-pr-pane-bar">
          <span class="docs-pr-pane-title">main.rs</span>
          <div class="docs-pr-pane-tools">
            <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Split"><i data-icon="panel-right"></i></button>
            <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="Search"><i data-icon="search"></i></button>
            <button class="penna-button" data-penna="button.quiet" data-shape="icon" data-size="small" type="button" aria-label="More"><i data-icon="ellipsis"></i></button>
          </div>
        </div>
        <div class="docs-pr-pane-body">
          <span><b>fn</b> main() {</span>
          <span>&nbsp;&nbsp;<i>let</i> pane = zz::open(<em>"work"</em>);</span>
          <span>&nbsp;&nbsp;pane.focus();</span>
        </div>
      </div>
    </div>
    <figcaption>Quiet controls let the work lead</figcaption>
  </figure>
  <figure data-verdict="dont">
    <div class="docs-dodont-panel docs-pr-pane-host" data-ground>
      <i class="docs-mark" data-icon="circle-x"></i>
      <div class="docs-pr-pane">
        <div class="docs-pr-pane-bar">
          <span class="docs-pr-pane-title" data-loud>main.rs</span>
          <div class="docs-pr-pane-tools">
            <button class="penna-button" data-penna="button" data-shape="icon" data-size="small" type="button" aria-label="Split"><i data-icon="panel-right"></i></button>
            <button class="penna-button" data-penna="button" data-shape="icon" data-size="small" type="button" aria-label="Search"><i data-icon="search"></i></button>
            <button class="penna-button" data-penna="button" data-shape="icon" data-size="small" type="button" aria-label="More"><i data-icon="ellipsis"></i></button>
          </div>
        </div>
        <div class="docs-pr-pane-body">
          <span><b>fn</b> main() {</span>
          <span>&nbsp;&nbsp;<i>let</i> pane = zz::open(<em>"work"</em>);</span>
          <span>&nbsp;&nbsp;pane.focus();</span>
        </div>
      </div>
    </div>
    <figcaption>Don't fill every control in chrome</figcaption>
  </figure>
</div>
```

## Every state shows

A control always tells what it is doing: rest, hover, pressed, focused, selected, disabled, loading or in error. Feedback lands in the same frame as input; motion is short (120–220 ms) and stops for reduced motion.

```figure uses=spinner,field,input,badge
<div class="docs-pr-states-wrap">
  <div class="docs-pr-states">
    <div class="docs-cell"><button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Rest</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover" type="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Hover</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="hover active" type="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Pressed</span></div>
    <div class="docs-cell"><button class="penna-button docs-pr-static" data-penna="button" data-state="focused" tabindex="-1" type="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Focused</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-state="current" type="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Selected</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" disabled type="button"><span class="penna-label" data-penna-text>Save</span></button><span class="docs-note">Disabled</span></div>
    <div class="docs-cell"><button class="penna-button" data-penna="button" data-loading type="button"><span class="penna-spinner"><i data-icon="loader"></i></span><span class="penna-label" data-penna-text>Saving</span></button><span class="docs-note">Loading</span></div>
  </div>
  <div class="docs-pr-state-extra">
    <div class="docs-pr-col">
      <div class="docs-pr-col-head">
        <span class="docs-pr-col-title">Error</span>
        <span class="docs-pr-col-meta">Clears when fixed</span>
      </div>
      <div class="docs-pr-card" data-penna="panel" data-ground>
        <div class="penna-field" data-principles-field>
          <span class="penna-field-label" data-penna-text>Host name</span>
          <label class="penna-input" data-penna="field" data-tone="danger"><input value="studio mac" aria-label="Host name" /></label>
          <div class="penna-field-error" data-principles-field-note><i data-icon="circle-x"></i><span data-penna-text>Use letters, numbers and dashes.</span></div>
        </div>
      </div>
    </div>
    <div class="docs-pr-col" data-principles-save>
      <div class="docs-pr-col-head">
        <span class="docs-pr-col-title">Live feedback</span>
        <span class="penna-badge" data-penna="badge" data-principles-save-badge><span class="badge-text" data-penna-text>Rest</span></span>
      </div>
      <div class="docs-pr-card" data-penna="panel" data-ground>
        <p class="docs-pr-live-copy">Press Save for same-frame feedback and short motion.</p>
        <div class="docs-pr-live-body">
          <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Save</span></button>
        </div>
      </div>
    </div>
  </div>
</div>
```

## One kit

The same components draw native `ala` windows, the phone and this site, in one look, light and dark. Knobs in [Look](look.html) tune color, corners, glass and faces — never a component's fills, sizes or behavior.

```figure uses=field,input,switch
<div class="docs-pr-kit">
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Desktop</span>
      <span class="docs-pr-col-meta">28 px controls</span>
    </div>
    <div class="docs-pr-card" data-penna="panel" data-ground>
      <div class="penna-field">
        <span class="penna-field-label" data-penna-text>Host</span>
        <label class="penna-input" data-penna="field"><input value="studio.local" aria-label="Desktop host" /></label>
      </div>
      <div class="docs-pr-detail-row">
        <span>Auto-reconnect</span>
        <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Desktop auto-reconnect" type="button"></button>
      </div>
      <div class="docs-pr-btn-row">
        <button class="penna-button" data-penna="button" type="button"><span class="penna-label" data-penna-text>Cancel</span></button>
        <button class="penna-button" data-penna="button" data-tone="primary" data-prominent type="button"><span class="penna-label" data-penna-text>Connect</span></button>
      </div>
    </div>
  </div>
  <div class="docs-pr-col">
    <div class="docs-pr-col-head">
      <span class="docs-pr-col-title">Phone</span>
      <span class="docs-pr-col-meta">56 px touch targets</span>
    </div>
    <div class="docs-pr-card docs-pr-phone-card" data-penna="panel" data-ground>
      <div class="penna-field" data-size="touch-large">
        <span class="penna-field-label" data-penna-text>Host</span>
        <label class="penna-input" data-penna="field" data-size="touch-large"><input value="studio.local" aria-label="Phone host" /></label>
      </div>
      <div class="docs-pr-detail-row" data-touch>
        <span class="docs-pr-touch-label">Auto-reconnect</span>
        <button class="penna-toggle" data-size="touch" role="switch" aria-checked="true" aria-label="Phone auto-reconnect" type="button"></button>
      </div>
      <button class="penna-button" data-penna="button" data-size="touch-large" data-tone="primary" data-prominent type="button" style="width: 100%"><span class="penna-label" data-penna-text>Connect</span></button>
    </div>
  </div>
</div>
```
