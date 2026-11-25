# Title bar

The window's top chrome strip: carries the drag region, platform window controls, pane title or tabs, and trailing icon actions.

## Bare and populated bars

`TitleBar` automatically sizes to `theme.title_bar_height()` (`40px` flush, `34px + 2 × pane_margin` when panes have gaps), clears macOS traffic lights on the leading edge, and attaches `WindowControls` on Windows and client-decorated Linux.

```figure uses=title_bar
<div class="docs-show" style="flex-direction: column; align-items: stretch; width: 100%; gap: 12px">
  <div class="penna-title-bar-frame" style="width: 100%; border: 1px solid var(--penna-divider); border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k)); overflow: hidden">
    <div class="penna-title-bar" data-penna="titlebar" role="banner" style="border-bottom: 0">
      <div class="penna-title-bar-drag" style="min-width: 0">
        <div class="penna-title-bar-content" style="min-width: 0; gap: 8px">
          <div class="penna-title-bar-group" style="min-width: 0; overflow: hidden">
            <i data-icon="square-terminal"></i>
            <span class="penna-title-bar-text penna-title-bar-title" data-penna-text>work</span>
            <span class="penna-title-bar-text penna-title-bar-muted" style="overflow: hidden; text-overflow: ellipsis">~/dev/penna</span>
          </div>
          <div class="penna-title-bar-actions">
            <button class="penna-button" data-penna="icon-button" data-shape="icon" type="button" aria-label="Split right" data-tooltip="Split right"><i data-icon="panel-right"></i></button>
            <button class="penna-button" data-penna="icon-button" data-shape="icon" type="button" aria-label="Settings" data-tooltip="Settings"><i data-icon="settings"></i></button>
          </div>
        </div>
      </div>
      <div class="penna-title-bar-controls"></div>
    </div>
  </div>
</div>
```

```rust
TitleBar::new().child(
    h_flex()
        .flex_1()
        .items_center()
        .justify_between()
        .child(
            h_flex()
                .items_center()
                .gap(Space::S6)
                .child(Icon::new(IconName::SquareTerminal))
                .child(Label::new(TextStyle::Body, "work").strong())
                .child(Label::new(TextStyle::Body, "~/dev/penna").muted()),
        )
        .child(
            h_flex()
                .items_center()
                .gap(Space::S2)
                .child(Button::icon("split", IconName::PanelRight).tooltip("Split right"))
                .child(Button::icon("prefs", IconName::Settings).tooltip("Settings")),
        ),
)
```

## Platform rules

- **macOS:** reserves `macos_traffic_light_clearance(cx)` on the left and positions traffic lights via `TitleBar::title_bar_options(height)` + `TitleBar::traffic_light_position(height, zoom)`. In fullscreen the inset collapses back to `12px`.
- **Windows and Linux CSD:** `WindowControls` draws minimize, maximize/restore, and close (`Tone::Danger` on hover/press) at `24px` square with `8px` gaps (`WINDOW_CONTROLS_SPAN = 88px`).
- **Window gestures:** double-clicking the drag region zooms the window; right-clicking on client-decorated Linux opens the system window menu.

## API

| Rust (`penna::TitleBar`, `blocks::title_bar`) | HTML (`uses=title_bar`) | Role |
| --- | --- | --- |
| `TitleBar::new()` | `.penna-title-bar[data-penna="titlebar"][role="banner"]` | Window title bar strip |
| `TitleBar::title_bar_options(height)` | — | `ala` window creation options |
| `macos_traffic_light_clearance(cx)` | — | Leading clearance on macOS |
| `WindowControls::new()` | `.penna-title-bar-controls` | Platform min/max/close buttons (native only) |
| `.on_close_window(f)` | — | Custom close hook on Linux CSD |
| `draws_window_controls(window)` | — | True when client-side buttons are painted |
