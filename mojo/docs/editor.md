# Editor pane and file picker port

The editor port follows `crates/zz/src/editor/view.rs` and
`crates/zz/src/file_picker.rs`. Their decisions now live in
`zz-client::editor_pane` and `zz-client::file_picker`, which the desktop app
re-exports and the Mojo modules are checked against. `EditorPane` in
`zzmojo/editor_pane.mojo` is the one entry point the workspace needs, and
`EditorDeck` in `zzmojo/editor_workspace.mojo` keeps one pane per editor
descriptor in the snapshot. Nothing in `WorkspaceView` calls them yet: the
wiring steps are at the end of this page.

The protocol carries no editor events. A pane learns about the daemon through
its descriptor in the snapshot (`path`, `cwd`, pane kind 3) and answers with
two commands, `set-editor-path` and `select-pane -T`.

## Checks

From the repository root, each acquires the shared build guard:

| Command | What it proves |
| --- | --- |
| `python3 mojo/scripts/editor_pane_vectors.py` | Descriptor synchronization (46 cases), file names, titles and languages (32), path validation (30), bounded reads (21), atomic saves (15, 18 saved entries) and 12 model scenarios of 3266 steps against `crates/zz-client/examples/editor_pane_vectors.rs` |
| `python3 mojo/scripts/file_picker_vectors.py` | Path priors (150), selection (9), query validation (24 + 22), ranking (4 sets, 42 queries, 3990 rows), 8 flow sequences of 1846 operations and the folder scan (18 entries, 12 rows) against `crates/zz-client/examples/file_picker_vectors.rs` |
| `python3 mojo/scripts/editor_pane.py` | `EditorPane` and `EditorDeck` drawn headless with mjui: open through the picker, typing, save, save as, retry, refusals, pointer and key claims, focus routing and the 64-pane limit, exact values |
| `/usr/bin/python3 mojo/scripts/editor_scene_pixels.py` | The open hint and the two error panels (failed open, failed save) against a native render of the source view from `crates/zz-ui/examples/editor_scene_fixture.rs`: 6 themes at scales 1, 1.25 and 2, 54 cases, 40824000 pixels, every pixel equal. Needs a Python with Pillow |

## Modules

| Module | Responsibility |
| --- | --- |
| `editor_state` | `EditorModel`: path, snapshot path, pending path, saved contents, dirty flag, title, error with its retry kind, picker flag; every transition returns `EditorEffect`s |
| `editor_files` | Reading a file of at most 8 MiB as UTF-8, and saving through a temporary file in the same folder that replaces the target |
| `file_picker` | `FilePicker`: the folder scan in slices, the query flow, ranking, selection and accept |
| `editor_pane` | `EditorPane`: the model, mjui's `Editor`, the picker overlay, the error panel and the outbox |
| `editor_workspace` | `EditorDeck`: descriptors from a snapshot, one pane each, focus, routing by pane id |

## Behavior carried from the source

- A pane without a path is a scratch buffer titled `editor`. While it is empty
  and has no error and no picker, the hint reads `Ctrl+O to open a file`
  (`⌘O to open a file` on macOS).
- Opening a path sets the language from the extension (`rs`, `md`, `markdown`,
  `mdx`, `json`, `jsonc`, `toml`, else plain text), publishes the file name as
  the pane title and reads the file. A file that loads publishes its path
  unless the snapshot already names it.
- A change that makes the buffer differ from the saved contents marks the pane
  dirty and publishes the title with ` •` appended; a change back, or a save,
  publishes the plain title. The title goes out only when it changes.
- The pane draws no header of its own. The title and the dirty mark are the
  pane title it publishes, so the model vectors cover them and the scene
  check covers the hint and the error panel.
- Save writes the buffer to the pane's path. Without a path it asks the host
  for one, suggesting `untitled` in the pane's working directory.
- A failed open shows `Couldn’t open this file`, a failed save
  `Couldn’t save this file`, each with the system message and a `Try again`
  button that repeats the same read or write. A failed save keeps the pane's
  path and the dirty mark.
- The error panel is the source's inline alert: at most 360 px wide, centered
  over the pane's plain panel with 12 px to the pane's edges, the danger
  triangle beside the title and the message, and the primary `Try again`
  button at the right of its own row. Title, message and button label use the
  code font, as the source's pane root sets it.
- A descriptor that echoes the path the pane published clears the pending
  path. A descriptor that names another path loads it. Paths and working
  directories must be absolute, at most 16384 bytes and free of control
  characters; a refused one is reported and the pane keeps its state.
