---
type: Design Plan
title: "Surface roles: shared materials composed per piece of UI"
description: "A theme defines its materials once (radius, corner smoothing, outline width, contact and float shadows, glass, primary, row inset) and every distinct piece of UI is a role that picks from them with seven props (fill, outline, shadow, shape, smoothing, hover, selected), resolved from its kind's built-in style and each group's built-in patch, then the theme's patches from the kind inward, so a theme's patch for a kind beats every built-in in it. Built 2026-10-13; regrouped 2026-10-14 into 10 broad kinds (bars, panels, panes, cards, floating, buttons, items, fields, labels, dividers) with groups under them, 73 zz roles in zz_ui::roles, every call site painting through Theme::layer, plus the inventory of about 450 painted pieces it was drawn from."
status: Built (2026-10-13; agent pane rows updated for the lens 2026-10-23; driver chat roles added 2026-10-24, retired with the Driver Agent 2026-10-28)
resource: libs/penna/src/foundation/surface.rs
tags:
- ui
- theme
- design-system
- design-plan
timestamp: 2026-10-31T00:00:00Z
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
| Kinds, styles, patches, `Layer` and the resolver (`Theme::style`, `Theme::layer`) | `libs/penna/src/foundation/surface.rs` |
| Painting helpers (`.surface`, `.surface_at`, `.layer`, `.rule_on`, `row_corners`, `row_radius`) | `libs/penna/src/foundation/styled.rs` |
| `Theme` fields for the materials, `styles` (the theme's patches) and `inspect` | `libs/penna/src/foundation/theme.rs` |
| Window chrome roles, `register`, `retire` and the lookups (`all()`, `find()`, `members()`) | `libs/penna/src/roles.rs` |
| zz's roles, retired ids, and the zz-ui lookups that register them first | `crates/zz-ui/src/roles.rs`, `zz_ui::register` in `crates/zz-ui/src/lib.rs` |
| Theme files: `Materials`, the `roles` map and its id check; `ThemeOverrides`, what Settings sets over a theme | `libs/penna/src/interface_theme.rs`; the built-ins in `libs/penna/themes/*.json`, zz's patches for its own roles over them in `crates/zz-ui/themes/*.json` |
| The theme creator | `penna_storybook::creator` in `libs/penna/storybook/src/creator/`, `libs/penna/storybook/src/knobs.rs`; zz's plates in `clients/storybook/src/stories/theme_creator.rs` |

## The model

Everything painted is a surface. A surface has a role. A role picks materials.

### Materials: defined once per theme

`Materials` in a theme file, applied onto the kit `Theme` by `Materials::apply`:

| Material | What the theme sets |
|---|---|
| `radius` | the base corner radius every `round` shape derives from |
| `smoothing` | the superellipse exponent every role's rounded corners follow unless the theme sets `smoothing` for the role, from 2 to 10; 4 when left out, the continuous corner every window draws with |
| `picked-smoothing` | whether only the roles the theme sets `smoothing` for curve that way: every other role a theme could smooth draws plain circular corners (2); roles no theme can smooth, like panes, keep `smoothing` |
| `outline` | the width of every outline in pixels; 0 draws none |
| `picked-outlines` | whether only the roles the theme turns `outline` on for draw it: every built-in outline is off (dividers keep their lines, and roles no theme can change keep theirs), and so are edges drawn without a role, like a field's focus border and color swatch edges; pane frames take the `pane` role's outline in either mode |
| `row-inset` | how far list rows sit in from their list's edges; 0 makes them square bands that touch |
| `primary` | what a prominent fill paints: `foreground` or `accent` |
| `flat-highlights` | whether a `lift` highlight stays flat: it washes one step deeper (selected 0.16 where a tint is 0.12) and draws no outline or shadow; other highlights are unchanged |
| `bright-surfaces` | whether cards, panes and floating surfaces rise toward white in light mode, as far in Oklab lightness as they would otherwise darken and stopping at white, so a `#fafafa` window gets white menus and cards the way GNOME draws them; their grounds (`Theme::surface`) rise the same way, and dark mode rises lighter either way |
| `highlight-strength` | how far a `tint` or `lift` highlight washes from the resting fill, alpha-premultiplied: 1 is the built-in step, 0 none (a lift still draws its outline and shadow), 2 twice as far; 1 when left out |
| `font` | the interface font family, or none for the platform's; a family the machine lacks draws in the platform's (Inter in the storybook) |
| `glass` | `null`, a glass preset name, or a preset with the fields it changes |
| `picked-shadows` | whether only the roles the theme gives a `shadow` cast one: every built-in shadow is off (roles no theme can change keep theirs, like the Linux window frame), and so is the knob shadow on switches, sliders and the color picker; a `lift` highlight still rises with the contact shadow |
| `contact` | the tight shadow of things resting on the window: `y`, `blur`, `spread`, `strength` |
| `float` | the deep shadow of things floating over it, with the same four fields |

Colors stay in the light and dark sets. Focus is not a material: one ring for every focusable
role, so keyboard focus always looks the same. A `strength` of 1 is the theme's shadow color as
the mode sets it, 0 casts nothing; the float shadow draws the contact shadow under a deep one.

### A role's style: seven props

`Style` in the kit, written in theme files as a `StylePatch` that names only what differs:

| Prop | Values | Meaning |
|---|---|---|
| `fill` | `none`, `tint`, `solid`, `glass` | what the surface is made of; one per surface |
| `outline` | `true`, `false` | draws the outline material |
| `shadow` | `none`, `contact`, `float` | which shadow material |
| `shape` | `square`, `round`, `pill` | `round` follows the radius material; rows take it less 4px so they sit concentric in their list, and go square when the row inset is 0 |
| `smoothing` | `2`, `2.5`, `3`, `3.5`, `4`, `5` | the superellipse exponent its rounded corners follow: `2` is a circular arc, `4` the continuous corner every window draws with, higher turns the corner tighter. Open wherever `shape` is; a pill always ends in half circles whatever its smoothing. Unset, a role takes the `smoothing` material, or `2` under `picked-smoothing` |
| `hover` | `none`, `tint`, `lift`, `accent`, `ring`, `bar`, `glass` | what pointing at it adds; press is the same highlight one wash deeper |
| `selected` | the same seven, and `edge` on fields | how "this is the current, chosen or on one" shows |
| `ink` | `bright`, `rest` | how its text sits: `bright` is the full ink in every state; `rest` is a step below it until hovered, pressed, selected or current, so pointing at it always brightens it. Open on buttons and items. The title bar's buttons and window tabs and the sidebar's rows take `rest` |

The highlights: `tint` is a wash a step deeper than the fill; `lift` is that wash plus the outline
and a contact shadow, as if the surface rose; `accent` fills with the accent and its text; `ring`
is a 2px accent ring outside the edge; `bar` is a 3px accent line along the bottom edge (on a
row, the `tint` wash with a 3 by 16px accent pill centered on its leading edge, as Windows 11 marks
the selected navigation item); `glass` turns the surface into the theme's glass for that state, tinted
with its solid fill, so a tab can be solid at rest and glass when selected. A surface already in
glass, or sitting on glass, takes `tint` instead, and a theme with no glass takes `lift`, so the
state never paints the same as rest. Hover and press restyle with `restyle`, which clears the
resting fill when the state draws glass, since a fill paints over the glass under it.

`selected` replaces the old split between Current (gray wash) and Selected (accent): each role says
which it wants. Tone stays separate: the role decides what a surface is made of, the tone (neutral,
primary, danger, warning, success) decides what its color means, so a danger button in a glass
toolbar is red-tinted glass. A prominent surface (`Surface::prominent`, what a solid `Button` is)
fills with its tone, primary when neutral; on glass it tints the glass with it. A `tint` surface
that is not a control (a card or a panel) takes its tone's wash when it has a tone, and the ground's
wash otherwise: `chat.bubble.mine`, retired on 2026-10-28, was a card drawn in the accent tone.

Rules are surfaces too: a divider role only uses `outline`, and `.rule_on(role, edge)` draws a 1px
line in the divider color when the role's outline is on.

### Kinds, groups and roles

A **kind** (`Kind`) is what a piece of UI is, broadly, so one patch can restyle everything of that
sort. There are 10:

| Kind | What it holds |
|---|---|
| `bar` | strips across a window, pane or sheet: the title bar, pane headers, the phone bar |
| `panel` | columns set into the window: the window frame, the sidebar |
| `pane` | terminal, browser, agent and editor panes |
| `card` | boxes in the flow: message bubbles, code blocks, figures, attachments, setting groups, previews |
| `floating` | everything over the content: menus, palettes, dialogs, sheets, toasts, tooltips, inline panels, floating panes, pane chips, pane numbers, drag chips, drop zones |
| `button` | everything you press to do something: buttons, icon buttons, menu items, answer options, the phone's pill and keys |
| `item` | what you pick that stays picked or open: sidebar rows, tabs, tray tabs, tiles |
| `field` | text fields, selects, the composer, the address bar, the agent's pickers; selected paints the one you are typing in or whose menu is open |
| `label` | badges, keycaps, status chips, grabbers |
| `rule` | divider lines |

Kinds hold things, and things nest: a dialog is floating but its Cancel is a button, and a
sidebar is a panel whose rows are items. So a kind never contains itself, and a patch for a kind
never stacks fill on fill or outline in outline.

Within a kind, roles group the way the pieces do. A role (`SurfaceRole`: an id, its kind, the
group it is in, a label and description for the creator, a built-in `StylePatch` and whether it is
a row) is either a kind itself, a group the kit names, or one of zz's places, which sits in a kind
or in a group (`SurfaceRole::under`). The kit's groups (`SurfaceRole::KIT` holds them with the
kinds):

