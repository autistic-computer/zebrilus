# design

HTML mockups that paint with zz's real theme. Use them to try three or four takes on a screen side by
side, pick one, then build that one in zz-ui once.

```bash
just design serve    # writes the tokens, then serves http://127.0.0.1:8090/kit/
just design tokens   # rewrite them while serving, after editing a theme, roles.rs or the kit's surface code
```

The tokens are build output under `kit/tokens/` and stay out of git.

## A page

```html
<link rel="stylesheet" href="../kit/zz.css">
<script src="../kit/kit.js"></script>

<div data-zz="popover">
  <div data-zz="menu-item">Split right</div>
  <div data-zz="menu-item" data-state="selected">Close pane</div>
</div>
<button data-zz="button" data-prominent>Save</button>
```

`kit.js` loads the tokens for the theme in the corner switcher (or `?theme=macos&mode=light`), rounds
corners the way zz-gpui does, and adds the glass lens in Chromium. `?bare` hides the switcher, `?flat`
turns the lens off.

## What maps to what

| HTML | zz-ui |
|---|---|
| `data-zz="sidebar.row"` | `.surface(roles::SIDEBAR_ROW, cx)`, any id from `roles.rs` or `SurfaceRole::KIT` |
| `:hover`, `:active`, `:focus-visible` or `data-state="hover"` | `PaintState::Hover`, `Active`, `Focused` |
| `data-state="selected"`, `"current"`, `"disabled"` | `PaintState::Selected`, `Current`, `Disabled` |
| `data-tone="danger"`, `data-prominent` | `Surface::new(role).tone(Tone::Danger).prominent(true)` (kit buttons and labels) |
| `data-ground` on a container, kit roles inside it | `Surface::new(role).on(theme.ground_of(container))` |
| `color: var(--zz-muted)` | `muted_on(&layer, cx)` |
| `display: flex; gap: 8px; padding: 0 12px` | `h_flex().gap(px(8.)).px(px(12.))` |

Colors, edges, shadows, radius and smoothing come out of `Theme::layer` for every role, state, tone and
theme, through `crates/zz-ui/src/theme_css.rs`. Mockups name roles, not colors, same rule as zz-ui.

## How close it is

- Exact: fills, ink, edges, shadows, radius, per theme, mode and state.
- Close: squircle corners need Chrome 139+ (`corner-shape`); other browsers draw round ones.
- Glass: the blur, tint and color boost match; the rim (glint, contour, fresnel) is box-shadows. Edge
  refraction is an SVG lens that only Chromium draws.
- Left out: chroma presets, tint, contrast, accent and zoom settings, and user theme files.
- Fonts: `system-ui`, which is SF on macOS like desktop zz. Thin clients use Inter.

## Mockups

Put each question in its own folder under `mockups/`, one page with the variants next to each other;
`mockups/sidebar-buttons` is an example. Mockups change layout and structure. To try a different look
for a role (a tint instead of an accent, another radius), edit its theme file in
`crates/zz-ui/themes/` and rerun `just design tokens`.
