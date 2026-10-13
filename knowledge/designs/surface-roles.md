---
type: Design Plan
title: "Surface roles: shared materials composed per piece of UI"
description: "Proposal: a theme defines its materials once (glass, tint, outline, shadows, radius, primary) and every distinct piece of UI is a role that picks from them with six props (fill, outline, shadow, shape, hover, selected), falling back to its widget kind. Includes the 2026-10-13 inventory of about 450 painted pieces collapsed into about 60 roles over 23 kit kinds."
status: In progress (engine, theme format and roles built 2026-10-13; call sites move in lanes)
resource: crates/zz-gpui-kit/src/foundation/styled.rs
tags:
- ui
- theme
- design-system
- design-plan
timestamp: 2026-10-13T00:00:00Z
---

# Surface roles: shared materials composed per piece of UI

## Why

The nine-knob look ([interface themes](/designs/interface-themes.md)) applies each knob to everything at
once: one outline width, one shadow policy, one glass. Real apps mix materials by hierarchy. macOS
Mail puts its toolbar icons in glass capsules and the mailbox filters right below them in flat
filled chips, one of them accent. A global knob can't say that, and per-component hand styling is
what drifted before (the agent composer kept an outline at `outline = 0`).

The middle ground is CSS classes: the theme defines each material once (`.glass {}`, `.shadow {}`,
`.outline {}`), and each piece of UI composes its look from them (`.text-input { no outline,
shadow, accent }`). GNOME's libadwaita works this way (`.flat`, `.pill`, `.card`, `.toolbar` making
its buttons flat), and so do Material's component tokens, which fall back to system tokens and
then to a built-in value.

## The model

Everything painted is a surface. A surface has a role. A role picks materials.

### Materials: defined once per theme

| Material | What the theme sets |
|---|---|
| `glass` | preset and tweaks (today's `GlassMaterial`) |
| `tint` | strength of the translucent wash used for soft fills, hover and press |
| `outline` | width and strength of an edge |
| `shadow.contact` | the tight shadow under resting surfaces (y, blur, strength) |
| `shadow.float` | the deep shadow under floating surfaces |
| `radius` | the base corner radius every shape derives from |
| `primary` | what a primary fill paints: the accent or the text color |
| `row-inset`, `font` | list inset and UI font (unchanged) |

Colors stay where they are (light and dark palettes). Focus is not a material: one ring for every
focusable role, so keyboard focus always looks the same.

### A role's style: six props

| Prop | Values | Meaning |
|---|---|---|
| `fill` | `none`, `tint`, `solid`, `glass`, `primary` | what the surface is made of; exactly one per surface |
| `outline` | `on`, `off` | draws the outline material |
| `shadow` | `none`, `contact`, `float` | which shadow material |
| `shape` | `square`, `round`, `pill` | `round` follows the radius material, concentric when nested |
| `hover` | `none`, `tint` | what pointing at it adds (press is one step deeper) |
| `selected` | `none`, `tint`, `primary`, `ring`, `bar` | how "this is the current / chosen / on one" shows |

`selected` replaces today's split between Current (gray wash) and Selected (accent): each role
says which it wants. Tone stays separate: the role decides what a surface is made of, the tone
(neutral, primary, danger, warning, success) decides what its color means, so a danger button in a
glass toolbar is red-tinted glass.

Rules are surfaces too: a divider role only uses `outline` (on or off).

### Kinds and roles

Kit widgets are **kinds**: `bar`, `panel`, `pane`, `card`, `tile`, `row`, `tab`, `chip`,
`button`, `icon-button`, `field`, `toggle`, `slider`, `badge`, `keycap`, `popover`, `dialog`,
`sheet`, `toast`, `tooltip`, `scrim`, `rule`, `scrollbar`. Each carries a built-in style equal to
today's Default look, so nothing moves until a theme says so.

zz's distinct pieces are **roles** (inventory below). A role names its kind. Lookup is two steps:
the theme's entry for the role, then the theme's entry for its kind, then the built-in. A field in
Settings is just a `field`; the agent composer is the role `agent.composer` whose kind is `field`.

### What the engine enforces

1. **One door.** `.surface(role, state, cx)` paints fill, edge, shadow, corners and state. Clippy
   already rejects hand-drawn borders and shadows outside the kit; it grows to reject the old
   recipes too, so every painted piece has a role and every role shows up in the creator.
2. **No glass on glass.** A glass role on a glass ground paints `tint` instead (Apple's rule, and
   our one-fill-per-surface rule). Mail's grouped toolbar capsule falls out of it: a glass group
   holding plain buttons.
3. **Concentric corners.** `round` inside a rounded parent uses the parent radius minus the inset,
   like Apple's concentric shapes (`inner_radius` today).
4. **Ink follows the ground.** Text and keycaps read on whatever fill the role chose (`Ground`
   today).
5. **Accessibility changes materials, not roles.** Reduce transparency makes `glass` paint
   `solid`; increase contrast turns outlines on. Renderers that can't sample the backdrop (WebGL,
   DirectX) already get the solid fallback.

