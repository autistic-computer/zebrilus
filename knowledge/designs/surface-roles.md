---
type: Design Plan
title: "Surface roles: shared materials composed per piece of UI"
description: "A theme defines its materials once (radius, outline width, contact and float shadows, glass, primary, row inset) and every distinct piece of UI is a role that picks from them with six props (fill, outline, shadow, shape, hover, selected), resolved from its kit kind's built-in style, the theme's patch for the kind, the role's own patch and the theme's patch for the role. Built 2026-10-13: 19 kit kinds, 53 zz roles in zz_ui::roles, every call site painting through Theme::layer, plus the inventory of about 450 painted pieces it was drawn from."
status: Built (2026-10-13)
resource: crates/zz-gpui-kit/src/foundation/surface.rs
tags:
- ui
- theme
- design-system
- design-plan
timestamp: 2026-10-13T00:00:00Z
---

# Surface roles: shared materials composed per piece of UI

## Why

The nine-knob look that came before ([interface themes](/designs/interface-themes.md)) applied each
knob to everything at once: one outline width, one shadow policy, one glass. Real apps mix
materials by hierarchy. macOS Mail puts its toolbar icons in glass capsules and the mailbox filters
right below them in flat filled chips, one of them accent. A global knob can't say that, and
per-component hand styling is what drifted before (the agent composer kept an outline at
`outline = 0`).

The middle ground is CSS classes: the theme defines each material once (`.glass {}`, `.shadow {}`,
`.outline {}`), and each piece of UI composes its look from them (`.text-input { no outline,
shadow, accent }`). GNOME's libadwaita works this way (`.flat`, `.pill`, `.card`, `.toolbar` making
its buttons flat), and so do Material's component tokens, which fall back to system tokens and
then to a built-in value.

## Where it lives

| Piece | Code |
|---|---|
| Kinds, styles, patches, `Layer` and the resolver (`Theme::style`, `Theme::layer`) | `crates/zz-gpui-kit/src/foundation/surface.rs` |
| Painting helpers (`.surface`, `.surface_at`, `.layer`, `.rule_on`, `row_corners`, `row_radius`) | `crates/zz-gpui-kit/src/foundation/styled.rs` |
| `Theme` fields for the materials, `styles` (the theme's patches) and `inspect` | `crates/zz-gpui-kit/src/foundation/theme.rs` |
| zz's roles by region (`REGIONS`, `all()`, `find()`) | `crates/zz-ui/src/roles.rs` |
| Theme files: `Materials`, `MaterialOverrides`, the `roles` map and its id check | `crates/zz-ui/src/interface_theme.rs`, `crates/zz-ui/themes/*.json` |
| The theme creator | `clients/storybook/src/stories/theme_creator.rs`, `clients/storybook/src/knobs.rs` |

## The model

Everything painted is a surface. A surface has a role. A role picks materials.

### Materials: defined once per theme

`Materials` in a theme file, applied onto the kit `Theme` by `Materials::apply`:

| Material | What the theme sets |
|---|---|
| `radius` | the base corner radius every `round` shape derives from |
| `outline` | the width of every outline in pixels; 0 draws none |
| `row-inset` | how far list rows sit in from their list's edges; 0 makes them square bands that touch |
| `primary` | what a prominent fill paints: `foreground` or `accent` |
| `font` | the interface font family, or none for the platform's |
| `glass` | `null`, a glass preset name, or a preset with the fields it changes |
| `contact` | the tight shadow of things resting on the window: `y`, `blur`, `spread`, `strength` |
| `float` | the deep shadow of things floating over it, with the same four fields |

Colors stay in the light and dark sets. Focus is not a material: one ring for every focusable
role, so keyboard focus always looks the same. A `strength` of 1 is the theme's shadow color as
the mode sets it, 0 casts nothing; the float shadow draws the contact shadow under a deep one.

### A role's style: six props

`Style` in the kit, written in theme files as a `StylePatch` that names only what differs:

| Prop | Values | Meaning |
|---|---|---|
| `fill` | `none`, `tint`, `solid`, `glass` | what the surface is made of; one per surface |
| `outline` | `true`, `false` | draws the outline material |
| `shadow` | `none`, `contact`, `float` | which shadow material |
| `shape` | `square`, `round`, `pill` | `round` follows the radius material; rows take it less 4px so they sit concentric in their list, and go square when the row inset is 0 |
| `hover` | `none`, `tint`, `lift`, `accent`, `ring`, `bar` | what pointing at it adds; press is the same highlight one wash deeper |
| `selected` | the same six | how "this is the current, chosen or on one" shows |

The highlights: `tint` is a wash a step deeper than the fill; `lift` is that wash plus the outline
and a contact shadow, as if the surface rose; `accent` fills with the accent and its text; `ring`
is a 2px accent ring outside the edge; `bar` is a 3px accent bar along the leading edge (under a
tab, along the bottom).

`selected` replaces the old split between Current (gray wash) and Selected (accent): each role says
which it wants. Tone stays separate: the role decides what a surface is made of, the tone (neutral,
primary, danger, warning, success) decides what its color means, so a danger button in a glass
toolbar is red-tinted glass. A prominent surface (`Surface::prominent`, what a solid `Button` is)
fills with its tone, primary when neutral; on glass it tints the glass with it.

Rules are surfaces too: a divider role only uses `outline`, and `.rule_on(role, edge)` draws a 1px
line in the divider color when the role's outline is on.

### Kinds and roles

Kit widgets are **kinds** (`Kind`, 19 of them): `bar`, `panel`, `pane`, `card`, `tile`, `row`,
`tab`, `chip`, `button`, `icon-button`, `field`, `badge`, `keycap`, `popover`, `dialog`, `sheet`,
`toast`, `tooltip`, `rule`. Each carries a built-in style (`Kind::base`) that draws the Default
theme as it looked before roles. The kit also names `button.quiet`, a button with no fill until
pointed at; `SurfaceRole::KIT` holds it and the 19 kinds.

zz's distinct pieces are **roles** (`SurfaceRole`: an id, a kind, a label and description for the
creator, and a built-in `StylePatch`), 53 of them in `zz_ui::roles` by region. A role resolves in
four steps (`Theme::style`):

