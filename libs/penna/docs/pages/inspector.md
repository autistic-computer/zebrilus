# Inspector

A trailing (or leading) side panel that inspects the selected item in detail — host settings, pane properties, or diagnostic metadata — without leaving the workspace.

## Structure

`Inspector` renders a `340px` panel on `SurfaceRole::INSPECTOR` with an inner-edge hairline (`roles::SIDEBAR_RULE`), a truncated `Heading` title, optional muted `Small` subtitle, optional trailing badge or status chip, a compact close button, and scrollable `inset-pane` (16 px) body sections.

```figure uses=inspector,label,chip,settings
<div style="display: flex; justify-content: flex-end; width: 100%; border: 1px solid var(--penna-divider); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k)); overflow: hidden; background: var(--penna-bg)">
  <aside class="penna-inspector" data-penna="inspector" data-ground style="width: min(100%, 304px); height: auto">
    <div class="penna-inspector-header">
      <div class="penna-inspector-titles">
        <span class="penna-type" data-type="heading" data-truncate data-penna-text>ubuntu</span>
        <span class="penna-type" data-type="small" data-muted>Connected · 2 screens</span>
      </div>
      <span class="penna-chip" data-penna="chip" data-size="xsmall"><i class="penna-dot" data-tone="success"></i><span class="penna-chip-label" data-penna-text>Ready</span></span>
      <button class="penna-button" data-penna="icon-button" data-shape="compact-icon" type="button" aria-label="Close" data-tooltip="Close" data-close><i data-icon="xmark"></i></button>
    </div>
    <div class="penna-settings-stack">
      <div class="penna-settings-group"><span data-penna-text>Permissions</span></div>
      <div class="penna-settings-list" role="list">
        <div class="penna-setting-entry" role="listitem">
          <div data-penna="settings.card" data-ground>
            <div class="penna-setting-body">
              <div class="penna-setting-heading">
                <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Control pointer</span></div></div>
                <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Control pointer" type="button"></button>
              </div>
            </div>
          </div>
        </div>
        <div class="penna-setting-entry" role="listitem">
          <div data-penna="settings.card" data-ground>
            <div class="penna-setting-rule"><div data-penna="rule.list"></div></div>
            <div class="penna-setting-body">
              <div class="penna-setting-heading">
                <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Share clipboard</span></div></div>
                <button class="penna-toggle" role="switch" aria-checked="true" aria-label="Share clipboard" type="button"></button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
    <div class="penna-stack" style="gap: 4px">
      <span class="penna-type" data-type="small" data-strong data-penna-text>Screens</span>
      <span class="penna-type" data-type="small" data-muted>DP-1 24″ · HDMI-1 24″</span>
    </div>
  </aside>
</div>
```

```rust
Inspector::new("host-inspector", "ubuntu")
    .subtitle("Connected · 2 screens")
    .trailing(Chip::new("Ready").xsmall().dot(Tone::Success))
    .on_close(|_, _, cx| close_inspector(cx))
    .child(permissions_stack)
```

- **Side placement:** defaults to `Side::Right` (left hairline). Pass `.side(Side::Left)` (`data-side="left"` in HTML) to dock on the leading edge.
- **Dismissal:** in HTML, clicking `[data-close]` or pressing `Escape` inside `.penna-inspector` dispatches a bubbling `penna-close` event for the host to hide or unmount the panel.

## API

| Rust (`penna::Inspector`) | HTML (`uses=inspector`) | Role |
| --- | --- | --- |
| `Inspector::new(id, title)` | `.penna-inspector[data-penna="inspector"][data-ground]` | 340 px scrollable side panel |
| `.subtitle(...)` | `.penna-inspector-titles > .penna-type[data-muted]` | Muted secondary line |
| `.trailing(...)` | Header sibling before `[data-close]` | Status chip or badge |
| `.on_close(f)` | `button[data-close]` (`penna-close` event) | Compact close control |
| `.side(Side::Left \| Right)` | `data-side="left"` | Flip border edge |
