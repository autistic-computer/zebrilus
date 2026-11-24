---
type: Configuration
title: UI design conventions
description: Which penna components and theme values zz uses for chrome, pane content, typography and input.
resource: crates/zz/src/theme.rs
tags: [ui, ala, penna, theme, chrome]
timestamp: 2026-11-24T00:00:00Z
---

## Components and surfaces

zz builds application UI in `crates/zz-ui` with [penna](/references/penna.md).
Penna defines the components, looks and surface roles. Reuse a component's
typed look for a button, row, field, tile, panel, chip or rule. If a role is
missing from the kit, add it there so it works across apps. The contracts are
in `libs/penna/knowledge/designs/kit.md` and `libs/penna/knowledge/designs/foundations.md`.

| Need | Use |
|---|---|
| Background, edge, corner, shadow or selection | The component's look, state, tone and ground. Use a `RuleLook` for a divider. |
| Text on a colored surface | A semantic label or text style. Derive muted and selected text from the surface it sits on. |
| A pane, float or PiP painted by zz | A penna `ContentLook`. `crates/zz-ui/src/pane_look.rs` applies active and inactive pane material, edges and fill. |
| Console cells or tmux-configured content | The renderer's palette and user settings. [Terminal appearance](/terminal/appearance.md) |
| Icons and app marks | The kit's theme-aware `IconName` or a zz-owned asset. [Icon set](/configuration/icon-set.md) |
| Notifications | Send through the workspace's Root toast host. A keyed notification can update an existing toast in place. |

`clippy.toml` disallows raw color construction and direct shape, border and
shadow helpers in application chrome. A renderer or protocol surface needing
an exception scopes it to the function and records the reason.
[Design system](/designs/design-system.md) · [Surface roles](/designs/surface-roles.md)

## Input and spacing

Use the shared control and input components for keyboard focus, selection, IME,
hit areas and accessibility behavior. Apply hover and press to the interactive
element with an ID. Color its label and icon from that same state. A click
target uses `click_cursor`, which follows the pointer preference. Text editing,
resizing and dragging retain their functional cursors.

Use the kit's size and spacing scales. The standard heights are 20, 24, 28,
32 and 40 logical pixels, with touch at 44 and large touch at 56. A component's
rung also selects its label, icon and insets. Keep a row's label, icons and hints
aligned to the same text line. Penna's icon alignment measures the font's cap
height. Keep related controls closer together than separate groups.

Use the chat card layout for questions, permissions and sign-in. Its shared
12-pixel gutter aligns headings and content. Keep each option a single row with
one hit target. Use the palette and picker blocks for selectable lists and key
hints. Use the shared Root for overlays so menus, dialogs and toast notifications
have the correct drawing order. See `libs/penna/src/foundation/styled.rs`,
`libs/penna/src/blocks/chat/card.rs` and `libs/penna/src/foundation/overlay/toast.rs`.

## Theme and terminal colors

`config::shown_theme` starts from the platform look with the user's overrides.
The appearance model resolves mode, accent, tint, corner radius, control shape
and glass. With `chrome-terminal-colors` enabled, a terminal look whose mode
matches the requested chrome mode can supply window colors. An explicit accent
still takes precedence. Terminal cells retain the terminal renderer's own palette.
See `crates/zz/src/config/mod.rs` and [app config](/configuration/app-config.md).

`apply_zz_overrides` in `crates/zz/src/theme.rs` sets fonts, contrast, glass,
window opacity, pane margin and selected-pane contrast. Pane frames and the
workspace read the same theme in previews and live panels. On thin clients the
appearance settings are device preferences. The storybook exposes both the
kit and zz components for checking the chosen look. [Storybook](/playbooks/storybook.md)

## Typography and zoom

Plumula Sans is the default UI font and Plumula Serif is the default chat face.
`ui-font-family`, `ui-chat-font-family` and `ui-mono-font-family` override
them. Desktop monospace falls back to the terminal's primary font and then
the platform mono face. Bundled fonts live under `crates/zz-ui/assets/fonts`
and are registered through `crates/zz-ui/src/fonts.rs`.

Desktop UI zoom applies to the whole window through `Window::set_zoom`. The
range is 50–300% in 10-point shortcut steps, reset at 100%. New windows inherit
the current value. Use logical dimensions so layout and hit testing agree as
zoom changes. Thin clients combine their stored zoom with the applicable system
text scale in `interface_zoom` in `clients/app/src/app/settings.rs`. See
`crates/zz/src/ui_scale.rs`.
