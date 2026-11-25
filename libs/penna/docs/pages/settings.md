# Settings

Grouped preference cards, keyboard-navigable settings sidebars, folding large titles, and standard About sections. Settings pages place rows inside `settings.card` stacks `gap-group` (16 px) apart inside a `760px` max-width pane.

## Stacked setting entries

`SettingsStack` glues adjacent `SettingEntry` cards together with `rule.list` hairlines so a group reads as one card. Disabled entries dim copy and disable their trailing control.

```figure uses=settings,select,icon
<div class="penna-settings-stack" style="width: 100%; max-width: 560px">
  <div class="penna-settings-group"><span data-penna-text>Startup</span></div>
  <div class="penna-settings-list" role="list">
    <div class="penna-setting-entry" role="listitem">
      <div data-penna="settings.card" data-ground>
        <div class="penna-setting-body">
          <div class="penna-setting-heading">
            <div class="penna-setting-copy">
              <div class="penna-setting-title"><span data-penna-text>Open at login</span></div>
              <div class="penna-setting-description"><span data-penna-text>Start automatically when you sign in</span></div>
            </div>
            <button class="penna-toggle" aria-checked="true" aria-label="Open at login" type="button"></button>
          </div>
        </div>
      </div>
    </div>
    <div class="penna-setting-entry" role="listitem">
      <div data-penna="settings.card" data-ground>
        <div class="penna-setting-rule"><div data-penna="rule.list"></div></div>
        <div class="penna-setting-body">
          <div class="penna-setting-heading">
            <div class="penna-setting-copy">
              <div class="penna-setting-title"><span data-penna-text>Start with</span></div>
            </div>
            <div class="penna-select" data-options="Last session, New window, Empty workspace" data-value="0" aria-label="Start with"></div>
          </div>
        </div>
      </div>
    </div>
    <div class="penna-setting-entry" role="listitem" aria-disabled="true">
      <div data-penna="settings.card" data-ground>
        <div class="penna-setting-rule"><div data-penna="rule.list"></div></div>
        <div class="penna-setting-body">
          <div class="penna-setting-heading">
            <div class="penna-setting-copy">
              <div class="penna-setting-title"><span data-penna-text>Sync settings</span></div>
              <div class="penna-setting-description"><span data-penna-text>Requires a linked machine</span></div>
            </div>
            <button class="penna-toggle" aria-checked="false" aria-label="Sync settings" type="button" disabled></button>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

```rust
SettingsStack::titled("Startup")
    .child(
        SettingEntry::new("Open at login")
            .description("Start automatically when you sign in")
            .control(Switch::new("login").checked(true)),
    )
    .child(
        SettingEntry::new("Start with")
            .control(settings_select_box(cx).child(Select::new(&start_with))),
    )
```

## About section

`blocks::settings::about` provides `about_hero`, `about_build_stack`, and `about_project_stack` so every app's About pane shares the same version badge, copyable build details, and external links.

```figure uses=settings
<div style="display: flex; flex-direction: column; gap: 16px; width: 100%; max-width: 560px">
  <div class="penna-about-hero">
    <i data-icon="square-terminal" style="--penna-icon-size: 36px"></i>
    <div class="penna-about-copy">
      <div class="penna-about-name"><span data-penna-text>zz</span></div>
      <div class="penna-about-tagline"><span data-penna-text>Native terminal and agent workspace</span></div>
      <span class="penna-badge" data-penna="badge.outline"><span data-penna-text>0.4.0</span></span>
    </div>
  </div>
  <div class="penna-settings-stack">
    <div class="penna-settings-group"><span data-penna-text>Build</span></div>
    <div class="penna-settings-list" role="list">
      <div class="penna-setting-entry" role="listitem">
        <div data-penna="settings.card" data-ground>
          <div class="penna-setting-body">
            <div class="penna-setting-heading">
              <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>Version</span></div></div>
              <span class="penna-settings-value" data-penna-text>0.4.0 (arm64)</span>
            </div>
          </div>
        </div>
      </div>
      <div class="penna-setting-entry" role="listitem">
        <div data-penna="settings.card" data-ground>
          <div class="penna-setting-rule"><div data-penna="rule.list"></div></div>
          <div class="penna-setting-body">
            <div class="penna-setting-heading">
              <div class="penna-setting-copy"><div class="penna-setting-title"><span data-penna-text>License</span></div></div>
              <span class="penna-settings-value" data-penna-text>MIT</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>
```

## Scroll columns, disclosures and phone titles

- **Scroll columns:** `settings_scroll_column` and `settings_virtual_column` fade top and bottom scroll edges into the window ground and pair with `SettingsFold` + `settings_title_bar` to fold a `LARGE_TITLE_HEIGHT` (`52px`) header into the title bar as the user scrolls.
- **Navigation:** `settings_navigation` builds a grouped sidebar with `ArrowUp`/`ArrowDown` keyboard movement and a back button on narrow/phone layouts.
- **Coarse pointers:** on touch devices, wide controls automatically drop onto their own trailing row below the title whenever the title's widest word and control cannot sit side by side.

## API

| Rust (`penna::blocks::settings`) | HTML (`uses=settings`) | Role |
| --- | --- | --- |
| `SettingsStack::titled(title)` | `.penna-settings-stack` + `.penna-settings-list[role="list"]` | Grouped settings card stack |
| `SettingEntry::new(title)` | `.penna-setting-entry > [data-penna="settings.card"][data-ground]` | Single preference row |
| `.description(...)` / `.control(...)` | `.penna-setting-description`, trailing control | Secondary copy and widget slot |
| `settings_navigation(id, sections, sel)` | `nav.penna-settings-nav` | Grouped sidebar navigation |
| `settings_list_disclosure_header(...)` | — | Collapsible group header (native only) |
| `settings_scroll_column` / `SettingsFold` | — | Edge-faded column + folding large title |
| `about::{about_hero, about_build_stack}` | `.penna-about-hero`, `.penna-settings-value` | Standard About hero and metadata |
