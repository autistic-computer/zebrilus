# penna: the fork of gpui-component

`penna` is the widget layer for apps built on ala. It became its own crate on 2026-10-09 so it
depends on nothing but ala; each app keeps its own application UI. It began as a thin facade over
[`gpui-component`][upstream] and was progressively forked, module by module,
until the dependency could be deleted outright. **`gpui-component` is no longer
a dependency of this workspace**; nothing outside ala (our ala) is left.

- Forked from: `longbridge/gpui-component`, now [longbridge/gpui-kit][upstream]
- Original source revision: `b004e595cf5de98a73b6b561394a559a94ae1e2a`
- Upstream license: Apache-2.0, retained here as `LICENSE-APACHE`
  (© 2024–2025 Longbridge). The icons are drawn by `src/icon/glyphs.rs` from
  `src/icon/solar.json`, adapted from the Solar Icons Set by 480 Design (CC BY 4.0,
  `src/icon/LICENSE-SOLAR`); they replaced an earlier Mac-school set, which replaced
  Tabler, which replaced Iconoir. The kit ships no brand marks: an app
  names its own icon files with `IconName::Asset` and serves them from its own
  asset source.
- `penna`'s own code is `MIT OR Apache-2.0` like the rest of the workspace;
  the ported portions remain under upstream's Apache-2.0 terms.

## Why fork

Upstream hardcodes visual decisions we needed to change: the dropdown item
text size, for one, was a literal `text_sm` with no theme token or builder
behind it. Owning the render code is the only way to change that, so the widget
moved in. Once that was true of enough widgets, keeping a partial dependency
cost more than finishing.

## Layout

```
src/
  foundation/   theme, palette, shared traits, window border
  highlighter/  syntax-highlight theme data + optional tree-sitter engine
  code_editor/  rope-backed native editor
  <widget>/     one directory per widget
  sheet.rs, dismissal.rs, touch/, pulse.rs, motion.rs   penna-original touch and motion primitives
  picker.rs, feedback.rs, which_key.rs   penna-original picker dialogs, inline alerts and prompt dialogs, which-key popup
```

Everything is reached through `penna::<widget>`, mirroring upstream's namespace.

## Ported modules and local deltas

