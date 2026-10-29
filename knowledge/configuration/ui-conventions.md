---
type: Configuration
title: UI design conventions
description: The component, palette, and styling rules that keep zz application chrome consistent and theme-aware.
resource: crates/zz-ui/src/command/palette_view.rs
tags: [ui, ala, zz-ui, theme, chrome, clippy]
timestamp: 2026-10-23T00:00:00-03:00
---

# Overview

**`penna` (`libs/penna`) is zz's base component layer**, re-exported by `zz-ui`. It is a full fork of `gpui-component`, not a
dependency on it: commit `7811a47` vendored the widget set and removed the upstream crate from the
workspace entirely. The fork revision and the per-module port notes live in `libs/penna/UPSTREAM.md`;
read that file before widening or diverging the surface.

Application chrome must use zz-ui's components and theme vocabulary rather than maintaining a
parallel widget set or color palette. Import `zz_ui::ActiveTheme` (not `gpui_component::ActiveTheme`;
that path does not resolve) and read colors from `cx.theme()` at the point where an ala element
is rendered.

The house-style reference is `crates/zz-ui/src/command/palette_view.rs`. It demonstrates `cx.theme()`
derivations alongside the `Input`/`InputState`, `ListItem`, `Kbd`, `Keycap`, `Badge` and `Check` widgets.
The browser client in `clients/web` (`just web run`) uses the shared ala components outside the desktop app.

# Surfaces and roles

Every painted piece of chrome names a surface role (see [surface roles](/designs/surface-roles.md)):
a kit kind or group such as `button`, `menu-item`, `floating` or `popover`, or a zz role in one from `zz_ui::roles`
(`crates/zz-ui/src/roles.rs`) such as `chrome.button`, `sidebar.row` or `pane.chip`. The role
decides the fill, outline, shadow, shape and the hover and selected highlights; the theme's
materials decide what those look like, and the theme can change any role by id. To paint one:

- A kit widget draws as its kind. Give it `.surface_role(roles::X)` when it is a specific piece:
  `Button`, the fields (`Input`, `NumberInput`, the `Select` trigger, the `CodeEditor` frame), list,
  menu and sheet rows, popovers, menus, dialogs, sheets, toasts, tooltips, badges, key caps,
  separators and the window controls all take it.
- Any other element takes `.surface(role, cx)` at rest, `.surface_at(role, state, cx)` in a
  `PaintState`, `.surface_hover(..)` to follow the pointer, or `.surface_interactive(..)` on an
  element with an id to press too. `Surface::new(role)` adds a tone (`.tone(Tone::Danger)`), marks
  the main action (`.prominent(true)`), or says what it sits on (`.on(ground)`).
- When code needs the parts, `cx.theme().layer(surface, state)` returns the `Layer`: fill or glass,
  border and its width, shadows, text, radius, and the ground what sits inside paints on.
  `.layer(&layer)` paints it, and `theme.ground_of(surface)` is the resting ground.
- A divider is `.rule_on(roles::BAR_RULE, Edge::Bottom, cx)` or another `rule.*` role; it draws
  only while the theme keeps that role's outline on.
- A list row is `zz_ui::picker::row_surface(element, role, state, height, cx)`, rounded with
  `row_corners`.
- A new piece of UI that a theme would want to style apart from every existing role gets its own
  role. Add a `SurfaceRole` to `crates/zz-ui/src/roles.rs`: an id spelled the way a theme file
  writes it (`region.piece`, lowercase with dots and dashes), the kind it is, a label and a
  plain-words description for the theme creator, and a `StylePatch` holding only what differs from
  the kind, so the Default theme draws it as designed. List it in its region in `REGIONS`: that list
  is what theme files are checked against and what the theme creator shows. A piece that looks and
  behaves like an existing role reuses that role instead.
- Do not hand-draw a border, a box shadow or a corner radius. Clippy rejects them outside the kit
  (rule 3).

# Rules

1. Every application-chrome color comes from a `cx.theme()` root or a `Colorize` derivation of one.
   The palette has six roots (`background`, `foreground`, `accent`, `success`, `warning`,
   `danger`), the fixed per-mode `scrim`, the `shadow_color` every drop shadow is drawn in and an
   optional `on_accent` text color, in `libs/penna/src/foundation/theme_color.rs`; the
   interface theme sets them per mode. Panels, hover fills, muted text, and
   focus rings are derived at paint time by `Colorize` in
   `libs/penna/src/foundation/color.rs`. Outside the kit, chrome names what it draws
   instead of mixing it (see [Design system](/designs/design-system.md)): a filled, tinted or
   outlined widget draws `theme.paint(tone, emphasis)`, a plane `theme.surface(elevation)`, anything
   with a fill, edge, corners or shadow its surface role (`.surface(role)` or a widget's
   `.surface_role(role)`), a rule `.rule_on(role, edge)`, a pane `theme.pane_background(fill)`, and
   quieter text `foreground.muted()` or `.faint()`. Do not color-match an old literal.
