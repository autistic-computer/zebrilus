---
type: Design Plan
title: "Foundations: the scales and rules every penna screen is built from"
description: "The agent-facing spec for penna's foundations: space, gap and inset tokens named by their pixel value, the interface, reading and touch type ladders, the control size ladder, color roles and contrast, corners, depth, focus, motion, the six layout primitives, and the order of work that brings penna onto them. People read the Foundations pages of the docs site instead."
status: In progress (2026-11-04, steps 1 to 3 of 9 built)
resource: src/foundation/space.rs
tags:
- ui
- design-system
- tokens
- design-plan
timestamp: 2026-11-03T00:00:00Z
---

# Foundations

The spec agents read before building or changing UI with penna. It holds every value, name and rule. People read the Foundations pages of the docs site (`docs/pages/foundations.md`, `color.md`, `typography.md`, `spacing.md`, `sizes.md`), which show the same ideas with real components and no numbers. When the two disagree, this file wins and the pages get fixed.

Values are design pixels: one unit is 1/16 of the root rem, so the interface font size and window zoom scale everything together. `rems_from_px` converts.

Status marks: **built** is in the code; **step N** is planned in the work order at the end.

## Rules

1. Take every length from a scale in this file. A pixel literal in a component is a bug, except hairlines (0.5 and 1) and optical offsets computed from font metrics (`cap_center_drop`).
2. Every element answers three questions, each with a token: its height (Size), its inset (padding, owned by the element), and the gap to its neighbors (owned by the parent).
3. Components never set an outer margin. The layout that holds them sets the gap.
4. Pick a gap by relationship, not by eye: tight for a label and what it names, related for siblings that work together, group for groups inside a pane, section for the parts of a page. When two levels both seem right, the grouping is unclear: add a group, never a value between levels.
5. Every piece of text has a text style: size, line height and weight together. Never set a size without its line height.
6. Every control in one row has the same size.
7. A token has one name everywhere. Its number is its pixel value. Rust and CSS spell it the same way (see Names).
8. Color comes from the theme's roles and looks, never a literal. Status colors always come with an icon or a word.

## Names

| Token | CSS | Rust | Status |
|---|---|---|---|
| `space-8` | `--penna-space-8` | `Space::S8` | built |
| `gap-related` | `--penna-gap-related` | `Space::GAP_RELATED` | built |
| `inset-card` | `--penna-inset-card` | `Space::INSET_CARD` | built |
| text style `body` | `--penna-text-body-size`, `-line`, `-weight` | `TextStyle::Body` | built |
| `control-md` | `--penna-control-md` | `Size::Medium` | step 7 |
| colors | `--penna-fg`, `--penna-fg-muted`, ... | `cx.theme()` fields and looks | built |

Until step 1, `Space` variants were named in 2px steps (`Space::S4` was 8px), while ala's helpers count 4px steps (`gap_4()` is 16px). Don't use ala's spacing helpers (`gap_2()`, `px_3()`, ...) in penna; use `Space`.

## Space

Scale: 2, 4, 6, 8, 10, 12, 16, 24, 32, 40, 64.

- 2, 6 and 10 only inside a control: icon gaps, field insets, segment padding.
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
| `inset-card` | 12 | cards, menus, popovers |
| `inset-pane` | 16 | panes and dialogs |
| `inset-page` | 32 | document pages |
| row inset | theme | lists around their rows: `theme.row_inset`, a theme knob (4, or 5 in the default theme) |

Rows in a list touch: their height keeps the rhythm.

An inset reaches the text's line box, not its letters. In `body` (13 on 18) capitals start 4.27px inside the box and the baseline sits 4.27px above its bottom, so an `inset-card` reads as about 16px to the capitals. Keep the token; never nudge text.

## Type

Faces: the platform system face natively (`.SystemUIFont`), Inter Variable in the storybook and on the web, Lilex for code (Menlo natively on macOS). Weights: 400 text, 500 emphasis in the interface (`strong`), 600 headings. No other weights.

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

`TextView` sets `read-body` on itself (`TextViewStyle::text`, default `Some(TextStyle::ReadBody)`). `None` makes it inherit its container's text style instead: zz's agent transcript does that to keep its own 12 to 13px sizes. Headings always take the reading ladder. A task list's checkbox is as tall as the font and sits in a box one line tall.

Touch, for the compact, touch and sheet blocks (`TextStyle::TouchSmall`, `TouchBody`, `TouchTitle`, built): `touch-small` 15 on 20, `touch-body` 17 on 22, `touch-title` 34 on 40, weights 400, 400, 600; strong is 500 for the first two.