| Module | Style | Notable local delta |
| --- | --- | --- |
| `foundation` | mixed | dropped upstream's JSON theme registry + schema (~1.4k lines, and the `schemars` dep); penna's themes are `interface_theme` files instead. Palette values ported verbatim. Metrics are down to two: `radius` (upstream's `radius_lg` is gone, and no widget derives halves or doubles off it any more) and a `CHROME_GAP` const. `rems_from_px` keeps named typography and control metrics on the 16px design baseline so changing GPUI's root rem scales them together; custom `Size::Size(px)` remains the fixed-pixel escape hatch. Added `oklab_lightness`, which exposes the L of the already-vendored Oklab conversion so an app can assert perceptual distance between two theme roots rather than eyeballing HSL. |
| `separator`, `spinner` | trimmed | reduced to the variants penna's widgets use |
| `tag` | trimmed | Three variants: Primary, Secondary, and Success; theme-driven radius |
| `kbd` | trimmed | `Kbd` formats a keystroke and draws it as our `Keycap` (neutral, soft by default, with `.emphasis()` and `.on(ground)`); upstream's `appearance(false)` plain-text mode becomes `Kbd::text`, which menus and tooltips show in the row's ink. Added `lowercase()` for hints that read as prose (`t`, `b`, `a`) rather than as a keycap legend. |
| `switch` | trimmed | dropped inline label/`Side`/custom color; kept the animated thumb. The thumb uses `foreground` for visibility against the track. The checked track is the `accent` root, the only chromatic fill a neutral control gets |
| `menu` | close-to-source | Owns its actions (`penna_menu`), key context (`PennaPopupMenu`) and `init()`; upstream's native `AppMenuBar` not carried over. Local rows use 12px text, 26px height (20px Small), 4px surface gutters, inset half-pixel separators, and the inset menu radius. Open submenu parents highlight in neutral gray; selected shortcut labels and icons brighten to full foreground. `PopupMenuItem::stepper` supports live values, pointer adjustments and Left/Right/Enter without dismissing, for a value such as a zoom level that the menu changes in place. |
| `icon` | trimmed | `IconName` is a **hand-written** enum instead of upstream's build-time proc-macro codegen; our own `Assets` replaces `gpui-component-assets`, drawing penna's own set (`glyphs`, `solar.json`) for the theme radius and corner smoothing; `IconName::Asset` names a file the app's own asset source serves |
| `tooltip` | trimmed | hangs off gpui's `.tooltip()` rather than upstream's `Root`-owned overlay; dropped `ComponentTooltip` after nothing adopted it |
| `popover` | trimmed | owns its `Cancel` action and `PennaPopover` context; `on_dismiss` runs once for every close path, including trigger toggles; the panel stays inside `fully_visible_bounds` (safe areas and a soft keyboard), using the size it measured the frame before |
| `slider` | **penna-original** | `DiscreteSlider` displays compact pills filled through the selected step, per-pill tooltips, a trailing selected value, keyboard navigation, and a slider accessibility value |
| `list` | trimmed | `ListItem` only; upstream's virtualized delegate `List` is unused. Rows use the inset menu radius and solid accent highlight retaining the normal foreground text and icons instead of a foreground outline. |
| `scroll` | close-to-source | custom-painted scrollbar kept faithful. penna fixes upstream's track-hover ordering bug: it compares the previous axis before storing the new one, so entering a hover-only track requests its repaint. |
| `button` | close-to-source | reimplements upstream's `pub(crate)` `ButtonIcon` on our `Spinner`; `ButtonRounded` keeps `Medium` (the theme control radius) and `Size(px)` for explicit overrides; `Button::compact_icon` fixes shared chrome controls at a 24px surface, Small 14px glyph, and 0.5px optical drop. Upstream's variant enum and color matrix are replaced by `ButtonVariant::Paint(Tone, Emphasis)` plus `Link` and `Text`: every fill, border and text color comes from the theme's `Paint`, and the shorthands `primary`, `secondary`, `ghost`, `danger`, `warning` and `success` name tone and emphasis pairs. The neutral soft button rests on the control surface (half-pixel edge and soft shadow) with its fill composited opaque so the shadow does not show through; the neutral ghost button shows that edge only on hover. `Button::flat()` keeps nested actions free of extra borders and shadows while retaining their wash and keyboard focus indicator. `Button::on(ground)` paints the button on the surface it sits on, such as a settings card. Themes that opt into glass inactive flattening also dim unselected neutral glass buttons and neutral buttons on glass when their native window loses focus; a web build keeps their active appearance. Upstream's `Info`, `Accent` and `Custom` variants have no counterpart: the look's `primary` setting decides whether the primary tone is the foreground or the accent. |
| `title_bar` | mixed | every `cfg!(target_os)` branch carried verbatim (macOS traffic lights, Linux/Windows client-side controls, WASM). `WindowControls` is public, unlike upstream's, so a window can go without a bar (a sidebar strip owns the drag region and the content reaches the top edge) and mount the cluster on its own through `shell::app_titlebar_strip`, the matching strip above the content column that only the platforms drawing their own buttons reserve. `TitleBar::title_bar_options(height)` supplies the window's titlebar options, with the traffic lights on the centre line of a bar `height` tall, whether or not the window renders a `TitleBar`. |
| `select` | trimmed | Retains value state and confirmation subscriptions, but renders the shared Button and PopupMenu. Uses left checkmarks with an 8px label gap, compact menu rows, bounded scrolling, and the same trigger treatment. Dropdown button labels use 13px text, one pixel larger than their menu entries. Settings triggers fit the selected label; menus fit their widest option within the popup width limit and stay at least as wide as the trigger. No separate Select row renderer. The trigger is a field drawn from the look's field paint; `Select::on(ground)` puts it on a card's ground. |
| `overlay` | close-to-source | `Root` + dialog + notification + `WindowExt`; dropped the sheet layer, upstream's `FocusTrapManager` (Tab is trapped by walking the top dialog's own focus handle) and the macOS accessibility hit-test forwarder. Toasts default to the top center of the window, with a vertically centered close button in the content row. Their width clamps to the visible window with 16px side insets on narrow screens. Dialog shadows use the `overlay` theme token rather than upstream's hardcoded `hsla` (see `clippy.toml`). The default dialog is deliberately compact: 400px wide, 12px gutters, 13px/12px title and body, and Small actions. A dialog keeps a 16px margin inside the window: a narrow window caps its width, a tall body scrolls under a max height, and a dialog too tall for the room below its tenth-of-the-viewport offset rises toward the top margin. `ROOT_KEY_CONTEXT` (`PennaRoot`) is public so an app can bind window-wide keys, such as root-rem UI scaling, below the key contexts of its own views. Local addition: `Notification::key` plus `Root::dismiss_notification`/`WindowExt::dismiss_notification`, so a toast raised for a status message that something else times can be retired by identity when that status clears (upstream can only clear the whole stack). |
| `input` | **written fresh** | not a port. Upstream's is ~12k lines because it doubles as a code editor (rope, LSP, tree-sitter, masking, OTP, in-input search); ours is a text field. Plain `String` storage, grapheme-safe indexing via `unicode-segmentation`, gpui's `EntityInputHandler` for IME. Owns `penna_input` actions and the `PennaInput` context. `text_align` is applied by the layout's index↔position math, not just at paint, so a centered field hit-tests correctly . upstream's does not. Small fields use an explicit 13px value size so compact form text matches Settings' primary row labels. A focused field asks its scrolling page to reveal it with a 20px margin when it takes focus or the visual viewport changes size (a soft keyboard), the way Flutter's `TextField` does with `scrollPadding`; settings scroll columns honor the request, and the virtual column takes it from the row before gpui's `list` sees it. `InputState::set_normal_mode` gives a view vi's normal mode in a plain field: `accepts_text_input` turns false (so a CJK input method stops claiming letters first on macOS, and Wayland's text-input turns off), typed and composed text is dropped, the field's actions still edit, and the caret becomes a block one grapheme wide with that grapheme repainted in `on()` ink, for a field that takes vi keys. |
| `code_editor/{state,input,element,mode}` | trimmed | ported from upstream `input/` at the revision above as a sibling of penna's small text field. Local `CodeEditor::background_opacity` controls text and gutter base fills without fading text or changing embedded Settings editors. Renamed the public surface to `CodeEditorState`/`CodeEditor`; keeps a rope buffer, line numbers, soft wrap, IME, single-cursor editing and upstream's tab default. Removed the `Root` downcast and mapped all chrome to penna's semantic theme. element.rs lays out differently from upstream: upstream shaped only the visible lines each prepaint, from the display map's wrap rows; penna shapes the whole buffer with gpui's `shape_text` and keeps it in a frame-to-frame `ShapedCache`, re-shaping only when content, wrap width, typography, or theme change, keyed by a `layout_generation` counter every content mutation bumps. |
| `code_editor/{cursor,blink_cursor,change,history,rope_ext}` | close-to-source | upstream cursor blink, edit grouping, undo/redo and rope helpers. `history` is upstream's `History` from before it split into a navigation trail and an undo history. It pops only the contiguous version block at the top of a stack, so a buried matching version cannot pull unrelated edits into the group, and a new edit clears the redo stack. `HistoryItem::merge` folds a typed insertion into the change before it in the same group, so a typing run undoes as one edit. |
| `code_editor/{movement,selection,indent}` | trimmed | upstream's actions over penna's own mechanics. Up/Down move by buffer line and byte column where upstream moved by wrap row and preferred x; word selection runs over `&str` and stays grapheme-safe; indent and outdent keep tab size and block/inline modes without upstream's indent guides. Multi-cursor behavior is deliberately omitted. |
| `code_editor/display_map` | close-to-source | upstream buffer/wrap/fold mapping, kept for a future fold UI. Nothing reads it yet, because element.rs wraps through `shape_text`, but every edit still reaches it with upstream's arguments (post-edit rope, pre-edit range, inserted text) and `set_value` reports a whole-range replacement. Folding compiles but has no exposed UI yet. |
| `code_editor/vim` | **penna-original** | not upstream code and not a port of anyone's: upstream has no vim layer, and the whole thing is hand-rolled on the vendored editor with zero new dependencies. Split into a pure core (`parser` keystroke grammar, `motion`, `text_object` . all plain functions over a `Rope`, no GPUI, no `CodeEditorState`) and a thin `executor` that spends the editor's existing primitives. Inert unless `set_vim_enabled(true)`: `CodeEditorState::vim` is `None` by default and every interception is behind that check. The hooks it needed in the ported files are small and marked in place . text input is diverted in `state.rs`'s `replace_text_in_range`/`replace_and_mark_text_in_range`, the bound keys (Escape, Enter, Backspace, Tab, arrows, Home/End, PageUp/Down) ask `vim_key` first, vim's control chords bind against a second `vim` key-context identifier `input.rs` adds only when the layer is on, and `element.rs` paints a block cursor plus an optional relative rail. Five vendored helpers widened from private to `pub(super)` so the executor can spend them instead of reimplementing them: `break_typing_group`, `pause_blink_cursor`, `indent_selection`, `outdent_selection`, `viewport_rows`. One deliberate structural change to `element.rs`: the line-number rail left `ShapedText`/`ShapedCache` and is now shaped per frame for visible rows only, because relative numbering depends on the cursor line and must never invalidate the buffer shaping. |
| `text` | close-to-source | renderer only . `markdown_ast` is `markdown::mdast`, so parsing comes from the `markdown` crate. Dropped upstream's HTML path (`html5ever`), which penna never rendered. Owns the window text-selection host that `overlay::Root` mounts. Heading base sizes are design pixels converted through the live root rem, keeping Markdown headings aligned with scaled body text. `InlineState` retains the hovered link range across GPUI's frame-owned mouse handlers, and a run with no links registers no move listener, so moving over plain glyphs never invalidates the window. A finished selection outlives a resize as each run's byte range (`TextViewState::preserve_inline_selection`) rather than through upstream's logical selection adapter. |
| `highlighter` | mixed | keeps the upstream non-tree-sitter stub as the default and ports the real parser behind penna's optional `tree-sitter` feature (tree-sitter 0.27, which builds for `wasm32-unknown-unknown`; the registry stores `LanguageFn` because a built `Language` is not `Send` there). The registry is trimmed to Rust, Markdown, JSON, TOML and tmux; colors come from the already-vendored `HighlightTheme` palettes rather than upstream's JSON theme registry. Incremental editor parses complete synchronously because penna does not vendor upstream's background-parser task. |

## Two gotchas

**Syntax highlighting is feature-gated.** With default features `highlighter/syntax.rs` returns
no styles and its parity test remains the contract. An app turns on `penna/tree-sitter` for real
styles; a `wasm32-unknown-unknown` build of it needs a C recipe (`llvm-ar`, tree-sitter-language's
wasm headers, empty libc stubs). Both paths use the same `HighlightTheme` colour
tables; no second theme registry exists.

**The foundation flipped last, on purpose.** `Theme` and the shared traits sit
underneath every widget. Vendoring them early would not have failed to
compile. It would have left two theme globals, with the app writing one and the
still-upstream widgets reading the other, so half the UI would have silently
stopped following the theme. The same coupling ran the other way through
`Root`: upstream's `input` and `text` downcast the window root to *upstream's*
`Root` by concrete type, so swapping ours in early would have panicked every
focused text field. Both are why `input`, `text`, `overlay` and `foundation`
landed in a single commit.

The shared Interface settings page includes a font picker using the shared Button and scrollable PopupMenu.
Native entrypoints register `AvailableFonts` with their platform text system so the list excludes
ala's hardcoded fallback names. A web build uses ala's font list. The picker includes
System default and filters internal dot-prefixed font aliases.

## Conventions

Corner radii come from each role's shape (`Theme::shape_radius`). Round control roles (buttons,
single-line inputs, number fields, select triggers) take the theme's control radius, and
`Theme::control_radius()` returns ala's full-rounding value above 24px, so Full mode can produce
pills and circles under adaptive rounding; slider pills and color swatches follow the same rule.
Row roles (popup menu, select, list and picker rows) take `Theme::menu_radius()`: a highlight
radius equal to the container radius minus 4px, without a fixed cap or Full-mode sentinel, and
square corners when the theme does not inset rows. Container surfaces and multiline inputs keep
`radius`.