2. Check the zz-ui widget set before hand-rolling a control. A custom ala
   element is appropriate only when a component cannot preserve required input routing,
   rendering, or interaction behavior. When a fork-local widget must diverge from upstream, record
   why in `libs/penna/UPSTREAM.md`.
3. `clippy.toml` enforces the chrome-color rule by disallowing calls to `ala::rgb`, `ala::rgba`,
   and `ala::hsla`, the design-system rule by disallowing `Hsla::opacity` and the mixing
   transforms of `Colorize` (`raised`, `washed`, `wash`, `fill`, `outline`, `subtle`, `glow`,
   `hover`, `active`, `on`, `floating`), and the shape rule by disallowing
   `ala::Styled::border` with its sized and per-side forms, `shadow_2xs` through `shadow_2xl`,
   the type `ala::BoxShadow`, and the kit helpers that would get around the roles
   (`StyledExt::surface_edge`, `surface_outline` and `chosen_ring`, `lifted_shadow`, and
   `Theme::control_radius`, `menu_radius` and `row_radius`), so edges, corners and shadows come
   from surface roles and follow the theme. The kit builds the engine from them, so
   `libs/penna/clippy.toml` keeps only the raw-color ban. Keep any exemption narrowly
   scoped, as an `#[allow(clippy::disallowed_methods, reason = ..)]` (or
   `clippy::disallowed_types`) on the one function, and say why the color or shape is not
   application chrome: a preview of another theme, page CSS, a pane effect with its own strength
   knob, a border colour tmux configured.
4. Do not scatter branding or palette literals through views, fixtures that model application
   chrome, or component state branches.
5. **Menu, picker, and list highlights use the same control treatment.** A selected row is its
   row role's `selected` highlight: the `row` kind fills with the accent and the theme's text on
   it, a theme can make it a tint instead (Glass does), and it casts no shadow. Secondary text
   takes the row's muted ink (`picker::muted_on`). Rows round from their role's shape: `round` is
   the theme radius minus 4px while rows are inset, and square at a 0 row inset. `row_corners`
   holds that radius to 40% of the row's height with fixed smoothing 2.5, and leaves a `pill` row a
   pill; `ListItem`, popup menu, select and model picker rows and the directory, session, file and
   history picker rows all use it. `zz_ui::picker::row_surface` draws a row of a given height
   through a role with its hover and press, and `list_row` builds one as the `row` kind.
   Directory and session rows share
   26px heights and 12px labels. Custom menu content receives the highlight state so
   descriptions, counts, and timestamps brighten to the normal foreground.
   Their requested radius reaches the renderer without adaptive compression; the cap keeps
   them short of full pills. Menu containers keep the window's adaptive squircle shape. Open submenu parent rows use a dim neutral highlight; leaf selections keep the
   accent. Selected shortcut labels and icons brighten to the full foreground. The command palette matches dropdown typography with 12px medium labels, 10px descriptions, 16px line heights,
   26px selectable rows, and 2px gaps. It is at most 560px wide, with 8px list gutters,
   a 40px bare Input, 12px search text and inline Badge pills, and a 10px footer. Matched characters
   use semibold foreground; breadcrumb prefixes stay muted until selected. Status dots are the
   shared 6px `navigation::status_dot`: a host's is success when online, warning while waiting,
   muted while running and faint when offline, and a session or window with a running agent shows
   one in the row's muted ink. All palette
   colors and corners follow the current Chroma theme. `PaletteRow::icon` and
   `PaletteRow::expanded` supply tree icons and disclosure state; `command_palette_tree_entry`
   gives the disclosure a toggle handler separate from row activation. Workspace and Navigate
   render selectable session/window/pane trees with these shared rows. Search flattens matching
   descendants into breadcrumbs. Shared row builders also render daemon command and value prompts.
6. Use one translucent signal instead of competing fills. Sidebar rows draw their hover and lit
   states through `navigation::workspace_row_fill` as `sidebar.row` (tree) or `sidebar.entry`
   (flat), and window pills as `titlebar.window-tab`: in the Default theme a lift, a translucent
   wash with the outline that preserves the desktop blur. Where you are or what is on (the active
   session row, the open Settings page, a toggle button that is on) is the role's `selected`
   highlight, a lift or a tint, and never the accent unless a theme gives the role one: the accent
   belongs to the cursor in a list you pick from (rule 5). Neutral buttons take hover, pressed,
   and current from their role (`button`, `button.quiet` for ghost and outline, `icon-button` for
   ghost icon-only). This covers toolbar icons, Back, and agent footer controls without
   per-button fills. A control casts a shadow only on the window and only when its role's `shadow`
   is on; one that does composites its fill over its ground (see the design-system doc).
   Tree fills keep a 1px vertical inset,
   so adjacent rounded rows never merge into one slab. The final Add host action is the exception:
   its row stays unpainted and only its label moves from muted to foreground on hover.
