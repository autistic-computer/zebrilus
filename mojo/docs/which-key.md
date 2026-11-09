# WhichKey

`zzmojo.which_key.WhichKeyModel` ports the renderer-free row model. `rows(tables, table, prefix)` folds directional and numbered key families; passing `families=False` produces action rows. Group zero represents an absent group, and groups one through six follow the source enum order. `opens_all_keys` retains the exact one-command, one-argument condition.

The model validates the complete table vector against the shared prefix metadata limits before producing output. It uses the first matching table and first matching stock binding. The stock prefix table and exact implemented command names come from the current Rust source; `--update-data` explicitly refreshes the checked-in contract. General command canonicalization uses the existing full command catalog.

Custom binding detection compares all invocation fields retained by the wire format: raw argument bytes, source location, command blocks, alias expansion, optional stdin, and stdin availability. Notes affect labels independently of custom binding detection. Unicode whitespace, unsigned integer spelling, mouse-key substring filtering, family order, custom-set insertion order, repeat conjunction, and core disjunction follow the source.

Run `python3 scripts/port/guard.py python3 mojo/scripts/which_key_vectors.py` from zebrilus. The actual `zz_client::which_key::{rows,action_rows,opens_all_keys}` functions supply 6,730 reference cases, with 19,516 resulting rows, 20,848 key sets, 21,700 keys and 11,395 all-keys queries. The built-in stock table is stored as its message payload without the frame header, so a protocol version change does not touch the model. The suite includes stock prefixes, all source tables, classification flags, aliases, prompt/confirmation wrappers, Unicode key shapes and whitespace, note overrides, metadata-only changes, feedback commands, duplicate table names, malformed command text, and family folding.

This check covers the bounded row model. Widget layout, native input ownership and complete application behavior require their separate checks. It does not establish a formal proof for the WhichKey renderer.

`differential-which-key-caps` compares the actual `ChromeKey::parse` → `ala_source` → `Keystroke::parse` → `Kbd::format_key` path with the Mojo formatter. Its 50,032 cases include every Unicode scalar whose case changes, the same scalars followed by sigma, named keys, standalone modifiers, malformed names, alternative modifier spellings, and multi-scalar casing contexts. All 50,000 accepted values and 32 rejected values match, including modifiers and optional key-character fields. The reference encodes 1,393,362 exact bytes; the checked build peaked at 500 MiB.

The formatter uses source-generated Unicode alphanumeric ranges and case-context masks. The masks observe Rust's actual lowercase result for every valid scalar in both a final position and before another cased character. Combined with the existing source lowercase table, they preserve final sigma across case-ignorable characters without relying on Mojo's context-free lowercase operation. Digit ranges, punctuation families and modifier spelling are then formatted from the parsed keystrokes.

`which_key_sheet` builds the native short or complete sheet from these checked rows. `prepare_which_key` measures and admits the complete text batch before constructing boxes, then returns a plan tied to the viewport, font generation, text records and appearance. `paint_which_key` validates that receipt before adding surfaces. It supports ordinary content and an overlay through the trailing `overlay` flag. A stale width, scale or theme refuses the plan without adding boxes. A scroll offset is clamped to the sheet's body viewport.

The widget preserves the source's grouped stacks, continuation titles, descending fit search, independently snapped style lengths, raw flex shrink and position arithmetic, repeat mark, header keycap and optional all-keys hint. Text truncation uses the raw available width before final layout rounding and retains an already fitting shaped string. The maximum sheet height follows the source even when the sheet extends above its parent's top chrome.

Run `python3 mojo/scripts/which_key_scene_pixels.py`. The probe paints each sheet through `LiveWhichKey.frame`, the short and custom sheets by a fired table timer and the complete prefix sheets by an all-keys intent. It compares 144 actual native `WhichKeyView` scenes, 127,023,750 RGBA pixels: six themes, scales 1, 1.25 and 2, and eight short, complete, narrow, overflowing and custom-family cases. A bordered sheet surface takes its border inside the panel, so the header and body start at the border plus the 12px inset and the panel grows by both borders; the bordered light and dark themes need this. The accent contract in mjui's `penna_which_key_data.mojo` is regenerated with `--generate-colors` when the source themes change. After the upstream theme and corner changes, 142 scenes match exactly. In two, the glass sheet's edge pixel differs by one level on both sides of one row in the corner: (68, 144) and (651, 144) at theme 0, scale 1, variant 0, and (185, 716) and (1094, 716) at theme 3, scale 2, variant 3. The widget's layout matches in both; the corner itself comes from mjui's shared native surface and glass emission. No pixel tolerance or source glyph substitution is used. The source fixture also exports bounded raw layout diagnostics and actual glyph clip masks. These exposed a physical-to-logical Float32 round trip in the native renderer's outward clip rounding; the opted-in mjui physical clip path follows the same operation order.

This evidence covers the closed Inter/JetBrains Mono font pool and the exercised desktop geometry. The renderer has no formal proof claim.

## Timing

