---
type: Design Plan
title: "Foundations: penna's design language"
description: "The spec for penna's design language and the source of truth for every penna screen: principles, the rules and scales (space, type, sizes, color, corners, depth, focus, motion, layout), and each component's purpose, anatomy, states, behavior, content and placement. The docs site draws it with real components."
status: Built (2026-11-04, steps 1 to 9)
resource: src/foundation/space.rs
tags:
- ui
- design-system
- tokens
- design-plan
timestamp: 2026-11-03T00:00:00Z
---

# Foundations

penna's design language: the principles, the rules and scales, and how each component looks and behaves. It is the source of truth for every penna screen and the spec agents read before building or changing UI. The docs site draws it: every section here has a page there made of real penna components (`docs/pages/principles.md`, `foundations.md`, `color.md`, `typography.md`, `spacing.md`, `sizes.md`, and a page per component), so people can see it. When a page and this file disagree, this file wins and the page gets fixed; when the code and this file disagree, the code gets fixed, and each component's Status says where it stands.

Values are design pixels: one unit is 1/16 of the root rem, so the interface font size and window zoom scale everything together. `rems_from_px` converts.

Status marks: **built** is in the code; **step N** is planned in the work order at the end.

## Principles

penna draws zz and nidus: tools people keep open all day, mostly driven from the keyboard, where the work (a terminal, a transcript, a document, the computers on a desk) matters more than the interface around it. Six principles follow from that. The rules and scales below are how they are kept; when a case is not covered, decide it by these.

1. **The keyboard comes first.** Every action has a keyboard path, and the interface shows it: shortcuts sit beside menu rows and in which-key, and a focus ring appears whenever the keyboard moves focus. Pointer and touch reach the same actions, sized for them. So: every control is focusable unless it is disabled, activates with Space and Enter, and shows its shortcut where it is offered.
2. **Calm, dense screens.** Most text on a screen is one size (`body`); weight and a few larger steps carry the hierarchy, and color is kept for meaning. Spacing follows relationships, so a dense screen still groups clearly, and density is a setting, never a squeeze. So: two or three text styles in a view, gaps picked by relationship, nothing added for decoration.
3. **One highlight, one accent.** What is picked or in focus is marked in the ink itself, whatever the tint: selection is a wash of the ink and a primary is a solid fill of it. The tint is the accent, always a color and only ever a detail: the track of a switch that is on, a slider's fill, a checked box, the dot that asks for attention, and links. Everything else stays black and white. Status colors mean status and always come with an icon or a word. So: at most one primary action in a view, and no color that only decorates.
4. **The work leads, chrome recedes.** Bars, headers and toolbars use quiet controls (no fill at rest), muted labels and the smaller rungs, so the eye goes to the content. Text people read (documents, chat) is larger and more open than text people scan (`read-body` against `body`). So: chrome never competes with content for the strongest ink, size or color.
5. **Every state shows.** A control always tells what it is doing: rest, hover, pressed, focused, selected, disabled, loading or in error. Feedback comes in the same frame as the input. Motion is short (120 to 220ms), shows where something came from or went, and turns off with reduced motion. So: no state without a look, and no animation that only decorates.
6. **One kit everywhere.** The same components draw native windows on every desktop, the phone and the web, in one look, light and dark, that apps and people adjust with a few knobs (Look). A knob changes color, corners, glass or faces, never a component's fills, edges, sizes, spacing or behavior. So: a component that only works in one look, one mode or one platform has a bug.

## Rules

1. Take every length from a scale in this file. A pixel literal in a component is a bug, except hairlines (0.5 and 1), optical offsets (computed from font metrics with `cap_center_drop`, or a glyph's own kerning, such as the control mark pulled toward the key it modifies on a touch key row), and the geometry listed under Space as staying in pixels.
2. Every element answers three questions, each with a token: its height (Size), its inset (padding, owned by the element), and the gap to its neighbors (owned by the parent).
3. Components never set an outer margin. The layout that holds them sets the gap.
4. Pick a gap by relationship, not by eye: tight for a label and what it names, related for siblings that work together, group for groups inside a pane, section for the parts of a page. When two levels both seem right, the grouping is unclear: add a group, never a value between levels.
5. Every piece of text has a text style: size, line height and weight together. Never set a size without its line height.
6. Every control in one row has the same size.
7. A token has one name everywhere. Its number is its pixel value. Rust and CSS spell it the same way (see Names).
8. Color comes from the look's roles, never a literal. Status colors always come with an icon or a word.

## Names

| Token | CSS | Rust | Status |
|---|---|---|---|
| `space-8` | `--penna-space-8` | `Space::S8` | built |
| `gap-related` | `--penna-gap-related` | `Space::GAP_RELATED` | built |
| `inset-card` | `--penna-inset-card` | `Space::INSET_CARD` | built |
| text style `body` | `--penna-text-body-size`, `-line`, `-weight` | `TextStyle::Body` | built |
| `control-md` | `--penna-control-md` | `Size::Medium` | built |
| colors | `--penna-fg`, `--penna-fg-muted`, ... | `cx.theme()` fields and looks | built |

Until step 1, `Space` variants were named in 2px steps (`Space::S4` was 8px), while ala's helpers count 4px steps (`gap_4()` is 16px). Don't use ala's spacing helpers (`gap_2()`, `px_3()`, ...) in penna; use `Space`.

## Space

Scale: 2, 4, 6, 8, 10, 12, 16, 24, 32, 40, 64.

- 2, 6 and 10 only inside a control (icon gaps, field insets, segment padding), with two exceptions: 6 is also `inset-chrome`, around the controls in a bar, and 2 separates the rows of an inset list.
- Layout uses 4, 8, 12 and 16 upward.
- 1, 3, 5, 7, 9, 11, 14, 18, 20, 22 and 28 are off the scale and fold by eye to a neighbor. 96 folds to 64.

Gaps, set by the parent:

| Token | px | Between |
|---|---|---|
| `gap-tight` | 4 | a label and what it names: a title and its description, a field label and its field |
| `gap-related` | 8 | siblings that work together: buttons in a bar, chips, a heading and its card |
| `gap-group` | 16 | groups inside a pane: form groups, cards in a column, settings in a card |
| `gap-section` | 32 | sections of a page |

Insets, set by the container:

| Token | px | Inside |
|---|---|---|
| `inset-chrome` | 6 | bars and pane headers, around their controls (today `CHROME_GAP`) |
| `inset-card` | 12 | cards and popovers |
| `inset-pane` | 16 | panes and dialogs |
| `inset-page` | 32 | document pages |
| row inset | theme | lists and menus around their rows: `theme.row_inset`, 6, at the sides; a menu pads its rows by it on every side |

Rows in a plain list (row inset 0) touch: their height keeps the rhythm. In an inset list (`theme.inset_rows()`, a row inset above 0, as in the look) rows sit `space-2` apart, so their rounded highlights never merge: menus, select lists, settings navigation and the touch which-key list. `list_inset` sets both the side inset and that gap. The docs navigation keeps its rows `space-2` apart in every theme.

Components take every padding, gap and margin from `Space` (built in step 6), so spacing scales with the rem like type does. Settings pages are a pane (`inset-pane` 16 around the page, `gap-group` 16 between groups); a settings row is inside a card (`inset-card` 12 all round, title and description `gap-tight` 4 apart). Dialogs are panes too (`inset-pane` 16). Sidebar sections are `gap-group` 16 apart. Which-key's columns and lines are `gap-group` 16 apart inside an `inset-card` sheet. What stays in pixels: drawing geometry inside a fixed-size picture (Appearance's theme miniatures), focus halos and swatch rings that feed radius math, window snap margins, and the sidebar title bar's insets, alignment geometry computed from the rows' action column (and the traffic lights on macOS) so the window's buttons line up over the rows.

An inset reaches the text's line box, not its letters. In `body` (13 on 18) capitals start 4.27px inside the box and the baseline sits 4.27px above its bottom, so an `inset-card` reads as about 16px to the capitals. Keep the token; never nudge text natively. An HTML twin may move a text run by a pixel at most (`position: relative; top`, usually ±0.5px) where ala rounds the baseline differently from the browser, so parity holds; that is the only nudge, and it lives in the twin's CSS.

## Type

Faces: the platform system face natively (`.SystemUIFont`), Inter Variable in the storybook and on the web. The code face is the platform's natively (`Theme::mono_font_family`: Menlo on macOS and iOS, Consolas on Windows, DejaVu Sans Mono elsewhere) and Lilex in the storybook and on the web. Weights: 400 text, 500 emphasis in the interface (`strong`), 600 headings. No other weights.

Interface (`TextStyle`, built):

| Style | Size | Line | Weight |
|---|---|---|---|
| `micro` | 10 | 14 | 400 |
| `caption` | 11 | 16 | 400 |
| `small` | 12 | 16 | 400 |
| `body` | 13 | 18 | 400 |
| `lead` | 14 | 20 | 400 |
| `heading` | 16 | 22 | 600 |
| `title` | 20 | 26 | 600 |
| `display` | 26 | 32 | 600 |
| `mono` | 13 | 18 | 400, code face |

Reading, for documents and chat (`TextStyle::ReadH1` to `ReadH5` and `ReadBody`, built). Sizes reuse the interface sizes with lines made for paragraphs:

| Style | Size | Line | Weight | Before step 2 |
|---|---|---|---|---|
| `read-h1` | 26 | 32 | 600 | 28 on 45.5, bold |
| `read-h2` | 20 | 28 | 600 | 21 on 34 |
| `read-h3` | 16 | 24 | 600 | 17.5 on 28.5 |
| `read-h4` | 14 | 22 | 600 | 15.75 on 25.5 |
| `read-h5` (h5 and h6) | 13 | 20 | 600 | 14 on 22.5 |
| `read-body` | 14 | 22 | 400 | 16 on 26 |
| `read-small` | 13 | 20 | 400 | planned, for captions and table cells |

Reading rhythm (built): blocks (paragraphs, lists, quotes, code, tables) sit `space-12` apart (`TextViewStyle::paragraph_gap`); a heading has `space-24` above it (h1 and h2) or `space-16` (h3 to h6) and `space-8` below. The first block has nothing above it. Bold in reading text is 600.

`TextView` sets `read-body` on itself (`TextViewStyle::text`, default `Some(TextStyle::ReadBody)`). `None` makes it inherit its container's text style instead, without the measure: zz's agent transcript sets `read-body` on its containers and lets its markdown inherit it. Headings always take the reading ladder. A task list's checkbox is as tall as the font and sits in a box one line tall.

Touch, for the compact, touch and sheet blocks (`TextStyle::TouchSmall`, `TouchBody`, `TouchTitle`, built): `touch-small` 15 on 20, `touch-body` 17 on 22, `touch-title` 34 on 40, weights 400, 400, 600; strong is 500 for the first two.

Component text (step 3, built): list and tree rows `lead`, centered in the row height (which follows density and the rem), their details `small`; sidebar headings strong `small`, under the `body` rows they head; menu rows `small` with 12 icons, and `body` with 14 icons on a 44 row under a coarse pointer; their shortcuts `small`; menu section labels `caption`. Controls take the text of their size from `Size::control_text()`, the Text column of the control ladder: buttons, fields, selects and segmented controls set it once on the control and their label, icon and children inherit it, so a caller's weight (a selected sidebar row's 500) still wins. A field's caret, selection and rows follow its line (18 at `body`). Badges are strong `caption` (`micro` at the `xs` and `sm` rungs), keycaps `caption` (`micro`), the stepped slider `small` (`caption` at `xs` and `sm`), and the touch slider's readout `title`. Surfaces (dialogs, alerts, notifications, callouts, notices, popovers) set `body` on the surface, before the caller's own style, so content with no style of its own is 13 on 18; their titles are `body` at the heading weight, 600, and their descriptions and messages `small`; a stacked notice's title is `lead` at 600. An empty state sets `small` on itself, since its message is its bare content, and its title is `body` at 600. A marker beside a first line (a notification, callout or notice icon) sits in a box as tall as that line: 18 beside a `body` title, 20 beside a stacked notice's `lead` title, 16 beside a message alone. Tooltips are `small`. Chrome: the title bar's content area and the tray panel set `body`; a window title in the title bar is `body` at 600, with a path beside it regular and muted. The docs header's brand is `lead` at 600. Which-key sets `small` on itself, its table name strong `small`, and `caption` for block titles, the repeat mark and its key caps in the code face. Settings: the page heading is `title`, group headers `body` at 600 (so a disclosure's chevron box is 18 tall), row titles `body`, descriptions `small`; the about page's name is `title`, its tagline `small`, its values `caption` in the code face. The picker's rows are `small`, its empty label `caption`, its footer `micro`. The color picker's label is `caption`, the touch trigger's hex `mono`. Appearance: slider values `small`, tile labels `caption`, and the theme file description `small`. Compact and touch: large titles `touch-title`; the HUD strong `touch-body`; the folding title `touch-body` at 600; pill titles and menu chips strong `touch-small`, pill meta `small`, the mini pill strong `body`; key rows `touch-small`; the touch which-key list `touch-body`, its caps `touch-small` in the code face, its group headers strong `body`; popover keys `touch-body`. Sheets: titles `display`, row labels `touch-body`, details `touch-small`. The code editor sets `mono` on itself (13 on 18) and draws line numbers at the `caption` size on the code line. The HTML versions read the `--penna-text-*` variables instead of copying sizes.

Code takes the size and line of the text around it, in the code face: `mono` in the interface, `read-body` size in reading text. ala can't size one run differently inside a line.

Root text (built): `Root` sets `body` on every window, before the caller's own style, and `penna.css` the same on `html`, so text with no style is 13 on 18 on both, where ala alone gives 16 on 25.89. A storybook section sets `body` too, and the storybook's own labels and sample text take text styles like any component (state labels are `caption` in the code face).

Centering: Inter centers its capitals in the line box (within 0.01px at 13 on 18); the macOS system face sits them 0.33px low and Lilex 0.34px. Center a line box to center text; center icons beside text on the capitals with `cap_center_drop`.

## Size

Control ladder. Every control at a size shares one anatomy:

| Token | px | Text | Inset | Field inset | Icon | Icon gap | Status |
|---|---|---|---|---|---|---|---|
| `control-xs` | 20 | `caption` | 8 | 6 | 12 | 4 | built: `Size::XSmall` (the small chip) |
| `control-sm` | 24 | `small` | 8 | 6 | 12 | 4 | built: `Size::Small` |
| `control-md` | 28 | `body` | 12 | 8 | 14 | 6 | built: `Size::Medium`, the default |
| `control-lg` | 32 | `body` | 16 | 10 | 16 | 6 | built: `Size::Large`, the tabs' default height |
| `control-xl` | 40 | `lead` | 16 | 12 | 16 | 8 | built: `Size::XLarge` |
| `control-touch` | 44 | `touch-body` | 16 | 12 | 16 | 8 | built: `Size::Touch` |

Fields use the field inset because their text starts at the edge. `Size` carries the whole row of the table: `control_px`, `control_text`, `inset`, `field_inset`, `icon_px` and `icon_gap`, and `Size::Size(px)` takes the nearest rung (`Size::rung()`) for everything but its height. Buttons are the exception: a `Size(px)` button keeps a proportional inset (a fifth of its height) and icon (three quarters of it) and a `space-6` icon gap, for custom shapes such as the title bar's chrome button. Chips take their rung's inset, icon gap and icon like buttons, and selects their rung's field inset and icon gap like fields; badges and keycaps pick their look by rung too, `Size(px)` included, while a checkbox at `Size(px)` takes that side. Badges, keycaps and checkboxes sit in a line of text, so from `Large` up they draw like `Medium`. A field's own marks (clear button, spinner, select chevron) are a rung smaller than the field from `Medium` up; `xs` and `sm` fields keep their own size.