### Theme file

```json
{
  "name": "Tahoe",
  "materials": {
    "radius": 12,
    "glass": "regular",
    "outline": { "width": 0.5, "strength": 0.1 },
    "shadow": {
      "contact": { "y": 1, "blur": 2, "strength": 0.15 },
      "float": { "y": 12, "blur": 32, "strength": 0.25 }
    },
    "primary": "accent"
  },
  "roles": {
    "chrome.button": { "fill": "glass", "shape": "pill", "shadow": "contact" },
    "button": { "fill": "none", "outline": "off" },
    "chip": { "fill": "tint", "shape": "pill", "selected": "primary" },
    "sidebar.row": { "selected": "tint" },
    "popover": { "fill": "glass", "shadow": "float" },
    "rule.bar": { "outline": "off" }
  },
  "light": {},
  "dark": {}
}
```

Kind names and role names share the `roles` map; only what differs from the built-in is written.

## Role inventory (2026-10-13)

Four read-only sweeps covered every painted piece in `crates/zz`, `crates/zz-ui`,
`crates/zz-gpui-kit` and `clients/app`: about 450 pieces, collapsed below into about 60 roles. "Where"
names the shared function that draws most instances; "Today" names the drift the role removes.
Paths: `ui/` is `crates/zz-ui/src`, `kit/` is `crates/zz-gpui-kit/src`, `zz/` is `crates/zz/src`, `app/` is
`clients/app/src`.

### Window and title bar

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `window` | panel | window plane (solid or blurred), Linux frame shadow and edge | `ui/shell.rs:36`, `zz/window/frame.rs:113` | desktop 93% under blur, thin client opaque |
| `titlebar` | bar | title bar strip, the control strip above panes | `ui/shell.rs:137` | |
| `chrome.button` | icon-button | sidebar toggle, settings gear, tree/flat eye, iPad search, window overflow | `ui/navigation.rs:101` | already one function |
| `titlebar.window-tab` | tab | window pills with hover close and bell/activity dots | `ui/navigation.rs:162` | active looks the same as hover |
| `titlebar.status-chip` | chip | agents "2 running", update "v1.2", host name | `ui/navigation/status.rs:54`, `zz/status_bar.rs:139` | three treatments, hand-made dot |
| `titlebar.window-controls` | button | minimize, maximize, close (Windows, Linux) | `kit/title_bar/mod.rs:214` | |

### Sidebar

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `sidebar` | panel | the column, slideover, bottom fade | `ui/navigation.rs:305`, `zz/app_shell.rs:286` | slideover fade band (bugs) |
| `sidebar.row` | row | tree rows (host, session, window, pane), Settings section rows, Back | `ui/navigation.rs:337`, `ui/settings.rs:145` | Settings rows are a 28px Button path, tree rows 32px |
| `sidebar.entry` | row | flat sidebar two-line window rows | `ui/navigation/flat.rs:455` | always lit, tree lit only with focus |
| `sidebar.row-action` | icon-button | + × … and layout on hovered rows, + on session labels | `ui/navigation.rs:454`, `ui/navigation/flat.rs:425`, `:817` | four versions with three inks |
| `pane-stack` | tile | pane tiles in flat rows and title bar pills, the hover plate | `ui/navigation/pane_stack.rs:171` | |

