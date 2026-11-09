# Windows chooser lifecycle

`WindowChooserController` stages Windows-picker state and typed native actions. `prepare(operation)` leaves published data unchanged. `commit(change)` checks both the retained controller identity and its generation, then publishes the prepared data once. A foreign controller, replayed change, exhausted generation or resource refusal cannot replace the current selection or pending activation. Projected row arrays use shared immutable ownership, so staging does not duplicate the full presentation.

The source oracle creates a real `CommandPaletteView` and `InputState` in an ala `TestAppContext`. It drives the actual constructor, synchronization, editor notifications, navigation, expansion, activation, close and return-to-default methods. Its backend records native requests; it does not implement the picker algorithm. The Mojo boundary receives the editor's resulting text and byte cursor, so this comparison tests the picker controller rather than substituting another editor or string splitter.

All 282 traces and 5,850 complete states match: 223,445 bytes of revision, local/window mode, finishing/navigation flags, selection, query/cursor, source-row indices, temporary expansion history, pending requests, saved selection, deferred activation, native frames, return-default events and exact deferred list-scroll requests. Each test step consumes the previous deferred scroll request as a rendered frame would; the reference reads the real handle's index, strategy, offset and strictness. The standalone staged-interface probe also checks preparation without publication, foreign-controller rejection, replay, oversized-query refusal and generation exhaustion.

Run `differential-window-chooser-lifecycle` and `test-window-chooser-controller`. The final fresh source build, comparison and scoped clippy check with warnings denied passed at 1,317 MiB under the shared 4 GiB guard.

The source has observable distinctions that the controller keeps:

- Synchronization preserves a selected target across row-index changes, falls back to the daemon's selected target, and ignores repeated revisions.
- Editing text or moving its byte cursor rebuilds the filtered rows. Clearing or entering a whitespace-only query picks the first row because the source runs hidden command completion before rebuilding the Windows rows. Constructor and synchronization selection retain their separate active-window and target rules.
- A raw nonempty query prevents left/right tree movement even when its trimmed value is empty and the presentation remains grouped.
- Search expands collapsed branches and later restores only its recorded temporary expansions. Manual expansion does not enter that restoration list.
- Activation waits for an outstanding expansion acknowledgement and then resolves its saved target against the latest daemon indices.
- Successful deferred activation returns before clearing the old pending tuple. Repeated activation is still callable while finishing; close and expansion advancement have their own finishing guards.
- Backspace on an empty field emits a return-default event. Handling that event closes the native chooser and hands input to the local palette; it is distinct from ordinary close.

`WindowChooserController.prepare` also takes a span of operations and applies them to one copy of the data, so a gesture such as Select plus Expand is one change. Queue admission, owner checks, the editor, IME composition and the list scroll are handled by `WindowChooserRuntime`; see [window-chooser.md](window-chooser.md). The default app keeps the native chooser capability off until the Buffer frontend is complete.