Other heights: `control-chrome` 22 (controls in bars, today `PANE_HEADER_CONTROL_HEIGHT`); `bar` 34 = `control-chrome` + 2 × `inset-chrome` for pane headers when the app gives panes a margin (`Theme::pane_margin` above 0), and 40 when panes sit flush (the default, `pane_margin` 0); the title bar is the pane header plus 2 × `pane_margin`, so beside the panes its center line is theirs; a title bar laid across the top of the panes instead (an app's window tabs with its sidebar hidden, `Theme::title_bar_over_panes_height()`) is the pane header plus one `pane_margin`, since the panes' own top margin finishes the gap under it, which makes it 40 at the default margin of 6, as tall as with panes flush, and a macOS window's traffic lights center on whichever of the two shows; its chrome buttons are 32 with a 17 icon (`CHROME_BUTTON_SIZE`, `CHROME_ICON_SIZE`); `row` is `control-md` (28 at regular density) for list, tree, sidebar, picker and menu rows, and a menu row under a coarse pointer is 44; a pane's own list rows (`picker::list_row_height`) and the settings navigation's Back slot are `control-lg` 32. Tabs take only a rung's height (`control-lg` 32 by default, any rung with `.with_size()`); their label, inset and icons are the caller's (`small` text in the stories and in `tab.css`).

Icons: 12 beside `small` and `caption` (menu rows included), 14 beside `body`, 16 beside `lead` and `heading`. List and tree rows are the exception: their icons stay 14 beside `lead`.

Inside a line of text, keycaps and badges are as tall as the line of the text they sit in (`TextStyle::line_height`: 16 in `small`, 18 in `body`), and checkboxes are 14 at `xs` and `sm` and 16 from `md` up, never a control size. Like every length here, these are design pixels and grow with the interface font.

Width: reading text stops at a measure of 640 (about 75 characters of `read-body`), natively and in HTML alike. A table column in reading text sizes to its content from 48 to 480. Sidebars are 200 to 256 wide. These widths are layout geometry rather than tokens, in design pixels like everything else, so they grow with the interface font.

Density (built): one setting picks the default control and row height: compact 24, regular 28, comfortable 32. Text and gaps stay, and so do the other rungs. It is `Appearance::density` (Size, under Text size), and `Size::Medium.control_px()` follows it, so buttons, fields, selects, segmented controls and list, tree, menu and picker rows move together. The HTML reads `--penna-control-md`, which `:root[data-density]` changes (`?density=` on a page). `Size::rung()` measures `Size(px)` against the regular ladder, so a fixed-height control keeps its rung at any density.

## Color

Roles (CSS root names, the look with the default knobs):

| Role | Light | Dark | Use |
|---|---|---|---|
| `bg` | #f6f6f3 | #1a1a19 | window and page |
| `panel` | #e9e9e6 | #212120 | sidebars and side panels |
| `popover` | #dcdcda | #292928 | menus and popovers, and the glass fallback |
| `fg` | #1c1c1a | #e9e7e3 | text and icons |
| `fg-muted` | #575756 | #908f8c | descriptions, captions, story labels |
| `muted` | #555553 | #a4a3a0 | muted labels and the ghost paint |
| `fg-faint` | #90908e | #4d4c4b | placeholders and disabled text only |
| `divider` | `fg` at 8% | `fg` at 8% | rules and hairlines; control and field edges take `fg` at 14% |
| `accent` | #0a64dd | #3183ff | the tint or the accent people pick in Appearance (`blue` by default): the details (Principles 3) and the dot that asks for attention |
| `on-accent` | #ebebeb | #ebebeb | the mark on an accent fill: a switch's knob, a check; near-white on every named accent |
| `primary` | #1c1c1a | #e9e7e3 | primary fills; always `fg` |
| `success`, `warning`, `danger` | #007b3e, #a75400, #cc2737 | #3fbf74, #f0a14a, #f2555f | status, always with an icon or a word |

The kit's base colors are these values and the default `neutral` reproduces them exactly (Look). Muted is 0.30 of the way to the pole: `fg-muted` mixes `fg` 0.30 toward its contrast pole (white under dark ink, black under light), and `muted` mixes the ghost paint's ink 0.30 toward the ground under it (on a tint's solid fill it is the ink itself, Look).

Surfaces stack in this order, each layer a step away from the one under it (darker in light, lighter in dark): window `bg`, then `panel`, then cards (`settings.card`), then `popover` and floating panels.

Highlight: what is picked or in focus is a wash of the ink, and a primary action a solid fill of it, whatever the tint. What is on (a switch, a checked box, a slider's value) takes `accent`.

Contrast (WCAG 2, built in step 8): in light and dark, for every tint, accent and `neutral` the knobs allow, muted text, accent, success, warning and danger as text reach 4.5:1 on `bg`, and so does the text on a solid fill of `primary` and the status colors. The muted mix is 0.30, as above. Light variants darken their accent and status colors until near-white text on them reads at 4.5:1, which also makes them read as text. Text on a solid status fill takes whichever of near-white or near-black gives more contrast, so dark variants put near-black labels on their mid-tone status fills. An accent fill holds marks, not text (a knob, a check), so its mark is near-white whenever that reaches 3:1, the floor for marks, and near-black only on an accent too pale for it; the named accents are vivid and all hold near-white in both modes. `interface_theme`'s tests assert all of it. `interface_theme`'s tests assert all of it. `fg-faint` stays under 4.5 on purpose: placeholders and disabled text only.

## Corners, depth, focus, motion

- Corners: the `radius`, `control-radius` and `smoothing` knobs (Look); 12, 12 and 3 by default. A control rounds to a share of its height, a container to `radius` in pixels, a corner inside another concentrically, and smoothing never changes how round a corner looks (Corners, under Look).
- Depth: steps of fill separate surfaces; controls, panes and cards draw no shadow. `shadow-float` (y 10, blur 30, spread -6) for menus, popovers, dialogs and the window, drawn tight (0.4 of its offset, blur and spread: y 4, blur 12, spread -2.4) under a floating surface without glass, whose hairline already marks its edge, `shadow-lifted` for cards that move, and `shadow-contact` (0 1 4 #00000033) only for the knob of a switch or slider. Never let a shadow alone separate two surfaces.
- Focus: a 1.5px ring (`FOCUS_RING_WIDTH`) drawn outside the border box with its own adaptive radius (`foundation::focus::with_focus_ring`), from the keyboard only; see Focus and keys. Only fields change their fill or edge on focus.
- Motion: appear 120ms, enter 200ms, turn 220ms, at half those under Fast motion. Respect reduced motion.

## Look

penna has one look, and every app and person adjusts it through a few knobs. The look is calm and flat: steps of fill carry the structure, so a dense screen holds few lines, the only thing that floats is what sits over the content, and glass marks what sits over something (Glass, below). Apps differ by their tint, their corners, their glass and their content (Tiles, Cards, display text), the way apps on one platform share their controls and differ in color and content. The values below are the look with the default knobs; `themes/default.json` and `web/tokens/default-*.css` carry them.

How each kind draws, at rest and under the pointer:

| Kind | Rest | Hover, pressed | Selected, current |
|---|---|---|---|
| Bar, panel | no fill; a panel takes `panel` | | |
| Pane | the ground, no edge, no shadow; when panes have gaps, glass, or a hairline in `divider` where the glass draws no rim | | ring |
| PiP (a pane floating over the others) | glass in the pane's own color, or the pane's ground opaque when glass is off; a hairline in `divider` where the glass draws no rim, gaps or not; `shadow-float` | | ring |
| Card | one step of fill above its ground, no edge, no shadow | | ring |
| Floating (menus, popovers, tooltips, dialogs, toasts, palettes, buttons over the content) | glass and `shadow-float`; where glass is off or draws no rim, the `popover` fill under a hairline in `divider`, and without glass `shadow-float` drawn tight | | |
| Button | a wash of the ink, no edge, no shadow; primary is a solid `primary` fill | a deeper wash | a deeper wash |
| Quiet button, icon button, menu row | no fill | a wash | the highlight |
| Item (list, menu and tree rows, tabs in content) | no fill | a wash | the highlight |
| Chrome item (sidebar rows and entries, window and page tabs) | no fill | a wash | glass |
| Field | a wash of the ink, no edge | a deeper wash | while typing, an edge in the ink |
| Label (badge, keycap) | a soft wash in its tone, no edge | | |
| Rule | a hairline in `divider` | | |

- The highlight on a selected row is a wash of the ink, whatever the tint. A toned row's highlight is its tone's solid fill.
- On that solid fill everything in the row takes the selected ink: label, muted text, icons, and a toned chip or badge, which draws as a wash of the selected ink in place of its tone. The word still says the status (Content). Natively the row hands its fill to its children as their ground (`.on(ground)`, with `cx.theme().ground_at(look, state)`); in HTML a row with `data-state~="selected"` or `"current"` and a solid highlight sets the ground inside it.
- A chrome item draws its selection as glass, a pill of the material over its bar or sidebar, under the material's rim and light. Its tint is the highlight it replaces, pushed away from the ground by the material's opacity, so over a flat bar or sidebar the pill reads exactly as strong as the highlight does where glass is off, and the blur adds only what lies behind; the highlight is its fallback. The chrome items are sidebar rows (`sidebar.row`, the Settings sections too), the flat sidebar's entries (`row.nav`), window tabs in the title bar (`tab.window`) and a browser's page tabs (`tab.page`). Glass is neutral, so the item keeps the window's ink and hands its children a glass ground, not a solid one. Where glass can't draw (glass `none`, Reduce transparency, or a glass ground under the item) it takes the highlight like any row.
- The rule is one for every solid ground, whatever its color, so a primary row in the ink counts as much as a danger one: a tone's solid fill under a selected or current row, and a prominent surface (a solid button, chip or badge). Natively the ground says so (`Ground::solid`: a layer's when it sits on a solid ground, its highlight is the solid fill, or it is prominent and not glass; a paint's when its ground is solid or its fill in that state is more than half opaque). There every tone but a solid one reads as neutral, and muted ink is the ink itself; faint keeps its mix toward the fill. A solid chip or badge keeps its tone, since its own fill carries its label. A status dot turns to the muted ink with the rest, and a dot on a solid chip takes the chip's ink.
- In HTML the tokens write one scope per solid ground, listing every role whose selected or current highlight is that fill in the theme (rows, list items, menu and tree rows, tabs, toggled buttons, and the chrome items where glass is off). A role with toned rules lists its untoned rows (`:not([data-tone])`) in the neutral highlight's scope and each `[data-tone]` in its tone's. A scope carries the inks that differ and `--penna-dot` for dots. The looks painted on a solid fill are written once for every scope they fit: the labels (badges, chips, keycaps) and the buttons rows hold (quiet, icon, flat icon and row icon buttons, not prominent ones and none inside a field set in the row), each button with every value it sets and every other value at its initial, so a toned one loses its tone as it does natively. Soft and prominent buttons keep their window paint there, and rows hold neither. A button inside a field set in the row (a menu stepper's quiet buttons) paints on that field's ground natively, so the scopes leave it alone. The solid scopes come last in the tokens, after the reduced-transparency block, so they beat a ground scope around the row at equal specificity. A dot reads `--penna-dot` before its own color, and `data-solid` sets it to the tone's on-ink. Tree rows, like list items, switch their icons to the row's muted ink on selection.
- Hairlines appear only where they mean something: rules, the outline variants (`button.outline`, `field.outline`, `badge.outline`, `chip.outline`), a field being typed in, and the edge of a gapped pane or a floating surface where glass draws no rim. Increase contrast adds none. Outlines are 1 wide.
- The knob of a switch or a slider keeps `shadow-contact`, the one control shadow in the look, so a light knob reads on a light ground.
- Variants (quiet, outline, solid, ghost) are drawn by the kit from their kind's style, so no knob can change one without the others.

The knobs, as a theme file spells them:

| Knob | Values | Default |
|---|---|---|
| `tint` | a named accent, or a color for light and one for dark | `blue` |
| `neutral` | a hue (0 to 360) and an amount (0 to 1) that tint `bg`, `fg` and the surfaces above them | warm: hue 60, the amount that gives the values in Color |
| `radius` | 0 to 25 | 12 |
| `control-radius` | 0 to 25, or `full` for pills | `radius` |
| `smoothing` | 2 (round arcs) to 5 (squircle) | 3 |
| `glass` | `none`, `liquid`, or a material of its own | `liquid` |
| `face` | the interface face; the code face stays the platform's | the platform's |
| `mode` | `follow`, `light` or `dark` | `follow` |

There are no presets to pick. The look starts from the platform's knobs (the `macos` theme file on Apple platforms, `default` elsewhere), and a person sets every knob directly in Appearance, the same in every app:

| Knob | Does |
|---|---|
| Mode | system, light or dark, unless a theme locks it |
| Accent | `tint`: one of the named accents, or a color |
| Tint, Tint strength | `neutral`'s hue and amount |
| Corner radius | `radius`, a slider from 0 to 100% along a square curve, `radius` = 25 × share², so the default 12 sits at 69% and the steps near square corners stay small. 100% is a round corner, never a pill: Controls makes pills |
| Controls | `control-radius`: Rounded follows `radius`, Pill is `full` |
| Glass | `glass`, a switch: on draws `liquid`, or a theme file's own material once one is pasted in; off draws none |
| UI font | `face` |
| Text size | the root rem; everything scales with it |
| Density | compact, regular or comfortable (Size) |
| Increase contrast | a slider from 0 to 100%, contrast 1 to 2: surface steps, fills and the divider further from their ground, muted and faint text closer to `fg`; no edges |
| Reduce transparency | glass draws as the `popover` fill |
| Motion | Off (reduced motion), Default, or Fast (Motion's durations halved) |

Appearance shows them in that order in three groups, none folded away: Look (Mode to UI font), Size (Text size and Density) and Accessibility (Increase contrast, Reduce transparency and Motion). An app's own rows go in groups of their own (zz's Sidebar after Look, and Window, with the app icon and window blur, last). `smoothing` and `mode` have no row: smoothing follows the platform's theme file. A theme file has no row either: an app that reads or writes theme files (the storybook's theme creator) does it itself, and `AppearanceChange::Import` sets every knob from one.

What follows the platform with no knob: the system face and code face, when scrollbars show, the window controls, and how keys are spelled. Glass is the same everywhere: `liquid` unless a person turns it off.

Everything else is the look and has no knob: each kind's fill, edge, shadow and shape, every state look, outline width, highlight strength, status colors, spacing, sizes, type and motion.

Corners follow the Corner radius knob, a share from 0 to 100% (`radius` = 25 × share², Knobs below), by four rules, so at any setting every rounded thing reads equally round whatever its size:

1. A control takes its corner from its height: `roundness` × its height, where `roundness` is 45% × the share of `control-radius` (`Theme::control_roundness()`; 31% at the default 12, 45% at 25). Buttons, fields, selects, tabs, chips, badges, keycaps, the segmented control's track and every row (sidebar rows and entries, menu, list and tree rows) are controls. Pill (`control-radius` `full`) makes the capsule controls half their height instead; the controls that are never capsules (menu rows, keycaps, text areas) keep `roundness` from `radius`. A 28 control at the default is 8.7, a 22 one 6.9, a 46 sidebar entry 14.3; at 100% they are 12.6, 9.9 and 20.7.
2. A container keeps `radius` in pixels: cards, settings cards, panels, popovers, dialogs, toasts, tooltips, panes, the window. A pane rounds no further than its header controls' corner plus `inset-chrome`, so a terminal's corners never eat its text (16 at 100%). One shorter than its corner rounds no further than 45% of its shorter side, on a `tanh` curve (`corner-fraction`).
3. A corner inside another is concentric, measured on the drawn corners: inner = outer minus the inset between them, and never rounder than a control of the inner one's height. The segmented control's pill is its track's corner less the border and inset; a menu's rows are the popover's corner less the row inset; a picker's panel is its rows' corner plus its inset; a PiP takes the pane's corner.
4. Smoothing changes how gradually a corner turns, never how round it looks. `smoothing` is a superellipse exponent, and a superellipse in the box of a circular arc reaches less far into the corner (at 4 it looks like an arc of 0.54 times the radius), so every corner is drawn in a box grown until its midpoint sits where the arc's would: 1.21 times the radius at 2.5, 1.42 at 3, 1.84 at 4. Where that box would pass half the side, it stops just short of it (the renderer draws a corner that reaches half the side as a plain round) and the exponent eases toward a circle until the midpoint matches. A capsule is a circle. Every role takes the theme's `smoothing`; nothing caps it.

Glass is for what sits over something, since it reads only with something behind it:

- Floating surfaces, and buttons that float over the content (`ButtonLook::Floating`), over whatever is under them.
- Panes over the window's frame, when panes have gaps (`Theme::pane_margin` above 0): the pane's ground is the tint, at the ground's own opacity, under the material's rim and light, with no blur or refraction, since the frame under a pane is one flat fill. A pane that sits flush has nothing under it and takes its ground.
- The selection in chrome, over its bar or sidebar (chrome items, above).

Never cards, fields, bars, panels, or the rows and controls inside content: they sit in their ground, not over it. Where glass is off each falls back: floating surfaces to the `popover` fill under a hairline in `divider` and the tight `shadow-float`, panes to their ground with a hairline in `divider` when they have gaps, a PiP to its pane's ground, opaque, under that hairline, chrome selections to the highlight. A gapped pane takes the same hairline under a material of its own that draws no rim (no bezel and no edge shadow), so its edge always shows.

A material is a full glass definition (blur, tint, saturation, bezel, refraction, specular, rim) for light and for dark:

- `liquid`: blur 20, bezel 3, refraction 4, a specular rim lit from above, one set of values for light and one for dark (`interface_theme::liquid`). Where the renderer can't bend light it keeps its blur, tint and rim and drops the lens: the web draws the lens only where `kit.js` refracts, and natively a window that can't sample its backdrop draws the fallback.
- A material of an app's own takes the same fields. The kit holds every material to two rules: text on it reaches 4.5:1, so its tint never drops below the opacity that gives that over the worst content, and it has a solid fallback, the `popover` fill.

Natively the blur reads the backdrop up to 2.5 blur radii past a glass surface's edge; the web's backdrop filter reads only what is under the surface. Where the backdrop changes just outside a glass edge, as under a dialog stacked over another, the two blend it a little differently. That is a raster difference, like text, not a choice.

The kit's theme files: `default` (the table's defaults), `macos` (tint `blue`, since ala exposes no system accent; `control-radius` `full`; `smoothing` 4), the look on Apple platforms, and `yaru` (tint orange; `radius` 8; `smoothing` 2; `glass` `none`), which the storybook, the HTML tokens and the parity runs draw as a third look. None of them is a choice a person makes.

HTML: a theme file's knobs generate its tokens (`cargo run --example theme_css`), so the HTML twin follows the same look. A theme that leaves `glass` out gets `liquid` in its tokens. `--penna-control-radius` carries the control corner (9999px for `full`). Under `prefers-reduced-transparency: reduce` the tokens give every glass role the `popover` fill and no backdrop. A card inside a container marked `data-ground` rises one step from that container's fill.

How the knobs are spelled and decided:

- `tint` is a named accent (`blue`, `orange`, ...), one hex color for both modes, or `{ "light": ..., "dark": ... }`, and sets `accent`; left out, it is `blue`. A person's Accent replaces it. The named accents, light then dark: blue #0a64dd #3183ff, purple #8549d1 #a366f7, pink #c02a82 #e648a0, red #cc252d #f44647, orange #ad4f00 #de6700, yellow #876400 #af8200, green #007b2c #009f3b, graphite #6a6a6a #868686. Each dark one is the most saturated color of its hue that reads at 4.5:1 as text on `bg` and holds a near-white mark at 3:1.
- `neutral` is `{ "hue": 60, "amount": 0.15 }` by default, hue in OKLCH degrees. Amount 1 is 0.02 of OKLCH chroma on `bg` and 0.35 of that on `fg`. The base colors in code are the Color table; a neutral moves them by the difference between its tint and the default's, so the default reproduces the table exactly and other values tint like a person's tint did. A person's Tint and Tint strength override the hue and the amount.
- `radius` and `control-radius` clamp to 0 to 25 and `smoothing` to 2 to 5, both where a file is parsed and where a person's override applies. `full` draws capsule controls (buttons, chips, tabs, badges, swatches) as pills and any other control at 25 with its 45% stop. A `radius` or `control-radius` of 25 is a round corner, never a pill.
- `glass` as an object is a material: `light` and `dark` each name the fields that differ from `liquid` in that mode; an object with neither applies to both modes. Every material's tint rises to the lowest opacity that keeps `fg` at 4.5:1 over a black backdrop under dark text and a white one under light text, blended in sRGB and ignoring blur: 0.596 in light and 0.705 in dark with the default colors, so `liquid` keeps 0.60 in light and draws 0.705 in dark.
- `mode` other than `follow` beats the person's Mode.
- Unknown keys are ignored, so a file from before step 10 loads as the default look under its own name.
- Increase contrast (contrast 1 to 2) scales the surface steps, fills, washes and `divider` up by the contrast and the muted and faint mixes down by it, so surfaces stand further apart and secondary text reads closer to `fg`. It adds no edge to any kind.
- Keyboard focus changes no fill but a field's; everything else shows the ring.
- Rings on a selected pane, card or tile, and a row's bar mark, take the highlight color, not `accent`.
- Panels (the sidebar and the inspector) take the `panel` fill; panes take their ground. On a see-through window (an app's window color under full opacity, as with window blur) a bar, panel or card takes its step as a wash of the pole over the window instead of a solid color, so the window stays as see-through as the person set it. The app tells the theme (`Theme::window_opacity`), and the window's ground (`Theme::ground()`) is then that see-through fill, so whatever is drawn straight on the window, a settings page's cards included, takes its wash without being told; a pane with gaps takes no tint there, since the window under it is its ground, and its glass rim, or its hairline, marks it. Tooltips and pane chips cast `shadow-float` like every floating surface; drop zones cast none.
- Glass is neutral: a floating surface with a tone (a solid or soft toned floating chip) draws its tone's own fill, opaque, under `shadow-float`, since a toned tint can't hold text at 4.5:1 over any backdrop. Only the neutral floating chip is glass; a callout keeps neutral glass and shows its tone in its icon. A soft toned chip lays its soft fill opaque on its ground with the soft ink, a solid one its solid fill; any tone counts, `primary` included, and a solid neutral chip is solid `primary`. In HTML a floating chip (`chip.floating`, `chip.marker`, `chip.drag`) takes `data-tone` like a badge, and `data-emphasis="solid"` draws no glass.
- A press replaces the look the row shows; nothing draws its pressed wash over its hover or selected wash. Natively a press highlight (sheet rows, touch which-key rows, and menu rows and rows under a coarse pointer) paints the wash that turns the row's hover wash, or its selection when it is selected, into the pressed one; over a tint's solid selection that is the whole pressed wash. A row that has one sets no pressed style of its own; tabs and chips keep their pressed style, which replaces the hover style.
- Muted, faint and tone ink are worked out for the ground under the text, natively and in HTML, where each ground scope (`data-ground`, `data-on`) carries the inks that differ from the window's: `--penna-muted`, `--penna-faint` and `--penna-<tone>-ink`, as values. Natively they mix the ink toward the ground's fill in OKLab rather than fade it, so the tokens write the mixed color. Cards, panels and floating grounds differ in muted and faint only; tone inks follow the ground's ink, which only a solid selection changes.
- Row inset (6), outline width (1) and highlight strength (0.8 of the wash) are fixed.

Status: built (steps 10, 12, 13, 14, 16, 17, 18, 19 and 20). Theme files are knobs only, the kinds and the built-in role patches draw the table above, and a person sets the knobs themselves over the platform's. The storybook's theme creator edits these knobs and exports a theme file, the docs Look page shows their range live, and the parity baselines hold the one look. Step 12 closed the last differences between native and HTML but one: a soft or prominent button set straight on a solid selection keeps its window paint in HTML, and rows hold none that way. What else remains is raster: text, and glass blur at its edges.

## Focus and keys

How Principles 1 is kept, for every component:

- A control (button, switch, segmented control, slider, select, field, checkbox, an interactive chip or tile) is one tab stop unless it is disabled, and Space and Enter act on it the way a click does. It acts on release of a click, never on press.
- A group of parts (a tab strip, a list, a tree, a segmented control's segments, a radio group) is one tab stop: the arrow keys move within it, Home and End go to its ends. Where penna draws the parts (`Tab`, `ListItem`, `Row`, tree rows), the container that holds them owns focus and the keys and tells the focused part with `PaintState::Focused`; penna owns how that looks.
- Focus shows as the 1.5px ring outside the border box, and only from the keyboard: a press on a control does not focus it. A focused part draws the ring, never its selected fill, so focus and selection always look different; selected and focused draws both. Where a control's fill is the ring's own color (a solid button, an on switch, a row selected in the accent), the ring stands one ring width off the edge so it still shows. `foundation::focus::ring_gap` decides it from the two colors, and the themes export it to HTML as `--penna-ring-gap`.
- A menu or popover trigger opens on Enter, Space and Down; focus goes into the menu, Escape closes it and returns focus to the trigger.
- A control's tooltip shows when the keyboard focuses it, as on hover.
- Keys never leave a focused terminal or text field: app shortcuts that act while one has focus need a modifier.

## Layout primitives (built)

Six primitives lay out app screens, stories and new components. They take tokens and draw nothing. The components that existed before them already lay out with `Space` directly and move onto the primitives only when they are rewritten.

| Primitive | Does | Rust | DOM |
|---|---|---|---|
| Stack | children in a column, a gap apart | `stack(Space::GAP_GROUP)` | `.penna-stack[data-gap="group"]` |
| Inline | children in a row on one line, wrapping when asked | `inline(Space::GAP_RELATED).flex_wrap()` | `.penna-inline[data-gap="related"][data-wrap]` |
| Inset | padding around one child | `inset(Space::INSET_CARD)` | `.penna-inset[data-inset="card"]` |
| Split | a fixed side beside a main that takes the rest | `split(width, gap, side, main)` | `.penna-split[data-gap]` with `--penna-aside` |
| Center | a column no wider than a measure | `center(px(640.))` | `.penna-center` with `--penna-measure` |
| Tiles | equal tiles that fill the width | `tiles(columns, gap)` | `.penna-tiles[data-gap]` with `--penna-tiles` |

Tiles takes a column count, not a minimum width: ala's grid takes a fixed number of equal columns, so both sides do the same. The Layout story shows each primitive, native and HTML.

Arrangement geometry stays with the app: where nidus puts computers on its board, a sidebar width someone dragged, terminal cells.

## Components

Every component has a section here, written in one shape: what it is for (and what to use instead), anatomy, kinds, sizes, states, behavior, content, placement, its HTML twin, and Status. Its docs page draws each part with the real component and carries the anatomy in plain words; the values stay here. The widget and its HTML twin take the same properties.

Every component page and story shows, where they apply: sizes, variants, tones, and the states rest, hover, pressed, focused, selected, disabled, loading and error, plus icon support. Every component matches its section; the Look section lists what still differs between native and HTML.

Status marks for a component: **matches** (the code and its page follow its section), or a list of what does not yet.

### Button

A button starts an action now. To go somewhere, use a link button; inside a menu, a menu row; for a setting that applies at once, a switch; to pick one of a few views, a segmented control.

Kinds, by tone and emphasis (`ButtonVariants`):

| Kind | Call | Paint | Use |
|---|---|---|---|
| Primary | `.primary()` | `Tone::Primary`, `Emphasis::Solid` | the main action of a view: at most one per dialog, form or pane |
| Secondary | default | `Tone::Neutral`, `Emphasis::Soft` | every other action |
| Ghost | `.ghost()` | `Tone::Neutral`, `Emphasis::Ghost` | actions in bars, toolbars and rows, where a fill at rest is noise; it fills on hover |
| Outline | `.emphasis(Emphasis::Outline)` | the tone's outline ink, no fill | a secondary action on a ground that has its own fill, where a soft fill would blend in |
| Danger | `.danger()` | `Tone::Danger`, `Emphasis::Soft` | an action that destroys something or can't be undone; the confirm button of a destructive dialog |
| Link | `.link()` | underlined ink, no padding | goes somewhere: a page, a URL, a settings pane; sits in text or at a row's end |
| Text | `.text()` | ink, no padding, muted while pressed | a plain action inside a line of text |

There are no warning or success buttons: status colors mean status (Principles 3). A notice's or callout's action is a primary placed on its ground (`.primary().on(Notice::ground(tone, cx))`), which paints it in that tone.

Apps' chrome uses the kit looks in `ButtonLook` (`Bar`, `Chrome`, `RowIcon`, `Floating`, `Send`, and the rest in kit.md); they follow this section like any button. `Floating`, a button over the content such as jump to latest, is glass (Look).

Anatomy, left to right, all from the button's rung of the control ladder (Size):

- Container: height `control_px`, radius the theme radius capped at 45% of the height, inset `inset` at both sides (8, 8, 12, 16, 16, 16 from `xs` to `touch`). Link and text buttons have no container inset.
- Icon (optional): `icon_px` (12, 12, 14, 16, 16, 16), before the label, `icon_gap` from it (4, 4, 6, 6, 8, 8). While loading it is a spinner of the same size.
- Label: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`) at weight 400. A primary's label takes whichever of near-white or near-black reads better on its fill.
- Caret (optional, `.dropdown_caret(true)`): a chevron at the end, `icon_gap` after the label, for a button that opens a menu. It is the same mark as a select's chevron at that size: a rung smaller than the button's icon from `md` up.
- Focus ring: 1.5px, outside the container, its own radius, one ring width further out on a solid fill (Corners, depth, focus, motion).

Icon only: the button is square (width = height) and must carry a tooltip that names the action, which is also its accessible name. A row's trailing actions use `Button::compact_icon` (24, ghost).

Sizes: `md` by default (follows density). `sm` and `xs` in toolbars, lists and rows that repeat; `lg` and `xl` for the one main action of a sparse screen (an empty state, a first run); `touch` under a coarse pointer. Every button in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the kind's paint | |
| hover | the paint's hover step (a lift) | not under a coarse pointer, not when selected or disabled |
| pressed | the paint's active step, while held | not under a coarse pointer, which draws the press highlight instead |
| focused | the focus ring; fill unchanged | from the keyboard only, never from a click; one ring width off a solid fill |
| selected | the picked look (`data-state="current"`) | a button that stands for an on state in a group |
| disabled | faint label, no hover or press | leaves the focus order; its tooltip still shows |
| loading | a spinner in the icon's place, or before the label when there is no icon | clicks are ignored; the label stays |

In a window that is not active, a glass button flattens and its label fades, like the rest of the window's chrome.

Behavior: it fires on release of a click or tap inside it, and on Space or Enter while focused. A button with a caret opens its menu below it. A tooltip shows after the hover delay, and at once when the keyboard focuses the button.

Content: the label is a verb or a short verb phrase in sentence case, one to three words ("Save", "Add host", "Open in browser"), with no closing period. It names the action, never "OK" or "Yes". End it with an ellipsis ("Allow…") when the action asks for more before it happens, such as opening a dialog. A danger button names what it removes ("Delete branch").

Placement: buttons in a group sit `gap-related` 8 apart. A dialog's or a form's actions sit at its end, aligned to the trailing edge, the primary last and its cancel just before it. A destructive dialog confirms with a danger button, not a primary one. Ghost buttons in a bar sit `inset-chrome` from its edges.

HTML: `.penna-button` with `data-penna` (the recipe: `button`, `button.quiet` for ghost, `button.outline`), `data-tone`, `data-prominent` (solid), `data-size` (`xsmall` to `touch`), `data-state="current"` (selected), `data-variant` (`link`, `text`), `disabled`, `data-loading`; the label is `.penna-label`, the icon `<i data-icon>`, and a button with a caret takes `data-caret` and holds a `.penna-caret`.

Status: matches.

### Badge

A badge tags something with a count or a short word, in a line of text or at the end of a row: "2/3", "New", "3 failing". To show the state of a thing with a dot and a detail, use a chip; to show a shortcut, a keycap; for plain text, a label.

Kinds, by emphasis (`.emphasis()`), each in any tone (`.tone()`): neutral by default, primary, accent, danger, warning, success. The theme paints the pair like any other piece with that tone and emphasis.

| Kind | Call | Use |
|---|---|---|
| Soft | default | most badges: a count, a tag, a status in its tone |
| Solid | `.emphasis(Emphasis::Solid)` | the one badge in a view that must stand out ("New") |
| Outline | `.emphasis(Emphasis::Outline)` | a quiet tag that says where something comes from (the settings provenance badge, a version) |
| Ghost | `.emphasis(Emphasis::Ghost)` | ink only, no fill or edge |

`.on(ground)` paints it for the ground it sits on. `.opaque()` flattens its fill onto that ground's fill, so it looks the same but hides what is under it: a picture, or the edge of a message bubble it overlaps.

Anatomy, left to right:

- Container: height 18 (the `body` line, `TextStyle::Body.line_height()`) from `md` up, 16 (the `small` line) at `xs` and `sm`; inset `space-6` (6) at both sides, `space-4` (4) at `xs` and `sm`; radius the theme radius capped at 45% of the height, so a pill.
- Icon (optional, `.icon()`): before the label, 12 at every size (the smallest icon, `Size::XSmall`); `space-4` (4) from the label.
- Label: `caption` (11 on 16) at 500, `micro` (10 on 14) at `xs` and `sm`; it never wraps.
- Children (optional): after the label; `Badge::empty()` has no label and takes only children.

Sizes: `md` by default (18), beside `body` text; `sm` (16) beside `small` text. `xs` draws like `sm`, and `lg`, `xl` and `touch` like `md`, since a badge sits in a line of text. `Size(px)` takes its rung's look.

States:

| State | Look | Notes |
|---|---|---|
| rest | the kind's paint | the only state: a badge is read-only, so it has no hover, pressed, focused, selected or disabled look |

Behavior: none. It takes no pointer or keyboard input and is never in the focus order; the row or control around it handles both.

Content: a count, or one to three words in sentence case ("3 failing", "Waiting"), no closing period. A status tone always comes with a word or an icon that says the status (Rules 8).

Placement: after the text it tags, `gap-related` (8) from it, or at the end of a row. Pick its size from the line it sits in.

HTML: `.penna-badge` with `data-penna` (`badge`, or `badge.outline` for outline), `data-tone`, `data-size` (`xsmall`, `small`), `data-emphasis` (`ghost`, or `solid` with `data-solid="<tone>"` and the label and icon wrapped in `.penna-solid-ink` as on a chip; `penna.css` holds these rules for both) and `data-opaque` with `data-on` naming the ground by its role (`data-on="card.bubble"`; the window without it): the tokens give each `data-on` that ground's fill as `--penna-ground`, and `badge.css` lays the badge's fill over it. The label is a `span.badge-text` marked `data-penna-text`, the icon `<i data-icon>`.

Status: matches.

### Bar

A bar is a strip across a sheet, a pane or the window that holds chrome: a title, a few ghost buttons, a notice across the top of a picker. It recedes so the content leads. For a box, use a card; for a plain line between things, a separator; for the window's title bar with its window buttons and tabs, the title bar block.

Looks (`BarLook`, `Bar::new(id, look)`):

| Look | Recipe | Paint | Use |
|---|---|---|---|
| Plain (default) | `bar` | no fill, no outline, no shadow, square; announced as a toolbar | sheet headers, a strip over a panel (`.on(ground)` there) |
| Pane | `bar.pane` | as Plain, with fixed knobs; announced as a toolbar | pane headers, a browser's tabs and toolbar |
| Window | `titlebar` | as Plain, with fixed knobs; announced as a title bar | the strip across the top of the window |

None of the three paints anything in the look: a bar takes the color of what it sits on, and its edge rule sets it apart.

Anatomy:

- Height: `bar`, the theme's `pane_header_height()`: 40 when panes sit flush (the default, `pane_margin` 0), 34 (`control-chrome` 22 + 2 × `inset-chrome`) when panes have a margin. The title bar is that plus 2 × `pane_margin`, or plus one `pane_margin` when it lies across the top of the panes rather than beside them.
- Inset: `inset-chrome` 6 between the edge and its controls.
- The bar sets its height and inset itself; a caller's height or padding replaces them.
- Controls: ghost, all one size (Rules 6), `control-chrome` 22 in pane headers, `gap-related` 8 apart. Labels are `body`, a window title `body` at 600.
- Edge: `.rule(RuleLook::Bar, edge, cx)` draws a 1px divider on the side that meets content.
- Ground: `.on(ground)` puts it on a ground other than the window's, such as a picker; what sits on it reads `cx.theme().ground_of(look)`.

Behavior: none of its own. Its controls are focusable and act as themselves; the bar doesn't light, take focus or move.

Placement: across the top of a sheet, pane or window, or across the top or bottom of a panel, at window level. It runs edge to edge across what holds it, with no inset around it.

HTML: `.penna-bar` with `data-penna` set to `bar`, `bar.pane` or `titlebar`, and `role="toolbar"` (`role="banner"` on the title bar, which has no ARIA role of its own); `bar.css` lays it out as a centered row with square corners, `--penna-bar` tall (the theme's `pane_header_height()`; HTML has no pane margin, so the title bar is as tall) with `inset-chrome` at the sides. It sets the height and inset at the weight of one class, so a page's own class or inline style replaces them. An edge rule is `data-rule` (`rule.bar` and the other rule looks) with `data-edge` (`top`, `right`, `bottom` when absent, `left`, or several, as `top left`) on any element, and `penna.css` draws it: a 1px border in the divider ink on that edge, taking its pixel of room as `.rule()` does natively. Every rule look draws in the one look, so nothing checks the theme first.

Status: matches.

### Card

A card is a box in the flow of content that holds one thing: a message, a block of code, a table, an image. It rests where the content puts it. For something raised over the view, use a panel; for a group of settings, a settings card; for a status message, a notice; for something you pick or drag, a tile.

Looks (`CardLook`, `Card::new(id, look)`):

| Look | Recipe | Paint | Use |
|---|---|---|---|
| Bubble | `card.bubble` | solid, one step above its ground, no outline, no shadow | your messages in an agent chat |
| Code | `card.code` | solid, one step above its ground, no outline, no shadow | code blocks, diagrams and inline code in messages |
| Figure | `card.figure` | no fill of its own, no outline, no shadow; announced as a figure | tables in messages |
| Image | `card.image` | as Bubble; announced as a figure | image tiles in messages and the composer |

Anatomy:

- Container: the look's paint, the `radius` knob (12 by default, 8 in Yaru). No outline.
- Inset: `inset-card` 12 all round, set by the card; a caller's padding replaces it.
- Content: the caller's, with its own text style (`body` for a message, `mono` for code).
- Ground: `.on(ground)` paints the card on a ground other than the window's, such as a panel. What sits on the card passes `cx.theme().ground_of(look)` to `.on()`, so a badge or muted label inside follows the card's fill.

Behavior: none. A card doesn't light on hover or take focus.

Placement: cards sit in the content, one step above what holds them: window `bg`, then `panel`, then cards, then popover. Cards in a column sit `gap-group` 16 apart; a heading sits `gap-related` 8 above its card. The parent sets both.

HTML: `.penna-card` with `data-penna` set to `card.bubble`, `card.code`, `card.figure` or `card.image`; Bubble and Code take `role="group"` and Figure and Image `role="figure"`, as natively. `card.css` sets `box-sizing` and the `inset-card` padding at the weight of one class, so a page's own class or inline style replaces it; the caller lays out the inside. A card inside a container marked `data-ground` rises one step from that container's fill, as `.on(ground)` does natively: the tokens write every card look under each container's ground. `data-ground` on the card makes controls inside paint on its ground.

Status: matches.

### Checkbox and radio

A checkbox turns one option on or off by itself; a radio picks one option of a set, all in view. Both take effect when the form or dialog they sit in is confirmed. For a setting that applies at once, use a switch; for one of two to five modes that applies at once, a segmented control; for one of many, a select.

Kinds:

| Kind | Call | Use |
|---|---|---|
| Checkbox | `Checkbox::new(id).label(text).checked(on)` | an option that is on or off by itself |
| Mixed checkbox | `.indeterminate(true)` | a checkbox over a set of checkboxes that are partly on |
| Radio group | `RadioGroup::new(id, labels).selected(index)` | one of two to seven options; more go in a select. `.selected(None)` while none is picked |
| Mark | `Check::checkbox(on)`, `Check::radio(on)`, `.indeterminate(true)` for the minus | the box alone, in a row, tile or task list that takes the press itself |

Anatomy, left to right:

- Box: 14 at `xs` and `sm`, 16 from `md` up; `Size(px)` takes that side, as a task list does to match its font. Off, it takes the tone's outline paint (an edge, no fill; neutral by default); on or mixed, the tone's solid fill (`accent` by default, `.tone()` to change it). A checkbox's corner is a quarter of the theme radius, at most a quarter of the side (3 in a 16 box with the default radius, 2 in Yaru). A radio is round.
- Mark: a checkbox's `check` icon at 70% of the side (11, or 10 in a 14 box); a mixed checkbox's `minus` at the same size; a radio's dot at 37.5% of the side (6, or 5). It takes the fill's ink.
- Label (checkbox and radio group): the rung's text style (`caption`, `small`, then `body` from `md` up), `icon_gap` after the box (4, 4, then 6). The box is centered on the label's first line, on its capitals (`cap_center_drop`), and a long label wraps under itself, never under the box.
- `.on(ground)` paints it for the surface it sits on, such as a picked tile.

A radio group stacks its radios `gap-related` 8 apart, or lays them in a row `gap-group` 16 apart with `.horizontal()`.

Sizes: `md` by default; `xs` and `sm` beside `caption` and `small` text. From `lg` up it draws like `md`, since it sits in a line of text (Size). Under a coarse pointer the hit area grows to the touch target: 44 tall around the label's line, and 44 wide around the box.

States:

| State | Look | Notes |
|---|---|---|
| off | the edge only | |
| on | the tone's fill with the mark | |
| mixed | the tone's fill with the minus | a checkbox only |
| hover | the box takes its paint's hover step | over the box or the label; not under a coarse pointer, not when disabled |
| pressed | the box takes its paint's active step, while held | not under a coarse pointer |
| focused | the focus ring, 1.5 outside the box, one ring width off it when its fill is the ring's ink (a toned box), in the tone's focus ink | from the keyboard only, never from a click; a radio group rings its picked radio, or its first while none is picked |
| disabled | the paint's disabled step: fainter edge, fill and mark; the label in `fg-faint` | presses and keys are ignored, and it leaves the focus order |
| error | `.tone(Tone::Danger)`: the edge, or the fill, in the danger tone | its Field shows the message with an icon (Field) |

Behavior: a checkbox flips on release of a click or tap on its box or its label, and on Space or Enter while focused, and calls `on_click` with the new value; the caller sets `checked`. A mixed checkbox turns on. A radio group is one tab stop: a click on a radio picks it; Up and Left pick the previous radio, Down and Right the next, Home and End the first and last, each through `on_select`, and they stop at the ends; while none is picked they step from the first. Space and Enter pick the ringed radio while none is picked, and pass on to the form once one is, so Enter still confirms a dialog. A click never focuses either. The mark alone has no behavior: the row or tile that holds it takes the press, the focus and the keys, and sets `checked`.

Content: the label names the option in sentence case with no closing period, phrased so that on means yes ("Restore sessions", not "Don't restore sessions"). A radio group's labels are short and parallel, and the Field around it names the set ("Start with").

Placement: in a form, checkboxes stack one per line `gap-related` 8 apart; a checkbox that needs help or an error, and every radio group, sits in a Field. The mark sits before its label in a list or task row, or at the trailing end of a tile, in place of its key, once that tile is picked.

HTML: the mark is `.penna-check` (in `check.css`, with the controls): `data-on` with an `<i data-icon="check">` inside, `data-mixed` with an `<i data-icon="minus">`, `data-kind="radio"` for a radio (its dot is drawn by the CSS), `data-size` (`xsmall`, `small`) for the 14 box, `data-tone` (`danger`, `warning`, `success`), and `data-disabled`. Its fills read the theme's `--penna-outline-hover` and `--penna-outline-pressed` (the neutral edge's steps) and `--penna-<tone>-hover`, `--penna-<tone>-pressed` and `--penna-on-<tone>` (a tone's solid steps and ink). A checkbox is `label.penna-checkbox` with `aria-checked` (`true`, `false`, `mixed`), `data-size`, `data-tone` and `aria-disabled="true"`, holding a `.penna-check` and a `.penna-label`. A radio group is `.penna-radio-group` with `data-value` (the picked index), `data-orientation="horizontal"`, `data-size`, `data-tone` and `aria-disabled="true"`, holding a `.penna-radio` per option, each a `.penna-check[data-kind="radio"]` and a `.penna-label`. `check.js` sets the roles (`checkbox`, `radiogroup`, `radio`), makes each checkbox and each group one tab stop, answers the clicks and keys above, keeps `aria-checked` and the marks in step, rounds each box and its ring, and fires `change` with `{ checked }` or `{ index }`. `:focus-visible` or `data-state="focused"` draws the ring. Reading text's task list draws its own `.md-check` (`role="checkbox"`, `aria-checked`).

Status: matches.

### Chip

A chip names the state of a thing at control size: a computer that is ready, the host a pane runs on, the mode an editor is in. It leads with a dot or an icon and can add a muted detail. For a count or a tag inside a line of text, use a badge; to start an action, a button; to pick one of a few options, a segmented control.

Kinds: tone and emphasis paint it like a badge (`.tone()`, `.emphasis()`): soft by default, solid, outline or ghost, in neutral (the default), primary, accent, danger, warning or success. Its look (`.look(ChipLook)`) fits it to where it sits:

| Look | Call | Use |
|---|---|---|
| Plain | default | status in a pane, a list or a dialog |
| Bar | `ChipLook::Bar` | status in a title bar or a pane header |
| Floating | `ChipLook::Floating` | over a pane's content: a pane number, an editor mode |
| Marker | `ChipLook::Marker` | the display-panes markers; an accent ring lights it |
| Drag | `ChipLook::Drag` | the chip that follows a dragged pane |
| Icon | `ChipLook::Icon` | the disc behind a blank page's icon |

Anatomy, left to right, all from the chip's rung of the control ladder (Size):

- Container: height `control_px` (20, 24, 28, 32, 40, 44 from `xs` to `touch`); full pill corners; inset `inset` at both sides (8, 8, 12, 16, 16, 16).
- Lead (optional): a dot (`.dot(tone)`), 6 across, in its tone (a neutral dot is muted ink; on a solid chip every dot is the chip's ink), or an icon (`.icon()`) at `icon_px` (12, 12, 14, 16, 16, 16).
- Label: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`) at 500.
- Detail (optional, `.detail()`): a middle dot and one short fact in the same style, muted. It is the only part that shrinks, and it ends in an ellipsis when it runs out of room.
- Gaps: `icon_gap` between every part (4, 4, 6, 6, 8, 8).
- Children (optional): after the detail; `Chip::empty(id)` has no label and takes only children.

Sizes: `sm` (24) by default, not `md`. `xs` (20) in dense rows and lists; `md` and up beside controls of that size; `touch` under a coarse pointer. Every control in one row has the same size (Rules 6).

States (`.state(PaintState)`, `.selected(true)`):

| State | Look | Notes |
|---|---|---|
| rest | the look's paint | |
| hover | marker: the accent ring | other looks keep their rest paint; lights on its own only with `.interactive(true)` |
| pressed | marker: the accent ring | as hover |
| focused | the look's rest paint and the focus ring, 1.5 outside | from the keyboard on a chip that takes focus, or set by the app |
| selected | a stronger fill in its tone; marker: the accent ring | |
| current | as selected; `.ring(color)` recolors the marker's ring | the marker of the pane in front |
| disabled | faded fill and ink | |

Behavior: read-only by default: the pointer passes through it and it is never in the focus order. `.interactive(true)` lights it on hover and press and announces it as a button; the caller adds the click. An interactive chip with a click handler is a tab stop: Space and Enter run it, and it shows the focus ring from the keyboard only, since a click never focuses it.

Content: the label is the state ("Ready", "Paused") or the thing's own name ("ubuntu"), one or two words, sentence case unless it is a name. The detail adds one fact ("this Mac", "offline"). A status dot always sits beside a word that says the status (Rules 8).

Placement: chips in a row sit `gap-related` (8) apart.

HTML: `.penna-chip` with `data-penna` (the look and emphasis: `chip`, `chip.outline`, `chip.bar`, `chip.floating`, `chip.marker`, `chip.drag`, or `badge.icon` with `data-look="icon-disc"`), `data-tone`, `data-size` (`xsmall` to `touch`; small when absent), `data-state` (`hover`, `active`, `focused`, `selected`, `current`, `disabled`), `data-emphasis` (`ghost`, or `solid` with `data-solid="<tone>"` and the dot and label wrapped in `.penna-solid-ink` carrying the solid button's `data-penna="button" data-prominent`, or `data-solid-ink="accent"` for the accent, which has no prominent button and takes `--penna-on-accent`), `data-interactive` (with `role="button"` and `tabindex="0"` when it takes focus; `chip.js` runs it on Space and Enter and keeps a click from focusing it), `data-ring="<tone>"`, and `data-detail` when it holds a detail. Its parts: `i.penna-dot[data-tone]`, `<i data-icon>`, `.penna-chip-label` and `.penna-chip-detail` (which `chip.js` truncates), each text run marked `data-penna-text`.

Status: matches.

### Field

A field frames a form control with what people need to fill it in: a label above it, help under it, and an error message in place of the help when the value can't be used. Use it around an input, select, checkbox or radio group in a form, dialog or sheet. A settings row names its control with its title and description instead.

Kinds: one, `Field::new().label(text).help(text).error(text).child(control)`.

Anatomy, top to bottom, the parts `gap-tight` 4 apart:

- Label (optional, `.label()`): `body` at 500, in the ink.
- Control: the input, select, checkbox or radio group, as the caller built it.
- Help (optional, `.help()`): `small` in `fg-muted`.
- Error (optional, `.error()`): in place of the help while it is set, `small` in `danger` (the role's own color, text and icon alike), after a 12 `circle-x` icon `space-4` before the text and centered on its first line's capitals: the icon sits in a box one `small` line (16) tall, dropped by `cap_center_drop`. The line is the `sm` rung of the control ladder: its text, icon and icon gap.

A field is as wide as its parent, and its help and errors wrap at that width. It never stretches its control: an input fills the width by itself, and a select stays as wide as its value unless the caller sets a width.

States:

| State | Look | Notes |
|---|---|---|
| rest | label, control and help | |
| error | the error replaces the help; the caller paints the control with `.tone(Tone::Danger)` | the icon and the words carry the error, never the color alone (Rules 8) |

A disabled control keeps its label and help as they are.

Behavior: none of its own. Show an error once the person leaves the control or confirms the form, never while they type, and clear it with the edit that fixes the value.

Content: the label is a noun or a short noun phrase in sentence case, with no colon and no closing period ("Host name"). Help says what the value is for or what shape it takes ("Other computers see this name."). An error says what is wrong and how to fix it ("Use letters, numbers and dashes."), never only "Invalid". Help and errors are one short sentence, with its period.

Placement: fields in a form sit `gap-group` 16 apart. Two short fields can share a line `gap-related` 8 apart, each with its own label.

HTML: `.penna-field` holding `.penna-field-label`, the control, and `.penna-field-help` or `.penna-field-error` (an `<i data-icon="circle-x">` before its text). Text runs are spans marked `data-penna-text`; `field.css` holds the twin.

Status: matches.

### Icon

An icon is a glyph from the kit's set that marks an action or a thing beside its label, or stands in for the label in a bar, where a tooltip names it. For loading use a spinner, for a key a keycap, and for a logo the app's own file (`IconName::Asset`).

Kinds:

| Kind | Call | Draws |
|---|---|---|
| Glyph | `Icon::new(IconName::Bell)`, or an `IconName` wherever a component takes an icon | one of the set's 95 glyphs, drawn for the theme |
| Asset | `IconName::Asset("icons/logo.svg")`, `Icon::empty().path(path)` | the app's own SVG, as it is |

The set: 95 glyphs in eight groups (chrome and layout, actions, things, navigation, keys, status, people and agents, theme), each on a 24 grid with a 1.5 stroke, no fill, in the current color. Corners take half the theme radius in grid units with the kit's corner smoothing, capped at 0.45 of the shorter side like every corner the kit draws; at radius 0 corners are sharp, caps square and joins mitered. So the set changes with the theme, natively and in each theme's `-icons.css`.

Anatomy: a square box the icon's size, the glyph scaled to fill it, in the color of the text around it unless `.text_color()` sets one.

Sizes, the Icon column of the control ladder (`Size::icon_px`):

| Size | px | Beside |
|---|---|---|
| `xsmall`, `small` | 12 | `caption` and `small`, menu rows included |
| `medium` | 14 | `body`; list and tree rows keep 14 beside `lead` |
| `large`, `xlarge`, `touch` | 16 | `lead` and `heading` |
| `Size(px)` | that px | custom shapes, such as the title bar's 17 |

With no size an icon is `medium` (14), natively and in HTML.

States: none of its own. Inside a control it takes the control's ink, so it follows hover, selected and disabled with the label.

Behavior: `.transform(Transformation::rotate(..))` turns it in paint only, so layout doesn't move (a disclosure chevron). A change of theme radius redraws the glyphs.

Content: one meaning per glyph across an app; the same action keeps the same icon, and no icon stands for two actions. Give it a label wherever there is room. Alone, it sits only on a square icon button in a bar, toolbar or row, with a tooltip that names the action and is its accessible name (Button). Never add one for decoration (Principles 2). A status icon takes its status color and, where the glyph alone could be misread, a word.

Placement: before its label, `icon_gap` from it (4 at `xs` and `sm`, 6 at `md` and `lg`, 8 at `xl` and `touch`), centered on the label's capitals: center the line box, and drop the icon by `cap_center_drop(font_size, icon_size, cx)` where the face sits its capitals off center. A marker beside a first line (notices, callouts, notifications) sits in a box as tall as that line.

HTML: `<i data-icon="name">` with the kebab-case `IconName` (`panel-right`, `circle-check`), drawn as a mask from the theme's `-icons.css` painted in `currentColor`. Its size is `--penna-icon-size`, 14 when nothing sets it; components set it for their rung, and with `icon.css` loaded `data-size` sets it on the icon (`xsmall` to `touch` from the ladder, `6` for 24). `data-tone` (`foreground`, `muted`, `accent`, `success`, `warning`, `danger`) sets the color, `data-turn` (1 to 3) turns it by quarters. A `.penna-glyph` around it lets `icon.js` thicken the stroke to match the native raster.

Status: matches.

### Input

A text field takes text the person types: a name, a path, a search. To pick from a fixed list, use a select; for a number with steps, a number input; for on or off, a switch.

Kinds (`Input` on an `InputState`):

| Kind | Call | Paint | Use |
|---|---|---|---|
| Field | default | the field role: a soft fill and a hairline edge | every text field |
| Outline | `.emphasis(Emphasis::Outline)` | an edge, no fill at rest | on a ground with its own fill, where the soft fill would blend in |
| Multi-line | `auto_grow(min, max)` on the state | the text area role | notes and messages: grows from `min` to `max` rows, then scrolls |
| Number | `NumberInput::new(&state)` with `step`, `min`, `max` on the state | a field with a ghost minus and plus button at its ends, the value centered | a number people nudge |
| Bare | `.appearance(false)` | no fill, edge or focus edge | a field drawn by its container, such as a title in a rule |
| Borderless | `.bordered(false)` | the fill with no edge | a field on a ground where a hairline is noise |

Password fields are a field with `.content_type(InputContentType::Password)`: the text is masked.

Anatomy, left to right, all from the field's rung of the control ladder (Size):

- Container: height `control_px`, radius the theme radius capped at 45% of the height, edge at least 1px, the field inset at both sides (6, 6, 8, 10, 12, 12 from `xs` to `touch`), `space-2` above and below from `md` up and none at `xs` and `sm`.
- Prefix (optional, `.prefix()`): before the text, `icon_gap` from it (4, 4, 6, 6, 8, 8), at most 75% of the field's width. An icon in it is `icon_px` (12, 12, 14, 16, 16, 16), in the muted ink.
- Text: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`); the placeholder in the field's faint ink; the selection a wash of the ink; the caret one line tall.
- Suffix (optional): after the text, `icon_gap` apart: the spinner while loading, the clear button when `.cleanable(true)` and there is text, then the caller's `.suffix()`. The spinner and clear button are a rung smaller than the field from `md` up (Size).

Sizes: `md` by default (follows density). `xs` and `sm` in toolbars and dense rows, `lg` and `xl` on a sparse screen, `touch` under a coarse pointer. Every field in a row has the same size as the controls beside it (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the kind's fill and edge | |
| hover | the fill's hover step | not when disabled |
| focused | the edge turns to the ink; the fill stays | from a click or the keyboard; the caret blinks only while the window is active |
| disabled | a fainter fill and edge; text, placeholder and affixes at half strength | no editing |
| loading | a spinner in the suffix | the text stays editable |
| error | `.tone(Tone::Danger)` paints the field in the danger tone: its edge, at rest and focused | its Field shows the message with an icon (Field) |

A tone paints only the edge: the tone's outline ink at rest and hovered, its focus ink when focused, and half of it when disabled. The fill, its hover step, the text and the placeholder stay the field's, and the edge draws even though a field has none at rest. Every tone paints this way; forms use danger.

A selection stays painted while the field is not focused.

Behavior: a click places the caret and a drag selects. The keyboard edits as the platform does: arrows, word jumps (Alt, or Ctrl), line ends (Cmd, Home, End), Shift to select, Cmd-Z and Shift-Cmd-Z, cut, copy and paste. Enter emits `PressEnter`; in a multi-line field it inserts a line break unless the state submits on Enter, and Shift-Enter always breaks the line. Escape closes the context menu or ends a composition, then passes up. `validate` rejects an edit that fails it. The clear button empties the field and focuses it, and is never a tab stop. A number input steps with Up and Down and with its buttons, clamps to `min` and `max`, and keeps the step's decimals.

Content: the placeholder hints at what to type ("Search sessions", "Write a commit message"), in sentence case with no closing period. It is never the only label, since it disappears with the first key.

Placement: in a form a field sits in a Field, which puts its label above it and its help or error under it, `gap-tight` 4 away; in settings it sits at the end of a row whose title names it. Fields sit `gap-group` 16 apart.

HTML: `label.penna-input` with `data-penna="field"` (`field.outline` for outline; `field.multiline` with `data-multiline` around a `<textarea>` that takes `data-min-rows` and `data-max-rows`), holding an `<input>`; `data-size` (`xsmall` to `touch`), `data-tone="danger"` for an error, `data-disabled` with `aria-disabled="true"` and `disabled` on the input, `data-bare`, `data-borderless`, `data-align` (`center`, `right`), and `data-selection="start,end"` on the input for a resting selection. `.penna-input-prefix` and `.penna-input-suffix` sit around the input; the suffix holds `.penna-spinner` while loading and `.penna-input-clear` (a ghost icon button a rung smaller) when cleanable. A number input is `.penna-number` (`data-penna="field"`, `role="spinbutton"`, `data-step`, `data-min`, `data-max`) holding a bare centered field between two buttons with `data-step-by`. `input.js` paints the caret and the resting selection the way ala does, sets `data-state="focused"`, grows text areas and steps number inputs.

Status: matches.

### Keycap

A keycap shows a keystroke the way it is pressed: a field's shortcut hint, a shortcut in a settings row, the which-key prefix, the key inside a button that submits. Menus and tooltips show their shortcuts as plain text (`Kbd::text()`) instead; to tag something with a word, use a badge.

Kinds:

- `Kbd::new(keystroke)`: one keystroke, a key or a chord of modifiers and a key, spelled for the platform. `Kbd::binding_for_action` finds the keystroke bound to an action.
- `Keycap::new(label)`: any short label in the same cap ("↵", "1").

Anatomy:

- Cap: height 18 (the `body` line) from `md` up, 16 (the `small` line) at `xs` and `sm`, and at least as wide as it is tall; inset `space-4` (4) at both sides; the keycap paint (a faint fill and edge); radius the theme radius capped at 45% of the height.
- Glyphs: modifiers, tab, return, escape, backspace and the arrows are icons at 1.2 times the text size (13, or 12 at `xs` and `sm`), with ISO marks off Apple platforms, where the command key reads "Win". Everything else is text, and text parts run together with "+" off Apple platforms. Parts sit `space-2` (2) apart.
- Text: `caption` (11 on 16) at 400, `micro` (10 on 14) at `xs` and `sm`, in muted ink. A letter shows uppercase unless `.lowercase()`.

Sizes: `md` by default (18), beside `body` text; `sm` (16) beside `small` text. `xs` draws like `sm`, and `lg` and up like `md`, since a keycap sits in a line of text. `Size(px)` takes its rung's look.

States:

| State | Look | Notes |
|---|---|---|
| rest | the keycap paint, muted ink | the only state: a keycap is read-only, so it has no hover, pressed, focused, selected or disabled look |

On a fill, `.on(ground)` gives it the paint and ink that read there, as inside a solid button.

Behavior: none. It takes no pointer or keyboard input and is never in the focus order. With `.id()` it is announced as a label named by the spelled-out keystroke ("⌘K", "Ctrl+K").

Content: one keystroke per cap. A sequence, such as a prefix and then a key, is one cap per keystroke. Use `.lowercase()` only where the case of the key matters.

Placement: the caps of one sequence sit `gap-tight` (4) apart; a cap sits at the end of the row or field it belongs to.

HTML: `<kbd class="penna-kbd" data-penna="keycap">` with `data-kbd`, the keystroke as the keymap spells it (`cmd-shift-p`), which `kbd.js` fills with glyphs and an `aria-label`; `data-kbd-lowercase`; `data-size` (`xsmall`, `small`). A fixed label holds its text in a `span` marked `data-penna-text`, as `kbd.js` writes it. `data-kbd-text` on any element fills it with the spelled-out keystroke.

Status: matches.

### Label

A label is one run of interface text in a text style: a title, a row's name, a description, a value. For text people read (documents, chat), use the text component; a button, chip or menu row draws its own label.

Kinds, on any text style (`Label::new(style, text)`):

| Kind | Call | Use |
|---|---|---|
| Full | default | the ink of the text around it |
| Muted | `.muted()` | descriptions, details, secondary values |
| Faint | `.faint()` | placeholders and disabled text only |
| Tone | `.tone(tone)` | a word that is a status ("Ready", "Failed") |
| Strong | `.strong()` | emphasis: 500 in the interface styles, `touch-small` and `touch-body`; 600 in `heading`, `title`, `display`, `touch-title` and the reading styles |
| Code | `.mono()` | the code face at the style's own size |

Anatomy: the text alone, with no box: size, line height and weight from its text style (Type). Muted, faint and tone ink are worked out for the ground under it; on a card or another fill, pass `.on(ground)`. `.truncate()` keeps it to one line that ends in an ellipsis.

Sizes: the text styles: `micro` (10 on 14), `caption` (11 on 16), `small` (12 on 16), `body` (13 on 18), `lead` (14 on 20), `heading` (16 on 22), `title` (20 on 26), `display` (26 on 32), `mono` (13 on 18, code face), and the reading and touch styles. Most text in a view is `body`.

States:

| State | Look | Notes |
|---|---|---|
| rest | its ink level | a label takes no input, so it has no hover, pressed, focused or selected look |
| disabled | faint | inside a disabled control |
| error | the danger tone | always a word that says what is wrong |

Behavior: none. It takes no pointer or keyboard input. It wraps when it runs out of room, unless it truncates.

Content: sentence case, no closing period on a single line. Two or three text styles in a view (Principles 2). Faint is under 4.5:1 on purpose, so it never carries text people need to read (Color).

Placement: a label and what it names sit `gap-tight` (4) apart: a title and its description, a field label and its field.

HTML: `.penna-type` with `data-type` (the style's name: `micro` to `display`, `mono`, `read-h1` to `read-body`, `touch-small` to `touch-title`), `data-strong`, `data-muted`, `data-faint`, `data-tone` (`primary`, `danger`, `warning`, `success`, `accent`), `data-mono` (the code face at the style's own size) and `data-truncate`; the run is marked `data-penna-text`. It wraps unless `data-truncate`. Muted, faint and tone take `--penna-muted`, `--penna-faint` and `--penna-<tone>-ink`, which the surface around it and each ground scope (`data-ground`, `data-on`, a tint's solid selection) set for their ground, as `.on(ground)` does natively.

Status: matches.

### List

A list shows a column of things to open or pick: hosts, sessions, files, a picker's results. Each row is a `ListItem`. For actions use a menu; for things nested in each other, a tree; for a fill behind a row the app lays out itself, a row.

Looks: a `ListItem` takes any row look (`.look(RowLook)`, see Row). The menu look (`menu-item`) is the default; side panels use the sidebar look, suggestions the menu tint.

Anatomy:

- Row: at least `row_height` tall, `control-md` by default (28, follows density); inset `space-12` at both sides; its children `space-4` apart, spread to both ends. Corners: a control's, `roundness` × the row height (Corners, under Look). The look paints its fill, outline and highlight.
- Icon (optional): 14, in muted ink (`fg-muted`), `space-8` before the label.
- Label: `lead` (14 on 20), centered in the row height; it takes the width left.
- Detail (optional): `small`, muted, at the end.

Rows in a list: an inset list (`list_inset`, as in the look) puts the row inset at the sides and the rows `space-2` apart, so their rounded highlights never merge; a plain list's rows touch.

Sizes: one. Rows follow density (24, 28, 32) and the text stays `lead`. `.row_height(height)` makes a taller row and sizes its corners for it: a pane's own list rows (`picker::list_row_height`) are `control-lg` 32.

States:

| State | Look | Notes |
|---|---|---|
| rest | the look at rest | |
| hover | the look's hover step | only at rest |
| pressed | the look's active step, while held | only at rest |
| selected | the look's selected fill | `.selected(true)`; no hover or press over it |
| focused | the focus ring, 1.5 outside, over the row's own fill | `.focused(true)`, set by the app; a selected row keeps its selected fill |
| disabled | the look's disabled paint, dimmed text | no hover, press or click |

Behavior: a click runs `on_click` on release; `on_mouse_down(button, handler)` takes other buttons, such as a right press, and `on_mouse_enter` follows the pointer, as a picker's highlight does. A disabled row runs none of them. The row announces itself as a list item (`.role()` changes that, to a tree item or an option). It has no keys of its own: the list the app lays out moves the selection and focus, marks the focused row `.focused(true)` and runs the picked row.

Content: the label is the thing's name as people know it (a host, a session, a file), on one line. The detail is a short fact in a word or two: a count, a state. Status reads as a word, never as color alone.

Placement: in a sidebar, a pane or a picker, full width, in a column of rows `space-2` apart with the row inset at the sides. One selected row shows which one is open.

HTML: `.penna-list` is the column (rows `space-2` apart). A row is `.penna-list-item[data-penna]` (`menu-item`, `sidebar.row`, `menu-item.tint`) with `role="option"`, `aria-selected="true"` or `data-state="selected"`, `aria-disabled="true"` or `data-state="disabled"`, `data-state="focused"` (or keyboard focus) for the focus ring, and `--penna-list-row` for its height. Inside, `.penna-list-content` holds `i.penna-list-icon[data-icon]`, `.penna-list-label` and `.penna-list-detail`, each text run in `.penna-text[data-penna-text]`. No script.

Status: matches.

### Menu

A menu lists actions that open from a trigger: a button with a caret, a right click or a long press. To pick a value use a select; to switch views, a segmented control or tabs; for one action on its own, a button.

Kinds, by how it opens (`PopupMenu` is the one surface; the select opens it too):

| Kind | Call | Opens | Use |
|---|---|---|---|
| Dropdown | `Button::dropdown_menu(build)` | `space-4` below its button, on a press, or on Enter, Space or Down while the button has focus | a button's set of actions; give the button a caret |
| Context | `.context_menu(build)` on any element | at the pointer, on a right press or a long press; at the element's top-left on Shift+F10 or the Menu key while focus is in it | actions on the thing under the pointer |
| Submenu | `.submenu_with_icon(icon, label, window, cx, build)` | beside its row, when the row is highlighted | a group that would crowd the parent |

Row kinds (`PopupMenuItem`): an action (`new`, with `.icon()` and `.action()`), a checked row (`.checked(true)`), a link (`link(label, href)`), custom content (`element`), a stepper (`stepper`), a submenu row, a section label (`label`) and a separator (`separator`).

Anatomy:

- Surface: the `popover` look (popover fill, `shadow-float`), the theme radius capped at 45% of its height. Its rows sit the row inset (`theme.row_inset`, 6) from every edge, `space-2` apart. At least 128 wide (8 rem), at most 500 (`max_w`); a scrollable menu stops at half the window's height, 450 at most (`max_h`).
- Row: `row` height, `control-md` (28, follows density), 44 under a coarse pointer; inset `space-8` at both sides; text `small`, `body` under a coarse pointer. Corners: concentric with the popover, its corner less the row inset, and never rounder than a 28 control (6 at the default, 12.6 at 100%).
- Icon slot: 12 (14 under a coarse pointer), `space-8` before the label, at 0.8 opacity. Every row keeps the slot when any row has an icon or a check, so labels line up; a checked row draws a check in it.
- Label: `small`, one line.
- Shortcut (optional): `small`, at the end, `space-12` after the label, in the row's ink at 0.8 opacity. It comes from the keymap binding of the row's action, never from the label.
- Mark (optional): a 12 chevron at the end of a submenu row, a 12 external-link mark after a link's label (`space-6`), at 0.8 opacity.
- Section label: `caption`, muted, as tall as its line, with the icon slot before it.
- Separator: a hairline rule, `space-2` above and below and the row inset at the sides. A separator first, doubled or last is not drawn.
- Stepper: the label, then `space-10` later a field-ground group of three small flat ghost buttons (minus, the value at least 48 wide, plus).

Sizes: one. Rows follow density (24, 28, 32); under a coarse pointer they are 44 with `body` text and 14 icons.

States:

| State | Look | Notes |
|---|---|---|
| rest | no fill | |
| highlighted | the `menu-item` selected fill; icon, marks and shortcut go to full opacity | one row at a time, from the pointer or Up and Down; it is the keyboard's place in the menu |
| submenu open | the parent row keeps the hover fill; the submenu's row takes the highlight | |
| checked | a check in the icon slot | |
| disabled | muted text, no highlight | skipped by the keys, ignores clicks |
| focused | the menu holds focus; no ring on rows | the highlight shows where the keyboard is |

Behavior:

- Opening: a dropdown opens on a press of its trigger, or on Enter, Space or Down while the trigger has focus; a context menu opens on a right press or a long press, at the pointer, or on Shift+F10 or the Menu key while focus is in its region, at the region's top-left. `.context_menu` doesn't make the region focusable: give it `.track_focus` for the keys to reach it. Either way the menu opens focused, kept 8 from the window's edges; opened from the keyboard, its first enabled row is highlighted. A submenu opens when its row is highlighted, beside the row with 8 of overlap, on the left when the right has no room, kept 4 from the window's edges.
- Keys while open: Up and Down move the highlight over enabled rows and wrap. Right opens a highlighted submenu and moves into it; Left steps back to the parent. On a stepper Left and Right step and Enter resets. Enter runs the highlighted row and closes the menu. Escape closes it and every parent, and gives focus back to where it was. No type-ahead.
- Pointer: pointing at a row highlights it; leaving clears it unless the last input was a key. A click runs the row on release and closes the whole chain; a press outside closes it. Stepper buttons step without closing.
- A row runs its handler, or else dispatches its action to the menu's action context.
- Motion: it fades in and rises 6 over 160ms, easing out; none under reduced motion.

Content: a row's label is a verb or a short verb phrase in sentence case with no closing period ("Split right", "Copy path"); options and checked rows are nouns ("Even horizontal", "Dark"). End it with an ellipsis when the row asks for more before it happens ("Rename…"). Never type a shortcut into a label: give the row its action and the key shows. Section labels name a group in a word or two. Group related rows and part groups with a separator.

Placement: a dropdown hangs `space-4` below its trigger, aligned to the trigger's start, and its trigger carries a caret. A context menu opens where the pointer is. Keep submenus to one level where you can.

HTML: built at runtime by `menu.js`: `pennaMenu.dropdown(trigger, items)`, `pennaMenu.context(region, items)` or `new pennaMenu.Menu(items)`, where an item is `{ label, icon, key, checked, disabled, link }` or `{ kind: "label" | "separator" | "submenu" | "stepper" | "note", ... }`. The markup: `.penna-popup[data-penna="popover"]` holds `.penna-popup-items[role="menu"]`, whose rows are `.penna-popup-row[data-penna="menu-item"][role="menuitem"]` with `data-kind` (`item`, `label`, `submenu`, `stepper`, `note`), `data-disabled`, and `data-selected` with `data-state="selected"` (`"hover"` on a submenu row) while highlighted. A row holds `.penna-popup-icon`, then `.penna-popup-main` with the `.penna-label` and a `.penna-label.penna-popup-key`; labels and submenu rows wrap theirs in `.penna-popup-line`; marks are `i.penna-popup-detail`, a link's label `.penna-popup-link`, a stepper's group `.penna-popup-stepper`. Separators are `.penna-popup-separator[role="separator"]`; an open submenu is a `.penna-popup.penna-popup-sub` inside its row. `@media (pointer: coarse)` switches rows to 44, `body` and 14 icons. A dropdown also opens on Enter, Space and Down, a context menu on the Menu key or Shift+F10 at the region's top-left, each with its first enabled row highlighted.

Status: matches.

### Panel

A panel is a surface that holds other chrome and stays where the app puts it: a find bar, a palette, a picker, a toast, a raised card inside a view, an opaque cover. The kit paints it; the app sizes, places, opens and closes it. For a panel that opens from a button and closes on its own, use a popover; for a box in the flow of content, a card; for a strip across a pane, a bar; for an area things are arranged on, a well.

Looks (`PanelLook`, `Panel::new(id, look)`):

| Look | Recipe | Paint | Use |
|---|---|---|---|
| Popover | `popover` | glass and `shadow-float`, or the `popover` fill (the window `bg` two steps up) under a hairline in `divider` and the tight `shadow-float` when glass is off; the `radius` knob | menus, dropdowns, the find bar, the palette |
| Inline | `inline-panel` | as Popover | panels raised inside a view: question cards, failure alerts |
| Picker | `picker`, under `dialog` | as Popover | dialogs that search a list and pick from it |
| Toast | `toast` | as Popover; announced as a status | notifications and banners |
| Plain | `panel` | the `panel` fill, opaque; square, no outline, no shadow | an opaque cover, such as a slide-over |

The four floating looks paint the same; each is its own recipe.

Anatomy:

- Container: the look's fill, one step above its ground, with no outline or shadow. Radius the `radius` knob (12 by default, 8 in Yaru); square for Plain.
- Inset: none of its own. The caller pads it: `inset-card` 12 around controls or text, the row inset around rows.
- Corner growth (`.inset(px)`): a rounded corner becomes the look's radius plus that inset, so a child drawn at the theme radius that far in sits concentric. Square and pill corners stay.
- Text: none set; content brings its own style.
- Ground: what sits on it passes `cx.theme().ground_of(look)` to `.on()`, so its paint and muted ink follow the panel's fill.

Behavior: none. A panel doesn't light on hover, take focus, open or close; the app or a popover does that.

Placement: the floating looks sit at the top of the surface stack (window `bg`, then `panel`, then cards, then popover and floating panels). Their fill is two steps above the window whatever they cover, so a panel over a card still reads as above it. Plain sits at window level and hides what is under it. The parent sets the gap around a panel.

HTML: `.penna-panel` with `data-penna` set to `popover`, `inline-panel`, `picker`, `toast` or `panel`. `panel.css` lays it out as a column and fills `panel` with `--penna-bg`; `data-inset="8"` grows the corner (`panel.js` reads a number on a `.penna-panel` only, after the tokens load, so the layout primitive's `data-inset="card"` keeps its own meaning); `data-ground` on the panel makes controls inside paint on its ground, and a toned or prominent control there keeps its tone: the tokens write each tone and the prominent paint under each ground, with only the values that differ from the plain control there. A toast takes `role="status"`, the other looks `role="group"`.

Status: matches.

### Popover

A popover opens a small panel from a button for one short task: rename something, pick a color, set two or three options. It closes on Escape or a press outside. For a list of actions, use a menu; for a hint on hover, a tooltip; for a task that must finish before anything else, a dialog; for a panel the app opens and places itself, a panel.

Anatomy:

- Trigger: any selectable element, usually a button. It shows selected (`data-state="current"`) while the popover is open.
- Panel: the `popover` look (glass, or the `popover` fill under a hairline where glass is off; `shadow-float`; the `radius` knob, 12 by default). Inset `inset-card` 12 all round. Text `body`, set on the panel before the content's own style.
- Offset: `space-4` between the trigger and the panel.
- Content: the caller's. Corners inside it are concentric with the panel's (12 for something at the inset in the default theme).

`.appearance(false)` drops the look, the inset and the text style, for content that paints its own surface.

Placement: by default the panel opens below the trigger with their leading edges aligned (`Anchor::TopLeft`). `.anchor()` picks the trigger's corner it hangs from: `TopCenter` and `TopRight` open below, centered or trailing; the `Bottom` anchors open above. The panel stays `8` inside the window's visible area (a window snap margin, which stays in pixels) and moves up when the area below shrinks, as when the on-screen keyboard rises. It floats at the top of the surface stack.

States: closed, or open. While open the trigger is selected and the panel holds focus. The panel itself has no hover or pressed look.

Behavior:

- A pointer press on the trigger, or Enter, Space or Down while the trigger has focus, opens the panel; the same again closes it. Keys pressed inside the panel stay with the panel.
- Opening moves focus into the panel; closing returns it to what had it, when focus was still inside.
- Escape closes it. A press outside closes it unless `.overlay_closable(false)`.
- `.on_dismiss` runs whenever it closes. There is no call to open it from code.

HTML: `.penna-popover` holds a `.penna-popover-trigger` (a button with `aria-haspopup`) and a `.penna-popover-panel` with `data-penna="popover"`, `tabindex="-1"` and `role="dialog"`. `data-open` marks it open; `data-anchor` takes `bottom` (opens above), `right` (aligns trailing edges) and `center` (centers on the trigger), alone or as `bottom right` and `bottom center`; `data-closable="false"` keeps it open on a press outside. `popover.js` sets `aria-expanded` and `data-state="current"` on the trigger, opens on Enter, Space and Down on the trigger, moves focus, closes on Escape and outside presses, fires `penna-open` and `penna-dismiss`, keeps the panel `8` inside the visual viewport on both axes, as natively, and moves it up when that shrinks, and exposes `pennaPopover.setOpen(popover, open)`. A panel whose trigger is out of view stays where it hangs, so a popover left open further down a page doesn't jump to the top of the screen. `popover.css` sizes the panel to its content, as natively.

Status: matches.

### Row

A row is the fill behind a row the app lays out itself: a menu or picker option, a sidebar entry, a navigation item, the sidebar's cursor. It paints a look in the state the app gives it and lights on hover and press; its height, inset and content are the caller's. For a ready list row use a list item; inside a menu, a menu row.

Looks (`RowLook`):

| Look | Recipe | Use |
|---|---|---|
| Menu | `menu-item` | menu, picker and option rows; the default |
| MenuCard | `menu-item.card` | the pane picker's rows |
| MenuTint | `menu-item.tint` | suggestions under a field: a tint even at rest |
| Sidebar | `sidebar.row` | sidebar rows; selected as glass (Look) |
| Nav | `row.nav` | navigation entries in a sidebar; selected as glass |
| Cursor | `row.cursor` | the sidebar's keyboard cursor: an outline at rest, the accent when selected |

`.tone(Tone::Danger)` paints a row that destroys something in the danger tone.

Anatomy:

- Fill: the look's fill, outline, shadow and highlight. Corners: a control's, `roundness` × the row's height; `.radius(px)` sets a round look's radius on the fill and its press highlight.
- Content, the caller's: the stories use a 32 row, `space-12` inset, `space-8` gaps, a 12 icon, a `small` label and an optional `caption` aside in muted ink.
- Fill row: with `.hover_group(group)` a Row is a fill set inside a taller row (2 from its top and bottom and `space-4` from its sides in the story) that lights when the whole row is pointed at.

Sizes: none of its own. `.row_height(height)` sizes the corners for a row that tall and leaves the height to the caller.

States:

| State | Look | Notes |
|---|---|---|
| rest | the look at rest | |
| hover | the look's hover step | while at rest or focused, and interactive |
| pressed | the look's active step, while held | as hover; under a coarse pointer a fill row draws the press highlight instead |
| selected | the look's selected fill | `PaintState::Selected` |
| current | the same fill as selected | `PaintState::Current`, the open page or section |
| focused | the rest fill and the focus ring, 1.5 outside | the app sets it; `.focused(true)` adds the ring to any state, so a selected row keeps its selected fill |
| disabled | the look's disabled paint, dimmed text | |

`.interactive(false)` paints the state alone, with no hover or press.

Behavior: a Row has no click or key handling of its own; it takes `on_click` and the other element handlers like any element. It announces itself as a list item with its id; a fill row (`hover_group` or `press_highlight(true)`) stays out of the accessibility tree. `.press_highlight(true)` draws the press highlight for a fill whose state the app drives.

Content: one line, the item's name; a short muted aside at the end.

Placement: rows in an inset list sit `space-2` apart with the row inset at the sides, so their highlights never merge; a plain list's rows touch.

HTML: `.penna-row[data-penna]` (the look's recipe), with no size, inset or text style of its own (the page sets them, as the caller does natively), and `data-state` (`hover`, `active`, `selected`, `current`, `focused`, `disabled`), `aria-selected`, `aria-disabled`, `data-tone`, and `data-still` for a row that is not interactive. It holds `<i data-icon>`, `.penna-row-label` and `.penna-row-aside`, each text run in `.penna-text[data-penna-text]`. A fill row is `.penna-row-fill[data-penna]` inside the caller's row, `data-round` for the 24 radius. `data-state="selected focused"` is a selected row with the ring. `.penna-item.penna-list-item` draws a list item with a Row's focus ring. `tab.js` marks keyboard focus and `kit.js` sizes the ring (`--penna-ring-r`).

Status: matches.

### Scrollbar

A scrollbar shows where a scroll area is in its content and lets people drag or jump through it. It overlays the area's end edge and takes no layout space. Every area that scrolls gets one; to hint that more content sits past an edge, add a fade (`top_fade`, `bottom_fade`) as well.

Kinds, by when it shows (`ScrollbarShow`: the theme's `scrollbar_show` unless `.scrollbar_show()` picks one):

| Kind | At rest | Use |
|---|---|---|
| Scrolling | hidden; shows while the area scrolls, then fades | the default, and the theme's pick when the system hides scrollbars |
| Hover | hidden; shows while the pointer is over the bar or the area scrolls | the theme's pick when the system keeps scrollbars |
| Always | the thumb | where the position matters at a glance |

Axes (`ScrollbarAxis`): vertical, horizontal, or both. With both, the horizontal bar stops one track width short of the corner. A bar draws only while its axis overflows.

Anatomy:

- Track: `space-16` (16) wide along the area's end edge (the right for vertical, the bottom for horizontal), no fill. It is the hit area.
- Thumb: `space-6` (6) wide at rest in Scrolling, `space-8` (8) otherwise, set `space-2` (2) in from the outer edge and the track's ends. Its length is the visible share of the track, at least three track widths (48). Fill `fg` at 28% (`fg.wash()`, `--penna-thumb`), 35% (`fg.glow()`) under the pointer or while dragged. Radius: the theme radius capped at half the thumb's width, so round in every shipped theme.

The lengths are `Rems` (`GUTTER_WIDTH`, `THUMB_WIDTH`, `THUMB_INSET`, `MIN_THUMB_SIZE`), turned into pixels with the window's rem when the bar lays out, so paint, hit testing and drag all use the same scaled lengths. A caller that draws its own thumb (zz's terminal scrollback) converts them the same way. `scroll.js` reads `--penna-space-16` and `--penna-space-2` at layout and again when the tokens load.

States:

| State | Look | Notes |
|---|---|---|
| rest | Scrolling and Hover: nothing; Always: the 8 thumb | |
| scrolling | the thumb at its resting width (6 in Scrolling) | |
| hover on the track | the thumb widens to 8 | only while the bar shows, or in Hover |
| hover on the thumb | 8, at the stronger fill | |
| dragging | 8, at the stronger fill | until release |
| fading | 2s after the last scroll or hover the thumb fades for a second (opacity 1 − t¹⁰) and is gone at 3s | with reduced motion it holds full and goes at 3s |

Behavior: dragging the thumb scrolls the area in proportion. A press on the track outside the thumb jumps so the thumb centers on the pointer. A wheel over the bar scrolls the area. The bar keeps presses and drags from reaching the content under it; a hidden Scrolling bar lets the pointer through. Each bar is a scroll bar in the accessibility tree, with its orientation and its position from 0 to 100. Keyboard scrolling belongs to the area, not the bar.

Content: none.

Placement: the last child of the scroll area's frame, absolutely filling it (`.vertical_scrollbar(&handle)`, `.overflow_y_scrollbar()`, or `Scrollbar::vertical(&handle)` in an `absolute().inset_0()` layer), so it overlays the content's end edge. The frame clips; the content keeps its own inset.

HTML: `.penna-scroll[data-axis="vertical|horizontal|both"][data-show="scrolling|hover|always"]` around a `.penna-scroll-viewport`. `scroll.js` adds a `.penna-scrollbar[data-axis]` holding `.penna-scroll-thumb` > `.penna-scroll-fill` per axis (or uses ones in the markup), sets `--penna-thumb-start` and `--penna-thumb-length`, `data-visible` while the bar shows, `data-state="hover"` on the thumb under the pointer and `data-state="active"` on the bar while dragged, and `hidden` on a bar whose axis doesn't overflow. `data-bar` names the bars' `data-id`s (`<id>-vertical`, `<id>-vertical-thumb`), `data-scroll-x` and `data-scroll-y` set a starting offset, and `data-hovered` shows the thumb hovered until the pointer moves. `pennaScroll.mount(element)` wires an area added later. With no `data-show` the bar is Scrolling, the kit's default, since a page can't read the system's choice.

Status: matches.

### Segmented control

A segmented control picks one of two to five short options, all in view, and the pick applies at once: a mode, a view, a density. For more options or longer ones, use a select; for on or off, a switch.

Kinds: one.

Anatomy, all from the control's rung of the control ladder (Size):

- Track: the field's paint (fill, a hairline edge of at least 1px, corners), with its hover and pressed fills while the pointer is on it, height `control_px` (28 at `md`, which follows density), `space-2` inside the edge.
- Pill: behind the picked segment, in the quiet button's selected paint (`data-state="current"`). It is as tall as the segments and its radius is concentric with the track's: the track radius less its edge and `space-2`, and a full pill inside a full track.
- Segments: one per option, side by side with no gap, each as wide as its label plus the rung's field inset and `space-2` at both sides (8, 8, 10, 12, 14, 14 from `xs` to `touch`), as tall as the track less `space-2` and the edge on each side.
- Labels: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`). The picked one takes the selected ink, the others the quiet button's ink.

Sizes: every rung through `.with_size()` (`Sizable`), `md` by default (follows density). `xs` and `sm` in toolbars and dense rows, `touch` under a coarse pointer. Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the track, labels in the quiet ink | |
| hover | the quiet button's hover fill behind the segment, the pill's height and radius, and its label in the hover ink | not on the picked segment |
| pressed | the quiet button's active fill behind the segment, while held | as hover |
| selected | the pill under it, its label in the selected ink | exactly one segment |
| focused | the focus ring, 1.5 outside the track; the track takes the field's focused edge, which only the default theme sets apart | from the keyboard only, never from a click |
| disabled | the track takes the field's disabled paint; the pill keeps its selected paint at half strength (fill and edge, no shadow), and every label takes its ink at half strength | presses, keys and hover are ignored, and it leaves the focus order; the pick still shows |

Behavior: a click on a segment picks it and calls `on_select` with its index; the caller sets `selected`. With `on_select` the control is one tab stop: Left and Up pick the previous segment, Right and Down the next, Home and End the first and last, each through `on_select`, and they stop at the ends. A click picks without focusing. The pill slides to the new segment in 220ms (`turn`) with an ease out, and holds still with reduced motion. Under a coarse pointer each segment's hit area grows to the touch slop.

Content: one or two words per segment, sentence case, no period ("System", "Light", "Dark"). Keep the labels about the same length, since each segment is as wide as its own.

Placement: at the trailing end of a settings row (Appearance's mode and density), or above content to switch its view (the docs' Rust and HTML tabs).

HTML: `.penna-segmented` (`data-penna="field"`, `role="radiogroup"`, `data-value` the picked index, `data-size` (`xsmall` to `touch`), `aria-disabled="true"`) holding `.penna-segments` (`data-penna="button.quiet"`, `data-state="current"`), which holds a `.penna-segment-pill` and a `.penna-segment` (`role="button"`) per option with a `.penna-label` inside. `.penna-segment` takes `:hover` or `data-state="hover"` and `:active` or `data-state="active"`, drawn in the root `--penna-quiet-hover` and `--penna-quiet-pressed` (the quiet button's fills) with the pill's radius (`--penna-pill-r`, which `segmented.js` sets). `segmented.js` makes the track a tab stop, places the pill, sets `aria-selected`, moves the pick on click and on the keys above, fires `change` with `{ index }`, and sizes the ring (`--penna-ring-r`) that `:focus-visible` or `data-state="focused"` draws. With `aria-disabled="true"` it leaves the track out of the tab order, ignores clicks and keys, and marks `.penna-segments` disabled so the tokens give the labels their disabled ink.

Status: matches.

### Select

A select picks one value from a list that opens under it. For two to five short options always in view, use a segmented control; for on or off, a switch; to type a value, an input; to run a command, a button with a caret.

Kinds: one. `Select::new(&state)` draws a `SelectState`, which holds the options (any `SelectDelegate`, such as a `Vec`) and the picked one. Under a coarse pointer the list opens as a bottom sheet instead of a menu.

Anatomy, left to right, all from the select's rung of the control ladder (Size):

- Trigger: a field (the field role's fill, a hairline edge of at least 1px, radius the theme radius capped at 45% of the height), height `control_px`, the field inset at both sides (6, 6, 8, 10, 12, 12 from `xs` to `touch`). It is as wide as its value unless the caller sets a width.
- Value: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`), cut with an ellipsis when it doesn't fit. With nothing picked it shows the placeholder (or "Select") in the field's faint ink.
- Chevron: `chevron-down` at the end, `icon_gap` after the value (4, 4, 6, 6, 8, 8), in the muted ink, a rung smaller than the trigger from `md` up: 12, 12, 12, 14, 16, 16.
- Menu: a popover menu `space-4` under the trigger, its trailing edge on the trigger's, at least as wide as the trigger and scrolling past its height (`.menu_max_h()`). Its rows are menu rows (`small`, 12 icons) with a check before the picked one. An empty list shows a 28 inbox icon in the faint muted ink, `space-24` above and below, or the caller's `.empty()`.
- Sheet (coarse pointer): a bottom sheet titled by `.title()` or the placeholder, with a row per option and a search field once there are more than 12.

Sizes: `md` by default (follows density). Same rungs and uses as a field. Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the field's fill and edge | |
| hover | the fill's hover step | not when disabled |
| focused | the field's focus edge | also while the menu is open |
| open | the focused look, with the menu under it and the picked row highlighted | |
| disabled | the field's disabled paint, faint value | leaves the focus order and never opens |
| error | `.tone(Tone::Danger)` paints the trigger in the danger tone, like a field: its edge only | its Field shows the message with an icon (Field) |

Behavior: a press on the trigger opens the menu, and a second press closes it. While focused, Enter, Space, Up or Down open it. In the menu, Up and Down move, Enter picks, Escape closes it and returns focus to the trigger. Picking a row closes the menu and emits `SelectEvent::Confirm`. Under a coarse pointer the trigger's hit area grows to the touch slop.

Content: options are short nouns in sentence case ("Rust", "Floating"). The placeholder names what is picked ("Language"), with no closing period.

Placement: like a field: in a Field in a form, or at the end of a settings row whose title names it.

HTML: `div.penna-select` with `data-options` (a comma list), `data-value` (the picked index), `data-placeholder`, `data-size` (`xsmall` to `touch`), `data-tone="danger"` for an error, `aria-disabled="true"`, and `data-open` (with `data-pinned` to stay open without focus). `select.js` builds `.penna-select-trigger` (`data-penna="field"`, carrying the select's `data-tone`) holding `.penna-select-title` (`data-placeholder` while nothing is picked) and `<i data-icon="chevron-down">`, and the menu `.penna-menu.penna-select-menu` (`data-penna="popover"`) of `menu-item` rows, each with a `.penna-select-check`. It sets `role="combobox"`, answers the same keys (Tab also closes), closes on a press outside and fires `change` with `{ index, value }`.

Status: matches.

### Separator

A separator is a one pixel line between things that takes no room along its axis. Reach for space first: a `gap-group` between groups usually separates them on its own, and a line is for where space can't, such as between panes or between groups in a menu. For a line on an element's own edge (under a bar, beside a sidebar), use `.rule(look, edge, cx)` on that element.

Looks (`RuleLook`, `.look()`):

| Look | Recipe | Use |
|---|---|---|
| Plain (default) | `rule` | between panes in a split |
| Bar | `rule.bar` | under the title bar and other bars, over footers |
| List | `rule.list` | between menu groups, setting rows and stacked cards |
| Tree | `rule.tree` | the guides that join a tree's children |

All four draw a hairline in the same ink.

Anatomy:

- Line: `RULE_WIDTH` 1, in `divider` (the ink at the theme's edge weight), centered on the separator's box.
- Box: full width and zero height (horizontal), or full height and zero width (vertical), so the gap on either side is the parent's alone. A vertical separator needs a parent with a definite height.
- `.rule(look, edge, cx)`: a 1px border in `divider` on one edge of any element; unlike a separator it takes its pixel of room.

Behavior: none. `.id(id)` makes it an accessibility node (a splitter, with its orientation); it doesn't drag.

Placement: between siblings, with the parent's gap on each side (`gap-related` 8 in the display story). A settings card insets its list rules `inset-card` 12 from its sides, so they line up with the row text.

Content: plain ink. `.tone(tone)` draws the line in a tone's ink (`Primary`, `Danger`, `Warning`, `Success`, `Accent`; `Neutral` is the plain ink), and nothing takes a free color (Rules 8). A status tone on it comes with a word next to it ("Danger zone").

HTML: `.penna-separator-rule` holding a `div` with `data-penna` set to `rule`, `rule.bar`, `rule.list` or `rule.tree`; `data-axis="vertical"` for a vertical one, `data-tone` (`primary`, `danger`, `warning`, `success`, `accent`) for a toned line, `role="separator"` (and `aria-orientation="vertical"`) where native gives it an id. The line is 1px, offset half a pixel to sit centered.

Status: matches.

### Slider

A slider sets a value on a range by dragging or with the arrow keys. For an exact number, use a number input; for a few named choices that read better side by side, a segmented control.

Kinds:

| Kind | Call | Use |
|---|---|---|
| Continuous | `Slider::new(id, label, value, range, on_change)` | a value on a range: tint, hue, zoom. `.step()` snaps it, `.display()` formats it |
| Stepped | `DiscreteSlider::new(id, label, steps, picked, on_change)` | one of a few ordered, named steps: scrollback, opacity |

Anatomy, continuous:

- Box: the rung's `control_px` tall (28 at `md`, which follows density; 44 under a coarse pointer at any size), half the knob inside at each side. The whole box takes the press.
- Rail: a pill, 4 tall at `xs` and `sm` and 6 from `md` up. Up to the value it takes the `accent` fill, past it the neutral soft paint's pressed fill (`--penna-control`); `.track(colors)` paints a gradient instead, as on the hue slider.
- Knob: a circle (Sizes; 28 under a coarse pointer) centered on the value, whichever of `bg` and `fg` is lighter (`--penna-knob`), with `shadow-contact`.
- Under a coarse pointer, unless `.inline()`: a field at the slider's rung showing the value and a 72 wide preview rail (4 tall, 10 knob); a tap opens a bottom sheet titled by the label, with the value in `title` over a full slider.

Anatomy, stepped (all text `small`, `caption` at `xs` and `sm`; the three parts `space-8` apart):

- Label: before the pills, in the muted ink.
- Pills: one per step, `space-4` apart, sharing the width, each at most 24 wide. Each is a bar as thick as the continuous rail (4 at `xs` and `sm`, 6 from `md` up) in a hit box the rung's `control_px` tall. Steps up to the picked one take the `accent` fill, the rest `--penna-control`.
- Value: the picked step's name in the ink, in a 52 wide column after the pills.

Sizes: every rung through `.with_size()` (`Sizable`) on both kinds, `md` by default (follows density):

| Size | Box | Rail | Knob |
|---|---|---|---|
| `xsmall` | 20 | 4 | 12 |
| `small` | 24 | 4 | 14 |
| `medium` (default) | 28 | 6 | 16 |
| `large` | 32 | 6 | 18 |
| `xlarge` | 40 | 6 | 22 |
| `touch` | 44 | 6 | 28 |

Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | as above | |
| hover | a stepped pill's fill takes its hover step | the continuous slider has none |
| pressed | the knob follows the pointer | continuous |
| focused | the focus ring outside the knob or the picked pill's bar, one ring width off when that fill is the ring's color, in the neutral focus ink | from the keyboard only: Tab, or a key that moves it after a press. A press focuses it with no ring |
| disabled | rail, pills and knob take their paint's disabled step (half strength); the knob keeps its shadow, as the switch's thumb does; a stepped slider's label and value in `fg-faint`; under a coarse pointer the field takes its disabled paint | presses, keys and hover are ignored, and it leaves the focus order |

Both kinds are tab stops. Behavior, continuous: a press anywhere on the box moves the knob there and focuses it, and a drag follows. `on_change` fires on every move, or once on release with `.commit_on_release()`. Left and Down step down, Right and Up step up (a hundredth of the range without `.step()`), Shift moves ten steps, Home and End go to the ends.

Behavior, stepped: a click on a pill picks that step and focuses the slider. The arrow keys move one step, Home and End go to the ends. Each pill's tooltip names its step.

Content: the label names the setting in sentence case ("Scrollback", "Opacity"). Values carry their unit ("10k", "70%"); format a continuous value with `.display()`.

Placement: in a settings row, between its title and its value; a stepped slider fills the row it sits in.

HTML: the stepped slider is `.penna-steps` with `data-label`, `data-steps` (a comma list), `data-value` (the picked index), `data-size` (`xsmall` to `touch`), `aria-disabled="true"`, and `data-focused` to draw focus. `slider.js` builds `.penna-steps-label`, `.penna-steps-pills` of `.penna-step` (`role="button"`, `data-filled`, `data-picked`) and `.penna-steps-value`, sets `role="slider"`, answers the same keys and fires `change` with `{ index, value }`. The continuous slider is `.penna-slider` with `data-min`, `data-max` (0 and 100 when left out), `data-step`, `data-value`, `data-size`, `aria-disabled="true"` and `aria-label` for its name, holding a `.penna-rail` with `.penna-fill` and `.penna-knob`; `slider.js` sets `role="slider"` and `--penna-value` (a percentage), moves the knob on a press and a drag, answers the same keys and fires `change` with `{ value }`. Both draw the ring on `:focus-visible` or `data-focused`, which the script sets when a key moves the slider and clears on a press or blur. A disabled one has no `tabindex` and ignores presses and keys. `slider.css` draws both. The gradient rail of `.track(colors)` has no HTML attribute; the Appearance block's hue slider draws its own.

Status: matches.

### Spinner

A spinner says something is under way and how long it takes is unknown: a busy button, a field that is checking, a line saying what is loading. When the wait has known steps, use steps.

Kinds: one, the `loader` icon turning. `.color()` tints it; without one it takes the ink of the text around it. Muted beside muted text; a status color only beside a word in that status (Rules 8).

Anatomy:

- Icon: `loader` at the rung's `icon_px` (12 at `xs` and `sm`, 14 at `md`, 16 from `lg` up); `Size(px)` draws it that many pixels across.
- Turn: one full turn every 800ms, eased in and out, repeating.

Sizes: `md` (14) by default, beside `body` text; `sm` (12) beside `small` and `caption`; `lg` (16) beside `lead` and `heading`. In a control it takes the size of the icon it stands in for: a busy button's spinner is its icon's size, a field's is a rung smaller than the field from `md` up.

States:

| State | Look | Notes |
|---|---|---|
| loading | turning | the only state: a spinner is the loading state of what holds it |
| reduced motion | still | it stops turning when reduced motion is on |

Behavior: none. It takes no pointer or keyboard input. With `.id()` it is announced as a progress indicator named "Loading".

Content: a spinner alone doesn't say what is loading. Outside a control, put a line beside it that does ("Connecting to devbox").

Placement: in a button it takes the icon's place, or sits before the label when there is no icon; in a field, at the end, before the clear button; in a line, before the text, `gap-related` (8) from it.

HTML: `.penna-spinner` holding `<i data-icon="loader">`, with `data-size` (`xsmall` to `touch`), `data-color` (`muted`, `accent`, `success`, `warning`, `danger`), `--penna-icon-size` for a size off the ladder, and `role="progressbar"` with `aria-label="Loading"` and no value when it stands alone, as natively. `spinner.css` stops the turn under `prefers-reduced-motion`.

Status: matches.

### Switch

A switch turns a setting on or off, and the change applies at once. To pick one of a few modes, use a segmented control; to start an action, a button.

Kinds: one.

Anatomy:

- Track: a pill with a transparent `space-2` border between its edge and the thumb. Off, it takes the neutral soft paint's pressed fill (`--penna-control`); on, the `accent` fill.
- Thumb: a circle as tall as the track less 4, with `shadow-contact`. Off, it is whichever of `bg` and `fg` is lighter (`--penna-knob`); on, the accent's mark (`--penna-on-accent`), near-white. It sits at the start when off and at the end when on.
- Name: not drawn. The settings row title beside it names it; `.label()` gives it the same name for accessibility, and `.tooltip()` a tooltip that names it when there is no label.

Sizes (the switch's own geometry; it does not follow density):

| Size | Track | Thumb |
|---|---|---|
| `xsmall` | 24 × 14 | 10 |
| `small` | 28 × 16 | 12 |
| `medium` (default) | 36 × 20 | 16 |
| `large` | 40 × 22 | 18 |
| `xlarge`, `touch` | 44 × 26 | 22 |

Under a coarse pointer a switch from `md` up draws at 44 × 26 and takes a 44 square target.

States:

| State | Look | Notes |
|---|---|---|
| off | neutral track, thumb at the start | |
| on | `accent` track, thumb at the end | |
| hover | the track one fill step deeper (on: the accent's hover fill); the thumb doesn't change | while interactive |
| pressed | the track one more step (on: the accent's active fill), while held | as hover |

Off, a fill step is the neutral soft paint's own step (its pressed fill less its hover fill), so hover and pressed sit one and two steps past `--penna-control`.
| focused | the focus ring, 1.5 outside the track, touching it (the ring is the ink, the track the accent) | from the keyboard only, never from a click |
| disabled | track and thumb at half strength | presses are ignored, and it leaves the focus order |

Behavior: it flips on release of a click or tap inside it, and on Space or Enter while focused, and calls `on_click` with the new value; the caller sets `checked`. It is a tab stop when it has `on_click`. The thumb slides in 150ms and holds still with motion off.

Content: the row title names the setting in sentence case ("Share clipboard"), never its state ("On", "Enabled"). A description under the title says what on does.

Placement: at the trailing end of a settings row, one switch per row.

HTML: `button.penna-toggle` with `aria-checked` (`true`, `false`), `data-size` (`xsmall` to `touch`), `disabled` and `aria-label`. `switch.js` sets `role="switch"`, adds `.penna-toggle-track` holding `.penna-toggle-thumb`, flips `aria-checked` on click and fires `change` with `{ checked }`. `:hover` or `data-state="hover"` and `:active` or `data-state="active"` draw hover and pressed (off: `--penna-control` at 4/3 and 5/3 of its alpha; on: `--penna-primary-hover` and `--penna-primary-pressed`); `:focus-visible` or `data-state="focused"` draws the ring.

Status: matches.

### Tabs

A tab switches between open things in one place: windows in the title bar, pages in a browser, tasks in the tray over a composer, a pane's title. To pick one of a few views of the same thing use a segmented control; to go somewhere, a link or a sidebar row.

Looks (`TabLook`):

| Look | Recipe | Use |
|---|---|---|
| Window | `tab.window` | window pills in the title bar; selected as glass (Look) |
| Page | `tab.page` | a browser's page tabs; selected as glass |
| Tray | `tab.tray` | chips in the tray over a composer |
| Title | `button.title` | a pane's title in its header, quiet, with a muted path |

Anatomy:

- Container: the rung's height, `control-lg` 32 by default; the look's fill, outline and corners (its radius, capped at 45% of the height).
- Inset, gap, icon and label are the caller's. The story and `tab.css` use `space-10` at the sides and `space-6` gaps, a 12 icon, a `small` label and an optional `caption` aside in muted ink; a title uses `space-8` and a `small` muted path.
- Focus ring: 1.5px, outside the container, with its own radius.

Sizes: `.with_size()` (or `.xsmall()` to `.touch()`) moves only the height along the control ladder: 20, 24, 28 (follows density), 32 (the default), 40, 44. Text, inset and icons stay the caller's. Every tab in a bar has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the look at rest | |
| hover | the look's hover step | while at rest or focused, and interactive |
| pressed | the look's active step, while held | as hover |
| selected | the look's selected fill | the open tab; `aria-selected` |
| current | the same fill as selected | |
| focused | the rest fill and the focus ring | the app sets it; `.focused(true)` adds the ring to any state, so the open tab keeps its selected fill |
| disabled | the look's disabled paint, dimmed text | |

`.interactive(false)` paints the state alone, with no hover or press.

Behavior: a Tab paints the state the app gives it and announces itself as a tab with its id. The app switches on click (`on_click`, like any element) and marks the open tab selected. Natively a Tab takes no focus or keys of its own: the bar the app lays out owns focus and the arrow keys and marks the focused tab. In the HTML twin, `tab.js` moves focus inside a `[role="tablist"]`: Left and Right go to the previous or next tab with a `tabindex` and wrap, Home and End to the first and last; keyboard focus sets `data-state="focused"`, and `kit.js` sizes the ring.

Content: the label names what the tab shows, as people name it: a session, a page title, a task. Keep it short; a count or the program goes in the muted aside. No closing period.

Placement: tabs sit side by side in a bar, each as wide as its content; an app that wants them equal gives each the same width. One tab per bar is selected. A title sits in its pane's header.

HTML: `.penna-tab[role="tab"][data-penna]` (`tab.window`, `tab.page`, `tab.tray`, `button.title`) inside a `[role="tablist"]`, with `data-size` (`xsmall` to `touch`; large when absent), `data-state`, `aria-selected`, `aria-disabled`, `data-still` and `tabindex`. It holds `<i data-icon>`, `.penna-tab-label`, `.penna-tab-aside` or `.penna-tab-path`, each text run in `.penna-text[data-penna-text]`. A title is `.penna-tab.penna-title-tab` in a `.penna-pane-header[data-penna="bar.pane"]`.

Status: matches.

### Text

`TextView` renders markdown as reading text: documents, help pages, release notes, chat and agent transcripts. Interface text people scan (labels, rows, descriptions) takes a text style on its own element instead, and code people edit goes in the code editor.

Kinds:

| Kind | Call | Use |
|---|---|---|
| Fixed | `TextView::markdown(id, text)` | text set once |
| Stateful | `TextView::new(&state)` with a `TextViewState` | text that changes; `.streaming(true)` while it arrives through `push_str` |
| Inherited | `.style(TextViewStyle::default().text(None))` | reading text inside a container that sets its own text style, such as zz's transcript; no measure |

Anatomy, top to bottom:

- Container: `read-body` (14 on 22, 400) set on itself (`TextViewStyle::text`), at most 640 wide (`READING_MEASURE`, about 75 characters). No inset of its own; the page or pane gives one (`inset-page` 32 for a document page).
- Headings: the reading ladder, all 600 and wrapping: h1 `read-h1` 26 on 32, h2 `read-h2` 20 on 28, h3 `read-h3` 16 on 24, h4 `read-h4` 14 on 22, h5 and h6 `read-h5` 13 on 20.
- Paragraphs: `read-body`. Bold is 600, italic slants, a strikethrough is a 1px line.
- Links: the accent with a 1px underline; a link's title shows in a tooltip.
- Inline code: the code face at the size of the text around it, on the `card.code` fill, corners at most 6.
- Lists: items touch, with no gap; markers `•`, `◦`, `▪` by depth, or `1.`; a nested list indents `space-16` (16). A task item's checkbox is as tall as the font, centered on the capitals in a box one line tall, `space-6` (6) before its text.
- Quotes: a `space-4` (4) bar in the divider color at the start, `space-16` (16) inside it, text in `fg-muted`.
- Code blocks: a `card.code` card, `inset-card` (12) all round, the code face at the reading size with the theme's syntax colors. With `code_block_actions` the top inset grows to 2 × `space-8` + `control-md` (44), and the actions sit `space-8` (8) from the top and sides, at the end.
- Tables: hairline rules between rows and between columns, cells `space-4` (4) by `space-8` (8), columns sized to their content from 48 to 480 (Size, Width). By default the columns share the container's width in that proportion, none narrower than 64, and their text wraps. With `overflow_x` scroll on `TextViewStyle::table` (`data-scroll`) they keep their widths, a narrower table grows in proportion to fill its container, and a wider one scrolls sideways in its own area. Text `read-body` (`read-small` is planned).
- Rule: a hairline in the divider color.

Rhythm: blocks sit `space-12` (12) apart (`TextViewStyle::paragraph_gap`). A heading has `space-24` (24) above it at h1 and h2, `space-16` (16) at h3 to h6, and `space-8` (8) below. The first block has nothing above it and the last nothing below.

States:

| State | Look | Notes |
|---|---|---|
| rest | the text | |
| selected | an `fg` wash (28%) behind the selected runs, across blocks | `.selectable(true)` |
| streaming | new text fades in under a veil over 120 to 220ms (appear to turn), following the pace it arrives at | none with reduced motion |

Behavior: with `.selectable(true)` a drag selects across blocks, a double click a word and a triple click a paragraph; while it has focus, ⌘C or Ctrl+C copies and ⌘A or Ctrl+A selects all. A drag past the top or bottom scrolls. `.scrollable(true)` fills its container and scrolls with a vertical scrollbar. Links open with the system unless `.links(..)` handles the click; a `TextLinks` can also restyle a link or give it a tooltip. `markdown_extensions` adds block kinds and `code_block_body` replaces a code block's body.

Content: markdown. Headings give structure, never emphasis; bold carries emphasis inside a line. Keep the measure; don't widen it.

Placement: inside a pane or page that gives the inset, `inset-page` (32) around a document page.

HTML: `.penna-md` (`data-selectable` to make it focusable and select all with ⌘A; `data-selected`; `data-streaming` with a `template[data-tail]`, whose blocks `text.js` appends under the same veil, one 220 fade on the native curve as for a first chunk; `data-text="inherit"` for the inherited kind, which takes its container's text style and drops the measure) holding plain `p`, `h1` to `h6`, `strong`, `em`, `s`, `a` and `code[data-penna="card.code"]`; quotes as `.md-quote > blockquote`; lists as `ul` or `ol` > `li` > `.md-item` > `.md-prefix` + `.md-body`, a task's box as `.md-box > .md-check[role="checkbox"][aria-checked]`; `hr`; code as `.md-code > pre[data-penna="card.code"] > code` with `.tk-*` runs, and `data-actions` and `.md-actions` for actions; tables as `.md-table` (`data-scroll`, with a `.md-scroll`) > `.md-figure[data-penna="card.figure"]` > `.md-row` > `.md-cell[data-align]`. `text.js` sizes table columns, rounds inline code and check corners, and places baselines as ala does.

Status: matches.

### Tile

A tile is a thumbnail you pick or drag, drawn to scale: a screen on a well, a computer waiting in a dock, a pane inside a window's tile. Its drop and veil variants mark where a dragged thing will land. For a box of content, use a card; for a row in a list, a row; for one of a few views, a segmented control.

Variants (`TileVariant`, `.variant()`):

| Variant | Recipe | Paint | Use |
|---|---|---|---|
| Card (default) | `tile.card` | solid, one step above its ground, no outline, no shadow | a card for something you can place, in a dock under a well |
| Screen | `tile.screen` | as Card | a screen drawn to scale on a well |
| Drop | `tile.drop` | a floating tint, no outline, no shadow; deeper on hover and selected | where a dragged thing will land |
| Veil | `tile.drop` at hover, no edge, on a glass ground | the hover tint | laid over panes while something is dragged |
| Mini | `tile.mini` | no fill of its own (the caller's `.bg` and `.rounded` show), no glass | a pane inside a window's tile in the phone overview |

Anatomy:

- Container: the variant's paint, the `radius` knob (12 by default, 8 in Yaru). Drop and Mini take the caller's corners. No outline.
- Size and inset: the caller's. A screen's size follows the real screen's shape; the canvas story draws them 150 by 90 with `space-6` inside, and dock cards 132 by 64 with `space-8` by `space-10`.
- Label: the caller's: a `caption` name on a screen, a strong `body` name over a `caption` detail on a card.
- Tone (`.tone()`): `Primary` or `Danger` tints the label ink, for a screen the pointer is on or one it can't move to.
- Ring: a 2px ring outside the edge when selected.

States:

| State | Look | Notes |
|---|---|---|
| rest | the variant's paint | |
| hover | Card, Screen and Mini lift: one step of fill above rest and `shadow-lifted` under them (Depth), so the lift reads in dark too, where a shadow barely shows; Drop one tint step deeper | the app sets it while the tile is dragged; the step is one wash of the ground's pole (`washed(1)`, the hover step of a solid surface) over the fill, a Mini's caller fill included |
| selected | a 2px ring in the highlight color (Card, Screen, Mini); Drop's selected tint | the one picked |
| focused | the variant's rest paint and the focus ring, 1.5 outside | from the keyboard on a tile that takes focus, or set by the app |
| disabled | fill at half strength, faint label, no shadow | a thing that is away or can't be placed |

Behavior: the native tile paints a state; the app picks, drags and drops. A tile with a click handler is a tab stop announced as a button: Space and Enter run it, and it shows the focus ring from the keyboard only, since a click never focuses it. `Tile::on(ground)` paints it on a ground, `Well::ground(cx)` on a well.

Placement: on a well or in a dock, `gap-group` 16 apart in a row, or where the app arranges them. Drop and veil sit absolutely over the panes they cover. A tile rises one step of fill from its ground, and casts a shadow only while dragged.

HTML: `.penna-tile` with `data-penna` (`tile.card`, `tile.screen`, `tile.drop`, `tile.mini`) and `data-variant` (`card`, `screen`, `drop`, `veil`, `mini`); `data-state` (`hover`, `selected`, `focused`, `disabled`), `data-tone` (`primary`, `danger`), `data-edge` on a drop zone, and `role="button"` with `tabindex="0"` on a tile that takes focus. `tile.css` draws the variants and states, the lift as `--penna-shadow-lifted` under an inset `--penna-wash` (one wash step, laid over whatever fill the tile or its well draws), the selected ring in `::before` and the focus ring in `::after`; size, inset, placement and a mini's fill and corners are the page's, as natively. `tile.js` runs a focusable tile on Space and Enter and keeps a click from focusing it. Picking and dragging are the page's, as natively the app's.

Status: matches.

### Tooltip

A tooltip names the control under the pointer: an icon-only button's action, a stepped slider's step, a link's title, and the key bound to it. It holds one short line. For more than a phrase, or for anything people click, use a popover; for status, a notice or a toast.

Kinds:

| Kind | Call | Use |
|---|---|---|
| Label | `Tooltip::new(text)`; `.tooltip(text)` on a button, switch or slider | names the control |
| Label and key | `Tooltip::new(text).action(&action, context)` | names it and shows the key bound to that action in that key context |

Any element takes one with ala's `.tooltip(|window, cx| Tooltip::new(..).build(window, cx))`; that one shows on hover only. A button's `.tooltip(text)` also shows on keyboard focus.

Anatomy, left to right:

- Box: the `tooltip` surface role, a floating surface with the popover fill, a hairline edge and `shadow-contact`; radius the theme radius capped at 45% of its height. Inset `space-2` (2) above and below and `space-8` (8) at the sides, so it is a `small` line (16) plus 4, and the edge.
- Label: `small` (12 on 16, 400) in the surface's ink, on one line. It never wraps.
- Key (optional): `small` in `fg-muted`, `space-12` (12) after the label, the binding written as text (⌘D on macOS, Ctrl+D elsewhere).

States:

| State | Look | Notes |
|---|---|---|
| hidden | nothing | |
| shown | the box, at once | no fade in or out |

Behavior: it shows 500ms after the pointer comes to rest on its target. Its frame starts 1px right of and below the pointer and the box sits `space-12` inside that frame, so it clears the cursor by 13. Where it would cross the window's right or bottom edge it flips to the other side of the pointer. It hides when the pointer leaves the target or presses. When the keyboard focuses its target it shows at once, placed as if the pointer rested on the middle of the target's bottom edge, and hides when focus leaves, on Escape (which still reaches the rest of the window) or when the pointer moves; it comes back only on the next keyboard focus. Natively, a long press on touch shows it until the finger lifts. One tooltip shows at a time, and a disabled button's tooltip still shows.

Content: a noun or a short verb phrase in sentence case that names the action or the thing ("Split right", "Notifications", "Copy path"), with no closing period. Never an instruction ("Click to split") or a sentence. On an icon-only button it is required and is the button's accessible name too. Don't repeat a label that is already visible; on a labeled control a tooltip adds the key or nothing. Give the key with `.action()` (`data-tooltip-key`), never typed into the text.

Placement: it follows the pointer, not the target, so there is nothing to place. Give one to every icon-only button (Button) and to controls whose key is worth learning.

HTML: any element with `data-tooltip="Label"` gets one from `tooltip.js`; `data-tooltip-key="cmd-d"` adds the key, written by `kbd.js` as a keycap's text. The script builds `.penna-tooltip[data-penna="tooltip"][role="tooltip"][data-floating]` with a `span[data-penna-text]` per run and `.penna-tooltip-key` for the key, sets `data-open` while shown, and places it with the same delay, offset and edge flip: on the window with `data-fixed`, or inside the nearest `[data-tooltip-host]` within a story section. A `[data-tooltip]` element that takes focus matching `:focus-visible` shows its tooltip at once from the middle of its bottom edge, until `focusout`, Escape or a pointer move. A touch press held 500ms on a `[data-tooltip]` element shows its tooltip from the press point until the finger lifts, as natively: moving more than 8 before then cancels it, and once shown the press neither scrolls nor clicks. `data-tooltip-pinned` keeps it open. A static box is the same markup without `data-floating`; the key run takes `data-kbd-text`.

Status: matches.

### Tree

A tree shows things nested in each other, such as a host, its sessions and their windows, as indented rows joined by guide lines. For a flat set use a list; for actions, a menu.

What penna gives: `IndentGuides`, a decoration for a `uniform_list` of rows that draws the guides in the `rule.tree` look, and `IndentGuideColors::tree(cx)` for their inks. The rows are list items (`ListItem`) announced as tree items in the sidebar look.

Anatomy:

- Row: a `ListItem` with `.role(Role::TreeItem)` and `.look(RowLook::Sidebar)`, full width, `control-md` tall (28, follows density), text `lead`. Inset `space-12` at the end and `space-12` plus one indent per level at the start.
- Icon: 14, muted, `space-8` before the label; it tells the kind of thing.
- Label: `lead`, one line.
- Guides: one vertical hairline (one device pixel) per open level, in divider ink. A guide runs down the rows of its level and stops short of its first and last rows, except where it runs past the viewport. The guide of the selected row's nearest parent lights in muted ink; the others stay plain.

The indent and the guide's place are the caller's (`IndentGuides::new(depths, active_row, indent, left_offset, end_padding, colors)`): the story indents 16 per level, puts each guide 20 in (the inset plus half the 16 icon slot, under the parent's icon) and stops it 4 short.

Sizes: one. Rows follow density; text stays `lead` and icons 14.

States: a row's states are a list item's: rest, hover, pressed, selected, focused and disabled. A selected row also lights its parent's guide.

Behavior: the app owns the rows, the selection (`active_row`), focus and what a click does; the guides follow the depths and the active row of the rows in view. Natively a tree takes no keys of its own: the app moves the selection and marks the row the keyboard is on `.focused(true)`, which draws the focus ring. In the HTML twin, `tree.js` selects and focuses a row on click, Up and Down move the selection and focus (stopping at the ends), only the selected row is in the tab order, and the guides redraw on every change. Neither folds a level open or closed.

Content: a row's label is the thing's own name ("lib.rs"), never its path: the depth already says where it is. One icon per kind of thing.

Placement: in a sidebar or a side panel, full width, rows touching so the guides run unbroken. The list keeps `space-2` free on every side, so a focused row's ring shows whole, natively and in HTML: natively the app pads its `uniform_list` with `.p(Space::S2)`, inside the list's clip, and the guides move with the rows.

HTML: `.penna-tree[role="tree"]` with `--penna-tree-row` (`control-md`), `--penna-tree-indent` (16), `--penna-tree-inset` (`space-12`), `--penna-tree-offset` (20) and `--penna-tree-end` (4). Rows are `.penna-tree-row[data-penna="sidebar.row"][role="treeitem"]` with `data-depth` and `style="--depth: n"`, `aria-selected`, `tabindex` and `data-state="focused"` (keyboard focus draws the ring too), holding `.penna-tree-content` with `i.penna-tree-icon[data-icon]` and a `.penna-label[data-penna-text]`. `tree.css` pads the tree `space-2`, and `tree.js` adds a `.penna-tree-guide` per guide inside that padding, `data-active` on the lit one.

Status: matches.

### Well

A well is the recessed area things are arranged on: screens on a desk, panes in a layout preview. Tiles sit on it. For a box of content, use a card; for a raised panel, a panel; for a preview of a setting, the settings preview.

Look: one recipe, `well` (a card kind): a tint of the ink over its ground (4%), no outline, no shadow, the `radius` knob (12 by default, 8 in Yaru). The tint, not an edge, sets it apart.

Anatomy:

- Container: the tint and the theme radius.
- Inset: the caller's. The canvas story pads it `space-24`.
- Contents: tiles in a wrapping row, `gap-group` 16 apart, or placed by the app where it arranges them (layout geometry, not tokens).
- Ground: `Well::ground(cx)` is the ground on the well; tiles and labels on it pass it to `.on()`, so a tile stands a step above the tinted recess rather than the window.

Behavior: none. It takes no id, no focus and no pointer state; what sits on it does.

Placement: on the window or a pane, at window level. It sinks into its ground rather than rising above it, and the tiles on it rise one step from it. The parent sets the gap around it.

HTML: `.penna-well` with `data-penna="well"`; `.penna-well-row` holds tiles in a wrapping row `gap-group` 16 apart. Tiles inside a well paint the well's tint over their fill, so they sit on its ground without a ground of their own; in light mode `well.js` sets `data-bright` when the tiles are brighter than the window, and they lift instead. `data-ground` on the well makes controls inside paint on its ground.

Status: matches.

## Blocks

A block is a piece of UI that owns the layout and state of several components. It takes plain data and callbacks; the app keeps whatever turns its own model into that data.

### Palette

A palette finds and runs things from one field: places, commands and values, matched as you type. For a short list that never changes, use a menu; to pick a file, the picker.

Parts (`penna::blocks::palette`):

| Part | Call | Use |
|---|---|---|
| Surface | `PaletteSurface::new(input, revision)` with `.usage`, `.reserve_usage`, `.rows`, `.hints`, `.input_last`, `.over_input`, `.footer_leading`, `.max_width` | the whole palette |
| Pill input | `palette_input(&state, pills, cx)` | a field led by `PalettePill`s: `Status` (a dot and a label), `Mode` (a mono prefix and a label), `Code` (mono), `Crumb` (shrinks first) and `Action` (accent ink) |
| Prompt input | `palette_prompt_input(&state, prompt, font, cx)` | a field led by a prompt, such as `:` |
| Row | `palette_row`, `palette_tree_row` (with a disclosure), `palette_badged_row` | one result from a `PaletteRow` |
| Section | `palette_section(label, hint, cx)` | names a group of rows, with its sigil at the end |
| Empty | `palette_empty(label, cx)` | nothing matched |
| Hints | `PaletteHint` in the footer, `palette_key_hint(keys, label)` elsewhere | the keys that work now |
| Vim | `PaletteVim`, `palette_vim_badge` | normal and insert modes over the list |

Anatomy:

- Surface: the `popover` look, at most 560 wide (`PALETTE_MAX_WIDTH`), its radius the popover's plus the 8 inset so rows nest inside it. Input, then the usage line, then rows, then the footer.
- Input: 40 tall, `small`, inset 14 at the sides and 8 above and below; pills `space-6` apart, each a badge with `space-6` inside and 8 at the sides.
- Usage line: 24 tall, `micro` mono, muted, inset 18, one line ending in an ellipsis. `.reserve_usage()` keeps its room when the highlighted row runs nothing, so the palette doesn't change height as the highlight moves.
- Rows: the row inset at the sides, 2 above and 8 below. A row is 28 tall (26 when rows are inset), inset 12 at the sides and 12 more per indent step; `space-8` between its parts.
- Row parts, in order: a 12 disclosure (tree rows), a 12 icon, a 6 status dot, the label (`small` medium; matched letters semibold in the row's ink; a muted path prefix), the detail (`micro`, muted, takes the room left), the right text (`micro`, muted), the shortcut (small keycaps, lowercase, or a mono keycap), a kind badge, and a muted dot when the row is running.
- Section: 28 tall, `caption`, muted, inset 10, with a `micro` mono hint at the end.
- Footer: at least 32 tall, a hairline rule above, `micro` muted, inset 18; hints `space-14` apart at the end, each its keys as glyphs in the foreground and a label.

States:

| State | Look | Notes |
|---|---|---|
| rest | no fill | |
| hovered | the `menu-item` hover fill | |
| selected | the `menu-item` selected fill; on a solid fill the row hands it to its parts as their ground, so the label, detail, keys and kind badge take the selected ink and a status dot turns to the muted ink (`Look`) | one row; Up and Down move it |
| matched | matched letters semibold | the app passes the ranges |
| running | a muted dot at the end | |
| expanded, collapsed | a down or a right chevron | the disclosure toggles without running the row or taking focus from the field |
| empty | `palette_empty` in place of the rows | |

Behavior:

- The field keeps focus while the palette is open. Up and Down move the selection; Enter runs it; Escape closes the palette.
- With vim on, the palette opens in insert mode. Escape goes to normal mode, where `j` and `k` move, `l` and `h` go in and out, `gg` and `G` jump to the ends, `yy` copies, `x` drops, `i`, `a` and `/` go back to insert, and `q` or a second Escape closes. A sigil typed in normal mode starts insert with it.
- Pressing a disclosure toggles its row and nothing else.
- Opening fades it in; none under reduced motion.

Content: rows name the place or the command as the app knows it; the detail says what it is or does in a few words. Hints name what the key does now ("run", "back"), in lowercase.

Placement: centered near the top of the window, above everything else. `.input_last()` puts the field at the bottom, for a palette that opens from below.

HTML: `.penna-palette-surface` with `data-penna="popover"` and `data-inset="8"` holds a `.penna-palette-input` (a `label` with `.penna-palette-pills` and an `input`; `data-prompt` and `data-mono` for a prompt), a `.penna-palette-usage`, a `.penna-palette-rows` and a `.penna-palette-footer` with a `.penna-palette-rule` (`data-penna="rule.bar"`) and `.penna-palette-hint`s. A row is `.penna-list-item.penna-palette-row` with `data-penna="menu-item"`, its parts in `.penna-palette-content`: `.penna-palette-disclosure` (`data-toggle` to make it toggle), `.penna-palette-icon`, `.penna-palette-dot` (`data-status`: `online`, `running`, `waiting`, `offline`, `busy`; it reads `--penna-dot` first, so a solid scope recolors it), `.penna-palette-label` (with `.penna-palette-match` and `.penna-palette-muted` runs), `.penna-palette-detail`, `.penna-palette-right` and `.penna-palette-shortcut`; `data-detail` when it has a detail, `--indent` for its indent. `palette.js` gives the surface its inset radius, moves the selection on Up and Down inside a surface, and toggles disclosures (firing `penna-toggle`).

Status: matches.

## Work order

| Step | What | Visible change | Status |
|---|---|---|---|
| 1 | Token names in Rust and CSS: `Space` named by pixels with gap and inset constants; space, gap, inset and text style CSS variables | none | built |
| 2 | Reading text: `TextView` and `text.css` on the reading ladder and rhythm | documents and chat get smaller, tighter text | built |
| 3 | Text styles: every component sets a full text style on its text, native and HTML, one family at a time: rows, lists and menus first, then controls, surfaces, chrome and the editor blocks, with the touch ladder | per family: rows that grew from a 1.618 line return to their row height | built |
| 4 | Root text style: `Root` and `penna.css` set `body` | app text with no style goes from 16 on 26 to 13 on 18; penna itself doesn't move | built |
| 5 | Layout primitives, native and DOM, with stories | none by themselves | built |
| 6 | Components onto the space tokens, one family at a time, starting with Appearance (its 11 and 14 insets); the primitives stay for app layouts and new code | small spacing moves, reviewed per family | built |
| 7 | Control ladder: `control-xs`, `-lg`, `-xl`, `-touch`, rows on `control-md`, tab heights on the ladder | menu rows grow 2px, tabs move | built |
| 8 | Color: contrast fixes across the six theme variants | flagged pairs darken or lighten | built |
| 9 | Density setting | new option | built |
| 10 | One look: the default theme becomes the look above, theme files shrink to knobs, `macos` and `yaru` become presets, role overrides go, and the kit draws variants from their kind | every screen: fewer edges and shadows, a warm ground, 12 corners | built |
| 11 | Every component matches its section: bars and cards set their own size and inset, the HTML twins take their roles, hover and pressed on switches and segmented controls, a lifted tile while dragged, no free separator color, 14 icons when no size is given | bars move to the pane header height, unsized icons grow 1px beside `body` | built |
| 12 | The Look's last native and HTML differences: toned floating chips opaque, presses replacing hovers, glass over stacked dialogs, label inks per ground in HTML, selected-row content in the selected ink, a dragged tile one fill step up | toned pane chips turn opaque, dragged tiles read in dark, chips on picked rows turn to the row's ink | built |
| 13 | Glass past floating surfaces: chrome items select as glass, panes with gaps take the material's rim and light, and buttons over the content are glass; where glass is off each keeps the look it had | selected sidebar rows, sidebar entries and window and page tabs turn to glass pills; gapped panes gain a lit rim; jump to latest turns to glass | built |
| 14 | Knobs, no presets: Appearance drops the theme picker and saved themes, gains Controls, Glass and a theme file menu, and the look starts from the platform's knobs | the Theme row goes; Look holds every knob; a pasted theme file sets them | built |
| 15 | Glass that reads on a flat ground: a chrome selection's tint matches the highlight it replaces, a material with no bezel draws no glint natively, and a gapped pane with no rim takes the `divider` hairline | `frosted` selections read like the highlight with no stray rim; gapped panes keep their edge with glass `none` or `frosted` | built |
| 16 | One glass: `frosted` is gone and Glass is a switch; on, it draws `liquid` on every platform, and a theme file's own material fills in over `liquid` | the Glass row turns to a switch; Linux, Windows and the web draw `liquid` where they drew `frosted` | built |
| 17 | Plain knobs: Corner radius in percent on a square curve with no pill at its top, Increase contrast a slider that adds no edges, Motion in three steps, no Theme file row, and the Advanced fold opened into Size and Accessibility | the radius slider reads 69% by default; controls lose their hairline under contrast; Reduce motion and Animation speed become one Motion row | built |
| 18 | A see-through window reads like an opaque one: the theme carries the window's opacity, so cards and bars on it take their step as a wash and stay a step above it, and `divider` drops to 8% | settings cards under window blur read a step above the page instead of darker than it; every rule is lighter | built |
| 19 | PiPs follow Glass: a PiP is glass when glass is on and its pane's ground, opaque, under the `divider` hairline when it is off | PiPs lose their frosted blur with glass off and gain the pane's hairline | built |
| 20 | One corner rule: controls round to a share of their height, containers keep `radius`, nested corners are concentric on drawn corners, and smoothing no longer changes how round a corner looks | controls on macOS (`smoothing` 4) read as round as their radius says instead of about half; rows, sidebar entries, tabs and the segmented pill match at every Corner radius; zz's flat sidebar entries follow the knob instead of a fixed 24 | built |

Each step lands in penna with its stories, scores parity, rebaselines only what the step meant to move, and is pulled into zz and nidus.