### Panes

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `pane` | pane | tiled pane (gaps or flush), active glow, inactive dim, zoomed, waiting, phone page | `ui/pane.rs:187` | dim rules differ desktop vs thin client |
| `pane.float` | pane | floating and popup panes, the confirm-before box | `ui/pane.rs:332` | confirm box uses the Panel fill |
| `pane.header` | bar | terminal and agent title rows, the browser's two-row header | `ui/pane.rs:47`, `ui/agent.rs:3600`, `ui/browser.rs:57` | three divider treatments, wrapper copied per client |
| `pane.header.button` | icon-button | split, close, drag grip | `ui/pane.rs:18` | |
| `pane.chip` | chip | Dead, Waiting, Sync, Copy mode, status line, Unzoom, agent status pill, vim mode, "Select an element" | `ui/pane.rs:729`, `ui/agent.rs:3654`, `zz/editor/view.rs:557` | four variants |
| `pane.display-panes` | chip | display-panes number chips | `ui/pane.rs:796` | hover invisible at outline 0 |
| `pane.find-bar` | popover | terminal find bar (and the browser's once it exists) | `zz/terminal/view.rs:660` | copied per client |
| `pane.drop-zone` | scrim | drop preview, armed veil, dragged-pane veil | `ui/pane.rs:425`, `:465` | hand-drawn edge |
| `pane.drag-chip` | chip | chip following the pointer while dragging | `ui/pane.rs:479` | |

### Agent pane

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `agent.bubble` | card | your messages | `ui/agent.rs:2416` | |
| `agent.composer` | field | the message box | `ui/agent/composer.rs:56` | |
| `agent.send` | icon-button | send, queue, stop | `ui/agent/controls.rs:42` | stop has no fill, send is solid |
| `agent.control` | button | mode, model and folder pickers, plan, tasks and queue chips | `ui/agent/controls.rs:59`, `ui/agent/tasks.rs:147` | plan chip uses Selected, tasks chip nothing |
| `agent.attachment` | tile | image tiles in bubbles and the composer, their remove × | `ui/agent.rs:2105` | × has no backing |
| `agent.trace` | row | tool and thought rows, the trace summary, its count badges | `ui/agent.rs:2266` | clickable rows have no hover |
| `agent.option` | tile | answer tiles in question and permission cards | `ui/agent/presentation.rs:239` | |
| `agent.jump` | icon-button | jump-to-bottom button over the transcript | `ui/agent.rs:1025` | |
| `inline-panel` | card | panels raised inside a pane: question cards, plan and tasks tray, slash menu, side answer, failure alerts (agent, browser, editor) | `ui/agent/presentation.rs:164`, `ui/agent/tasks.rs:221`, `ui/feedback.rs:22` | all wear the floating popover look while sitting in the layout |

### Reading content (markdown in agent replies)

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `code` | card | code blocks and their header, inline code | `kit/text/node.rs:719`, `kit/text/inline.rs:213` | inline code goes square in paragraphs with images |
| `figure` | card | Mermaid frames, markdown tables | `ui/agent.rs:3033`, `kit/text/node.rs:1544` | three card fills across bubble, tile, diagram |

### Browser pane

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `browser.tab` | tab | browser tabs | `ui/browser.rs:460` | |
| `browser.button` | icon-button | back, forward, reload, picker, more, new tab | `ui/browser.rs:526` | |
| `browser.address` | field | the address bar and its site button | `ui/browser.rs:183` | |
| `browser.suggestion` | row | recent pages on the blank page | `ui/browser.rs:623` | |

### Settings

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `settings.card` | card | grouped setting cards | `ui/settings.rs:1343` | stitched per row with hand-clipped shadows |
| `settings.row` | row | a setting row, its touch press | `ui/settings.rs:1253` | some controls skip the card ground |
| `settings.tile` | tile | theme, mode and app icon tiles with the chosen ring | `ui/settings/appearance.rs:267` | 1px hand ring, 2px `chosen_ring` elsewhere |
| `settings.preview` | card | preview wells: panes, status bar, terminal, config editor | `ui/settings/panes_preview.rs:102`, `ui/settings/status_bar_preview.rs:157`, `zz/config/settings.rs:1761` | four recipes |

Text fields, number fields, selects, switches and buttons on setting rows are plain kinds.

### Phone (narrow thin client)

| Role | Kind | Covers | Where | Today |
|---|---|---|---|---|
| `phone.bar` | bar | bottom bar strip, key strip, 44px nav bars (settings, overview, sheet header) | `app/app/compact.rs:1039`, `:1144`, `app/app/settings.rs:709` | three hand-made 44px headers |
| `phone.pill` | button | the bar's pane pill, its icon tile and page dots | `ui/compact/bar.rs:87` | takes floating glass while in the layout |
| `phone.button` | icon-button | back, close, gear, keyboard | `ui/compact/bar.rs:78`, `kit/sheet.rs:94`, `app/app/compact/overview.rs:478` | three sizes, two bare divs |
| `phone.key` | keycap | key row keys, ctrl, alt and prefix keys, arrow pad | `ui/compact/key_row.rs:50` | |
| `phone.card` | tile | overview pane cards, the New pane tile | `app/app/compact/overview.rs:667`, `:529` | |
| `phone.row` | row | sheet options, all-bindings list, settings index, New window | `kit/sheet.rs:126`, `ui/compact/which_key_list.rs:54`, `ui/settings.rs:222` | four corner rules |
| `grabber` | tile | phone bar and sheet grabbers | `ui/compact/bar.rs:46`, `kit/sheet.rs:376` | two sizes and colors |

### Overlays (kit kinds used as roles)

| Role | Covers | Where | Today |
|---|---|---|---|
| `popover` | context and dropdown menus, tmux display-menu, select dropdowns, omnibox, model picker, pane-stack plate, key popovers, image preview | `kit/menu/popup_menu.rs:1184` | |
| `row` | rows in menus, the palette, pickers, choosers, omnibox, slash menu, pane picker, new-session actions | `kit/menu/menu_item.rs:97`, `kit/list/mod.rs:123`, `ui/picker.rs:111` | three corner rules; menus hover with the selection, lists with a wash |
| `palette` | command palette, path picker | `ui/command.rs:152` | no scrim, unlike choosers |
| `picker` | agent history, file picker, choose-tree and choose-buffer | `ui/picker.rs:34`, `ui/chooser.rs:118` | history picker copied per client and drifting |
| `dialog` | alerts and prompts (ssh, add host, restart daemon, import) | `kit/overlay/dialog.rs:499` | |
| `sheet` | which-key, phone bottom sheets | `ui/which_key.rs:405`, `kit/sheet.rs:355` | |
| `toast` | notifications, update offer, disconnected banner, pinch HUD | `kit/overlay/notification.rs:188`, `app/app.rs:2036`, `ui/compact/hud.rs:5` | banner and HUD use the popover look |
| `tooltip` | tooltips | `kit/tooltip/mod.rs:53` | |
| `scrim` | dimming behind dialogs, pickers, sheets, slideover, modal floats | `kit/overlay/dialog.rs:486` | |
| `keycap` | key hints everywhere | `kit/kbd.rs:189`, `:253` | Soft, Outline and plain text versions |

### Dividers

| Role | Covers | Where |
|---|---|---|
| `rule.bar` | title bar bottom, pane header bottom, phone bar top, palette and picker footers | `kit/foundation/styled.rs:133` |
| `rule.sidebar` | sidebar edge and its resize hover | `ui/navigation.rs:323` |
| `rule.split` | split divider and the accent half on the active pane's side | `ui/pane.rs:636` |
| `rule.list` | menu separators, setting row lines, prompt card lines | `ui/settings.rs:1357`, `kit/menu/popup_menu.rs:922` |
| `rule.tree` | tree indent guides | `ui/navigation/tree.rs:157` |

### Not roles

- Setting form controls and other one-off widget uses: they follow their kind.
- Text-only pieces: labels, section headers, empty-state copy, captions.
- The terminal grid: cursor, selection, search hits and links paint from the terminal palette.
- Native pieces: macOS traffic lights, the iPad key bar, the iOS Done bar.
- Invisible hit targets: split and float grips, the display-panes target.

## Theme creator

The creator becomes four steps over the same live preview:

1. **Colors**: light and dark palettes, accent, font (as today).
2. **Materials**: one card per material with its own sample (glass over a busy backdrop, both
   shadows on a plain plate, the outline at three zooms).
3. **Roles**: the inventory as a tree by region. Each role shows its six props; unset props show the
   inherited value and where it comes from (kind or built-in). Picking a role pulses every instance
   in the preview, and Inspect mode does the reverse: click anything in the preview to open its
   role. `.surface(role)` tags each element with its role, so this comes for free.
4. **Export**: JSON with only what differs.

Starter presets: Default, Tahoe (glass chrome, flat content), Linear (solid, hairlines, no
shadows), Flat.

## Settings

Advanced > Look keeps only materials (radius, outline width and strength, shadow strength, glass,
row inset, primary, font). Roles live in theme files, made in the creator and brought in with
Import or Duplicate. `widget-shadows`, `widget-selection` and `widget-field` retire into role props.

## Migration

1. Engine in the kit: role and style types, `Theme.materials`, `.surface(role, state)`, kinds
   carrying today's Default. The recipes (`card_style`, `popover_style`, `control_surface`,
   `menu_item_corners`, `selection_highlight` and the rest) become thin wrappers, then go.