- The picker lists the files under the working directory, filters them with
  the typed query, moves the selection with Up and Down or the pointer, opens
  the selection with Enter or a click and closes on Escape or a press outside
  it. A typed absolute path opens that file.

## Limits

| Limit | Value |
| --- | --- |
| File read into a pane | 8 MiB |
| Path or working directory | 16384 bytes |
| Editor panes per client | 64 |
| Picker rows shown | 500 |
| Picker scan | 50000 entries, depth 8, 5 s |
| Picker query | 64 KiB |
| Picker scan slice per frame | 4 ms |

Every refusal is a raised `Error` with the text the source shows, or an
`EditorError` on the model when the source shows it in the pane.

## `EditorPane`

```
EditorPane(pane: UInt64, path: Optional[String], cwd: String, th: PennaTheme) raises
descriptor(path: Optional[String], cwd: String) raises
command(kind: UInt8, text: String = String()) raises
key(event: KeyEvent) raises -> EditorKeyRoute
text(ui, input: TextInput, clip, key: UInt64, skip: Int = 0) raises
pointer(phase, button, pressed, x, y, mods, clicks = 1) raises -> Bool
wheel(x, y, dx, dy, mods) -> Bool
build(ui, th: PennaTheme, key: UInt64) raises
take_outbox() -> List[EditorOutbound]
set_focused(focused: Bool), title(), dirty(), picking()
```

- `command` kinds: `EDITOR_CMD_OPEN` opens the picker, `EDITOR_CMD_SAVE`
  saves, `EDITOR_CMD_SAVE_AS` saves to `text`, `EDITOR_CMD_RETRY` repeats the
  failed read or write.
- `key` answers `EDITOR_KEY_PASS` (not ours), `EDITOR_KEY_TEXT` (a text key:
  its `TextInput` events must reach `text`) or `EDITOR_KEY_TAKEN`. Ctrl+O and
  Ctrl+S (Cmd on macOS) are taken; with the picker open so are Escape, Enter
  and the two arrows.
- `text` runs once a frame before `build`. With the picker open the events go
  to its field; otherwise they go to the editor when the pane is focused.
  `skip` is the number of leading events another surface already consumed.
- `pointer` and `wheel` only claim the event: mjui's `Ui` already has it, and
  the editor scrolls and selects from there. `pointer` also closes the picker
  on a left press outside it.
- `build` opens one box with the id `key`. The pane uses these sub keys:
  `sub(key, 1)` retry button, `2` editor, `3` picker surface, `4` picker
  field, `5` picker list, `sub(sub(key, 6), index)` picker rows.
- The outbox holds `EDITOR_OUT_COMMAND` (a command name and its arguments,
  the target already written as `%<pane>`) and `EDITOR_OUT_SAVE_AS` (`name`
  is the folder, `arguments[0]` the suggested file name).

`EditorDeck` has the same calls with a pane id in front, `sync(snapshot, th)`
to follow the snapshot, `focus(pane)` and `owns(pane)`. `sync` raises a
refused descriptor once and keeps the other panes in step.

## Wiring into `WorkspaceView`

The editor front does not own `view.mojo`, `workspace_keys.mojo`,
`backend.mojo` or the window code. These are the steps for whoever does, each
next to the browser line it mirrors.

1. Field: `var editors: EditorDeck`, set to `EditorDeck()` in `__init__`,
   plus `var editor_visible: List[UInt64]` cleared where `browser_visible` is.
2. Snapshot: after `self.sync_browser(ui)` call

   ```
   try:
       self.editors.sync(self.model.snapshot, self.theme)
   except error:
       self.primary_notice(ui, String(error))
   self.editor_outbox(ui)
   ```

3. Outbox: `editor_outbox` drains `self.editors.take_outbox()`. For
   `EDITOR_OUT_COMMAND` call `self.picker_command(ui, item.name,
   item.arguments)`. For `EDITOR_OUT_SAVE_AS` open the host's save dialog
   (`host.save_file`) in folder `item.name` with the file name
   `item.arguments[0]`, and answer a chosen path with
   `self.editors.command(item.pane, EDITOR_CMD_SAVE_AS, path)`; a cancelled
   dialog needs no call. Drain it again after any `key`, `text`, `command` or
   `build` that ran this frame, or once at the end of `build`.