Component text (step 3, built): list and tree rows `lead` (4 + 20 + 4 makes the 28 row), their details `small`; sidebar headings strong `small`, under the `body` rows they head; menu rows and their shortcuts `small` (`body` under a coarse pointer), menu section labels `caption`. Controls take the text of their size from `Size::control_text()`, the Text column of the control ladder: buttons, fields, selects and segmented controls set it once on the control and their label, icon and children inherit it, so a caller's weight (a selected sidebar row's 500) still wins. A field's caret, selection and rows follow its line (18 at `body`). Badges are strong `caption` (`micro` when small), keycaps `caption` (`micro`), the stepped slider `small`, and the touch slider's readout `title`. Surfaces (dialogs, alerts, notifications, callouts, popovers) set `body` on the surface, before the caller's own style, so content with no style of its own is 13 on 18; their titles are `body` at the heading weight, 600, and their descriptions and messages `small`. An empty state sets `small` on itself, since its message is its bare content, and its title is `body` at 600. A marker beside a first line (a notification or callout icon) sits in a box as tall as that line: 18 under a title, 16 beside a message alone. Tooltips are `small`. Chrome: the title bar's content area and the tray panel set `body`; a window title in the title bar is `body` at 600, with a path beside it regular and muted. The docs header's brand is `lead` at 600. Which-key sets `small` on itself, its table name strong `small`, and `caption` for block titles, the repeat mark and its key caps in the code face. Settings: the page heading is `title`, group headers `body` at 600 (so a disclosure's chevron box is 18 tall), row titles `body`, descriptions `small`; the about page's name is `title`, its tagline `small`, its values `caption` in the code face. The picker's rows are `small`, its empty label `caption`, its footer `micro`. The color picker's label is `caption`, the touch trigger's hex `mono`. Appearance: slider values `small`, tile labels `caption`, and the theme picker's label takes the style of the button or menu row around it. Compact and touch: large titles `touch-title`; the HUD strong `touch-body`; the folding title `touch-body` at 600; pill titles and menu chips strong `touch-small`, pill meta `small`, the mini pill strong `body`; key rows `touch-small`; the touch which-key list `touch-body`, its caps `touch-small` in the code face, its group headers strong `body`; popover keys `touch-body`. Sheets: titles `display`, row labels `touch-body`, details `touch-small`. The code editor sets `mono` on itself (13 on 18) and draws line numbers at the `caption` size on the code line. The HTML versions read the `--penna-text-*` variables instead of copying sizes.

Code takes the size and line of the text around it, in the code face: `mono` in the interface, `read-body` size in reading text. ala can't size one run differently inside a line.

Root text (step 4): ala gives text with no style 16px on a line 1.618 times its size (25.89px), and the HTML root is 13px on 1.618. `Root` will set `body` on every window, as it already sets the corner smoothing, and `penna.css` the same on `html`, so text with no style gets 13 on 18 on both. It lands after step 3, once no component leans on the default, so it moves only app text that sets no style.

Centering: Inter centers its capitals in the line box (within 0.01px at 13 on 18); the macOS system face sits them 0.33px low and Lilex 0.34px. Center a line box to center text; center icons beside text on the capitals with `cap_center_drop`.

## Size

Control ladder. Every control at a size shares one anatomy:

| Token | px | Text | Inset | Field inset | Icon | Icon gap | Status |
|---|---|---|---|---|---|---|---|
| `control-xs` | 20 | `caption` | 8 | 6 | 12 | 4 | step 7 (chips today) |
| `control-sm` | 24 | `small` | 8 | 6 | 12 | 4 | built: `Size::Small` |
| `control-md` | 28 | `body` | 12 | 8 | 14 | 6 | built: `Size::Medium`, the default |
| `control-lg` | 32 | `body` | 16 | 10 | 16 | 6 | step 7 |
| `control-xl` | 40 | `lead` | 16 | 12 | 16 | 8 | step 7 |
| `touch` | 44 | `touch-body` | 16 | 12 | 16 | 8 | step 6 |

Fields use the field inset because their text starts at the edge.

Other heights: `control-chrome` 22 (controls in bars, today `PANE_HEADER_CONTROL_HEIGHT`); `bar` 34 = `control-chrome` + 2 × `inset-chrome` (title bar and pane headers); `row` 28 (list, sidebar and menu rows; menu rows are 26 today and move to 28 in step 7); tabs take the ladder (36 today becomes 32 or 40 in step 7).

Icons: 12 beside `small` and `caption`, 14 beside `body`, 16 beside `lead` and `heading`.

Inside a line of text, keycaps, badges and checkboxes take the line height of the text they sit in (16 in `small`, 18 in `body`), never a control size.

Width: reading text stops at a measure of 640 (about 75 characters of `read-body`). Sidebars are 200 to 256 wide, layout geometry rather than tokens.

Density (step 9): one setting picks the default control and row height: compact 24, regular 28, comfortable 32. Text and gaps stay.

## Color

Roles (CSS root names, default theme):

| Role | Light | Dark | Use |
|---|---|---|---|
| `bg` | #fcfcfc | #18181a | window and page |
| `panel` | #efefef | #1f1f21 | sidebars and side panels |
| `popover` | #e2e2e2 | #272729 | menus and popovers |
| `fg` | #1d1d1f | #e4e4e7 | text and icons |
| `fg-muted` | #636365 | #7f7f81 | descriptions, captions, story labels |
| `muted` | #626264 | #959598 | muted labels and the ghost paint |
| `fg-faint` | #909091 | #4b4b4c | placeholders and disabled text only |
| `accent` | #3d6df2 | #6b8cff | links, text selection, picked accent |
| `on-accent` | #ebebeb | #010101 | text on accent fills |
| `primary` | #1d1d1f | #e4e4e7 | primary fills; follows `fg` unless an accent is picked |
| `success`, `warning`, `danger` | #1f8a4c, #b25e09, #d22f3c | #3fbf74, #f0a14a, #f2555f | status, always with an icon or a word |