2. Theme format: `materials` and `roles` replace `look` (unreleased, so no migration); the four
   built-ins are rewritten, plus a Tahoe built-in that uses the mix.
3. Assign roles in three lanes by region (chrome, sidebar and panes; agent, browser and overlays;
   settings, thin client and phone). Each lane folds its region's duplicates into the one role.
4. Creator steps and Inspect mode in the storybook.
5. Settings Look group shrinks to materials.

## Drift and bugs found by the inventory

- Radius has eight readings, outline five, focus four, hover three; Current and Selected are mixed
  up (plan chip, model picker vendor rows).
- List rows use three corner rules, touch rows four; row hover actions exist in four versions;
  status pills in four; key hints in three.
- Desktop and thin client each carry their own copy of the find bar, the image preview, the agent
  history picker (already drifting), the pane header wrapper, the slideover and the resize handle.
- Bugs:
  - the slideover sidebar fades to the window color while sitting on a popover or glass fill, which
    leaves a band (`zz/workspace/sidebar.rs:1190`);
  - display-panes chips hover by recoloring only their edge, invisible at outline 0
    (`ui/pane.rs:862`);
  - "Restart the zz daemon" and "Update and restart" end every session but use a primary button,
    not danger;
  - an agent that finished shows success in the tree and accent in the flat list counts;
  - the trace count badge cuts its ring in the window color, wrong on glass or translucent panes
    (`ui/agent.rs:2393`);
  - the iOS browser pane read the removed `theme.shadow` (fixed 2026-10-13).

## Open questions

- Glass on chrome. The 2026-10-10 rule was glass only on floating surfaces. Mail-style toolbar
  glass reverses it; ten glass buttons cost about 0.06 ms per frame on an M4 Max.
- Should Settings section rows share `sidebar.row` (today they differ)?
- Is `selected: bar` wanted? Nothing draws an accent bar today.
