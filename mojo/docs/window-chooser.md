# Windows chooser

The Windows chooser is a distinct command-palette mode. It uses `UnifiedPalette.rebuild_window_chooser`, rather than the generic Panes/Clients modal. The native capability remains disabled by default while the Buffer frontend is being completed. `WindowChooserRuntime` in `zzmojo/window_chooser_runtime.mojo` is the one entry point the workspace needs; it is not wired into `WorkspaceView` yet, and the wiring steps are at the end of this page.

## Rows and search

`WindowChooserBuilder` owns reusable source-exact palette matching tables. `build(items, query, grouped)` returns `WindowChooserRows`: projected `PaletteRow` records, stable fuzzy scores, the original active-window index and the source initial selection. Each row keeps its original daemon item index in `row.index`; its action is `WINDOW_CHOOSE_ACTION`. Navigation records use the existing source-derived palette parent/child and selection rules.

The projection retains sessions, windows and panes and skips Client targets. A window label loses its prefix only when the text before the first colon parses as a Rust `u32`; custom formatted text takes precedence. Active state follows the source hierarchy, including the last active window even when filtering hides that row. Grouped empty queries keep source tree order, indentation and expansion flags. Other queries add the source path prefix and use stable score order. Hidden metadata can match without adding visible highlight ranges. Custom text suppresses the displayed detail while retaining that detail for metadata matching.

`WindowChooserSearch` ports the GUI's actual expansion acknowledgement helper. The Rust GUI uses the extracted `zz_client::chooser::WindowChooserSearch` too. Searching expands the first collapsed branch; ending search collapses recorded branches in reverse order. A matching target that has not reached the requested expansion state keeps the request pending. Removed targets and branches that lose their children follow the source's existing completion rule. Selection is retained independently for the controller's later restoration step.

The application bounds the query and each candidate's projected row field at 64 KiB, rows and recorded expansion targets at 65,536, and aggregate projected text plus highlight storage at 16 MiB. Projection is staged in a returned value, so a refusal preserves the caller's previously published rows. A full expansion-history refusal preserves the pending request too. These are explicit refusals of oversized source-valid data, not source parser errors.

The staged lifecycle and native-action controller is described in [window-chooser-lifecycle.md](window-chooser-lifecycle.md).

## Admission

Every controller change reaches the daemon through `stage_window_chooser` and `commit_window_chooser`. Staging prepares the controller change and the complete native request batch against the current action queue without publishing either. Commit checks that the chooser owner (daemon source plus a scope that changes on every reopen) is still current, that the controller generation and queue generation have not moved, then publishes the batch and the change together. A refusal leaves the queue, the selection and the pending expansion unchanged.

A gesture made of several operations is admitted as one change. Clicking a row's disclosure is Select followed by Expand: `WindowChooserController.prepare` takes the operations as a span, applies them in order to one copy of the data, joins their native actions and keeps the last scroll request. If the queue cannot take the Select and Expand requests together, the row selection does not move either. Editor results (a navigation key plus the text change it caused) and pointer results go through the same path.

## Editor and composition

`WindowChooserComponent` owns the palette text field. Typed and pasted text goes through `window_chooser_text`, which drops the characters the native single-line input drops: every C0 and C1 control, so a line break or tab disappears instead of turning into a space. Text that would push the field past 64 KiB is refused and the field keeps its old text.

The native input splices an IME composition into its value, so the palette query changes while the user is still composing. The component keeps the composition out of the editable buffer but reports the same query: `state()` is the buffer with the composition over its selection, and the cursor is the composition start plus the IME's selection end. The rest follows the native field:

- Composing over a selection deletes that selection when the composition starts. Cancelling the composition (an empty preedit) leaves it deleted.
- The IME's selection offsets are UTF-16 positions in the text it sent. The native field applies them to the text after control characters are dropped; `window_chooser_marked_offset` does the same, including clamping past the end and stopping before a split surrogate pair.
- A preedit that would push the query past 64 KiB is ignored and the previous composition stays.
- Committing replaces the composition. A committed text that would exceed 64 KiB is refused and the composed text stays in the field, as it stays in the native value.
- Ending the composition without a commit (`unmark`, also used when the chooser loses input ownership) keeps the composed text and the IME's selection. The query does not change.

mjui reports an empty composition after any unmark, including focus changes. An empty preedit with no composition in progress is therefore a no-op here, while the native field would delete a selection in that case. On macOS mjui turns `unmarkText` into a committed insert, so the caret lands at the end of the composed text there instead of at the IME's selection.

