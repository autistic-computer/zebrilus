---
type: Reference
title: tmux compatibility gap report
description: "Live TODO and status report for tmux compatibility gaps, decisions, evidence, and acceptance gates."
resource: compat/tmux-gaps.json
tags: [tmux, compatibility, gaps, tracker]
timestamp: 2026-10-18T00:00:00-03:00
---

# tmux compatibility gaps

**2 open, 0 blocked, 46 accepted** across 48 groups and 365 classified items. 7 items remain open or blocked. [Closed history](#closed-history) covers 226 earlier groups.

Decisions (2 adopt, 38 native, 0 park, 8 never). Priorities (1 now, 0 next, 1 later, 46 none).

The [gap registry](../../compat/tmux-gaps.json) holds the decision, rationale and evidence for each ID. Look up any current or closed group from the repository root.

```sh
jq --arg id 'desktop.overlay-consumers' '(.gaps[], .closed[]) | select(.id == $id)' compat/tmux-gaps.json
```

## Open and blocked work

### `desktop.overlay-consumers`: Give every daemon overlay payload a desktop consumer

**open · adopt · now**. Owner `gui`, effort `medium`, impact daily, gui.

**Tracked items** `presentation:gui-menu-mouse-modality`

**Acceptance criteria**

- Every overlay payload the daemon can publish reaches a desktop consumer: the element draws on the window, it takes the keyboard, a key it answers reaches the daemon as its own InputMessage, and the retiring payload takes the element away. The kinds are EventPayload::CommandPrompt, ChooseTree, ChooseTreeUpdate, ChooseBuffer, ChooseBufferUpdate, DisplayPanes, Popup, Menu and Confirm.
- Adding a CoreEvent variant cannot compile until someone classifies it as an overlay or not, so a future overlay kind cannot reach the desktop without a consumer.
- Measured on pinned tmux d77c9dc6, menu.c menu_key_cb: a MENU_NOMOUSE menu, every display-menu without -M and without an invoking mouse event, for which menu_prepare also leaves MODE_MOUSE_ALL and MODE_MOUSE_BUTTON off, closes with nothing chosen for every report whose MOUSE_BUTTONS(m->b) is not MOUSE_BUTTON_1, and that arm runs before the box test so the pointer's position does not matter. Only a button-1 press is swallowed: tty-keys.c sets b = 3 for an SGR release and MOUSE_BUTTONS(3) is 3, so a button-1 release leaves the menu, which a live probe on the pin confirms (press keeps the menu with nothing chosen, release returns display-menu with nothing chosen). The desktop menu obeys that and still chooses the row under button 1 when the menu did take the mouse.
- menu_key_cb's whole mouse arm, for a menu that took the mouse: a press or a motion inside the box sets md->choice to m->y - (md->py + 1) and does nothing else; a release inside the box goes to chosen, which reads md->choice, so it chooses the row the highlight already sits on and not the row it landed on; a release outside the box closes a menu that is not stay-open; and a stay-open menu instead closes on any report that is neither MOUSE_RELEASE, MOUSE_WHEEL nor MOUSE_DRAG. The desktop answers each of those through zz_client::resolve_menu_mouse, with the pointer's pixels mapped onto the same cell box menu_frame draws.
- The desktop draws one menu row per cell of the box menu.c measured, because menu.c sizes a menu as count + 2 rows and the client is handed that height. A row that takes its text height instead overflows the box whenever the terminal cell is shorter than the row's line height, and FloatingSurface clips what overflows.
- OPEN: mouse modality. The pin hands menu_key_cb every mouse report while the menu owns the client's overlay. The desktop intercepts a press and a release anywhere, but a motion or a wheel that lands outside the menu's own surface still reaches whatever is under it, so scrolling a pane under an open menu still scrolls the pane.

### `options.byte-path-residue`: Keep non-UTF-8 bytes on the text paths fix.option-bytes left lossy

**open · adopt · later**. Owner `mux`, effort `hard`, impact scripts.

**Tracked items** `semantic:command-print-argument-bytes`, `semantic:control-environment-request-bytes`, `semantic:format-loop-option-bytes`, `semantic:format-template-argument-bytes`, `semantic:show-options-format-value-bytes`, `semantic:window-name-invalid-utf8`

**Acceptance criteria**

- With LC_ALL=C, TMUX unset and @x holding 61 fe 62, a -u command client's `display-message -p a<fe>b` and `show-options -g -F '#{option_value}' @x` print 61 fe 62 0a as tmux 3.8 does (zz prints 61 ef bf bd 62 0a; both sides print 61 5f 62 0a without -u).
- `list-keys` after `bind-key -T t x display-message a<fe>b`, and `show-hooks` for a hook whose command holds that byte, print the argument as `a\376b` as 3.8 does (zz prints `a_b`).
- A control client line `set-buffer -b b "$X"` with a global X of 61 fe 62 stores 61 fe 62 as 3.8 does (zz stores 61 ef bf bd 62 because the EnvironmentRequest answer is text).
- `rename-window a<fe>b` answers `invalid window name: a_b` and keeps the old name as 3.8 does (zz renames the window to 61 ef bf bd 62).
- Measured 2026-10-09 with @x 61 fe 62 set globally and @y 61 fe 62 set on window 0 of two: `#{O/g:#{?#{==:#{option_name},@x},#{option_value},}}` and `#{O/w:...}` for @y print 61 fe 62 0a to a -u client on 3.8 (format_loop_add_option adds options_to_string's bytes) and 61 ef bf bd 62 0a on zz, and `#{W:#{?#{==:#{window_index},1},#{prev_@y},}}` prints 61 fe 62 0a on 3.8 (format_add_window_neighbour copies options_to_string(o, NULL, 1)) and 61 ef bf bd 62 0a on zz; without -u both sides print 61 5f 62 0a. Both close when FormatOptionRow.value and the neighbour copies carry RawText through the loop expander.
- Each row is a hex row in compat/scenarios/fixtures/option-bytes.sh that reads clean on both binaries.

## Measured surface

The [oracle](../../compat/tmux-oracle.json) inventories tmux 3.8 at `7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53`. The registry classifies differences found by the compatibility gate.

| Surface | Pinned inventory | Registry counts |
| --- | --- | --- |
| Commands | 92 commands, 78 aliases | 90 commands catalogued, 47 zz-only commands |
| Flags | 593 shapes (332 valueless, 253 required-value, 8 optional-value) | 8 unsupported flags, 0 arity mismatches, 24 zz-only flags |
| Arguments | Positional bounds, 14 callback-bearing commands, 6 effective `args_parse` rules | 0 minimum and 0 maximum mismatches, 0 implemented commands missing callback verification |
| Options | 183 options | 16 absent from `BEHAVES`, 0 scope and 0 default mismatches |
| Formats | 214 global formats, 37 modifiers, 204 literal pairs across 37 producers, 5 derived families | 44 limited formats, 0 scoped gaps, 0 accepted native context names |
| Hooks | 89 hooks | 2 producer gaps, 0 zz-only hooks |
| Keys | 308 default bindings across 5 tables | 19 omitted keys, 37 divergent shared bindings, 93 zz-only keys |

**Items by registry prefix** `binding` 37, `command` 2, `extension-flag` 24, `flag` 8, `format` 44, `hook` 2, `key` 19, `native-command` 47, `native-key` 93, `option` 16, `presentation` 8, `protocol` 1, `semantic` 64

## Enforcement boundary

The gate checks catalog entries, resolver prefixes, format and hook inventories, default binding commands and repeat bits, and the expected difference counts below. Runtime parsing, format values, hook delivery and key behavior need differential scenarios, attached-client fixtures, unit tests or GUI checks. The [stored run](../../compat/results/summary.md) records the measured results.

## Accepted divergences

Each row records a decision to keep a difference from the pin.

| ID | Decision | Group | Items |
| --- | --- | --- | ---: |
| `aliases.remote-client-preflight` | native | Route a remote host statically to its ssh lifecycle | 1 |
| `capture.grid-storage-line-flags` | native | Print tmux's grid storage letters in capture-pane -F | 1 |
| `capture.line-times` | native | Keep capture-pane -I unimplemented: the engine keeps no per-line history time | 1 |
| `capture.rich-transports` | native | Add the raw grid and pending-input capture transports | 3 |
| `capture.terminal-engine-limits` | native | Tell OSC 8 links, OSC 133 prompt kinds and moved output lines apart the way tmux's grid does | 3 |
| `clients.interactive-refresh` | native | Complete interactive client commands | 5 |
| `clients.read-only-and-focus` | native | Retain native client focus semantics | 4 |
| `commands.cold-parse-diagnostics` | native | Report a cold CLI parse error as the parse error | 1 |
| `commands.json-output` | native | Keep --json on the list verbs a zz-only flag | 5 |
| `commands.native-client-tools` | native | Use native client tools | 2 |
| `commands.native-superset` | native | Keep the zz-native command namespace explicit | 66 |
| `config.background-if-shell-race` | native | Accept that zz picks one branch where the pin's own race picks either | 1 |
| `control-mode.read-only-commands` | native | Keep read-only control clients read-only | 1 |
| `display-popup.modal-pane` | native | Run display-popup as a modal floating pane, as tmux master does | 6 |
| `formats.expansion-budgets` | native | Keep format expansion deterministic | 1 |
| `formats.history-counters` | native | Answer history_added, history_collected and history_generation natively: the engine keeps no scroll counters | 3 |
| `formats.mouse-context` | native | Expose mouse event formats | 2 |
| `formats.native-modes` | native | Keep native mode row formats | 3 |
| `formats.native-typed-context-producers` | native | Keep typed native context producers explicit | 1 |
| `formats.pane-current-command-exec-path` | native | Name the foreground process by its exec path, not the kernel comm | 1 |
| `formats.pane-runtime` | native | Expose pane mode formats | 1 |
| `formats.session-activity-wake-lifecycle` | native | Keep native wake lifecycle outside session activity | 1 |
| `formats.terminal-cells` | native | Expose terminal cell formats | 5 |
| `formats.terminal-runtime` | native | Expose terminal runtime formats | 22 |
| `hooks.shutdown-window-unlinked-order` | never | Do not reproduce the forced-shutdown window-unlinked order | 1 |
| `keys.copy-mode-native-numeric-prefix` | native | Keep native copy-mode numeric prefix capture | 18 |
| `keys.default-prefix` | native | Keep the native default prefix table | 31 |
| `keys.native-defaults` | native | Keep zz-native default key tables explicit | 93 |
| `keys.root-native-mouse` | native | Keep native root mouse bindings | 8 |
| `layout.safety-invariants` | never | Keep bounded valid layouts | 6 |
| `layout.spread-mixed-upstream-bug` | never | Reject corrupt mixed-parent spread | 1 |
| `list-keys.deterministic-sort-ties` | never | Keep list-keys sorting total and deterministic | 1 |
| `menus.client-owned` | native | Keep menus as client overlays | 1 |
| `mouse.bound-context` | native | Carry bound mouse event context | 1 |
| `options.client-terminal-negotiation` | native | Keep extended-keys-format, assume-paste-time and xterm-keys native; capability-string overrides open | 4 |
| `options.lock-program` | native | Keep locking off the client terminal | 3 |
| `options.native-mode-styles` | native | Keep native mode styling | 1 |
| `options.native-overlay-styles` | native | Keep native overlay styling | 5 |
| `options.native-pane-scrollbars` | native | Keep pane scrollbars outside the cell grid | 4 |
| `options.terminal-engine-limits` | native | Keep the VT engine's own Unicode, clipboard, and collector limits | 4 |
| `presentation.native-status` | native | Keep native status and lifecycle presentation | 13 |
| `protocol.socket-acl` | never | Keep the daemon socket single-user | 1 |
| `protocol.socket-interop` | never | Do not speak tmux private protocol | 1 |
| `sessions.linked-groups` | never | Do not add linked session groups | 19 |
| `terminal.resize-pane-trim` | never | Keep the scrollback out of the active grid | 1 |
| `terminal.zoom-reflow` | native | Keep the VT engine's own reflow of a zoomed pane's history | 1 |

## Known differential scenarios

The registry gives expected counts for 10 scenarios.

| Scenario | Gap | TOPO | GEO | FMT | OUT | WARN |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| known/known-display-popup-drag.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 2 |
| known/known-display-popup-menu-policy.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 1 |
| known/known-display-popup-menu.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 1 |
| known/known-display-popup-resize-lifecycle.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 2 |
| known/known-display-popup-style-refresh.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 2 |
| known/known-display-popup-to-pane.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 1 |
| known/known-jobs-display-popup-environment.txt | display-popup.modal-pane | 0 | 0 | 0 | 1 | 2 |
| known/known-pane-scrollbar-columns.txt | options.native-pane-scrollbars | 0 | 2 | 0 | 1 | 0 |
| known/known-spread-mixed.txt | layout.spread-mixed-upstream-bug | 0 | 1 | 0 | 0 | 0 |
| known/known-terminal-runtime.txt | formats.terminal-runtime | 0 | 0 | 6 | 0 | 0 |

## Closed history

The [registry](../../compat/tmux-gaps.json) retains the resolution and evidence for all 226 closed groups. This command lists them by ID with the date and title.

```sh
jq -r '.closed[] | [.closed_on, .id, .title] | @tsv' compat/tmux-gaps.json
```

Regenerate this page with `python3 compat/tmux-tracker.py write-report`.