4. Focus: `editor_pane(ui)` is `browser_pane(ui)` with `self.editors.owns`,
   and `self.editors.focus(self.editor_pane(ui))` goes next to
   `self.browser.focus(...)`.
5. Paint: in the pane kind dispatch add `elif kinds.get(item.pane, UInt8(1))
   == 3: self.paint_editor(ui, item.pane)`, where

   ```
   var key = self.name_key(String(11, ":", pane))
   _ = ui.interact(key, focusable=True, focus_on_press=False)
   self.editors.text(ui, input, self.clipboard, pane, key, search_skip)
   self.editors.build(ui, self.theme, pane, key)
   self.editor_visible.append(pane)
   ```

   `input` and `search_skip` are the frame's `TextInput` and the count of
   events the palette, search, chooser and windows consumed, so the paint path
   needs them passed down. The clipboard is the view's `Clipboard` value.
6. Terminal text: the loop that calls `self.commit_text_event(input, event)`
   must skip when `self.editor_pane(ui)` is set, or typed text also goes to
   the daemon.
7. Keys: in `route_workspace_key`, right after the `view.browser_key` step,

   ```
   var route = view.editor_key(ui, key)
   if route.kind == EDITOR_KEY_TEXT:
       result.claimed = True
       result.after = ui.focus
       return result^
   if route.kind == EDITOR_KEY_TAKEN:
       keyboard.claim_text(key, Span(input.bytes))
       keyboard.claim_text_command(key)
       result.claimed = True
       result.after = ui.focus
       return result^
   ```

   `view.editor_key` returns `EDITOR_KEY_PASS` on the same early exits as
   `browser_key` (no editor pane has the keyboard, `self.modal_capture()`,
   `self.model.routing.claims_prefix_input(input)`), else
   `self.editors.key(pane, key)`. A text key is claimed without `claim_text`
   so its `TextInput` events still arrive.
8. Pointer: `editor_mouse` next to `browser_mouse` in `view.mouse`. Find the
   pane under the pointer with `ui.rect_of(self.name_key(String(11, ":",
   pane)))` over `editor_visible`. On a press over an editor pane that is not
   active: `self.blur_pointer()`, `self.model.select_pane(pane)`,
   `self.focus_epoch += 1`, `self.next_poll = 0`, then `ui.focus =
   TERMINAL_KEY` and `self.picker_command(ui, "select-pane", ["-t",
   "%<pane>"])`. Then return `self.editors.pointer(pane, phase, button,
   pressed, event.x, event.y, event.mods, event.clicks)`; the phase and button
   values are the browser's (`EDITOR_POINTER_DOWN` 0, `EDITOR_BUTTON_LEFT` 1).
9. Wheel: `editor_wheel` next to `browser_wheel` in `view.scroll`, returning
   `self.editors.wheel(pane, x, y, event.dx, event.dy, ui.mods)`.
10. Text input: pass `True` to `win.set_text_input` while
    `self.view.editor_pane(ui)` is set, as it is for a terminal pane.
11. Pane titles come back from the daemon through `select-pane -T`; nothing
    else reads `title()`.

## Differences from the source

- Reading and saving run inline on the UI thread, not on a background
  executor. The 8 MiB limit bounds the stall.
- The picker lists files with zz's own walker and ranks them with the path
  ranker; the source uses the desktop file index. The walker reads no ignore
  files: it skips names that start with a dot, symlinks, `node_modules`,
  `target`, `__pycache__`, `venv` and `go/pkg/mod`. The scan and the ranking
  run on the UI thread, the scan in 4 ms slices.
- The picker is drawn with the palette kit: one usage line carries the
  scanning note, the limit note and the query refusal, and rows have no mode
  icon. Its field takes no pointer caret placement and no composition.
- Line numbers, relative numbers and soft wrap are fixed by mjui's `Editor`
  (soft wrap on); the source reads them from the config. The font size is the
  pane's `font_size`, 13.
- No vim mode and no mode chip.
- Highlighting is lexical unless the app is built with `-D MJ_TS=1`.
- The save-as prompt needs the host's save dialog, which only SDL builds have.
- The hint, the error panel and the picker are sized from the pane's rectangle
  in the previous frame; the first frame of a pane asks for another.
- The error panel's icon and the picker footer's key icons are drawn only in a
  swash-raster build.
- The error title uses the theme's one code font. The source asks for the
  semibold weight, so the two differ where a semibold code font is installed;
  the scene check loads one weight on both sides.