1. the kind's built-in style;
2. the theme's patch for the kind;
3. the role's built-in patch;
4. the theme's patch for the role.

A field in Settings is just a `field`; the agent composer is the role `agent.composer`, whose kind
is `field`. A widget with no role draws as its kind.

### What the engine enforces

1. **One door.** `Theme::layer(surface, state)` returns everything a surface paints in one state:
   fill or glass, edge and its width, shadows, ink, radius, and the ground what sits inside it
   paints on. `.surface(role)`, `.surface_at(role, state)`, `.surface_hover` and
   `.surface_interactive` paint it on an element; kit widgets take `.surface_role(role)`. The root
   `clippy.toml` rejects hand-drawn borders, the `shadow_*` presets, `BoxShadow`, and the kit's
   leftover edge, ring and radius helpers outside the kit, so every painted piece has a role and
   every role shows up in the creator.
2. **No glass on glass.** A glass role on a glass ground paints `tint` instead, and a glass role
   in a theme without glass paints `solid`. Mail's grouped toolbar capsule falls out of it.
3. **Controls cast shadows only on the window.** Rows, tabs, chips, buttons, icon buttons, fields,
   badges and keycaps on a panel or a popover keep their outline and drop the shadow, since that
   surface already stands off the window.
4. **Highlights never change a border's width.** A role that draws an outline, or lifts on hover
   or selection, reserves the outline width in every state and only recolors it.
5. **Fills composite opaque under an outer shadow.** GPUI draws a shadow under the whole box, so a
   translucent fill over one turns muddy. A non-glass surface that casts a shadow, or shows a
   ring, blends its fill over its ground first.
6. **Ink follows the ground.** `Layer::ground` carries the composited fill, the ink, the elevation
   and whether it is glass, so text, keycaps and nested controls read on whatever the role chose.