Rows are drawn through `row_corners`, `row_surface` and `list_row`: ala's per-element fixed radius
mode, smoothing 2.5, and a radius capped at 40% of the row height. `row_state` picks the selected
or disabled layer, and custom menu content receives the highlight state so descriptions brighten
to the normal foreground. Fill and border use native ala painting; no highlight painter or row
masks are involved. The menu surface keeps the window's adaptive squircle rounding.

Fields (inputs, number fields, select triggers) are painted by `input::field_chrome` from the
look's field paint (`input::field_paint`). They always reserve a 1px border, so focus changes only
the border color and never the layout. In an outlined look without glass, a focused field whose
role highlights with an edge takes the paint's focus color on that edge. Neutral focus colors,
button keyboard focus rings included, are the accent when the look's primary is the accent and the
ink otherwise. Clear buttons and steppers are ghost buttons that stay inside the outer control. `InputState::set_placeholder` updates a field's hint
without replacing its editing state.

Popup menus, pickers and dialogs share a 160ms entrance (`surface_enter`): they fade in while
settling upward by 6px, following ala's reduced-motion setting. Dialog movement starts near its
final position instead of sliding from the window edge. Each opened dialog has its own animation
identity, so a replacement dialog animates again.

- **Close-to-source ports** keep upstream's structure so a future re-sync is a
  small diff. They carry a module-level
  `#![allow(clippy::pedantic, clippy::style, clippy::complexity)]` because they
  follow upstream's style, not this workspace's pedantic lints. Correctness,
  perf and suspicious lints stay active.