## Checks

| Command | Covers |
|---|---|
| `differential-window-chooser` | 1,338 model cases and 134,253 full rows, plus 113 expansion traces and 1,428 complete states: 5,911,260 exact bytes against the real Rust palette rebuild and the GUI's search helper |
| `differential-window-chooser-lifecycle` | 282 traces and 5,850 states of the real `CommandPaletteView` (see the lifecycle page) |
| `mojo/scripts/window_chooser_composition_vectors.py` | 24 traces and 164 states of the real `CommandPaletteView` driving `InputState` through `replace_and_mark_text_in_range`, `unmark_text`, `replace_text_in_range` and `set_selected_range`: query, cursor, field value and caret, rows, selection, frames and scroll after every step |
| `mojo/scripts/window_chooser_text_vectors.py` | the insertion sanitizer against `InputState.replace`: 277 cases covering all 1,112,064 Unicode scalars |
| `mojo/scripts/window_chooser_scroll_vectors.py` | list scrolling against the real `UniformList` layout: 4,752 cases of Top and Nearest requests and wheel dispatch at scales 1, 1.25 and 2 |
| `window_chooser_admission.py`, `window_chooser_session.py`, `window_chooser_controller.py` | queue capacity, stale queue and retired source refusals, atomic Select plus Expand, reopen ownership and the local palette fence |
| `window_chooser_component.py`, `window_chooser_runtime.py` | retained focus, disclosure hit targets, bounded paste, wheel, editor rollback on refusal, composition through the runtime, an ordered committed insert and one PRIMARY paste per middle click |

Large-row navigation sampling keeps the reference harness bounded; the independent palette-navigation differential covers the full navigation policy. Scoped clippy of the `zz-ui` fixture and the three reference examples passes with warnings denied at 1,267 MiB under the shared 4 GiB guard.

## Wiring WindowChooserRuntime into the workspace

The integrator owns `view.mojo`, `backend.mojo`, `workspace_keys.mojo`, `input_timeline.mojo` and `stream_transport.mojo`. Steps:

1. State. Keep one `WindowChooserRuntime` in `WorkspaceView` next to `chooser`. It runs only while `backend.job[].native_chooser` is on, the backend is online and `model.dialogs.chooser` has kind 0; the Panes/Clients chooser keeps the other kinds.
2. Frame start. Where `prepare_chooser_input` runs, call `observe(ui, backend, model.dialogs, chooser_source(), palette.opened(), focus_epoch, input)`, then `prepare(ui, input)`. When the result has `replaced_local`, close the local palette. When it has `closed` and the local palette is not open, give focus back to the terminal as the Panes chooser does.
3. Ordered input. In `InputTimeline.process`, offer every event to `ordered(ui, backend, input, event, host, focus_epoch)` before `view.chooser_event`, `view.palette_event` and `view.ordered_pointer`. True means the chooser consumed it. It handles typed and committed text, editing commands, PRIMARY paste on the field and pointer events on the field and rows, in their timeline order. Only the middle press pastes PRIMARY; the release and drag markers of the same click return false and go on to `view.ordered_pointer`. Routed key markers stay with the timeline.
4. Frame end. After the event loop call `compose(ui, backend, input, focus_epoch)` so the composition that is current at the end of the frame lands after the frame's commits. False means the composition was over 64 KiB; show a notice. When `unmark` is set the chooser opened, closed or handed input away this frame: drop the platform composition the way `chooser_unmark` does.
5. Wheel. In `view.scroll`, offer wheel events to `wheel(ui, x, y, dx, dy, precise)` first while the chooser is open.
6. Drawing. Call `paint(ui, theme)` once per frame after the workspace is built. It opens the modal layer, draws the field, rows, disclosures and hints, applies the pending list scroll and records whether the chooser owns input for the next frame's composition.
7. Local palette. When `returned` is set (Backspace on an empty query), open the local palette and call `local_opened(model.dialogs.chooser_revision)`. Call `local_opened` whenever the user opens the local palette too, so a chooser revision that is already behind it does not reopen over it, while a newer revision replaces it.
8. Errors. Every raising method leaves the field, the controller and the queue as they were. Show the error as a notice, as the workspace does for the Panes chooser.

Turn `native_chooser` on by default only after the Buffer frontend is complete.