7. A control's edge is the outline material (0.5px in the built-ins that draw one, none at 0) in
   10% of its ground's ink, drawn when its role's `outline` is on or it lifts, with the contact
   shadow when its role casts one and it sits on the window. Fields draw their border through
   `field_chrome`, and cards and floating surfaces draw the border color at the same width.
   Settings stacks draw `settings.card` around the whole group, with `rule.list` separators
   between entries. The engine reserves the border width in every state and changes only its
   color, so a hover or a lift never moves what is inside. Inputs and number fields reserve at
   least a 1px border, in the paint's focus color when focused if the theme draws outlines and the
   field is not glass (a glass rim is its own edge).
   Settings value pickers use Button triggers and PopupMenu rows, including font, search engine,
   and multiplexer options. The shared Select API keeps their values and confirmation events;
   it has no separate trigger or row styling.
   Use `Button::flat()` for actions embedded in another control, including tree-row
   close buttons, browser tab close buttons, input clear buttons, and number steppers. These retain the keyboard focus indicator without another raised edge. Tree-row
   actions and browser tab close buttons also use `Button::text()` so hover only brightens their
   foreground, without a button fill. Other embedded actions keep their wash.
   Gapped panes cast the `pane` role's shadow (the contact shadow in every built-in) and draw their frame
   only in glass: no border, the glass edge (glint, fresnel, thin contour) is the frame, tinted
   with the pane's own color, its rim the theme's glass as the theme defines it (glint width,
   specular, fresnel, edge, noise) and its corners the theme radius plus 3.5
   (`Theme::pane_radius()`). A theme without glass (Default) draws a hairline the outline's width
   instead (`Theme::pane_edge_width()`), none when the outline is 0. The
   continuous app background sits beneath both pane and shadow. Flush panes have no
   outer shadow. Only shared divider segments touching the active flush pane use the accent
   color; exterior edges stay unoutlined. The settings preview follows the same rule.

   The local renderer makes blurred shadows follow the pane's corner smoothing, including
   its small 2px control shadow. Circular and fully rounded surfaces retain circular shadows.

   The active gapped pane shows focus through its fill and rim:
   `Theme::pane_frame` washes its fill toward white (5% in dark, 30% in light) and veils the other
   panes' fills toward black (18% in dark, 5% in light), and the others' rims, the glass glint or
   the hairline without glass (`Theme::pane_edge_color`), show at 70%. Their content dims by
   `pane-inactive-opacity` as before. Settings > Panes > Selected pane contrast scales all of it
   from 0–200%; 0 draws the active pane like the others. `pane-glow-strength` stores the factor
   from 0–2, defaulting to 1. It is the draw the pane already makes, with no layer of its own.
   Floating panes still edge the active one in the accent (`pane_border_color`).
   `PaneChrome::active` and `PaneChrome::frame` share this across desktop, thin client, and preview
   callers. Without gaps, pane content roots paint their own backgrounds using the configured pane
   opacity; under a frame they paint none.

Settings > Panes scrolls its three-pane preview with the controls, like the Terminal preview. The shared
`settings::panes_preview::PanesPreview` draws a terminal, Claude Code in a terminal with its lens
button, and a browser, inside `pane_surface` and `pane_split_surface` at the workspace's real size:
`terminal_pane_header` and `BrowserHeader` with its tab row and toolbar, full-size logical-pixel
margins and corners, and the theme's pane frames over the workspace background. Inactive samples
dim the way the workspace dims them, the terminal's header and text and the browser's chrome, with
no scrim. An occluding layer over each sample makes it one click target, so its header buttons stay
inert; clicking a sample changes only the preview selection and moves the accent edge. The Browser
sample keeps its page opaque. Desktop pane edits and resets refresh the saved configuration
immediately, including valid numeric input while typing; the file watcher still handles external edits.