- **Trimmed rewrites** port only the API penna's widgets and apps call and document the
  omissions, so a deliberate omission is never mistaken for a missing feature.
- **Chrome colors come from `cx.theme()`**, never a literal; `clippy.toml`
  enforces it, and the ports were corrected where upstream hardcoded a color.

Re-syncing a module against a newer upstream revision means updating the
revision above, re-applying that module's delta, and re-running the build
and the tests, for the host and for `wasm32-unknown-unknown`.

## Cherry-picked since the fork revision

The fork revision above is *not* moved by these: each is an individual upstream
fix re-applied by hand on top of penna's local delta, so the next wholesale
re-sync knows what it already has. Every one carries a regression test.

On 2026-09-12, we checked upstream through
[`84f57fdfcb4910623fb0bb7f795b077e249f9271`][checked-2026-09-12] and selected the
menu, scrollbar, and Markdown fixes listed below. We retained penna's widget APIs,
theme rules, text selection, and editor changes. This records selective ports;
the original source revision above remains the base for future comparisons.
On 2026-09-25, a sweep through [`f8cd4860`][checked-2026-09-25] added the rows from
`db527f50` down. On 2026-10-19, a sweep through [`1676826d`][checked-2026-10-19] (v0.8.0)
added the rows from `a4ba683e` down.