- floating: `popover` (menus and popovers), `dialog`, `sheet` with `drawer` in it (every touch
  bottom sheet, resting above the bottom safe area), `toast`, `tooltip`;
- button: `icon-button` with `icon-button.flat` in it, `button.quiet`, and `menu-item`, the rows
  you press in menus, palettes, pickers and dropdowns;
- item: `row`, `tab`, `tile`;
- field: `field.multiline`;
- label: `badge` with `grabber` in it, `keycap`, `icon-tile` (the tile that leads a sheet row; its
  ink is the icon's color, rest by default).

So `chrome.button` is in `icon-button`, which is in `button`; `palette` is in `popover`, which is
in `floating`; `pane.chip` is in `floating` directly. A role resolves the way author CSS beats the browser's defaults
(`Theme::style`):

1. the kind's built-in style (`Kind::base`), which draws the Default theme;
2. each group's built-in patch from the kind inward, ending with the role's;
3. the theme's patch for each of them in the same order, ending with the role's.

So any theme patch beats every built-in, and among theme patches the closest one wins: a theme
that makes `floating` glass makes drop zones and pane chips glass too, though both are tint or
solid as built in, and a patch for `pane.drop-zone` takes it back. Built-in patches only say how
the Default theme draws a role. A widget
with no role draws as its kind. `zz_ui::roles::all()` lists the kit's kinds and groups, then zz's
67 roles; `members(group)` lists what is directly in a group.

A row (`SurfaceRole::row`, set on `menu-item` and `row` and taken by everything in them) rounds
concentric inside its list: its round corners are the radius less 4px, square when the row inset
is 0, and a bar highlight is the wash with a short pill on its leading edge (ala's
`EdgeMark`, carried on `Layer::mark`) instead of a line along the bottom.

### Knobs: what a theme may change

Not every prop means something on every kind, so each kind says which values a theme may give each
prop (`Kind::knobs`, a `Knobs` of allowed values per prop; an empty list keeps the built-in). A role
takes its group's knobs unless it names its own (`SurfaceRole::with_knobs`). A theme file that sets
anything else is an error naming the role, the prop and what it takes, and the creator's Roles step
shows only the allowed props and values:

| Kind | Fill | Outline | Shadow | Shape | Hover | Selected |
|---|---|---|---|---|---|---|
| bar | all | yes | none, contact | fixed | fixed | fixed |
| panel | fixed | fixed | fixed | fixed | fixed | fixed |
| pane | all | yes | all | fixed | fixed | none, ring |
| card | all | yes | all | square, round | fixed | fixed |
| floating | tint, solid, glass | yes | all | all | fixed | fixed |
| button | all | yes | all | all | all | all |
| item | all | yes | none, contact | all | all | all |
| field | all | yes | all | all | all | none, tint, lift, ring, bar, glass |
| label | all | yes | none, contact | all | fixed | fixed |
| rule | fixed | yes | fixed | fixed | fixed | fixed |

`popover`, `dialog`, `sheet`, `pane.float` and `inline-panel` take only square and round, since
a pill box that size is a stadium; `menu-item` casts no shadow. `pane.chip`, `pane.display-panes`,
`pane.drop-zone` and `titlebar.status-chip` also take hover and selected (`Knobs::pressable`), and
`window`, `titlebar`, `pane.header` and `sidebar` take nothing (`Knobs::FIXED`): a header
fill ignored the pane's corner radius and stacked glass on a glass pane, and the sidebar stacked
glass on the window's blur. That leaves nothing in `panel` a theme can change, so the kind is
fixed too, and `bar` reaches only `phone.bar`. A theme's patch for a kind or group reaches
each role only through that role's knobs, so a `floating` pill leaves dialogs round and a `panel`
patch leaves the window alone. A strip's corners belong to what holds it, so bars and panels have
no shape, and a pane's radius is the app's pane corner setting, which ignores the role's. Any
selected highlight but none lights the active pane the same way, so a pane offers only none and
ring. A field's selected is how it looks while focused (`PaintState::Focused`), or while a
select's menu is open; accent would hide the caret and the selection, so fields leave it out.
Fields alone also take `edge`, their built-in: the outline turns the focus color, where the field
draws one and is not glass. `none` leaves a focused field as it rests.

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
3. **Controls cast shadows only on the window.** Buttons, items, fields and labels on a panel or
   a popover keep their outline and drop the shadow, since that surface already stands off the
   window. Floating things step their fill off the window's color wherever they sit.
4. **Highlights never change a border's width.** A role that draws an outline, or lifts on hover
   or selection, reserves the outline width in every state and only recolors it.
5. **Fills composite opaque under an outer shadow.** ala draws a shadow under the whole box, so a
   translucent fill over one turns muddy. A non-glass surface that casts a shadow, or shows a
   ring, blends its fill over its ground first.
6. **Ink follows the ground.** `Layer::ground` carries the composited fill, the ink, the elevation
   and whether it is glass, so text, keycaps and nested controls read on whatever the role chose.
7. **Inspect.** `Theme.inspect` names one role, group or kind id; every surface drawn through that
   role, or through any role in that group or kind that its patch still reaches
   (`Theme::group_reaches`: some prop the role's knobs open that no theme patch closer to the
   role sets), adds a 2px magenta ring just inside its edge, so a translucent fill
   never shows it through. `Theme.inspect_strength` scales its alpha, which the creator fades to
   flash it.
8. **Capsules go pill at the largest radius.** Past a 24px radius (`Theme::control_radius`) a
   round role that is a capsule takes a full pill. `SurfaceRole::capsule` says which, resolved
   from the role, then its groups, then its kind: buttons and fields are capsules; menu items
   (menus, palettes, pickers, recent pages, phone rows) and multi-line fields are not, so a menu
   highlight stays a rounded row concentric in its list; the `row` and `tab` groups (sidebar rows
   and entries, window pills, browser and tray tabs), badges (status chips
   included), pane chips, drag chips and `pane-stack` (the icon tiles in flat sidebar rows and
   window pills, which turn into circles) are. Everything else, keycaps and other tiles included, keeps
   the radius, which the window's adaptive corners hold under half its height.

`Theme::casts_shadow(role)`, `shadows_of(shadow)`, `contact_shadows()`, `float_shadows()`,
`shape_radius(role, shape)` and `ground_of(surface)` expose the same answers to code that needs
one value.

### Theme file

A theme with glass chrome over flat pill buttons, colors left out:

```json
{
  "name": "Glass chrome",
  "materials": {
    "radius": 18,
    "outline": 0.5,
    "row-inset": 5,
    "primary": "accent",
    "glass": { "preset": "regular", "blur": 19.7, "tint": "#ffffff87" }
  },
  "roles": {
    "chrome.button": { "fill": "glass", "shape": "pill", "shadow": "contact", "hover": "tint", "selected": "tint" },
    "button": { "fill": "tint", "outline": false, "shadow": "none", "shape": "pill" },
    "tab": { "shape": "pill", "hover": "tint", "selected": "glass" },
    "agent.composer": { "fill": "glass", "shadow": "contact", "shape": "round" },
    "sidebar.row": { "hover": "tint", "selected": "tint" },
    "rule.bar": { "outline": false }
  }
}
```

Kind names, group ids and role ids share the `roles` map. An id `penna::roles::find` does not know is an
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
  built-in. Roles carry their own built-in patch; since 2026-10-14 every built-in applies before
  any theme patch, as above.
- **Kinds.** 19, not 23: `toggle`, `slider`, `scrim` and `scrollbar` are not kinds. Switches and
  sliders paint from tone and emphasis, scrims fill with `theme.scrim`, and scrollbars keep their
  own paint.
- **Role kinds.** Lanes changed some kinds once they drew the real pieces: `pane-stack` (proposed
  tile) and `pane.drop-zone` (proposed scrim) are chips, so their washes deepen and their edges
  follow the row or pill they sit on; `inline-panel` and `code` (proposed card) are popovers, so
  their fill steps off the window as before, with `code` a solid fill without an outline or
  shadow; `agent.option` (proposed tile) was a button with a red wash on a danger option until 10-15,
  when it joined `menu-item` so answer options look like the pane's other lists; `titlebar.window-controls` (proposed button) is a square icon button; `grabber`
  (proposed tile) is a pill badge.
- **`pane.display-panes` and `pane.drag-chip`** are popovers, as their old floating look was, so a
  theme's chip patch does not reach them.
- **Broad kinds (2026-10-14).** The 19 kinds above were narrow enough that a theme had to say the
  same thing five times (popover, dialog, sheet, toast, tooltip) and still missed pane chips. They
  became 10 kinds with groups under them, and every role kept its resolved Default style: popover,
  dialog, sheet, toast and tooltip became groups of `floating`, which also took pane chips, drop
  zones, floating panes and inline panels; `icon-button` and the old `row` (renamed `menu-item`,
  the rows that run something) became groups of `button`, with the phone keys; `tab`, `tile` and a
  new `row` (rows that show which one is current) became groups of `item`; `badge` and `keycap`
  became groups of `label`; `chip` is gone; `code` is a card. `agent.control` split into
  `agent.picker` (a field), `agent.tray` (a tab) and the queue chip, now a quiet button.
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
- **`pane.selected`** decides whether the active pane stands apart from the others: `none` draws every pane alike.
- **Not wired.** Reduce transparency and increase contrast do not change materials yet; renderers
  that cannot sample the backdrop (WebGL, DirectX) still get the glass's solid fallback.

## Role inventory (2026-10-13)

Four read-only sweeps covered every painted piece in `crates/zz`, `crates/zz-ui`,
`libs/penna` and `clients/app`: about 450 pieces, collapsed below into roles. "Where" names
the shared function that drew most instances at the time of the sweep; "Drift found" names what the
role was meant to remove, and what became of it. Groups are as built after the 2026-10-14 regroup. Paths: `ui/` is
`crates/zz-ui/src`, `kit/` is `libs/penna/src`, `zz/` is `crates/zz/src`, `app/` is
`clients/app/src`.

### Window and title bar

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `window` | panel | window plane (solid or blurred), Linux frame shadow and edge | `ui/shell.rs:36`, `zz/window/frame.rs:113` | desktop 93% under blur, thin client opaque; nothing draws through the role yet |
| `titlebar` | bar | title bar strip, the control strip above panes | `ui/shell.rs:137` | |
| `chrome.button` | icon-button | sidebar toggle, settings gear, tree/flat eye, iPad search, window overflow | `ui/navigation.rs:101` | already one function |
| `titlebar.window-tab` | tab | window pills with hover close and bell/activity dots | `ui/navigation.rs:162` | active looks the same as hover; Default still lifts both, a theme can tell them apart (`selected: glass` turns the active pill to glass) |
| `titlebar.status-chip` | badge | update "v1.2", host name (the agents chip, "2 running", went on 2026-10-24) | `ui/navigation/status.rs:54`, `zz/status_bar.rs:139` | three treatments, hand-made dot; fixed, one chip |
| `titlebar.window-controls` | icon-button | minimize, maximize, close (Windows, Linux) | `kit/title_bar/mod.rs:214` | |

### Sidebar

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `sidebar` | panel | the column, slideover, bottom fade | `ui/navigation.rs:305`, `zz/app_shell.rs:286` | slideover fade band; fixed (see Bugs) |
| `sidebar.row` | row | tree rows (host, session, window, pane), Settings section rows, Back | `ui/navigation.rs:337`, `ui/settings.rs:145` | Settings rows were a 28px Button path, tree rows 32px; fixed, Settings section rows and Back draw as `sidebar.row` |
| `sidebar.entry` | row | flat sidebar two-line window rows | `ui/navigation/flat.rs:455` | always lit, tree lit only with focus |
| `sidebar.row-action` | icon-button | + × … and layout on hovered rows, + on session labels | `ui/navigation.rs:454`, `ui/navigation/flat.rs:425`, `:817` | four versions with three inks; fixed, one function and one ink, muted until pointed at |
| `pane-stack` | tile | pane tiles in flat rows and title bar pills, open or at rest | `ui/navigation/pane_stack.rs:171` | |

### Panes

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `pane` | pane | tiled pane (gaps or flush), active glow, inactive dim, zoomed, waiting, phone page | `ui/pane.rs:187` | dim rules differ desktop vs thin client |
| `pane.float` | floating | floating and popup panes, the confirm-before box | `ui/pane.rs:332` | confirm box uses the Panel fill |
| `pane.pip` | pane.float | every float that is not modal, as a PiP card casting the float shadow; focus does not change it | `ui/pip.rs` (`PipSurface`) | new 10-18 |
| `pane.header` | bar | terminal and agent title rows, the browser's two-row header | `ui/pane.rs:47`, `ui/agent.rs:3600`, `ui/browser.rs:57` | three divider treatments, wrapper copied per client; fixed, one `pane_header_frame` and the `rule.bar` divider |
| `pane.header.button` | icon-button | split and the other buttons in pane headers | `ui/pane.rs:19` | the X in a pane title used it until 10-16, so a theme's hover lift put a disc behind it; it is ink only now, like a browser tab's X |
| `pane.header.lens` | pane.header.button | the agent lens toggle in a terminal header | `ui/pane.rs` | tinted while the lens is open (selected `tint`), so the open state reads at a glance; before 10-24 it was a `pane.header.button`, whose selected highlight is none |
| `pane.header.title` | button.quiet | pane titles in terminal, agent and browser headers: drag to move, X closes; tinted on hover and when selected | `ui/pane.rs:46` (`PaneTitle`) | the grip and Close were separate buttons; since 10-15 the title is both, and since 10-16 in browsers too |
| `pane.chip` | floating | Dead, Waiting, Sync, Copy mode, status line, Unzoom, vim mode, "Select an element" | `ui/pane.rs:729`, `zz/editor/view.rs:557` | four variants; fixed, one chip with a contact shadow |
| `pane.display-panes` | floating | display-panes number chips | `ui/pane.rs:796` | hover invisible at outline 0; fixed (see Bugs) |
| `pane.find-bar` | popover | terminal find bar (and the browser's once it exists) | `zz/terminal/view.rs:660` | copied per client; fixed, one `terminal_find_bar` in `zz_ui::terminal` |
| `pane.drop-zone` | floating | drop preview, armed veil | `ui/pane.rs:425`, `:465` | hand-drawn edge |
| `pane.drag-chip` | floating | chip following the pointer while dragging | `ui/pane.rs:479` | |
| `pane.picker` | menu-item | Terminal, Browser, Editor and Agent in a freshly split pane | `ui/pane.rs` (`pane_picker_row`) | drew as plain `menu-item` until 10-16, so styling it restyled every menu; its own role since |

### Agent pane

The Agent pane was removed on 2026-10-23; these roles now style the agent lens over a Claude Code
or Codex terminal ([agent lens](/designs/agent-lens.md)). The lens has no status pill and no
pickers, so `agent_status_pill` and the composer pickers are deleted. Their roles, `agent.status`
and `agent.picker`, were retired on 2026-10-27: they are listed in `roles::RETIRED`, and a theme
file that still names one loads with the entry dropped. The agent history picker
(`ui/agent/history.rs`) went too; its delete banner was the last `picker.notice`, which since
2026-10-27 draws the chooser's prompt and key help strips.

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `agent.bubble` | card | your messages | `ui/agent.rs:2416` | |
| `agent.composer` | field.multiline | the lens's composer pill, a pill in every theme (a taller draft keeps the one-line corner) | `ui/agent/composer.rs` (`Composer`) | took over `chat.composer` 2026-10-26 |
| `agent.send` | button | send, stop and queue, and the composer's `+` | `ui/agent/controls.rs` (`composer_action_button`, `composer_add_button`) | stop had no fill, send was solid; fixed, stop takes send's fill. Moved from `icon-button` to `button` 2026-10-26: themes that turn icon-button fills off (macOS, Yaru) left `+` and stop bare. Send and stop draw prominent, `+` and queue neutral, all on the composer's ground |
| `agent.tray` | tab | plan and tasks chips that open the tray over the composer (the queue chip is a quiet button) | `ui/agent/tasks.rs:147` | plan chip used Selected, tasks chip nothing; fixed, the open plan chip shows the tab's selected lift |
| `agent.attachment` | card | image tiles in bubbles and the composer, their remove × | `ui/agent.rs:2105` | × has no backing |
| `agent.option` | menu-item | answer tiles in question and permission cards, Show terminal on the card for an approval the lens cannot answer | `ui/agent/presentation.rs:239` | a row like the pane picker since 10-15; no fill at rest |
| `agent.jump` | icon-button | jump-to-bottom button over the transcript | `ui/agent.rs:1025` | |
| `agent.action` | icon-button | copy on answers, code blocks and diagrams, rewind on prompts | `ui/agent.rs:2464` | washed and lifted under the pointer, and the copy buttons always showed; since 10-15 no fill in any state, shown only while their message, block or figure is pointed at |
| `inline-panel` | floating | panels raised inside a pane: question and approval cards, plan and tasks tray, slash menu, side answer, failure alerts (agent, browser, editor) | `ui/agent/presentation.rs:211`, `ui/agent/tasks.rs:227`, `kit/feedback.rs:31` | all wear the floating popover look while sitting in the layout; kept on purpose, now as a role a theme can change |

The agent's step rows (the trace line, thoughts and tool calls) had `agent.trace`, a `row`, until 2026-10-15. They draw no fill now: their ink is muted and a row you can open brightens it under the pointer, so the role had nothing left to style.

### Driver chat (retired)

The Driver Agent's chat, under the sidebar's list, in the command palette's Driver mode and in a
phone drawer, went with the Driver Agent on 2026-10-28. Its roles `chat.bubble`,
`chat.bubble.mine`, `rule.chat`, `phone.driver-card` and `drawer.driver` are listed in
`roles::RETIRED`, and a theme file that still names one loads with the entry dropped.

### Reading content (markdown in agent replies)

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `code` | card | code blocks and their header, inline code | `kit/text/node.rs:719`, `kit/text/inline.rs:213` | inline code went square in paragraphs with images; fixed (see Bugs) |
| `figure` | card | Mermaid frames, markdown tables | `ui/agent.rs:3033`, `kit/text/node.rs:1544` | three card fills across bubble, tile, diagram; fixed, figures have no fill of their own and tables wear the figure look |

### Browser pane

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `browser.tab` | tab | browser tabs | `ui/browser.rs:460` | |
| `browser.button` | icon-button | back, forward, reload, picker, more, new tab | `ui/browser.rs:526` | |
| `browser.address` | field | the address bar and its site button | `ui/browser.rs:183` | |
| `browser.suggestion` | menu-item | recent pages on the blank page | `ui/browser.rs:623` | |

### Settings

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `settings.card` | card | grouped setting cards | `ui/settings.rs:1343` | stitched per row with hand-clipped shadows; still rounded only at a run's ends with the shadow clipped per row, now from the role's layer |
| `settings.tile` | tile | theme, mode and app icon tiles with the chosen ring | `kit/settings/appearance.rs:672` | 1px hand ring, 2px `chosen_ring` elsewhere; fixed, the role's 2px selected ring |
| `settings.preview` | card | preview wells: panes, status bar, terminal, config editor | `ui/settings/panes_preview.rs:186`, `ui/settings/status_bar_preview.rs:157`, `zz/config/settings.rs:1761` | four recipes; fixed, one `settings_preview` well |

Text fields, number fields, selects, switches and buttons on setting rows are plain kinds, and the rows themselves draw nothing of their own: every control on a card sits on `settings_card_ground`. `settings.row` (a whole row as a button, pressed on touch) was dropped on 2026-10-15 because no app made a row clickable.

### Phone (narrow thin client)

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `phone.bar` | bar | 44px nav bars (settings, sheet header) | `kit/compact/bar.rs:205` (`compact_nav_bar`), `kit/compact/header.rs:61` (`folding_title_bar`) | three hand-made 44px headers; fixed, one `compact_nav_bar` |
| `phone.pill` | button | the pills floating at the bottom of the phone, concentric with the screen's corners (`PillPlacement`): the pane pill with its icon tile and page dots, the host pill, the Hosts page's Settings button, the pill the keys sit in | `kit/compact/bar.rs:91` | took floating glass while in the layout; fixed, a solid fill without an edge |
| `phone.button` | icon-button | back, close, gear | `kit/compact/bar.rs:192`, `app/app/settings.rs:566`, `app/app/compact/overview.rs:668` | three sizes, two bare divs; fixed, one 44pt `compact_bar_button` with a 20pt glyph |
| `phone.key` | button | key row keys, ctrl, alt and prefix keys, arrow pad; flat on the key pill until latched or held, concentric in it | `kit/touch/key_row.rs:104` | |
| `phone.card` | tile | the overview's bento tiles: hosts on the Hosts page, windows on a host's page | `kit/compact/bento.rs` (`bento_tile`) | was one card per pane in a row per window until the 2026-10-26 drill-in |
| `phone.mini` | tile | the panes inside a window's tile, where the window splits them, on the terminal's background; ringed in the accent on the pane the window opens to | `ui/compact/bento.rs` (`mini_pane`) | new 2026-10-26 with the drill-in overview |
| `phone.add` | tile | the Add host tile at the end of the Hosts page (until 2026-10-26 the New pane tile that ended each overview row) | `kit/compact/bento.rs` (`bento_add_tile`) | shared `phone.card` with the live cards and was narrower than them; split out |
| `phone.row` | menu-item | sheet options, all-bindings list, settings index, New window | `kit/sheet.rs:126`, `kit/touch/which_key_list.rs:60`, `ui/settings.rs:222` | four corner rules; fixed, one role |
| `drawer.hosts`, `drawer.new`, `drawer.add-host`, `drawer.new-pane`, `drawer.tabs`, `drawer.bindings` | drawer | each phone drawer, so a theme can set one's corner smoothing apart from the rest | `app/app/compact/overview.rs`, `app/app/picker.rs`, `app/app/browser_pane.rs`, `app/app/compact.rs` | no way to tune one drawer apart from the rest; macOS sets `drawer` smoothing 2 |
| `grabber` | badge | sheet grabbers | `kit/sheet.rs:505` | two sizes and colors |

### Overlays

`popover`, `row`, `dialog`, `sheet`, `toast`, `tooltip` and `keycap` are kinds, not zz roles; the
overlays name these roles over them:

| Role | Group | Covers | Where | Drift found |
|---|---|---|---|---|
| `palette` | popover | command palette, path picker | `ui/command.rs:152` | no scrim, unlike choosers |
| `picker` | dialog | file picker, choose-tree and choose-buffer | `kit/picker.rs:58`, `ui/chooser.rs:118` | |
| `picker.notice` | bar | the chooser's prompt and key help strips | `ui/chooser.rs` (`ChooserModal`) | the strips drew as the bare `bar` kind until 10-27; the role became a bar then, so the Default look kept |

What the inventory found on the kinds themselves:

| Kind | Covers | Where | Drift found |
|---|---|---|---|
| `popover` | context and dropdown menus, tmux display-menu, select dropdowns, omnibox, model picker, pane-stack plate, key popovers, image preview | `kit/menu/popup_menu.rs:1184` | |
| `menu-item` | rows in menus, the palette, pickers, choosers, omnibox, slash menu, new-session actions | `kit/menu/menu_item.rs:97`, `kit/list/mod.rs:123`, `kit/picker.rs:134` | three corner rules; fixed, `row_corners` everywhere. Menus still move their selection on hover, lists hover with a wash |
| `dialog` | alerts and prompts (ssh, add host, restart daemon, import) | `kit/overlay/dialog.rs:499` | |
| `sheet` | which-key, phone bottom sheets | `kit/which_key.rs:419`, `kit/sheet.rs:355` | |
| `toast` | notifications, update offer, disconnected banner, pinch HUD | `kit/overlay/notification.rs:69`, `app/app.rs:2238`, `kit/compact/hud.rs:18` | banner and HUD used the popover look; fixed, both draw as toasts |
| `tooltip` | floating | `kit/tooltip/mod.rs:53` | |
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
| `rule.tree` | tree indent guides | `kit/list/tree.rs:29` |

### Not roles

- Setting form controls and other one-off widget uses: they follow their kind.
- Text-only pieces: labels, section headers, empty-state copy, captions.
- The terminal grid: cursor, selection, search hits and links paint from the terminal palette.
- Native pieces: macOS traffic lights, the iPad key bar, the iOS Done bar.
- Invisible hit targets: split and float grips, the display-panes target.

## Theme creator

The theme creator (`penna_storybook::creator`, on penna's storybook page and on every app's page
built on its harness) is four steps over the same live preview, then an App tab for the app's
knobs (zz's pane settings) and the preview around the showcase, which are not theme:

1. **Colors**: the mode with contrast and the tint, light and dark palettes, accent, font.
2. **Materials**: presets, shape (radius, smoothing, global or picked smoothing, outline,
   built-in or picked outlines, row inset, primary), glass, built-in or picked shadows, and both
   shadow materials.
3. **Roles**: every kind, each with its groups and roles indented under it. Each role shows the
   props and values its knobs allow; an unset prop shows the value it inherits and from which
   kind or group, and whether that is the built-in or this theme's patch. Setting a prop writes
   only what differs into the theme's `roles` patch. Picking a role, group or kind shows only the
   regions that draw it and flashes `Theme.inspect` over its surfaces for about a second. The
   flash is an inset ring, which glass surfaces do not draw, so under a glass theme such as macOS a glass role
   (the PiP bar, for one) shows no ring.
4. **Export**: JSON with only what differs, and Import.

Every built-in starts the creator with its materials and roles. The knobs panel resizes from its
left edge (double-click resets it); the browser remembers the width. Clicking the preview to open
a role is not built.

The preview is the catalog's plates: regions that paint real UI. penna's page shows penna's five
(an app window with the sidebar and the appearance rows, controls, floating surfaces, content and
the phone chrome); zz lists its eleven instead. An app makes its own roles editable by registering
them (`penna::register`, which takes the roles, the ids that are gone and the app's theme layers) in its
catalog's `register` function and adding a plate that draws them to the catalog's `plates`, next to penna's
if it wants the kit's surfaces in view. Its roles show in the Roles step under their parents from
`penna::roles::all()`, picking one shows the plates that drew it (each section host records the
roles it paints, so no plate lists them), Export writes their patches, and Import drops retired
ids and refuses unknown ones with the same "unknown role" error as any theme file. The recipe is
in [storybook](/playbooks/storybook.md).

## Settings

Settings > Interface sets the materials all at once by picking a theme under Themes
(`interface-theme`) and only the radius on its own (`widget-corner-radius`, first under Advanced).
The Materials group that held one key per material (`widget-outline-width`,
`widget-shadow-strength`, `widget-row-inset`, `widget-primary`, `widget-glass`) was removed on
2026-10-15 and its keys retired. Roles live in theme files, made in the creator and
brought in with Import, or saved from what is on screen with Save. `widget-shadows`,
`widget-selection` and `widget-field` are gone into role props, and `shadow-strength` points to
`interface-theme`. Settings' Style row and `interface-style`, which drew a theme with material
presets, were removed later the same day: a look is a whole theme now; see [interface themes](/designs/interface-themes.md) and
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
5. Settings' Look group became the Materials group (itself replaced by the Style row on
   2026-10-15, and that by the Themes strip the same day).

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