Settings > Status bar scrolls its live preview with the controls. The titlebar holds only the
sidebar toggle, with no divider; settings lives in the sidebar footer. With the sidebar open the toggle
sits at the top-right corner of the sidebar, and the navigation fades into the footer through the kit's
`bottom_fade` (40px toward the sidebar's ground, solid for its last 12px) while rows remain below. The agent button shares the active window pill’s background,
theme border, shadow, and 32px height (a step up from the sidebar's 28px buttons), with widths sized to their contents. The titlebar, a pane header tall (34px) plus the pane margin above and below it, puts each pill on the pane headers' center line. Window pills always align left, and the right edge shows an
agent status dot and summary with a pane-selection menu. Time/date and alignment controls are absent. Window pills share the
production pane deck, the flat sidebar's stack drawn at 20px (`navigation::pane_stack`): the focused pane's tile with its glyph in front and at most two blank tiles fanned out behind it, each tile outlined on its own (dark in both modes and lit like a program icon, with no theme hairline: a gray line outside and a white rim inside in light mode, a dark line and a faint rim in dark mode), so the one in front reads against the one behind,
5px apart inside a vertically centered 32px pill. Pointing at it opens every pane in a row drawn
on the pill itself, with no plate of its own: the name under the tiles is hidden and fades back in
past them over `LABEL_FADE` (`PaneStack::beside`); tooltips show pane details and clicking a tile
selects that pane directly. Browser
tiles use cached favicons with a globe fallback.
The settings sample allows hover while selection actions remain inert.
Automatically named window pills use the active browser pane's title. Browser labels fall
back to the active URL and then “browser”; explicitly renamed windows keep their chosen names.

Every pane header puts its controls 22px tall (`PANE_HEADER_CONTROL_HEIGHT`), 6px (`CHROME_GAP`)
in from the pane's top and sides, so a terminal header is 34px; the sidebar's
28px buttons and the titlebar's 32px tabs stay larger. A Browser header's first row is 40px so
its 28px tabs, as tall as the URL field, keep the 6px inset: the title, the tabs, New tab right
after the last tab, then the split buttons at the row's end; under it the navigation row, then 6px
(`crates/zz-ui/src/browser.rs`, `BrowserHeader`). The tabs share the row like a desktop browser's,
never scrolling: each up to 180px, shrinking evenly, and below 84px a tab shows only its favicon,
which turns into its X on hover.
Every header's title is the drag handle: `pane::PaneTitle`, a 22px button
(`pane.header.title`) holding the kind icon and the title, the provider name and session title
for an agent, and a globe with "Browser" for a browser (`BrowserTabStrip::title`). Dragging it splits or swaps under the existing availability rules
(`pane::PaneDragSource`). Pointed at, it tints, its icon turns into the four-way move arrows, and
an X at its right end closes the pane; a coarse pointer (touch, iPad) never hovers, so there the X
always shows and the icon stays. The title keeps one width, 180px (as wide as a browser tab;
less only when the header is narrower), whatever its label, so the X sits in the same place; a label too long for it fades out
over its last 28px under the X (`text_fade`) instead of ending in an ellipsis. Only "Browser",
which never changes, sizes to its label (`PaneTitle::fit`). Split bottom and Split right stay on
the right.
The URL field contains a 24px site-controls button at its left edge. Its popup reads Chromium connection
status and tab sound state when opened, and offers the existing site-data clearing confirmation.
The URL field is a neutral ghost field: no resting background, the ghost hover fill, and a
border in the focus color while focused. Active tabs take the neutral soft fill and the shared
control highlight; inactive tabs are ghost with muted text. Both reserve the border width so state
changes preserve layout.
A tab's label is the page title; one too long for its tab fades out the same way under the tab's
X, which overlays the tab's right end. Tab labels and URL text use 13px type with a 16px line height. Tab icons have 12px leading padding and a 6px gap before the label.
Omnibox suggestions use 32px single-line rows with title and muted URL text. Their rounded
hover and selection fills sit inside the dropdown's 4px padding. Tabs and history rows use
stored page favicons, with a globe fallback.

# Control density

Menus and select dropdowns draw as `popover` (a floating group), the command palette and path picker as
`palette` (a popover), and the file, history and tree pickers as `picker` (a dialog): in the
Default theme an opaque `theme.surface(Elevation::Popover)` fill or the theme's glass, the
outline, the theme radius and the float shadow in the theme's `shadow_color`. Tooltips are the
`tooltip` kind (glass with the contact shadow) and toasts the `toast` kind. Menus use
`Theme.row_inset` gutters (4px by default, through `list_inset`), 26px rows, 12px text, and
`rule.list` separators inset from the surface edge. At a 0 row inset (`Theme::inset_rows` is
false) the gutters, the gaps between rows, and the separator inset go away, and rows become
full-width bands with square corners; the workspace tree and command palette follow the same rule.
`Theme.outline` is the width of every outline a role draws; 0 draws none.

Which surfaces cast drop shadows is each role's `shadow`, all drawn in the theme's shadow color:
`contact` is the tight shadow a surface sits on (panes, cards, resting controls on the window) and
`float` the contact shadow with a deep, soft one under it (popovers, dialogs, floating panes, a
dragged pane chip). The theme's `contact` and `float` materials set their offset, blur, spread and
strength. `lifted_shadow` is the small shadow under a
switch thumb and follows the contact strength. `Theme::shadows_of`, `contact_shadows`,
`float_shadows` and `casts_shadow(role)` read them.

Corners come from the role's shape through its layer's `radius` (`Theme::shape_radius`), so new
widgets read the layer rather than a literal: `row_corners(element, layer, height)` for rows,
`picker::outer_radius(layer, inset)` for a container around inset rows, `Theme::outer_radius(inset)`
and `inner_radius(inset)` to keep other nested corners concentric, and `outlined()`, `inset_rows()`
and `casts_shadow(role)` for the theme's switches. Control heights, fills, rules and transitions
no longer change per theme: `Theme::dense`, `Theme::control_background` and `Theme::duration` are
gone, `Size::control_h()` and `input_py()` take no density, and `divider_color()` is the
foreground at the edge weight, translucent so a rule lightens glass in a dark theme the way it does
a solid fill.

Picker search fields draw the soft field paint on the popover ground with no border and no focus
ring, since they are always focused. Command palettes embed a bare shared Input
inside their raised container, with inline mode, host, and command Badge pills. They use 8px list
gutters; the container radius adds that inset to the theme radius. Rows retain the inset menu
radius, so the configured corner setting reaches both the surface and its contents.

The browser action menu puts the current profile and page zoom together. Its zoom stepper reads
live page zoom from the browser view, stays open after each click, and preserves the existing
zoom steps and limits. Its row stays at least 26px high to fit the embedded controls. With that row selected, Left/Right changes zoom and Enter resets it;
Escape dismisses the menu. Other menu actions retain their existing dismissal behavior.
`PopupMenuItem::stepper` owns those interactions so nested buttons cannot trigger a parent
menu dismissal.

Controls come in two sizes, set with `Sizable`: Medium, the default, is 28px tall with 13px text
(the macOS control font, and the comfortable click target in Apple's guidelines); `.small()` is
24px with 12px text, for toolbars and dense rows. Buttons pad their label by 12px at Medium and
8px at Small; fields by 8px and 6px. Anything else takes an explicit `Size::Size(px)` or a
`.size(...)` override. `Size::control_h` is the one height table, and the touch hit slop derives
from it. Icons keep the ambient font size unless sized: `.small()` is 12px, `Icon::medium()` 14px.
tmux `display-menu` draws as one of these menus (`zz_ui::command::floating::display_menu`): the
title is the small muted label row with its `#[align]` honoured, key annotations are plain text on
the right, and disabled rows never highlight. zz's default menu styles are theme slots and leave the
theme's own popover and row roles; only literal colours from the options or `-s`, `-S` and `-H`
(last field wins) tint the surface, the outline and separators, or the highlight. The tmux
`display-panes` indicator is one `pane.display-panes` chip per pane with the key to press, the pane's
`display-panes-format` line as a muted caption, and a 2px accent ring on the active pane (or
`display-panes-active-colour` when it differs from zz's default). A pane that fails in place (a browser page that will not load, an
editor file that will not open or save, an agent that stopped) shows `zz_ui::feedback::inline_alert`:
the alert dialog's layout on the `inline-panel` role, a 14px danger icon, a 13px title when there is one,
the message, and `alert_actions` below with label-only Medium buttons and the main action last on
the right.
Settings rows use 13px labels, so a Medium field's value matches them. The shared dialog surface
is similarly compact: 400px default width, 12px gutters, 13px title, 12px description, and Medium
action buttons. Explicitly sized content dialogs such as the attachment
preview retain their own width.

Dedicated pane/client tree and paste-buffer choosers use `zz-ui::chooser::ChooserModal`: 600px and 640px
maximum widths, 40px rows, 12px header/footer gutters, and the shared compact close button.
The surface grows with its row count up to ten visible rows and scrolls within the available
window height. It sits near the top of the workspace like the command palette. Tree rows use
the shared navigation icons, a muted target ID after the label, and a small checkmark for active
entries. They are `ListItem` rows, so they select the way the `row` kind does; the header and
footer share the body color. The outer radius adds the 4px row inset to the theme radius. The Commands & choosers
catalog includes the complete modal alongside the individual rows and search footer.
Daemon `choose-tree -s` and `choose-tree -w` requests use the compact Navigate palette instead,
preserving their initial tree expansion and selection. See the
[palette contract](/concepts/command-palette.md) for navigation and search behavior.

**A toast is that same surface.** `widget/overlay/notification.rs` imports the width, gutter and two
text sizes from `widget/overlay/dialog.rs` rather than restating them, so the two things that
interrupt a window . the modal it must answer and the toast it may ignore . speak at one size. Its
icon is a flex child aligned to the first line, not an absolutely positioned glyph the copy column
pads around.

**Every toast stacks at the top center of the workspace window**, whichever view raised it. The
close button sits in the toast's flex row, vertically centered beside its content. Settings is a route in
that window, and `AppShell` mounts both the dialog and notification layers for either route.
`crates/zz/src/window/toast.rs` keeps the workspace `WindowHandle<Root>` in a global and defers its
update until the current window callback finishes. Updating that handle during its own click
callback fails because ala has temporarily removed the window from its registry. Views that
already receive the workspace `Window` can call `window.push_notification` directly.

# Spacing

Spacing comes from a 4px grid (2px half steps inside small controls) and four rules. The agent
pane's cards (permission, question, error, task tray) follow them through one set of builders in
`crates/zz-ui/src/agent/presentation.rs` (`card_surface`, `card_header`, `card_tiles`, `CardTile`,
`card_close`, `card_footer`, `card_button`), so a new card inherits the spacing instead of
restating it.

1. **One gutter, measured to ink.** A floating card keeps 12px (`CARD_GUTTER`, the dialog's
   `CONTENT_PADDING`) between its edge and what the eye sees: the cap top of the title, the tiles,
   the filled edge of the last button. Line boxes are taller than their glyphs, so paddings are set
   so the *ink* lands on 12px: a 13px title in an 18px line sits 4px inside its box, so the
   header's top padding is 8. Icon buttons at an edge (the close button) take a negative margin of
   their padding so the glyph, not the hover box, meets the gutter.
2. **One left edge.** The title and the tiles start on the same line; tile content is padded 12px
   inside the tile.
3. **Closer inside a group than between groups.** Tiles are 2px apart, a title is 10px above its
   first tile, and separate groups (another question, the footer) sit 12px or a divider away.
4. **Tiles are list rows.** An option tile is the `agent.option` role, under `menu-item`, so it
   looks like the other lists a pane shows (the new-session actions, the pane picker, recent
   pages): at least `picker::LIST_ROW_HEIGHT` (32px) tall, 12px text with 11px descriptions, no
   fill at rest, a tint on hover, the accent when picked, and the row corner for that height. The 12px gutter is deeper
   than any built-in theme's card radius, so a concentric inner corner would round to square and
   fall to an arbitrary floor instead.

Options are tiles (`CardTile`) drawn as `agent.option`: bare at rest, with danger-toned text for a
destructive choice, a tint on hover and the accent when picked (a tint in Glass; see
[Design system](/designs/design-system.md)). Hover only changes the fill, which ala repaints;
text color cannot follow a hover style. What sits on a tile paints on the tile's ground, so it
stays legible on an accent selection: the description (`CardTile::detail`, the tile's
`muted_at`), the keycap that picks the option (`CardTile::keycap`), and the `Check` a
multi-select tile shows, or the radio a picked single-select tile shows. The free-text answer is
a field inside an "Other" tile. Glyphs beside a line of text sit in a box as tall as the line,
centred on its cap height (see "Icons next to text" below).

# UI font

Interface settings offers a UI font picker over ala’s system font families on macOS, Linux, and
Windows. `ui-font-family` in `zz/config` sets `Theme.font_family`; System default uses ala’s
`.SystemUIFont` alias. On Linux the fork resolves that alias to the desktop interface font, read
through the settings portal (GNOME `font-name`, KDE `kdeglobals` `font`), and follows changes to it;
without a portal value it falls back to ala's fallback stack. Config reload and OS appearance
changes reapply the choice to open windows.
The `UiFontConfig` global holds this string separately so `AppConfig` remains `Copy`.
Native entrypoints register `AvailableFonts` from their ala platform text system. The picker queries
that catalog directly to avoid offering ala fallback names that are not installed.

# UI zoom and scalable metrics

Application UI zoom is transient whole-window content zoom in `crates/zz/src/ui_scale.rs`, expressed
as a percentage of the default scale: `Cmd/Ctrl +` and `Cmd/Ctrl -` move it by 10 percentage points
between 50% and 300%, and `Cmd/Ctrl 0` restores 100%. The Appearance page's **UI zoom** row is a
`NumberInput` over that same percentage, so its steppers and the shortcuts land on the same values.
Each change updates the global `UiZoom` and applies it to every open window with
`Window::set_zoom`; newly opened windows receive the current value too.

`Window::set_zoom` folds that multiplier into the window scale factor and logical viewport, so the
entire ala element tree follows UI zoom, including values expressed in pixels. Continue to use a
named `Size` or `zz_ui::rems_from_px` when that captures a component's semantic sizing contract, but
do not multiply metrics by `UiZoom` again. Browser viewports already receive the effective window
scale factor, so native-to-CEF geometry follows the same rule. Geometry that must remain physically
screen-sized needs to opt out explicitly with `UiZoom::unzoomed`.

The thin clients compute their zoom in `interface_zoom` (`clients/app/src/app/settings.rs`).
On iPad the system text size multiplies the stored UI zoom unless **Match system text size** is off.
On iPhone the stored UI zoom multiplies the system text size (Dynamic Type), which counts up to
135% because zoom grows controls and icons as well as text; Settings shows UI zoom and hides
**Match system text size**.

# Terminal color exception

Terminal grid colors are a separate, renderer-neutral system described by
[terminal appearance](/terminal/appearance.md). Terminal foregrounds, backgrounds, palette
entries, selections, and cursor colors come from `TerminalAppearance`,
`AppearanceColor`, and `Color`; they are not application chrome colors. Exact terminal-color
conversion code and tests may therefore carry narrowly scoped lint exemptions. The surrounding
chrome (borders, labels, hover states, and badges) still reads its colors from
`cx.theme()`.

# Chrome chroma is independent of the terminal

**Application colors come from the interface theme (`interface-theme` in `zz/config`): its
materials, roles and colors for light and for dark, with the style, accent and tint `zz/config`
sets over it, never from the terminal.** The mode follows the OS appearance
unless `theme-mode` pins one, and only picks which of the theme's color sets applies. There is no
knob that derives chrome from terminal colors, and a Ghostty palette cannot repaint the window.

`crates/zz-ui/src/interface_theme.rs` owns `InterfaceTheme`, `Materials` and `ThemeColors`, the
built-in themes compiled from `crates/zz-ui/themes/*.json`, and the theme file parser, shared by
the desktop, browser and iOS clients and the storybook. `crates/zz/src/theme.rs` applies desktop
configuration. It holds the latest immutable `TerminalAppearance` and `AppearanceProvenance` as
ala globals (used by settings badges and the detach action, not to color anything).
`apply_zz_overrides` layers these values over the zz-ui base:

- the theme on screen (`config::shown_theme`: the active theme, a built-in or a saved file from
  the `themes` folder, with the `widget-corner-radius`, `chrome-accent` and
  tint keys `zz/config` sets composed over it by
  `ThemeOverrides::apply_to`): its colors for the effective mode (`InterfaceTheme::colors`), then
  its materials, applied with `Materials::apply`: radius, outline width, both shadows, row inset,
  primary, the interface font and the glass; then its role patches
  (`InterfaceTheme::apply_roles` into `Theme.styles`). One radius therefore reaches every role
  that rounds, and gapped pane corners (`Theme::pane_radius()`), and survives a light/dark switch.
  Corner smoothing is not a material: every window draws `CORNER_SMOOTHING` (4);
- `font_family` from `ui-font-family` when present (a System default line keeps the system UI
  font even under a theme with its own), else the theme's font, else the system UI font;
- `mono_font_family` from the terminal's resolved primary family, so Agent Markdown and code blocks
  match the terminal typeface;
- `theme.contrast` from `chrome-contrast`, preserved across the same refreshes.

The thin clients keep the selected theme id, their own saved themes and their overrides in their
preferences, and `Preferences::apply` draws `Preferences::shown_theme()` (the theme with the
device's `ThemeOverrides` composed the same way): its colors, materials and roles, with the UI
font taken from the preference, else the theme's font, else the bundled Inter.

`chrome-contrast` defaults to 1.0 and accepts 1.0–2.0; other values are reported like any numeric
key. The Interface page shows it as a slider from 100% to 200%, in steps of 5. It scales the elevation step in `raised()` and `washed()`, the amounts in `hover()` and
`active()`, the alpha factors in `fill()`, `glow()`, and `wash()`, and the foreground weight in
`border()`. It divides the `muted()` amount, keeping muted text closer to foreground as contrast
rises. Those factors clamp to 0–1. `on()`, `outline()`, `subtle()`, `floating()`, explicit opacity,
and color math keep their existing values; literals such as `foreground.opacity(0.1)` stay fixed.
The Rust derivations read a thread-local scalar set by `Theme::set_contrast`. The native macOS
client keeps the default derivation strengths.

Icon-only chrome controls use `Button::compact_icon`: a 24px hover surface around a Small 14px
glyph with a 0.5px downward optical adjustment. The titlebar, sidebar row actions, browser pane,
pane header buttons (`pane_header_icon_button`, the agent lens toggle among them) and the agent
timeline share this constructor, which fixes padding and icon scale in one place. Browser
controls override the hover surface to 28px while retaining the shared icon scale. The sidebar
toggle, Settings, the view switch (eye) and the iOS palette button are 28px buttons with 17px
glyphs, a step under the 32px titlebar pills (`WORKSPACE_CHROME_BUTTON_SIZE` in `navigation.rs`).
The sidebar's rows stop 8px short of its trailing edge past the row inset
(`WORKSPACE_SIDEBAR_SCROLLBAR_GUTTER`, on the flat list and the tree's `uniform_list`), so the
scrollbar thumb runs in a lane of its own beside them. Every row action (the flat rows'
… and +, the tree's 24px buttons) centers 19px in from the row fill's trailing edge
(`WORKSPACE_ROW_ACTION_CENTER`), and the toggle and the view switch center on the same line. A
titlebar window pill's hover close sits 10px from the pill's edge (`workspace_row_action_gap`).
Sidebar titles, descriptions and session and host labels fade out over their last 28px
(`LABEL_FADE`) instead of ending in an ellipsis.

Text and icons take a hover color only from the `hover` of an element with an id. ala shapes text
and resolves an icon's color at layout; an element with an id records its own hover and lays out
with it on the next frame, while without an id `hover` and `group_hover` apply at paint only, where
fills, borders and visibility change but text and icons keep their color. So muted ink that
brightens under the pointer is a `hover` text color on the element with the id: a button does it
for itself, which is how sidebar row actions and browser tab close buttons brighten. The agent
pane's step rows draw no fill, take muted ink, and turn foreground under the pointer when the row
opens something (`activity_row_marked`); a label in its own color (a running tool, "Waiting for
you") keeps it. Its message actions (copy on answers, code blocks and diagrams, rewind on prompts)
are `agent.action` icon buttons with muted ink and no fill in any state (`message_action`), shown
only while their message, code block (`text::CODE_BLOCK_GROUP`) or figure is pointed at, and
always under a coarse pointer (`shown_on_hover`). The slash menu's rows are 40px: the command at 12px over its description at
11px.

Icons next to text are centred on the text's cap height, as Apple places SF Symbols. The icon is
centred on the line, then moved down by `penna::icon::cap_center_drop(font_size, icon_size,
cx)`: half of the theme font's ascent less descent less cap height, 0.31px for SF Pro at 12px and
0.33px at 13px, nothing for Inter, nudged by up to a quarter point so the icon and the baseline
round to the same device pixel grid. Rows that wrap give the icon a first-line strut as tall as the
first line's pixel line height. The agent timeline's activity rows, cards, alerts, task tray and composer
footer, the workspace status pills, markdown task lists and the settings disclosure heading all
use it. The glyph's own ink has to be centred in its 24-unit box too; the ala notes are in
`.agents/skills/ala-interface/SKILL.md`.

A radius is a *request*, not the final corner. ala caps one at half the shorter side . the point a
rounded rectangle stops existing . so one global setting applied to components of different sizes
would make each change shape category at a different value of it: a 24px icon button becoming a
circle at 11→12, a 30px input becoming a pill at 14→15. Windows therefore opt into
`Window::set_adaptive_corner_fraction` (`ADAPTIVE_CORNER_FRACTION`, `crates/zz/src/theme.rs`), which
resolves every radius against the element it rounds at paint time, in one place rather than at 72
call sites. An ordinary corner approaches 45% of the shorter side along `cap * tanh(radius / cap)`:
within a fraction of a pixel of the request where radii are actually set, bending rather than
stopping beyond it, so no component ever changes category *or* stops responding while its neighbours
keep moving.

Settings accepts widget radii from 0 through 25. Every surface whose role's shape is `round`
takes the radius through the adaptive corners described above, so a large radius rounds a short
control nearly into a pill without making it one; rows take their separate inset corner rule.
Pills are a role's choice: the `pill` shape resolves to ala's `FULL_CORNER_RADIUS`, the same
value as `rounded_full()`, so the piece stays a pill or a circle at any radius (a theme makes
its buttons and chips pills this way). The Corner radius slider applies changes
immediately and shows the radius in pixels beside it. `Theme::control_radius`,
which turns a radius above 24 into a pill, now rounds only the kit's slider.

Switch tracks, status dots, avatars, and scrollbar thumbs still use `rounded_full()` at any
widget radius. Explicit button radius overrides remain in effect; the browser site controls
button follows the address bar's role: a circle while the address bar is a pill, the bar's
radius less its inset otherwise.

`MuxClient` refreshes those globals from the initial `ServerHello` and every `AppearanceChanged`
event. `refresh_current_theme` restores the correct zz-ui base before reapplying the theme and
overrides, while `sync_system_appearance` records the actual OS mode separately before doing the
same after every light/dark refresh. Every application window must route appearance syncs through
that wrapper. The separate OS-mode global is load-bearing: the installed `Theme::mode` may be a
Light/Dark pin and cannot be reused as the System preference.

`theme-mode` is a `zz/config` key, and Settings' Appearance page opens with a Theme group whose first
card is the System / Light / Dark picker, three drawn window previews
(`config/settings.rs`, `theme_preview`). A pinned mode wins in both `refresh_current_theme` and
`sync_system_appearance`, so the pin survives an OS light/dark switch; returning to System restores
the last recorded OS mode. The previews paint each half from the active theme's colors for that mode.

`ThemeColor` holds six palette roots, the per-mode scrim, the shadow color and the optional text on
the accent. Selected menu, list, and picker rows use their row role's `selected` highlight: the
opaque accent with the theme's `on_accent` text, or a tint where the theme asks for one. The accent also marks checked switches,
selected tiles, the active pane's glass edge, keyboard focus, the Agent send button, effort pills,
and palette mode prefixes and running-agent dots. Use the shared component treatment for these
roles instead of choosing a separate highlight color. `border()` is an opaque Oklab mix
of 86% background and 14% foreground at the default contrast. Other colors derive through `Colorize` at paint time, so
a theme names its roots and needs no parallel token table kept in step. Views read
`cx.theme()` and never receive a copied palette or local color literals.
The app shell paints one continuous chrome background beneath the pane layout. On Linux with
client-side decorations, `RoundedWindowFrame` paints that background through the outer frame and
`AppShell` leaves its inset surface transparent. This keeps the background beneath the antialiased
border instead of exposing the shadow at the join, without stacking translucent fills. Fixed sidebar,
titlebar, Settings, margins, split gaps, and rounded pane corners inherit that fill. The slideover
sidebar paints its own overlay surface. App-owned pane roots paint no fill, so the window shows
through them; only PiPs paint one, at `Theme::pip_opacity` (`pip-background-opacity`, 80%).
The Agent composer is a field shell (`field_chrome` as the `agent.composer` role) and overlaps the
transcript by 12 px at its top edge.
Prefix cards disable this overlap. The transcript clips before the footer, which inherits the pane background, and
the shared pane frame adds no background fill. The terminal blends its Ghostty tint with the
theme base before applying the factor. Browser pages remain opaque.

# Related

- `libs/penna/UPSTREAM.md` . the fork's source revision and per-module record of what was ported,
  trimmed, or rewritten
- [ala revision pin](/references/ala.md) . the pinned ala sources
- [`app` crate](/crates/zz.md) . the ala client governed by these conventions
- [Terminal appearance](/terminal/appearance.md) . the independent terminal color model
- [Design system](/designs/design-system.md) . tone, emphasis and ground: how widgets pick their colors
- [Surface roles](/designs/surface-roles.md) . kinds, roles and materials: how every surface picks its fill, edge, corners and shadow