| Upstream | What it fixes here |
| --- | --- |
| [`3de68cd1`][u1] | `Language::Plain` carried the **JSON** grammar, and `SyntaxHighlighter::new` falls back to `"text"` for anything unregistered . so every ` ```bash ` / ` ```python ` fence in a Markdown document built a JSON parser and parsed the whole block to produce no styles. `LanguageConfig::language` is now `Option`, plain text is grammarless, and `SyntaxHighlighter::build_inert` never parses. |
| [`66cadafb`][u2] + [`98af8912`][u3] | `ContextMenuExt::context_menu` derived its fallback element ID from a **stack address**, so a redraw from the event-dispatch path silently dropped the open menu without a `DismissEvent` and stranded focus on it. Now `#[track_caller]` + `ElementId::CodeLocation`, plus `PopupMenu::previous_focus_handle` so dismiss restores focus without moving action dispatch. |
| [`1a667218`][u4] | `render_list_item` matched only `Paragraph` and `List` and dropped everything else on the floor . a fenced code block, table, blockquote or heading nested in a list item rendered as **nothing**, which is most of what an LLM puts in a numbered list. |
| [`be3c8413`][u5] | `CodeBlock` captured the `HighlightTheme` at *parse* time and memoized styles against it, so markdown code blocks kept the palette that was active when they were parsed across a light/dark switch. The theme is now read at render and travels with the memo. |
| [`5a2a96e6`][u6] | Menu dismissal preserves focus that an item handler moved to another control. Dismissal still restores the previous focus when the menu owns it. |
| [`d8376ad5`][u7] | Clicking a scrollbar track requests a repaint after changing the offset, including when a custom handle drives content through notifications. |
| [`c3937a36`][u8] + [`b0a1836b`][u9] | Markdown hard breaks render as newlines. Soft LF, CRLF, and CR breaks reflow as spaces inside text nodes, with mark ranges measured after normalization. |
| [`26cc9366`][u10] | Mixed text and image paragraphs split at hard newlines before wrapping. The text shaper receives one line at a time, and inline images after a hard break appear on the next line. |
| [`db527f50`][u11] | Both tree-sitter read callbacks sliced the rope chunk as `&str`, so a stale tree asking for an offset inside a multi-byte character panicked inside tree-sitter's `extern "C"` trampoline and aborted the process. They slice bytes now. |
| [`41bee9c2`][u12] (redo rule only) | `History::push` never cleared the redo stack, so undo, a new edit, then redo replayed the undone changes onto the new text. The transaction undo manager from the same commit is not ported. |
| [`e12e39ca`][u13] | Escape did nothing while a hover-opened submenu was up, because the parent kept focus and its `dismiss` bailed on an active submenu. The guard moved to the click-outside path, where a press inside the submenu must not close the parent first. |
| [`c3e85e76`][u14] + [`86f9c0fb`][u15] | Dialogs overflowed narrow and short windows: the width and x were unclamped and there was no max height. Width and height now stay 16px inside the window, and an oversized dialog rises to the top margin. Upstream does the second part with its `Positioner`; penna uses a shrinking flex spacer above the surface. |
| [`21622a70`][u16] | The context menu's dismiss subscription held a strong `Rc` of the state that owned it, leaking the menu when the row went away with the menu open. penna's `DropdownMenuPopover` and `PopoverState` had the same cycle, which upstream still carries; all three hold weak handles or use the emitter now. |
| [`7d0aaf55`][u17] | Copy and cut in a masked field put the real password on the clipboard, and word motions exposed its word boundaries. Masked fields refuse copy and cut, disable both menu items, and treat the value as one word. |
| [`a4ba683e`][u18] + [`2ef99681`][u19] (text part) | `TextViewState` dropped every background parse older than the newest revision, so a stream that appended faster than the parser showed nothing until it stopped, and the background copy of a synchronous parse was committed a second time. Results now commit when they are at least the last full replacement and newer than the last commit. penna also commits only the newest of the results that reach the view together, so a burst of chunks costs one commit, not one per chunk. |
| [`1269b7ab`][u20] | GFM splits a table cell on every unescaped pipe, even inside backticks, and markdown-rs keeps ragged rows, so such a row added a column. Rows are padded or truncated to the header's column count. |
| [`3cc2d0d6`][u21] (byte-count part) | The wrap table layout proportioned columns by UTF-8 byte length, giving CJK columns three times their share. Both layouts now size columns from one measurement of the shaped text. Upstream's narrow-first shrinking and device-pixel rounding are not ported. |
| [`8e20e3bb`][u22] (content-mask part) | A run's selected range skipped glyphs outside the content mask, so a selection taller than its clip copied only what was on screen. `SelectionFormat::Source` is not ported. |
| [`0bbd9870`][u23] | Any size change cleared a TextView's selection, and every commit reset it, so streaming replies and window resizes dropped selections. Height-only growth keeps the endpoints, append commits keep the selection, and a width change freezes each run's range until the next gesture. |
| [`af78e935`][u24] + [`58d078b6`][u25] | Custom Markdown blocks gave their text only to select-all. A drag that crosses a block takes it whole; a gesture that stays inside one never does. |
| [`6b7a07c0`][u26] (highlighter part) | Injection trees were reused without `Tree::edit`, so a same-length edit inside a fence kept stale highlights. Layers take every edit; combined layers reuse their edited tree by language. |
| [`319b56cf`][u27] | The scrollbar tested raw bounds for presses and hover, so clicks on a menu over its track scrolled the list. It asks its hitbox now. |
| [`1188c110`][u28] (blink part) | `code_editor` `BlinkCursor::stop` kept its visible and paused flags, hiding a refocused caret for 500ms. The 1px caret and press-to-focus changes are not ported. |
| [`cc839912`][u29] (PopupMenu part) | A key press ended hover and cleared the menu highlight, so an unbound key reset keyboard navigation. The clear is skipped while the last input was the keyboard. |
| [`9c4b175c`][u30] | ContextMenu focused its menu during layout, every frame, which panicked debug builds with accessibility on. It focuses the menu before its first frame; PopupMenu refocuses itself when a hover closes a focused submenu. |
| [`369f7a8f`][u31] | Button skipped its hover style while selected, leaving ala's cached hover state stale after a dropdown closed. The style is always registered and empty when it should not apply. |
| [`70465ac7`][u32] (Space only) | Space on a focused select did nothing; it opens it, outside `PennaInput` contexts. |
| [`40ac007f`][u33] (vertical selection) | Shift+Up on the first editor line selected nothing, and horizontal selection kept a stale remembered column. |
| [`dca2c369`][u34] (hover part) | Markdown runs redrew their view on every glyph the pointer crossed; they redraw only when the hovered link changes. |
| [`2769e656`][u35] (undo coalescing) | Undoing a typing run replayed each keystroke with a full-buffer `to_string` and reparse; contiguous typed insertions merge into one change. |
| [`dba9f13e`][u36] (inline `<br>`) | Lost when penna dropped the HTML path: a lone `<br>`, `<br/>` or `<br />` renders as a line break again; other inline HTML stays literal. |

