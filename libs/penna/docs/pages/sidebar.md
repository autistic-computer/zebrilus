# Sidebar

The full-height navigation rail on the window's leading edge. A sidebar takes the `sidebar` panel surface, aligns its top strip past macOS traffic lights, and highlights the active `sidebar.row` as liquid glass.

## Sections, selection and chrome

Click any row or press `ArrowUp` / `ArrowDown` inside the rail below to move the glass selection pill. Sections separate with `sidebar_gap` (16 px) or a muted `sidebar_heading`.

```figure uses=sidebar,settings
<div class="penna-sidebar-frame" data-penna="sidebar" data-ground style="border-radius: calc(var(--penna-radius) * var(--penna-corner-grow, 1.42)); corner-shape: superellipse(var(--penna-window-k))">
  <nav class="penna-settings-nav" aria-label="Workspace">
    <div class="penna-sidebar-heading" style="padding-top: 4px"><span data-penna-text>Workspace</span></div>
    <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button"><i data-icon="square-terminal"></i><span class="penna-label" data-penna-text>Sessions</span></button>
    <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button" aria-current="page"><i data-icon="palette"></i><span class="penna-label" data-penna-text>Appearance</span></button>
    <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button"><i data-icon="bell"></i><span class="penna-label" data-penna-text>Notifications</span></button>
    <div class="penna-sidebar-gap"></div>
    <div class="penna-sidebar-heading" style="padding-top: 0"><span data-penna-text>System</span></div>
    <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button"><i data-icon="keyboard"></i><span class="penna-label" data-penna-text>Keyboard</span></button>
    <button class="penna-button penna-sidebar-row" data-penna="sidebar.row" type="button"><i data-icon="info"></i><span class="penna-label" data-penna-text>About</span></button>
  </nav>
</div>
```

```rust
sidebar_panel(
    "sidebar",
    SIDEBAR_DEFAULT_WIDTH,
    sidebar_titlebar(
        "sidebar-bar",
        chrome_button("toggle", IconName::PanelLeft, "Hide sidebar"),
        cx,
    ),
    v_flex()
        .px(Space::S8)
        .gap(Space::S2)
        .child(sidebar_heading("Workspace", cx))
        .child(sidebar_row("sessions", "Sessions", false, cx))
        .child(sidebar_row("appearance", "Appearance", true, cx)),
    cx.theme().ground(),
    true,
    cx,
)
```

## Layout and scroll fades

- **Width and insets:** default width is `SIDEBAR_DEFAULT_WIDTH` (`256px`), content inset is `Space::S8` (`8px`), and rows sit `Space::S2` (`2px`) apart so rounded selection pills breathe evenly.
- **Titlebar clearance:** `sidebar_titlebar` indents trailing chrome buttons past macOS traffic lights via `controls_leading_inset(cx)` and aligns them with the row action column (`SIDEBAR_ROW_ACTION_CENTER`).
- **Resize handle:** `sidebar_resize_handle` places an `8px` drag strip on the trailing edge; when `divider` is `false`, hovering the handle reveals `roles::SIDEBAR_RULE`.
- **Bottom fade:** `sidebar_fade(ground, cx)` blends the sidebar fill over the window ground so scrolled lists fade cleanly at the bottom edge.

## API

| Rust (`penna::blocks::sidebar`) | HTML (`uses=sidebar,settings`) | Role |
| --- | --- | --- |
| `sidebar_panel(id, width, bar, nav, ground, divider, cx)` | `[data-penna="sidebar"][data-ground]` | Full-height sidebar container |
| `sidebar_row(id, label, selected, cx)` | `.penna-button.penna-sidebar-row[data-penna="sidebar.row"]` | Navigation row (`aria-current="page"`) |
| `sidebar_heading(title, cx)` | `.penna-sidebar-heading` | Muted section heading |
| `sidebar_gap()` | `.penna-sidebar-gap` | `gap-group` (16 px) section break |
| `sidebar_titlebar(id, controls, cx)` | `.penna-strip` | Traffic-light-cleared top chrome bar |
| `sidebar_footer(id, leading, trailing, cx)` | Custom footer flex row | Pinned bottom actions bar |
| `chrome_button(id, icon, tooltip)` | `[data-penna="chrome.button"]` | 28 px square chrome icon button |
| `sidebar_resize_handle(id, divider, cx)` | — | 8 px trailing resize strip |
| `status_dot(color)` | `.penna-dot` | 6 px status dot beside a row |