Surfaces stack in this order, each layer a step away from the one under it (darker in light, lighter in dark): window `bg`, then `panel`, then cards (`settings.card`), then `popover` and floating panels.

Highlight: what is on, picked or in focus uses `primary`, which is the ink itself in the default look and the accent once one is picked in Appearance.

Contrast (WCAG 2, on `bg`): `fg` 16.4 light / 14.0 dark; `fg-muted` 5.8 / 4.4; `muted` 5.9 / 5.9; `fg-faint` 3.1 / 2.0; `accent` as text 4.4 / 5.8; `on-accent` on `accent` 3.8 / 6.8; `success` 4.3 / 7.5; `warning` 4.6 / 8.4; `danger` 4.9 / 5.3. Under 4.5 and flagged for step 8: `fg-muted` dark, `accent` and `success` as text in light, `on-accent` light. Check all six theme variants (three looks, light and dark), not only the default.

## Corners, depth, focus, motion

- Corners: `radius-theme` is a theme knob people change (24 in default and macOS, 16 in Yaru; 25 at most). A control's radius is the theme radius capped at 45% of its height (`corner-fraction`). A corner inside another is concentric: inner radius = outer radius minus the inset between them. Rows draw at most smoothing 2.5.
- Depth: `shadow-contact` (0 1 4 #00000033) for controls on a surface; `shadow-lifted` for cards that move; `shadow-float` for menus, popovers, dialogs and the window. Themes can turn outlines and shadows off, so never let a shadow alone separate two surfaces.
- Focus: a 1.5px ring (`FOCUS_RING_WIDTH`) drawn outside the border box with its own adaptive radius. Only fields change their fill or edge on focus.
- Motion: appear 120ms, enter 200ms, turn 220ms, scaled by the animation speed setting (0.25 to 2). Respect reduced motion.

## Layout primitives (step 5)

Six primitives place everything. They take tokens and draw nothing.

| Primitive | Does | Rust | DOM |
|---|---|---|---|
| Stack | children in a column, a gap apart | `stack(Space::GAP_GROUP)` | `.penna-stack[data-gap="group"]` |
| Inline | children in a row on one line, wrapping when asked | `inline(Space::GAP_RELATED).wrap()` | `.penna-inline[data-gap="related"][data-wrap]` |
| Inset | padding around one child | `inset(Space::INSET_CARD)` | `.penna-inset[data-inset="card"]` |
| Split | a fixed side beside a flexible main | `split(width, gap)` | `.penna-split` with `--penna-aside` |
| Center | a column no wider than a measure | `center(px(640.))` | `.penna-center` |
| Tiles | equal tiles that fill the width | `tiles(min_width, gap)` | `.penna-tiles` with `--penna-tile` |

Arrangement geometry stays with the app: where nidus puts computers on its board, a sidebar width someone dragged, terminal cells.

## Components

Every component page and story shows, where they apply: sizes, variants, tones, and the states rest, hover, pressed, focused, selected, disabled, loading and error, plus icon support. Known gaps: error states on select, text field, checkbox and radio; helper text; an indeterminate checkbox; radio groups; prefix and suffix on fields; equal-width tabs.

Each component's docs page carries an anatomy: its parts and the tokens they use. The widget and its HTML version take the same properties.

## Work order

| Step | What | Visible change | Status |
|---|---|---|---|
| 1 | Token names in Rust and CSS: `Space` named by pixels with gap and inset constants; space, gap, inset and text style CSS variables | none | built |
| 2 | Reading text: `TextView` and `text.css` on the reading ladder and rhythm | documents and chat get smaller, tighter text | built |
| 3 | Text styles: every component sets a full text style on its text, native and HTML, one family at a time: rows, lists and menus first, then controls, surfaces, chrome and the editor blocks, with the touch ladder | per family: rows that grew from a 1.618 line return to their row height | built |
| 4 | Root text style: `Root` and `penna.css` set `body` | app text with no style goes from 16 on 26 to 13 on 18; penna itself doesn't move | |
| 5 | Layout primitives, native and DOM, with stories | none by themselves | |
| 6 | Components onto the primitives and space tokens, one family at a time, starting with Appearance (its 11 and 14 insets) | small spacing moves, reviewed per family | |
| 7 | Control ladder: `control-xs`, `-lg`, `-xl`, rows 28, tabs on the ladder | menu rows grow 2px, tabs move | |
| 8 | Color: contrast fixes across the six theme variants | flagged pairs darken or lighten | |
| 9 | Density setting | new option | |

Each step lands in penna with its stories, scores parity, rebaselines only what the step meant to move, and is pulled into zz and nidus.