Checked against this fork and deliberately **not** taken:

- `accb9616` (scroll mask wheel routing) . `ScrollableMask`/`horizontal_scroll_area` were dropped from `scroll` on purpose; nothing here uses them.
- `c48fb6f0` (IME selection range) . both `input` and `code_editor` already compute the selection with `utf8_offset` from the replacement start; the port never had the bug.
- `4ac87b15` (IME composition underline) and `3c270ed2` (unset gutter background) . neither the fresh `input` nor `code_editor` paints an IME underline or a gutter fill, so there is nothing to correct.
- `f03f3713` (stub styles lose the text color) . `Inline::request_layout` already emits a trailing run in the base text style when the highlight list is empty, so the non-`tree-sitter` stub can keep returning nothing. `highlighter::syntax`'s inertness test stays the contract.
- `bc174a7e` (nested submenu paint priority) . it wraps *every* menu in `deferred().with_priority(..)`, changing paint order for every menu to fix a bug that needs two nested submenu levels, which nothing built on penna opens yet.
- From the 2026-10-19 sweep: the rest of each partly ported commit above (named in its row), and syntect-based highlighting, which penna does not take.
- `630b664f` (Linux client-side decorations) . Linux-only shadow/border cosmetics that cannot be verified from this workstation, landing in a file whose local delta (`resize_hit_size`, the restore/inset double-counting fix) must stay intact. ala solves the rounded-corner problem upstream works around here with its own `Window::set_window_corner_mask`.

