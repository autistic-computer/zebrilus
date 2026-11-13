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
9. A phone screen is built from the phone components and keeps the phone rules (Phone). Desktop components under a coarse pointer are a different case: they keep their layout and grow their targets (Size).

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

Rows in a plain list (row inset 0) touch: their height keeps the rhythm. In an inset list (`theme.inset_rows()`, a row inset above 0, as in the look) rows sit `space-2` apart, so their rounded highlights never merge: menus, select lists, settings navigation and the touch which-key list. `list_inset` sets both the side inset and that gap; `theme.row_gap()` is the gap alone, for a list whose sides are padded some other way. The docs navigation keeps its rows `space-2` apart in every theme.

Components take every padding, gap and margin from `Space` (built in step 6), so spacing scales with the rem like type does. Settings pages are a pane (`inset-pane` 16 around the page, `gap-group` 16 between groups); a settings row is inside a card (`inset-card` 12 all round, title and description `gap-tight` 4 apart); under a coarse pointer its title is never cut or covered and nothing leaves the card: the title stays on one line while it and its control fit, a control that can shrink gives way first (a select, 200 wide, down to 120, which still shows about ten characters of its value, `settings_select_box`), then the title wraps onto more lines, never inside a word, beside a control of fixed size (a switch, a segmented control), which never shrinks; when the title's widest word and the control at its smallest do not fit side by side, the control goes on its own line under the title, at the row's end, `space-8` below it. The control stays centered on the title's lines and the description sits under them. Dialogs are panes too (`inset-pane` 16). Sidebar sections are `gap-group` 16 apart. Which-key's columns and lines are `gap-group` 16 apart inside an `inset-card` sheet. What stays in pixels: drawing geometry inside a fixed-size picture (Appearance's theme miniatures), focus halos and swatch rings that feed radius math, window snap margins, and the sidebar title bar's insets, alignment geometry computed from the rows' action column (and the traffic lights on macOS) so the window's buttons line up over the rows.

An inset reaches the text's line box, not its letters. In `body` (13 on 18) capitals start 4.27px inside the box and the baseline sits 4.27px above its bottom, so an `inset-card` reads as about 16px to the capitals. Keep the token; never nudge text natively. An HTML twin may move a text run by a pixel at most (`position: relative; top`, usually ±0.5px) where ala rounds the baseline differently from the browser, so parity holds; that is the only nudge, and it lives in the twin's CSS.

## Type

Faces: the platform system face natively (`.SystemUIFont`), Inter Variable in the storybook and on the web. The code face is the platform's natively (`Theme::mono_font_family`: Menlo on macOS and iOS, Consolas on Windows, DejaVu Sans Mono elsewhere) and Lilex in the storybook and on the web, until Appearance's Mono font picks another. Weights: 400 text, 500 emphasis in the interface (`strong`), 600 headings. No other weights.

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

Touch, for the compact, touch and sheet blocks and the phone components (`TextStyle::TouchSmall`, `TouchBody`, `TouchTitle`, built): `touch-small` 15 on 20, `touch-body` 17 on 22, `touch-title` 34 on 40, weights 400, 400, 600; strong is 500 for the first two.

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
| `control-touch-lg` | 56 | `touch-body` | 16 | 12 | 20 | 8 | built: `Size::TouchLarge` (`src/foundation/styled.rs:385`) |

Two rungs are for fingers. `control-touch` 44 is the smallest thing a finger can hit: on a phone it is the minimum target and the size of icon buttons and of the accessories inside a field, and on a tablet or a touch screen the desktop components take it under a coarse pointer, where `touch_scale` grows them by 15/13 and controls drawn smaller grow their hit area to 44 (Checkbox). That second case is desktop layout made reachable, not a phone screen. `control-touch-lg` 56 is the phone's rung for main actions and fields. It equals the pill's height (`COMPACT_PILL_HEIGHT`), so a main action at the bottom of a screen lines up with the pill it takes the place of, and a 44 accessory fits inside a 56 field with the 4 inside gap (`INSIDE_GAP`, Button). Its icon is 20, as on the pill's buttons (`compact_bar_button`). A field's marks at 56 are not a rung smaller: its clear button is the 44 icon button and its spinner is 20, the rung's icon. HTML: `data-size="touch-large"` and `--penna-control-touch-lg`, which `theme_css` writes with the rest of the ladder.

Fields use the field inset because their text starts at the edge. `Size` carries the whole row of the table: `control_px`, `control_text`, `inset`, `field_inset`, `icon_px` and `icon_gap`, and `Size::Size(px)` takes the nearest rung (`Size::rung()`) for everything but its height. Buttons are the exception: a `Size(px)` button keeps a proportional inset (a fifth of its height) and icon (three quarters of it) and a `space-6` icon gap, for custom shapes such as the title bar's chrome button. Chips take their rung's inset, icon gap and icon like buttons, and selects their rung's field inset and icon gap like fields; badges and keycaps pick their look by rung too, `Size(px)` included, while a checkbox at `Size(px)` takes that side. Badges, keycaps and checkboxes sit in a line of text, so from `Large` up they draw like `Medium`. A field's own marks (clear button, spinner, select chevron) are a rung smaller than the field from `Medium` up; `xs` and `sm` fields keep their own size.

Other heights: `control-chrome` 22 (controls in bars, today `PANE_HEADER_CONTROL_HEIGHT`); `bar` 34 = `control-chrome` + 2 × `inset-chrome` for pane headers when the app gives panes a margin (`Theme::pane_margin` above 0), and 40 when panes sit flush (the default, `pane_margin` 0); the title bar is the pane header plus 2 × `pane_margin`, so beside the panes its center line is theirs; a title bar laid across the top of the panes instead (an app's window tabs with its sidebar hidden, `Theme::title_bar_over_panes_height()`) is the pane header plus one `pane_margin`, since the panes' own top margin finishes the gap under it, which makes it 40 at the default margin of 6, as tall as with panes flush, and a macOS window's traffic lights center on whichever of the two shows; its chrome buttons are 32 with a 17 icon (`CHROME_BUTTON_SIZE`, `CHROME_ICON_SIZE`); `row` is `control-md` (28 at regular density) for list, tree, sidebar, picker and menu rows, and a menu row under a coarse pointer is 44; a pane's own list rows (`picker::list_row_height`) and the settings navigation's Back slot are `control-lg` 32. Tabs take only a rung's height (`control-lg` 32 by default, any rung with `.with_size()`); their label, inset and icons are the caller's (`small` text in the stories and in `tab.css`).

Icons: 12 beside `small` and `caption` (menu rows included), 14 beside `body`, 16 beside `lead` and `heading`. List and tree rows are the exception: their icons stay 14 beside `lead`.

Inside a line of text, keycaps and badges are as tall as the line of the text they sit in (`TextStyle::line_height`: 16 in `small`, 18 in `body`), and checkboxes are 14 at `xs` and `sm` and 16 from `md` up, never a control size. Like every length here, these are design pixels and grow with the interface font.

Width: reading text stops at a measure of 640 (about 75 characters of `read-body`), natively and in HTML alike. A table column in reading text sizes to its content from 48 to 480. Sidebars are 200 to 256 wide. These widths are layout geometry rather than tokens, in design pixels like everything else, so they grow with the interface font.

Density (built): one setting picks the default control and row height: compact 24, regular 28, comfortable 32. Text and gaps stay, and so do the other rungs. It is `Appearance::density` (Size, under Text size), and `Size::Medium.control_px()` follows it, so buttons, fields, selects, segmented controls and list, tree, menu and picker rows move together. The HTML reads `--penna-control-md`, which `:root[data-density]` changes (`?density=` on a page). `Size::rung()` measures `Size(px)` against the regular ladder, so a fixed-height control keeps its rung at any density. A phone has no Density row and is always comfortable (Phone).

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

On a see-through window both, and `fg-faint` and `faint` with them, are the ink at a share over what shows behind it instead of a mix, so they take the hue of the desktop coming through rather than sitting on it as gray. The share is the least that keeps, over the worst ground the window can show, the contrast the mix has on the opaque window: the window's fill over the brightest backdrop for light ink and over the darkest for dark ink. The app says how dark and how bright its backdrop can get, as sRGB gray values (`Theme::set_window(opacity, [darkest, brightest])`; 0 and 1, black and white, unless it says), and no share goes past the ink itself, so muted text is never heavier than body text. An opaque window, and every HTML page, draws the mixes as above.

A theme can bring its own base colors instead (`colors`, Look): `bg`, `fg` and the status colors, for light, dark or both. An app uses this to draw its chrome in the colors of what it shows, as zz does with a terminal theme. Everything above them is worked out the same way from whatever they are: the surface steps, `divider`, the washes, `primary` (always `fg`), `on-accent` and the glass floor.

Surfaces stack in this order, each layer a step away from the one under it (darker in light, lighter in dark): window `bg`, then `panel`, then cards (`settings.card`), then `popover` and floating panels.

A window has two planes. Chrome (the sidebar, the title bar and the gaps between panes) sits on the window's ground, and the sidebar is one `panel` step above it. Content (panes and the settings page) sits on a plate of `bg` over the window (`Theme::content_plate()`), so the sidebar always reads one step lighter than content in dark and one step darker in light. A card is one step above the plate it sits on (`Theme::content_ground()`), not above the window.

Highlight: what is picked or in focus is a wash of the ink, and a primary action a solid fill of it, whatever the tint. What is on (a switch, a checked box, a slider's value) takes `accent`.

Contrast (WCAG 2, built in step 8): in light and dark, for every tint, accent and `neutral` the knobs allow, muted text, accent, success, warning and danger as text reach 4.5:1 on `bg`, and so does the text on a solid fill of `primary` and the status colors. The muted mix is 0.30, as above. Light variants darken their accent and status colors until near-white text on them reads at 4.5:1, which also makes them read as text. Text on a solid status fill takes whichever of near-white or near-black gives more contrast, so dark variants put near-black labels on their mid-tone status fills. An accent fill holds marks, not text (a knob, a check), so its mark is near-white whenever that reaches 3:1, the floor for marks, and near-black only on an accent too pale for it; the named accents are vivid and all hold near-white in both modes. `interface_theme`'s tests assert all of it. `fg-faint` stays under 4.5 on purpose: placeholders and disabled text only.

Base colors from a theme (`colors`) and a person's own accent color are fitted to these rules, since nothing proves them in advance. An accent or status color that reads under 4.5:1 as text on `bg` moves in OKLCH lightness away from `bg`, keeping its hue and as much chroma as the screen can show, until it reaches 4.5:1; on the table's `bg` the table's colors and the named accents never move. `fg` that reads under 7:1 on `bg` moves the same way until it reaches 7:1, the floor the theme files meet: a theme brought from elsewhere (a terminal's colors) often sets its text for code at a size and weight where 5:1 reads, and interface text at 12 and 13 needs more; on the table's `bg` the table's `fg` never moves. The muted mix is the largest weight up to 0.30 that keeps `fg-muted` at 4.5:1 on `bg` (0.30 for the table's colors), so a dim `fg` gets a lighter mix.

## Corners, depth, focus, motion

- Corners: the `radius`, `control-radius` and `smoothing` knobs (Look); 12, 12 and 3 by default. A control rounds to a share of its height, a container to `radius` in pixels, a corner inside another concentrically, and smoothing never changes how round a corner looks (Corners, under Look).
- Depth: steps of fill separate surfaces; controls, panes and cards draw no shadow. `shadow-float` (y 10, blur 30, spread -6) for menus, popovers, dialogs and the window, drawn tight (0.4 of its offset, blur and spread: y 4, blur 12, spread -2.4) under a floating surface without glass, whose hairline already marks its edge, `shadow-lifted` for cards that move, and `shadow-contact` (0 1 4 #00000033) only for the knob of a switch or slider. Never let a shadow alone separate two surfaces.
- Focus: a 1.5px ring (`FOCUS_RING_WIDTH`) drawn outside the border box with its own adaptive radius (`foundation::focus::with_focus_ring`), from the keyboard only; see Focus and keys. Only fields change their fill on focus.
- Motion: appear 120ms, enter 200ms, turn 220ms, at half those under Fast motion. Respect reduced motion.
- Live motion, for work under way with no end in sight, keeps one steady speed and never eases: a spinner turns once every 900ms; a live line (a chat's activity row while the agent works) rests in `fg-muted` and sweeps a crest of `fg` across its label, left to right, once every 2600ms (a 2000ms pass, then 600ms still). The crest is 12 characters wide with a raised-cosine falloff, its colors mixed in 32 steps so no step shows, and it moves with time, not a character at a time. The glyph beside it breathes on the same cycle, opacity 0.5 to 1 and scale 0.9 to 1 on a cosine, brightest as the crest leaves it. Under reduced motion all of it holds still: the label in `fg-muted`, the glyph whole.
- A chat activity row's label is `body` in `fg-muted` on a 16 line; the detail after it (a path, a pattern, an error) is `small` in the same `fg-muted`, on the label's baseline, so the step between them is size, not ink, and both read at 4.5:1. On a touch row the label is `touch-small` and the detail `body`, since nothing on a phone is under 13. A caller can give the detail its own ink, as zz does with `fg` for an answered question.
- Pointer: one choice for the whole app, `PointerCursor` (the Pointing hand row, `pointer` in `Appearance`): the hand over everything that acts on a click (buttons, rows, chips, tabs, tiles, switches, checkboxes, segments, selects, links), or the arrow over all of them. The hand is the default. Every clickable element calls `click_cursor()` from `StyledExt` and nothing else sets the hand, so no control differs from its neighbors. A disabled control, a surface that only takes focus, a drag handle and a scrollbar keep the arrow; text keeps the I-beam. Content draws its own pointer: a page in a browser and a link in a terminal follow the page and the terminal, not this choice. HTML: every clickable rule reads `--penna-click-cursor` (`penna.css`), `pointer` by default and `default` under `:root[data-pointer="arrow"]` and on anything `:disabled`, `aria-disabled="true"`, `data-disabled` or `data-state~="disabled"`; a row is clickable when it is a `button`, a link or has a button, menu item, option, tab or tree item role. A page sets `data-pointer` from the same `pointer` preference; the kit pages take `?pointer=arrow`.

## Look

penna has one look, and every app and person adjusts it through a few knobs. The look is calm and flat: steps of fill carry the structure, so a dense screen holds few lines, the only thing that floats is what sits over the content, and glass marks what sits over something (Glass, below). Apps differ by their tint, their corners, their glass and their content (Tiles, Cards, display text), the way apps on one platform share their controls and differ in color and content. The values below are the look with the default knobs; `themes/default.json` and `web/tokens/default-*.css` carry them.

How each kind draws, at rest and under the pointer:

| Kind | Rest | Hover, pressed | Selected, current |
|---|---|---|---|
| Bar, panel | no fill; a panel takes `panel` | | |
| Pane | the content plate, no edge, no shadow; when panes have gaps, glass tinted with the plate, or a hairline in `divider` where the glass draws no rim | | ring |
| PiP (a pane floating over the others) | glass in the pane's own color, or the pane's ground opaque when glass is off; a hairline in `divider` where the glass draws no rim, gaps or not; `shadow-float` | | ring |
| Card | one step of fill above its ground, no edge, no shadow | | ring |
| Floating (menus, popovers, tooltips, dialogs, toasts, palettes, buttons over the content) | glass and `shadow-float`; where glass is off or draws no rim, the `popover` fill under a hairline in `divider`, and without glass `shadow-float` drawn tight | | |
| Button | a wash of the ink, no edge, no shadow; primary is a solid `primary` fill | a deeper wash | a deeper wash |
| Quiet button, icon button, menu row | no fill | a wash | the highlight |
| Item (list, menu and tree rows, tabs in content) | no fill | a wash | the highlight |
| Chrome item (sidebar rows and entries, window and page tabs) | no fill | a wash | glass |
| Field | a wash of the ink, no edge | a deeper wash | while typing, the highlight, no edge |
| Label (badge, keycap) | a soft wash in its tone, no edge | | |
| Rule | a hairline in `divider` | | |

- The highlight on a selected row is a wash of the ink, whatever the tint. A toned row's highlight is its tone's solid fill. A field being typed in takes the same wash a selected menu row takes on its ground, so a search field and the picked row under it read as one.
- A picker tile in Settings (`settings.tile`: the theme, mode and app icon tiles) has no fill and shows its pick as a menu row does, in the same wash around its preview; it rings only for keyboard focus.
- On that solid fill everything in the row takes the selected ink: label, muted text, icons, and a toned chip or badge, which draws as a wash of the selected ink in place of its tone. The word still says the status (Content). Natively the row hands its fill to its children as their ground (`.on(ground)`, with `cx.theme().ground_at(look, state)`); in HTML a row with `data-state~="selected"` or `"current"` and a solid highlight sets the ground inside it.
- A chrome item draws its selection as glass, a pill of the material over its bar or sidebar, under the material's rim and light. Its tint is the highlight it replaces, pushed away from the ground by the material's opacity, so over a flat bar or sidebar the pill reads exactly as strong as the highlight does where glass is off, and the blur adds only what lies behind; the highlight is its fallback. The chrome items are sidebar rows (`sidebar.row`, the Settings sections too), the flat sidebar's entries (`row.nav`), window tabs in the title bar (`tab.window`) and a browser's page tabs (`tab.page`). Glass is neutral, so the item keeps the window's ink and hands its children a glass ground, not a solid one. Where glass can't draw (glass `none`, the system's reduced transparency, or a glass ground under the item) it takes the highlight like any row, and a picked window or page tab adds the control edge (its ground's ink at 10%, `outline` wide) in place of the rim, so it still stands apart from its strip or tab bar.
- The rule is one for every solid ground, whatever its color, so a primary row in the ink counts as much as a danger one: a tone's solid fill under a selected or current row, and a prominent surface (a solid button, chip or badge). Natively the ground says so (`Ground::solid`: a layer's when it sits on a solid ground, its highlight is the solid fill, or it is prominent and not glass; a paint's when its ground is solid or its fill in that state is more than half opaque). There every tone but a solid one reads as neutral, and muted ink is the ink itself; faint keeps its mix toward the fill. A solid chip or badge keeps its tone, since its own fill carries its label. A status dot turns to the muted ink with the rest, and a dot on a solid chip takes the chip's ink.
- In HTML the tokens write one scope per solid ground, listing every role whose selected or current highlight is that fill in the theme (rows, list items, menu and tree rows, tabs, toggled buttons, and the chrome items where glass is off). A role with toned rules lists its untoned rows (`:not([data-tone])`) in the neutral highlight's scope and each `[data-tone]` in its tone's. A scope carries the inks that differ and `--penna-dot` for dots. The looks painted on a solid fill are written once for every scope they fit: the labels (badges, chips, keycaps) and the buttons rows hold (quiet, icon, flat icon and row icon buttons, not prominent ones and none inside a field set in the row), each button with every value it sets and every other value at its initial, so a toned one loses its tone as it does natively. Soft and prominent buttons keep their window paint there, and rows hold neither. A button inside a field set in the row (a menu stepper's quiet buttons) paints on that field's ground natively, so the scopes leave it alone. The solid scopes come last in the tokens, after the reduced-transparency block, so they beat a ground scope around the row at equal specificity. A dot reads `--penna-dot` before its own color, and `data-solid` sets it to the tone's on-ink. Tree rows, like list items, switch their icons to the row's muted ink on selection.
- Hairlines appear only where they mean something: rules, the outline variants (`button.outline`, `field.outline`, `badge.outline`, `chip.outline`), the edge of a gapped pane or a floating surface where glass draws no rim, and a picked window or page tab where glass draws none. Increase contrast adds none. Outlines are 1 wide.
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
| `face` | the interface face | the platform's |
| `mode` | `follow`, `light` or `dark` | `follow` |
| `colors` | `bg`, `fg`, `success`, `warning` and `danger`, for light and for dark | the Color table |

There are no presets to pick. The look starts from the platform's knobs (the `macos` theme file on Apple platforms, `default` elsewhere), and a person sets every knob directly in Appearance, the same in every app:

| Knob | Does |
|---|---|
| Mode | system, light or dark, unless a theme locks it |
| Accent | `tint`: one of the named accents, or a color |
| Tint, Tint strength | `neutral`'s hue and amount |
| Corner radius | `radius`, a slider from 0 to 100% along a square curve, `radius` = 25 × share², so the default 12 sits at 69% and the steps near square corners stay small. 100% is a round corner, never a pill: Controls makes pills. None on a phone, where `radius` is the look's default (Phone) |
| Controls | `control-radius`: Rounded follows `radius`, Pill is `full` |
| Glass | `glass`, a switch: on draws `liquid` with the mode's glass look (Glass looks, below), or a theme file's own material once one is pasted in; off draws none. On also makes the window see-through over a blurred desktop where the system can blur behind a window; off keeps it opaque |
| UI font | `face` |
| Mono font | the code face: code blocks, inline code and every other monospaced line in the interface. Not a theme knob, so a theme file leaves it alone. Its list opens on Monospaced, with All fonts beside it. Default is the platform's code face, or one the app names (zz: the terminal font) |
| Text size | the root rem; everything scales with it. None on a phone, which follows the device's text size from 100% to 135% (Phone) |
| Density | compact, regular or comfortable (Size). None on a phone, which is comfortable (Phone) |
| Increase contrast | a slider from 0 to 100%, contrast 1 to 2: surface steps, fills and the divider further from their ground, muted and faint text closer to `fg`; no edges |
| Motion | Off (reduced motion), Default, or Fast (Motion's durations halved) |

Appearance shows them in that order in three groups, none folded away: Look (Mode to Mono font), Size (Text size and Density) and Accessibility (Increase contrast, Motion and Pointing hand). A phone leaves out Corner radius, Text size and Density, and with them the Size group (Phone). An app's own rows go in groups of their own (zz's Terminal and Sidebar after Look, and Window, with the app icon, last), except an accessibility setting of its own, which joins Accessibility right after Increase contrast (zz's Terminal text contrast). Every setting about how the app looks lives on Appearance; another page keeps only what that part of the app does. `smoothing` and `mode` have no row: smoothing follows the platform's theme file. A theme file has no row either: an app that reads or writes theme files (the storybook's theme creator) does it itself, and `AppearanceChange::Import` sets every knob from one.

What follows the platform with no knob: the system face, when scrollbars show, the window controls, how keys are spelled, and reduced transparency. Glass is the same everywhere: on unless a person turns it off or the system asks for less transparency.

Reduced transparency has no row, since the system already has one. Where the system asks for it (Reduce transparency on macOS and iOS, Transparency effects off on Windows, `prefers-reduced-transparency: reduce` in a browser), glass draws as it does with Glass off and the window stays opaque, whatever the switch says. Linux, Android, Safari and Firefox have no such setting, so there Glass off does the same. The app reads the system's setting and turns `glass` to `none` in the Appearance it applies, the way it turns Motion off for the system's Reduce motion.

Everything else is the look and has no knob: each kind's fill, edge, shadow and shape, every state look, outline width, highlight strength, status colors (a theme file's `colors` may bring its own, but no person sets them), spacing, sizes, type and motion.

Corners follow the Corner radius knob, a share from 0 to 100% (`radius` = 25 × share², Knobs below), by four rules, so at any setting every rounded thing reads equally round whatever its size:

1. A control takes its corner from its height: `roundness` × its height, where `roundness` is 45% × the share of `control-radius` (`Theme::control_roundness()`; 31% at the default 12, 45% at 25). Buttons, fields, selects, tabs, chips, badges, keycaps, the segmented control's track and every row (sidebar rows and entries, menu, list and tree rows) are controls. Pill (`control-radius` `full`) makes the capsule controls half their height instead; the controls that are never capsules (menu rows, keycaps, text areas) keep `roundness` from `radius`. A 28 control at the default is 8.7, a 22 one 6.9, a 46 sidebar entry 14.3; at 100% they are 12.6, 9.9 and 20.7.
2. A container keeps `radius` in pixels: cards, settings cards (a phone container on a phone, Phone), panels, popovers, dialogs, toasts, tooltips, the window. A pane keeps it up to 10.24 (64%), then eases to 16 at 100% on a parabola that leaves `radius` at its own slope and lands flat, so the slider has no kink and a terminal's corners never eat its text: 16 × (1 − (1 − share)² / 0.36), 11.8 at the default 69%, 14.2 at 80%, 15.6 at 90% (`Theme::pane_radius()`), the same under Rounded and Pill controls. At 100% a pane under `smoothing` 4 is drawn 29.5 out from the corner and turns 4.7 in at 45°, and its 22 header buttons sit about concentric in it under Rounded (9.9 plus `inset-chrome` is 15.9). One shorter than its corner rounds no further than 45% of its shorter side, on a `tanh` curve (`corner-fraction`).
3. A corner inside another is concentric, measured on the drawn corners: inner = outer minus the inset between them, and never rounder than a control of the inner one's height. The segmented control's pill is its track's corner less the border and inset; a picker's panel is its rows' corner plus its inset, and the chat composer its buttons' corner plus its inset (square at a `radius` of 0), and a tab strip its tabs' corner plus its inset; a PiP takes the pane's corner. A menu's rows are the exception: a popover keeps `radius` in pixels while its rows are controls, so at middle settings its corner less the row inset would leave the rows nearly square (1.3 at 54%); each row takes its own control corner instead, never rounder than the popover's.
4. Smoothing changes how gradually a corner turns, never how round it looks. `smoothing` is a superellipse exponent, and a superellipse in the box of a circular arc reaches less far into the corner (at 4 it looks like an arc of 0.54 times the radius), so every corner is drawn in a box grown until its midpoint sits where the arc's would: 1.21 times the radius at 2.5, 1.42 at 3, 1.84 at 4. Where that box would pass half the side, it stops just short of it (the renderer draws a corner that reaches half the side as a plain round) and the exponent eases toward a circle until the midpoint matches. A capsule is a circle. Every role takes the theme's `smoothing`; nothing caps it.

### Glass looks

Glass has two looks, one for light and one for dark (`foundation::glass`, `GlassLook::of(mode)`). Dark glass and light glass treat the backdrop in opposite ways: dark dims it with a soft curve and caps its highlights so the chrome stays dark; light lifts it toward white and calms its color, under a much denser fill. A look holds single values:

- the backdrop behind a see-through window: chrome opacity, blur, saturation, then a curve (contrast, then brightness: a backdrop value `v` becomes `brightness × (contrast × v + (1 − contrast) / 2)`), a brightness cap (`compressLuminance`, `v / (1 + luma × (1 / cap − 1))`; 1 is none) and the share of the wallpaper's hue in the neutral tint;
- the content plate's opacity over the chrome;
- the floating material's fields and the pane rim's fields that differ from `liquid`;
- the window opacity where the app can't shape the backdrop (a plain blur behind the window, or none).

All of it is in sRGB-encoded values: the window's pixels blend over the backdrop that way, and the curve and the cap work on encoded values too.

| | Light | Dark |
|---|---|---|
| Chrome opacity | 0.86 | 0.65 |
| Content plate (content in all) | 0.5 (0.93) | 0.57 (0.85) |
| Blur, saturation | 40, 3 | 40, 1.6 |
| Contrast, brightness | 0.6, 1.47 | 0.75, 0.55 |
| Cap | none | 0.55 |
| Backdrop black to white | 0.29 to 1 | 0.07 to 0.35 |
| Floating tint | 0.8 | 0.75 |
| Window opacity without a shaped backdrop | 0.97 | 0.97 |

Both looks draw the wallpaper's hue at 0 and float glass at blur 30 with no refraction or dispersion, glint evenness 1, no light direction and edge spread 1. Light's floating material has saturation 2, contrast 1.5, brightness 0.3, bezel 1.5, glint 0.2 at width 0.85 lit from a quarter turn, fresnel 0.05, edge shadow 0.2 at width 0.5 and full edge alpha, and noise 0.025. Its pane rim has bezel 4.5, glint 0.2 at width 1.7 and full light alpha with the same angle and fresnel, and edge shadow 0.2 at width 0.3. Its selection has bezel 1, tint 0.6, saturation 2.6 and no dispersion or noise. Dark's floating material has saturation 1.5, bezel 0.5, glint 0.17 at width 0.5, no fresnel, edge width 0.5 and no edge shadow, adds noise 0.01 and a glow of 0.02 over 300, and merges floating glass within 60. Its pane rim has no dark line: bezel 30, glint 0.17 at full light alpha, edge width and spread 0.

Floating glass (popovers, menus, the palette, a PiP) casts the look's float shadow: strength, blur, offset and spread (`float_shadow`, `float_shadow_blur`, `float_shadow_y`, `float_shadow_spread`), 0.8, 30, 10 and -6 in dark, the same numbers as `shadow-float`, and 3.5, 15, 0 and -9 in light, in the mode's shadow ink (black at 5% in light and 20% in dark, times the strength). Glass off keeps its tight float shadow and the window keeps `shadow-float`.

A chrome selection (the selected sidebar row, flat sidebar entry, window tab pill and browser tab: the `Highlight::Glass` roles) draws the floating material with the look's `selection.` overrides on top, and a tint lightness set there replaces the lightness the highlight would give. It casts the look's selection shadow (`selection_shadow`, `selection_shadow_blur`, `selection_shadow_y`, `selection_shadow_spread`): strength 1 over offset 0, blur 3 and spread 0 in light, and strength 0 in dark, so none, over offset 2 and blur 6. Hovered rows and pills, segmented thumbs and the composer don't take it. A shadow may leave the scroll list or tab strip that clips its piece, up to the nearest surface that contains shadows (the sidebar panel and every bar, `Styled::contain_shadows`), except on a side where the piece itself is cut: a row scrolled partly out keeps its shadow inside the list on that side.

A pane with gaps casts the look's pane shadow (`pane_shadow`, `pane_shadow_blur`, `pane_shadow_y`, `pane_shadow_spread`): strength 1.1 over offset 0, blur 3 and spread 1.5 in light, and strength 0 in dark, so none, over offset 3 and blur 14, in the same shadow ink. It is drawn only outside the pane's shape, so it never shows through a see-through plate, and it reaches no further than the pane margin, so it stops at a neighbor's edge.

Each look keeps three rules over the darkest and the brightest backdrop it can show, with the sidebar's panel step on the chrome (`glass`'s tests hold them):

1. Body text reads at 4.5:1 on the chrome and on content.
2. Muted text, the sidebar's second line, reads at 3:1 on the chrome.
3. Light chrome is at least L* 80 and light content at least L* 90 over black (86.05 and 94.7); dark chrome is at most L* 30 and dark content at most L* 22 over white (23.5 and 13.85).

Not in the look: the panel wash step, the inactive pane veil, border and divider weight and the text floor on glass. A theme file's own material keeps its material and takes the mode's look for the rest. Where a window is never see-through (the thin clients), the look sets only the floating material and the pane rim.

Glass is for what sits over something, since it reads only with something behind it:

- Floating surfaces, and buttons that float over the content (`ButtonLook::Floating`), over whatever is under them.
- Panes over the window's frame, when panes have gaps (`Theme::pane_margin` above 0): the content plate is the tint, at the plate's own opacity, under the material's rim and light, with no blur or refraction, since the frame under a pane is one flat fill. On a see-through window a glass tint only recolors the window and keeps its opacity, so there the pane paints the plate as a plain fill and draws only the material's rim and light over it. A pane that sits flush has nothing under it and takes the plate.
- The selection in chrome, over its bar or sidebar (chrome items, above).
- The chat composer (`field.composer`), over the transcript that scrolls under it.

Never cards, other fields, bars, panels, or the rows and controls inside content: they sit in their ground, not over it. Where glass is off each falls back: floating surfaces to the `popover` fill under a hairline in `divider` and the tight `shadow-float`, panes to the content plate with a hairline in `divider` when they have gaps, a PiP to its pane's ground, opaque, under that hairline, chrome selections to the highlight, the composer to the activity card's wash made opaque on its ground, with no edge. A gapped pane takes the same hairline under a material of its own that draws no rim (no bezel and no edge shadow), so its edge always shows.

A material is a full glass definition (blur, tint, saturation, bezel, refraction, specular, rim) for light and for dark:

- `liquid`: the base material both looks start from, blur 20, bezel 3, refraction 4, a specular rim lit from above, one set of values for light and one for dark (`interface_theme::liquid`). The mode's look changes its blur, tint, saturation and edges (Glass looks). Where the renderer can't bend light it keeps its blur, tint and rim and drops the lens: the web draws the lens only where `kit.js` refracts, and natively a window that can't sample its backdrop draws the fallback.
- A material of an app's own takes the same fields. The kit holds every material to two rules: text on it reaches 4.5:1, so its tint never drops below the opacity that gives that over the worst content, and it has a solid fallback, the `popover` fill.
- On a see-through window glass is as see-through as the window under it: the tint recolors what the window drew there and keeps its opacity, so the desktop shows through a popover as much as through the chrome around it, in the same hue, one step lighter in dark. Over opaque content it covers as much as ever, so the 4.5:1 floor still holds there; over the bare window, text on glass reads at least as well as text on the chrome beside it.

Natively the blur reads the backdrop up to 2.5 blur radii past a glass surface's edge; the web's backdrop filter reads only what is under the surface. Where the backdrop changes just outside a glass edge, as under a dialog stacked over another, the two blend it a little differently. That is a raster difference, like text, not a choice.

The kit's theme files: `default` (the table's defaults), `macos` (tint `blue`, since ala exposes no system accent; `control-radius` `full`; `smoothing` 4), the look on Apple platforms, and `yaru` (tint orange; `radius` 8; `smoothing` 2; `glass` `none`), which the storybook, the HTML tokens and the parity runs draw as a third look. None of them is a choice a person makes.

HTML: a theme file's knobs generate its tokens (`cargo run --example theme_css`), so the HTML twin follows the same look. A theme that leaves `glass` out gets `liquid` in its tokens. `--penna-control-radius` carries the control corner (9999px for `full`). Under `prefers-reduced-transparency: reduce` the tokens give every glass role the `popover` fill and no backdrop. The mode's look changes the theme's materials before its tokens are written, so the tokens and a page that writes its stylesheet from a person's Appearance (zz's HTML client) draw it. A card inside a container marked `data-ground` rises one step from that container's fill.

How the knobs are spelled and decided:

- `tint` is a named accent (`blue`, `orange`, ...), one hex color for both modes, or `{ "light": ..., "dark": ... }`, and sets `accent`; left out, it is `blue`. A person's Accent replaces it. The named accents, light then dark: blue #0a64dd #3183ff, purple #8549d1 #a366f7, pink #c02a82 #e648a0, red #cc252d #f44647, orange #ad4f00 #de6700, yellow #876400 #af8200, green #007b2c #009f3b, graphite #6a6a6a #868686. Each dark one is the most saturated color of its hue that reads at 4.5:1 as text on `bg` and holds a near-white mark at 3:1.
- `neutral` is `{ "hue": 60, "amount": 0.15 }` by default, hue in OKLCH degrees. Amount 1 is 0.02 of OKLCH chroma on `bg` and 0.35 of that on `fg`. The base colors in code are the Color table; a neutral moves them by the difference between its tint and the default's, so the default reproduces the table exactly and other values tint like a person's tint did. A person's Tint and Tint strength override the hue and the amount.
- `radius` and `control-radius` clamp to 0 to 25 and `smoothing` to 2 to 5, both where a file is parsed and where a person's override applies. `full` draws capsule controls (buttons, chips, tabs, badges, swatches) as pills and any other control at 25 with its 45% stop. A `radius` or `control-radius` of 25 is a round corner, never a pill.
- `glass` as an object is a material: `light` and `dark` each name the fields that differ from `liquid` in that mode; an object with neither applies to both modes. Every material's tint rises to the lowest opacity that keeps `fg` at 4.5:1 over a black backdrop under dark text and a white one under light text, blended in sRGB and ignoring blur: 0.596 in light and 0.705 in dark with the default colors, so both looks keep their own tint, 0.8 and 0.75.
- `mode` other than `follow` beats the person's Mode.
- `colors` is `{ "light": { ... }, "dark": { ... } }`, each with any of `bg`, `fg`, `success`, `warning` and `danger` as hex colors: `{ "dark": { "bg": "#131313", "fg": "#dddddd", "danger": "#e7988a" } }`. A field left out, or a mode left out, takes the Color table's value. A neutral moves them the way it moves the table, so the default neutral leaves them exact. They are fitted (Color) after the neutral and the tint. A status color brought this way still means its status: whoever fills it keeps it red, green and amber.
- Unknown keys are ignored, so a file from before step 10 loads as the default look under its own name.
- Increase contrast (contrast 1 to 2) scales the surface steps, fills, washes and `divider` up by the contrast and the muted and faint mixes down by it, so surfaces stand further apart and secondary text reads closer to `fg`. It adds no edge to any kind.
- Keyboard focus changes no fill but a field's; everything else shows the ring.
- Rings on a selected pane, card or tile, and a row's bar mark, take the highlight color, not `accent`.
- Panels (the sidebar and the inspector) take the `panel` fill; panes and the settings page take the content plate. On a see-through window (an app's window color under full opacity, as with Glass over a blurred desktop) a bar, panel or card takes its step as a wash of the pole over the window instead of a solid color, so the window stays as see-through as the person set it. The app tells the theme (`Theme::window_opacity`), and the window's ground (`Theme::ground()`) is then that see-through fill, so whatever is drawn straight on the window takes its wash without being told. The content plate is then `bg` at the glass look's plate opacity over the window, so content lets less of the desktop through than the chrome does: 0.57 over 0.65 chrome in dark (0.85 in all) and 0.5 over 0.86 in light (0.93). `Theme::content_ground()` is the window with the plate over it, still short of full opacity, so a settings card or a control on content takes its wash against it. Controls follow: a filled control (the composer's buttons, tiles) on a see-through ground is its wash alone, with no copy of the window color under it, and `Theme::border()` is the ink at the edge weight (14%) instead of an opaque mix of `bg` and `fg`. A menu stepper's group keeps its field's wash there instead of the menu's color; a pane with gaps paints the plate as a plain fill there, and its glass rim, or its hairline, marks it. Tooltips and pane chips cast `shadow-float` like every floating surface; drop zones cast none.
- Glass is neutral: a floating surface with a tone (a solid or soft toned floating chip) draws its tone's own fill, opaque, under `shadow-float`, since a toned tint can't hold text at 4.5:1 over any backdrop. Only the neutral floating chip is glass; a callout keeps neutral glass and shows its tone in its icon. A soft toned chip lays its soft fill opaque on its ground with the soft ink, a solid one its solid fill; any tone counts, `primary` included, and a solid neutral chip is solid `primary`. In HTML a floating chip (`chip.floating`, `chip.marker`, `chip.drag`) takes `data-tone` like a badge, and `data-emphasis="solid"` draws no glass.
- A press replaces the look the row shows; nothing draws its pressed wash over its hover or selected wash. Natively a press highlight (sheet rows, touch which-key rows, and menu rows and rows under a coarse pointer) paints the wash that turns the row's hover wash, or its selection when it is selected, into the pressed one; over a tint's solid selection that is the whole pressed wash. A row that has one sets no pressed style of its own; tabs and chips keep their pressed style, which replaces the hover style.
- Muted, faint and tone ink are worked out for the ground under the text, natively and in HTML, where each ground scope (`data-ground`, `data-on`) carries the inks that differ from the window's: `--penna-muted`, `--penna-faint` and `--penna-<tone>-ink`, as values. Natively they mix the ink toward the ground's fill in OKLab rather than fade it, so the tokens write the mixed color; on a see-through window a ground whose fill lets the window through takes the ink at a share instead (Color), worked out against that fill over the brightest backdrop. Cards, panels and floating grounds differ in muted and faint only; tone inks follow the ground's ink, which only a solid selection changes.
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

## Phone

A phone screen is not the desktop made bigger. The desktop components stay what they are under a coarse pointer (Size); a phone screen is built from the components this section lists, which are sized for a thumb from the start. The chrome that moves around a phone (the pill, its stack menu, the bento overview, the zoom, the key row and sheets) is the compact, touch and sheet blocks.

Rules:

1. The whole thing is the target. A tile, row or card is tapped as one piece; no button sits inside one. More actions live one zoom in, or behind press-and-hold.
2. Nothing tappable is under 44 (`control-touch`), and what is drawn is as big as what is tappable: no small drawing with an invisible hit area, but for the mini pill and a badge on a thumbnail (Layout).
3. One main action per screen, full width, at the bottom where the thumb is, at `control-touch-lg` 56. It rides the keyboard. The top of the screen is for reading.
4. Going into a thing is a zoom: the tile grows into the screen and shrinks back the way it came. A short question that returns you where you were is a sheet.
5. State is said in words and a tone dot, never by look alone (Rules 8).
6. Text is `touch-body` 17 by default and `touch-small` 15 for details. Nothing is under 13 (`body`).
7. Text says a state, a value or a name. No eyebrows, subheads, helper lines or microcopy that explains what the screen already shows.

Size and corners on a phone have no knobs: Appearance on a phone has no Corner radius, Text size or Density row. The interface follows the device's text size from 100% to 135% and never goes under 100%: the touch layout already sits on its minimum targets (rule 2), so a device set to smaller text leaves it at its default size, and a larger one stops at 135%. Density is comfortable. `radius` is the look's own default whatever a person stored elsewhere, so the phone containers are 24 (Corners, below). Status: zz's thin client does all three when it applies its Appearance on a phone and leaves the three rows off its Appearance page, and re-applies when the device's text size changes.

Press and hold: holding a tile, or a section header that heads a group of tiles, opens a sheet (Sheet) of what can be done to that thing, titled with its name: one-line touch rows, each a 20 glyph and a verb, the plain actions first and the one that destroys or removes last in `danger` ink. The rule is one for every tile, whatever it holds, so nothing that acts on a tile ever sits inside it (rule 1). A tap still goes in. A header that can be held is 44 tall (rule 2; List, Touch rows). A session's header is its name and nothing else; holding it gives Rename session and Close session. What a tile leaves out to stay quiet (a window's folder, branch, age, and the work's message and percent) is one muted line under the sheet's title (Sheet). Status: a tile takes the hold (`PhoneTile::on_hold`, `src/blocks/compact/tile.rs`) and so does a section header (`TouchHeader::on_hold`, `src/blocks/touch/row.rs`); zz's machine tile, window tile and session header open their sheets with it.

Layout, top to bottom:

- The platform's status area, then a top row 44 tall for the close or back control (`COMPACT_TITLE_BAR_HEIGHT`). The control's glyph lines up with the body, not its 44 box: a machine's close ends where its tiles end, 16 from the side, and settings' back starts where the large title's first letter starts, 1 in from the title's box, with the button's box hanging past that edge toward the screen's (`top_row_button`, which counts the 12 between the box and the 20 glyph and the glyph's 3 of empty margin).
- A large title 52 tall in `touch-title` (`large_title`), folding into the top row as the page scrolls (`folding_title_bar`): the page runs under the status area and the top row, which frost over the page from 4 to 28 of scroll, then the title fades into the row's middle in `touch-body` at 600 by 42. Settings on a phone fold the same way, with the back control at the start of the top row where a machine has its close at the end (`settings_title_bar` draws `folding_title_bar`, and the settings column starts its large title under the row).
- The body, `inset-pane` 16 at the sides, groups `gap-group` 16 apart.
- The bottom: the pill on a screen of tiles and on a pane, or the action bar on a zoom card or sheet. They hold the same spot, 16 from the sides and 20 above the bottom edge (`PILL_INSET_MIN`, `PILL_INSET_MAX`), so a screen has one or the other.

A screen of things is the same at every level: the top row, the large title, then tiles (Tile, Phone tile), under a section header per group when they fall in groups. Machines are square tiles two to a row with the add tile last. Inside a machine, each session's windows are a bento grid of tiles sized by what they hold, under the session's header, which is the session's name alone; the grid has no add tile, since one fixed-size tile among mixed widths leaves holes, and the header has no button. Windows and sessions are made in one place, the pill's menu (Blocks, Pill), and the pill says where they will land: inside a machine its second line is the session in view. An empty screen is the title and one add tile alone; inside a machine with no sessions that tile opens the pill's menu, so the first session starts from the same choice. Going into a tile is a zoom (rule 4), and the screen it opens may be another screen of tiles; the close control, dragging the pill up or a pinch goes back one level.

Inside a pane there is no top row, no title and no header: the pane's content runs from the status area to the bottom of the screen, padded by the safe area, and the pill is the one handle (Blocks, Pill). Nothing floats over a pane's content by default. Status: built; the agent's flip no longer floats over a phone pane and the pill's menu does not list it.

The keyboard rule: when the keyboard is up, the pill gives its place to what you type with: the composer in a chat (Chat on a phone), the key row in a terminal (Key row), or the find field while finding (Terminal on a phone), the address field in a browser (Browser on a phone). It sits 8 above the keyboard, 16 from the sides, and nothing else rides with it. Keyboard down, the pill is back. When the keyboard is up for something that is not zz's, a field on a web page, nothing rides it: the pill hides until the keyboard goes down. Status: in a terminal the pill turns into the key row, or into the find field while finding, 8 above the keyboard and 16 from the sides (`compact.rs`, `KeyRow`, `PhoneFind`); in a chat the pill gives way and the composer rides 8 above the keyboard (`compact.rs`, `PhoneDock`); in a browser the address field takes the pill's place (`PhoneFind::on_clear`), and while a field on the page has the keyboard the pill hides (`compact.rs`).

The mini pill is one of two things drawn smaller than their target (rule 2). Scrolling a pane's content 24 shrinks the pill to it (`compact_pill_morph`): 32 tall, its label in strong `body`, 14 lower than the pill's spot; scrolling back or a tap brings the pill back. Its target is 44, reaching 6 past the drawing on every side. It is a handle like a sheet's grabber: it marks where the pill waits while you read, and drawn at 44 it would cover the last line you are reading. Status: built at 32 (`COMPACT_MINI_PILL_HEIGHT`); the 44 target is not built.

A badge on a thumbnail is the other. It is an accessory of the picture, not a control beside it, so it is drawn small and its target stays 44: drawn at 44 it would cover a quarter of the picture. It is a `control-sm` 24 disc with its 12 glyph, on the floating surface (Surfaces), its corner a control's from its height, inset 4 in the thumbnail's top right corner; its 44 target is centered on the disc and may reach past the thumbnail. The pasted image's remove control is one (Chat on a phone). Status: built as `phone_thumbnail` (`src/blocks/chat/phone.rs`).

One row holds a button, against rule 1: a tab's 44 close in the browser's tabs sheet (Browser on a phone). Closing tabs one after another is half of what that sheet is for, and a hold sheet per tab would make each close three taps.

Components (each specified in its own section):

| Component | Section | Status |
|---|---|---|
| Main action and icon button | Button, Phone | built: the 56 rung (`Size::TouchLarge`) and the 44 icon button (`compact_bar_button`); zz's add card and connecting card use the 56 rung |
| Field | Field, Phone | built at 56 (`Field` and `Input` at `Size::TouchLarge`); zz's add card uses it |
| Touch row, row group, section header | List, Touch rows | built (`TouchRow`, `TouchGroup`, `TouchHeader` in `src/blocks/touch/row.rs`), with the mark trailing kind (`TouchTrailing::State`, `src/blocks/touch/row.rs:72`) and rows in a sheet (`TouchRow::in_sheet`); zz's add card and hold sheets use them, and a header takes the hold (`TouchHeader::on_hold`); the swipe not built |
| Phone tile, machine and window content | Tile, Phone tile | built (`PhoneTile`, `MiniPane`, `src/blocks/compact/tile.rs:46`, `:254`) with the hold (`PhoneTile::on_hold`), and zz's overview draws it and opens a window tile's hold sheet; a machine's last seen and the tap into the pane that asks not built |
| Session header | List, Touch rows | built: zz's session header is `TouchHeader::over_tiles` with the hold (`TouchHeader::on_hold`), which opens Rename session and Close session |
| Pill and its menu | Blocks, Pill | the pill, its tap, swipe and lift, and its menu as a stack menu with the hold, the slide and the release built (`StackMenu::pill_anchor`, `src/blocks/touch/stack.rs:575`), with zz's machine menu (`zz_ui::compact::machine_menu`): Settings, All machines, New session, then New in and the session in view over New browser, New terminal and New chat; the second line names the session in view (`zz_ui::compact::session_in_view`) |
| Stack menu | Blocks, Stack menu | built (`StackMenu`, `src/blocks/touch/stack.rs:277`; the gesture `StackGesture`, `src/blocks/touch/stack_gesture.rs:45`; key anchors `StackKey`, `src/blocks/touch/stack_key.rs:35`), with the tick (`App::selection_haptic`); zz's Close chip opens its confirm sheet |
| Pill inside a pane | Blocks, Pill | the pill, its two lines with the tile's words and tones (`PillLabel::word`), its dots, the swipe and drag built, and the pane menu (`zz_ui::compact::pane_menu`) with Find in a terminal, and in a browser the browser's stack with the pane menu behind More (`zz_ui::compact::browser_menu`) |
| Mini pill | Phone, Layout | built at 32; the 44 target not built |
| Badge on a thumbnail | Phone, Layout | built (`phone_thumbnail`, `src/blocks/chat/phone.rs`) |
| Keyboard rule | Phone, Layout | built in a terminal, a chat and a browser (the address field, and the pill hidden under a page's keyboard) |
| Phone composer, ask card, queued row, chat rows | Blocks, Chat on a phone | built (`PhoneComposer`, `AskCard`, `phone_queued_row`, `ActivityMark::touch`, `src/blocks/chat/phone.rs`); zz's plus sheet and ask cards are `zz_ui::agent::phone`; the queued sheet, Folder and Resume on the empty chat and the image zoom not built |
| Hold a message | Blocks, Chat on a phone | built: a held message opens a sheet with Copy and Fork from here (`zz_ui::agent::phone::held_sheet`) |
| Key row | Blocks, Key row | built: one 56 floating pill of 44 keys, the prefix pinned at the start and the arrow pad and hide at the end, the keys between scrolling and fading at an edge where more wait, the latch as a wash, the chord stacks, the prefix's stack with its hold, and the arrow pad with its 44 floating bubble (`KeyRow`, `src/blocks/touch/key_row.rs`); on glass the keys rest with no wash and no rim |
| Terminal on a phone | Blocks, Terminal on a phone | the terminal, scrolling, pinch, long-press selection with the system edit menu, the exited pill line, Find in the menu with the find field (`PhoneFind`), the link sheet (`zz_ui::compact::link_sheet`) and jump to bottom built; the links' underline, selection handles and the scroll indicator not built |
| Browser on a phone | Blocks, Browser on a phone | built: the pill as the bar, the browser's stack (`zz_ui::compact::browser_menu`), the address field, the progress line, the touch tabs sheet (`zz_ui::compact::tabs_sheet`), the empty state for a failed page (`zz_ui::compact::failed_page`), Picking an element on the pill and the hidden pill under a page's keyboard; suggestions not built |
| Keycap at touch size | Keycap | not built: All bindings spells keys as mono text at the leading end |
| Confirm and rename sheets | Blocks, Sheet | built in zz on penna's sheet (`zz_ui::compact::Confirm`, `Rename`): Close asks first, Rename opens a sheet with a 56 field |
| Press and hold sheet | Phone, Sheet | built for a machine, a window and a session: the title's muted line (`BottomSheet::detail`) over touch rows (`TouchRow::in_sheet`); zz's window and session sheets are `zz_ui::compact::hold_sheet` |
| Action bar | Blocks, Action bar | built (`action_bar`, `src/blocks/touch/screen.rs`), riding the keyboard in zz's cards; the floating surface not built |
| Zoom card | Blocks, Zoom card | built (`ZoomCard`, `src/blocks/compact/card.rs`), with zz's add card and connecting card; drag down, pinch out and the scanner not built |
| Sheet | Blocks, Sheet | built, with the 44 close, the phone corner and rows capped by it (`src/blocks/sheet/mod.rs:22`, `:285`, `:539`); the action bar not built |
| Empty state | Blocks, Empty state on a phone | built (`PhoneEmpty`, `src/blocks/touch/screen.rs`) |
| Status marks | below | built (`StatusMark`, `src/blocks/touch/mark.rs:12`); the count badge not built |

Status marks: a tone dot 8 across, always beside its word; the spinner at the touch rung's icon, 16, one turn every 900ms, in the ink of the text it leads (the title's in a tile, the word's in a row); a count badge 20 tall, the `touch-small` line, strong `touch-small`, where a desktop badge follows a `body` line (18). The word takes its tone's ink. Natively the mark is `StatusMark` (`src/blocks/touch/mark.rs:12`): `Success`, `Warning`, `Danger`, `Ink`, `Off` and `Spinner`, and `StatusMark::word_ink` gives its word's ink; a tile, a row (`TouchTrailing::State`, the mark `space-6` before its word) or the pill leads with it.

| State | Mark | Word |
|---|---|---|
| connected | `success` dot | muted |
| waiting for you (a permission, a question) | `warning` dot | `warning` ink |
| failed, or exited with an error | `danger` dot | `danger` ink |
| done and not yet seen, or a bell not yet seen | `fg` dot | `fg` |
| working | the spinner | muted, the work's own words |
| connecting | the spinner | muted |
| off | `fg-faint` dot | muted |
| idle, or done and seen | no mark | none, or muted |

Working is the spinner, never a green dot: `success` means a machine is connected and nothing else. No state takes `accent`: it is a person's pick and can sit on the hue of warning or danger (Yaru's tint is orange), so done and not yet seen takes the ink. Where states roll up into a count ("1 waiting for you", "2 working"), the count takes the strongest tone it holds, in the order danger, warning, `fg`.

Window tiles narrow rule 5. A window tile always shows its mark, but words only when the window asks for you or broke: "Needs approval" and "Has a question" in `warning` ink, "Failed" and "Exit 1" in `danger` ink. The words are the ones the pill says for the same pane (Blocks, Pill), so a shell that exited reads "Exited 1" on both. Working, done and not yet seen, and idle are the mark alone. A machine holds many windows and most are calm; a word on every tile turned each into a paragraph, and the tiles that wanted you drowned among them. Here the preview already shows what a calm window is doing, the spinner and the dot mean one thing everywhere, and the tiles with words are the ones to look at. The words a tile drops are the mark's accessible name, and the details are one press away in the hold sheet. Machine tiles and every other place keep rule 5 as written.

Surfaces: the Look's rule, applied to the phone: flat by default, glass only for what sits over something, and a floating surface without glass drawn as the Look says (Look; Corners, depth, focus, motion). The phone's window is never see-through, so the screen is the window's own `bg`, which is also the content plate there (Look), and the mode's glass look sets only the floating material (Glass looks): what floats is floating glass in the mode's look and casts the look's float shadow, close and soft in light and `shadow-float` in dark. Glass is on by default on the phone as everywhere: the thin client's Appearance turns it on when nothing turns it off, and Reduce transparency turns it off.

| Piece | Surface | Glass off |
|---|---|---|
| Screen | the window's `bg`, opaque, which is the content plate on a window that is never see-through | the same |
| Phone tile (machine, window), add tile | `phone.card`, `phone.add`: a step of fill, no edge; pressed a wash | the same |
| Touch rows and row groups | rows no fill, a wash when pressed; a group a `settings.card` step | the same |
| Field | the field's wash; focused, the highlight a selected row takes, with no edge (Input) | the same |
| Empty state | none of its own | the same |
| Zoom card | flat, the tile's look; once it fills the screen it is the ground | the same |
| Main action | a solid fill, never glass | the same |
| Action bar | none of its own while it holds only the solid main action; a floating surface like the pill whose spot it takes when it holds a quiet secondary or content scrolls under it | a floating surface without glass (Look); not built |
| Pill | floating glass and the float shadow (`floating_layer`) | a floating surface without glass (Look) |
| Stack menu chips, glyph chips included, and group label | floating glass and the float shadow, over the scrim; the chip under the finger the `primary` solid, a danger chip the `danger` solid | a floating surface without glass (Look) |
| Stack menu close | the solid `primary`, never glass | the same |
| Floating buttons (the gear disc) | floating glass and the float shadow (`compact_floating_group`) | a floating surface without glass (Look) |
| Folding title bar | a blur of its own over the scrolled page: blur 20, the ground at 72%, saturation 1.8 (`folding_title_bar`), with the `phone.bar` hairline under it | should be the ground, opaque, under the hairline; today it keeps its blur with glass off: not built |
| Chat composer, the phone composer row and the ask card | glass, as Chat composer says: the `field.composer` look, which focus leaves as it is | its opaque card surface |
| Jump to latest, jump to bottom | floating glass and the float shadow, a 44 icon button | a floating surface without glass (Look) |
| Find field, address field | glass, the `field.composer` look, as the composer row | its opaque card surface |
| Address suggestions | glass, the `field.composer` look, as the ask card | its opaque card surface |
| Page progress line | the `accent` fill, no track | the same |
| Arrow pad bubble | floating glass and the float shadow; the cell under the finger the `primary` solid | a floating surface without glass (Look) |
| Badge on a thumbnail | floating glass and the float shadow, a 24 disc | a floating surface without glass (Look) |
| Sheet | the `drawer` look: floating glass and the float shadow | a floating surface without glass (Look) |
| Key row | its pill floating glass and the float shadow; its keys `phone.key`, flat, a wash when pressed or latched, solid when inverted | the pill a floating surface without glass (Look), the keys the same; natively the pill matches (`key_pill_layer`), and on glass the keys rest with no wash and no rim: not built |

Outline buttons sit only on a ground with its own fill (Button); a phone screen's secondary action is quiet or text.

Corners: the four corner rules (Corners, under Look) hold on the phone unchanged, with one number added. Controls take their corner from their height: buttons, fields, icon buttons, the pill, and the floating disc, which is a control-shaped surface (a circle under Pill controls, `roundness` × 56 under Rounded), never a container's `radius`. Phone containers share one corner, twice `radius` (24 at the default 12, 16 in Yaru, 0 at 0): the phone's main rung, 56, is twice the desktop's 28, so a container rounds in the same proportion to what it holds. A phone has no Corner radius knob, so `radius` is the look's default there and the phone containers are 24. Like any container it stops at 45% of its shorter side. The phone containers are the phone tile (machine or window) and the add tile, a row group, a settings card and a settings preview, the sheet panel, the zoom card while it is smaller than the screen (once it fills the screen it is the ground and takes the screen's corner), and the scanner's picture. Inside them corners are concentric (rule 3): a mini pane in a window tile is the tile's corner less its 12 inset; a touch row is a control that is never a capsule, like a menu row, so it takes its own corner from its height, never rounder than its group's corner less the group's 4 inset, or the sheet's less the row inset 6; the icon button in the disc is the disc's corner less its 6 inset; a floating action bar is its buttons' corner plus its 8 inset. Smoothing is rule 4, as everywhere.

| Piece | Kind | Default | macos | Yaru | Native today |
|---|---|---|---|---|---|
| Main action, field (56) | control | 17.5 | 28, a capsule | 14.3 | matches (`Size::TouchLarge`) |
| Icon button (44) | control | 13.7 | 22, a circle | 11.2 | matches (`compact_bar_button`) |
| Pill (56) | control | 17.5 | 28, a capsule | 14.3 | not built: natively and in the compact block's HTML it follows the screen's corner, a capsule on a phone with round corners in every look (`floating_layer`) |
| Floating disc (56) | control | 17.5 | 28, a circle | 14.3 | matches (`compact_floating_group`, `src/blocks/compact/bar.rs:180`: a capsule under Pill, `roundness` of its height under Rounded), from its height whatever its width |
| Icon button in the disc | control, concentric | 11.5 | 22, a circle | 8.3 | matches (`compact_floating_group`) |
| Floating action bar | floating, concentric | 25.5 | 36 | 22.3 | not built |
| Phone composer row | floating, concentric: its 44 buttons' corner plus its 6 inset, kept as it grows | 19.7 | 28 | 17.2 | matches (`phone_composer_corner`, `src/blocks/chat/phone.rs`) |
| Find field, address field | floating, concentric, as the composer row | 19.7 | 28 | 17.2 | the find field matches (`PhoneFind`, `src/blocks/touch/find.rs`); the address field not built: it is 40 in the top bar, its input 28 |
| Ask card, address suggestions | phone container | 24 | 24 | 16 | the ask card matches (`AskCard`); address suggestions not built |
| Key row pill (56) | control | 17.5 | 28, a capsule | 14.3 | matches (`key_pill_layer`, `src/blocks/touch/key_row.rs`) |
| Key (44) | control, concentric: the key row pill's corner less 6 | 11.5 | 22, a circle | 8.3 | matches (`key_layer`) |
| Stack chip (52) | control | 16.2 | 26, a capsule | 13.2 | matches (`control_layer`, `src/blocks/touch/stack.rs:845`) |
| Stack glyph chip (52 square) | control | 16.2 | 26, a circle | 13.2 | matches (`control_layer`) |
| Stack group label (28) | control | 8.7 | 14, a capsule | 7.1 | matches (`control_layer`) |
| Stack close (56) | control | 17.5 | 28, a circle | 14.3 | matches (`close_disc`, `src/blocks/touch/stack.rs:1247`) |
| Arrow pad bubble | floating, concentric: its 44 cells' corner plus its 6 inset | 19.7 | 28 | 17.2 | matches (`arrow_pad_bubble`, `src/blocks/touch/arrow_pad.rs:125`) |
| Arrow cell (44) | control | 13.7 | 22, a circle | 11.2 | matches (`arrow_pad_bubble`) |
| Touch row, 52 and 64 | control, never a capsule | 16.2, 20 | 16.2, 20 | 13.2, 16.3 | matches in sheets (`row_radius`) |
| Touch row in a group | the same, at most the group's corner less 4 | 16.2, 20 | 16.2, 20 | 12, 12 | matches (`TouchGroup`, `src/blocks/touch/row.rs:259`) |
| Row in a sheet | the same, at most the sheet's corner less 6 | 16.2, 18 | 16.2, 18 | 10, 10 | matches (`src/blocks/sheet/mod.rs:285`) |
| Page progress line | none, square ends | 0 | 0 | 0 | not built |
| Phone tile, add tile | phone container | 24 | 24 | 16 | matches (`PhoneTile`) |
| Mini pane in a window tile | concentric, the tile's corner less 12 | 12 | 12 | 4 | matches (`mini_radius`, `src/blocks/compact/tile.rs:39`) |
| Row group | phone container | 24 | 24 | 16 | matches (`Theme::phone_radius`, `src/foundation/theme.rs:161`) |
| Settings card, settings preview | phone container | 24 | 24 | 16 | matches where the app says it draws a phone (`PhoneScreen`, `src/blocks/touch/mod.rs:88`; `phone_container_or`, `src/blocks/settings/mod.rs:1119`, which `SettingEntry` and `settings_preview` take their corner from) |
| Sheet panel | phone container | 24 | 24 | 16 | matches (`src/blocks/sheet/mod.rs:539`) |
| Zoom card, scanner picture | phone container | 24 | 24 | 16 | zz's zoom starts from the corner of what grows: the phone corner from a machine tile or the add tile, the mini pane's from a window (`Zoom::corner`); the card matches (`ZoomCard`); no scanner |

Decisions: a tile's title is strong `touch-body`, 500, since strong is 500 for the touch styles (Type); a tile's inset is `inset-card` 12; a main action that is loading keeps its label (Button).

HTML: the phone components' twins live with their tiers: the rung in `button.css`, `input.css` and `penna.css` (`data-size="touch-large"`); rows, groups, headers, the action bar, the empty state and the marks (`.penna-tone-dot` with `data-tone` `warning`, `danger`, `strong` for the ink and `off`, no tone for success; `.penna-touch-spinner`, `.penna-touch-badge`) in `blocks/touch/touch.css`; the phone tile, the bento grid and the window tile, the zoom card, the icon button (`.penna-touch-icon`) and its disc (`.penna-touch-disc[data-penna="floating"]`) in `blocks/compact/compact.css`; the 44 close in `blocks/sheet/sheet.css`; the 44 key row in `blocks/touch/touch.css`; the composer row, the ask card, the queued row and the touch transcript step in `blocks/chat/chat.css`. The phone container corner is `--penna-phone-radius` (`penna.css`, twice `--penna-radius`), natively `Theme::phone_radius()`; a settings card on a phone page takes it with `--penna-r: var(--penna-phone-radius)`, and natively once the app sets `PhoneScreen`; a row nested in a group or a sheet caps its corner with `--penna-nest-inset` and `--penna-nest-outer`, and a wrapper marked `data-penna-clip` (the swipe) takes its first row's drawn corner (`kit.js`).

Status: drawn in HTML on the docs Phone pages; natively the 56 rung, the 56 button and field, the 44 icon button, touch rows, the row group, the section header, the status marks, the phone tile with both contents and its hold, the sheet's 44 close, the hold sheets for a machine, a window and a session, zz's confirm and rename sheets, the pill's machine and pane menus, the phone container corner on tiles, groups, sheets, settings cards and previews and the disc, the stack menu on the pill and the key row's keys, the key row as one 56 pill of 44 keys with both ends pinned, the arrow pad's 44 bubble, the terminal's find field, link sheet and jump to bottom, the action bar, the empty state and the zoom card are built (the Phone components table), and the rest is not.

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

Icon only: the button is square (width = height) and must carry a tooltip that names the action, which is also its accessible name. A row's trailing actions use `Button::compact_icon` (24, ghost). A button set inside a tab or a row's fill, such as a tab's close or a tree row's actions, takes `.inside(look, height)` with the container's look and height: it sits `INSIDE_GAP` (4) in from the container's top, bottom and trailing edge, so its side is the container's height less 8 and its corner is concentric with the container's (that corner less 4, a circle inside a capsule), and its icon is its side less 8, from 10 to 14. Like every ghost button it washes when pointed at, the row action look (`icon-button.row`) included.

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

A chrome toggle, a title bar button that turns a view on and off (the sidebar, the chat view), shows on with its fill glyph instead of the selected fill, so the title bar has no fill at rest. Its paint, hover and press stay those of the chrome button.

In a window that is not active, a glass button flattens and its label fades, like the rest of the window's chrome.

Behavior: it fires on release of a click or tap inside it, and on Space or Enter while focused. A button with a caret opens its menu below it. A tooltip shows after the hover delay, and at once when the keyboard focuses the button.

Content: the label is a verb or a short verb phrase in sentence case, one to three words ("Save", "Add host", "Open in browser"), with no closing period. It names the action, never "OK" or "Yes". End it with an ellipsis ("Allow…") when the action asks for more before it happens, such as opening a dialog. A danger button names what it removes ("Delete branch").

Placement: buttons in a group sit `gap-related` 8 apart. A dialog's or a form's actions sit at its end, aligned to the trailing edge, the primary last and its cancel just before it. A destructive dialog confirms with a danger button, not a primary one. Ghost buttons in a bar sit `inset-chrome` from its edges.

Phone (Phone, rules 2 and 3):

- Main action: full width at `control-touch-lg` 56, `touch-body`, inset 16, a 20 icon `space-8` from the label. One per screen, in the action bar. Kinds: primary for the main action, secondary (soft) for the one secondary, ghost for a text-only way out ("Type an address instead"), danger soft for an action that removes something, named for what it removes.
- States: rest; pressed as the press highlight, a wash that rises over 120ms after a 100ms delay, holds at least 130ms and falls over 180ms (`touch::press`); disabled faint; loading with the spinner in the icon's place, or before the label when there is none. Loading keeps the label, which can turn to its -ing form ("Connecting"), so the button never changes width or loses its name.
- Corners: a control's, from its height (Phone, Corners): 17.5 at 56 and 13.7 at 44 by default, a capsule under Pill controls.
- Icon button: 44 square with a 20 icon (`compact_bar_button`), ghost, its tooltip and accessible name naming the action. Over the content it sits in a floating disc 56 tall, the pill's height (`compact_floating_group`). The disc is control-shaped, a circle under Pill and `roundness` × 56 under Rounded, never a container's `radius`; the button in it is the disc's corner less its 6 inset (Phone, Corners).

HTML: `.penna-button` with `data-penna` (the recipe: `button`, `button.quiet` for ghost, `button.outline`), `data-tone`, `data-prominent` (solid), `data-size` (`xsmall` to `touch`, and `touch-large`), `data-state="current"` (selected), `data-variant` (`link`, `text`), `disabled`, `data-loading`; the label is `.penna-label`, the icon `<i data-icon>`, and a button with a caret takes `data-caret` and holds a `.penna-caret`. A button set inside a tab or row takes `data-inside` with `--penna-inside-rows` set to the container's height, and `kit.js` rounds it concentric.

Status: matches, Phone included (`Size::TouchLarge`; the 44 icon button is `compact_bar_button`, `src/blocks/compact/bar.rs:224`). zz's phone screens don't ask for the touch rungs yet (Work order, step 21).

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

A card is a box in the flow of content that holds one thing: a message, a block of code, a table or diagram, an image, a piece of an agent's work. It rests where the content puts it. For something raised over the view, use a panel; for a group of settings, a settings card; for a status message, a notice; for something you pick or drag, a tile.

Looks (`CardLook`, `Card::new(id, look)`):

| Look | Recipe | Paint | Use |
|---|---|---|---|
| Bubble | `card.bubble` | a wash of the accent over its ground, ink as on the ground, no outline, no shadow | your messages in an agent chat |
| Code | `card.code` | solid, one step above its ground, no outline, no shadow | code blocks and inline code in messages |
| Figure | `card.figure` | no fill of its own, no outline, no shadow; announced as a figure | tables and diagrams in messages |
| Image | `card.image` | solid, one step above its ground, no outline, no shadow; announced as a figure | image tiles in messages and the composer |
| Activity | `card.activity` | a flat wash one step above its ground, no outline, no shadow; a wash deeper on hover when it takes a click | subagents, background tasks, plans and answered questions in an agent chat |

Anatomy:

- Container: the look's paint, the `radius` knob (12 by default, 8 in Yaru). No outline.
- Inset: `inset-card` 12 all round, set by the card; a caller's padding replaces it.
- Content: the caller's, with its own text style (`body` for a message, `mono` for code).
- Ground: `.on(ground)` paints the card on a ground other than the window's, such as a panel. What sits on the card passes `cx.theme().ground_of(look)` to `.on()`, so a badge or muted label inside follows the card's fill.

Behavior: none, except that an Activity card given `on_click` washes deeper on hover. A card doesn't take focus.

Placement: cards sit in the content, one step above what holds them: window `bg`, then `panel`, then cards, then popover. Cards in a column sit `gap-group` 16 apart; a heading sits `gap-related` 8 above its card. The parent sets both.

HTML: `.penna-card` with `data-penna` set to `card.bubble`, `card.code`, `card.figure`, `card.image` or `card.activity`; Bubble, Code and Activity take `role="group"` and Figure and Image `role="figure"`, as natively. `card.css` sets `box-sizing` and the `inset-card` padding at the weight of one class, so a page's own class or inline style replaces it; the caller lays out the inside. A card inside a container marked `data-ground` rises one step from that container's fill, as `.on(ground)` does natively: the tokens write every card look under each container's ground. `data-ground` on the card makes controls inside paint on its ground.

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

Phone: a field on a phone is `control-touch-lg` 56, `touch-body` text, field inset 12, a 20 leading icon, its corner a control's from its height (17.5 by default, a capsule under Pill; Phone, Corners). Its label is strong `touch-small` and its help and error `touch-small`, the error after a 16 `circle-x`; parts `gap-tight` 4 apart as above. Its accessories (clear, scan, send) are 44 icon buttons set inside it, `INSIDE_GAP` 4 or more from its edge, never a rung smaller as a desktop field's marks are. Focused, it fills with the highlight a selected row takes on its ground and draws no edge, as every field does (Input, States): the add card's address, the password in an ssh sheet and the name in the rename sheet. The fields that float (the composer row, find and the browser's address) are `field.composer` glass, which focus leaves as it is (Chat composer). At 56 the field stretches what it holds to its width, since a phone's fields and selects are full width. The accessories sit `icon_gap` 8 apart, the last 4 from the field's end; the error icon keeps `space-4` before its text. Natively: `Field::new().with_size(Size::TouchLarge)` around `Input::with_size(Size::TouchLarge)` (`src/components/field/mod.rs:20`, `src/components/input/field.rs:316`); its clear button and an accessory are `compact_bar_button`. zz's phone screens still draw fields at the default rung. HTML: `.penna-field[data-size="touch-large"]` around `.penna-input[data-size="touch-large"]`, its accessories `.penna-button[data-penna="phone.button"][data-size="touch"][data-shape="icon"]` in `.penna-input-suffix`.

HTML: `.penna-field` holding `.penna-field-label`, the control, and `.penna-field-help` or `.penna-field-error` (an `<i data-icon="circle-x">` before its text). Text runs are spans marked `data-penna-text`; `field.css` holds the twin.

Status: matches.

### Icon

An icon is a glyph from the kit's set that marks an action or a thing beside its label, or stands in for the label in a bar, where a tooltip names it. For loading use a spinner, for a key a keycap, and for a logo the app's own file (`IconName::Asset`).

Kinds:

| Kind | Call | Draws |
|---|---|---|
| Glyph | `Icon::new(IconName::Bell)`, or an `IconName` wherever a component takes an icon | one of the set's 103 glyphs, drawn for the theme |
| Asset | `IconName::Asset("icons/logo.svg")`, `Icon::empty().path(path)` | the app's own SVG, as it is |

The set: 103 glyphs in eight groups (chrome and layout, actions, things, navigation, keys, status, people and agents, theme), each on a 24 grid with a 1.5 stroke, no fill, in the current color. The two fill glyphs (`ChatFill`, `PanelLeftFill`) are their outline glyph's shape filled solid in the same color, its inner strokes cut out of the fill at the stroke's width, for a chrome toggle that is on (Button). A fill glyph keeps its outline glyph's outer edge, so nothing moves when the toggle flips. Corners take half the theme radius in grid units with the kit's corner smoothing, capped at 0.45 of the shorter side like every corner the kit draws; at radius 0 corners are sharp, caps square and joins mitered. So the set changes with the theme, natively and in each theme's `-icons.css`.

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
| Field | default | the field role: a soft fill, no edge | every text field |
| Outline | `.emphasis(Emphasis::Outline)` | an edge, no fill at rest | on a ground with its own fill, where the soft fill would blend in |
| Multi-line | `auto_grow(min, max)` on the state | the text area role | notes and messages: grows from `min` to `max` rows, then scrolls |
| Number | `NumberInput::new(&state)` with `step`, `min`, `max` on the state | a field with a ghost minus and plus button at its ends, the value centered | a number people nudge |
| Bare | `.appearance(false)` | no fill, edge or focus look | a field drawn by its container, such as a title in a rule |
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
| focused | the fill takes the highlight a selected menu row takes on the same ground; no edge, and the outline kind keeps its rest edge | from a click or the keyboard; the caret blinks only while the window is active |
| disabled | a fainter fill and edge; text, placeholder and affixes at half strength | no editing |
| loading | a spinner in the suffix | the text stays editable |
| error | `.tone(Tone::Danger)` paints the field in the danger tone: its edge, at rest and focused | its Field shows the message with an icon (Field) |

A tone paints only the edge: the tone's outline ink at rest and hovered, its focus ink when focused, and half of it when disabled. The fill, its hover step, its focused highlight, the text and the placeholder stay the field's, and the edge draws even though a field has none at rest. Every tone paints this way; forms use danger.

A selection stays painted while the field is not focused.

Behavior: a click places the caret and a drag selects. The keyboard edits as the platform does: arrows, word jumps (Alt, or Ctrl), line ends (Cmd, Home, End), Shift to select, Cmd-Z and Shift-Cmd-Z, cut, copy and paste. Enter emits `PressEnter`; in a multi-line field it inserts a line break unless the state submits on Enter, and Shift-Enter always breaks the line. Escape closes the context menu or ends a composition, then passes up. `validate` rejects an edit that fails it. The clear button empties the field and focuses it, and is never a tab stop. A number input steps with Up and Down and with its buttons, clamps to `min` and `max`, and keeps the step's decimals. Its value scrubs: while the field isn't being edited the pointer over the value is the left-right resize cursor, and pressing and sliding moves the value a step for every 4 points from where the press started, clamped the same way. A press focuses the field like its buttons, so it commits when it loses focus; one that doesn't move 3 points puts the caret where it pressed, as a click in a field does. A coarse pointer doesn't scrub.

Content: the placeholder hints at what to type ("Search sessions", "Write a commit message"), in sentence case with no closing period. It is never the only label, since it disappears with the first key.

Placement: in a form a field sits in a Field, which puts its label above it and its help or error under it, `gap-tight` 4 away; in settings it sits at the end of a row whose title names it. Fields sit `gap-group` 16 apart.

HTML: `label.penna-input` with `data-penna="field"` (`field.outline` for outline; `field.multiline` with `data-multiline` around a `<textarea>` that takes `data-min-rows` and `data-max-rows`), holding an `<input>`; `data-size` (`xsmall` to `touch`), `data-tone="danger"` for an error, `data-disabled` with `aria-disabled="true"` and `disabled` on the input, `data-bare`, `data-borderless`, `data-align` (`center`, `right`), and `data-selection="start,end"` on the input for a resting selection. `.penna-input-prefix` and `.penna-input-suffix` sit around the input; the suffix holds `.penna-spinner` while loading and `.penna-input-clear` (a ghost icon button a rung smaller) when cleanable. A number input is `.penna-number` (`data-penna="field"`, `role="spinbutton"`, `data-step`, `data-min`, `data-max`) holding a bare centered field between two buttons with `data-step-by`. `input.js` paints the caret and the resting selection the way ala does, sets `data-state="focused"`, grows text areas, and steps and scrubs number inputs.

Status: matches.

### Keycap

A keycap shows a keystroke the way it is pressed: a field's shortcut hint, a shortcut in a settings row, the which-key prefix, a tooltip's key, the key inside a button that submits. Menus show their shortcuts as plain text (`Kbd::text()`) instead; to tag something with a word, use a badge.

Kinds:

- `Kbd::new(keystroke)`: one keystroke, a key or a chord of modifiers and a key, spelled for the platform. `Kbd::binding_for_action` finds the keystroke bound to an action.
- `Keycap::new(label)`: any short label in the same cap ("↵", "1").
- `Keycap::icon(icon)`: one glyph in a square cap of the same height, for a cap that marks something no key does (the check on a picked answer past the ninth).

Emphasis: `Keycap` takes `.emphasis(..)` and `.tone(..)` like a badge. `soft` (the default) is the keycap paint; `solid` is the tone's solid fill with the ink that reads on it, a cap that is lit (a picked answer's key is `solid` `accent`); `outline` is no fill and the edge; `ghost` is the glyphs alone.

Anatomy:

- Cap: height 18 (the `body` line) from `md` up, 16 (the `small` line) at `xs` and `sm`, and at least as wide as it is tall; inset `space-4` (4) at both sides; the keycap paint (a faint fill and edge); radius the theme radius capped at 45% of the height.
- Glyphs: modifiers, tab, return, escape, backspace and the arrows are icons at 1.2 times the text size (13, or 12 at `xs` and `sm`), with ISO marks off Apple platforms, where the command key reads "Win". Everything else is text, and text parts run together with "+" off Apple platforms. Parts sit `space-2` (2) apart. `Keycap::icon` draws its glyph at the same size.
- Text: `caption` (11 on 16) at 400, `micro` (10 on 14) at `xs` and `sm`, in muted ink, or in the solid's ink on a lit cap. A letter shows uppercase unless `.lowercase()`.

Sizes: `md` by default (18), beside `body` text; `sm` (16) beside `small` text. `xs` draws like `sm`, and `lg` and up like `md`, since a keycap sits in a line of text. `Size(px)` takes its rung's look. `touch` (22, the `touch-body` line) sits beside `touch-body` text on a phone, its text `touch-small` (15 on 20), its glyphs 18, inset `space-6` at the sides: the key at the end of a binding's touch row (Terminal on a phone).

States:

| State | Look | Notes |
|---|---|---|
| rest | the paint of its emphasis, muted ink (the solid's ink when lit) | a keycap is read-only, so it has no hover, pressed, focused or selected look |
| disabled | the fill at the disabled opacity, the disabled ink | `.disabled(true)`, for a cap in a row that is disabled; a lit cap stays lit, dimmed |

On a fill, `.on(ground)` gives it the paint and ink that read there, as inside a solid button.

Behavior: none. It takes no pointer or keyboard input and is never in the focus order. With `.id()` it is announced as a label named by the spelled-out keystroke ("⌘K", "Ctrl+K").

Content: one keystroke per cap. A sequence, such as a prefix and then a key, is one cap per keystroke. Use `.lowercase()` only where the case of the key matters.

Placement: the caps of one sequence sit `gap-tight` (4) apart; a cap sits at the end of the row or field it belongs to.

HTML: `<kbd class="penna-kbd" data-penna="keycap">` with `data-kbd`, the keystroke as the keymap spells it (`cmd-shift-p`), which `kbd.js` fills with glyphs and an `aria-label`; `data-kbd-lowercase`; `data-size` (`xsmall`, `small`, `touch`); `data-emphasis` (`solid`, `outline`, `ghost`) with `data-tone`, and `data-solid` naming the tone of a solid one; `data-state="disabled"`. A fixed label holds its text in a `span` marked `data-penna-text`, as `kbd.js` writes it; a `Keycap::icon` cap is marked `data-glyph` and holds an `i` with `data-icon`. `data-kbd-text` on any element fills it with the spelled-out keystroke.

Status: matches, but for the `touch` size, which is not built.

### Key field

A key field records one keystroke the person presses: a shortcut, a prefix key. To show a keystroke that can't be changed, use a keycap; to type text, an input.

Kinds: one. `KeyField::new(&state)` draws a `KeyFieldState`, which holds the keystroke (`.value()` when built, `set_value()` later, `keystroke()` to read it) and emits `KeyFieldEvent::Record(keystroke)` when one is recorded.

Anatomy, all from the field's rung of the control ladder (Size):

- Container: a field (the field role's fill, a hairline edge of at least 1px, radius the theme radius capped at 45% of the height), height `control_px`, the field inset at both sides (6, 6, 8, 10, 12, 12 from `xs` to `touch`). It is as wide as its container.
- Value: the keystroke as a keycap, centered; `sm` caps at `xs` and `sm`, `md` caps from `md` up. On a ground (`.on(ground)`) the cap takes that ground too.
- Placeholder: with no keystroke, the placeholder (`.placeholder()`, or "None") centered in the field's faint ink, in the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`).
- Prompt: while recording, "Press a key" at the start of the field, in the faint ink, in place of the value.

Sizes: `md` by default (follows density). Same rungs and uses as a field.

States:

| State | Look | Notes |
|---|---|---|
| rest | the field's fill and edge, the value centered | |
| hover | the fill's hover step | not when disabled |
| focused | the field's focused fill, the value centered | from Tab, or after a recording |
| recording | the focused look with the prompt at the start | the field holds focus |
| disabled | the field's disabled paint | leaves the focus order and never records |
| error | `.tone(Tone::Danger)` paints the edge in the danger tone, like a field | |

Behavior: a press focuses the field and starts recording, as a press in a text field focuses it; Tab only focuses it, and Enter or Space then starts recording. While recording, the next key that isn't a modifier alone is recorded: it is taken before any key binding (`App::intercept_keystrokes`), so a shortcut the app binds is recorded instead of run. Recording stops, focus stays, and the field emits `KeyFieldEvent::Record`. A held key's repeats are ignored. A symbol typed with Shift records as the symbol (`"`, not Shift-'), the way a keymap spells it; Shift stays on letters and named keys. Escape with no modifier cancels, and so does leaving the field.

Content: the placeholder says what no keystroke means ("None", "Unbound"), in sentence case. The settings row's title names what the key does, and its description says when it is pressed.

Placement: at the end of a settings row, on the card's ground, at a width the caller sets (96 fits a chord of three modifiers and a key at `md`); in a form, in a Field.

HTML: `div.penna-key-field[data-penna="field"]` with `data-key`, the keystroke as the keymap spells it (`ctrl-b`, `cmd-shift-p`, `%`), `data-placeholder`, `data-size` (`xsmall` to `touch`), `data-tone="danger"`, `aria-disabled="true"`, and `data-state="recording"` (or `"focused"`) to show that state at load. `key_field.js` makes it a `role="button"` tab stop, writes a `<kbd class="penna-kbd" data-penna="keycap" data-kbd>` for the value (filled by `kbd.js`) or a `.penna-key-field-text` run for the placeholder and prompt, sets `data-state` (`focused`, `recording`, `disabled`) and `aria-label`, records on a press or on Enter and Space, captures the next keydown on the document while recording (default prevented, propagation stopped), and fires `change` with `{ key }`. A field marked `data-state="recording"` starts recording when the page loads.

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

Touch rows (Phone): a list on a phone (found machines, chats, settings) uses touch rows.

- Row: one line 52 or two lines 64 tall (`TOUCH_ROW_HEIGHT` and `TOUCH_DETAIL_ROW_HEIGHT`, which the sheet's rows share), inset `space-12` at the sides, children `space-12` apart. Leading icon 20 in muted ink. Title `touch-body`, detail `touch-small` muted `space-2` under it, both on one line. The `phone.row` look: no fill at rest, the press highlight while held (and the hover wash in HTML), the disabled paint. Corners: a control that is never a capsule, like a menu row: `roundness` from `radius` × the row's height (16.2 at 52 and 20 at 64 by default), never rounder than the corner of what holds it less the inset between them (Phone, Corners). The fill is the row's own rounded shape.
- Trailing, one of: a glyph when the tap zooms in (`chevron-right`) or adds the thing (`plus`); a value in `touch-small`, muted, at most 60% of the row; a 20 check in the ink on the picked row of a choice; a tone dot and its word; a 16 spinner and its word. A mark sits `space-6` before its word, which is `touch-small` in the mark's ink (Phone, Status marks). A trailing glyph is 20 in the muted ink, the leading icon's size and stroke; no accent. Icons center on the row, not on the title's capitals, so one-line and two-line rows place them alike. The whole row is the target; nothing in it is a button.
- Disabled: the title faint, the leading icon and the detail saying why ("No zz on it yet") in the disabled paint's muted ink, no trailing mark. It keeps its place and takes no fill, hovered or pressed.
- Group: tappable rows in a `settings.card` with the phone container corner (Phone, Corners), `space-4` in from its edges and `space-2` apart, as a menu's rows sit in a popover, and no separators: each row's fill is its own rounded shape and never meets an edge or a line. The group grows by its inset, so a row's leading icon sits 16 in from the group's edge (4 plus the row's 12), the page's rhythm.
- Swipe: swiping a row left reveals one trailing action 64 wide (`space-64`), solid danger, as tall as the row ("Forget"), clipped to the row's corner.
- Section header: over a group or a grid, 28 tall (as the touch which-key list's), strong `touch-small` muted, inset 16 so it lines up with the leading icons of the rows in the group under it (12 over a grid of tiles, so it lines up with their text), `space-4` above the card; an optional count at its end in regular `touch-small` with tabular figures, `gap-related` 8 from the title, the number of things under it. A header that heads a group of things you can act on as a whole (a session's windows) can be held (Phone, Press and hold), and is then 44 tall with its text at the same foot, so the target meets rule 2. A header holds no button: new things are made from the pill's menu (Blocks, Pill), never from the header.
- Danger: a row that closes, removes or forgets takes `danger` ink for its glyph, title and detail, and comes last in its group.

Status for touch rows: built in `src/blocks/touch/row.rs`: `TouchRow` (one and two lines, the leading glyph, the trailing glyph, value, check and mark with its word (`TouchTrailing::State`), disabled, danger, and the quiet and accent inks the ask card's last rows take (`TouchRow::ink`)), `TouchGroup` (the phone container corner, rows 4 in and 2 apart with their corners capped by the group's) and `TouchHeader` (the count, `over_tiles`, and the 44 `holdable` form). Sheet rows share their frame (`touch_row_frame`, `src/blocks/touch/row.rs:31`) and keep their 40 icon tiles, with their corners capped by the sheet's. A header given `on_hold` is 44 tall and takes the long press. A row can keep the glyph slot empty (`TouchRow::slot`), put a check in it in the row's ink (`TouchRow::checked`), and end in a 44 button (`TouchRow::end`, the row's trailing inset then 4); the browser's tabs sheet is the one row that does. Not built: the swipe; phone code still uses `sheet_row` only. HTML: `.penna-touch-row[data-penna="phone.row"]` (`data-lines="2"` for two lines) holding an `<i data-icon>`, `.penna-touch-row-lines`, and a `.penna-touch-row-value`, `.penna-touch-row-state` (a mark and its word, `data-tone` `warning`, `danger` or `strong` for the word's ink) or trailing `<i data-icon>`; `.penna-touch-group[data-penna="settings.card"][data-ground]` around rows, whose ground scope gives its rows the card's muted ink and washes (`theme_css` writes `phone.row` into every ground scope); `.penna-touch-swipe` (`data-penna-clip`, `data-open`) holding a row and `.penna-touch-swipe-action`; `.penna-touch-header` with an optional `.penna-touch-count` (`data-hold` for the 44 header); `data-tone="danger"` on a row.

HTML: `.penna-list` is the column (rows `space-2` apart). A row is `.penna-list-item[data-penna]` (`menu-item`, `sidebar.row`, `menu-item.tint`) with `role="option"`, `aria-selected="true"` or `data-state="selected"`, `aria-disabled="true"` or `data-state="disabled"`, `data-state="focused"` (or keyboard focus) for the focus ring, and `--penna-list-row` for its height. Inside, `.penna-list-content` holds `i.penna-list-icon[data-icon]`, `.penna-list-label` and `.penna-list-detail`, each text run in `.penna-text[data-penna-text]`. No script.

Status: matches, but for the parts of Touch rows above that are not built.

### Menu

A menu lists actions that open from a trigger: a button with a caret, a right click or a long press. To pick a value use a select; to switch views, a segmented control or tabs; for one action on its own, a button.

Kinds, by how it opens (`PopupMenu` is the one surface; the select opens it too):

| Kind | Call | Opens | Use |
|---|---|---|---|
| Dropdown | `Button::dropdown_menu(build)` | `space-4` below its button, on a press, or on Enter, Space or Down while the button has focus | a button's set of actions; give the button a caret |
| Context | `.context_menu(build)` on any element | at the pointer, on a right press or a long press; at the element's top-left on Shift+F10 or the Menu key while focus is in it | actions on the thing under the pointer |
| Submenu | `.submenu_with_icon(icon, label, window, cx, build)` | beside its row, when the row is highlighted | a group that would crowd the parent |

Any menu can carry a search field: `.search(placeholder, window, cx)` in its builder, and `set_query` to fill it from code. A select turns it on past 12 options. Use it for a long list of names, such as fonts or themes.

A menu can also carry scopes: `.scopes([MenuScope::new(label, keep), MenuScope::all(label)], picked)` in its builder, and `set_scope` to switch from code. Each scope is a segment; one built with `keep` shows only the rows whose label it keeps. Use two to five of them to narrow a long list to a kind, such as monospaced fonts. A select passes its `.scopes()` through.

Row kinds (`PopupMenuItem`): an action (`new`, with `.icon()` and `.action()`), a checked row (`.checked(true)`), a link (`link(label, href)`), custom content (`element`), a stepper (`stepper`), a submenu row, a section label (`label`) and a separator (`separator`).

Anatomy:

- Surface: the `popover` look (popover fill, `shadow-float`), the theme radius capped at 45% of its height. Its rows sit the row inset (`theme.row_inset`, 6) from every edge, `space-2` apart. At least 128 wide (8 rem), at most 500 (`max_w`); a scrollable menu stops at half the window's height, 450 at most (`max_h`).
- Row: `row` height, `control-md` (28, follows density), 44 under a coarse pointer; inset `space-8` at both sides; text `small`, `body` under a coarse pointer. Corners: a 28 control's (8.7 at the default, 12.6 at 100%), never rounder than the popover's (4 at a `radius` of 4).
- Icon slot: 12 (14 under a coarse pointer), `space-8` before the label, at 0.8 opacity. Every row keeps the slot when any row has an icon or a check, so labels line up; a checked row draws a check in it.
- Label: `small`, one line.
- Shortcut (optional): `small`, at the end, `space-12` after the label, in the row's ink at 0.8 opacity. It comes from the keymap binding of the row's action, never from the label.
- Mark (optional): a 12 chevron at the end of a submenu row, a 12 external-link mark after a link's label (`space-6`), at 0.8 opacity.
- Section label: `caption`, muted, as tall as its line, with the icon slot before it.
- Separator: a hairline rule, `space-2` above and below, from the rows' leading content edge to their trailing one (`space-8` in from the highlight at each side). A separator first, doubled or last is not drawn.
- Stepper: the label, then `space-10` later a field-ground group of three small flat ghost buttons (minus, the value at least 48 wide, plus).
- Search field (optional): a `md` field on the popover's ground, the row inset from the top and sides, with a 14 search icon in the muted ink before the text and a clear button once there is text. It stays put and the rows scroll under it, so the rows' scroll area is the menu's height less the field and one row inset. The menu keeps the width it opened at while the rows filter. When nothing matches, one faint `small` row says "No matches", `space-8` in.
- Scopes (optional): a `md` segmented control at full width (Segmented control) on the popover's ground, under the search field or on top without one, the row inset from the sides and from what is above it. It stays put like the field, so the rows' scroll area also loses its height and one row inset.

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
- Search: a menu with a search field opens with the field focused. Typing keeps the rows whose label holds the text, in any case, and hides section labels, separators and custom rows; the highlight goes to the first match, or to the checked row while the field is empty. Up, Down, Enter and Escape act on the menu as above; Left, Right and Space edit the text. Enter with no match does nothing.
- Scopes: the menu opens on the scope it was given. A press on a segment, or Tab and Shift-Tab from the field or the menu, switches scope and wraps at the ends; focus stays where it was. A scope with `keep` filters the rows like the query, and both apply together; the highlight follows the search rules, going to the checked row only while it is shown, and "No matches" shows when nothing is left.
- Pointer: pointing at a row highlights it; leaving clears it unless the last input was a key. A click runs the row on release and closes the whole chain; a press outside closes it. Stepper buttons step without closing.
- A row runs its handler, or else dispatches its action to the menu's action context.
- Motion: it fades in and rises 6 over 160ms, easing out; none under reduced motion.

Content: a row's label is a verb or a short verb phrase in sentence case with no closing period ("Split right", "Copy path"); options and checked rows are nouns ("Even horizontal", "Dark"). End it with an ellipsis when the row asks for more before it happens ("Rename…"). Never type a shortcut into a label: give the row its action and the key shows. Section labels name a group in a word or two. Group related rows and part groups with a separator.

Placement: a dropdown hangs `space-4` below its trigger, aligned to the trigger's start, and its trigger carries a caret. A context menu opens where the pointer is. Keep submenus to one level where you can.

HTML: built at runtime by `menu.js`: `pennaMenu.dropdown(trigger, items)`, `pennaMenu.context(region, items)` or `new pennaMenu.Menu(items)`, where an item is `{ label, icon, key, checked, disabled, link }` or `{ kind: "label" | "separator" | "submenu" | "stepper" | "note", ... }`. The markup: `.penna-popup[data-penna="popover"]` holds `.penna-popup-items[role="menu"]`, whose rows are `.penna-popup-row[data-penna="menu-item"][role="menuitem"]` with `data-kind` (`item`, `label`, `submenu`, `stepper`, `note`), `data-disabled`, and `data-selected` with `data-state="selected"` (`"hover"` on a submenu row) while highlighted. A row holds `.penna-popup-icon`, then `.penna-popup-main` with the `.penna-label` and a `.penna-label.penna-popup-key`; labels and submenu rows wrap theirs in `.penna-popup-line`; marks are `i.penna-popup-detail`, a link's label `.penna-popup-link`, a stepper's group `.penna-popup-stepper`. Separators are `.penna-popup-separator[role="separator"]`; an open submenu is a `.penna-popup.penna-popup-sub` inside its row. `@media (pointer: coarse)` switches rows to 44, `body` and 14 icons. A dropdown also opens on Enter, Space and Down, a context menu on the Menu key or Shift+F10 at the region's top-left, each with its first enabled row highlighted. A search field comes from `{ search: "Search fonts" }` in the options of `new Menu(items, options)` or `dropdown(trigger, items, options)`: the popup takes `data-search` and starts with `.penna-popup-search`, which holds a `label.penna-input` (search icon in `.penna-input-prefix`, a clear button in `.penna-input-suffix`); filtered rows take `hidden`, `.penna-popup-none` says "No matches", `menu.filter(text)` filters from code, and `--penna-popup-max` sets the menu's height (half the viewport, 450 at most, by default). `input.js` exposes `pennaInput.wire(root)` for fields added after load. Scopes come from `{ scopes: [{ label, keep }], scope }` in the same options, where `keep(label)` says whether a row stays and a scope without it keeps every row: the popup takes `data-scopes` and holds `.penna-popup-scopes` after the search, with a full-width `.penna-segmented` (`data-width="full"`) whose segments are `button:<label>`; Tab and Shift-Tab switch scope, `menu.setScope(index)` switches from code, and the page loads `segmented.css` and `segmented.js`.

Status: matches.

### Panel

A panel is a surface that holds other chrome and stays where the app puts it: a find bar, a palette, a picker, a toast, a raised card inside a view, an opaque cover. The kit paints it; the app sizes, places, opens and closes it. For a panel that opens from a button and closes on its own, use a popover; for a box in the flow of content, a card; for a strip across a pane, a bar; for an area things are arranged on, a well.

Looks (`PanelLook`, `Panel::new(id, look)`):

| Look | Recipe | Paint | Use |
|---|---|---|---|
| Popover | `popover` | glass and `shadow-float`, or the `popover` fill (the window `bg` two steps up) under a hairline in `divider` and the tight `shadow-float` when glass is off; the `radius` knob | menus, dropdowns, the find bar, the palette |
| Inline | `inline-panel` | as Popover | panels raised inside a view: failure alerts, plans |
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
- `.on_dismiss` runs whenever it closes.
- `.open(open)` hands the open state to the app, for a popover the app opens from its own keys or closes when a pick is made: the panel is open exactly while `open` is true, and opening or closing that way moves focus as above. The trigger, Escape and a press outside then change nothing themselves; they call `.on_toggle` with the state they ask for, and the app decides.

HTML: `.penna-popover` holds a `.penna-popover-trigger` (a button with `aria-haspopup`) and a `.penna-popover-panel` with `data-penna="popover"`, `tabindex="-1"` and `role="dialog"`. `data-open` marks it open; `data-anchor` takes `bottom` (opens above), `right` (aligns trailing edges) and `center` (centers on the trigger), alone or as `bottom right` and `bottom center`; `data-closable="false"` keeps it open on a press outside. `popover.js` sets `aria-expanded` and `data-state="current"` on the trigger, opens on Enter, Space and Down on the trigger, moves focus, closes on Escape and outside presses, fires `penna-open` and `penna-dismiss`, keeps the panel `8` inside the visual viewport on both axes, as natively, and moves it up when that shrinks, and exposes `pennaPopover.setOpen(popover, open)`. A panel whose trigger is out of view stays where it hangs, so a popover left open further down a page doesn't jump to the top of the screen. `popover.css` sizes the panel to its content, as natively.

Status: matches.

### Progress

A progress bar says how far a task has come: a download, an install, a long copy. When the task has a measure, the bar fills to it; when it has none yet (verifying, staging, waiting for a total), it sweeps. For a wait beside a word or in a control, use a spinner; for a setup with named steps, steps; to set a value, a slider.

Kinds:

| Kind | Call | Use |
|---|---|---|
| Determinate | `Progress::new(id, value)` | a value from 0 to 1, clamped; a value that isn't a number reads as 0 |
| Indeterminate | `Progress::indeterminate(id)` | a step with no measure |
| Line | `Progress::new(id, value).line()` | a web page loading: along the top edge of the page, 2 tall, no track, square ends, the fill in `accent`, filling to the page's estimate and gone once it loads (Browser on a phone) |

Anatomy:

- Track: a pill as thick as the slider's rail (4 at `xs` and `sm`, 6 from `md` up) in the neutral soft paint's pressed fill (`--penna-control`), as wide as its container. It has no hit box around it: the box is the track.
- Fill: from the start of the track to the value, a pill in the `accent` fill. At 0 it draws nothing.
- Segment (indeterminate): a pill in the `accent` fill a third of the track long, cut where it meets the track's ends, so it grows in at the start and shrinks out at the end.

Sizes: `md` by default (6), `sm` (4) under `small` text or in a dense row, through `.with_size()` (`Sizable`). It doesn't follow density: only the rail's two thicknesses exist.

States:

| State | Look | Notes |
|---|---|---|
| determinate | the fill up to the value | |
| changing | the fill eases to the new value in 220ms (`turn`) with an ease out, from wherever it is drawn | jumps with reduced motion |
| indeterminate | the segment crosses the track left to right at one steady speed, once every 2600ms: a 2000ms pass, then 600ms with the track empty, the live line's cycle (Live motion) | |
| reduced motion | indeterminate: the segment holds still in the middle third | |

Behavior: none. It takes no pointer or keyboard input and no focus. It is announced as a progress indicator named by `.label()` ("Progress" without one); a determinate bar adds its value, 0 to 1, read as a percentage ("42%"), and an indeterminate one has none.

Content: the bar never says what is under way or how much is left. A line beside it does, in sentence case: "Downloading 42%", "Verifying". The label names the task the same way.

Placement: under the text it measures, as wide as that text's column, never with a wrapper. In a toast, as `.content()`: it sits `gap-tight` 4 under the message, its ends on the text's; a toast built with `.closable(false)` keeps no room for the close button, so the text and the bar run to the inset on the right. A toast with a `.key()` that is already showing is replaced in place when pushed again (`push_toast`), and `replace_toast` updates it only while it shows, so a bar can follow a task without a toast the person closed coming back. A toast with no key and no `.content()` whose kind, title and message equal one that is still showing replaces that one in place and restarts its timer, so a repeated message never stacks a second copy. In a settings row, as a child of `SettingEntry`: it sits `gap-related` 8 under the heading, `inset-card` 12 from the card's sides, its ends on the title's.

HTML: `.penna-progress` with `role="progressbar"`, `aria-label`, `data-size` (`xsmall` to `touch`), and either `--penna-value` (a percentage) with `aria-valuenow`, `aria-valuemin="0"`, `aria-valuemax="1"` and `aria-valuetext`, or `data-indeterminate`. It holds one `.penna-progress-fill`. `progress.css` eases a change of `--penna-value` over 220ms, sweeps the fill under `data-indeterminate` and holds both still under `prefers-reduced-motion`. The line is `data-kind="line"`.

Status: matches; the line is HTML only.

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

- Fill: the look's fill, outline, shadow and highlight. Corners: a control's, `roundness` × the row's height; `.radius(px)` gives a round look that radius in place of the share, fixed at any height, so a row that wraps to several lines keeps the corner of a one-line row. It sets the fill and its press highlight.
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

HTML: `.penna-row[data-penna]` (the look's recipe), with no size, inset or text style of its own (the page sets them, as the caller does natively), and `data-state` (`hover`, `active`, `selected`, `current`, `focused`, `disabled`), `aria-selected`, `aria-disabled`, `data-tone`, and `data-still` for a row that is not interactive. It holds `<i data-icon>`, `.penna-row-label` and `.penna-row-aside`, each text run in `.penna-text[data-penna-text]`. A fill row is `.penna-row-fill[data-penna]` inside the caller's row, `data-round` for a radius of half the row (the story's 24 on its 48 row), drawn as a full round. `data-state="selected focused"` is a selected row with the ring. `.penna-item.penna-list-item` draws a list item with a Row's focus ring. `tab.js` marks keyboard focus and `kit.js` sizes the ring (`--penna-ring-r`).

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
- Fade (`top_fade`, `bottom_fade`): the content at that edge fades into the ground under it, clear to all ground at 70% of its height. It is as tall as the caller asks but never more than twice the content past the edge, so it grows in as the area leaves its end. The settings page fades both edges by 32 into the window's ground (`Theme::ground()`), the workspace sidebar its bottom by 40. Over an opaque ground it is a strip of the ground painted over the content. Over a see-through ground it is `Window::paint_fade`, which mixes what is drawn there toward the ground, alpha included, so the desktop shows through the faded edge as it does through the window and no band forms; where the window can't read its backdrop it falls back to the strip.
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
- Segments: one per option, side by side with no gap, each as wide as its label plus the rung's field inset and `space-2` at both sides (8, 8, 10, 12, 14, 14 from `xs` to `touch`), as tall as the track less `space-2` and the edge on each side. With `.full_width()` the track fills its container and the segments share its width equally, each growing from zero and never narrower than its label.
- Labels: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`). The picked one takes the selected ink, the others the quiet button's ink.

Sizes: every rung through `.with_size()` (`Sizable`), `md` by default (follows density). `xs` and `sm` in toolbars and dense rows, `touch` under a coarse pointer. Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the track, labels in the quiet ink | |
| hover | the quiet button's hover fill behind the segment, the pill's height and radius, and its label in the hover ink | not on the picked segment |
| pressed | the quiet button's active fill behind the segment, while held | as hover |
| selected | the pill under it, its label in the selected ink | exactly one segment |
| focused | the focus ring, 1.5 outside the track; the track takes the field's focused fill | from the keyboard only, never from a click |
| disabled | the track takes the field's disabled paint; the pill keeps its selected paint at half strength (fill and edge, no shadow), and every label takes its ink at half strength | presses, keys and hover are ignored, and it leaves the focus order; the pick still shows |

Behavior: a click on a segment picks it and calls `on_select` with its index; the caller sets `selected`. With `on_select` the control is one tab stop: Left and Up pick the previous segment, Right and Down the next, Home and End the first and last, each through `on_select`, and they stop at the ends. A click picks without focusing. The pill slides to the new segment in 220ms (`turn`) with an ease out, and holds still with reduced motion. Under a coarse pointer each segment's hit area grows to the touch slop.

Content: one or two words per segment, sentence case, no period ("System", "Light", "Dark"). Keep the labels about the same length, since each segment is as wide as its own.

Placement: at the trailing end of a settings row (Appearance's mode and density), above content to switch its view (the docs' Rust and HTML tabs), or at full width under a menu's search field to scope its rows (Menu).

HTML: `.penna-segmented` (`data-penna="field"`, `role="radiogroup"`, `data-value` the picked index, `data-size` (`xsmall` to `touch`), `aria-disabled="true"`) holding `.penna-segments` (`data-penna="button.quiet"`, `data-state="current"`), which holds a `.penna-segment-pill` and a `.penna-segment` (`role="button"`) per option with a `.penna-label` inside. `.penna-segment` takes `:hover` or `data-state="hover"` and `:active` or `data-state="active"`, drawn in the root `--penna-quiet-hover` and `--penna-quiet-pressed` (the quiet button's fills) with the pill's radius (`--penna-pill-r`, which `segmented.js` sets). `segmented.js` makes the track a tab stop, places the pill, sets `aria-selected`, moves the pick on click and on the keys above, fires `change` with `{ index }`, and sizes the ring (`--penna-ring-r`) that `:focus-visible` or `data-state="focused"` draws. With `aria-disabled="true"` it leaves the track out of the tab order, ignores clicks and keys, and marks `.penna-segments` disabled so the tokens give the labels their disabled ink. `data-width="full"` fills the container with the segments sharing its width. `pennaSegmented.wire(root)` sets up controls added after load, and `pennaSegmented.set(control, index)` moves the pick from code without firing `change`.

Status: matches.

### Select

A select picks one value from a list that opens under it. For two to five short options always in view, use a segmented control; for on or off, a switch; to type a value, an input; to run a command, a button with a caret.

Kinds: one. `Select::new(&state)` draws a `SelectState`, which holds the options (any `SelectDelegate`, such as a `Vec`) and the picked one. Under a coarse pointer the list opens as a bottom sheet instead of a menu.

Anatomy, left to right, all from the select's rung of the control ladder (Size):

- Trigger: a field (the field role's fill, a hairline edge of at least 1px, radius the theme radius capped at 45% of the height), height `control_px`, the field inset at both sides (6, 6, 8, 10, 12, 12 from `xs` to `touch`). It is as wide as its value unless the caller sets a width.
- Value: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`), cut with an ellipsis when it doesn't fit. With nothing picked it shows the placeholder (or "Select") in the field's faint ink.
- Chevron: `chevron-down` at the end, `icon_gap` after the value (4, 4, 6, 6, 8, 8), in the muted ink, a rung smaller than the trigger from `md` up: 12, 12, 12, 14, 16, 16.
- Menu: a popover menu `space-4` under the trigger, its trailing edge on the trigger's, at least as wide as the trigger and scrolling past its height (`.menu_max_h()`). Its rows are menu rows (`small`, 12 icons) with a check before the picked one. An empty list shows a 28 inbox icon in the faint muted ink, `space-24` above and below, or the caller's `.empty()`. Past 12 options it carries the menu's search field (Menu), with the placeholder from `.search_placeholder()` ("Search" by default); `.searchable(bool)` forces it on or off. With `.scopes()` it carries the menu's scopes (Menu) and opens on the first scope that keeps the picked option's title, or the first scope.
- Sheet (coarse pointer): a bottom sheet titled by `.title()` or the placeholder, with a row per option, a search field when the select is searchable, as above, and under it a full-width `touch` segmented control when it has scopes, which filters the rows the same way.

Sizes: `md` by default (follows density). Same rungs and uses as a field. Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the field's fill and edge | |
| hover | the fill's hover step | not when disabled |
| focused | the field's focused fill | also while the menu is open |
| open | the focused look, with the menu under it and the picked row highlighted | |
| disabled | the field's disabled paint, faint value | leaves the focus order and never opens |
| error | `.tone(Tone::Danger)` paints the trigger in the danger tone, like a field: its edge only | its Field shows the message with an icon (Field) |

Behavior: a press on the trigger opens the menu, and a second press closes it. While focused, Enter, Space, Up or Down open it. In the menu, Up and Down move, Enter picks, Escape closes it and returns focus to the trigger. Picking a row closes the menu and emits `SelectEvent::Confirm`. A searchable menu opens with its field focused and the picked row highlighted, and filters as the menu's search does. Under a coarse pointer the trigger's hit area grows to the touch slop.

Content: options are short nouns in sentence case ("Rust", "Floating"). The placeholder names what is picked ("Language"), with no closing period.

Placement: like a field: in a Field in a form, or at the end of a settings row whose title names it.

HTML: `div.penna-select` with `data-options` (a comma list), `data-value` (the picked index), `data-placeholder`, `data-size` (`xsmall` to `touch`), `data-tone="danger"` for an error, `aria-disabled="true"`, and `data-open` (with `data-pinned` to stay open without focus). `select.js` builds `.penna-select-trigger` (`data-penna="field"`, carrying the select's `data-tone`) holding `.penna-select-title` (`data-placeholder` while nothing is picked) and `<i data-icon="chevron-down">`, and the menu `.penna-menu.penna-select-menu` (`data-penna="popover"`) of `menu-item` rows, each with a `.penna-select-check`. It sets `role="combobox"`, answers the same keys (Tab also closes), closes on a press outside and fires `change` with `{ index, value }`. Past 12 options, or with `data-searchable` (`"false"` turns it off), the menu takes `data-searchable` and holds `.penna-select-search` (a `label.penna-input` with the search icon, its placeholder from `data-search-placeholder`) over `.penna-select-rows`; filtered rows take `hidden` and `.penna-select-none` says "No matches". `data-scopes` is a JSON list of `{ "label", "keep": [option names] }`, where a scope without `keep` keeps every option: the menu takes `data-scoped` and holds `.penna-select-scopes`, a full-width segmented control, after the search; it opens on the first scope that keeps the picked option, `data-scope` on the select holds the shown one, and Tab and Shift-Tab switch scope instead of closing. The page loads `segmented.css` and `segmented.js`.

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
| Rail | `StepRail::new(id, label, steps, picked, on_change)` | the same, as one thick rail to drag, where the control has a row to itself: an effort under a list of models in a popover. `.open_start()` when the first step means "not set" |

Anatomy, continuous:

- Box: the rung's `control_px` tall (28 at `md`, which follows density; 44 under a coarse pointer at any size), half the knob inside at each side. The whole box takes the press.
- Rail: a pill, 4 tall at `xs` and `sm` and 6 from `md` up. Up to the value it takes the `accent` fill, past it the neutral soft paint's pressed fill (`--penna-control`); `.track(colors)` paints a gradient instead, as on the hue slider.
- Knob: a circle (Sizes; 28 under a coarse pointer) centered on the value, whichever of `bg` and `fg` is lighter (`--penna-knob`), with `shadow-contact`.
- Under a coarse pointer, unless `.inline()`: a field at the slider's rung showing the value and a 72 wide preview rail (4 tall, 10 knob); a tap opens a bottom sheet titled by the label, with the value in `title` over a full slider.

Anatomy, stepped (all text `small`, `caption` at `xs` and `sm`; the three parts `space-8` apart):

- Label: before the pills, in the muted ink.
- Pills: one per step, `space-4` apart, sharing the width, each at most 24 wide. Each is a bar as thick as the continuous rail (4 at `xs` and `sm`, 6 from `md` up) in a hit box the rung's `control_px` tall. Steps up to the picked one take the `accent` fill, the rest `--penna-control`.
- Value: the picked step's name in the ink, in a 52 wide column after the pills.

Anatomy, rail:

- Head: 26 tall, the label in `caption`, strong, in the muted ink at the left, the picked step's name in `small` in the ink at the right. While a drag is held the name follows the knob.
- Rail: a pill 26 tall across the full width, in `--penna-control`. From its left end to the picked step it takes the `accent` fill, a pill of its own that ends 13 past the step's center, under the knob. A dot 4 across marks each step, the first 13 in from the left end and the last 13 in from the right, in the muted ink past the pick and in the accent's ink under the fill. With `.open_start()` the first step is a ring 6 across with a 1 wide line instead of a dot, and picking it leaves the rail unfilled.
- Knob: a circle 30 across centered on the picked step, 2 past the rail above and below and at either end, `--penna-knob` with `shadow-contact`. With nothing picked there is no knob and no fill.

Sizes: every rung through `.with_size()` (`Sizable`), `md` by default (follows density). `xs` and `sm` in toolbars and dense rows, `touch` under a coarse pointer. Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the track, labels in the quiet ink | |
| hover | the quiet button's hover fill behind the segment, the pill's height and radius, and its label in the hover ink | not on the picked segment |
| pressed | the quiet button's active fill behind the segment, while held | as hover |
| selected | the pill under it, its label in the selected ink | exactly one segment |
| focused | the focus ring, 1.5 outside the track; the track takes the field's focused fill | from the keyboard only, never from a click |
| disabled | the track takes the field's disabled paint; the pill keeps its selected paint at half strength (fill and edge, no shadow), and every label takes its ink at half strength | presses, keys and hover are ignored, and it leaves the focus order; the pick still shows |

Behavior: a click on a segment picks it and calls `on_select` with its index; the caller sets `selected`. With `on_select` the control is one tab stop: Left and Up pick the previous segment, Right and Down the next, Home and End the first and last, each through `on_select`, and they stop at the ends. A click picks without focusing. The pill slides to the new segment in 220ms (`turn`) with an ease out, and holds still with reduced motion. Under a coarse pointer each segment's hit area grows to the touch slop.

Content: one or two words per segment, sentence case, no period ("System", "Light", "Dark"). Keep the labels about the same length, since each segment is as wide as its own.

Placement: at the trailing end of a settings row (Appearance's mode and density), above content to switch its view (the docs' Rust and HTML tabs), or at full width under a menu's search field to scope its rows (Menu).

HTML: `.penna-segmented` (`data-penna="field"`, `role="radiogroup"`, `data-value` the picked index, `data-size` (`xsmall` to `touch`), `aria-disabled="true"`) holding `.penna-segments` (`data-penna="button.quiet"`, `data-state="current"`), which holds a `.penna-segment-pill` and a `.penna-segment` (`role="button"`) per option with a `.penna-label` inside. `.penna-segment` takes `:hover` or `data-state="hover"` and `:active` or `data-state="active"`, drawn in the root `--penna-quiet-hover` and `--penna-quiet-pressed` (the quiet button's fills) with the pill's radius (`--penna-pill-r`, which `segmented.js` sets). `segmented.js` makes the track a tab stop, places the pill, sets `aria-selected`, moves the pick on click and on the keys above, fires `change` with `{ index }`, and sizes the ring (`--penna-ring-r`) that `:focus-visible` or `data-state="focused"` draws. With `aria-disabled="true"` it leaves the track out of the tab order, ignores clicks and keys, and marks `.penna-segments` disabled so the tokens give the labels their disabled ink. `data-width="full"` fills the container with the segments sharing its width. `pennaSegmented.wire(root)` sets up controls added after load, and `pennaSegmented.set(control, index)` moves the pick from code without firing `change`.

Status: matches.

### Select

A select picks one value from a list that opens under it. For two to five short options always in view, use a segmented control; for on or off, a switch; to type a value, an input; to run a command, a button with a caret.

Kinds: one. `Select::new(&state)` draws a `SelectState`, which holds the options (any `SelectDelegate`, such as a `Vec`) and the picked one. Under a coarse pointer the list opens as a bottom sheet instead of a menu.

Anatomy, left to right, all from the select's rung of the control ladder (Size):

- Trigger: a field (the field role's fill, a hairline edge of at least 1px, radius the theme radius capped at 45% of the height), height `control_px`, the field inset at both sides (6, 6, 8, 10, 12, 12 from `xs` to `touch`). It is as wide as its value unless the caller sets a width.
- Value: the rung's text style (`caption`, `small`, `body`, `body`, `lead`, `touch-body`), cut with an ellipsis when it doesn't fit. With nothing picked it shows the placeholder (or "Select") in the field's faint ink.
- Chevron: `chevron-down` at the end, `icon_gap` after the value (4, 4, 6, 6, 8, 8), in the muted ink, a rung smaller than the trigger from `md` up: 12, 12, 12, 14, 16, 16.
- Menu: a popover menu `space-4` under the trigger, its trailing edge on the trigger's, at least as wide as the trigger and scrolling past its height (`.menu_max_h()`). Its rows are menu rows (`small`, 12 icons) with a check before the picked one. An empty list shows a 28 inbox icon in the faint muted ink, `space-24` above and below, or the caller's `.empty()`. Past 12 options it carries the menu's search field (Menu), with the placeholder from `.search_placeholder()` ("Search" by default); `.searchable(bool)` forces it on or off. With `.scopes()` it carries the menu's scopes (Menu) and opens on the first scope that keeps the picked option's title, or the first scope.
- Sheet (coarse pointer): a bottom sheet titled by `.title()` or the placeholder, with a row per option, a search field when the select is searchable, as above, and under it a full-width `touch` segmented control when it has scopes, which filters the rows the same way.

Sizes: `md` by default (follows density). Same rungs and uses as a field. Every control in a row has the same size (Rules 6).

States:

| State | Look | Notes |
|---|---|---|
| rest | the field's fill and edge | |
| hover | the fill's hover step | not when disabled |
| focused | the field's focused fill | also while the menu is open |
| open | the focused look, with the menu under it and the picked row highlighted | |
| disabled | the field's disabled paint, faint value | leaves the focus order and never opens |
| error | `.tone(Tone::Danger)` paints the trigger in the danger tone, like a field: its edge only | its Field shows the message with an icon (Field) |

Behavior: a press on the trigger opens the menu, and a second press closes it. While focused, Enter, Space, Up or Down open it. In the menu, Up and Down move, Enter picks, Escape closes it and returns focus to the trigger. Picking a row closes the menu and emits `SelectEvent::Confirm`. A searchable menu opens with its field focused and the picked row highlighted, and filters as the menu's search does. Under a coarse pointer the trigger's hit area grows to the touch slop.

Content: options are short nouns in sentence case ("Rust", "Floating"). The placeholder names what is picked ("Language"), with no closing period.

Placement: like a field: in a Field in a form, or at the end of a settings row whose title names it.

HTML: `div.penna-select` with `data-options` (a comma list), `data-value` (the picked index), `data-placeholder`, `data-size` (`xsmall` to `touch`), `data-tone="danger"` for an error, `aria-disabled="true"`, and `data-open` (with `data-pinned` to stay open without focus). `select.js` builds `.penna-select-trigger` (`data-penna="field"`, carrying the select's `data-tone`) holding `.penna-select-title` (`data-placeholder` while nothing is picked) and `<i data-icon="chevron-down">`, and the menu `.penna-menu.penna-select-menu` (`data-penna="popover"`) of `menu-item` rows, each with a `.penna-select-check`. It sets `role="combobox"`, answers the same keys (Tab also closes), closes on a press outside and fires `change` with `{ index, value }`. Past 12 options, or with `data-searchable` (`"false"` turns it off), the menu takes `data-searchable` and holds `.penna-select-search` (a `label.penna-input` with the search icon, its placeholder from `data-search-placeholder`) over `.penna-select-rows`; filtered rows take `hidden` and `.penna-select-none` says "No matches". `data-scopes` is a JSON list of `{ "label", "keep": [option names] }`, where a scope without `keep` keeps every option: the menu takes `data-scoped` and holds `.penna-select-scopes`, a full-width segmented control, after the search; it opens on the first scope that keeps the picked option, `data-scope` on the select holds the shown one, and Tab and Shift-Tab switch scope instead of closing. The page loads `segmented.css` and `segmented.js`.

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
| Rail | `StepRail::new(id, label, steps, picked, on_change)` | the same, as one thick rail to drag, where the control has a row to itself: an effort under a list of models in a popover. `.open_start()` when the first step means "not set" |

Anatomy, continuous:

- Box: the rung's `control_px` tall (28 at `md`, which follows density; 44 under a coarse pointer at any size), half the knob inside at each side. The whole box takes the press.
- Rail: a pill, 4 tall at `xs` and `sm` and 6 from `md` up. Up to the value it takes the `accent` fill, past it the neutral soft paint's pressed fill (`--penna-control`); `.track(colors)` paints a gradient instead, as on the hue slider.
- Knob: a circle (Sizes; 28 under a coarse pointer) centered on the value, whichever of `bg` and `fg` is lighter (`--penna-knob`), with `shadow-contact`.
- Under a coarse pointer, unless `.inline()`: a field at the slider's rung showing the value and a 72 wide preview rail (4 tall, 10 knob); a tap opens a bottom sheet titled by the label, with the value in `title` over a full slider.

Anatomy, stepped (all text `small`, `caption` at `xs` and `sm`; the three parts `space-8` apart):

- Label: before the pills, in the muted ink.
- Pills: one per step, `space-4` apart, sharing the width, each at most 24 wide. Each is a bar as thick as the continuous rail (4 at `xs` and `sm`, 6 from `md` up) in a hit box the rung's `control_px` tall. Steps up to the picked one take the `accent` fill, the rest `--penna-control`.
- Value: the picked step's name in the ink, in a 52 wide column after the pills.

Anatomy, rail:

- Rows: one per step, the last step on top, each 32 tall with its name in `small`, `space-10` from the left. The picked row takes the menu row's selected fill, the rest its hover on the pointer.
- Lane: 32 wide, `space-8` from the right edge, over the rows. In it a rail 12 wide, a pill from the first row's center to the last's, in `--penna-control`; up to the picked row it takes the `accent` fill. A dot 4 across marks each step, in the faint ink past the pick and in the accent's ink under it.
- Knob: a circle 22 across on the picked row's center, `--penna-knob` with `shadow-contact`, an 8 dot of the accent in it. With nothing picked there is no knob and no fill.

Sizes: every rung through `.with_size()` (`Sizable`) on the continuous and stepped kinds, `md` by default (follows density); the rail has one size:

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

Behavior, rail: a press anywhere on the rail moves the knob to the nearest step and focuses it, and a drag follows, a step at a time. `on_change` fires once, on release, with the step the knob was let go on, and not at all when that is the picked one. Up and Right step up, Down and Left step down, Home and End go to the ends; the ring is on the knob. It is drawn the same under a coarse pointer. Disabled, the rail, fill, dots and knob take half strength, the head takes `fg-faint`, and presses and keys are ignored.

Content: the label names the setting in sentence case ("Scrollback", "Opacity"). Values carry their unit ("10k", "70%"); format a continuous value with `.display()`.

Placement: in a settings row, between its title and its value; a stepped slider fills the row it sits in.

HTML: the stepped slider is `.penna-steps` with `data-label`, `data-steps` (a comma list), `data-value` (the picked index), `data-size` (`xsmall` to `touch`), `aria-disabled="true"`, and `data-focused` to draw focus. `slider.js` builds `.penna-steps-label`, `.penna-steps-pills` of `.penna-step` (`role="button"`, `data-filled`, `data-picked`) and `.penna-steps-value`, sets `role="slider"`, answers the same keys and fires `change` with `{ index, value }`. The continuous slider is `.penna-slider` with `data-min`, `data-max` (0 and 100 when left out), `data-step`, `data-value`, `data-size`, `aria-disabled="true"` and `aria-label` for its name, holding a `.penna-rail` with `.penna-fill` and `.penna-knob`; `slider.js` sets `role="slider"` and `--penna-value` (a percentage), moves the knob on a press and a drag, answers the same keys and fires `change` with `{ value }`. Both draw the ring on `:focus-visible` or `data-focused`, which the script sets when a key moves the slider and clears on a press or blur. A disabled one has no `tabindex` and ignores presses and keys. `slider.css` draws both. The gradient rail of `.track(colors)` has no HTML attribute; the Appearance block's hue slider draws its own.

Status: the continuous and stepped kinds match. The rail is drawn in ala only: it has no HTML yet, and no storybook section or docs figure.

### Spinner

A spinner says something is under way and how long it takes is unknown: a busy button, a field that is checking, a line saying what is loading. When the wait has known steps, use steps.

Kinds: one, the `loader` icon turning. `.color()` tints it; without one it takes the ink of the text around it. Muted beside muted text; a status color only beside a word in that status (Rules 8).

Anatomy:

- Icon: `loader`, an open ring (a 293° arc with its gap at the upper right), at the rung's `icon_px` (12 at `xs` and `sm`, 14 at `md`, 16 from `lg` up); `Size(px)` draws it that many pixels across.
- Turn: one full turn every 900ms at a constant speed, repeating (Live motion).

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
| Window | `tab.window` | window pills in the title bar, in a strip; selected as glass (Look) |
| Page | `tab.page` | a browser's page tabs, in a strip; paints as Window |
| Tray | `tab.tray` | chips in the tray over a composer |
| Title | `button.title` | a pane's title in its header, quiet, with a muted path |

Anatomy:

- Container: the rung's height, `control-lg` 32 by default; the look's fill, outline and corners (its radius, capped at 45% of the height).
- Inset, gap, icon and label are the caller's. A close button sits inside the tab (`Button::inside`, Button), the tab's trailing inset `INSIDE_GAP` 4 so the button is as far from the end as from the top. The story and `tab.css` use `space-10` at the sides and `space-6` gaps, a 12 icon, a `small` label and an optional `caption` aside in muted ink; a title uses `space-8` and a `small` muted path.
- Focus ring: 1.5px, outside the container, with its own radius.
- Strip (`TabStrip::new(id, look)`, `tab.strip`): window tabs and a browser's page tabs each sit in one track, a wash of its ground with no edge, `space-2` inside it and `space-2` apart. `.with_size()` sets its tabs' height (28 in the title bar and a browser header), and the strip is that plus `space-2` above and below. Its corner is the tabs' corner plus `space-2`, so the picked tab's glass sits concentric in it: square at a `radius` of 0, a capsule when the tabs are. A strip holding one tab draws no track and no inset, and the tab fills it (`.h_full()`), so a lone window tab is 32 in the title bar, as tall as the buttons beside it, and the strip keeps its height when a second tab opens. `TabStrip::ground(on, tabs, cx)` is the ground the tabs inside paint on: the strip's, or `on` for a lone tab.

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

HTML: `.penna-tab[role="tab"][data-penna]` (`tab.window`, `tab.page`, `tab.tray`, `button.title`) inside a `[role="tablist"]`, with `data-size` (`xsmall` to `touch`; large when absent), `data-state`, `aria-selected`, `aria-disabled`, `data-still` and `tabindex`. It holds `<i data-icon>`, `.penna-tab-label`, `.penna-tab-aside` or `.penna-tab-path`, each text run in `.penna-text[data-penna-text]`. A title is `.penna-tab.penna-title-tab` in a `.penna-pane-header[data-penna="bar.pane"]`. A strip is `.penna-tab-strip[data-penna="tab.strip"][role="tablist"]` around the tabs, its corner set by `kit.js` from `--penna-around`; with one tab (`:only-child`) it draws no fill and no inset, and the tab grows by `space-2` above and below to fill it.

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
- Callouts: an alert (`> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`) or a `:::name title` block is a `card.activity` card washed in its tone (note and info the accent, tip success, important primary, warning warning, caution, danger and error danger, any other name neutral), `space-12` (12) by `space-8` (8) inside. Its first line is the tone's icon as tall as the font, centered on the capitals, `space-8` (8) before the title at 600: the alert's own title or its kind, the text after a directive's name or the name itself; its blocks follow `space-4` (4) below. HTML has no callout yet.
- Code blocks: a `card.code` card, `inset-card` (12) all round, the code face at the reading size with the theme's syntax colors; a diff's added and removed lines take `success` and `danger`. With `code_block_actions` the top inset grows to 2 × `space-8` + `control-md` (44), and the actions sit `space-8` (8) from the top and sides, at the end.
- Tables: hairline rules between rows and between columns, cells `space-4` (4) by `space-8` (8), columns sized to their content from 48 to 480 (Size, Width). By default the columns share the container's width in that proportion, none narrower than 64, and their text wraps. With `overflow_x` scroll on `TextViewStyle::table` (`data-scroll`) they keep their widths, a narrower table grows in proportion to fill its container, and a wider one scrolls sideways in its own area. Text `read-body` (`read-small` is planned).
- Rule: a hairline in the divider color.
- Footnotes: a reference is its number in superscript digits, in the order references first appear. The notes gather after the last block, `paragraph_gap` (12) below it and `space-2` (2) apart: `caption` in `fg-muted`, the number and a period, then the note `space-4` (4) after it.

Rhythm: blocks sit `space-12` (12) apart (`TextViewStyle::paragraph_gap`). A heading has `space-24` (24) above it at h1 and h2, `space-16` (16) at h3 to h6, and `space-8` (8) below. The first block has nothing above it and the last nothing below.

States:

| State | Look | Notes |
|---|---|---|
| rest | the text | |
| selected | an `fg` wash (28%) behind the selected runs, across blocks | `.selectable(true)` |
| streaming | new text fades in under a veil over 120 to 220ms (appear to turn), following the pace it arrives at | none with reduced motion |

Behavior: with `.selectable(true)` a drag selects across blocks, a double click a word and a triple click a paragraph; while it has focus, ⌘C or Ctrl+C copies and ⌘A or Ctrl+A selects all. A drag past the top or bottom scrolls. `.scrollable(true)` fills its container and scrolls with a vertical scrollbar. Links open with the system unless `.links(..)` handles the click; a `TextLinks` can also restyle a link or give it a tooltip. `markdown_extensions` adds block kinds and `code_block_body` replaces a code block's body.

Content: markdown, parsed with GFM tables, task lists, strikethrough, autolinks and footnotes, plus alerts, `:::` blocks and math in code spans. `MarkdownExtensions::inline_links` turns spans of a text run, or a whole code span, into links after parsing, so nothing inside a code block or a code span's text ever matches. Headings give structure, never emphasis; bold carries emphasis inside a line. Keep the measure; don't widen it.

Placement: inside a pane or page that gives the inset, `inset-page` (32) around a document page.

HTML: `.penna-md` (`data-selectable` to make it focusable and select all with ⌘A; `data-selected`; `data-streaming` with a `template[data-tail]`, whose blocks `text.js` appends under the same veil, one 220 fade on the native curve as for a first chunk; `data-text="inherit"` for the inherited kind, which takes its container's text style and drops the measure) holding plain `p`, `h1` to `h6`, `strong`, `em`, `s`, `a` and `code[data-penna="card.code"]`; quotes as `.md-quote > blockquote`; lists as `ul` or `ol` > `li` > `.md-item` > `.md-prefix` + `.md-body`, a task's box as `.md-box > .md-check[role="checkbox"][aria-checked]`; `hr`; footnotes as `.md-footnotes` > `.md-footnote` > a `span` number + `p`; code as `.md-code > pre[data-penna="card.code"] > code` with `.tk-*` runs, and `data-actions` and `.md-actions` for actions; tables as `.md-table` (`data-scroll`, with a `.md-scroll`) > `.md-figure[data-penna="card.figure"]` > `.md-row` > `.md-cell[data-align]`. `text.js` sizes table columns, rounds inline code and check corners, and places baselines as ala does.

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

Phone tile (Phone): the cell of every screen of things on a phone (Phone, Layout). Two contents: a machine on the machines overview, a window inside a machine.

- Container: the `phone.card` look, `gap-related` 8 apart over `inset-pane` 16 (`BENTO_GAP`, `BENTO_INSET`), inset `inset-card` 12, the phone container corner, twice `radius` (Phone, Corners). The whole tile is the target; press and hold opens its sheet (Phone). No button sits inside a tile.
- Mark and name: a status mark (Phone, Status marks), a tone dot 8 or a 16 spinner, then the name in strong `touch-body` (500), `space-8` apart, on one line ending in an ellipsis.

Machine content: square, two to a row. Under the name, the state in `touch-small`, one line, its word in the mark's ink; at the bottom, lines in `touch-small` muted saying what it holds or how it is reached. Ink follows the ground: a tile's muted lines (a neutral state word, the foot lines, the add tile's label) take the muted ink of the tile's own fill, as rows in a group do, natively and in HTML (`--penna-muted`, which the `phone.card` and `phone.add` roles set).

| State | Mark, word | A tap |
|---|---|---|
| connected | `success` dot, "Connected" | zooms into the machine |
| waiting | `warning` dot, "1 waiting for you" | zooms into the machine |
| connecting | spinner, "Connecting", how it is reached | zooms into the connecting card, where Cancel is the one action (Zoom card) |
| offline | faint dot, "Not connected", when it was last seen | reconnects |
| pressed | the bento press (`control` over the tile) | |
| add | the `phone.add` look with a 20 `plus` over "Add machine" in `touch-body` muted, `space-6` apart, centered; the same in a wide bento cell | zooms into the add card (Zoom card) |

Window content: a header line, then the preview. The preview is the tile; text is one line.

- Size: a bento grid of four columns over the body's width, the cells `gap-related` 8 apart, packed in order into the first spot that fits (`bento_pack`). A window with one or two panes takes 2 by 2 cells, half the width and square; three panes 4 by 2; four or more 4 by 3 (`window_span`). Over a 370 body a cell is 86.5, so the three sizes are 181 by 181, 370 by 181 and 370 by 275.5.
- Header: the mark, then the title on one line. Nothing else: no window number, folder, branch, age or pane glyphs.
- State: only when the window asks for you or broke, a second line under the title in `touch-small`, `warning` or `danger` ink ("Needs approval", "Has a question", "Failed", "Exit 1"). Calm states are the mark alone (Phone, Status marks, which says why this narrows rule 5).
- Preview: `gap-related` 8 under the header, filling the rest of the tile. It is the window's own split: each pane a mini pane at its share of the window, 4 apart, on the ground's fill, its corner concentric (the tile's less 12). A mini pane has no label, glyph or ring of its own. The preview is a picture, so the floor of rule 6 does not apply to it; the host is its only word, and it keeps the floor.

| Pane | Preview |
|---|---|
| terminal | the screen's top lines as live text, mono 5 on a 6 line, muted, with the few colors the terminal gives kept; clipped at the pane's edge |
| browser | the page's host in `body` 13, muted, one line, `space-6` from the top and `space-8` from the sides, over a quiet stand-in of a page: a bar, a few lines and a block in a faint fill |
| agent chat | the last exchange, held to the bottom: your message as a bubble block on the trailing side, then the reply as three to five lines in a faint fill |

The page and the chat are stand-ins, the same picture for every page and chat: the page a 6 bar at 45% in `fg` at 22% over two 4 lines at 90% and 70% in `fg` at 14% and a block 12 to 48 tall in `fg` at 8%, `space-8` in and `space-4` apart; the chat a bubble 64% wide in `fg` at 10% holding lines at 100% and 55% in `fg` at 22%, over four reply lines at 100%, 85%, 95% and 60% in `fg` at 14%. A terminal's text sits `space-6` from the pane's top and sides. A preview stays calm at half width: a terminal shows what fits and no more, and a chat never more than one exchange, so a screen of tiles reads as pictures rather than noise.

| State | Mark, line | A tap |
|---|---|---|
| waiting for you | `warning` dot, "Needs approval" or "Has a question" | zooms into the pane that asks |
| failed | `danger` dot, "Failed" or the exit ("Exit 1") | zooms into the pane that failed |
| working | spinner | zooms into the pane at work |
| done, not yet seen | `fg` dot | zooms into the pane that finished |
| idle | none | zooms into the window's active pane |
| pressed | the bento press over the whole tile | |

The zoom grows the preview of the pane it opens into that pane (Zoom card). Mini panes are not targets: a pane's share of a half tile is often under 44, and a window is entered as one.

A bento grid has no add tile and its session's header no button: windows are made from the pill's menu (Blocks, Pill). A machine with no sessions shows one add tile alone, which opens that menu.

Nothing on a screen of tiles is drawn as a ghost: machines not yet added are rows in the add card.

Status for the phone tile: built in `src/blocks/compact/tile.rs`: `PhoneTile` (`:46`) with machine content (`.mark`, `.state`, `.foot`) and window content (`.pane`, `.panes` of `MiniPane`, `:254`: `terminal`, `browser`, `chat`, `blank`), the add tile (`PhoneTile::add`, `:75`), `window_span` (`:30`), the phone corner and the concentric mini pane (`mini_radius`, `:39`), and the bento press over the whole tile; the grid and its packing are `bento_grid` and `bento_pack` (`src/blocks/compact/bento.rs:52`, `:21`). The whole tile is a button named by its title and its words, and `PhoneTile::on_hold` takes the hold. zz-ui maps its states to marks and words (`host_mark`, `host_state`, `window_mark`); a machine whose connect failed says "Failed" in `danger` ink with the reason as its foot line. A window tile's hold opens its sheet in zz. Not built: a machine's last seen (zz does not keep it, so an offline tile has no foot line); the tap into the pane that asks (zz opens the window's active pane).

HTML: a machine is `.penna-machine-tile[data-penna="phone.card"]` in a `.penna-machine-grid` (`data-penna="phone.add"` and `data-add` for the add tile), with `.penna-machine-name`, `.penna-machine-state` (`data-tone` `warning` or `danger`), `.penna-machine-foot` and a `.penna-press` (`data-state="active"` for the pressed state). A window is `.penna-bento-tile.penna-window-tile[data-penna="phone.card"]` placed in a `.penna-bento` by `--column`, `--row`, `--columns` and `--span` (`--rows` on the grid, `--penna-bento-width` for the body's width), holding the same `.penna-machine-name` and `.penna-machine-state`, then `.penna-window-preview` with a `.penna-window-pane[data-penna="tile.mini"]` per pane placed by `--x`, `--y`, `--w` and `--h` (fractions of the window), and a `.penna-press`. In a pane: `.penna-window-term` (a `pre`, `data-ink` on a span for `success` or `danger` ink), `.penna-window-host` over `.penna-window-page`, or `.penna-window-chat` holding `.penna-window-bubble` and `.penna-window-lines`; their bars are `i` elements with `--w` as a share of the pane.

HTML: `.penna-tile` with `data-penna` (`tile.card`, `tile.screen`, `tile.drop`, `tile.mini`) and `data-variant` (`card`, `screen`, `drop`, `veil`, `mini`); `data-state` (`hover`, `selected`, `focused`, `disabled`), `data-tone` (`primary`, `danger`), `data-edge` on a drop zone, and `role="button"` with `tabindex="0"` on a tile that takes focus. `tile.css` draws the variants and states, the lift as `--penna-shadow-lifted` under an inset `--penna-wash` (one wash step, laid over whatever fill the tile or its well draws), the selected ring in `::before` and the focus ring in `::after`; size, inset, placement and a mini's fill and corners are the page's, as natively. `tile.js` runs a focusable tile on Space and Enter and keeps a click from focusing it. Picking and dragging are the page's, as natively the app's.

Status: matches, but for Phone tile above, whose last seen is not built.

### Tooltip

A tooltip names the control under the pointer: an icon-only button's action, a stepped slider's step, a link's title, and the key bound to it. It holds one short line. For more than a phrase, or for anything people click, use a popover; for status, a notice or a toast.

Kinds:

| Kind | Call | Use |
|---|---|---|
| Label | `Tooltip::new(text)`; `.tooltip(text)` on a button, switch or slider | names the control |
| Label and key | `Tooltip::new(text).action(&action, context)` | names it and shows the keys bound to that action in that key context |
| Label and keys | `Tooltip::new(text).keys(strokes)`; `.tooltip_keys(strokes)` on a button | names it and shows a sequence the app binds itself, such as a prefix and then a key |

Any element takes one with ala's `.tooltip(|window, cx| Tooltip::new(..).build(window, cx))`; that one shows on hover only. A button's `.tooltip(text)` also shows on keyboard focus.

Anatomy, left to right:

- Box: the `tooltip` surface role, a floating surface with the popover fill, a hairline edge and `shadow-contact`; radius the theme radius capped at 45% of its height. Inset `space-4` (4) above and below and `space-10` (10) at the sides, so it is a `small` line (16) plus 8, and the edge.
- Label: `small` (12 on 16, 400) in the surface's ink, on one line. It never wraps.
- Key (optional): one `sm` keycap (16, the `small` line) per keystroke, on the tooltip's ground, `gap-tight` (4) apart and `space-12` (12) after the label: ⌘D on macOS, Ctrl+D elsewhere, and a prefix then a key as two caps (⌃A |).

States:

| State | Look | Notes |
|---|---|---|
| hidden | nothing | |
| shown | the box, at once | no fade in or out |

Behavior: it shows 500ms after the pointer comes to rest on its target. Its frame starts 1px right of and below the pointer and the box sits `space-12` inside that frame, so it clears the cursor by 13. Where it would cross the window's right or bottom edge it flips to the other side of the pointer. It hides when the pointer leaves the target or presses. When the keyboard focuses its target it shows at once, placed as if the pointer rested on the middle of the target's bottom edge, and hides when focus leaves, on Escape (which still reaches the rest of the window) or when the pointer moves; it comes back only on the next keyboard focus. Natively, a long press on touch shows it until the finger lifts. One tooltip shows at a time, and a disabled button's tooltip still shows.

Content: a noun or a short verb phrase in sentence case that names the action or the thing ("Split right", "Notifications", "Copy path"), with no closing period. Never an instruction ("Click to split") or a sentence. On an icon-only button it is required and is the button's accessible name too. Don't repeat a label that is already visible; on a labeled control a tooltip adds the key or nothing. Give the key with `.action()` or `.keys()` (`data-tooltip-key`), never typed into the text.

Placement: it follows the pointer, not the target, so there is nothing to place. Give one to every icon-only button (Button) and to controls whose key is worth learning.

HTML: any element with `data-tooltip="Label"` gets one from `tooltip.js`; `data-tooltip-key="cmd-d"` adds the key, and a sequence is its strokes split by spaces (`"ctrl-a |"`). The script builds `.penna-tooltip[data-penna="tooltip"][role="tooltip"][data-floating]` with a `span[data-penna-text]` per run and `.penna-tooltip-key` holding a small `kbd.penna-kbd` per stroke, sets `data-open` while shown, and places it with the same delay, offset and edge flip: on the window with `data-fixed`, or inside the nearest `[data-tooltip-host]` within a story section. A `[data-tooltip]` element that takes focus matching `:focus-visible` shows its tooltip at once from the middle of its bottom edge, until `focusout`, Escape or a pointer move. A touch press held 500ms on a `[data-tooltip]` element shows its tooltip from the press point until the finger lifts, as natively: moving more than 8 before then cancels it, and once shown the press neither scrolls nor clicks. `data-tooltip-pinned` keeps it open. A static box is the same markup without `data-floating`, its `.penna-tooltip-key` holding the caps.

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

### Action bar

The action bar holds a phone screen's one main action at the bottom, where the thumb is. For moving around, the pill.

Anatomy: a column, `gap-related` 8 apart, 16 from the sides and 20 above the bottom edge, the pill's spot (`PILL_INSET_MIN`, `PILL_INSET_MAX`). It holds the main action (Button, Phone) and at most one secondary: above it as a ghost or soft button, or beside it, the two sharing the width, the main action last. As a floating surface it pads its buttons by `space-8` and its corner is theirs plus that inset (25.5 by default, 36 under Pill; Phone, Corners), never a container's `radius`.

Behavior: when the keyboard is up it rides on it, 8 above. It never scrolls with the content.

Placement: on a zoom card or a sheet. A screen of tiles and a pane have the pill in that spot instead, and a screen of tiles makes things from the pill's menu (Pill) or its add tile, not a bar.

HTML: `.penna-action-bar` (`data-row` for two side by side, `data-keys` riding the keyboard, `data-penna="floating"` when it is a floating surface), holding `.penna-button[data-size="touch-large"]`.

Status: built as `action_bar` (`src/blocks/touch/screen.rs`), a column whose bottom inset the caller sets: `ACTION_BAR_BOTTOM` at rest, the keyboard's height plus `ACTION_BAR_KEYBOARD` while it is up. Not built: the floating surface.

### Stack menu

The stack menu is the phone's one menu: chips that rise in a column out of the thing you pressed, picked with a tap, or in one stroke with a press, a slide and a release. The pill's menu and the key row's chord menus are stack menus. For a question, or a choice that needs more than a few words, a sheet.

Anchor: the pill (Pill), or a key in the key row: ctrl, alt, or the prefix, which stands in for the pill while the keyboard is up (Key row). The stack grows out of the anchor, nearest chip first, and the chip most used sits nearest, where the thumb already is.

Two ways in, one menu:

| Gesture | What happens |
|---|---|
| Tap the anchor | the stack opens and stays. A tap on a chip picks it; a tap on the anchor or the scrim closes it |
| Hold the anchor 280ms | the stack opens under the finger, with a tick. Letting go on the anchor leaves it open, as a tap does |
| Hold, then slide | the chip under the finger lifts. Letting go on it picks it; letting go on the anchor leaves the stack open; letting go anywhere else closes it with nothing picked |

- Hold: the finger stays within 10 of where it landed for 280ms, the key row's hold. A move of 10 before then is not a hold but whatever the anchor does with a drag: the pill pages or lifts (Pill), a key pans the row sideways, and a key's slide up opens its stack at once, as the hold would.
- Once the stack is open the touch belongs to it until the finger lifts: nothing under it scrolls, pages or lifts.
- Over a chip or none: while the finger slides, each chip owns half the gap around it, so moving from chip to chip never passes over nothing. Outside the column the finger is over none.
- The anchor while the finger holds it stays itself, pressed: the pill under its press wash, a key inverted. When the stack stays open after the finger lifts, the pill turns into the close, a 56 solid `primary` disc with a 22 `xmark` in the pill's spot; a key stays inverted and is its own close.
- An anchor whose tap already does something keeps it: a tap on ctrl or alt latches (Key row), so their stacks open only by the hold or the slide. The pill and the prefix open on both.

Chips:

- A chip is a floating surface (Phone, Surfaces) 52 tall, its corner a control's from its height (16.2 by default, a capsule under Pill controls; Phone, Corners), inset 16 at the leading side and 20 at the trailing: a 20 glyph, `space-10`, then its label in strong `touch-body`. 52 rather than 44, since a sliding thumb covers what it is over and needs room to land on the next one. The whole chip is the target.
- Chips stand `gap-related` 8 apart inside a group and `gap-group` 16 between groups. Menu chips are each as wide as what they hold, centered on the column's axis.
- A group of up to six chips is one column. A group of more than six goes in two columns of equal width, `gap-related` apart, filled a row at a time from the anchor out.
- A chord chip has no glyph: the chord in `touch-small` mono, muted, at least 32 wide, then its name in strong `touch-body`. The chips of a chord group are all as wide as the widest, so the chords line up.
- Glyph row: a group may be one row of up to three glyph chips, `gap-related` 8 apart, centered on the column's axis like any group. A glyph chip is 52 square, the same floating surface and corner rule as every chip (a circle under Pill controls; Phone, Corners), a 20 glyph and no label, so each carries an accessible name ("Back"). It rests, lifts, ticks and is picked like any chip, and across the row each chip owns half the gap to its neighbour, as up the column. A row is for a few actions known by their glyph and taken in a run, such as a browser's Back, Forward and Reload (Browser on a phone).
- A chip may end in a value, `space-10` after its label, in `touch-small` muted and tabular: a count ("Tabs 3"). A chip that names a place may hold the place itself, such as a browser's address, cut in the middle so both ends show, at most the column's width, the screen less 32.
- Group label: a group whose chips all make things in one place is headed by a label chip saying where ("New in zz"): the floating surface, 28 tall, inset 12 at the sides, its words in strong `touch-small` muted, with no glyph. It is not a target: a finger over it is over none, and it does not lift.
- Looks: every chip rests the same, so the inverted look only ever means under the finger or picked; the thing most likely wanted (New chat) is marked by sitting nearest the thumb, not by a fill. Quiet, in muted ink, for a chip set apart at the far end (Settings, All bindings); `danger` ink for one that destroys. A disabled chip is in the disabled ink and does not lift or tick; letting go on it picks nothing.
- A danger chip never acts at once: picking it opens its confirm sheet (Sheet, Confirm), so a slide cannot destroy anything by accident.

Placement: the nearest chip `gap-related` 8 above the anchor, the column centered on the anchor's middle. A column that would come closer than 16 to a side of the screen keeps 16 from it, and its chips line up along that side: the prefix's stack runs along the start. The column stays 16 under the status area.

What does not fit: chips keep 52 and nothing shrinks. The groups furthest from the anchor fold behind one More chip (`ellipsis`) at the far end, as many groups as it takes for the rest and More to fit. Picking More, by a release or a tap, turns the stack to the folded groups, open and staying, the anchor still the close. Folding needs nothing a sliding finger cannot do; a stack that scrolls would need an edge that scrolls on its own under the finger.

Feedback:

- Under the finger: the chip inverts, the `primary` solid and its ink (a danger chip the `danger` solid and its ink), and grows 4% about its center over `appear` 120ms. The platform's selection tick sounds each time the finger enters a chip, and once when the hold opens the stack.
- Opening: the chips rise 14 out of the anchor and fade in, nearest first, each 0.06 of the way behind the one before, on the pill's spring (`PillSpring`, stiffness 520, damping 46).
- Picking: the other chips fold back into the anchor on the same spring while the picked chip holds its inverted look for 120ms, then it follows them; the action runs as the fold starts. Closing with nothing picked is the fold alone.
- Reduced motion: no growth, no rise and no fold. The stack fades in and out over `appear` 120ms and the chip under the finger only inverts. The ticks stay.

Scrim: the `scrim` over the screen above the keyboard, the anchor and the stack over it. A tap on the scrim closes the stack, and a drag on it moves nothing: while the stack shows, nothing under it scrolls, pages, lifts or pinches, the pill under the close included. A key typed on the system keyboard closes it too, and types.

The arrow pad (Key row) is its sibling: held only, never a menu, drawn in the same language: the floating surface, 44 cells, the cell under the finger inverted and grown 4% with a tick.

Content: a chip says a verb or a place in a word or two; a chord chip says the chord and what it does.

HTML: `.penna-stack[data-open]` holding `.penna-stack-scrim` and `.penna-stack-column` (placed by the app; `data-align="start"` or `"end"` against a side) of `.penna-stack-group`s (`data-columns="2"` for two columns, `data-chords` for chord chips of one width, `data-row` for a glyph row), each an optional `.penna-stack-label[data-penna="floating"]` and `.penna-stack-chip[data-penna="floating"]`s: an `i[data-icon]` and a `span`, then an optional `.penna-stack-value`, or a `.penna-stack-cap` and a `span`, or for a glyph chip (`data-glyph`) the `i[data-icon]` alone with `aria-label`. `data-state="selected"` on the chip under the finger, `data-picked` while it flashes, `data-look="quiet"`, `data-tone="danger"`, `disabled`. The pill's close is `.penna-stack-close[data-penna="button"][data-tone="primary"][data-prominent]`. `touch.js` places a `.penna-stack[data-from]` (a selector for the anchor inside the stack's parent): the column centered on the anchor, 8 above it and 16 from the sides, and the close on the anchor's center.

Decisions where this section was silent: the hold's 10 is the system's pan slop on the pill, 8 on iOS and Android, since past it the touch is already the pager's or the lift's; a key and the mouse use 10. When not even the nearest group fits beside More, the stack is More alone. Any group of more than six chips takes two columns, not only chords. The scrim under a held pill lies below the pill, and a key anchor is drawn again over its scrim, so the anchor stays itself while the finger holds it; what takes the touches lies over the pill and its bar, drawn with nothing, so a drag that starts beside the close finds no pill under it. A chip grows 4% with its text; an arrow cell grows its box and keeps its glyph.

Status: built. `StackMenu` (`src/blocks/touch/stack.rs:277`) takes `StackGroup`s of `StackChip`s, nearest first (a glyph row is `StackGroup::row`, a chord is `StackChip::chord`, a value `.value`, an address `.place`), places itself against its anchor in its own layer, folds what does not fit behind More (`fitting`, `src/blocks/touch/stack.rs:200`, or `set_fold` from the start), rises, staggers and folds back on `PillSpring` with the picked chip held 120ms, and fades under reduced motion. An app that draws the shade itself, under its bars (`scrim_apart`, `StackMenu::scrim`), still gets the menu's own catch over the screen: a layer drawn with nothing that closes the stack on a tap and keeps every drag, wheel and pinch from what is under it (`scrim_catch`); zz's pill menu is one. Its gesture is the pure `StackGesture` (`src/blocks/touch/stack_gesture.rs:45`), each chip owning half the gap around it (`zones`, `:194`). The pill is an anchor through `StackMenu::pill_anchor` (`src/blocks/touch/stack.rs:575`): a touch on it sets the window's long press to 280ms (`Window::set_long_press_delay`), so a finger that moves first stays the pager's or the lift's, and a held finger is the stack's until it lifts. ctrl, alt and the prefix are `StackKey`s (`src/blocks/touch/stack_key.rs:35`). The tick is `App::selection_haptic`: a selection feedback generator on iOS, `performHapticFeedback` on Android, nothing elsewhere. zz's Close chip opens its confirm sheet rather than acting. Text typed on the system keyboard into a terminal closes the key row's stacks too (zz's `TerminalTyped`). zz's browser stack uses the glyph row, the address chip and the value chip, and folds the pane's groups behind More with `set_fold` (`zz_ui::compact::browser_menu`).

### Pill

The pill sits at the bottom of a screen of tiles and of a pane: it says where you are and where a new thing will land, and its menu holds what you can do from there. It is the one place a machine's windows and sessions are made. For a screen's one main action, the action bar.

Anatomy: a floating capsule 56 tall (`COMPACT_PILL_HEIGHT`), in the action bar's spot, 16 from the sides and 20 above the bottom edge; a 40 icon tile with a 20 glyph, then two lines: the title in strong `touch-small`, and under it a `small` muted line with the page dots at its end, one per page the pill swipes through.

The second line names where new things land. Inside a machine the title is the machine's name and the second line the session in view ("macbook" over "zz"): the session whose section is under the top row, following the scroll, or the first while the large title shows. A machine with more than one session ends its page with room for the last to rise under the top row, so any session can be the one in view. A machine with no sessions says "No sessions".

Behavior: five gestures start on the pill, told apart by time first and direction second.

| Gesture | Does | Told apart by |
|---|---|---|
| Tap | opens its menu, staying (Stack menu) | let go before 280ms, having moved less than 10 |
| Hold | opens its menu under the finger | still within 10 of where it landed at 280ms |
| Hold, then slide | picks the chip it lets go on | after the hold every move belongs to the menu |
| Sideways swipe | pages to the next machine or pane | moved 10 before 280ms, more across than up |
| Drag up | goes back one level, the lift (Zoom card) | moved 10 before 280ms, upward; it opens past a third of its 300 or on a flick of 500 a second, and springs back short of both |

In plain words: a finger that moves 10 within 280ms is a swipe or the lift, exactly as today, and the hold never starts. A finger that stays put 280ms is a hold, and from then on the menu owns the touch, so moving up slides over chips and never lifts. The thresholds agree with the lift's: it starts only once the system's pan has seen about 10 of movement (`Lift::scroll`), so a drag that covers 10 in 280ms, 36 a second and slower than any deliberate drag, is never taken for a hold, and the hold is settled long before the lift could open at 100.

Menu: a stack menu (Stack menu) with the pill as its anchor, chips in a column centered over the pill, the close in the pill's spot once it stays open. Chips fall in groups. The thing you most likely want sits nearest the thumb; Settings sits apart at the top, quiet. The group label ("New in zz") has a surface because the menu's scrim is light (5% in the light themes) and bare text over it runs into the tiles behind.
- Inside a machine, top to bottom: Settings; All machines; New session; then "New in" and the session in view over New browser, New terminal and New chat, New chat nearest the thumb. What the group makes lands in the session the label names, which is the one the second line names. New chat goes on to the chat picker; New terminal and New browser make the window and zoom into it.
- With no sessions the label reads "In a new session" and New session is left out, since each chip under it starts one. The empty machine's add tile opens this same menu.

Inside a pane: the pill is the pane's only chrome (Phone, Layout). The icon tile holds the pane's kind (chat, terminal, browser) and the title is the pane's title. The second line uses a window tile's words and tones (Phone, Window tiles): "Needs approval" or "Has a question" in `warning` ink, "Failed" or "Exit 1" in `danger` ink (a terminal whose last command failed, with its folder after it), "Exited 1" in `danger` ink or "Exited" muted for a terminal whose shell has exited, the agent's working message in muted ink while it works ("Running cargo test"), and otherwise where the pane works, muted: folder and branch for a chat or a terminal ("zebrilus · main"), the host for a browser, as today (`pane_detail`). Not the window's name: a window is usually named after its chat, so it would repeat the title. The dots are one per pane of the session, in a `space-2` group per window with `space-6` between groups, the pane in view the long one and a pane that waits for you in `warning`. A tap or a hold opens the menu; a sideways swipe pages to the next pane, the content sliding with the finger and the long dot moving with it; dragging up goes to the machine. Keyboard up, the pill gives its place to what you type with (Phone, Layout).

The menu inside a pane has the machine screen's shape. Top to bottom:

- All bindings, quiet, alone, and only in a terminal: the tmux bindings are a terminal's.
- Overview, alone: back to the machine, as dragging the pill up.
- This pane: Find, only in a terminal (Terminal on a phone), then Rename, then Close in `danger` ink. Close asks first in a sheet; Rename opens a sheet with the field (Sheet).
- "New in" and the session over New browser, New terminal and New chat, New chat nearest the thumb. Each makes a window holding that one thing in the pill's session, and zooms into it; New chat goes on to the chat picker.

In a browser the pill is the browser's bar, and its stack leads with the browser's own chips, the groups above behind More (Browser on a phone).

Not in it: New window and New pane apart (a phone shows one pane at a time, so the three New chips make a window, and splitting stays on the desktop), and Last pane (a swipe away, and still a key binding).

HTML: `.penna-compact-pill[data-penna="floating"]` holding `.penna-pill-tile[data-penna="phone.pill.icon"]` and `.penna-pill-copy` (`.penna-pill-title`, then `.penna-pill-meta` with `.penna-pill-meta-text` and `.penna-dots`); its menu is a `.penna-stack` (Stack menu).

A chip that destroys takes `danger` ink (`data-tone="danger"` in HTML, `PillMenuLook::Danger` natively).

Status: the pill, its tap, hold, slide and release, the swipe and the lift are built, and its menu is a stack menu (Stack menu, Status). Inside a machine the title is the machine's name and the second line the session in view, following the scroll (`zz_ui::compact::session_in_view`), or "No sessions"; the last of several sessions leaves room to rise under the top row (`session_tail`). The machine's menu is `zz_ui::compact::machine_menu`, and what it makes lands in the session the label names: zz attaches to that session first when it is not the attached one, then makes the window in it. With no sessions a chip starts a session and puts its one thing in the session's first window. New chat makes a picker pane, which on the phone always offers the agents; the chat it starts may move to a session named for its folder, as a chat started anywhere does. Inside a pane the pill, its lines, its dots, the swipe and the drag are built; the second line is `pane_detail` drawn as the tile's words: a waiting agent says "Needs approval" or "Has a question" in `warning` ink, "Failed", "Exit 1" and "Exited 1" take `danger` ink with the folder after an exit code muted (`PillLabel::word`, zz's `pill_words`). The pane's menu is `zz_ui::compact::pane_menu`, with Find in a terminal, which puts the find field in the pill's place: Close asks first in its confirm sheet and Rename opens the rename sheet. In a browser the stack is `zz_ui::compact::browser_menu`: the browser's three groups, then the pane menu folded behind More from the start (`StackMenu::set_fold`).

### Zoom card

A zoom card is what a tile becomes when it grows to fill the screen: the thing you went into, or the form behind the add tile. For a short question, a sheet.

The thing you went into may be a screen of tiles in turn (Phone, Layout), and then it has the pill at the bottom in place of the action bar, and its tiles zoom one level further.

Anatomy, top to bottom: the platform's status area; a top row 44 with the close control at its end, `inset-pane` 16 from the side (an icon button, Button, Phone); a large title 52 in `touch-title`; the body, `inset-pane` 16 at the sides and `gap-group` 16 between its groups; the action bar. The look is the tile's (`phone.card` for a machine or the add card), filling the screen. Its corner is the phone container corner while it is smaller than the screen, the tile's at the start of the zoom, and the screen's once it fills it (Phone, Corners).

Behavior: it opens with the zoom (`zoom_layers`: the tile's layer grows to the screen while the grid behind scales up around it and they cross-fade between 0.3 and 0.7 of the way, on the touch spring) and closes the same way back, from the close control, a drag down or a pinch out; a screen of tiles also closes when its pill is dragged up. Each close goes back one level. When what it shows goes away (the machine disconnects), it zooms back out to its tile, which says why. Going into a window, what grows is the mini pane the tap opens, not the whole tile, so its preview becomes the pane (built: the lift starts from the mini pane's bounds).

The add card, behind the add tile, titled "Add machine":

- Found machines first: the section header "Found on your tailnet" with its count over a row group of two-line touch rows (List, Touch rows), the machine's name over its system and "runs zz", the add mark at the end. Tapping a row adds the machine and connects it. A found machine without zz is a disabled row that says so ("No zz on it yet"). With no tailnet the group is left out.
- Then the address field (Field, Phone), labelled "Or an address" under found rows and "Address" when alone, a 44 scan accessory inside it. Alone, it opens focused with the keyboard up.
- Connect is the main action, off until an address is typed. Connect, a found row or a read code zoom the card back out to the overview, where the new machine's tile is connecting, before the add tile.
- Scan: the scan accessory turns the same card into the scanner. The title stays; the body is the camera's picture, square, the body's width, at the phone container corner; the action bar holds "Type an address instead", a secondary at 56, which brings the field back focused. The code holds `zz://host/<machine>.ts.net:<port>`.

The connecting card, behind a connecting tile, titled with the machine's name: the body is the empty state's anatomy (Empty state on a phone) with the spinner at 32 in place of the icon, "Connecting" as its title and one line saying what it waits for ("Waiting for ssh on fabrico@xps"). Cancel is the one action, a secondary at 56: stopping is not what the card is for, so it is not drawn as the main action. Cancel stops the connect and zooms back to the tile, now "Not connected". When the connection lands the card becomes the machine. A question from ssh comes up as a sheet over whatever is on screen (Sheet).

HTML: `.penna-zoom-card[data-penna]` holding `.penna-zoom-top`, `.penna-zoom-title`, `.penna-zoom-body` and `.penna-action-bar`. The scanner's picture is the app's, not the kit's.

Status: built. `ZoomCard` (`src/blocks/compact/card.rs`) draws the card on the `phone.card` look (`zoom_card_ground`) with its 44 close, the large title, the body and the action bar, and zz grows it out of its tile with the zoom (`zoom_layers`), the phone corner from a machine or the add tile and the mini pane's from a window. zz's add card has the found rows, the field and Connect; its connecting card has the empty state with the spinner and Cancel, zooms back to its tile when the connect stops, and becomes the machine when it lands. Not built: drag down and pinch out, the scanner and the field's scan accessory (the phone has no camera of its own; the system camera opens the code's link), and the zoom back out when a machine that was in view disconnects.

### Sheet

A sheet asks a short question and returns you where you were: a password, a confirmation, a few choices. To go into something, a zoom card.

Anatomy (`penna::blocks::sheet`): floating `space-8` from the screen's sides on the `drawer` look, with the phone container corner, twice `radius` (Phone, Corners); a grabber 36 by 5, `space-6` from the top; a header 60 tall with the title in `display` and the close control at its end; the body, `inset-pane` 16 at the sides, in `touch-body`; rows from List, Touch rows; on a phone the action bar at the bottom, riding the keyboard. It rises over 240ms and is dismissed by a drag down past half its height, a fling, the scrim or the close control.

Phone: the close control is a 44 icon button (Phone, rule 2) with a 20 glyph, its corner a control's from its height like every icon button's (13.7 by default, a circle under Pill) (`CLOSE`, `src/blocks/sheet/mod.rs:22`). The panel takes the phone container corner (`src/blocks/sheet/mod.rs:539`), and its rows cap their corner at it less the row inset (`src/blocks/sheet/mod.rs:285`).

Press and hold (Phone) opens a sheet: the header titled with the thing's name, with one line under the title in `touch-small` muted for the details the tile leaves out (a window's work in progress, then where it works and its age: "Testing, 62% · zebrilus · pane-stack · now"), ending in an ellipsis; then a `.penna-sheet-list` of one-line touch rows, the danger row last (List, Touch rows), and no action bar, since each row is an action. HTML: `.penna-sheet-copy` around `.penna-sheet-title` and `.penna-sheet-detail` in the header. Natively the line is `BottomSheet::detail` and the rows `TouchRow::in_sheet`; zz's machine tile opens one (Connect or Disconnect, then Remove machine), a window tile opens Rename and Close window, and a session header Rename session and Close session (`zz_ui::compact::hold_sheet`).

Confirm: a sheet that asks before something is destroyed. The title is the question ("Close this pane?"), one line under it in `touch-small` muted says what goes ("The chat stops. Its transcript stays."), then the action bar: the main action says the verb ("Close") in the `danger` tone at 56, and Cancel over it as a quiet button at 56. No close control in the header, since Cancel is one. Built in zz as `zz_ui::compact::Confirm`: Close is the soft danger button (Button, Phone) and Cancel a ghost one.

Rename: titled with the verb and the thing ("Rename pane"), the body a 56 field (Field, Phone) holding the current name, selected, with the keyboard up; Save is the main action in the action bar, riding the keyboard, off while the name is empty or unchanged. Return saves too. Built in zz as `zz_ui::compact::Rename`, with the header's 44 close.

Content: a question from ssh is titled for the machine ("Sign in to xps"), shows ssh's own prompt word for word over a 56 field, and its main action says what it does ("Sign in"). It comes up over whatever is on screen, the overview or a card, and closing it cancels the connect. Not built: the thin client asks in a centered dialog whose action reads "Continue".

HTML: `.penna-sheet` with `.penna-sheet-scrim`, `.penna-sheet-frame`, `.penna-sheet-panel`, `.penna-sheet-grip`, `.penna-sheet-header`, `.penna-sheet-title`, `.penna-sheet-close` (`data-size="touch"` for 44) and `.penna-sheet-row`.

Status: built. The action bar in a sheet is `action_bar` placed in the sheet's body by zz's confirm and rename sheets; penna's sheet has no slot of its own for it.

### Empty state on a phone

An empty phone screen says what is missing; its main action is how to get it.

Anatomy: in the upper middle, a 32 icon in muted ink, a title in `title`, and when there is a state to say, one line in `touch-small` muted, `gap-tight` 4 apart and `space-8` under the icon, `space-32` from the sides. Its action is the screen's main action in the action bar, never a button under the text.

Content: the title says what is missing ("No machines yet") or what is ready ("Claude Code is ready"). The line says a state or a value ("Waiting for ssh on fabrico@xps"), never what the screen is for or how to use it (Phone, rule 7).

HTML: `.penna-empty` with an `<i data-icon>`, a `b` and a `span`.

Status: built as `PhoneEmpty` (`src/blocks/touch/screen.rs`), its mark an icon or the 32 spinner; the desktop's empty state (`small` text, a `body` title at 600) stays as it is.

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

### Chat composer

The chat composer is the one place a person talks to an agent: they write the next message in it, and when the agent needs an answer it asks there too. A question, a tool approval or a plan approval grows out of the top of the composer instead of opening a card in the chat, so the answer is always given where the typing happens. The chat keeps one row per ask.

Parts (`penna::blocks::chat`):

| Part | Call | Use |
|---|---|---|
| Composer | `Composer { input, leading, usage, action, attachments, command_hint, ask, secret, ground }` | the field; `ask` is the body it grows while the agent waits; `secret` hides what is typed; `ground` is the pane's ground under it, lit or veiled where the app lights its panes, and `composer_ground(ground, cx)` the ground its parts paint on |
| Frame | `ChatComposer { composer, prefix, over, footer, controls }` | the field with what sits above and below it; `over` floats over the chat from the composer's top edge, 8 above it, as wide as the composer and growing up, and leaves the chat's layout alone (zz's @ menu) |
| Highlights | `InputState::set_highlights(ranges, cx)` | ranges of the field's text drawn in their own ink and fill (zz's picked targets); masked text and the placeholder ignore them |
| Context ring | `context_ring(fraction, color, cx)`, or `context_meter(id, fraction, tooltip, cx)` for `usage` | how much of the agent's context is used: a 16 ring 2 thick, the track in the neutral soft paint's pressed fill and the arc in `color` (`context_meter` takes the muted foreground and centers it in a 28 box with the tooltip); an app can put the ring in its own footer control instead of `usage` |
| Header | `ask_header(icon, title, ground, cx)` with `ask_count` and `ask_step` at its end | what kind of approval this is, and which of several |
| Question | `ask_question(question, ground, cx)` with `ask_count` or the `ask_step` pager at its end | a question, which is its own header |
| Inset | `ask_body()` holding `ask_inset(cx)` and `ask_note(text, ground)` | what the agent wants to run or change, and where |
| Sheet | `ask_sheet(cx)` | a plan, in its own scrolling inset |
| Options | `ask_options()` holding `AskOption`s and `ask_divider(cx)`; `AskDetail::Under` or `Beside` places an option's detail; `.picked(bool)` on each answer of a question that takes several | the answers on offer |
| Row slots | `ask_leading(child)` with `ask_key(key, ground)`, or `ask_picked_key(key, ground)` once that answer is picked, then `ask_skip(id, ground)` and `ask_send(id, label, ground)`, `ASK_GAP` apart | what the input row carries while asking |
| Hints | `ask_hints(items, ground)` of `AskHint`s in the frame's footer | the keys that work now, and how many answers are picked |

Anatomy:

- Surface: the `field.composer` look. While the look draws a glass material it is glass, edged by the material's own rim and no hairline. Where glass is off it rests on the surface the activity cards use, made opaque so the transcript scrolling under it never shows through: one wash step from its ground (lighter in dark, darker in light), no edge, no shadow. Questions and approvals are drawn inside it, so they take the same surface, and everything in it (wells, option rows, keycaps, buttons, the context ring, the placeholder, the rule over the input row) paints on its ground. At rest it is the input row alone, a pill 40 tall. While asking the corner drops to 16 and the body sits above the input row inside the same surface; the input row stays at the bottom, under a hairline rule, and moves onto the sheet's gutter and gap (Input row while asking, below).
- Header: `small`, muted, `space-8` between its parts, inset 14 at the start and 8 above. A 14 glyph centered on the cap height, the title in the foreground at medium weight taking the room, then the count ("1 of 2") or a 22 ghost button for each step.
- Question: a question is the header. The same row, 8 above and inset 14 at the sides, holds the 14 chat glyph, the question in `body` at medium weight in the foreground, `space-8` apart, then the count or the pager (a 22 step button, the count, a 22 step button). The question takes the room and wraps; it is never cut short. Each line is 18, with 2 above the first and below the last, so one line is as tall as the step buttons. The glyph sits in a strut as tall as that first line (22) and is centered on its cap height, and the count and the step buttons sit at the top in a 22 box, so both stay on the first line however many lines follow. The options start 8 under it; a question with no options leaves the same 8 to the rule over the input row. The question's short label is not shown in the sheet; the chat's row and the answered record use it.
- One column: the wells, the option rows and the divider all sit 14 (the gutter) from the sides of the surface, so their edges line up, and everything inside a well or an option row starts 10 in from that edge: the first letters of a command, a plan and the option labels line up, and so do the key caps at the end. The header and the note under a command sit on the gutter itself, and so do the input row and the hints under the surface.
- Inset: a `well` with a 9 corner, inset 10 at the sides and 7 above and below, on the gutter, 8 above. A command reads in `small` mono and wraps anywhere. A note under it is `caption`, muted.
- Sheet: a plan's text in a `well` with a 9 corner, inset 10 at the sides and 10 above and below, on the gutter, 8 above and 10 below, 152 tall and scrolling inside. The header's step button opens the whole plan in a view pane.
- Options: on the gutter, 5 below, 2 between rows. A row is the `menu-item` look with a 10 corner that stays 10 however many lines the row wraps to, inset 10 at the sides, `space-10` between its parts: the label in `body` at medium weight, a `small` muted detail (under the label for a question, beside it for an approval), and a small keycap with the row's digit at the end. A hairline divider parts the answers that say no from the ones that say yes, `space-4` from the rows on each side whether they are lit or not.
- Picked, when several answers can be picked: no checkbox. A picked answer's keycap is lit, `solid` in the `accent` tone (the accent's fill, the ink that reads on it), and keeps its digit, so the same key takes the pick back. An answer that is not picked looks exactly like an answer of a question that takes one. An answer past the ninth has no keycap: picked, it shows a lit cap of the same size holding a check in that slot, and nothing when it is not, so the labels keep their edge. The row is a checkbox to assistive tech, checked or not.
- Input row while asking: on the gutter, its last button ending 14 from the side, the same 10 between the rule and the buttons as between the buttons and the bottom edge, `space-8` (the sheet's gap) between its parts. The keycap of the next digit leads it with its left edge on the option labels' (the gutter and the row's 10, 24 from the side), lit like a picked answer's once a typed answer is picked in a question that takes several; with no keycap (a question with no options, or the `+` while the field is not the answer's) the first part starts on the gutter. The placeholder says what typing does ("Something else", "Deny and say what to do instead", "Keep planning and say what to change"). At the end: Skip (a ghost button, questions only), the primary button (28 tall, a 9 corner, its label and the return glyph), then Stop.
- Hints: `caption`, muted, `space-14` apart, starting on the gutter, in the frame's footer in place of what the footer shows at rest. A question that takes several answers leads with "Pick any", which becomes "2 picked" in the foreground at medium weight once an answer is picked; a typed answer counts once it is picked and holds text.

States:

| State | Look | Notes |
|---|---|---|
| rest | the input row alone | |
| hovered | the wash a step deeper, as any field | not while it has focus |
| focused | as at rest; the caret shows it | the composer is where typing goes, so focus draws no edge and no ring |
| asking | the body above the input row, corner 16 | one ask at a time; a count in the header when more wait |
| highlighted | the `menu-item` selected fill | one row; Up and Down move it; Enter picks it |
| picked | the row's keycap lit (several answers) or the highlight (one answer) | holds with and without the highlight on the row |
| disabled | the `menu-item` disabled ink, keycaps disabled | an ask that someone else is answering; a lit keycap stays lit, dimmed |
| answered | the body goes; the chat shows one row: the ask's glyph, its label muted, the answer in the foreground | |

Behavior:

- Focus stays in the input row. A digit picks its row when the field is empty; Up and Down move the highlight; Enter runs the highlighted row, or sends what was typed; Escape skips a question, denies an approval and keeps planning on a plan.
- Several questions show one at a time. Left and Right page between them while the field is empty, and so do the header's step buttons; Enter on the last one sends them all.
- Typing during an approval or a plan never answers yes. Enter with text sends the no answer and the text as the next message.
- The message draft is kept while an ask is up and comes back when it is answered.
- The body is at most half the height of its pane and scrolls past that; a plan scrolls inside its own inset.

Content: the title names an approval in a few words ("Run this command?", "Plan"); a question's header is the question, as the agent wrote it. Option labels are the agent's own. Hints name the key and what it does now, in lowercase ("1 to 3 pick", "esc deny").

Placement: at the bottom of a chat, at most `COMPOSER_MAX_WIDTH` wide. On a narrow pane the input row's buttons keep their size and the field shrinks.

On a phone the composer is one row and an ask is a card in its place (Chat on a phone).

HTML: `.penna-chat-composer` holds `.penna-chat-field` (`data-penna="field.composer"`, `data-asking` while asking). Inside it `.penna-chat-ask` holds `.penna-chat-ask-header` (`data-question` for a question, with `.penna-chat-ask-glyph` and `.penna-chat-ask-question`; otherwise `.penna-chat-ask-title`; `.penna-chat-ask-count` either way), `.penna-chat-ask-inset` (`data-penna="well"`), `.penna-chat-ask-note` and `.penna-chat-ask-options` with `.penna-chat-ask-option` rows (`data-penna="menu-item"`, `aria-selected` on the highlighted one, `role="checkbox"` and `aria-checked` when several answers can be picked, a lit keycap or `.penna-chat-ask-tick` cap when picked) and `.penna-chat-ask-divider`; `.penna-chat-input-row` is the input row, with `.penna-chat-ask-leading`, `.penna-chat-ask-skip` and `.penna-chat-ask-send`. `.penna-chat-ask-hints` sits in the footer, its count in a `b`.

Status: matches.

### Chat on a phone

The chat at phone sizes: the same transcript, composer and asks as the desktop, each drawn for a thumb. Everything here follows the Phone rules; what is not said is as Chat composer says.

The transcript:

- Your turn is a bubble at the trailing side, at most 85% of the width; the agent's reply is plain text. Both in `touch-body`, on its line.
- A tool step or a subagent is a row at least 44 tall, the whole row the target: a 20 glyph in muted ink, the label in `touch-small` muted, its detail after it; a running step has the spinner in place of its glyph. A tap opens it as on the desktop.
- No buttons under a message. Holding a message opens a sheet titled with its first words (Phone, Press and hold) with Copy, then Fork from here.
- An image in the transcript zooms to the screen on a tap (rule 4), not a dialog.
- Jump to latest is a 44 icon button on the floating surface, its corner a control's from its height, centered 8 above the composer, there only while the transcript is scrolled away from its end.

The composer row:

- The `field.composer` surface, 16 from the sides and 8 above the pill, or 8 above the keyboard while it is up (Phone, Layout). At least 56 tall: a 44 plus at the start, the field in `touch-body`, the 44 action at the end, all inset 6. The field grows with the text to five lines, then scrolls; the two buttons stay at the bottom. Its corner is its buttons' plus the 6 inset and stays that as it grows (Phone, Corners).
- Nothing else is in the row: no chips, no context ring, no hints.
- The action: send, solid, with the up arrow, off while the field is empty; stop, solid with the stop glyph, while the agent works and the field is empty; queue, the soft look with the clock glyph, once you type while it works. Send is one look whatever the chat's permission mode: the mode shows in the plus sheet's Permissions row and nowhere else in the chat.
- A pasted image is a thumbnail inside the surface above the field, 88 square, at the surface's corner less its inset, with its remove control as a badge in its top right corner (Phone, Layout): a 24 disc with a 12 `xmark` on the floating surface, inset 4, and a 44 target centered on the disc.
- The plus opens a sheet titled with the agent's name: Mention first, then the chat's settings as touch rows with their value at the trailing end in muted `touch-small` (the app picks the rows and their words; zz has Model, Effort, Permissions and Account, in the desktop's order and words), and a last muted line in `touch-small` for the use the desktop's usage menu meters: the context, then the account's 5-hour and weekly limits when the agent reports them ("Context 34% · 5-hour 41% · Week 12%"), a limit at 80% or more in `warning` ink, as the desktop's usage pill warns. A settings row opens its choices in the same sheet, under a back control: one row per choice, its name as the desktop shows it (a permission mode leads with its glyph), the current one checked at the trailing end. A tap picks and goes back. Account lists the agent's accounts the same way, then Add account.
- Mention opens the composer's @ list in the same sheet, under a back control, with what the desktop's @ menu lists, in its order: the other chats first, then this machine's sessions, the other hosts, and `~/` for files. Each is a two-line touch row, the name the chat gives it over where it is ("Claude Code · zz:1"). A session, a host or a folder ends in a `chevron-right` and opens what is inside it in the same sheet (its windows and their panes, its sessions, its files), where the row for the thing itself comes first, so a whole session can be picked too. A tap closes the sheet and writes @ and that name into the field at the caret, drawn in the reference highlight (the `primary` soft paint at medium weight), as the desktop does. An edit inside a name turns it back to plain text; on send the app puts each name's reference back.

Queued messages: one row above the composer, 44 tall, 16 from the sides, its 20 glyph and "2 queued" in muted `touch-small`, the whole row the target. It opens a sheet titled "Queued" with a row per message; tapping one takes it back into the field, as the desktop's chip does.

The ask card: a permission request or a question takes the composer's place.

- Where: docked where the composer was, 16 from the sides and 8 above the pill, with no scrim, so the transcript above still scrolls. The `field.composer` floating surface at the phone container corner (Phone, Corners), its rows inset 4 like a row group. At most half the screen tall; past that its options scroll.
- What is asked, in one or two lines, inset 16: a permission says what it would do in strong `touch-body` ("Run this command?", "Edit this file?") over the command or the path in `touch-small` mono, muted, one line ending in an ellipsis; a tap on it shows the whole command. A question is its own text in strong `touch-body`, two lines at most before it ends in an ellipsis, a tap shows the rest. Several asks waiting add "1 of 3" at the end of the first line, `touch-small` muted.
- Then the options as touch rows (List, Touch rows), 52 tall or 64 with a detail, each the whole row and each answering on tap. A permission offers "Allow once", "Always allow" and what ("Always allow cargo test"), then "Deny". A question offers its own answers, then "Something else" last in muted ink, which brings the composer back with the keyboard up and the question one line above the field. No key caps and no send button.
- A question that takes several answers is the one exception: a tap marks a row with a check at its trailing end, and a last row "Send 2 answers" in `accent` ink sends them.
- The pill's second line says "Needs approval" or "Has a question" in `warning` ink (Pill).

Empty chat: the phone empty state in the upper middle (Empty state on a phone), the agent's glyph over "<agent> is ready", then a row group of two touch rows: Folder, with the chat's folder at the trailing end, and Resume, with the last chat's title; each opens a sheet list. The composer is under it as usual.

HTML: the composer row is `.penna-chat-field[data-size="touch"]` holding the plus, `.penna-chat-input` and the action, both `button.send` buttons drawn at 44: send and stop solid with `data-solid="primary"`, queue with no `data-solid` for the soft look. Thumbnails come first, in `.penna-chat-attach`, which takes its own line, and the question that Something else answers is `.penna-chat-above`, also on its own line: `.penna-chat-thumb` holding `button.penna-chat-thumb-remove`, the 44 target, around `.penna-chat-thumb-disc[data-penna="floating"]`, the 24 disc. The ask card is `.penna-chat-ask-card[data-penna="field.composer"]` with `.penna-chat-ask-what` (`.penna-chat-ask-what-title`, `.penna-chat-ask-what-detail`, `.penna-chat-ask-what-count`) and `.penna-touch-row`s. The queued row is `.penna-touch-row.penna-chat-queued`. Jump to latest is `.penna-touch-icon[data-penna="floating"]`. A transcript step is `.penna-chat-activity[data-size="touch"]`.

Status: built. The transcript draws at touch sizes when the chat's look says so (`ChatLook::touch` in zz, `ActivityMark::touch` and `touch_transcript_text` in penna): your turn a bubble at most 85% wide and the reply at `touch-body`, steps and subagents at least 44 tall with 20 glyphs, no buttons under messages, a held message opening Copy and Fork from here, and jump to latest a 44 floating button 8 above the composer (`phone_jump_button`). The composer row is `PhoneComposer` with `phone_plus_button` and `phone_action_button` (send, stop, queue with the clock), five lines then it scrolls (`InputState::set_max_rows`), thumbnails with their badge (`phone_thumbnail`). The ask card is `AskCard`, filled by zz's `PhoneCard` (`zz_ui::agent::phone`): a tap on what is asked shows all of it, a plan opens in a view pane, several answers take checks and an accent row that sends, and Something else brings the composer back with a 44 close where the plus was and the question over the field; a question with no answers to pick goes straight to that composer. The plus sheet is `PlusSheet` with Mention, Model, Effort, Permissions and Account, each page under a back control, and the usage line with a limit at 80% or more in `warning` ink. Mention drives the composer's @ picker from the sheet (`TargetPicker::open_here`, `drill`, `climb`, `accept_item`). Not built: the Queued sheet (the wire carries only the count, so the row hands every queued message back to the field, as the desktop's chip does); Folder and Resume under the empty chat; an image in the transcript zooming to the screen (a tap opens it as on the desktop); typing @ in the field still opens the desktop's floating list.

### Key row

The key row gives a terminal the keys a phone keyboard lacks. It takes the pill's place while the keyboard is up (Phone, Layout).

Anatomy: the pill's shape and spot, 56 tall, 16 from the sides and 8 above the keyboard, a floating surface (Phone, Surfaces). Keys are 44, `phone.key`, `space-2` apart, inset 6 from the pill's ends, their corner the pill's less 6 (Phone, Corners); a glyph key's glyph is 20, a text key's label `touch-small`. Three parts: the prefix pinned at the start, `gap-related` 8 before the rest; then the keys that scroll; then the arrow pad and hide pinned at the end, `gap-related` after the scrolling keys. The scrolling keys fade out under 16 at an edge where more wait.

Content (zz's): the prefix pinned at the start; then esc, tab, ctrl, alt, `/` and `@`, most used first, scrolling; then the arrow pad and hide pinned at the end. On a 393 phone the scrolling part shows four keys and part of a fifth (esc, tab, ctrl, alt), on a 440 phone five, and the pinned ends never leave the screen.

States:

| State | Look | Notes |
|---|---|---|
| rest | no fill; on glass a faint wash of the ink with the glass rim, so a key reads as a key | |
| pressed | inverted: the `primary` solid and its ink, the label at the strong weight | while the finger is down |
| latched | a wash of the ink, so it reads apart from a pressed key | ctrl or alt after a tap, until the next key |
| held | inverted, its stack or bubble open above the row | ctrl, alt and the prefix after 280ms, the arrow pad at once |
| armed | inverted | the prefix, while the multiplexer waits for the key after it |

Behavior:

- A key sends on release. A drag that starts on a scrolling key scrolls the row instead, and that key does not send.
- The prefix: the anchor of the pill's own menu while the keyboard is up (Stack menu). A tap or a hold opens it from the prefix, its chips lined up along the start; it is the same menu the pill opens, folded where it does not fit above the keyboard.
- ctrl and alt: a tap latches the modifier for the next key, from the row or the keyboard; a second tap lets it go. Holding one 280ms, or sliding up 10 from it, opens its chords as a stack menu over the key; a sideways drag scrolls the row instead.
- Chords: a stack of chord chips (Stack menu), each the chord in mono and its name, the most used nearest the key. ctrl's ten go in two columns, from the key out: ^C Interrupt and ^D End input, ^Z Suspend and ^L Clear, ^R History and ^U Kill line, ^W Kill word and ^K Kill to end, ^A Line start and ^E Line end. alt's four go in one: M-b Word back nearest, then M-f Word ahead, M-d Kill word, M-. Last arg. Slide onto a chip and let go to send it; let go on the key and the stack stays open until a tap on a chip, the key, the scrim or a key on the keyboard.
- Arrow pad, the stack menu's sibling: holding it opens its bubble above it at once, kept 8 inside the screen: the floating surface (Phone, Surfaces), inset 6, a 3 by 3 grid of 44 cells `space-4` apart (152 square), the four arrows around a hub dot, its corner its cells' plus the 6 inset (Phone, Corners). Dragging 10 from where the finger landed picks the nearest direction: its cell inverts, the `primary` solid and its ink, grows 4% and ticks, as a chip under the finger does. Holding a direction repeats it, after 300ms and then every 80ms; letting go closes the bubble. Reduced motion: the cell only inverts.
- Hide lowers the keyboard and gives the pill back. A tap in the terminal raises it again.

HTML: `.penna-key-row[data-size="touch"]` holding `.penna-key-row-pill`, then `.penna-key-row-pin` (the prefix), `.penna-key-row-scroll` and a second `.penna-key-row-pin` (arrow pad and hide), with `.penna-key-row-more` kept for the script. A key is `.penna-key[data-penna="phone.key"]`, `data-latched` when latched, `data-pressed` while pressed or held, `data-armed` when armed. The chords are a `.penna-stack` with a `.penna-stack-group[data-chords]` of `.penna-stack-chip`s (Stack menu). The arrow pad's bubble at 44 is `.penna-arrow-bubble[data-size="touch"][data-penna="floating"]`, `data-state="selected"` on the cell under the finger.

Status: built. `KeyRow` (`src/blocks/touch/key_row.rs`) is one 56 pill on the floating surface (`key_pill_layer`), 16 from the sides and 8 above the keyboard, its 44 keys `space-2` apart and inset 6, their corner the pill's less 6 (`key_layer`). `KeyRow::lead` pins the prefix at the start and `KeyRow::trail` the arrow pad and hide at the end, `gap-related` from the keys between, which scroll and fade out over 16 at an edge where more wait, each key by the share of it under the fade. zz's order is esc, tab, ctrl, alt, `/`, `@` (`tool_keys.rs`). A latched key is a wash (`KeyLook::Latched`, the `current` state); pressed, held, open and armed are inverted. ctrl, alt and the prefix are `StackKey`s (`src/blocks/touch/stack_key.rs:35`): ctrl and alt latch on a tap and open their chords as stacks on a hold or a slide up, ctrl's ten in two columns (`tool_keys.rs`); the prefix opens the pill's own stack on a tap or a hold, lined up along the start. The arrow pad's bubble is the floating surface with 44 cells, the one under the finger the `primary` solid, grown 4% with a tick, repeating after 300ms and then every 80ms (`arrow_pad_bubble`, `src/blocks/touch/arrow_pad.rs:125`). Not built: on glass a key rests with no wash and no rim.

### Terminal on a phone

A terminal pane on a phone: the terminal fills the pane and the pill is its handle (Phone, Layout). What the desktop does with a pointer, the phone does with a tap, a hold or a sheet. Everything here follows the Phone rules.

- At rest: the terminal runs from the status area to the bottom of the screen at the terminal's font size (13 by default) times the phone's own scale, the pill over its last lines. The pill's title is the pane's title and its second line as Pill says ("zebrilus · main", "Exit 1 · zebrilus · main"). A tap anywhere in the terminal raises the keyboard, and the key row takes the pill's place (Key row).
- Pinch: changes the text size in 5% steps from 50% to 300%, the phone's own scale, kept across launches, its percentage in a passing HUD. Nothing else is drawn for it.
- Scrolled back: a drag scrolls the scrollback, with momentum. Scrolling 24 shrinks the pill to the mini pill (Phone, Layout), and Jump to bottom appears: the chat's Jump to latest (Chat on a phone), a 44 icon button with `arrow-down` on the floating surface, its corner a control's from its height, centered over the mini pill and 8 above its 44 target. A tap goes to the end and the pill comes back. New output does not move a view that is scrolled back. The position shows only as the platform's scroll indicator at the trailing edge while it moves, 3 wide: an indicator, not a target.
- Selection: holding 500ms selects the word under the finger, and the drag that follows extends it. Letting go draws a handle at each end and opens the system's edit menu over the selection: Copy, Paste when the clipboard holds text or an image, Select All. Paste lives there and nowhere else. A handle is a 2 bar in `accent` the height of the line, with a 44 disc of `accent` at 20% and a 12 dot of solid `accent` at its center, under the end and over the start, so what is drawn is the target (Phone, rule 2). Dragging a handle moves its end and the menu comes back on release; a tap elsewhere clears the selection.
- Links: a link carries a faint underline in its own ink, since a phone has no hover to find one. A tap on it opens a sheet (Sheet) titled with the link, cut in the middle so its host and its end both show, with three one-line touch rows: Open in a browser pane (`globe`), which makes a window holding a browser in the pill's session and zooms into it as New browser does; Open on this phone (`external-link`), in the phone's own browser; Copy (`copy`). A tap anywhere else still raises the keyboard.
- Find: Find in the pill's menu (Pill) raises the keyboard and puts the find field in the pill's place, by the keyboard rule (Phone, Layout): the `field.composer` floating surface, 56 tall, its parts inset 6 and its corner the composer row's (Phone, Corners). A 44 close at the start, the query in `touch-body`, the match count as its trailing value in `touch-small` muted, tabular ("3 of 12"), then previous and next as 44 quiet icon buttons (`chevron-up`, `chevron-down`). Matches are lit in the terminal in its search colors, the current one stronger and scrolled into view; return goes to the next. With nothing found the count says "No matches" and previous and next are off. Lowering the keyboard keeps the field in the pill's spot; close ends the find, clears the marks and brings the pill back.
- An exited terminal: when the shell exits and the pane stays (`remain-on-exit`), the terminal keeps its last screen and the pill's second line says "Exited 1" in `danger` ink, or "Exited" muted after a clean exit (Pill). The phone has no respawn: Close in the pill's menu is the way out. The pane draws no badge of its own; the pill says it.
- The pill's menu in a terminal: as Pill says, Find first in this pane's group and All bindings alone at the top.
- All bindings: a sheet titled "All bindings" with one muted line under the title for the prefix ("Prefix ⌃B"), then the bindings as one-line touch rows under touch section headers (Panes, Windows, Sessions, Copy and paste, Other, Yours): the binding's name in `touch-body`, the key that follows the prefix at the trailing end as a `touch` keycap (Keycap), in `accent` ink when the binding is the person's own. A binding with several keys has a cap for each. A tap runs it and closes the sheet.

Not on the phone: the right-click menu and the hover previews. A hold and a tap do their work.

HTML: `.penna-find[data-penna="field.composer"]` holding a close `.penna-button[data-penna="button.quiet"][data-shape="icon"]`, `.penna-find-query`, `.penna-find-count` and the previous and next buttons; a selection handle is `.penna-select-handle` with `data-end="start"` or `"end"`, placed by the app. Jump to bottom is `.penna-touch-icon[data-penna="floating"]`; the link sheet and All bindings are `.penna-sheet`s of `.penna-touch-row`s, with `.penna-kbd[data-size="touch"]` as the row's value. The terminal, its marks and its underlines are the app's.

Status: built: the terminal at the pane's full size, a tap raising the keyboard (`terminal.rs`), momentum scrolling, the mini pill, pinch in 5% steps with its HUD (`compact.rs`), the long press that selects a word and extends with the drag, then the system edit menu with Copy, Paste and Select All (`ios/window.rs`), and the "Exited 1" line (`pane_detail`). Find is in the pane menu, and the find field (`PhoneFind`, `src/blocks/touch/find.rs`) takes the pill's place on the terminal's own search, with the count from `zz_ui::compact::find_count`. A tap on a link opens the link sheet (`zz_ui::compact::link_sheet`, its title cut in the middle by `BottomSheet::cut_middle`): the tap asks the terminal for the link under it, and the link that comes back opens the sheet instead of the browser; Open on this phone shows on iOS and Android only, and the open rows only for a link the app can open. Jump to bottom (`phone_jump_button`) shows while the terminal is scrolled back and the keyboard is down, placed by `over_pill`, and the pane draws neither "+3 output" nor the exited badge on the phone. Not built: the links' faint underline (the terminal finds a link only under a tap or a hover); the tap on a link in a program that tracks the mouse, which goes to the program; the handles; the indicator (the terminal's scrollbar is a 16 target); and All bindings at touch sizes (44 rows with the key spelled in mono at the leading end and a prefix keycap beside the title, `which_key_list.rs`).

### Browser on a phone

A browser pane on a phone: the page fills the pane and the pill is the browser's bar (Phone, Layout). The page is the platform's own web view laid over the pane (WKWebView, Android's WebView), with the phone's own scrolling, taps, zoom and keyboard, not frames streamed from the host; zz draws only what is around it. Everything here follows the Phone rules.

- At rest: the page runs from the status area to the bottom of the screen, the pill over its last lines. No header and no bar. The pill's title is the page's title and its second line the page's host (Pill). Scrolling the page 24 shrinks the pill to the mini pill, as on every pane. Pulling down at the top of the page reloads it, with the platform's spinner at the top.
- The pill's stack (Stack menu), nearest the thumb first: a glyph row of Back (`arrow-left`), Forward (`arrow-right`) and Reload (`redo-2`); then the Address chip, a `globe` and the current address cut in the middle; then one group of Tabs (`layers`) with the tab count as its value, Pick element (`inspector`) and Open on this phone (`external-link`); then More. Back and Forward are disabled when there is nowhere to go; while the page loads, Reload is Stop (`stop`, not the desktop's `xmark`, which would read as the stack's close just under it). What the pill offers on every other pane (Overview; Rename and Close; "New in" and the session) is behind More from the start, whatever room there is, so the browser's own chips are the stack. Back is one stroke from the pill: hold it, slide onto Back, let go.
- Address: picking the Address chip raises the keyboard, and the address field takes the pill's place by the keyboard rule (Phone, Layout). It is the find field's shape (Terminal on a phone): the `field.composer` floating surface, 56 tall, its parts inset 6 and its corner the composer row's (Phone, Corners). A 44 close at the start, the address in `touch-body`, all of it selected so typing replaces it, then a 44 clear (`circle-x`) at the end while it holds text. Go is the keyboard's own key; it loads what was typed as an address, or as a search when it is not one, and the pill comes back. Close puts the address back as it was.
- Suggestions: while typing, up to four two-line touch rows (List, Touch rows) in a group `gap-related` 8 above the field, the best match nearest the field: a 20 `history` glyph, the page's title over its address in `touch-small` muted. The group is the ask card's: the `field.composer` floating surface at the phone container corner, its rows inset 4 (Chat on a phone). A tap loads that page. With nothing to suggest there is no group.
- Loading: the progress line along the top edge of the page, under the status area (Progress, Line), filling to the page's estimate and gone once it loads. The pill's second line stays the host. In the stack, Reload reads Stop.
- Tabs: Tabs opens a sheet titled "Tabs" (Sheet) with a two-line touch row per tab: the title over the host in `touch-small` muted, the current tab with a `check` in the 20 glyph slot at the start and the others with the slot empty, so the titles line up. A tap switches to that tab and closes the sheet. With more than one tab each row ends in a 44 quiet close (`xmark`), the one row in the kit that holds a button (Phone, Layout). New tab is the main action in the sheet's action bar, 56; it opens a blank tab with the address field up.
- Pick element: picking it turns the page into the element picker the desktop has: the element under the finger is outlined and lit, with its tag and class in a small mono label, which is the page's drawing, not zz's chrome. The pill's second line says "Picking an element" while it waits, and a tap on the pill ends it with nothing copied. A tap on an element copies its context as text, and a picture of it, to the clipboard, and the pill says "Element context + screenshot copied" with a `check` in its tile for a moment; when the element cannot be read the pill says "Could not inspect that element. Try again." in `warning` ink. Pick element is off while the page loads.
- A page that failed to load: the phone empty state (Empty state on a phone) in the page's place, `triangle-alert` in muted ink over "This page couldn’t load" and the web view's error as its one line. Try again is the main action, 56, 16 from the sides and 8 above the pill, where a chat's composer sits.
- The page's own keyboard: when a field on the page takes the keyboard, nothing of zz rides it and the pill hides until the keyboard goes down. On iOS the web view's own form bar stays over the keyboard; it is the platform's.
- Links: a tap follows a link, as in any browser; a link that opens a new window opens a new tab. A long press keeps what the platform does: on iOS the web view's link preview and menu, which zz does not change; Android's web view shows no menu, since none is registered.
- Edge swipe: on iOS a swipe in from the leading edge goes back and from the trailing edge forward, the web view's own gesture (`allowsBackForwardNavigationGestures`). Android's back gesture is the app's: it closes a sheet, the stack or the overview, never the page's history.

Not on the phone: the tab strip, site controls, Copy URL, Page zoom (a pinch zooms), Clear site data, Developer tools, address suggestions from the host's history, the right-click menu and the hover previews. Open in default browser is Open on this phone.

HTML: the glyph row and chips are the stack menu's (Stack menu). The address field is `.penna-address[data-penna="field.composer"]` holding the close `.penna-button[data-penna="button.quiet"][data-shape="icon"]`, `.penna-address-text` (`data-selected` while the address is all selected, the text in a `span`) and the clear button; it shares the find field's rules. Suggestions are a `.penna-touch-group[data-penna="field.composer"]` of `.penna-touch-row[data-lines="2"]`s. The progress line is `.penna-progress[data-kind="line"]`. The tabs sheet is a `.penna-sheet` of two-line `.penna-touch-row`s, each ending in a `.penna-button[data-penna="button.quiet"][data-shape="icon"][data-size="touch"]`, and a `.penna-action-bar`. The page and the picker's drawing over it are the app's.

Status: built: the web view over the pane with its own scrolling and keyboard (`ios_browser.rs`, `ZzBrowser.java`), pull to refresh with the platform's spinner, the iOS edge swipe and new windows as new tabs. On the phone the pane has no header: the page runs from the status area to the bottom with the pill over it (`browser_pane.rs`). The pill's title is the page's title over its host, and "Picking an element" while the picker waits; a tap on the pill then ends it. Its stack is `zz_ui::compact::browser_menu`: the glyph row (Back and Forward off with nowhere to go, Reload turning to Stop while the page loads), the Address chip cut in the middle, Tabs with its count, Pick element (off while loading) and Open on this phone (iOS and Android, an http or https page), then the pane menu behind More. Address opens the address field in the pill's place (`PhoneFind` with `on_clear`), the address selected; Go loads it and the pill comes back, close puts the address back, and a tap on the page closes it. The progress line is `Progress::line` along the page's top edge, filling to `estimatedProgress` on iOS and `onProgressChanged` on Android, cut out of the web view so it shows over the page. The tabs sheet is `zz_ui::compact::tabs_sheet`: two-line touch rows with the check in the glyph slot (`TouchRow::checked`, `TouchRow::slot`), a 44 quiet close on each when there is more than one (`TouchRow::end`), and New tab as the main action, which opens a blank tab with the address field up. A failed page is `zz_ui::compact::failed_page`, Try again 8 above the pill. A blank tab draws nothing but the page's ground. When a field on the page has the keyboard the pill hides. Not built: suggestions (the desktop's come from its own history, `recent_pages`, which neither the daemon nor the phone holds); Android's back gesture does not close the tabs sheet or the address field; before the host connects, a line still says "Waiting for the host connection…".

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
| 21 | Phone components: the `control-touch-lg` rung, phone buttons and fields on the touch rungs, touch rows outside sheets, phone tile states and window content, press and hold, the action bar, the zoom card, a 44 sheet close and the phone empty state | phone buttons grow from about 32 to 44 and 56, tiles lose their buttons, text in tiles grows to 17 and 15 | in progress: the 56 rung, the 56 button and field, touch rows, the row group, the section header and the phone container corner built in penna; the rest, and zz's screens on them, not built |
| 22 | Inside a pane: the pill's tile words and pane menu, the keyboard rule, the confirm and rename sheets, the chat on a phone (composer row, plus sheet, ask card, queued row, touch transcript rows, hold a message), the 44 key row with pinned ends, the stack menu for the pill and the key row with hold, slide and release, and the browser's pill stack, address field, progress line and tabs sheet | the phone chat loses its chips, key caps and 24 buttons; terminal keys grow to 44 and the arrow pad stays in view; the pill menu's chips grow to 52, the chord card turns into chips, and holding the pill opens its menu; the phone browser loses its top bar | built, but for address suggestions |
| 23 | Glass looks: Glass stays on or off, with one look for light and one for dark that holds the three legibility rules over black and white backdrops; Frost goes | the Frost slider goes; light glass turns much lighter and calmer, dark glass dims and caps the backdrop | built |

Each step lands in penna with its stories, scores parity, rebaselines only what the step meant to move, and is pulled into zz and nidus.