7. **Inspect.** `Theme.inspect` names one role id; every surface drawn through it adds a 2px
   magenta ring just inside its edge, so a translucent fill never shows it through.

`Theme::casts_shadow(role)`, `shadows_of(shadow)`, `contact_shadows()`, `float_shadows()`,
`shape_radius(kind, shape)` and `ground_of(surface)` expose the same answers to code that needs
one value.

### Theme file

From `crates/zz-ui/themes/tahoe.json`, colors left out:

```json
{
  "name": "Tahoe",
  "materials": {
    "radius": 14,
    "outline": 0.5,
    "row-inset": 5,
    "primary": "accent",
    "glass": { "preset": "regular", "blur": 19.7, "tint": "#ffffff87" }
  },
  "roles": {
    "chrome.button": { "fill": "glass", "shape": "pill", "shadow": "contact", "hover": "tint", "selected": "tint" },
    "button": { "fill": "tint", "outline": false, "shadow": "none", "shape": "pill" },
    "chip": { "fill": "tint", "outline": false, "shadow": "none", "shape": "pill", "selected": "accent" },
    "agent.composer": { "fill": "glass", "shadow": "contact", "shape": "round" },
    "sidebar.row": { "hover": "tint", "selected": "tint" },
    "rule.bar": { "outline": false }
  }
}
```

Kind names and role ids share the `roles` map. An id `zz_ui::roles::find` does not know is an
error that suggests the role it was probably meant to be (`chromebutton` gets `chrome.button`,
`composer` gets `agent.composer`).

## Where the build differs from the proposal