`which_key_timing.WhichKeyTiming` ports `zz_client::which_key::timing`. The desktop workspace calls that module's `hidden_generation`, `hide_for_input`, `arm_table` and `timer_decision` where it hides the sheet on a keystroke, re-arms on `KeyTableChanged`, fires the timer and drops it; `WhichKeyTiming` composes them the way `AppView` does:

- A keystroke the workspace intercepts hides the sheet, bumps the generation and drops the timer, unless the key is held while the prefix is armed.
- A key table change hides first, then arms only for a non-repeat table and a delay above zero. The delay is the configured milliseconds rounded half away from zero; zero disables the sheet.
- The timer is stale when the generation moved. Otherwise it is consumed, and shows the short sheet only when the active table still equals the armed table without repeat and nothing blocks it. The sheet is visible only when it has rows.
- Each render drops a visible sheet and a pending timer while blocked, without a new generation.
- The all-keys key shows the complete prefix sheet and leaves the timer alone.

`python3 mojo/scripts/which_key_timing_vectors.py` runs 4,052 transition cases over every operation at generations 0, 1, 2, `u64::MAX - 1` and `u64::MAX`, plus 32 seeded 96-step walks, and 13 table arming cases. Every state, notify and build flag matches.

## Live controller

`which_key_live.LiveWhichKey` drives the sheet from the live workspace. Its frame entry point is `frame(ui, theme, tables, context) -> WhichKeyFrame`; typed intents reach it through `apply(intent)` at the three places the source reacts:

| Source event | Intent | Where to call `apply` |
| --- | --- | --- |
| `AppView::intercept_keystroke` | `WhichKeyIntent.input(eligible, held, armed)` | Every key press in the workspace window, before routing it. `eligible` is `desktop_intercepts(window focused, key name)`, `held` the key's repeat flag, `armed` `model.routing.armed` before this key. |
| `KeyTableChanged` | `WhichKeyIntent.table_changed(time_ns, table, repeat, delay_ms)` | After each key-table-active event (stream event 54) updates `model.routing`, and when a reset, detach or server stop clears a table that was set. `time_ns` is `UInt64(ui.time * 1e9)`, `delay_ms` is `DesktopKeyConfig.which_key_ms()`. A delay above 2,000ms or a table name above the 64 KiB prefix text limit is refused. |
| The all-keys route of `ForwardKey` | `WhichKeyIntent.all_keys(effect)` | For each `DesktopUiEffect` of kind 3 returned by `DesktopWorkspace.acknowledge`. Pass `which_key=which_key_ms() > 0` to `DesktopWorkspace.prepare` so the route only exists while the delay is enabled. Any other effect kind is refused. |

The all-keys intent builds from the prefix bindings and canonical spelling captured with the effect, which equal what the source reads at that keystroke. Applying intents at once keeps no queue. An input or table intent settles the same state an earlier overdue timer would. The all-keys intent leaves a pending timer alone, as the source does, so in one case the order shows: a held key kept a timer pending past its deadline and the all-keys intent arrives before the next frame. That frame fires the timer and the short sheet replaces the complete one, where the source fired first and kept the complete sheet.

Call `frame` once per workspace frame after the panes, with:

- `tables`: `model.key_tables.snapshot.value()[].tables` while attached, otherwise an empty list.
- `context.table` and `context.repeat`: `model.routing.table` and `model.routing.repeat`.
- `context.prefix`: `canonical_key(model.routing.options[0].value)` while attached, otherwise `None`.
- `context.blocked`: the source's `which_key_blocked`: a menu, confirm, local palette, window chooser or settings route is open, a palette, buffer or tree chooser, display-panes or command output overlay is visible, a pane drag or any other drag is active, or a dialog is open.
- `context.top` and `context.bottom`: the pane canvas top and the pane margin.

`frame` fires a due timer, applies the render block, prepares and paints the sheet as an overlay with key `0x7600000000000000`, and requests a wake at the pending deadline in `ui.time` seconds. It reads the current time from `ui.time` rounded to the nanosecond, so the frame woken at a deadline always reaches it. It returns whether the sheet is visible, the pending deadline, any refusal and the painted panel and body geometry. A sheet whose rows or text cannot be admitted is dropped and reported in `WhichKeyFrame.refusal` rather than painted; a paint failure after a valid plan still raises. The controller has no wheel scrolling yet, and the paint treats the ui viewport as the source's pane canvas: the source sizes the sheet from the pane canvas, falling back to the viewport when it is empty, and centers it in the workspace surface.

`python3 mojo/scripts/which_key_live.py` replays the source workspace tests: the 400ms pause shows the short prefix sheet at 400ms and not at 399ms with the `?` all-keys hint, and the next key hides it; a held prefix key keeps the timer; repeat tables, cleared tables and a blocking overlay hide it and a blocked timer never shows; a custom table lists `h l` flat with repeat marks and its own key hides it; the all-keys intent shows `M-1` without the hint until the next key; a zero delay never arms.