[u1]: https://github.com/longbridge/gpui-component/commit/3de68cd11752e09b5f8834d27a8a3cb43032ad3d
[u2]: https://github.com/longbridge/gpui-component/commit/66cadafb58061e2d65fe6ddbc58fbc72db2b6f64
[u3]: https://github.com/longbridge/gpui-component/commit/98af8912ab0b3fe08df519dff7acd96a77b19586
[u4]: https://github.com/longbridge/gpui-component/commit/1a66721833a75d796b949e59370a3374baba793d
[u5]: https://github.com/longbridge/gpui-component/commit/be3c8413766cafc736a0c1c80306ff0f293e04f3
[u6]: https://github.com/longbridge/gpui-kit/commit/5a2a96e63e22d8d3f3b9131cf6215623a45f2ebf
[u7]: https://github.com/longbridge/gpui-kit/commit/d8376ad5635b963b2e751fbd4e328f9195d565a5
[u8]: https://github.com/longbridge/gpui-kit/commit/c3937a36dbe97347fd84f453761fc19784374ae2
[u9]: https://github.com/longbridge/gpui-kit/commit/b0a1836b1e2e3053b8998517f1816ab47e4474ae
[u10]: https://github.com/longbridge/gpui-kit/commit/26cc9366abb27ccedce386ac99a615a8fa7018da
[u11]: https://github.com/longbridge/gpui-kit/commit/db527f50a0b1c5b829a18b151e2b4d3dbac3ffdb
[u12]: https://github.com/longbridge/gpui-kit/commit/41bee9c280b6708d3671b5e9b137a78f49394568
[u13]: https://github.com/longbridge/gpui-kit/commit/e12e39ca55051d02f0e2ae8a910a674905c722d6
[u14]: https://github.com/longbridge/gpui-kit/commit/c3e85e76fd009dd7011b64cc6a4728ebb05796b0
[u15]: https://github.com/longbridge/gpui-kit/commit/86f9c0fb5907549d9a5b0aff53b4b6865933a8e8
[u16]: https://github.com/longbridge/gpui-kit/commit/21622a70efd25219d26aa459164878c4da9e39f8
[u17]: https://github.com/longbridge/gpui-kit/commit/7d0aaf55852add1e0af48cabd7ae6fa3ae791cde
[u18]: https://github.com/longbridge/gpui-kit/commit/a4ba683e44c062325a030ad8ae36d6b830a9eb62
[u19]: https://github.com/longbridge/gpui-kit/commit/2ef9968180e72a5e70d084ea3fc669b986f9c209
[u20]: https://github.com/longbridge/gpui-kit/commit/1269b7abc9a57aa4252bcc5fb525b72bf7fcd692
[u21]: https://github.com/longbridge/gpui-kit/commit/3cc2d0d624ce124be62eb4670e04197bc78bb8a5
[u22]: https://github.com/longbridge/gpui-kit/commit/8e20e3bb2055c71e66e176032c79063cee03bbab
[u23]: https://github.com/longbridge/gpui-kit/commit/0bbd9870ce420af9feacbf948b21d3824d82dc0b
[u24]: https://github.com/longbridge/gpui-kit/commit/af78e935f237ff7cf0b5fc6de8132acaf8678411
[u25]: https://github.com/longbridge/gpui-kit/commit/58d078b67fdbddc739faf1f475f2da964aad2bff
[u26]: https://github.com/longbridge/gpui-kit/commit/6b7a07c00d0106bc07194b92247a45fe5ce0e703
[u27]: https://github.com/longbridge/gpui-kit/commit/319b56cf555d4d6794a77b8b39054adb1b6c628d
[u28]: https://github.com/longbridge/gpui-kit/commit/1188c11003b5cb5b83bc07d3fb9112e7674fbfa7
[u29]: https://github.com/longbridge/gpui-kit/commit/cc8399120aa6888ebb2c0b4ed2d085c8fd161724
[u30]: https://github.com/longbridge/gpui-kit/commit/9c4b175c6b7716d9df141544db6f6c4880501a78
[u31]: https://github.com/longbridge/gpui-kit/commit/369f7a8f5a6602da980f008cf726467588ad517f
[u32]: https://github.com/longbridge/gpui-kit/commit/70465ac7413920a8cc7b5fb71920280f7bfb5c85
[u33]: https://github.com/longbridge/gpui-kit/commit/40ac007f93e0ca7397013b9a86c21b54013281c9
[u34]: https://github.com/longbridge/gpui-kit/commit/dca2c369ec900b097f5fe10c10c738255e74fc12
[u35]: https://github.com/longbridge/gpui-kit/commit/2769e6567eea0f9eb9917f91845041f3f2fad9a5
[u36]: https://github.com/longbridge/gpui-kit/commit/dba9f13efa20c4dc8d09229df5863becc4ff9bb1
[checked-2026-09-12]: https://github.com/longbridge/gpui-kit/commit/84f57fdfcb4910623fb0bb7f795b077e249f9271
[checked-2026-09-25]: https://github.com/longbridge/gpui-kit/commit/f8cd486005ab1c09fd6cf9fc00e8a4ca42a4ad17
[checked-2026-10-19]: https://github.com/longbridge/gpui-kit/commit/1676826d9d49b90f4d7b6808b69924f58796f6f1

[upstream]: https://github.com/longbridge/gpui-kit