- **Materials.** There is no `tint` material and no outline strength. The outline is a width
  only; its ink is fixed (the border color on surfaces, 10% of the ground's ink on controls), and
  tints are the kit's wash steps. Shadows gained `spread`.
- **Fill.** `primary` is not a fill. Prominence is a property of the surface (`Surface::prominent`),
  so the theme's `primary` material and the tone decide its color whatever the role's fill.
- **Highlights.** `hover` and `selected` take the same six values, and `lift` is new: the wash with
  the outline and a contact shadow, which Default's icon buttons, tabs, sidebar rows and quiet
  buttons take on hover and selection.
- **Lookup.** The proposal looked up the theme's role entry, then its kind entry, then the
  built-in. Roles carry their own built-in patch, so the order is the four steps above.
- **Kinds.** 19, not 23: `toggle`, `slider`, `scrim` and `scrollbar` are not kinds. Switches and
  sliders paint from tone and emphasis, scrims fill with `theme.scrim`, and scrollbars keep their
  own paint.
- **Role kinds.** Lanes changed some kinds once they drew the real pieces: `pane-stack` (proposed
  tile) and `pane.drop-zone` (proposed scrim) are chips, so their washes deepen and their edges
  follow the row or pill they sit on; `inline-panel` and `code` (proposed card) are popovers, so
  their fill steps off the window as before, with `code` a solid fill without an outline or
  shadow; `agent.option` (proposed tile) is a button, so a danger option keeps its red wash and
  hairline; `titlebar.window-controls` (proposed button) is a square icon button; `grabber`
  (proposed tile) is a pill badge.
- **`pane.display-panes` and `pane.drag-chip`** are popovers, as their old floating look was, so a
  theme's chip patch does not reach them.
- **New roles.** The review pass after the lanes added the roles a piece needed to keep its
  Default look while still taking a theme:
  - kit: `field.multiline` (multi-line fields and framed editors, always round so a pill field
    theme does not turn the composer into a stadium), `icon-button.flat` (icon buttons that never
    lift: find bar toggles, chooser close, settings reset, site controls) and `grabber` (bar and
    sheet handles, moved from zz-ui so every sheet takes it);
  - zz: `picker.notice` (square strips across a picker that ask first, like the history delete
    banner), `sidebar.cursor` (the flat sidebar's keyboard cursor row), `sidebar.host-action`
    (Connect, Retry, Upgrade on host headers), `browser.empty-icon` (the blank page's globe disc),
    `phone.pill.icon` (the pane-kind tile inside the phone pill) and `phone.arrow-pad` (the arrow
    pad, which rests clear unlike the other keys).
- **`window`** drives the Linux client-side frame: its shadow follows the role's shadow and its
  edge the role's outline. The window plane's own fill stays the ground everything sits on.
- **`pane.selected`** decides whether the active pane lights its edge: `none` turns the glow off.
- **Not wired.** Reduce transparency and increase contrast do not change materials yet; renderers
  that cannot sample the backdrop (WebGL, DirectX) still get the glass's solid fallback.

## Role inventory (2026-10-13)

Four read-only sweeps covered every painted piece in `crates/zz`, `crates/zz-ui`,
`crates/zz-gpui-kit` and `clients/app`: about 450 pieces, collapsed below into roles. "Where" names
the shared function that drew most instances at the time of the sweep; "Drift found" names what the
role was meant to remove, and what became of it. Kinds are as built. Paths: `ui/` is
`crates/zz-ui/src`, `kit/` is `crates/zz-gpui-kit/src`, `zz/` is `crates/zz/src`, `app/` is
`clients/app/src`.

### Window and title bar

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `window` | panel | window plane (solid or blurred), Linux frame shadow and edge | `ui/shell.rs:36`, `zz/window/frame.rs:113` | desktop 93% under blur, thin client opaque; nothing draws through the role yet |
| `titlebar` | bar | title bar strip, the control strip above panes | `ui/shell.rs:137` | |
| `chrome.button` | icon-button | sidebar toggle, settings gear, tree/flat eye, iPad search, window overflow | `ui/navigation.rs:101` | already one function |
| `titlebar.window-tab` | tab | window pills with hover close and bell/activity dots | `ui/navigation.rs:162` | active looks the same as hover; Default still lifts both, a theme can tell them apart (Tahoe fills the active pill with the accent) |
| `titlebar.status-chip` | chip | agents "2 running", update "v1.2", host name | `ui/navigation/status.rs:54`, `zz/status_bar.rs:139` | three treatments, hand-made dot; fixed, one chip |
| `titlebar.window-controls` | icon-button | minimize, maximize, close (Windows, Linux) | `kit/title_bar/mod.rs:214` | |

### Sidebar

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `sidebar` | panel | the column, slideover, bottom fade | `ui/navigation.rs:305`, `zz/app_shell.rs:286` | slideover fade band; fixed (see Bugs) |
| `sidebar.row` | row | tree rows (host, session, window, pane), Settings section rows, Back | `ui/navigation.rs:337`, `ui/settings.rs:145` | Settings rows were a 28px Button path, tree rows 32px; fixed, Settings section rows and Back draw as `sidebar.row` |
| `sidebar.entry` | row | flat sidebar two-line window rows | `ui/navigation/flat.rs:455` | always lit, tree lit only with focus |
| `sidebar.row-action` | icon-button | + × … and layout on hovered rows, + on session labels | `ui/navigation.rs:454`, `ui/navigation/flat.rs:425`, `:817` | four versions with three inks; fixed, one function and one ink, muted until pointed at |
| `pane-stack` | chip | pane tiles in flat rows and title bar pills, the hover plate | `ui/navigation/pane_stack.rs:171` | |

### Panes

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `pane` | pane | tiled pane (gaps or flush), active glow, inactive dim, zoomed, waiting, phone page | `ui/pane.rs:187` | dim rules differ desktop vs thin client |
| `pane.float` | pane | floating and popup panes, the confirm-before box | `ui/pane.rs:332` | confirm box uses the Panel fill |
| `pane.header` | bar | terminal and agent title rows, the browser's two-row header | `ui/pane.rs:47`, `ui/agent.rs:3600`, `ui/browser.rs:57` | three divider treatments, wrapper copied per client; fixed, one `pane_header_frame` and the `rule.bar` divider |
| `pane.header.button` | icon-button | split, close, drag grip | `ui/pane.rs:18` | |
| `pane.chip` | chip | Dead, Waiting, Sync, Copy mode, status line, Unzoom, agent status pill, vim mode, "Select an element" | `ui/pane.rs:729`, `ui/agent.rs:3654`, `zz/editor/view.rs:557` | four variants; fixed, one chip with a contact shadow |
| `pane.display-panes` | chip | display-panes number chips | `ui/pane.rs:796` | hover invisible at outline 0; fixed (see Bugs) |
| `pane.find-bar` | popover | terminal find bar (and the browser's once it exists) | `zz/terminal/view.rs:660` | copied per client; fixed, one `terminal_find_bar` in `zz_ui::terminal` |
| `pane.drop-zone` | chip | drop preview, armed veil, dragged-pane veil | `ui/pane.rs:425`, `:465` | hand-drawn edge |
| `pane.drag-chip` | chip | chip following the pointer while dragging | `ui/pane.rs:479` | |

### Agent pane

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `agent.bubble` | card | your messages | `ui/agent.rs:2416` | |
| `agent.composer` | field | the message box | `ui/agent/composer.rs:56` | |
| `agent.send` | icon-button | send, queue, stop | `ui/agent/controls.rs:42` | stop had no fill, send was solid; fixed, stop takes send's fill |
| `agent.control` | button | mode, model and folder pickers, plan, tasks and queue chips | `ui/agent/controls.rs:59`, `ui/agent/tasks.rs:147` | plan chip used Selected, tasks chip nothing; fixed, the open plan chip shows the control's selected lift |
| `agent.attachment` | tile | image tiles in bubbles and the composer, their remove × | `ui/agent.rs:2105` | × has no backing |
| `agent.trace` | row | tool and thought rows, the trace summary, its count badges | `ui/agent.rs:2266` | clickable rows had no hover; fixed |
| `agent.option` | button | answer tiles in question and permission cards | `ui/agent/presentation.rs:239` | |
| `agent.jump` | icon-button | jump-to-bottom button over the transcript | `ui/agent.rs:1025` | |
| `inline-panel` | popover | panels raised inside a pane: question cards, plan and tasks tray, slash menu, side answer, failure alerts (agent, browser, editor) | `ui/agent/presentation.rs:164`, `ui/agent/tasks.rs:221`, `ui/feedback.rs:22` | all wear the floating popover look while sitting in the layout; kept on purpose, now as a role a theme can change |

### Reading content (markdown in agent replies)

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `code` | popover | code blocks and their header, inline code | `kit/text/node.rs:719`, `kit/text/inline.rs:213` | inline code went square in paragraphs with images; fixed (see Bugs) |
| `figure` | card | Mermaid frames, markdown tables | `ui/agent.rs:3033`, `kit/text/node.rs:1544` | three card fills across bubble, tile, diagram; fixed, figures have no fill of their own and tables wear the figure look |

### Browser pane

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `browser.tab` | tab | browser tabs | `ui/browser.rs:460` | |
| `browser.button` | icon-button | back, forward, reload, picker, more, new tab | `ui/browser.rs:526` | |
| `browser.address` | field | the address bar and its site button | `ui/browser.rs:183` | |
| `browser.suggestion` | row | recent pages on the blank page | `ui/browser.rs:623` | |

### Settings

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `settings.card` | card | grouped setting cards | `ui/settings.rs:1343` | stitched per row with hand-clipped shadows; still rounded only at a run's ends with the shadow clipped per row, now from the role's layer |
| `settings.row` | row | a setting row, its touch press | `ui/settings.rs:1253` | some controls skipped the card ground; fixed, every control on a card sits on `settings_card_ground` |
| `settings.tile` | tile | theme, mode and app icon tiles with the chosen ring | `ui/settings/appearance.rs:267` | 1px hand ring, 2px `chosen_ring` elsewhere; fixed, the role's 2px selected ring |
| `settings.preview` | card | preview wells: panes, status bar, terminal, config editor | `ui/settings/panes_preview.rs:102`, `ui/settings/status_bar_preview.rs:157`, `zz/config/settings.rs:1761` | four recipes; fixed, one `settings_preview` well |

Text fields, number fields, selects, switches and buttons on setting rows are plain kinds.

### Phone (narrow thin client)

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `phone.bar` | bar | bottom bar strip, key strip, 44px nav bars (settings, overview, sheet header) | `app/app/compact.rs:1039`, `:1144`, `app/app/settings.rs:709` | three hand-made 44px headers; fixed, one `compact_nav_bar` |
| `phone.pill` | button | the bar's pane pill, its icon tile and page dots | `ui/compact/bar.rs:87` | took floating glass while in the layout; fixed, a solid fill without an edge |
| `phone.button` | icon-button | back, close, gear, keyboard | `ui/compact/bar.rs:78`, `kit/sheet.rs:94`, `app/app/compact/overview.rs:478` | three sizes, two bare divs; fixed, one 44pt `compact_bar_button` with a 20pt glyph |
| `phone.key` | keycap | key row keys, ctrl, alt and prefix keys, arrow pad | `ui/compact/key_row.rs:50` | |
| `phone.card` | tile | overview pane cards, the New pane tile | `app/app/compact/overview.rs:667`, `:529` | |
| `phone.row` | row | sheet options, all-bindings list, settings index, New window | `kit/sheet.rs:126`, `ui/compact/which_key_list.rs:54`, `ui/settings.rs:222` | four corner rules; fixed, one role |
| `grabber` | badge | phone bar and sheet grabbers | `ui/compact/bar.rs:46`, `kit/sheet.rs:376` | two sizes and colors |

### Overlays

`popover`, `row`, `dialog`, `sheet`, `toast`, `tooltip` and `keycap` are kinds, not zz roles; the
overlays name these roles over them:

| Role | Kind | Covers | Where | Drift found |
|---|---|---|---|---|
| `palette` | popover | command palette, path picker | `ui/command.rs:152` | no scrim, unlike choosers |
| `picker` | dialog | agent history, file picker, choose-tree and choose-buffer | `ui/picker.rs:34`, `ui/chooser.rs:118` | history picker copied per client and drifting; fixed, one picker in `zz_ui::agent::history` |
| `picker.notice` | badge | strips across a picker that ask first, like the delete banner | `ui/agent/history.rs:181` | new |

What the inventory found on the kinds themselves:

| Kind | Covers | Where | Drift found |
|---|---|---|---|
| `popover` | context and dropdown menus, tmux display-menu, select dropdowns, omnibox, model picker, pane-stack plate, key popovers, image preview | `kit/menu/popup_menu.rs:1184` | |
| `row` | rows in menus, the palette, pickers, choosers, omnibox, slash menu, pane picker, new-session actions | `kit/menu/menu_item.rs:97`, `kit/list/mod.rs:123`, `ui/picker.rs:111` | three corner rules; fixed, `row_corners` everywhere. Menus still move their selection on hover, lists hover with a wash |
| `dialog` | alerts and prompts (ssh, add host, restart daemon, import) | `kit/overlay/dialog.rs:499` | |
| `sheet` | which-key, phone bottom sheets | `ui/which_key.rs:405`, `kit/sheet.rs:355` | |
| `toast` | notifications, update offer, disconnected banner, pinch HUD | `kit/overlay/notification.rs:188`, `app/app.rs:2036`, `ui/compact/hud.rs:5` | banner and HUD used the popover look; fixed, both draw as toasts |
| `tooltip` | tooltips | `kit/tooltip/mod.rs:53` | |
| `keycap` | key hints everywhere | `kit/kbd.rs:189`, `:253` | Soft, Outline and plain text versions; fixed, `Kbd` and `Keycap` draw one cap (menus and tooltips still show bindings as text) |

Scrims (dimming behind dialogs, pickers, sheets, the slideover and modal floats) are not a role:
they fill with `theme.scrim`.

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

The storybook's theme creator (`clients/storybook`) is four steps over the same live preview:

1. **Colors**: light and dark palettes, accent, font.
2. **Materials**: shape (radius, outline, row inset, primary), glass, and both shadow materials.
3. **Roles**: `zz_ui::roles::REGIONS` as a tree by region, with Kinds last. Each role shows its six
   props; an unset prop shows the value it inherits and where from (the kind's built-in, the
   theme's patch for the kind, or the role's built-in). Setting a prop writes only what differs
   into the theme's `roles` patch. Picking or pointing at a role sets `Theme.inspect`, which rings
   every surface drawn through it.
4. **Export**: JSON with only what differs, and Import.

Every built-in starts the creator with its materials and roles. Clicking the preview to open a
role is not built.

## Settings

Settings > Interface > Advanced ends with a Materials group: corner radius, outline, shadow
strength, row inset, primary and glass (`widget-corner-radius`, `widget-outline-width`,
`widget-shadow-strength`, `widget-row-inset`, `widget-primary`, `widget-glass`). Shadow strength
scales both shadow materials from 0 to 2. Roles live in theme files, made in the creator and
brought in with Import or Duplicate. `widget-shadows`, `widget-selection` and `widget-field` are
gone into role props, and `shadow-strength` points to `widget-shadow-strength`; see
[app config](/configuration/app-config.md).

## Migration

All five steps landed on 2026-10-13.

1. Engine in the kit: kinds, styles, patches, `Theme::style` and `Theme::layer`, with each kind's
   built-in equal to the Default look. The old recipes (`card_style`, `popover_style`,
   `control_surface`, `control_highlight`, `surface_fill`, `menu_item_corners`,
   `selection_highlight`, the `rule_*` family, `contact_shadow`, `control_shadow`,
   `floating_shadow`) are deleted.
2. Theme format: `materials` and `roles` replace `look` (unreleased, so no migration). Default,
   macOS, Linear and Glass are rewritten, and Tahoe joins them: glass chrome and browser buttons
   over flat filled pill buttons and chips, and no bar dividers.
3. Roles assigned in three lanes by region (chrome, sidebar and panes; agent, browser and
   overlays; settings, thin client and phone). Each lane folded its region's desktop and
   thin-client copies into one zz-ui function: the terminal find bar, the pasted-image preview,
   the agent history picker, the pane header frame, the sidebar slideover, panel, resize handle and
   fade, the settings page heading and group labels, `settings_preview`, `compact_nav_bar` and
   `compact_bar_button`.
4. Creator steps and the inspect ring in the storybook.
5. Settings' Look group became the Materials group.

## Drift and bugs found by the inventory

- Radius had eight readings, outline five, focus four, hover three, and Current and Selected were
  mixed up (plan chip, model picker vendor rows). Corners now come from the role's shape, outlines
  from the one material, and the plan chip and the current vendor take the control's and the
  row's selected highlight.
- List rows used three corner rules and touch rows four; row hover actions existed in four
  versions, status pills in four and key hints in three. Each is one role or one function now.
- Desktop and thin client each carried their own copy of the find bar, the image preview, the
  agent history picker, the pane header wrapper, the slideover and the resize handle. Each is one
  zz-ui function now.
- Bugs, all fixed:
  - the slideover sidebar faded to the window color while sitting on a popover or glass fill,
    which left a band; the fade now ends in the surface it sits on (`workspace_sidebar_fade`);
  - display-panes chips hovered by recoloring only their edge, invisible at outline 0; they ring
    on hover now;
  - "Restart the zz daemon" and "Update and restart" end every session but used a primary button;
    they confirm with a danger button;
  - an agent that finished showed success in the tree and accent in the flat list counts; both
    use the success color;
  - inline code in a paragraph holding an image went square; it gets the mono font and the
    rounded fill of every other paragraph;
  - the trace count badge cut its ring in the window color, wrong on glass or translucent panes;
    it takes the badge tint and cuts its ring in the pane's color;
  - the iOS browser pane read the removed `theme.shadow`.

## Decisions

- **Glass on chrome is a theme's call.** The 2026-10-10 rule was glass only on floating surfaces.
  Tahoe puts Mail-style glass on chrome and browser buttons; Default keeps them plain. Ten glass
  buttons cost about 0.06 ms per frame on an M4 Max.
- **Settings section rows share `sidebar.row`**, like the session tree.
- **`selected: bar` stays.** The engine draws it; no built-in theme uses it yet.
